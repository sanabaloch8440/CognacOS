#![no_std]
#![no_main]

use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};
use cognacos_acpi as acpi;
use cognacos_nt_abi::{bus::{ACPI_FIRMWARE_INTERFACE, INTERFACE_HEADER, ACPI_FIRMWARE_INTERFACE_VERSION}, io::DRIVER_OBJECT, ntstatus::*, types::PVOID};


struct AcpiState {
    ready: AtomicBool,
    madt: UnsafeCell<MaybeUninit<acpi::MadtInfo>>,
    mcfg: UnsafeCell<MaybeUninit<acpi::McfgInfo>>,
    mcfg_count: core::sync::atomic::AtomicUsize,
    dmar_present: AtomicBool,
}
unsafe impl Sync for AcpiState {}
static STATE: AcpiState = AcpiState {
    ready: AtomicBool::new(false),
    madt: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg_count: core::sync::atomic::AtomicUsize::new(0),
    dmar_present: AtomicBool::new(false),
};


// ACPI.sys obtains firmware tables through the HAL's Windows-compatible
// HalAcpiGetTableEx export. It never dereferences a bootloader-supplied physical
// address or calls a Rust-private kernel/HAL function.
unsafe extern "win64" {
    fn HalAcpiGetTableEx(signature: u32, instance: u32, table: *mut *mut core::ffi::c_void, table_size: *mut u32) -> i32;
}
core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.global HalAcpiGetTableEx
.type HalAcpiGetTableEx,@function
HalAcpiGetTableEx:
    jmp QWORD PTR [rip + __imp_hal_HalAcpiGetTableEx]
.size HalAcpiGetTableEx, .-HalAcpiGetTableEx

.section .data.imports.hal,"aw",@progbits
.p2align 3
.global __imp_hal_HalAcpiGetTableEx
.type __imp_hal_HalAcpiGetTableEx,@object
__imp_hal_HalAcpiGetTableEx: .quad 0
.size __imp_hal_HalAcpiGetTableEx,8
"#);

const MAX_ACPI_TABLE_LENGTH: u32 = 1024 * 1024;
unsafe fn query_hal_table(signature: [u8; 4], instance: u32) -> Result<&'static [u8], i32> {
    let mut table = core::ptr::null_mut();
    let mut length = 0u32;
    let status = unsafe { HalAcpiGetTableEx(u32::from_le_bytes(signature), instance, &mut table, &mut length) };
    if status != STATUS_SUCCESS { return Err(status); }
    if table.is_null() || length < 36 || length > MAX_ACPI_TABLE_LENGTH {
        return Err(STATUS_INVALID_PARAMETER);
    }
    Ok(unsafe { core::slice::from_raw_parts(table.cast::<u8>(), length as usize) })
}

const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_QUERY_DEVICE_RELATIONS: u8 = 0x07;
const IRP_MN_QUERY_INTERFACE: u8 = 0x08;

unsafe extern "win64" fn pnp_dispatch(_device: *mut cognacos_nt_abi::io::DEVICE_OBJECT, irp: *mut cognacos_nt_abi::io::IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut cognacos_nt_abi::io::IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let status = match unsafe { (*stack).minor_function } {
        IRP_MN_START_DEVICE if STATE.ready.load(Ordering::Acquire) => STATUS_SUCCESS,
        IRP_MN_STOP_DEVICE | IRP_MN_REMOVE_DEVICE => STATUS_SUCCESS,
        // A full ACPI namespace/AML interpreter and PDO publication path is
        // required before this bus driver can truthfully return BusRelations.
        IRP_MN_QUERY_DEVICE_RELATIONS => STATUS_NOT_SUPPORTED,
        IRP_MN_QUERY_INTERFACE => {
            let parameters = unsafe { &(*stack).parameters };
            let guid = unsafe { core::ptr::read_unaligned(parameters.as_ptr().cast::<*const cognacos_nt_abi::bus::GUID>()) };
            let size = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(8).cast::<u16>()) };
            let version = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(10).cast::<u16>()) };
            let output = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(16).cast::<*mut ACPI_FIRMWARE_INTERFACE>()) };
            if guid.is_null() || output.is_null() {
                STATUS_INVALID_PARAMETER
            } else if unsafe { *guid } != cognacos_nt_abi::bus::GUID_ACPI_FIRMWARE_INTERFACE
                || version as u32 != ACPI_FIRMWARE_INTERFACE_VERSION
                || (size as usize) < core::mem::size_of::<ACPI_FIRMWARE_INTERFACE>() {
                STATUS_NOT_SUPPORTED
            } else {
                unsafe { cognac_acpi_get_firmware_interface(output) }
            }
        },
        IRP_MN_START_DEVICE => STATUS_DEVICE_NOT_READY,
        _ => STATUS_INVALID_DEVICE_REQUEST,
    };
    unsafe { (*irp).io_status.status.status = status; (*irp).io_status.information = 0; }
    status
}
unsafe extern "win64" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    STATE.ready.store(false, Ordering::Release);
}

/// DriverEntry performs driver-wide firmware validation only. Device objects,
/// ACPI namespace PDOs, AML evaluation, and per-device resources belong to the
/// later PnP/ACPI namespace integration path, not to DriverEntry.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    let object = unsafe { &mut *(driver as *mut DRIVER_OBJECT) };
    let madt_bytes = match unsafe { query_hal_table(*b"APIC", 0) } {
        Ok(bytes) => bytes,
        Err(status) => return status,
    };
    let madt = match acpi::parse_madt(madt_bytes) {
        Ok(info) => info,
        Err(error) => return acpi::error_to_ntstatus(error),
    };
    unsafe { (*STATE.madt.get()).write(madt); }

    // MCFG and DMAR are optional. HAL reports a missing table distinctly from
    // malformed firmware data; malformed tables fail driver initialization.
    match unsafe { query_hal_table(*b"MCFG", 0) } {
        Ok(bytes) => match acpi::parse_mcfg(bytes) {
            Ok(info) => {
                STATE.mcfg_count.store(info.count, Ordering::Release);
                unsafe { (*STATE.mcfg.get()).write(info); }
            }
            Err(error) => return acpi::error_to_ntstatus(error),
        },
        Err(STATUS_NOT_FOUND) => {}
        Err(status) => return status,
    }
    match unsafe { query_hal_table(*b"DMAR", 0) } {
        Ok(bytes) => match acpi::parse_dmar(bytes) {
            Ok(_) => STATE.dmar_present.store(true, Ordering::Release),
            Err(error) => return acpi::error_to_ntstatus(error),
        },
        Err(STATUS_NOT_FOUND) => {}
        Err(status) => return status,
    }
    STATE.ready.store(true, Ordering::Release);
    object.major_function[IRP_MJ_PNP] = Some(pnp_dispatch);
    object.driver_unload = Some(driver_unload);
    STATUS_SUCCESS
}

/// Copy a validated firmware table through the versioned ACPI bus interface.
/// The caller owns the destination buffer; no firmware physical pointer escapes.
unsafe extern "win64" fn query_firmware_table(
    context: PVOID,
    signature: *const u8,
    instance: u32,
    buffer: PVOID,
    buffer_length: u32,
    required_length: *mut u32,
) -> i32 {
    if context.is_null() || signature.is_null() || required_length.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if !STATE.ready.load(Ordering::Acquire) {
        return STATUS_DEVICE_NOT_READY;
    }
    if context != core::ptr::addr_of!(STATE).cast_mut().cast() {
        return STATUS_INVALID_PARAMETER;
    }
    let signature = unsafe { core::slice::from_raw_parts(signature, 4) };
    let signature = [signature[0], signature[1], signature[2], signature[3]];
    let table = match unsafe { query_hal_table(signature, instance) } {
        Ok(table) => table,
        Err(status) => return status,
    };
    let Ok(length) = u32::try_from(table.len()) else {
        return STATUS_INSUFFICIENT_RESOURCES;
    };
    unsafe { required_length.write(length); }
    if buffer_length < length {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { core::ptr::copy_nonoverlapping(table.as_ptr(), buffer.cast::<u8>(), table.len()); }
    STATUS_SUCCESS
}

/// Return ACPI.sys' stable, versioned firmware-table interface to a consumer.
unsafe extern "win64" fn cognac_acpi_get_firmware_interface(
    output: *mut ACPI_FIRMWARE_INTERFACE,
) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    unsafe {
        output.write(ACPI_FIRMWARE_INTERFACE {
            header: INTERFACE_HEADER {
                size: core::mem::size_of::<ACPI_FIRMWARE_INTERFACE>() as u16,
                version: ACPI_FIRMWARE_INTERFACE_VERSION as u16,
                context: core::ptr::addr_of!(STATE).cast_mut().cast::<core::ffi::c_void>(),
                interface_reference: None,
                interface_dereference: None,
            },
            query_table: Some(query_firmware_table),
        });
    }
    STATUS_SUCCESS
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"ACPI____";
#[used]
#[unsafe(link_section = ".data")]
static DRIVER_COOKIE: u64 = 0x5244_4F54_4F53_0001;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

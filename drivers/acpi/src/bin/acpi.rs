#![no_std]
#![no_main]

use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};
use cognacos_acpi as acpi;
use cognacos_nt_abi::{bus::{ACPI_FIRMWARE_INTERFACE, INTERFACE_HEADER, ACPI_FIRMWARE_INTERFACE_VERSION}, io::DRIVER_OBJECT, ntstatus::*, types::PVOID};

const HANDOFF_MAGIC: u64 = 0x434f_474e_4143_4143; // COGNACAC
const HANDOFF_VERSION: u32 = 1;
const MAX_HANDOFF_SIZE: usize = core::mem::size_of::<AcpiDriverHandoff>();

#[repr(C)]
pub struct AcpiDriverHandoff {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    /// Physical RSDP address from the UEFI boot handoff. Firmware tables must
    /// be identity-mapped and remain resident for the duration of discovery.
    pub rsdp_address: u64,
}

struct AcpiState {
    ready: AtomicBool,
    madt: UnsafeCell<MaybeUninit<acpi::MadtInfo>>,
    mcfg: UnsafeCell<MaybeUninit<acpi::McfgInfo>>,
    mcfg_count: core::sync::atomic::AtomicUsize,
    dmar_present: AtomicBool,
    rsdp_address: core::sync::atomic::AtomicU64,
}
unsafe impl Sync for AcpiState {}
static STATE: AcpiState = AcpiState {
    ready: AtomicBool::new(false),
    madt: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg_count: core::sync::atomic::AtomicUsize::new(0),
    dmar_present: AtomicBool::new(false),
    rsdp_address: core::sync::atomic::AtomicU64::new(0),
};

const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_QUERY_DEVICE_RELATIONS: u8 = 0x07;

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
    if object.driver_extension.is_null() { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(object.driver_extension as *const AcpiDriverHandoff) };
    if handoff.magic != HANDOFF_MAGIC || handoff.version != HANDOFF_VERSION
        || (handoff.size as usize) < MAX_HANDOFF_SIZE || handoff.rsdp_address == 0 {
        return STATUS_INVALID_PARAMETER;
    }

    let madt = match unsafe { acpi::discover_madt(handoff.rsdp_address) } {
        Ok(info) => info,
        Err(error) => return acpi::error_to_ntstatus(error),
    };
    unsafe { (*STATE.madt.get()).write(madt); }

    // MCFG and DMAR are optional ACPI tables. If present, they must validate;
    // malformed firmware data is not silently ignored.
    match unsafe { acpi::find_table(handoff.rsdp_address, *b"MCFG", 0) } {
        Ok(bytes) => match acpi::parse_mcfg(bytes) {
            Ok(info) => {
                STATE.mcfg_count.store(info.count, Ordering::Release);
                unsafe { (*STATE.mcfg.get()).write(info); }
            }
            Err(error) => return acpi::error_to_ntstatus(error),
        },
        Err(acpi::AcpiError::MissingRoot) | Err(acpi::AcpiError::BadSignature) => {}
        Err(acpi::AcpiError::Truncated) | Err(acpi::AcpiError::BadLength)
        | Err(acpi::AcpiError::BadChecksum) | Err(acpi::AcpiError::MalformedEntry)
        | Err(acpi::AcpiError::TooManyEntries) | Err(acpi::AcpiError::AddressOutOfRange)
        | Err(acpi::AcpiError::MissingMadt) => return STATUS_INVALID_PARAMETER,
    }
    match unsafe { acpi::find_table(handoff.rsdp_address, *b"DMAR", 0) } {
        Ok(bytes) => match acpi::parse_dmar(bytes) {
            Ok(_) => STATE.dmar_present.store(true, Ordering::Release),
            Err(error) => return acpi::error_to_ntstatus(error),
        },
        Err(acpi::AcpiError::BadSignature) | Err(acpi::AcpiError::MissingRoot) => {}
        Err(_) => return STATUS_INVALID_PARAMETER,
    }
    STATE.rsdp_address.store(handoff.rsdp_address, Ordering::Release);
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
    let rsdp = context as u64;
    if rsdp == 0 || rsdp != STATE.rsdp_address.load(Ordering::Acquire) {
        return STATUS_INVALID_PARAMETER;
    }
    let signature = unsafe { core::slice::from_raw_parts(signature, 4) };
    let signature = [signature[0], signature[1], signature[2], signature[3]];
    let table = match unsafe { acpi::find_table(rsdp, signature, instance as usize) } {
        Ok(table) => table,
        Err(error) => return acpi::error_to_ntstatus(error),
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
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacAcpiGetFirmwareInterface(
    output: *mut ACPI_FIRMWARE_INTERFACE,
) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    unsafe {
        output.write(ACPI_FIRMWARE_INTERFACE {
            header: INTERFACE_HEADER {
                size: core::mem::size_of::<ACPI_FIRMWARE_INTERFACE>() as u16,
                version: ACPI_FIRMWARE_INTERFACE_VERSION as u16,
                context: STATE.rsdp_address.load(Ordering::Acquire) as PVOID,
                interface_reference: None,
                interface_dereference: None,
            },
            query_table: Some(query_firmware_table),
        });
    }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacAcpiQueryMadt(output: *mut acpi::MadtInfo) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    unsafe { output.write((*STATE.madt.get()).assume_init_read()); }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacAcpiQueryMcfg(output: *mut acpi::McfgInfo) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if STATE.mcfg_count.load(Ordering::Acquire) == 0 { return STATUS_NOT_FOUND; }
    unsafe { output.write((*STATE.mcfg.get()).assume_init_read()); }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub extern "win64" fn CognacAcpiDmarPresent() -> u32 {
    STATE.dmar_present.load(Ordering::Acquire) as u32
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

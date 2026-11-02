#![no_std]
#![no_main]
//! PCI IDE hardware-dependent function driver (pciide.sys model).
//!
//! Binds above pciidex.sys, consumes the PnP-assigned raw/translated resource
//! lists on START_DEVICE, and commits controller state only after the lower
//! PCI stack completes successfully. Hardware register programming is kept
//! fail-closed until a HAL-backed port/MMIO mapping provider is connected.

use core::ffi::c_void;
use cognacos_nt_abi::{
    bus::{GUID_PCI_DEVICE_INTERFACE, PCI_DEVICE_INTERFACE, PCI_DEVICE_INFORMATION},
    io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STACK_LOCATION},
    ntstatus::*,
    types::PVOID,
};
use cognacos_storage::pciidex::{
    parse_translated_resources, AssignedResource, ControllerState, IdeResourceSet,
    PciIdeController, PciIdeIdentity, ResourceKind,
};

const IRP_MJ_MAXIMUM_FUNCTION: usize = 27;
const IRP_MJ_PNP: u8 = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_SURPRISE_REMOVAL: u8 = 0x17;
const FILE_DEVICE_CONTROLLER: u32 = 4;
const FDO_EXTENSION_MAGIC: u64 = 0x434f_474e_5043_4944;

#[repr(C)]
struct PciIdeFdoExtension {
    magic: u64,
    lower_device: *mut DEVICE_OBJECT,
    physical_device: *mut DEVICE_OBJECT,
    controller: PciIdeController,
    previous_state: ControllerState,
}

unsafe extern "win64" {
    fn IoCreateDevice(driver: *mut DRIVER_OBJECT, extension_size: u32,
        device_name: *mut cognacos_nt_abi::object::UNICODE_STRING,
        device_type: u32, characteristics: u32, exclusive: u8,
        output: *mut *mut DEVICE_OBJECT) -> i32;
    fn IoDeleteDevice(device: *mut DEVICE_OBJECT);
    fn IoAttachDeviceToDeviceStack(source: *mut DEVICE_OBJECT, target: *mut DEVICE_OBJECT) -> *mut DEVICE_OBJECT;
    fn IoDetachDevice(target: *mut DEVICE_OBJECT);
    fn IoCallDriver(device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32;
    fn IoSkipCurrentIrpStackLocation(irp: *mut IRP);
    fn IoCopyCurrentIrpStackLocationToNext(irp: *mut IRP);
    fn IoSetCompletionRoutine(irp: *mut IRP,
        routine: Option<unsafe extern "win64" fn(*mut DEVICE_OBJECT, *mut IRP, PVOID) -> i32>,
        context: PVOID, success: u8, error: u8, cancel: u8);
    fn IoPnPQueryBusInterface(device: *mut DEVICE_OBJECT, guid: *const cognacos_nt_abi::bus::GUID,
        size: u16, version: u16, output: PVOID) -> i32;
    fn IoPnPRegisterDeviceDriver(driver: *mut c_void,
        add_device: Option<unsafe extern "win64" fn(*mut c_void, *mut DEVICE_OBJECT) -> i32>) -> i32;
}

core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.macro IMPORT name
.global \name
.type \name,@function
\name:
    jmp QWORD PTR [rip + __imp_ntoskrnl_\name]
.size \name, .-\name
.endm
IMPORT IoCreateDevice
IMPORT IoDeleteDevice
IMPORT IoAttachDeviceToDeviceStack
IMPORT IoDetachDevice
IMPORT IoCallDriver
IMPORT IoSkipCurrentIrpStackLocation
IMPORT IoCopyCurrentIrpStackLocationToNext
IMPORT IoSetCompletionRoutine
IMPORT IoPnPQueryBusInterface
IMPORT IoPnPRegisterDeviceDriver

.section .data.imports.ntoskrnl,"aw",@progbits
.p2align 3
.macro IAT name
.global __imp_ntoskrnl_\name
.type __imp_ntoskrnl_\name,@object
__imp_ntoskrnl_\name: .quad 0
.size __imp_ntoskrnl_\name,8
.endm
IAT IoCreateDevice
IAT IoDeleteDevice
IAT IoAttachDeviceToDeviceStack
IAT IoDetachDevice
IAT IoCallDriver
IAT IoSkipCurrentIrpStackLocation
IAT IoCopyCurrentIrpStackLocationToNext
IAT IoSetCompletionRoutine
IAT IoPnPQueryBusInterface
IAT IoPnPRegisterDeviceDriver
"#);

unsafe fn resource_list_bytes(list: *const u8) -> Option<&'static [u8]> {
    if list.is_null() { return Some(&[]); }
    let full_count = unsafe { core::ptr::read_unaligned(list.cast::<u32>()) } as usize;
    if full_count > 16 { return None; }
    let mut cursor = 4usize;
    for _ in 0..full_count {
        let partial_count = unsafe { core::ptr::read_unaligned(list.add(cursor + 12).cast::<u32>()) } as usize;
        if partial_count > 64 { return None; }
        cursor = cursor.checked_add(16)?.checked_add(partial_count.checked_mul(16)?)?;
        if cursor > 4 + 16 * (16 + 64 * 16) { return None; }
    }
    Some(unsafe { core::slice::from_raw_parts(list, cursor) })
}

unsafe fn read_start_resources(stack: *mut IO_STACK_LOCATION) -> Result<IdeResourceSet, i32> {
    if stack.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let raw = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(4).cast::<*const u8>()) };
    let translated = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(12).cast::<*const u8>()) };
    if raw.is_null() != translated.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let raw_bytes = unsafe { resource_list_bytes(raw) }.ok_or(STATUS_INVALID_PARAMETER)?;
    let translated_bytes = unsafe { resource_list_bytes(translated) }.ok_or(STATUS_INVALID_PARAMETER)?;
    let raw_set = parse_translated_resources(raw_bytes)?;
    let translated_set = parse_translated_resources(translated_bytes)?;
    if raw_set.count != translated_set.count { return Err(STATUS_INVALID_PARAMETER); }
    for index in 0..raw_set.count {
        let raw_resource = raw_set.get(index).ok_or(STATUS_INVALID_PARAMETER)?;
        let translated_resource = translated_set.get(index).ok_or(STATUS_INVALID_PARAMETER)?;
        if raw_resource.kind != translated_resource.kind { return Err(STATUS_INVALID_PARAMETER); }
    }
    Ok(translated_set)
}

unsafe extern "win64" fn pnp_complete(
    _lower_device: *mut DEVICE_OBJECT, irp: *mut IRP, context: PVOID,
) -> i32 {
    let fdo = context.cast::<DEVICE_OBJECT>();
    if fdo.is_null() || irp.is_null() { return STATUS_SUCCESS; }
    let extension = unsafe { (*fdo).device_extension.cast::<PciIdeFdoExtension>() };
    if extension.is_null() || unsafe { (*extension).magic != FDO_EXTENSION_MAGIC } {
        return STATUS_SUCCESS;
    }
    let status = unsafe { (*irp).io_status.status.status };
    let stack = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
    let minor = if stack.is_null() { 0xff } else { unsafe { (*stack).minor_function } };
    match minor {
        IRP_MN_START_DEVICE => {
            if status == STATUS_SUCCESS {
                let _ = unsafe { (*extension).controller.start_succeeded() };
            } else {
                unsafe { (*extension).controller.start_failed((*extension).previous_state); }
            }
        }
        IRP_MN_STOP_DEVICE | IRP_MN_SURPRISE_REMOVAL => {
            if status == STATUS_SUCCESS && unsafe { (*extension).controller.state } == ControllerState::Started {
                let _ = unsafe { (*extension).controller.stop_succeeded() };
            }
        }
        IRP_MN_REMOVE_DEVICE => {
            let lower = unsafe { (*extension).lower_device };
            unsafe { (*extension).magic = 0; }
            if !lower.is_null() { unsafe { IoDetachDevice(lower); } }
            unsafe { IoDeleteDevice(fdo); }
        }
        _ => {}
    }
    STATUS_SUCCESS
}

unsafe extern "win64" fn dispatch(device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if device.is_null() || irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let extension = unsafe { (*device).device_extension.cast::<PciIdeFdoExtension>() };
    if extension.is_null() || unsafe { (*extension).magic != FDO_EXTENSION_MAGIC } {
        return STATUS_DELETE_PENDING;
    }
    let lower = unsafe { (*extension).lower_device };
    if lower.is_null() { return STATUS_DEVICE_NOT_READY; }
    let stack = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }

    if unsafe { (*stack).major_function } == IRP_MJ_PNP {
        match unsafe { (*stack).minor_function } {
            IRP_MN_START_DEVICE => {
                let resources = match unsafe { read_start_resources(stack) } {
                    Ok(value) => value,
                    Err(status) => return status,
                };
                let previous = unsafe { (*extension).controller.state };
                if let Err(status) = unsafe { (*extension).controller.assign_resources(resources) } {
                    return status;
                }
                unsafe { (*extension).previous_state = previous; }
            }
            IRP_MN_STOP_DEVICE | IRP_MN_REMOVE_DEVICE | IRP_MN_SURPRISE_REMOVAL => {}
            _ => {}
        }
        unsafe {
            IoCopyCurrentIrpStackLocationToNext(irp);
            IoSetCompletionRoutine(irp, Some(pnp_complete), device.cast(), 1, 1, 1);
            return IoCallDriver(lower, irp);
        }
    }
    unsafe {
        IoSkipCurrentIrpStackLocation(irp);
        IoCallDriver(lower, irp)
    }
}

unsafe extern "win64" fn add_device(driver: *mut c_void, pdo: *mut DEVICE_OBJECT) -> i32 {
    if driver.is_null() || pdo.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut interface = core::mem::MaybeUninit::<PCI_DEVICE_INTERFACE>::zeroed();
    let status = unsafe {
        IoPnPQueryBusInterface(pdo, &GUID_PCI_DEVICE_INTERFACE,
            core::mem::size_of::<PCI_DEVICE_INTERFACE>() as u16,
            cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION as u16,
            interface.as_mut_ptr().cast())
    };
    if status != STATUS_SUCCESS { return STATUS_NOT_SUPPORTED; }
    let interface = unsafe { interface.assume_init() };
    if interface.header.size as usize < core::mem::size_of::<PCI_DEVICE_INTERFACE>()
        || interface.header.version as u32 != cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION {
        return 0xC000_0059u32 as i32;
    }
    let Some(query) = interface.query_information else { return STATUS_NOT_SUPPORTED; };
    let mut information = PCI_DEVICE_INFORMATION::default();
    let status = unsafe { query(interface.header.context, &mut information) };
    if status != STATUS_SUCCESS { return status; }
    if information.class_code != 0x01 || information.subclass != 0x01 {
        return STATUS_NOT_SUPPORTED;
    }
    let identity = PciIdeIdentity {
        vendor_id: information.vendor_id, device_id: information.device_id,
        base_class: information.class_code, sub_class: information.subclass,
        programming_interface: information.programming_interface,
        revision_id: information.revision_id,
    };
    let mut fdo = core::ptr::null_mut();
    let status = unsafe {
        IoCreateDevice(driver.cast(), core::mem::size_of::<PciIdeFdoExtension>() as u32,
            core::ptr::null_mut(), FILE_DEVICE_CONTROLLER, 0, 0, &mut fdo)
    };
    if status != STATUS_SUCCESS { return status; }
    if fdo.is_null() { return STATUS_INSUFFICIENT_RESOURCES; }
    let lower = unsafe { IoAttachDeviceToDeviceStack(fdo, pdo) };
    if lower.is_null() {
        unsafe { IoDeleteDevice(fdo); }
        return 0xC000_000Eu32 as i32;
    }
    let extension = unsafe { (*fdo).device_extension.cast::<PciIdeFdoExtension>() };
    let controller = PciIdeController::new(identity);
    unsafe {
        extension.write(PciIdeFdoExtension {
            magic: FDO_EXTENSION_MAGIC, lower_device: lower,
            physical_device: pdo, controller,
            previous_state: ControllerState::Created,
        });
        (*fdo).flags = ((*fdo).flags & !0x80) | ((*lower).flags & (0x02 | 0x04));
    }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: *mut DRIVER_OBJECT, _context: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe {
        for index in 0..=IRP_MJ_MAXIMUM_FUNCTION {
            (*driver).major_function[index] = Some(dispatch);
        }
        (*driver).driver_unload = None;
    }
    unsafe { IoPnPRegisterDeviceDriver(driver.cast(), Some(add_device)) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

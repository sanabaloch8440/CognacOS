#![no_std]
#![no_main]
//! PCI IDE controller framework FDO (pciidex.sys model).
//!
//! This driver binds only to PCI IDE-class PDOs. It publishes an FDO and
//! forwards the complete WDM request stream to the PCI bus PDO. Hardware
//! programming remains in pciide.sys; this framework owns stack attachment
//! and PnP lifetime. This is a project-owned WDM contract, not Microsoft's
//! private pciidex binary ABI.

use core::ffi::c_void;
use cognacos_nt_abi::{
    bus::{GUID_PCI_DEVICE_INTERFACE, PCI_DEVICE_INTERFACE, PCI_DEVICE_INFORMATION},
    io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP},
    ntstatus::*,
    types::PVOID,
};

const IRP_MJ_MAXIMUM_FUNCTION: usize = 27;
const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const FILE_DEVICE_CONTROLLER: u32 = 0x00000004;
const PCI_PDO_CLASS: u8 = 0x01;
const PCI_PDO_SUBCLASS_IDE: u8 = 0x01;
const FDO_EXTENSION_MAGIC: u64 = 0x434f_474e_5049_4445;

#[repr(C)]
struct PciIdexFdoExtension {
    magic: u64,
    lower_device: *mut DEVICE_OBJECT,
    physical_device: *mut DEVICE_OBJECT,
    identity: PCI_DEVICE_INFORMATION,
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
IAT IoSetCompletionRoutine
IAT IoPnPQueryBusInterface
IAT IoPnPRegisterDeviceDriver
"#);

unsafe extern "win64" fn remove_complete(
    _device: *mut DEVICE_OBJECT, _irp: *mut IRP, context: PVOID,
) -> i32 {
    let fdo = context.cast::<DEVICE_OBJECT>();
    if fdo.is_null() { return STATUS_SUCCESS; }
    let extension = unsafe { (*fdo).device_extension.cast::<PciIdexFdoExtension>() };
    if extension.is_null() || unsafe { (*extension).magic != FDO_EXTENSION_MAGIC } {
        return STATUS_SUCCESS;
    }
    let lower = unsafe { (*extension).lower_device };
    unsafe { (*extension).magic = 0; }
    if !lower.is_null() { unsafe { IoDetachDevice(lower); } }
    unsafe { IoDeleteDevice(fdo); }
    STATUS_SUCCESS
}

unsafe extern "win64" fn dispatch(
    device: *mut DEVICE_OBJECT, irp: *mut IRP,
) -> i32 {
    if device.is_null() || irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let extension = unsafe { (*device).device_extension.cast::<PciIdexFdoExtension>() };
    if extension.is_null() || unsafe { (*extension).magic != FDO_EXTENSION_MAGIC } {
        return STATUS_DELETE_PENDING;
    }
    let lower = unsafe { (*extension).lower_device };
    if lower.is_null() { return STATUS_DEVICE_NOT_READY; }

    // Remove teardown must run after the lower bus driver has consumed REMOVE.
    let stack = unsafe { (*irp).tail[8] as *mut cognacos_nt_abi::io::IO_STACK_LOCATION };
    if !stack.is_null() && unsafe { (*stack).major_function == IRP_MJ_PNP && (*stack).minor_function == IRP_MN_REMOVE_DEVICE } {
        unsafe {
            IoSetCompletionRoutine(irp, Some(remove_complete), device.cast(), 1, 1, 1);
            IoSkipCurrentIrpStackLocation(irp);
            return IoCallDriver(lower, irp);
        }
    }
    unsafe {
        IoSkipCurrentIrpStackLocation(irp);
        IoCallDriver(lower, irp)
    }
}

/// Called by the kernel PnP manager for each newly published bus PDO.
/// Non-IDE devices are rejected without creating an FDO.
unsafe extern "win64" fn add_device(
    driver: *mut c_void, pdo: *mut DEVICE_OBJECT,
) -> i32 {
    if driver.is_null() || pdo.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut interface = core::mem::MaybeUninit::<PCI_DEVICE_INTERFACE>::zeroed();
    let status = unsafe {
        IoPnPQueryBusInterface(
            pdo, &GUID_PCI_DEVICE_INTERFACE,
            core::mem::size_of::<PCI_DEVICE_INTERFACE>() as u16,
            cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION as u16,
            interface.as_mut_ptr().cast(),
        )
    };
    if status != STATUS_SUCCESS { return STATUS_NOT_SUPPORTED; }
    let interface = unsafe { interface.assume_init() };
    if (interface.header.size as usize) < core::mem::size_of::<PCI_DEVICE_INTERFACE>()
        || interface.header.version as u32 != cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION {
        return (0xC000_0059u32 as i32);
    }
    let Some(query) = interface.query_information else { return STATUS_NOT_SUPPORTED; };
    let mut identity = PCI_DEVICE_INFORMATION::default();
    let status = unsafe { query(interface.header.context, &mut identity) };
    if status != STATUS_SUCCESS { return status; }
    if identity.class_code != PCI_PDO_CLASS || identity.subclass != PCI_PDO_SUBCLASS_IDE {
        return STATUS_NOT_SUPPORTED;
    }

    let mut fdo = core::ptr::null_mut();
    let status = unsafe {
        IoCreateDevice(driver.cast(), core::mem::size_of::<PciIdexFdoExtension>() as u32,
            core::ptr::null_mut(), FILE_DEVICE_CONTROLLER, 0, 0, &mut fdo)
    };
    if status != STATUS_SUCCESS { return status; }
    if fdo.is_null() { return STATUS_INSUFFICIENT_RESOURCES; }
    let lower = unsafe { IoAttachDeviceToDeviceStack(fdo, pdo) };
    if lower.is_null() {
        unsafe { IoDeleteDevice(fdo); }
        return (0xC000_000Eu32 as i32);
    }
    let extension = unsafe { (*fdo).device_extension.cast::<PciIdexFdoExtension>() };
    unsafe {
        extension.write(PciIdexFdoExtension {
            magic: FDO_EXTENSION_MAGIC, lower_device: lower,
            physical_device: pdo, identity,
        });
        // Intermediate FDO mirrors the lower device's I/O buffering policy.
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

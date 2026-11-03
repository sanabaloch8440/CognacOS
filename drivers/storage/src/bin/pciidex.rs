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
    bus::{BUS_INTERFACE_STANDARD, GUID_BUS_INTERFACE_STANDARD, PCI_DEVICE_INFORMATION},
    io::{DEVICE_OBJECT, DRIVER_EXTENSION, DRIVER_OBJECT, IRP, IO_STACK_LOCATION},
    ntstatus::*,
    types::PVOID,
};

const IRP_MJ_MAXIMUM_FUNCTION: usize = 27;
const IRP_MJ_PNP: u8 = 0x1b;
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
 fn IoAllocateIrp(stack_size:i8,charge_quota:u8)->*mut IRP;
 fn IoFreeIrp(irp:*mut IRP);
 fn KeInitializeEvent(event:PVOID,event_type:i32,state:u8);
 fn KeSetEvent(event:PVOID,increment:i32,wait:u8)->i32;
 fn KeWaitForSingleObject(event:PVOID,wait_reason:i32,wait_mode:i32,alertable:u8,timeout:*const i64)->i32;
 fn IoGetAttachedDeviceReference(device:*mut DEVICE_OBJECT)->*mut DEVICE_OBJECT;
 fn ObDereferenceObject(object:PVOID);
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
IMPORT ObDereferenceObject
IMPORT IoGetAttachedDeviceReference
IMPORT KeWaitForSingleObject
IMPORT KeSetEvent
IMPORT KeInitializeEvent
IMPORT IoFreeIrp
IMPORT IoAllocateIrp

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
IAT ObDereferenceObject
IAT IoGetAttachedDeviceReference
IAT KeWaitForSingleObject
IAT KeSetEvent
IAT KeInitializeEvent
IAT IoFreeIrp
IAT IoAllocateIrp

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
            set_completion_routine(irp, Some(remove_complete), device.cast(), 1, 1, 1);copy_current_irp_stack_location_to_next(irp);
            return IoCallDriver(lower, irp);
        }
    }
    unsafe {
        skip_current_irp_stack_location(irp);
        IoCallDriver(lower, irp)
    }
}

/// Called by the kernel PnP manager for each newly published bus PDO.
/// Non-IDE devices are rejected without creating an FDO.
unsafe extern "win64" fn add_device(
    driver: *mut DRIVER_OBJECT, pdo: *mut DEVICE_OBJECT,
) -> i32 {
    if driver.is_null() || pdo.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut interface = core::mem::MaybeUninit::<BUS_INTERFACE_STANDARD>::zeroed();
    let status = unsafe {
        query_pci_bus_interface(
            pdo, &GUID_BUS_INTERFACE_STANDARD,
            core::mem::size_of::<BUS_INTERFACE_STANDARD>() as u16,
            cognacos_nt_abi::bus::BUS_INTERFACE_STANDARD_VERSION as u16,
            interface.as_mut_ptr().cast(),
        )
    };
    if status != STATUS_SUCCESS { return STATUS_NOT_SUPPORTED; }
    let interface = unsafe { interface.assume_init() };
    if (interface.size as usize) < core::mem::size_of::<BUS_INTERFACE_STANDARD>() || interface.version != 1 {
        if let Some(deref) = interface.interface_dereference { unsafe { deref(interface.context); } }
        return 0xC000_0059u32 as i32;
    }
    let Some(get_bus_data) = interface.get_bus_data else {
        if let Some(deref) = interface.interface_dereference { unsafe { deref(interface.context); } }
        return STATUS_NOT_SUPPORTED;
    };
    let mut config = [0u8; 64];
    let bytes = unsafe { get_bus_data(interface.context, 0, config.as_mut_ptr().cast(), 0, config.len() as u32) };
    if let Some(deref) = interface.interface_dereference { unsafe { deref(interface.context); } }
    if bytes < 16 { return STATUS_DEVICE_DATA_ERROR; }
    let mut identity = PCI_DEVICE_INFORMATION::default();
    identity.vendor_id = u16::from_le_bytes([config[0], config[1]]);
    identity.device_id = u16::from_le_bytes([config[2], config[3]]);
    identity.command = u16::from_le_bytes([config[4], config[5]]);
    identity.revision_id = config[8];
    identity.programming_interface = config[9];
    identity.subclass = config[10];
    identity.class_code = config[11];
    identity.header_type = config[14];
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
        return 0xC000_000Eu32 as i32;
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
    {
 let extension=unsafe{(*driver).driver_extension.cast::<DRIVER_EXTENSION>()};
 if extension.is_null(){return STATUS_INVALID_PARAMETER}
 unsafe{(*extension).add_device=Some(add_device);}
 STATUS_SUCCESS
}
}

#[panic_handler]

/// WDK inline macro equivalents: these are local source helpers, not PE imports.
fn current_stack_location(irp:*mut IRP)->*mut IO_STACK_LOCATION {
 if irp.is_null(){return core::ptr::null_mut()}
 unsafe{(*irp).tail[8] as *mut IO_STACK_LOCATION}
}
fn next_stack_location(irp:*mut IRP)->*mut IO_STACK_LOCATION {
 let current=current_stack_location(irp);
 if current.is_null(){core::ptr::null_mut()}else{unsafe{current.sub(1)}}
}
unsafe fn copy_current_irp_stack_location_to_next(irp:*mut IRP) {
 let current=current_stack_location(irp);let next=next_stack_location(irp);
 if !current.is_null()&&!next.is_null(){unsafe{
  let bytes=core::mem::offset_of!(IO_STACK_LOCATION,completion_routine);
  core::ptr::copy_nonoverlapping(current.cast::<u8>(),next.cast::<u8>(),bytes);
  (*next).control=0;
  (*next).completion_routine=None;
  (*next).context=core::ptr::null_mut();
 }}
}
unsafe fn skip_current_irp_stack_location(irp:*mut IRP) {
 if irp.is_null(){return}
 let current=current_stack_location(irp);if current.is_null(){return}
 unsafe{(*irp).current_location=(*irp).current_location.saturating_add(1);(*irp).tail[8]=current.add(1) as usize;}
}
unsafe fn set_completion_routine(
 irp:*mut IRP,
 routine:Option<unsafe extern "win64" fn(*mut DEVICE_OBJECT,*mut IRP,PVOID)->i32>,
 context:PVOID,success:u8,error:u8,cancel:u8,
) {
 let next=next_stack_location(irp);if next.is_null(){return}
 let mut control=0u8;
 if success!=0{control|=0x40} if error!=0{control|=0x80} if cancel!=0{control|=0x20}
 unsafe{(*next).completion_routine=routine;(*next).context=context;(*next).control=control;}
}
unsafe extern "win64" fn query_interface_complete(_device:*mut DEVICE_OBJECT,_irp:*mut IRP,context:PVOID)->i32 {
 if !context.is_null(){let _=unsafe{KeSetEvent(context,0,0)};}
 0xC0000016u32 as i32
}
/// Windows WDM IRP_MN_QUERY_INTERFACE path; no private kernel export is required.
unsafe fn query_pci_bus_interface(
 device:*mut DEVICE_OBJECT,guid:*const cognacos_nt_abi::bus::GUID,
 size:u16,version:u16,output:PVOID
)->i32 {
 if device.is_null()||guid.is_null()||output.is_null(){return STATUS_INVALID_PARAMETER}
 let target=unsafe{IoGetAttachedDeviceReference(device)};
 if target.is_null(){return STATUS_DELETE_PENDING}
 let stack_size=unsafe{(*target).stack_size.max(1)};
 let irp=unsafe{IoAllocateIrp(stack_size as i8,0)};
 if irp.is_null(){unsafe{ObDereferenceObject(target.cast())};return STATUS_INSUFFICIENT_RESOURCES}
 let mut event=[0u64;3];
 unsafe{KeInitializeEvent(event.as_mut_ptr().cast(),0,0);}
 let stack=next_stack_location(irp);
 if stack.is_null(){unsafe{IoFreeIrp(irp);ObDereferenceObject(target.cast())};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=0x1b; // IRP_MJ_PNP
  (*stack).minor_function=0x08; // IRP_MN_QUERY_INTERFACE
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<*const cognacos_nt_abi::bus::GUID>(),guid);
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<u16>(),size);
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(10).cast::<u16>(),version);
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(16).cast::<PVOID>(),output);
  set_completion_routine(irp,Some(query_interface_complete),event.as_mut_ptr().cast(),1,1,1);
 }
 let status=unsafe{IoCallDriver(target,irp)};
 if status==STATUS_PENDING {
  let wait=unsafe{KeWaitForSingleObject(event.as_mut_ptr().cast(),0,0,0,core::ptr::null())};
  if wait<0{unsafe{IoFreeIrp(irp);ObDereferenceObject(target.cast())};return wait}
 }
 let io_status=unsafe{(*irp).io_status.status.status};
 unsafe{IoFreeIrp(irp);ObDereferenceObject(target.cast())}
 if status==STATUS_PENDING{io_status}else if status>=0{io_status}else{status}
}

fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

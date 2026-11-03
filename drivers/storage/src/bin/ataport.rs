#![no_std]
#![no_main]
//! ATA port driver. Owns the lower controller FDO, validates buffered disk
//! requests, serializes miniport registration, and dispatches bounded PIO I/O.
//! The controller-specific PCI IDE function driver attaches above this FDO
//! and registers its implementation with IOCTL_ATA_PORT_REGISTER_MINIPORT.
use core::{cell::UnsafeCell, ffi::c_void, sync::atomic::{AtomicBool, Ordering}};
use cognacos_nt_abi::{
    bus::{GUID_PCI_DEVICE_INTERFACE, PCI_DEVICE_INTERFACE, PCI_DEVICE_INFORMATION},
    io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STACK_LOCATION},
    ntstatus::*, types::PVOID,
};
use cognacos_storage::ata_port::{
    AtaPortMiniportInterface, AtaPortOperation, AtaPortRequest, validate_request,
    ATA_PORT_INTERFACE_VERSION,
};

const IRP_MJ_CREATE:usize=0;
const IRP_MJ_CLOSE:usize=2;
const IRP_MJ_READ:usize=3;
const IRP_MJ_WRITE:usize=4;
const IRP_MJ_FLUSH_BUFFERS:usize=9;
const IRP_MJ_INTERNAL_DEVICE_CONTROL:usize=0x0f;
const IRP_MJ_PNP:usize=0x1b;
const IRP_MJ_MAXIMUM_FUNCTION:usize=27;
const IRP_MN_START_DEVICE:u8=0x00;
const IRP_MN_STOP_DEVICE:u8=0x04;
const IRP_MN_REMOVE_DEVICE:u8=0x02;
const IRP_MN_SURPRISE_REMOVAL:u8=0x17;
const FILE_DEVICE_CONTROLLER:u32=4;
const PCI_CLASS_MASS_STORAGE:u8=1;
const PCI_SUBCLASS_IDE:u8=1;
const FDO_MAGIC:u64=0x434f_474e_4154_4150;
const IOCTL_ATA_PORT_REGISTER_MINIPORT:u32=(FILE_DEVICE_CONTROLLER<<16)|(0x800<<2)|3;
const IOCTL_ATA_PORT_UNREGISTER_MINIPORT:u32=(FILE_DEVICE_CONTROLLER<<16)|(0x801<<2)|3;
const IOCTL_ATA_PORT_SUBMIT_MINIPORT:u32=(FILE_DEVICE_CONTROLLER<<16)|(0x802<<2)|3;
const IRP_BUFFERED_IO:u32=0x10;
const SECTOR_SIZE:u32=512;
const MAX_PIO_SECTORS:u32=256;

unsafe extern "win64" {
 fn IoCompleteRequest(irp:*mut IRP,priority_boost:i8);
 fn IoCreateDevice(driver:*mut DRIVER_OBJECT,extension_size:u32,name:*mut cognacos_nt_abi::object::UNICODE_STRING,device_type:u32,characteristics:u32,exclusive:u8,output:*mut *mut DEVICE_OBJECT)->i32;
 fn IoDeleteDevice(device:*mut DEVICE_OBJECT);
 fn IoAttachDeviceToDeviceStack(source:*mut DEVICE_OBJECT,target:*mut DEVICE_OBJECT)->*mut DEVICE_OBJECT;
 fn IoDetachDevice(target:*mut DEVICE_OBJECT);
 fn IoCallDriver(device:*mut DEVICE_OBJECT,irp:*mut IRP)->i32;
 fn IoCopyCurrentIrpStackLocationToNext(irp:*mut IRP);
 fn IoSkipCurrentIrpStackLocation(irp:*mut IRP);
 fn IoSetCompletionRoutine(irp:*mut IRP,routine:Option<unsafe extern "win64" fn(*mut DEVICE_OBJECT,*mut IRP,PVOID)->i32>,context:PVOID,success:u8,error:u8,cancel:u8);
 fn IoPnPQueryBusInterface(device:*mut DEVICE_OBJECT,guid:*const cognacos_nt_abi::bus::GUID,size:u16,version:u16,output:PVOID)->i32;
 fn IoPnPRegisterDeviceDriver(driver:*mut c_void,add_device:Option<unsafe extern "win64" fn(*mut c_void,*mut DEVICE_OBJECT)->i32>)->i32;
}
core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.macro IMPORT name
.global \name
.type \name,@function
\name: jmp QWORD PTR [rip + __imp_ntoskrnl_\name]
.size \name, .-\name
.endm
IMPORT IoCompleteRequest
IMPORT IoCreateDevice
IMPORT IoDeleteDevice
IMPORT IoAttachDeviceToDeviceStack
IMPORT IoDetachDevice
IMPORT IoCallDriver
IMPORT IoCopyCurrentIrpStackLocationToNext
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
IAT IoCompleteRequest
IAT IoCreateDevice
IAT IoDeleteDevice
IAT IoAttachDeviceToDeviceStack
IAT IoDetachDevice
IAT IoCallDriver
IAT IoCopyCurrentIrpStackLocationToNext
IAT IoSkipCurrentIrpStackLocation
IAT IoSetCompletionRoutine
IAT IoPnPQueryBusInterface
IAT IoPnPRegisterDeviceDriver
"#);

#[repr(C)]
struct AtaPortFdoExtension {
 magic:u64,
 lower:*mut DEVICE_OBJECT,
 physical:*mut DEVICE_OBJECT,
 miniport:Option<AtaPortMiniportInterface>,
 started:u8,
 removing:u8,
 active_requests:u32,
 reserved:[u8;2],
}
const _:()=assert!(core::mem::size_of::<AtaPortFdoExtension>()<=256);

static PORT_LOCK:AtomicBool=AtomicBool::new(false);
struct ActiveRequest(*mut AtaPortFdoExtension);
impl Drop for ActiveRequest {
 fn drop(&mut self) {
  let _guard=Guard::acquire();
  unsafe{if (*self.0).active_requests>0{(*self.0).active_requests-=1;}}
 }
}
struct Guard;
impl Guard {
 fn acquire()->Self { while PORT_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();} Self }
}
impl Drop for Guard { fn drop(&mut self){PORT_LOCK.store(false,Ordering::Release);} }

fn current_stack(irp:*mut IRP)->*mut IO_STACK_LOCATION {
 if irp.is_null(){core::ptr::null_mut()}else{unsafe{(*irp).tail[8] as *mut IO_STACK_LOCATION}}
}
unsafe fn complete(irp:*mut IRP,status:i32,information:usize)->i32 {
 unsafe{(*irp).io_status.status.status=status;(*irp).io_status.information=information;IoCompleteRequest(irp,0);}
 status
}
fn extension(device:*mut DEVICE_OBJECT)->Option<*mut AtaPortFdoExtension>{
 if device.is_null(){return None}
 let ext=unsafe{(*device).device_extension.cast::<AtaPortFdoExtension>()};
 if ext.is_null()||unsafe{(*ext).magic!=FDO_MAGIC}{None}else{Some(ext)}
}

/// Miniport registration is delivered as a private METHOD_NEITHER internal
/// control IRP from pciide.sys. The table is copied into the controller FDO.
unsafe fn register_miniport(ext:*mut AtaPortFdoExtension,table:*const AtaPortMiniportInterface)->i32{
 if table.is_null(){return STATUS_INVALID_PARAMETER}
 let candidate=unsafe{core::ptr::read_unaligned(table)};
 if candidate.size as usize!=core::mem::size_of::<AtaPortMiniportInterface>()||candidate.version!=ATA_PORT_INTERFACE_VERSION||candidate.submit.is_none(){return STATUS_REVISION_MISMATCH}
 {
  let _guard=Guard::acquire();
  if unsafe{(*ext).started!=1||(*ext).removing!=0}{return STATUS_DEVICE_NOT_READY}
  if unsafe{(*ext).miniport.is_some()}{return STATUS_DEVICE_BUSY}
  unsafe{(*ext).started=2;}
 }
 let mut status=STATUS_SUCCESS;
 if let Some(init)=candidate.initialize {status=unsafe{init(candidate.miniport_context,core::ptr::null())};}
 if status==STATUS_SUCCESS {
  if let Some(start)=candidate.start {status=unsafe{start(candidate.miniport_context)};}
 }
 if status!=STATUS_SUCCESS {
  if let Some(stop)=candidate.stop {let _=unsafe{stop(candidate.miniport_context)};}
  let _guard=Guard::acquire();unsafe{(*ext).started=1;}
  return status;
 }
 let _guard=Guard::acquire();
 unsafe{(*ext).miniport=Some(candidate);(*ext).started=1;}
 STATUS_SUCCESS
}
unsafe fn unregister_miniport(ext:*mut AtaPortFdoExtension,context:*mut c_void)->i32{
 let current={
  let _guard=Guard::acquire();
  let Some(current)= (unsafe{(*ext).miniport}) else{return STATUS_DEVICE_NOT_READY};
  if current.miniport_context!=context{return STATUS_ACCESS_DENIED}
  unsafe{(*ext).started=2;(*ext).miniport=None;}
  current
 };
 while unsafe{(*ext).active_requests}!=0 {core::hint::spin_loop();}
 let mut status=STATUS_SUCCESS;
 if let Some(stop)=current.stop {status=unsafe{stop(current.miniport_context)};}
 if let Some(unload)=current.unload {unsafe{unload(current.miniport_context)};}
 let _guard=Guard::acquire();unsafe{(*ext).started=1;}
 status
}

unsafe extern "win64" fn pnp_complete(_lower:*mut DEVICE_OBJECT,irp:*mut IRP,context:PVOID)->i32{
 let fdo=context.cast::<DEVICE_OBJECT>();if fdo.is_null()||irp.is_null(){return STATUS_SUCCESS}
 if let Some(ext)=extension(fdo){
  let stack=current_stack(irp);let minor=if stack.is_null(){0xff}else{unsafe{(*stack).minor_function}};
  let status=unsafe{(*irp).io_status.status.status};
  if status==STATUS_SUCCESS {match minor{IRP_MN_START_DEVICE=>unsafe{(*ext).started=1},IRP_MN_STOP_DEVICE|IRP_MN_SURPRISE_REMOVAL=>unsafe{(*ext).started=0},_=>{}}}
 }
 STATUS_SUCCESS
}

unsafe extern "win64" fn remove_complete(_lower:*mut DEVICE_OBJECT,irp:*mut IRP,context:PVOID)->i32{
 let fdo=context.cast::<DEVICE_OBJECT>();if fdo.is_null()||irp.is_null(){return STATUS_SUCCESS}
 if let Some(ext)=extension(fdo){
  unsafe{(*ext).removing=1;(*ext).magic=0;}
  let lower=unsafe{(*ext).lower};if !lower.is_null(){unsafe{IoDetachDevice(lower)}}
 }
 unsafe{IoDeleteDevice(fdo)}
 STATUS_SUCCESS
}

unsafe fn execute_request(ext:*mut AtaPortFdoExtension,request_ptr:*const AtaPortRequest)->(i32,u32){
 if request_ptr.is_null(){return (STATUS_INVALID_PARAMETER,0)}
 let request=unsafe{core::ptr::read_unaligned(request_ptr)};
 let expected=match validate_request(&request){Ok(n)=>n,Err(status)=>return(status,0)};
 let miniport={
  let _guard=Guard::acquire();
  if unsafe{(*ext).started!=1||(*ext).removing!=0}{return(STATUS_DEVICE_NOT_READY,0)}
  let Some(current)=(unsafe{(*ext).miniport}) else{return(STATUS_DEVICE_NOT_READY,0)};
  unsafe{(*ext).active_requests=(*ext).active_requests.saturating_add(1);}
  current
 };
 let _active=ActiveRequest(ext);
 let Some(submit)=miniport.submit else{return(STATUS_DEVICE_NOT_READY,0)};
 let mut transferred=0u32;
 let status=unsafe{submit(miniport.miniport_context,&request,&mut transferred)};
 if status!=STATUS_SUCCESS{return(status,0)}
 if transferred!=expected{return(STATUS_DEVICE_DATA_ERROR,0)}
 (STATUS_SUCCESS,transferred)
}

unsafe extern "win64" fn dispatch(device:*mut DEVICE_OBJECT,irp:*mut IRP)->i32{
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let Some(ext)=extension(device) else{return STATUS_DELETE_PENDING};
 let stack=current_stack(irp);if stack.is_null(){return STATUS_INVALID_PARAMETER}
 let major=unsafe{(*stack).major_function as usize};
 if major==IRP_MJ_PNP {
  let minor=unsafe{(*stack).minor_function};
  if minor==IRP_MN_REMOVE_DEVICE {
   unsafe{IoSetCompletionRoutine(irp,Some(remove_complete),device.cast(),1,1,1);IoSkipCurrentIrpStackLocation(irp);return IoCallDriver((*ext).lower,irp);}
  }
  if minor==IRP_MN_START_DEVICE||minor==IRP_MN_STOP_DEVICE||minor==IRP_MN_SURPRISE_REMOVAL {
   unsafe{IoSetCompletionRoutine(irp,Some(pnp_complete),device.cast(),1,1,1);IoSkipCurrentIrpStackLocation(irp);return IoCallDriver((*ext).lower,irp);}
  }
  unsafe{IoSkipCurrentIrpStackLocation(irp);return IoCallDriver((*ext).lower,irp);}
 }
 if major==IRP_MJ_CREATE||major==IRP_MJ_CLOSE{return unsafe{complete(irp,STATUS_SUCCESS,0)}}
 if major==IRP_MJ_INTERNAL_DEVICE_CONTROL{
  // DeviceIoControl: IOCTL code +16 and METHOD_NEITHER Type3InputBuffer +24
  // relative to Parameters, matching the x64 WDM IO_STACK_LOCATION layout.
  let code=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(16).cast::<u32>())};
  let input=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(24).cast::<*const c_void>())};
  let (status,information)=match code{
   IOCTL_ATA_PORT_REGISTER_MINIPORT=>(unsafe{register_miniport(ext,input.cast())},0),
   IOCTL_ATA_PORT_UNREGISTER_MINIPORT=>(unsafe{unregister_miniport(ext,input.cast_mut())},0),
   IOCTL_ATA_PORT_SUBMIT_MINIPORT=>unsafe{execute_request(ext,input.cast())},
   _=>(STATUS_INVALID_DEVICE_REQUEST,0),
  };
  return unsafe{complete(irp,status,information as usize)}
 }
 let operation=match major{
  IRP_MJ_READ=>AtaPortOperation::Read,
  IRP_MJ_WRITE=>AtaPortOperation::Write,
  IRP_MJ_FLUSH_BUFFERS=>AtaPortOperation::Flush,
  _=>return unsafe{complete(irp,STATUS_INVALID_DEVICE_REQUEST,0)}
 };
 let mut request=AtaPortRequest{
  size:core::mem::size_of::<AtaPortRequest>() as u32,operation,request_id:1,
  lba:0,sector_count:0,flags:0,system_buffer:core::ptr::null_mut(),
  sg_list:core::ptr::null(),sg_count:0,timeout_ms:30_000,
  packet_cdb:core::ptr::null(),packet_cdb_length:0,reserved:[0;7],
 };
 if operation!=AtaPortOperation::Flush {
  let length=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
  let offset=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<i64>())};
  let buffer=unsafe{(*irp).associated_irp[0] as *mut u8};let flags=unsafe{(*irp).flags};
  if flags&IRP_BUFFERED_IO==0||buffer.is_null()||offset<0||offset as u64%SECTOR_SIZE as u64!=0||length==0||length%SECTOR_SIZE!=0||length/SECTOR_SIZE>MAX_PIO_SECTORS{return unsafe{complete(irp,STATUS_INVALID_PARAMETER,0)}}
  request.lba=offset as u64/SECTOR_SIZE as u64;
  request.sector_count=length/SECTOR_SIZE;
  request.system_buffer=buffer;
 }
 let (status,information)=unsafe{execute_request(ext,&request)};
 unsafe{complete(irp,status,if status==STATUS_SUCCESS{information as usize}else{0})}
}

unsafe extern "win64" fn add_device(driver:*mut c_void,pdo:*mut DEVICE_OBJECT)->i32{
 if driver.is_null()||pdo.is_null(){return STATUS_INVALID_PARAMETER}
 let mut interface=core::mem::MaybeUninit::<PCI_DEVICE_INTERFACE>::zeroed();
 let status=unsafe{IoPnPQueryBusInterface(pdo,&GUID_PCI_DEVICE_INTERFACE,core::mem::size_of::<PCI_DEVICE_INTERFACE>() as u16,cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION as u16,interface.as_mut_ptr().cast())};
 if status!=STATUS_SUCCESS{return STATUS_NOT_SUPPORTED}
 let interface=unsafe{interface.assume_init()};
 if interface.header.size as usize<core::mem::size_of::<PCI_DEVICE_INTERFACE>()||interface.header.version as u32!=cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION{return STATUS_REVISION_MISMATCH}
 let Some(query)=interface.query_information else{return STATUS_NOT_SUPPORTED};
 let mut info=PCI_DEVICE_INFORMATION::default();let status=unsafe{query(interface.header.context,&mut info)};if status!=STATUS_SUCCESS{return status}
 if info.class_code!=PCI_CLASS_MASS_STORAGE||info.subclass!=PCI_SUBCLASS_IDE{return STATUS_NOT_SUPPORTED}
 let mut fdo=core::ptr::null_mut();
 let status=unsafe{IoCreateDevice(driver.cast(),core::mem::size_of::<AtaPortFdoExtension>() as u32,core::ptr::null_mut(),FILE_DEVICE_CONTROLLER,0,0,&mut fdo)};
 if status!=STATUS_SUCCESS{return status}if fdo.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let lower=unsafe{IoAttachDeviceToDeviceStack(fdo,pdo)};
 if lower.is_null(){unsafe{IoDeleteDevice(fdo)};return STATUS_DEVICE_NOT_READY}
 let ext=unsafe{(*fdo).device_extension.cast::<AtaPortFdoExtension>()};
 unsafe{ext.write(AtaPortFdoExtension{magic:FDO_MAGIC,lower,physical:pdo,miniport:None,started:0,removing:0,active_requests:0,reserved:[0;2]});(*fdo).flags=(((*fdo).flags&!0x80)|((*lower).flags&(0x02|0x04)))|0x04;}
 STATUS_SUCCESS
}

#[used]
#[unsafe(link_section=".rodata")]
static DRIVER_SIGNATURE:[u8;8]=*b"ATAPORT_";
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver:*mut DRIVER_OBJECT,_context:PVOID)->i32{
 if driver.is_null(){return STATUS_INVALID_PARAMETER}
 unsafe{for index in 0..=IRP_MJ_MAXIMUM_FUNCTION{(*driver).major_function[index]=Some(dispatch)}(*driver).driver_unload=None;}
 unsafe{IoPnPRegisterDeviceDriver(driver.cast(),Some(add_device))}
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>)->!{loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}}

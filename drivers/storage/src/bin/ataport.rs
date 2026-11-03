#![no_std]
#![no_main]
//! ATA port driver. Owns the lower controller FDO, validates buffered disk
//! requests, serializes miniport registration, and dispatches bounded PIO I/O.
//! The controller-specific PCI IDE function driver attaches above this FDO
//! and registers its implementation with IOCTL_ATA_PORT_REGISTER_MINIPORT.
use core::{ffi::c_void, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
use cognacos_nt_abi::{
    bus::{BUS_INTERFACE_STANDARD, GUID_BUS_INTERFACE_STANDARD, PCI_DEVICE_INFORMATION},
    io::{DRIVER_EXTENSION,DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STACK_LOCATION},
    ntstatus::*, types::PVOID,
};
use cognacos_storage::ata_port::{
    AtaPortMiniportInterface, AtaPortOperation, AtaPortRequest, validate_request,
    ATA_PORT_INTERFACE_VERSION,
};
use cognacos_storage::disk_class::{parse_ata_identify, DiskDescriptor};

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
const FILE_DEVICE_DISK:u32=7;
const DISK_PDO_MAGIC:u64=0x434f_474e_4449_534b;
const IRP_MN_QUERY_DEVICE_RELATIONS:u8=0x07;
const BUS_RELATIONS:u32=0;
const MAX_PIO_SECTORS:u32=256;

unsafe extern "win64" {
 fn IoCompleteRequest(irp:*mut IRP,priority_boost:i8);
 fn IoCreateDevice(driver:*mut DRIVER_OBJECT,extension_size:u32,name:*mut cognacos_nt_abi::object::UNICODE_STRING,device_type:u32,characteristics:u32,exclusive:u8,output:*mut *mut DEVICE_OBJECT)->i32;
 fn IoDeleteDevice(device:*mut DEVICE_OBJECT);
 fn IoAttachDeviceToDeviceStack(source:*mut DEVICE_OBJECT,target:*mut DEVICE_OBJECT)->*mut DEVICE_OBJECT;
 fn IoDetachDevice(target:*mut DEVICE_OBJECT);
 fn IoCallDriver(device:*mut DEVICE_OBJECT,irp:*mut IRP)->i32;

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
\name: jmp QWORD PTR [rip + __imp_ntoskrnl_\name]
.size \name, .-\name
.endm
IMPORT IoCompleteRequest
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
IAT IoCompleteRequest
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

#[repr(C)]
struct AtaPortDeviceRelations {
 count:u32,
 objects:[*mut DEVICE_OBJECT;64],
}
impl AtaPortDeviceRelations {
 const EMPTY:Self=Self{count:0,objects:[core::ptr::null_mut();64]};
}

#[repr(C)]
struct AtaPortFdoExtension {
 magic:u64,
 lower:*mut DEVICE_OBJECT,
 physical:*mut DEVICE_OBJECT,
 self_device:*mut DEVICE_OBJECT,
 driver:*mut DRIVER_OBJECT,
 miniport:Option<AtaPortMiniportInterface>,
 started:u8,
 removing:u8,
 active_requests:u32,
 reserved:[u8;2],
 relations:AtaPortDeviceRelations,
}
const _:()=assert!(core::mem::size_of::<AtaPortFdoExtension>()<=768);

#[repr(C)]
struct AtaDiskPdoExtension {
 magic:u64,
 parent:*mut DEVICE_OBJECT,
 slot:u8,
 reserved:[u8;7],
 descriptor:DiskDescriptor,
}
const _:()=assert!(core::mem::size_of::<AtaDiskPdoExtension>()<=160);
static NEXT_REQUEST_ID:AtomicU64=AtomicU64::new(1);

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


/// WDK inline macro equivalents. These are deliberately local to the driver;
/// none of these identifiers is imported from ntoskrnl.exe.
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
 if success!=0{control|=0x40}
 if error!=0{control|=0x80}
 if cancel!=0{control|=0x20}
 unsafe{(*next).completion_routine=routine;(*next).context=context;(*next).control=control;}
}

/// Completion for a synchronous WDM IRP_MN_QUERY_INTERFACE request. Returning
/// STATUS_MORE_PROCESSING_REQUIRED leaves the caller-allocated IRP intact.
unsafe extern "win64" fn query_interface_complete(
 _device:*mut DEVICE_OBJECT,_irp:*mut IRP,context:PVOID
)->i32 {
 if !context.is_null(){let _=unsafe{KeSetEvent(context,0,0)};}
 0xC0000016u32 as i32
}

/// Query a bus-provided interface using the Windows PnP IRP contract, not a
/// Query the standard Windows bus interface synchronously through the PnP IRP.
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

fn disk_extension(device:*mut DEVICE_OBJECT)->Option<*mut AtaDiskPdoExtension>{
 if device.is_null(){return None}
 let ext=unsafe{(*device).device_extension.cast::<AtaDiskPdoExtension>()};
 if ext.is_null()||unsafe{(*ext).magic!=DISK_PDO_MAGIC}{None}else{Some(ext)}
}

unsafe fn identify_slot(ext:*mut AtaPortFdoExtension,slot:u8)->Result<DiskDescriptor,i32>{
 let mut buffer=[0u8;512];
 let request=AtaPortRequest{
  size:core::mem::size_of::<AtaPortRequest>() as u32,
  operation:AtaPortOperation::Identify,request_id:NEXT_REQUEST_ID.fetch_add(1,Ordering::Relaxed),
  lba:0,sector_count:1,flags:slot as u32,system_buffer:buffer.as_mut_ptr(),
  sg_list:core::ptr::null(),sg_count:0,timeout_ms:30_000,
  packet_cdb:core::ptr::null(),packet_cdb_length:0,reserved:[0;7],
 };
 let (status,bytes)=unsafe{execute_request(ext,&request)};
 if status!=STATUS_SUCCESS{return Err(status)}
 if bytes!=512{return Err(STATUS_DEVICE_DATA_ERROR)}
 let words:[u16;256]=core::array::from_fn(|index|u16::from_le_bytes([buffer[index*2],buffer[index*2+1]]));
 parse_ata_identify(&words).map_err(cognacos_storage::error_to_ntstatus)
}

unsafe fn publish_disk_child(ext:*mut AtaPortFdoExtension,slot:u8,descriptor:DiskDescriptor)->i32{
 if unsafe{(*ext).relations.count as usize}>=64{return STATUS_INSUFFICIENT_RESOURCES}
 if descriptor.geometry.logical_sector_size!=SECTOR_SIZE{return STATUS_NOT_SUPPORTED}
 let mut child=core::ptr::null_mut();
 let status=unsafe{IoCreateDevice((*ext).driver,core::mem::size_of::<AtaDiskPdoExtension>() as u32,
  core::ptr::null_mut(),FILE_DEVICE_DISK,0,0,&mut child)};
 if status!=STATUS_SUCCESS{return status}
 if child.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let child_ext=unsafe{(*child).device_extension.cast::<AtaDiskPdoExtension>()};
 unsafe{
  child_ext.write(AtaDiskPdoExtension{magic:DISK_PDO_MAGIC,parent:unsafe{(*ext).self_device},slot,reserved:[0;7],descriptor});
  (*child).flags|=0x0000_1004;
 }
 let relations=unsafe{&mut (*ext).relations};
 let index=relations.count as usize;
 relations.objects[index]=child;
 relations.count+=1;
 STATUS_SUCCESS
}

unsafe fn discover_disk_children(ext:*mut AtaPortFdoExtension){
 for slot in 0..4u8{
  let count=unsafe{(*ext).relations.count as usize};
  if unsafe{(*ext).relations.objects[..count].iter().any(|child|{
   let Some(disk)=disk_extension(*child) else{return false};
   (*disk).slot==slot
  })}{continue}
  if let Ok(descriptor)=unsafe{identify_slot(ext,slot)}{
   let _=unsafe{publish_disk_child(ext,slot,descriptor)};
  }
 }
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
 {
  let _guard=Guard::acquire();
  unsafe{(*ext).miniport=Some(candidate);(*ext).started=1;}
 }
 unsafe{discover_disk_children(ext);}
 let physical=unsafe{(*ext).physical};
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


unsafe fn dispatch_disk_pdo(device:*mut DEVICE_OBJECT,disk:*mut AtaDiskPdoExtension,irp:*mut IRP)->i32{
 if irp.is_null(){return STATUS_INVALID_PARAMETER}
 let stack=current_stack(irp);if stack.is_null(){return STATUS_INVALID_PARAMETER}
 let major=unsafe{(*stack).major_function as usize};
 if major==IRP_MJ_CREATE||major==IRP_MJ_CLOSE{return unsafe{complete(irp,STATUS_SUCCESS,0)}}
 if major==IRP_MJ_PNP{
  let minor=unsafe{(*stack).minor_function};
  if minor==IRP_MN_REMOVE_DEVICE{
   unsafe{(*disk).magic=0;IoDeleteDevice(device);}
   return unsafe{complete(irp,STATUS_SUCCESS,0)}
  }
  return unsafe{complete(irp,STATUS_SUCCESS,0)}
 }
 let operation=match major{
  IRP_MJ_READ=>AtaPortOperation::Read,
  IRP_MJ_WRITE=>AtaPortOperation::Write,
  IRP_MJ_FLUSH_BUFFERS=>AtaPortOperation::Flush,
  _=>return unsafe{complete(irp,STATUS_INVALID_DEVICE_REQUEST,0)}
 };
 let descriptor=unsafe{(*disk).descriptor};
 let parent=unsafe{(*disk).parent};
 let Some(parent_ext)=extension(parent) else{return unsafe{complete(irp,STATUS_DEVICE_NOT_READY,0)}};
 let mut request=AtaPortRequest{
  size:core::mem::size_of::<AtaPortRequest>() as u32,operation,
  request_id:NEXT_REQUEST_ID.fetch_add(1,Ordering::Relaxed),lba:0,sector_count:0,
  flags:unsafe{(*disk).slot as u32},system_buffer:core::ptr::null_mut(),
  sg_list:core::ptr::null(),sg_count:0,timeout_ms:30_000,
  packet_cdb:core::ptr::null(),packet_cdb_length:0,reserved:[0;7],
 };
 if operation!=AtaPortOperation::Flush{
  let length=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
  let offset=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<i64>())};
  let buffer=unsafe{(*irp).associated_irp[0] as *mut u8};
  let flags=unsafe{(*irp).flags};
  let sector_size=descriptor.geometry.logical_sector_size;
  if flags&IRP_BUFFERED_IO==0||buffer.is_null()||offset<0||sector_size==0
   ||offset as u64%sector_size as u64!=0||length==0||length%sector_size!=0
   ||length/sector_size>MAX_PIO_SECTORS{
   return unsafe{complete(irp,STATUS_INVALID_PARAMETER,0)}
  }
  let lba=offset as u64/sector_size as u64;
  let sectors=length/sector_size;
  if descriptor.geometry.validate_io(lba,sectors).is_err(){
   return unsafe{complete(irp,STATUS_INVALID_PARAMETER,0)}
  }
  request.lba=lba;request.sector_count=sectors;request.system_buffer=buffer;
 }
 let (status,information)=unsafe{execute_request(parent_ext,&request)};
 unsafe{complete(irp,status,if status==STATUS_SUCCESS{information as usize}else{0})}
}

unsafe extern "win64" fn dispatch(device:*mut DEVICE_OBJECT,irp:*mut IRP)->i32{
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let Some(ext)=extension(device) else{
  if let Some(disk)=disk_extension(device){return unsafe{dispatch_disk_pdo(device,disk,irp)}}
  return STATUS_DELETE_PENDING
 };
 let stack=current_stack(irp);if stack.is_null(){return STATUS_INVALID_PARAMETER}
 let major=unsafe{(*stack).major_function as usize};
 if major==IRP_MJ_PNP {
  let minor=unsafe{(*stack).minor_function};
  if minor==IRP_MN_QUERY_DEVICE_RELATIONS {
   let relation_type=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
   if relation_type==BUS_RELATIONS {
    let relations=unsafe{core::ptr::addr_of!((*ext).relations)};
    return unsafe{complete(irp,STATUS_SUCCESS,relations as usize)}
   }
  }
  if minor==IRP_MN_REMOVE_DEVICE {
   let physical=unsafe{(*ext).physical};
   unsafe{(*ext).relations=AtaPortDeviceRelations::EMPTY;}
   unsafe{copy_current_irp_stack_location_to_next(irp);set_completion_routine(irp,Some(remove_complete),device.cast(),1,1,1);return IoCallDriver((*ext).lower,irp);}
  }
  if minor==IRP_MN_START_DEVICE||minor==IRP_MN_STOP_DEVICE||minor==IRP_MN_SURPRISE_REMOVAL {
   unsafe{copy_current_irp_stack_location_to_next(irp);set_completion_routine(irp,Some(pnp_complete),device.cast(),1,1,1);return IoCallDriver((*ext).lower,irp);}
  }
  unsafe{skip_current_irp_stack_location(irp);return IoCallDriver((*ext).lower,irp);}
 }
 if major==IRP_MJ_CREATE||major==IRP_MJ_CLOSE{return unsafe{complete(irp,STATUS_SUCCESS,0)}}
 if major==IRP_MJ_INTERNAL_DEVICE_CONTROL{
  // DeviceIoControl: IOCTL code +8 and METHOD_NEITHER Type3InputBuffer +16
  // relative to Parameters, matching the x64 WDM IO_STACK_LOCATION layout.
  let code=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<u32>())};
  let input=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(16).cast::<*const c_void>())};
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

unsafe extern "win64" fn add_device(driver:*mut DRIVER_OBJECT,pdo:*mut DEVICE_OBJECT)->i32{
 if driver.is_null()||pdo.is_null(){return STATUS_INVALID_PARAMETER}
 let mut interface=core::mem::MaybeUninit::<BUS_INTERFACE_STANDARD>::zeroed();
 let status=unsafe{query_pci_bus_interface(pdo,&GUID_BUS_INTERFACE_STANDARD,core::mem::size_of::<BUS_INTERFACE_STANDARD>() as u16,cognacos_nt_abi::bus::BUS_INTERFACE_STANDARD_VERSION as u16,interface.as_mut_ptr().cast())};
 if status!=STATUS_SUCCESS{return STATUS_NOT_SUPPORTED}
 let interface=unsafe{interface.assume_init()};
 if (interface.size as usize)<core::mem::size_of::<BUS_INTERFACE_STANDARD>()||interface.version!=1 {
  if let Some(deref)=interface.interface_dereference{unsafe{deref(interface.context)}}
  return STATUS_REVISION_MISMATCH;
 }
 let Some(get_bus_data)=interface.get_bus_data else {
  if let Some(deref)=interface.interface_dereference{unsafe{deref(interface.context)}}
  return STATUS_NOT_SUPPORTED;
 };
 let mut config=[0u8;64];
 let bytes=unsafe{get_bus_data(interface.context,0,config.as_mut_ptr().cast(),0,config.len() as u32)};
 if let Some(deref)=interface.interface_dereference{unsafe{deref(interface.context)}}
 if bytes<16{return STATUS_DEVICE_DATA_ERROR}
 let mut info=PCI_DEVICE_INFORMATION::default();
 info.vendor_id=u16::from_le_bytes([config[0],config[1]]);
 info.device_id=u16::from_le_bytes([config[2],config[3]]);
 info.command=u16::from_le_bytes([config[4],config[5]]);
 info.revision_id=config[8];
 info.programming_interface=config[9];
 info.subclass=config[10];
 info.class_code=config[11];
 info.header_type=config[14];
 if info.class_code!=PCI_CLASS_MASS_STORAGE||info.subclass!=PCI_SUBCLASS_IDE{return STATUS_NOT_SUPPORTED}
 let mut fdo=core::ptr::null_mut();
 let status=unsafe{IoCreateDevice(driver.cast(),core::mem::size_of::<AtaPortFdoExtension>() as u32,core::ptr::null_mut(),FILE_DEVICE_CONTROLLER,0,0,&mut fdo)};
 if status!=STATUS_SUCCESS{return status}if fdo.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let lower=unsafe{IoAttachDeviceToDeviceStack(fdo,pdo)};
 if lower.is_null(){unsafe{IoDeleteDevice(fdo)};return STATUS_DEVICE_NOT_READY}
 let ext=unsafe{(*fdo).device_extension.cast::<AtaPortFdoExtension>()};
 unsafe{ext.write(AtaPortFdoExtension{magic:FDO_MAGIC,lower,physical:pdo,self_device:fdo,driver:driver.cast(),miniport:None,started:0,removing:0,active_requests:0,reserved:[0;2],relations:AtaPortDeviceRelations::EMPTY});(*fdo).flags=(((*fdo).flags&!0x80)|((*lower).flags&(0x02|0x04)))|0x04;}
 STATUS_SUCCESS
}

#[used]
#[unsafe(link_section=".rodata")]
static DRIVER_SIGNATURE:[u8;8]=*b"ATAPORT_";
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver:*mut DRIVER_OBJECT,_context:PVOID)->i32{
 if driver.is_null(){return STATUS_INVALID_PARAMETER}
 unsafe{for index in 0..=IRP_MJ_MAXIMUM_FUNCTION{(*driver).major_function[index]=Some(dispatch)}(*driver).driver_unload=None;}
 {
  let extension=unsafe{(*driver).driver_extension.cast::<DRIVER_EXTENSION>()};
  if extension.is_null(){return STATUS_INVALID_PARAMETER}
  unsafe{(*extension).add_device=Some(add_device);}
  STATUS_SUCCESS
 }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>)->!{loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}}

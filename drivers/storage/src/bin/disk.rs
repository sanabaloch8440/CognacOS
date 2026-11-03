#![no_std]
#![no_main]
//! Disk class driver. Binds to ATA disk PDOs published by ataport.sys, creates
//! a function FDO, exposes geometry/capacity/identity queries, and forwards
//! block I/O to the lower port stack. Partition and filesystem discovery are
//! separate upper layers; this driver does not parse FAT metadata.
use core::ffi::c_void;
use cognacos_nt_abi::{
    io::{DEVICE_OBJECT, DRIVER_EXTENSION, DRIVER_OBJECT, IRP, IO_STACK_LOCATION},
    ntstatus::*,
    types::PVOID,
};
use cognacos_storage::disk_class::{DiskDescriptor, DiskIdentity};

const IRP_MJ_DEVICE_CONTROL:usize=0x0e;
const IRP_MJ_PNP:usize=0x1b;
const IRP_MN_REMOVE_DEVICE:u8=0x02;
const IRP_MJ_MAXIMUM_FUNCTION:usize=27;
const FILE_DEVICE_DISK:u32=7;
const FDO_MAGIC:u64=0x434f_474e_4449_5346;
const PDO_MAGIC:u64=0x434f_474e_4449_534b;
const IOCTL_DISK_GET_DRIVE_GEOMETRY:u32=0x0007_0000;
const IOCTL_DISK_GET_LENGTH_INFO:u32=0x0007_405c;
const IOCTL_COGNACOS_DISK_QUERY_IDENTITY:u32=0x002d_1c00;
const IOCTL_STORAGE_QUERY_PROPERTY:u32=0x002d_1400;
const IOCTL_STORAGE_GET_DEVICE_NUMBER:u32=0x002d_1080;

#[repr(C)]
struct AtaDiskPdoExtension {
    magic:u64,
    parent:*mut DEVICE_OBJECT,
    slot:u8,
    reserved:[u8;7],
    descriptor:DiskDescriptor,
}
#[repr(C)]
struct DiskFdoExtension {
    magic:u64,
    lower:*mut DEVICE_OBJECT,
    physical:*mut DEVICE_OBJECT,
    descriptor:DiskDescriptor,
    slot:u8,
    reserved:[u8;7],
}
#[repr(C)]
#[derive(Clone,Copy)]
struct DiskGeometryInformation {
    cylinders:i64,
    media_type:u32,
    tracks_per_cylinder:u32,
    sectors_per_track:u32,
    bytes_per_sector:u32,
}
#[repr(C)]
#[derive(Clone,Copy)]
struct StorageDeviceDescriptor {
    version:u32,size:u32,device_type:u8,device_type_modifier:u8,
    removable_media:u8,command_queueing:u8,
    vendor_id_offset:u32,product_id_offset:u32,product_revision_offset:u32,
    serial_number_offset:u32,bus_type:u32,raw_properties_length:u32,
}
#[repr(C)]
#[derive(Clone,Copy)]
struct StorageDeviceNumber { device_type:u32,device_number:u32,partition_number:u32 }

#[repr(C)]
#[derive(Clone,Copy)]
struct DiskIdentityInformation {
    stable_id:u64,
    sector_count:u64,
    capacity_bytes:u64,
    logical_sector_size:u32,
    physical_sector_size:u32,
    removable:u8,
    supports_flush:u8,
    reserved:[u8;6],
    identity:DiskIdentity,
}

unsafe extern "win64" {
    fn IoCreateDevice(driver:*mut DRIVER_OBJECT,extension_size:u32,
        name:*mut cognacos_nt_abi::object::UNICODE_STRING,device_type:u32,
        characteristics:u32,exclusive:u8,output:*mut *mut DEVICE_OBJECT)->i32;
    fn IoDeleteDevice(device:*mut DEVICE_OBJECT);
    fn IoAttachDeviceToDeviceStack(source:*mut DEVICE_OBJECT,target:*mut DEVICE_OBJECT)->*mut DEVICE_OBJECT;
    fn IoDetachDevice(target:*mut DEVICE_OBJECT);
    fn IoCallDriver(device:*mut DEVICE_OBJECT,irp:*mut IRP)->i32;
    fn IoCompleteRequest(irp:*mut IRP,priority_boost:i8);
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
IMPORT IoCreateDevice
IMPORT IoDeleteDevice
IMPORT IoAttachDeviceToDeviceStack
IMPORT IoDetachDevice
IMPORT IoCallDriver

IMPORT IoCompleteRequest
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

IAT IoCompleteRequest
"#);


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
 if !current.is_null()&&!next.is_null(){unsafe{core::ptr::copy(current,next,1)}}
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
 unsafe{(*next).completion_routine=routine;(*next).context=context;(*next).control|=control;}
}

fn extension(device:*mut DEVICE_OBJECT)->Option<*mut DiskFdoExtension>{
 if device.is_null(){return None}
 let ext=unsafe{(*device).device_extension.cast::<DiskFdoExtension>()};
 if ext.is_null()||unsafe{(*ext).magic!=FDO_MAGIC}{None}else{Some(ext)}
}
fn current_stack(irp:*mut IRP)->*mut IO_STACK_LOCATION{
 if irp.is_null(){core::ptr::null_mut()}else{unsafe{(*irp).tail[8] as *mut IO_STACK_LOCATION}}
}

fn identity_string_length(value:&[u8])->usize{
 value.iter().position(|byte|*byte==0).unwrap_or(value.len())
}
unsafe fn write_storage_string(buffer:*mut u8,output_length:usize,offset:usize,value:&[u8])->Option<usize>{
 let length=identity_string_length(value);
 let end=offset.checked_add(length)?.checked_add(1)?;
 if end>output_length{return None}
 unsafe{core::ptr::copy_nonoverlapping(value.as_ptr(),buffer.add(offset),length);buffer.add(offset+length).write(0);}
 Some(end)
}
unsafe fn complete(irp:*mut IRP,status:i32,information:usize)->i32{
 unsafe{(*irp).io_status.status.status=status;(*irp).io_status.information=information;IoCompleteRequest(irp,0);}
 status
}

unsafe extern "win64" fn remove_complete(_lower:*mut DEVICE_OBJECT,irp:*mut IRP,context:PVOID)->i32{
 let fdo=context.cast::<DEVICE_OBJECT>();
 if fdo.is_null()||irp.is_null(){return STATUS_SUCCESS}
 if let Some(ext)=extension(fdo){
  let lower=unsafe{(*ext).lower};
  unsafe{(*ext).magic=0;}
  if !lower.is_null(){unsafe{IoDetachDevice(lower)}}
  unsafe{IoDeleteDevice(fdo)}
 }
 STATUS_SUCCESS
}

unsafe extern "win64" fn dispatch(device:*mut DEVICE_OBJECT,irp:*mut IRP)->i32{
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let Some(ext)=extension(device) else{return STATUS_DELETE_PENDING};
 let lower=unsafe{(*ext).lower};
 if lower.is_null(){return STATUS_DEVICE_NOT_READY}
 let stack=current_stack(irp);
 if stack.is_null(){return STATUS_INVALID_PARAMETER}
 let major=unsafe{(*stack).major_function as usize};
 if major==IRP_MJ_DEVICE_CONTROL{
  let code=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<u32>())};
  let output_length=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
  let buffer=unsafe{(*irp).associated_irp[0] as *mut u8};
  let descriptor=unsafe{(*ext).descriptor};
  match code{
   IOCTL_DISK_GET_LENGTH_INFO=>{
    if buffer.is_null()||output_length<8{return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}}
    let Ok(capacity)=descriptor.geometry.capacity_bytes() else{return unsafe{complete(irp,STATUS_DEVICE_DATA_ERROR,0)}};
    unsafe{core::ptr::write_unaligned(buffer.cast::<i64>(),capacity.min(i64::MAX as u64) as i64);}
    return unsafe{complete(irp,STATUS_SUCCESS,8)}
   }
   IOCTL_DISK_GET_DRIVE_GEOMETRY=>{
    if buffer.is_null()||output_length<core::mem::size_of::<DiskGeometryInformation>() as u32{return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}}
    let sectors_per_track=63u32;let tracks=255u32;
    let cylinders=descriptor.geometry.sector_count/(sectors_per_track as u64*tracks as u64);
    let geometry=DiskGeometryInformation{cylinders:cylinders.min(i64::MAX as u64) as i64,media_type:12,
     tracks_per_cylinder:tracks,sectors_per_track,bytes_per_sector:descriptor.geometry.logical_sector_size};
    unsafe{core::ptr::write_unaligned(buffer.cast::<DiskGeometryInformation>(),geometry);}
    return unsafe{complete(irp,STATUS_SUCCESS,core::mem::size_of::<DiskGeometryInformation>())}
   }
   IOCTL_STORAGE_GET_DEVICE_NUMBER=>{
    if buffer.is_null()||output_length<core::mem::size_of::<StorageDeviceNumber>() as u32{return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}}
    let number=StorageDeviceNumber{device_type:FILE_DEVICE_DISK,device_number:descriptor.identity.stable_id as u32,partition_number:u32::MAX};
    unsafe{core::ptr::write_unaligned(buffer.cast::<StorageDeviceNumber>(),number);}
    return unsafe{complete(irp,STATUS_SUCCESS,core::mem::size_of::<StorageDeviceNumber>())}
   }
   IOCTL_STORAGE_QUERY_PROPERTY=>{
    let input_length=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(4).cast::<u32>())};
    if buffer.is_null()||input_length<8||output_length<core::mem::size_of::<StorageDeviceDescriptor>() as u32{return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}}
    let property=unsafe{core::ptr::read_unaligned(buffer.cast::<u32>())};
    let query_type=unsafe{core::ptr::read_unaligned(buffer.add(4).cast::<u32>())};
    if property!=0||query_type!=0{return unsafe{complete(irp,STATUS_INVALID_DEVICE_REQUEST,0)}}
    let header_size=core::mem::size_of::<StorageDeviceDescriptor>();
    unsafe{core::ptr::write_bytes(buffer,0,output_length as usize);}
    let vendor=b"ATA";
    let mut cursor=header_size;
    let vendor_offset=cursor;
    cursor=match unsafe{write_storage_string(buffer,output_length as usize,cursor,vendor)}{Some(v)=>v,None=>return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}};
    let product_offset=cursor;
    cursor=match unsafe{write_storage_string(buffer,output_length as usize,cursor,&descriptor.identity.model)}{Some(v)=>v,None=>return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}};
    let revision_offset=cursor;
    cursor=match unsafe{write_storage_string(buffer,output_length as usize,cursor,&descriptor.identity.firmware)}{Some(v)=>v,None=>return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}};
    let serial_offset=cursor;
    cursor=match unsafe{write_storage_string(buffer,output_length as usize,cursor,&descriptor.identity.serial)}{Some(v)=>v,None=>return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}};
    let header=StorageDeviceDescriptor{version:header_size as u32,size:cursor as u32,device_type:0,device_type_modifier:0,
     removable_media:descriptor.removable as u8,command_queueing:0,
     vendor_id_offset:vendor_offset as u32,product_id_offset:product_offset as u32,
     product_revision_offset:revision_offset as u32,serial_number_offset:serial_offset as u32,
     bus_type:3,raw_properties_length:0};
    unsafe{core::ptr::write_unaligned(buffer.cast::<StorageDeviceDescriptor>(),header);}
    return unsafe{complete(irp,STATUS_SUCCESS,cursor)}
   }
   IOCTL_COGNACOS_DISK_QUERY_IDENTITY=>{
    if buffer.is_null()||output_length<core::mem::size_of::<DiskIdentityInformation>() as u32{return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}}
    let Ok(capacity)=descriptor.geometry.capacity_bytes() else{return unsafe{complete(irp,STATUS_DEVICE_DATA_ERROR,0)}};
    let info=DiskIdentityInformation{stable_id:descriptor.identity.stable_id,
     sector_count:descriptor.geometry.sector_count,capacity_bytes:capacity,
     logical_sector_size:descriptor.geometry.logical_sector_size,
     physical_sector_size:descriptor.geometry.physical_sector_size,
     removable:descriptor.removable as u8,supports_flush:descriptor.supports_flush as u8,
     reserved:[0;6],identity:descriptor.identity};
    unsafe{core::ptr::write_unaligned(buffer.cast::<DiskIdentityInformation>(),info);}
    return unsafe{complete(irp,STATUS_SUCCESS,core::mem::size_of::<DiskIdentityInformation>())}
   }
   _=>{}
  }
 }
 if major==IRP_MJ_PNP&&unsafe{(*stack).minor_function}==IRP_MN_REMOVE_DEVICE{
  unsafe{set_completion_routine(irp,Some(remove_complete),device.cast(),1,1,1);
   skip_current_irp_stack_location(irp);return IoCallDriver(lower,irp);}
 }
 unsafe{skip_current_irp_stack_location(irp);IoCallDriver(lower,irp)}
}

unsafe extern "win64" fn add_device(driver:*mut DRIVER_OBJECT,pdo:*mut DEVICE_OBJECT)->i32{
 if driver.is_null()||pdo.is_null(){return STATUS_INVALID_PARAMETER}
 let pdo_ext=unsafe{(*pdo).device_extension.cast::<AtaDiskPdoExtension>()};
 if pdo_ext.is_null()||unsafe{(*pdo_ext).magic!=PDO_MAGIC}{return STATUS_NOT_SUPPORTED}
 let descriptor=unsafe{(*pdo_ext).descriptor};
 if descriptor.geometry.validate().is_err()||descriptor.identity.stable_id==0{return STATUS_DEVICE_CONFIGURATION_ERROR}
 let mut fdo=core::ptr::null_mut();
 let status=unsafe{IoCreateDevice(driver.cast(),core::mem::size_of::<DiskFdoExtension>() as u32,
  core::ptr::null_mut(),FILE_DEVICE_DISK,0,0,&mut fdo)};
 if status!=STATUS_SUCCESS{return status}
 if fdo.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let lower=unsafe{IoAttachDeviceToDeviceStack(fdo,pdo)};
 if lower.is_null(){unsafe{IoDeleteDevice(fdo)};return STATUS_DEVICE_NOT_READY}
 let ext=unsafe{(*fdo).device_extension.cast::<DiskFdoExtension>()};
 unsafe{
  ext.write(DiskFdoExtension{magic:FDO_MAGIC,lower,physical:pdo,descriptor,slot:(*pdo_ext).slot,reserved:[0;7]});
  (*fdo).flags=(((*fdo).flags&!0x80)|((*lower).flags&(0x02|0x04)))|0x04;
 }
 STATUS_SUCCESS
}

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

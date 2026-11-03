#![no_std]
#![no_main]
//! Partition manager filter. It discovers MBR/GPT partitions below disk.sys,
//! publishes one PDO per partition, and translates partition-relative I/O to
//! the parent disk FDO. It never accesses controller registers.
use core::ffi::c_void;
use cognacos_nt_abi::{io::{DEVICE_OBJECT,DRIVER_OBJECT,IRP,IO_STACK_LOCATION},ntstatus::*,types::PVOID};
use cognacos_storage::{disk_class::{parse_mbr,parse_gpt,DiskDescriptor,Partition},Geometry};

const IRP_MJ_READ:usize=3;
const IRP_MJ_WRITE:usize=4;
const IRP_MJ_FLUSH_BUFFERS:usize=9;
const IRP_MJ_DEVICE_CONTROL:usize=0x0e;
const IRP_MJ_PNP:usize=0x1b;
const IRP_MJ_MAXIMUM_FUNCTION:usize=27;
const IRP_MN_START_DEVICE:u8=0;
const IRP_MN_REMOVE_DEVICE:u8=2;
const IRP_MN_QUERY_DEVICE_RELATIONS:u8=7;
const BUS_RELATIONS:u32=0;
const FILE_DEVICE_DISK:u32=7;
const IRP_BUFFERED_IO:u32=0x10;
const FDO_MAGIC:u64=0x434f_474e_5041_5254;
const DISK_PDO_MAGIC:u64=0x434f_474e_4449_534b;
const PARTITION_PDO_MAGIC:u64=0x434f_474e_5054_4e50;
const IOCTL_DISK_GET_LENGTH_INFO:u32=0x0007_405c;
const IOCTL_STORAGE_GET_DEVICE_NUMBER:u32=0x002d_1080;
const GPT_ENTRY_BUFFER_BYTES:usize=16*1024;

#[repr(C)]
struct AtaDiskPdoExtension{magic:u64,parent:*mut DEVICE_OBJECT,slot:u8,reserved:[u8;7],descriptor:DiskDescriptor}
#[repr(C)]
struct DeviceRelations{count:u32,objects:[*mut DEVICE_OBJECT;64]}
impl DeviceRelations{const EMPTY:Self=Self{count:0,objects:[core::ptr::null_mut();64]};}
#[repr(C)]
struct PartMgrFdoExtension{
 magic:u64,lower:*mut DEVICE_OBJECT,physical:*mut DEVICE_OBJECT,self_device:*mut DEVICE_OBJECT,driver:*mut DRIVER_OBJECT,
 descriptor:DiskDescriptor,relations:DeviceRelations,
}
#[repr(C)]
struct StorageDeviceNumber{device_type:u32,device_number:u32,partition_number:u32}
#[repr(C)]
struct PartitionPdoExtension{
 magic:u64,parent:*mut DEVICE_OBJECT,partition:Partition,geometry:Geometry,stable_id:u64,
}
const _:()=assert!(core::mem::size_of::<PartMgrFdoExtension>()<=1024);
const _:()=assert!(core::mem::size_of::<PartitionPdoExtension>()<=192);

unsafe extern "win64"{
 fn IoCreateDevice(driver:*mut DRIVER_OBJECT,extension_size:u32,name:*mut cognacos_nt_abi::object::UNICODE_STRING,device_type:u32,characteristics:u32,exclusive:u8,output:*mut *mut DEVICE_OBJECT)->i32;
 fn IoDeleteDevice(device:*mut DEVICE_OBJECT);
 fn IoAttachDeviceToDeviceStack(source:*mut DEVICE_OBJECT,target:*mut DEVICE_OBJECT)->*mut DEVICE_OBJECT;
 fn IoDetachDevice(target:*mut DEVICE_OBJECT);
 fn IoCallDriver(device:*mut DEVICE_OBJECT,irp:*mut IRP)->i32;
 fn IoSkipCurrentIrpStackLocation(irp:*mut IRP);
 fn IoCopyCurrentIrpStackLocationToNext(irp:*mut IRP);
 fn IoSetCompletionRoutine(irp:*mut IRP,routine:Option<unsafe extern "win64" fn(*mut DEVICE_OBJECT,*mut IRP,PVOID)->i32>,context:PVOID,success:u8,error:u8,cancel:u8);
 fn IoAllocateIrp(stack_size:i8,charge_quota:u8)->*mut IRP;
 fn IoGetNextIrpStackLocation(irp:*mut IRP)->*mut IO_STACK_LOCATION;
 fn IoFreeIrp(irp:*mut IRP);
 fn IoCompleteRequest(irp:*mut IRP,priority_boost:i8);
 fn IoPnPRegisterDeviceDriver(driver:*mut c_void,add_device:Option<unsafe extern "win64" fn(*mut c_void,*mut DEVICE_OBJECT)->i32>)->i32;
 fn IoPnPEnumerateBus(bus:*mut DEVICE_OBJECT)->i32;
 fn IoMountFAT32Volume(device:*mut DEVICE_OBJECT)->i32;
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
IMPORT IoSkipCurrentIrpStackLocation
IMPORT IoCopyCurrentIrpStackLocationToNext
IMPORT IoSetCompletionRoutine
IMPORT IoAllocateIrp
IMPORT IoGetNextIrpStackLocation
IMPORT IoFreeIrp
IMPORT IoCompleteRequest
IMPORT IoPnPRegisterDeviceDriver
IMPORT IoPnPEnumerateBus
IMPORT IoMountFAT32Volume
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
IAT IoAllocateIrp
IAT IoGetNextIrpStackLocation
IAT IoFreeIrp
IAT IoCompleteRequest
IAT IoPnPRegisterDeviceDriver
IAT IoPnPEnumerateBus
IAT IoMountFAT32Volume
"#);

fn fdo_extension(device:*mut DEVICE_OBJECT)->Option<*mut PartMgrFdoExtension>{
 if device.is_null(){return None}let ext=unsafe{(*device).device_extension.cast::<PartMgrFdoExtension>()};
 if ext.is_null()||unsafe{(*ext).magic!=FDO_MAGIC}{None}else{Some(ext)}
}
fn partition_extension(device:*mut DEVICE_OBJECT)->Option<*mut PartitionPdoExtension>{
 if device.is_null(){return None}let ext=unsafe{(*device).device_extension.cast::<PartitionPdoExtension>()};
 if ext.is_null()||unsafe{(*ext).magic!=PARTITION_PDO_MAGIC}{None}else{Some(ext)}
}
fn current_stack(irp:*mut IRP)->*mut IO_STACK_LOCATION{if irp.is_null(){core::ptr::null_mut()}else{unsafe{(*irp).tail[8] as *mut IO_STACK_LOCATION}}}
unsafe fn complete(irp:*mut IRP,status:i32,information:usize)->i32{unsafe{(*irp).io_status.status.status=status;(*irp).io_status.information=information;IoCompleteRequest(irp,0);}status}

unsafe fn read_disk(lower:*mut DEVICE_OBJECT,lba:u64,sectors:u32,buffer:&mut[u8])->i32{
 if lower.is_null()||sectors==0||buffer.len()!=sectors as usize*512{return STATUS_INVALID_PARAMETER}
 let offset=match lba.checked_mul(512){Some(v) if v<=i64::MAX as u64=>v as i64,_=>return STATUS_INVALID_PARAMETER};
 let length=buffer.len() as u32;
 let irp=unsafe{IoAllocateIrp((*lower).stack_size.max(1) as i8,0)};
 if irp.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let stack=unsafe{IoGetNextIrpStackLocation(irp)};
 if stack.is_null(){unsafe{IoFreeIrp(irp)};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=IRP_MJ_READ as u8;
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<u32>(),length);
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<i64>(),offset);
  (*irp).associated_irp[0]=buffer.as_mut_ptr() as usize;
  (*irp).flags|=IRP_BUFFERED_IO;
 }
 let status=unsafe{IoCallDriver(lower,irp)};
 if status==STATUS_PENDING{return STATUS_NOT_SUPPORTED}
 let final_status=unsafe{(*irp).io_status.status.status};
 unsafe{IoFreeIrp(irp)}
 if status<0{status}else{final_status}
}

fn partition_identity(disk_id:u64,p:Partition)->u64{
 let mut h=0xcbf2_9ce4_8422_2325u64;
 let mut feed=|v:u8|{h^=v as u64;h=h.wrapping_mul(0x0000_0100_0000_01b3);};
 for b in disk_id.to_le_bytes(){feed(b)}
 for b in p.start_lba.to_le_bytes(){feed(b)}
 for b in p.sector_count.to_le_bytes(){feed(b)}
 for b in p.type_guid{feed(b)}
 for b in p.unique_guid{feed(b)}
 if h==0{1}else{h}
}

unsafe fn scan_partitions(ext:*mut PartMgrFdoExtension)->i32{
 if unsafe{(*ext).relations.count}!=0{return STATUS_SUCCESS}
 let descriptor=unsafe{(*ext).descriptor};
 let lower=unsafe{(*ext).lower};
 let mut mbr=[0u8;512];
 let status=unsafe{read_disk(lower,0,1,&mut mbr)};
 if status!=STATUS_SUCCESS{return status}
 let protective=(0..4).any(|i|mbr[446+i*16+4]==0xee);
 let table=if protective{
  let mut header=[0u8;512];
  let status=unsafe{read_disk(lower,1,1,&mut header)};
  if status!=STATUS_SUCCESS{return status}
  let entries_lba=u64::from_le_bytes(header[72..80].try_into().unwrap_or([0;8]));
  let entry_count=u32::from_le_bytes(header[80..84].try_into().unwrap_or([0;4])) as usize;
  let entry_size=u32::from_le_bytes(header[84..88].try_into().unwrap_or([0;4])) as usize;
  let required=match entry_count.checked_mul(entry_size){Some(v) if entry_count>0&&entry_count<=128&&entry_size>=128&&v<=GPT_ENTRY_BUFFER_BYTES=>v,_=>return STATUS_DEVICE_DATA_ERROR};
  let sectors=((required+511)/512) as u32;
  if descriptor.geometry.validate_io(entries_lba,sectors).is_err(){return STATUS_DEVICE_DATA_ERROR}
  let mut entries=[0u8;GPT_ENTRY_BUFFER_BYTES];
  let status=unsafe{read_disk(lower,entries_lba,sectors,&mut entries[..sectors as usize*512])};
  if status!=STATUS_SUCCESS{return status}
  match parse_gpt(&header,&entries[..required],descriptor.geometry){Ok(v)=>v,Err(_)=>return STATUS_SUCCESS}
 }else{
  match parse_mbr(&mbr,descriptor.geometry){Ok(v)=>v,Err(_)=>return STATUS_SUCCESS}
 };
 unsafe{(*ext).relations=DeviceRelations::EMPTY;}
 for partition in table.entries[..table.count].iter().copied(){
  if unsafe{(*ext).relations.count}>=64{break}
  let mut child=core::ptr::null_mut();
  let status=unsafe{IoCreateDevice((*ext).driver,core::mem::size_of::<PartitionPdoExtension>() as u32,core::ptr::null_mut(),FILE_DEVICE_DISK,0,0,&mut child)};
  if status!=STATUS_SUCCESS||child.is_null(){continue}
  let child_ext=unsafe{(*child).device_extension.cast::<PartitionPdoExtension>()};
  unsafe{
   child_ext.write(PartitionPdoExtension{magic:PARTITION_PDO_MAGIC,parent:unsafe{(*ext).self_device},partition,geometry:descriptor.geometry,stable_id:partition_identity(descriptor.identity.stable_id,partition)});
   (*child).flags|=0x0000_1004;
   (*child).stack_size=(*ext).lower.stack_size.saturating_add(1);
  }
  let relations=unsafe{&mut (*ext).relations};
  let index=relations.count as usize;relations.objects[index]=child;relations.count+=1;
 }
 STATUS_SUCCESS
}

unsafe extern "win64" fn pnp_complete(_lower:*mut DEVICE_OBJECT,irp:*mut IRP,context:PVOID)->i32{
 let fdo=context.cast::<DEVICE_OBJECT>();if fdo.is_null()||irp.is_null(){return STATUS_SUCCESS}
 if let Some(ext)=fdo_extension(fdo){
  let stack=current_stack(irp);let minor=if stack.is_null(){0xff}else{unsafe{(*stack).minor_function}};
  let status=unsafe{(*irp).io_status.status.status};
  if minor==IRP_MN_START_DEVICE&&status==STATUS_SUCCESS{
   let _=unsafe{scan_partitions(ext)};
   let physical=unsafe{(*ext).physical};if !physical.is_null(){let _=unsafe{IoPnPEnumerateBus(physical)};}
   // Prefer later data partitions over the EFI system partition. Only one
   // FAT32 volume is currently mounted by the kernel's single-volume VPB path.
   let count=unsafe{(*ext).relations.count as usize};
   for index in (0..count).rev(){
    let child=unsafe{(*ext).relations.objects[index]};
    if !child.is_null()&&unsafe{IoMountFAT32Volume(child)}==STATUS_SUCCESS{break}
   }
  }
  if minor==IRP_MN_REMOVE_DEVICE{
   let lower=unsafe{(*ext).lower};unsafe{(*ext).magic=0};
   if !lower.is_null(){unsafe{IoDetachDevice(lower)}}
   unsafe{IoDeleteDevice(fdo)}
  }
 }
 STATUS_SUCCESS
}

unsafe fn dispatch_partition(device:*mut DEVICE_OBJECT,ext:*mut PartitionPdoExtension,irp:*mut IRP)->i32{
 let stack=current_stack(irp);if stack.is_null(){return STATUS_INVALID_PARAMETER}
 let major=unsafe{(*stack).major_function as usize};
 if major==0||major==2{return unsafe{complete(irp,STATUS_SUCCESS,0)}}
 if major==IRP_MJ_PNP{
  if unsafe{(*stack).minor_function}==IRP_MN_REMOVE_DEVICE{
   unsafe{(*ext).magic=0;IoDeleteDevice(device);}
  }
  return unsafe{complete(irp,STATUS_SUCCESS,0)}
 }
 let parent=unsafe{(*ext).parent};
 let Some(parent_ext)=fdo_extension(parent) else{return unsafe{complete(irp,STATUS_DEVICE_NOT_READY,0)}};
 let lower=unsafe{(*parent_ext).lower};
 if major==IRP_MJ_DEVICE_CONTROL{
  let code=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<u32>())};
  if code==IOCTL_DISK_GET_LENGTH_INFO{
   let output=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
   let buffer=unsafe{(*irp).associated_irp[0] as *mut u8};
   let bytes=match unsafe{(*ext).partition.sector_count.checked_mul((*ext).geometry.logical_sector_size as u64)}{Some(v) if v<=i64::MAX as u64=>v,_=>return unsafe{complete(irp,STATUS_DEVICE_DATA_ERROR,0)}};
   if buffer.is_null()||output<8{return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}}
   unsafe{core::ptr::write_unaligned(buffer.cast::<i64>(),bytes as i64)}
   return unsafe{complete(irp,STATUS_SUCCESS,8)}
  }
  if code==IOCTL_STORAGE_GET_DEVICE_NUMBER{
   let output=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
   let buffer=unsafe{(*irp).associated_irp[0] as *mut u8};
   if buffer.is_null()||output<core::mem::size_of::<StorageDeviceNumber>() as u32{return unsafe{complete(irp,STATUS_BUFFER_TOO_SMALL,0)}}
   let number=StorageDeviceNumber{device_type:FILE_DEVICE_DISK,device_number:unsafe{(*parent_ext).descriptor.identity.stable_id as u32},partition_number:unsafe{(*ext).partition.number}};
   unsafe{core::ptr::write_unaligned(buffer.cast::<StorageDeviceNumber>(),number);}
   return unsafe{complete(irp,STATUS_SUCCESS,core::mem::size_of::<StorageDeviceNumber>())}
  }
 }
 if major==IRP_MJ_READ||major==IRP_MJ_WRITE{
  let length=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
  let offset=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<i64>())};
  let sector_size=unsafe{(*ext).geometry.logical_sector_size};
  let partition=unsafe{(*ext).partition};
  let buffer=unsafe{(*irp).associated_irp[0] as *mut u8};
  let flags=unsafe{(*irp).flags};
  if flags&IRP_BUFFERED_IO==0||buffer.is_null()||offset<0||sector_size==0||offset as u64%sector_size as u64!=0||length==0||length%sector_size!=0
   ||offset as u64>partition.sector_count.saturating_mul(sector_size as u64)
   ||length as u64>partition.sector_count.saturating_mul(sector_size as u64).saturating_sub(offset as u64){
   return unsafe{complete(irp,STATUS_INVALID_PARAMETER,0)}
  }
  let base=match partition.start_lba.checked_mul(sector_size as u64){Some(v)=>v,_=>return unsafe{complete(irp,STATUS_INVALID_PARAMETER,0)}};
  let absolute=match base.checked_add(offset as u64){Some(v) if v<=i64::MAX as u64=>v as i64,_=>return unsafe{complete(irp,STATUS_INVALID_PARAMETER,0)}};
  unsafe{core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<i64>(),absolute);}
 }
 unsafe{IoSkipCurrentIrpStackLocation(irp);IoCallDriver(lower,irp)}
}

unsafe extern "win64" fn dispatch(device:*mut DEVICE_OBJECT,irp:*mut IRP)->i32{
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 if let Some(part)=partition_extension(device){return unsafe{dispatch_partition(device,part,irp)}}
 let Some(ext)=fdo_extension(device) else{return STATUS_DELETE_PENDING};
 let stack=current_stack(irp);if stack.is_null(){return STATUS_INVALID_PARAMETER}
 if unsafe{(*stack).major_function as usize}==IRP_MJ_PNP{
  let minor=unsafe{(*stack).minor_function};
  if minor==IRP_MN_QUERY_DEVICE_RELATIONS{
   let kind=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
   if kind==BUS_RELATIONS{
    let relations=unsafe{core::ptr::addr_of!((*ext).relations)};
    unsafe{(*irp).io_status.status.status=STATUS_SUCCESS;(*irp).io_status.information=relations as usize;IoCompleteRequest(irp,0);}
    return STATUS_SUCCESS
   }
  }
  if minor==IRP_MN_REMOVE_DEVICE{
   let physical=unsafe{(*ext).physical};
   unsafe{(*ext).relations=DeviceRelations::EMPTY;}
   if !physical.is_null(){let _=unsafe{IoPnPEnumerateBus(physical)};}
   let lower=unsafe{(*ext).lower};
   unsafe{IoCopyCurrentIrpStackLocationToNext(irp);IoSetCompletionRoutine(irp,Some(pnp_complete),device.cast(),1,1,1);return IoCallDriver(lower,irp);}
  }
  if minor==IRP_MN_START_DEVICE{
   let lower=unsafe{(*ext).lower};
   unsafe{IoCopyCurrentIrpStackLocationToNext(irp);IoSetCompletionRoutine(irp,Some(pnp_complete),device.cast(),1,1,1);return IoCallDriver(lower,irp);}
  }
  let lower=unsafe{(*ext).lower};unsafe{IoSkipCurrentIrpStackLocation(irp);return IoCallDriver(lower,irp);}
 }
 let lower=unsafe{(*ext).lower};unsafe{IoSkipCurrentIrpStackLocation(irp);IoCallDriver(lower,irp)}
}

unsafe extern "win64" fn add_device(driver:*mut c_void,pdo:*mut DEVICE_OBJECT)->i32{
 if driver.is_null()||pdo.is_null(){return STATUS_INVALID_PARAMETER}
 let pdo_ext=unsafe{(*pdo).device_extension.cast::<AtaDiskPdoExtension>()};
 if pdo_ext.is_null()||unsafe{(*pdo_ext).magic!=DISK_PDO_MAGIC}{return STATUS_NOT_SUPPORTED}
 let descriptor=unsafe{(*pdo_ext).descriptor};
 if descriptor.geometry.validate().is_err()||descriptor.identity.stable_id==0{return STATUS_DEVICE_CONFIGURATION_ERROR}
 let mut fdo=core::ptr::null_mut();
 let status=unsafe{IoCreateDevice(driver.cast(),core::mem::size_of::<PartMgrFdoExtension>() as u32,core::ptr::null_mut(),FILE_DEVICE_DISK,0,0,&mut fdo)};
 if status!=STATUS_SUCCESS{return status}if fdo.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let lower=unsafe{IoAttachDeviceToDeviceStack(fdo,pdo)};
 if lower.is_null(){unsafe{IoDeleteDevice(fdo)};return STATUS_DEVICE_NOT_READY}
 let ext=unsafe{(*fdo).device_extension.cast::<PartMgrFdoExtension>()};
 unsafe{
  ext.write(PartMgrFdoExtension{magic:FDO_MAGIC,lower,physical:pdo,self_device:fdo,driver:driver.cast(),descriptor,relations:DeviceRelations::EMPTY});
  (*fdo).flags=(((*fdo).flags&!0x80)|((*lower).flags&(0x02|0x04)))|0x04;
 }
 STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver:*mut DRIVER_OBJECT,_context:PVOID)->i32{
 if driver.is_null(){return STATUS_INVALID_PARAMETER}
 unsafe{for index in 0..=IRP_MJ_MAXIMUM_FUNCTION{(*driver).major_function[index]=Some(dispatch)}(*driver).driver_unload=None;}
 unsafe{IoPnPRegisterDeviceDriver(driver.cast(),Some(add_device))}
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>)->!{loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}}

//! Generic synchronous block request helpers for filesystem and volume drivers.
//! This layer owns IRP construction/submission only; it knows no filesystem.
use crate::nt_abi::io::IRP as AbiIrp;
use crate::wdm::{self,DeviceObject};
const STATUS_PENDING:i32=0x0000_0103;
const IRP_MJ_READ:u8=3; const IRP_MJ_WRITE:u8=4; const IRP_MJ_FLUSH_BUFFERS:u8=9; const IRP_MJ_DEVICE_CONTROL:u8=0x0e;
const IRP_BUFFERED_IO:u32=0x10;
/// Submit one bounded synchronous request through the normal device stack.
pub fn submit(device:*mut DeviceObject,major:u8,offset:u64,buffer:*mut u8,length:u32,control_code:u32)->Result<usize,i32>{
 if device.is_null()||unsafe{crate::ntoskrnl::ke::runtime::ke_get_current_irql()}!=0||offset>i64::MAX as u64{return Err(0xC000_0185u32 as i32)}
 if major!=IRP_MJ_FLUSH_BUFFERS&&(length==0||buffer.is_null()){return Err(0xC000_000Du32 as i32)}
 let stack_size=unsafe{(*device).stack_size.max(1)};
 let irp=unsafe{wdm::io_allocate_irp(stack_size as i8,0)}; if irp.is_null(){return Err(0xC000_009Au32 as i32)}
 let stack=unsafe{wdm::io_get_next_irp_stack_location(irp)}; if stack.is_null(){unsafe{wdm::io_free_irp(irp)};return Err(0xC000_009Au32 as i32)}
 unsafe{(*stack).major_function=major;(*stack).parameters.fill(0);
 if major==IRP_MJ_READ||major==IRP_MJ_WRITE{core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<u32>(),length);core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<i64>(),offset as i64);}
 else if major==IRP_MJ_DEVICE_CONTROL{core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<u32>(),length);core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<u32>(),control_code);}
 let abi=irp.cast::<AbiIrp>();(*abi).flags|=IRP_BUFFERED_IO;(*abi).associated_irp[0]=buffer as usize;}
 let status=unsafe{wdm::io_call_driver(device,irp)};
 if status==STATUS_PENDING{return Err(0xC000_0185u32 as i32)}
 let (final_status,information)=unsafe{let abi=irp.cast::<AbiIrp>();((*abi).io_status.status.status,(*abi).io_status.information)};
 unsafe{wdm::io_free_irp(irp)};
 if status<0||final_status<0{return Err(0xC000_0185u32 as i32)}
 if (major==IRP_MJ_READ||major==IRP_MJ_WRITE)&&information!=length as usize{return Err(0xC000_0185u32 as i32)}
 Ok(information)
}
pub fn reference_device(device:*mut DeviceObject)->Result<*mut DeviceObject,i32>{if device.is_null(){return Err(0xC000_0185u32 as i32)}let p=unsafe{wdm::io_reference_device_object(device)};if p.is_null(){Err(0xC000_0185u32 as i32)}else{Ok(p)}}
pub unsafe fn dereference_device(device:*mut DeviceObject){if !device.is_null(){unsafe{wdm::io_dereference_device_object(device)}}}

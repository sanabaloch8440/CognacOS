//! Kernel-side block-device adapter for the partition PDO published by partmgr.sys.
//!
//! All physical transport remains in the loadable disk/port/miniport stack.
//! The FAT32 filesystem reaches storage only through synchronous buffered IRPs;
//! the current ATA PIO miniport completes synchronously. Pending requests fail
//! closed until the kernel PASSIVE_LEVEL work-item/rundown path is enabled.
use cognacos_fat32::DiskError;
use crate::nt_abi::io::IRP as AbiIrp;
use crate::wdm::{self, DeviceObject, Irp, IoStackLocation};

const STATUS_SUCCESS:i32=0;
const STATUS_PENDING:i32=0x0000_0103;
const IRP_MJ_READ:u8=3;
const IRP_MJ_WRITE:u8=4;
const IRP_MJ_FLUSH_BUFFERS:u8=9;
const IRP_MJ_DEVICE_CONTROL:u8=0x0e;
const IRP_BUFFERED_IO:u32=0x10;
const IOCTL_DISK_GET_LENGTH_INFO:u32=0x0007_405c;
const SECTOR_SIZE:u32=512;

pub struct IoManagerBlockDevice {
    device:*mut DeviceObject,
    capacity:u64,
}
unsafe impl Send for IoManagerBlockDevice {}

impl IoManagerBlockDevice {
    pub fn sector_count(&self)->u64{self.capacity/SECTOR_SIZE as u64}
    pub fn capacity_bytes(&self)->u64{self.capacity}

    fn submit(&self,major:u8,offset:u64,buffer:*mut u8,length:u32,control_code:u32)->Result<usize,DiskError>{
        if self.device.is_null(){return Err(DiskError::DeviceFailure)}
        if offset>i64::MAX as u64{return Err(DiskError::OutOfRange)}
        let end=offset.checked_add(length as u64).ok_or(DiskError::Overflow)?;
        if end>self.capacity{return Err(DiskError::OutOfRange)}
        if major!=IRP_MJ_FLUSH_BUFFERS&&(length==0||buffer.is_null()){return Err(DiskError::InvalidGeometry)}
        let stack_size=unsafe{(*self.device).stack_size.max(1)};
        let irp=unsafe{wdm::io_allocate_irp(stack_size,0)};
        if irp.is_null(){return Err(DiskError::DeviceFailure)}
        let stack=unsafe{wdm::io_get_next_irp_stack_location(irp)};
        if stack.is_null(){unsafe{wdm::io_free_irp(irp)};return Err(DiskError::DeviceFailure)}
        unsafe{
            (*stack).major_function=major;
            (*stack).parameters.fill(0);
            if major==IRP_MJ_READ||major==IRP_MJ_WRITE{
                core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<u32>(),length);
                core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<i64>(),offset as i64);
            }else if major==IRP_MJ_DEVICE_CONTROL{
                core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<u32>(),length);
                core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<u32>(),control_code);
            }
            let abi=irp.cast::<AbiIrp>();
            (*abi).flags|=IRP_BUFFERED_IO;
            (*abi).associated_irp[0]=buffer as usize;
        }
        let status=unsafe{wdm::io_call_driver(self.device,irp)};
        if status==STATUS_PENDING{
            // The PIO contract is synchronous. Do not free an IRP that a
            // future asynchronous lower driver may still own.
            return Err(DiskError::DeviceFailure)
        }
        let (final_status,information)=unsafe{
            let abi=irp.cast::<AbiIrp>();
            ( (*abi).io_status.status.status, (*abi).io_status.information )
        };
        unsafe{wdm::io_free_irp(irp)}
        if status<0{return Err(DiskError::DeviceFailure)}
        if final_status<0{return Err(DiskError::DeviceFailure)}
        if major==IRP_MJ_READ||major==IRP_MJ_WRITE{
            if information!=length as usize{return Err(DiskError::DeviceFailure)}
        }
        Ok(information)
    }
}

impl Drop for IoManagerBlockDevice{
    fn drop(&mut self){
        if !self.device.is_null(){unsafe{wdm::io_dereference_device_object(self.device)}}
    }
}

impl cognacos_fat32::BlockDevice for IoManagerBlockDevice{
    fn sector_size(&self)->u32{SECTOR_SIZE}
    fn sector_count(&self)->u64{self.capacity/SECTOR_SIZE as u64}
    fn read_sectors(&mut self,lba:u64,output:&mut[u8])->Result<(),DiskError>{
        if output.is_empty()||output.len()%SECTOR_SIZE as usize!=0{return Err(DiskError::InvalidGeometry)}
        let offset=lba.checked_mul(SECTOR_SIZE as u64).ok_or(DiskError::Overflow)?;
        let length=u32::try_from(output.len()).map_err(|_|DiskError::OutOfRange)?;
        self.submit(IRP_MJ_READ,offset,output.as_mut_ptr(),length,0).map(|_|())
    }
    fn write_sectors(&mut self,lba:u64,input:&[u8])->Result<(),DiskError>{
        if input.is_empty()||input.len()%SECTOR_SIZE as usize!=0{return Err(DiskError::InvalidGeometry)}
        let offset=lba.checked_mul(SECTOR_SIZE as u64).ok_or(DiskError::Overflow)?;
        let length=u32::try_from(input.len()).map_err(|_|DiskError::OutOfRange)?;
        // The buffered I/O manager owns the system buffer for the duration of
        // this synchronous call; the input is copied into a mutable kernel buffer.
        let mut buffer=[0u8;4096];
        if input.len()>buffer.len(){return Err(DiskError::OutOfRange)}
        buffer[..input.len()].copy_from_slice(input);
        self.submit(IRP_MJ_WRITE,offset,buffer.as_mut_ptr(),length,0).map(|_|())
    }
    fn flush(&mut self)->Result<(),DiskError>{
        self.submit(IRP_MJ_FLUSH_BUFFERS,0,core::ptr::null_mut(),0,0).map(|_|())
    }
}

/// Acquire and validate a partition PDO as the filesystem's block provider.
/// partmgr.sys implements IOCTL_DISK_GET_LENGTH_INFO on every partition PDO.
pub fn fat32_block_device(device:*mut DeviceObject)->Result<IoManagerBlockDevice,DiskError>{
    if device.is_null(){return Err(DiskError::DeviceFailure)}
    let referenced=unsafe{wdm::io_reference_device_object(device)};
    if referenced.is_null(){return Err(DiskError::DeviceFailure)}
    let temporary=IoManagerBlockDevice{device:referenced,capacity:u64::MAX};
    let mut length=0i64;
    let result=temporary.submit(IRP_MJ_DEVICE_CONTROL,0,(&mut length as *mut i64).cast(),8,IOCTL_DISK_GET_LENGTH_INFO);
    let capacity=match result{
        Ok(8) if length>0&&(length as u64)%SECTOR_SIZE as u64==0=>length as u64,
        _=>return Err(DiskError::InvalidGeometry),
    };
    core::mem::forget(temporary);
    Ok(IoManagerBlockDevice{device:referenced,capacity})
}

pub fn debug_trace_store_read(_offset:u64,_output:&mut[u8])->bool{false}
pub fn debug_trace_store_write(_offset:u64,_input:&[u8])->bool{false}

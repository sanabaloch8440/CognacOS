//! FAT32-specific BlockDevice adapter over generic I/O Manager block IRPs.
use crate::wdm::DeviceObject;
use cognacos_fat32::DiskError;
const SECTOR_SIZE:u32=512; const IOCTL_DISK_GET_LENGTH_INFO:u32=0x0007_405c;
pub struct Fat32BlockDevice{device:*mut DeviceObject,capacity:u64}
unsafe impl Send for Fat32BlockDevice{}
impl Fat32BlockDevice{pub fn sector_count(&self)->u64{self.capacity/SECTOR_SIZE as u64}pub fn capacity_bytes(&self)->u64{self.capacity}}
impl Drop for Fat32BlockDevice{fn drop(&mut self){unsafe{crate::io::block_io::dereference_device(self.device)}}}
impl cognacos_fat32::BlockDevice for Fat32BlockDevice{
 fn sector_size(&self)->u32{SECTOR_SIZE} fn sector_count(&self)->u64{self.capacity/SECTOR_SIZE as u64}
 fn read_sectors(&mut self,lba:u64,out:&mut[u8])->Result<(),DiskError>{if out.is_empty()||out.len()%SECTOR_SIZE as usize!=0{return Err(DiskError::InvalidGeometry)}let off=lba.checked_mul(SECTOR_SIZE as u64).ok_or(DiskError::Overflow)?;let len=u32::try_from(out.len()).map_err(|_|DiskError::OutOfRange)?;if off.checked_add(len as u64).filter(|e|*e<=self.capacity).is_none(){return Err(DiskError::OutOfRange)}crate::io::block_io::submit(self.device,3,off,out.as_mut_ptr(),len,0).map(|_|()).map_err(|_|DiskError::DeviceFailure)}
 fn write_sectors(&mut self,lba:u64,input:&[u8])->Result<(),DiskError>{if input.is_empty()||input.len()%SECTOR_SIZE as usize!=0{return Err(DiskError::InvalidGeometry)}let off=lba.checked_mul(SECTOR_SIZE as u64).ok_or(DiskError::Overflow)?;let len=u32::try_from(input.len()).map_err(|_|DiskError::OutOfRange)?;if off.checked_add(len as u64).filter(|e|*e<=self.capacity).is_none(){return Err(DiskError::OutOfRange)}crate::io::block_io::submit(self.device,4,off,input.as_ptr() as *mut u8,len,0).map(|_|()).map_err(|_|DiskError::DeviceFailure)}
 fn flush(&mut self)->Result<(),DiskError>{crate::io::block_io::submit(self.device,9,0,core::ptr::null_mut(),0,0).map(|_|()).map_err(|_|DiskError::DeviceFailure)}
}
pub fn acquire(device:*mut DeviceObject)->Result<Fat32BlockDevice,DiskError>{let referenced=crate::io::block_io::reference_device(device).map_err(|_|DiskError::DeviceFailure)?;let mut length=0i64;let query=crate::io::block_io::submit(referenced,0x0e,0,(&mut length as *mut i64).cast(),8,IOCTL_DISK_GET_LENGTH_INFO);match query{Ok(8)if length>0&&(length as u64)%SECTOR_SIZE as u64==0=>Ok(Fat32BlockDevice{device:referenced,capacity:length as u64}),_=>{unsafe{crate::io::block_io::dereference_device(referenced)};Err(DiskError::InvalidGeometry)}}}

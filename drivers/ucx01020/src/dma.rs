//! Device-scoped DMA leases for UCX request and endpoint-ring allocations.
#![no_std]
use cognacos_hal_contract::{DeviceServiceTable,HAL_DMA_COHERENT,HAL_DMA_CONTIGUOUS,HAL_DMA_ZEROED};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DmaError{InvalidService,InvalidDevice,InvalidLength,InvalidAlignment,AllocationFailed,ValidationFailed,FreeFailed}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct DmaBuffer{pub cpu_address:u64,pub device_address:u64,pub length:u64,pub flags:u32}
pub struct DeviceDma{services:*const DeviceServiceTable,device_object:u64}
impl DeviceDma{
 pub const fn new(services:*const DeviceServiceTable,device_object:u64)->Self{Self{services,device_object}}
 fn table(&self)->Result<&DeviceServiceTable,DmaError>{if self.services.is_null(){return Err(DmaError::InvalidService)}let t=unsafe{&*self.services};if !t.compatible()||!t.has_dma(){return Err(DmaError::InvalidService)}if self.device_object==0{return Err(DmaError::InvalidDevice)}Ok(t)}
 /// Allocate a buffer in the DMA domain belonging to this PCI PDO identity.
 /// The returned device address is not interchangeable with a CPU pointer.
 pub fn allocate(&self,length:u64,alignment:u64,maximum_address:u64,flags:u32)->Result<DmaBuffer,DmaError>{
  if length==0||length>u32::MAX as u64{return Err(DmaError::InvalidLength)}
  if alignment==0||!alignment.is_power_of_two(){return Err(DmaError::InvalidAlignment)}
  let t=self.table()?;let f=t.allocate_dma.ok_or(DmaError::InvalidService)?;
  let mut cpu=0u64;let mut dma=0u64;
  let status=unsafe{f(self.device_object,length,alignment,maximum_address,flags,&mut cpu,&mut dma)};
  if status.code!=0{return Err(DmaError::AllocationFailed)}
  let valid=cpu!=0&&dma!=0&&cpu&(alignment-1)==0&&dma&(alignment-1)==0
   &&dma.checked_add(length-1).map(|end|end<=maximum_address).unwrap_or(false);
  if !valid{
   if let Some(free)=t.free_dma{unsafe{let _=free(self.device_object,cpu,dma,length);}}
   return Err(DmaError::ValidationFailed)
  }
  let validate=t.validate_dma.ok_or(DmaError::InvalidService)?;
  if unsafe{validate(self.device_object,cpu,dma,length)}.code!=0{
   if let Some(free)=t.free_dma{unsafe{let _=free(self.device_object,cpu,dma,length);}}
   return Err(DmaError::ValidationFailed)
  }
  Ok(DmaBuffer{cpu_address:cpu,device_address:dma,length,flags})
 }
 pub fn validate(&self,b:DmaBuffer)->Result<(),DmaError>{
  let t=self.table()?;let f=t.validate_dma.ok_or(DmaError::InvalidService)?;
  if b.length==0||b.cpu_address==0||b.device_address==0{return Err(DmaError::ValidationFailed)}
  if unsafe{f(self.device_object,b.cpu_address,b.device_address,b.length)}.code==0{Ok(())}else{Err(DmaError::ValidationFailed)}
 }
 pub fn free(&self,b:DmaBuffer)->Result<(),DmaError>{
  self.validate(b)?;let t=self.table()?;let f=t.free_dma.ok_or(DmaError::InvalidService)?;
  if unsafe{f(self.device_object,b.cpu_address,b.device_address,b.length)}.code==0{Ok(())}else{Err(DmaError::FreeFailed)}
 }
}
pub const UCX_DMA_COMMON_BUFFER_FLAGS:u32=HAL_DMA_CONTIGUOUS|HAL_DMA_COHERENT|HAL_DMA_ZEROED;

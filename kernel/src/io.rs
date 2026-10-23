//! Initial Windows-NT-inspired I/O manager contracts: device dispatch, bounded
//! IRPs, single-dispatch ownership, cancellation before dispatch, and completion.
//! This is not yet a full NT IRP stack or hardware DMA implementation.
use crate::hal::IrqSpinLock;
pub const MAX_DEVICES:usize=16;pub const MAX_IRPS:usize=64;pub const MAX_IRP_BYTES:usize=4096;
pub const IRP_MJ_CREATE:u8=0;pub const IRP_MJ_CLOSE:u8=2;pub const IRP_MJ_READ:u8=3;pub const IRP_MJ_WRITE:u8=4;pub const IRP_MJ_DEVICE_CONTROL:u8=14;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IoStatus{Free,Pending,Success,InvalidRequest,NoSuchDevice,BufferTooLarge,DeviceError,Cancelled}
#[derive(Clone,Copy,Debug)]
pub struct Irp{pub id:u64,pub major:u8,pub device_id:u32,pub user_buffer:u64,pub length:usize,pub information:usize,pub status:IoStatus,pub completed:bool,pub in_flight:bool}
impl Irp{const EMPTY:Self=Self{id:0,major:0,device_id:0,user_buffer:0,length:0,information:0,status:IoStatus::Free,completed:false,in_flight:false};}
pub type DispatchFn=fn(&mut Irp)->IoStatus;
#[derive(Clone,Copy)]
pub struct Device{pub id:u32,pub name:[u8;32],pub name_len:u8,pub dispatch:[Option<DispatchFn>;16],pub used:bool}
impl Device{const EMPTY:Self=Self{id:0,name:[0;32],name_len:0,dispatch:[None;16],used:false};}
struct State{devices:[Device;MAX_DEVICES],irps:[Irp;MAX_IRPS],next_device:u32,next_irp:u64}
impl State{const fn new()->Self{Self{devices:[Device::EMPTY;MAX_DEVICES],irps:[Irp::EMPTY;MAX_IRPS],next_device:1,next_irp:1}}}
static STATE:IrqSpinLock<State>=IrqSpinLock::new(State::new());
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IoError{InvalidName,NameTooLong,TooManyDevices,TooManyRequests,UnknownDevice,InvalidMajor,BufferTooLarge,UnknownIrp,AlreadyCompleted,AlreadyInFlight,NotCancellable}

pub fn register_device(name:&[u8],dispatch:[Option<DispatchFn>;16])->Result<u32,IoError>{
 if name.is_empty(){return Err(IoError::InvalidName);}if name.len()>32{return Err(IoError::NameTooLong);}
 let mut s=STATE.lock();
 if s.devices.iter().any(|d|d.used&&d.name_len as usize==name.len()&&d.name[..name.len()]==*name){return Err(IoError::InvalidName);}
 let index=s.devices.iter().position(|d|!d.used).ok_or(IoError::TooManyDevices)?;
 let id=s.next_device;s.next_device=s.next_device.wrapping_add(1).max(1);
 let mut device=Device::EMPTY;device.id=id;device.name[..name.len()].copy_from_slice(name);device.name_len=name.len() as u8;device.dispatch=dispatch;device.used=true;s.devices[index]=device;Ok(id)
}
pub fn submit(device_id:u32,major:u8,user_buffer:u64,length:usize)->Result<u64,IoError>{
 if major as usize>=16{return Err(IoError::InvalidMajor);}if length>MAX_IRP_BYTES{return Err(IoError::BufferTooLarge);}
 let mut s=STATE.lock();
 if !s.devices.iter().any(|d|d.used&&d.id==device_id){return Err(IoError::UnknownDevice);}
 let index=s.irps.iter().position(|i|i.status==IoStatus::Free).ok_or(IoError::TooManyRequests)?;
 let id=s.next_irp;s.next_irp=s.next_irp.wrapping_add(1).max(1);
 s.irps[index]=Irp{id,major,device_id,user_buffer,length,information:0,status:IoStatus::Pending,completed:false,in_flight:false};Ok(id)
}
/// Cancel only before dispatch begins. Once a callback owns the request, callers
/// must use a future driver-specific cancellation protocol rather than race it.
pub fn cancel_irp(irp_id:u64)->Result<(),IoError>{
 let mut s=STATE.lock();
 let irp=s.irps.iter_mut().find(|i|i.id==irp_id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
 if irp.completed{return Err(IoError::AlreadyCompleted);}
 if irp.in_flight{return Err(IoError::NotCancellable);}
 irp.status=IoStatus::Cancelled;irp.completed=true;irp.in_flight=false;Ok(())
}
/// Claim an IRP under lock, invoke its callback outside the lock, then publish
/// completion. in_flight prevents a second CPU from dispatching it concurrently.
pub fn dispatch_one(irp_id:u64)->Result<IoStatus,IoError>{
 let (mut irp,callback)={
  let mut s=STATE.lock();
  let index=s.irps.iter().position(|i|i.id==irp_id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
  if s.irps[index].completed{return Err(IoError::AlreadyCompleted);}
  if s.irps[index].in_flight{return Err(IoError::AlreadyInFlight);}
  let d=s.devices.iter().find(|d|d.used&&d.id==s.irps[index].device_id).ok_or(IoError::UnknownDevice)?;
  let cb=d.dispatch[s.irps[index].major as usize];
  s.irps[index].in_flight=true;
  (s.irps[index],cb)
 };
 let status=if let Some(cb)=callback{cb(&mut irp)}else{IoStatus::InvalidRequest};
 irp.status=status;irp.completed=status!=IoStatus::Pending;irp.in_flight=status==IoStatus::Pending;
 let mut s=STATE.lock();
 let index=s.irps.iter().position(|i|i.id==irp_id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
 s.irps[index]=irp;Ok(status)
}
/// Complete an asynchronously dispatched IRP. Only the callback-owned
/// in-flight request may be completed; terminal states cannot be overwritten.
pub fn complete_irp(id:u64,status:IoStatus,information:usize)->Result<(),IoError>{
 if status==IoStatus::Free||status==IoStatus::Pending{return Err(IoError::AlreadyCompleted);}
 let mut s=STATE.lock();
 let irp=s.irps.iter_mut().find(|i|i.id==id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
 if irp.completed{return Err(IoError::AlreadyCompleted);}
 if !irp.in_flight{return Err(IoError::NotCancellable);}
 if information>irp.length{return Err(IoError::BufferTooLarge);}
 irp.information=information;irp.status=status;irp.completed=true;irp.in_flight=false;Ok(())
}
pub fn query_irp(id:u64)->Option<Irp>{
 let s=STATE.lock();s.irps.iter().find(|i|i.id==id&&i.status!=IoStatus::Free).copied()
}
pub fn release_irp(id:u64)->Result<(),IoError>{
 let mut s=STATE.lock();
 let index=s.irps.iter().position(|i|i.id==id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
 if !s.irps[index].completed||s.irps[index].in_flight{return Err(IoError::AlreadyCompleted);}
 s.irps[index]=Irp::EMPTY;Ok(())
}

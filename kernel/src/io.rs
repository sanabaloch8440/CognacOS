//! Minimal driver/I/O-manager contracts: device registration, bounded IRPs,
//! dispatch, and completion. No hardware driver is claimed by this interface alone.
pub const MAX_DEVICES:usize=16;pub const MAX_IRPS:usize=64;pub const MAX_IRP_BYTES:usize=4096;
pub const IRP_MJ_CREATE:u8=0;pub const IRP_MJ_CLOSE:u8=2;pub const IRP_MJ_READ:u8=3;pub const IRP_MJ_WRITE:u8=4;pub const IRP_MJ_DEVICE_CONTROL:u8=14;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IoStatus{Free,Pending,Success,InvalidRequest,NoSuchDevice,BufferTooLarge,DeviceError,Cancelled}
#[derive(Clone,Copy,Debug)]
pub struct Irp{pub id:u64,pub major:u8,pub device_id:u32,pub user_buffer:u64,pub length:usize,pub information:usize,pub status:IoStatus,pub completed:bool}
impl Irp{const EMPTY:Self=Self{id:0,major:0,device_id:0,user_buffer:0,length:0,information:0,status:IoStatus::Free,completed:false};}
pub type DispatchFn=fn(&mut Irp)->IoStatus;
#[derive(Clone,Copy)]
pub struct Device{pub id:u32,pub name:[u8;32],pub name_len:u8,pub dispatch:[Option<DispatchFn>;16],pub used:bool}
impl Device{const EMPTY:Self=Self{id:0,name:[0;32],name_len:0,dispatch:[None;16],used:false};}
struct State{devices:[Device;MAX_DEVICES],irps:[Irp;MAX_IRPS],next_device:u32,next_irp:u64}
struct Shared{lock:core::sync::atomic::AtomicBool,state:core::cell::UnsafeCell<State>}
unsafe impl Sync for Shared{}
static STATE:Shared=Shared{lock:core::sync::atomic::AtomicBool::new(false),state:core::cell::UnsafeCell::new(State{devices:[Device::EMPTY;MAX_DEVICES],irps:[Irp::EMPTY;MAX_IRPS],next_device:1,next_irp:1})};
fn lock(){while STATE.lock.compare_exchange_weak(false,true,core::sync::atomic::Ordering::Acquire,core::sync::atomic::Ordering::Relaxed).is_err(){core::hint::spin_loop();}}
fn unlock(){STATE.lock.store(false,core::sync::atomic::Ordering::Release);}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IoError{InvalidName,NameTooLong,TooManyDevices,TooManyRequests,UnknownDevice,InvalidMajor,BufferTooLarge,UnknownIrp,AlreadyCompleted}
pub fn register_device(name:&[u8],dispatch:[Option<DispatchFn>;16])->Result<u32,IoError>{
 if name.is_empty(){return Err(IoError::InvalidName);}if name.len()>32{return Err(IoError::NameTooLong);}
 lock();let s=unsafe{&mut *STATE.state.get()};let Some(d)=s.devices.iter_mut().find(|d|!d.used)else{unlock();return Err(IoError::TooManyDevices);};
 let id=s.next_device;s.next_device=s.next_device.wrapping_add(1).max(1);d.id=id;d.name[..name.len()].copy_from_slice(name);d.name_len=name.len() as u8;d.dispatch=dispatch;d.used=true;unlock();Ok(id)
}
pub fn submit(device_id:u32,major:u8,user_buffer:u64,length:usize)->Result<u64,IoError>{
 if major as usize>=16{return Err(IoError::InvalidMajor);}if length>MAX_IRP_BYTES{return Err(IoError::BufferTooLarge);}
 lock();let s=unsafe{&mut *STATE.state.get()};
 if !s.devices.iter().any(|d|d.used&&d.id==device_id){unlock();return Err(IoError::UnknownDevice);}
 let Some(slot)=s.irps.iter_mut().find(|i|i.status==IoStatus::Free)else{unlock();return Err(IoError::TooManyRequests);};
 let id=s.next_irp;s.next_irp=s.next_irp.wrapping_add(1).max(1);*slot=Irp{id,major,device_id,user_buffer,length,information:0,status:IoStatus::Pending,completed:false};unlock();Ok(id)
}
/// Dispatch one request. Device callbacks execute outside the global lock.
pub fn dispatch_one(irp_id:u64)->Result<IoStatus,IoError>{
 let (mut irp,callback)={lock();let s=unsafe{&mut *STATE.state.get()};
 let i=s.irps.iter().position(|i|i.id==irp_id&&i.status!=IoStatus::Free).ok_or_else(||{unlock();IoError::UnknownIrp})?;
 let request=s.irps[i];if request.completed{unlock();return Err(IoError::AlreadyCompleted);}
 let Some(d)=s.devices.iter().find(|d|d.used&&d.id==request.device_id)else{unlock();return Err(IoError::UnknownDevice);};
 let cb=d.dispatch[request.major as usize];unlock();(request,cb)};
 irp.status=if let Some(cb)=callback{cb(&mut irp)}else{IoStatus::InvalidRequest};irp.completed=irp.status!=IoStatus::Pending;
 lock();let s=unsafe{&mut *STATE.state.get()};if let Some(slot)=s.irps.iter_mut().find(|i|i.id==irp_id){*slot=irp;}else{unlock();return Err(IoError::UnknownIrp);}unlock();Ok(irp.status)
}
pub fn query_irp(id:u64)->Option<Irp>{lock();let s=unsafe{&*STATE.state.get()};let out=s.irps.iter().find(|i|i.id==id&&i.status!=IoStatus::Free).copied();unlock();out}
pub fn release_irp(id:u64)->Result<(),IoError>{lock();let s=unsafe{&mut *STATE.state.get()};let Some(i)=s.irps.iter_mut().find(|i|i.id==id&&i.status!=IoStatus::Free)else{unlock();return Err(IoError::UnknownIrp);};if !i.completed{unlock();return Err(IoError::AlreadyCompleted);}*i=Irp::EMPTY;unlock();Ok(())}

//! Minimal NT-inspired object/handle foundation. Access checks are explicit and
//! names are byte strings; this is not yet the full NT Object Manager namespace.
use core::sync::atomic::{AtomicBool,Ordering};
pub const MAX_HANDLES:usize=128;pub const MAX_NAMED_OBJECTS:usize=32;pub const MAX_NAME:usize=48;
pub const ACCESS_READ:u32=1;pub const ACCESS_WRITE:u32=2;pub const ACCESS_CONTROL:u32=4;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub enum ObjectType { None=0, Process=1, Thread=2, Event=3, Semaphore=4, File=5, Device=6, Section=7, Directory=8 }
#[derive(Clone,Copy,Debug)]
pub struct HandleInfo {pub handle:u32,pub object_id:u64,pub kind:ObjectType,pub access:u32,pub used:bool}
impl HandleInfo{const EMPTY:Self=Self{handle:0,object_id:0,kind:ObjectType::None,access:0,used:false};}
#[derive(Clone,Copy)]
struct Named {name:[u8;MAX_NAME],len:u8,object_id:u64,kind:ObjectType,access:u32,used:bool}
impl Named{const EMPTY:Self=Self{name:[0;MAX_NAME],len:0,object_id:0,kind:ObjectType::None,access:0,used:false};}
struct State {handles:[HandleInfo;MAX_HANDLES],names:[Named;MAX_NAMED_OBJECTS],next_handle:u32,next_object:u64}
struct Shared{lock:AtomicBool,state:core::cell::UnsafeCell<State>}
unsafe impl Sync for Shared{}
static STATE:Shared=Shared{lock:AtomicBool::new(false),state:core::cell::UnsafeCell::new(State{handles:[HandleInfo::EMPTY;MAX_HANDLES],names:[Named::EMPTY;MAX_NAMED_OBJECTS],next_handle:4,next_object:1})};
fn lock(){while STATE.lock.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}}
fn unlock(){STATE.lock.store(false,Ordering::Release);}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ObjectError{InvalidName,NameTooLong,NameExists,NotFound,TableFull,AccessDenied,InvalidHandle}
pub fn create(kind:ObjectType,access:u32)->Result<HandleInfo,ObjectError>{
 if kind==ObjectType::None{return Err(ObjectError::InvalidHandle);}
 lock();let s=unsafe{&mut *STATE.state.get()};
 let Some(index)=s.handles.iter().position(|h|!h.used)else{unlock();return Err(ObjectError::TableFull);};
 let handle=s.next_handle;s.next_handle=s.next_handle.wrapping_add(4).max(4);let id=s.next_object;s.next_object=s.next_object.wrapping_add(1).max(1);
 let out=HandleInfo{handle,object_id:id,kind,access,used:true};s.handles[index]=out;unlock();Ok(out)
}
pub fn lookup(handle:u32,required_access:u32)->Result<HandleInfo,ObjectError>{
 lock();let s=unsafe{&*STATE.state.get()};let out=s.handles.iter().find(|h|h.used&&h.handle==handle).copied();unlock();
 let h=out.ok_or(ObjectError::InvalidHandle)?;if h.access&required_access!=required_access{return Err(ObjectError::AccessDenied);}Ok(h)
}
pub fn close(handle:u32)->Result<(),ObjectError>{
 lock();let s=unsafe{&mut *STATE.state.get()};let Some(index)=s.handles.iter().position(|h|h.used&&h.handle==handle)else{unlock();return Err(ObjectError::InvalidHandle);};s.handles[index]=HandleInfo::EMPTY;unlock();Ok(())
}
pub fn name_object(name:&[u8],object_id:u64,kind:ObjectType,access:u32)->Result<(),ObjectError>{
 if name.is_empty(){return Err(ObjectError::InvalidName);}if name.len()>MAX_NAME{return Err(ObjectError::NameTooLong);}
 lock();let s=unsafe{&mut *STATE.state.get()};
 if s.names.iter().any(|n|n.used&&n.len as usize==name.len()&&n.name[..name.len()]==*name){unlock();return Err(ObjectError::NameExists);}
 let Some(index)=s.names.iter().position(|n|!n.used)else{unlock();return Err(ObjectError::TableFull);};
 let mut entry=Named::EMPTY;entry.name[..name.len()].copy_from_slice(name);entry.len=name.len() as u8;entry.object_id=object_id;entry.kind=kind;entry.access=access;entry.used=true;s.names[index]=entry;unlock();Ok(())
}
pub fn open_named(name:&[u8],required_access:u32)->Result<HandleInfo,ObjectError>{
 let (id,kind,access)={lock();let s=unsafe{&*STATE.state.get()};let found=s.names.iter().find(|n|n.used&&n.len as usize==name.len()&&n.name[..name.len()]==*name).map(|n|(n.object_id,n.kind,n.access));unlock();found.ok_or(ObjectError::NotFound)?};
 if access&required_access!=required_access{return Err(ObjectError::AccessDenied);}
 lock();let s=unsafe{&mut *STATE.state.get()};let Some(index)=s.handles.iter().position(|h|!h.used)else{unlock();return Err(ObjectError::TableFull);};
 let handle=s.next_handle;s.next_handle=s.next_handle.wrapping_add(4).max(4);s.handles[index]=HandleInfo{handle,object_id:id,kind,access:required_access,used:true};let out=s.handles[index];unlock();Ok(out)
}

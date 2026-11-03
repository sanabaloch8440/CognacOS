//! ACPI-6 hotplug, eject, removal and wake lifecycle primitives.
#![no_std]
pub const MAX_HOTPLUG_EVENTS:usize=128;
pub const NOTIFY_BUS_CHECK:u8=0;
pub const NOTIFY_DEVICE_CHECK:u8=1;
pub const NOTIFY_DEVICE_WAKE:u8=2;
pub const NOTIFY_EJECT_REQUEST:u8=3;
pub const NOTIFY_DEVICE_CHECK_LIGHT:u8=4;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum HotplugError{QueueFull,InvalidTransition,MethodFailed,RemovalNotDrained,StaleGeneration}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum HotplugEventKind{BusCheck,DeviceCheck,DeviceWake,EjectRequest,DeviceCheckLight}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct HotplugEvent{pub node:u16,pub generation:u32,pub kind:HotplugEventKind}
pub struct HotplugQueue{events:[Option<HotplugEvent>;MAX_HOTPLUG_EVENTS],head:usize,len:usize}
impl HotplugQueue{
 pub const fn new()->Self{Self{events:[None;MAX_HOTPLUG_EVENTS],head:0,len:0}}
/// Queue one firmware notification without overwriting an event that has not been processed.
 pub fn push(&mut self,e:HotplugEvent)->Result<(),HotplugError>{if self.len==MAX_HOTPLUG_EVENTS{return Err(HotplugError::QueueFull)}let i=(self.head+self.len)%MAX_HOTPLUG_EVENTS;self.events[i]=Some(e);self.len+=1;Ok(())}
/// Remove and return the oldest queued firmware notification, preserving FIFO order.
 pub fn pop(&mut self)->Option<HotplugEvent>{if self.len==0{return None}let e=self.events[self.head].take();self.head=(self.head+1)%MAX_HOTPLUG_EVENTS;self.len-=1;e}
/// Return the number of notifications currently awaiting processing.
 pub fn len(&self)->usize{self.len}
}
/// Translate an ACPI Notify value into a supported hotplug event; unknown values are ignored.
pub fn decode_notify(v:u32)->Option<HotplugEventKind>{match v{0=>Some(HotplugEventKind::BusCheck),1=>Some(HotplugEventKind::DeviceCheck),2=>Some(HotplugEventKind::DeviceWake),3=>Some(HotplugEventKind::EjectRequest),4=>Some(HotplugEventKind::DeviceCheckLight),_=>None}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DeviceLifecycle{Absent,Present,Enumerated,EjectPending,RemovalPending,Removed}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct HotplugDevice{pub node:u16,pub generation:u32,pub state:DeviceLifecycle,pub wake_armed:bool,pub eject_in_progress:bool}
impl HotplugDevice{pub const EMPTY:Self=Self{node:0,generation:0,state:DeviceLifecycle::Absent,wake_armed:false,eject_in_progress:false};}
pub trait HotplugBackend{
 fn evaluate_ej0(&mut self,node:u16)->bool;
 fn report_ost(&mut self,node:u16,status:u32,argument:u32);
 fn arm_wake(&mut self,node:u16)->bool;
 fn disarm_wake(&mut self,node:u16)->bool;
 fn request_pnp_remove(&mut self,node:u16,surprise:bool)->bool;
 fn pdo_references_drained(&mut self,node:u16)->bool;
 fn remove_pdo(&mut self,node:u16)->bool;
}
pub const OST_STATUS_SUCCESS:u32=0;
pub const OST_STATUS_FAILURE:u32=1;
pub const OST_STATUS_EJECT_IN_PROGRESS:u32=2;
/// Begin a new device-presence generation after validating the previous lifecycle state.
pub fn device_arrived(d:&mut HotplugDevice,node:u16,g:u32)->Result<(),HotplugError>{
 if d.state!=DeviceLifecycle::Absent&&d.state!=DeviceLifecycle::Removed{return Err(HotplugError::InvalidTransition)}
 *d=HotplugDevice{node,generation:g,state:DeviceLifecycle::Present,wake_armed:false,eject_in_progress:false};Ok(())
}
/// Promote a present device to the enumerated state only for its current generation.
pub fn mark_enumerated(d:&mut HotplugDevice,g:u32)->Result<(),HotplugError>{if d.generation!=g{return Err(HotplugError::StaleGeneration)}if d.state!=DeviceLifecycle::Present{return Err(HotplugError::InvalidTransition)}d.state=DeviceLifecycle::Enumerated;Ok(())}
/// Run the orderly eject transaction: quiesce wake, evaluate _EJ0, then request PnP removal.
pub fn request_eject(d:&mut HotplugDevice,g:u32,b:&mut impl HotplugBackend)->Result<(),HotplugError>{
 if d.generation!=g{return Err(HotplugError::StaleGeneration)}if d.state!=DeviceLifecycle::Enumerated{return Err(HotplugError::InvalidTransition)}
 d.state=DeviceLifecycle::EjectPending;d.eject_in_progress=true;b.report_ost(d.node,OST_STATUS_EJECT_IN_PROGRESS,0);
 // Disarm wake before asking firmware to eject the device. Once _EJ0
 // succeeds, firmware may remove power or make the device inaccessible.
 // If _EJ0 refuses the eject, restore the prior wake configuration.
 let had_wake=d.wake_armed;
 if had_wake&&!b.disarm_wake(d.node){
  d.state=DeviceLifecycle::Enumerated;d.eject_in_progress=false;
  b.report_ost(d.node,OST_STATUS_FAILURE,0);return Err(HotplugError::MethodFailed)
 }
 if had_wake{d.wake_armed=false;}
 if !b.evaluate_ej0(d.node){
  let restored=!had_wake||b.arm_wake(d.node);
  d.wake_armed=had_wake&&restored;
  d.state=DeviceLifecycle::Enumerated;d.eject_in_progress=false;
  b.report_ost(d.node,OST_STATUS_FAILURE,0);return Err(HotplugError::MethodFailed)
 }
 if !b.request_pnp_remove(d.node,false){d.state=DeviceLifecycle::RemovalPending;b.report_ost(d.node,OST_STATUS_FAILURE,0);return Err(HotplugError::MethodFailed)}
 d.state=DeviceLifecycle::RemovalPending;Ok(())
}
/// Begin surprise removal without running the firmware orderly-eject method.
pub fn surprise_remove(d:&mut HotplugDevice,g:u32,b:&mut impl HotplugBackend)->Result<(),HotplugError>{
 if d.generation!=g{return Err(HotplugError::StaleGeneration)}if d.state!=DeviceLifecycle::Enumerated&&d.state!=DeviceLifecycle::Present{return Err(HotplugError::InvalidTransition)}
 if d.wake_armed&&!b.disarm_wake(d.node){return Err(HotplugError::MethodFailed)}d.wake_armed=false;
 if !b.request_pnp_remove(d.node,true){return Err(HotplugError::MethodFailed)}d.state=DeviceLifecycle::RemovalPending;Ok(())
}
/// Destroy a removal-pending PDO only after the kernel confirms all outstanding references drained.
pub fn finalize_removal(d:&mut HotplugDevice,g:u32,b:&mut impl HotplugBackend)->Result<(),HotplugError>{
 if d.generation!=g{return Err(HotplugError::StaleGeneration)}if d.state!=DeviceLifecycle::RemovalPending{return Err(HotplugError::InvalidTransition)}
 if !b.pdo_references_drained(d.node){return Err(HotplugError::RemovalNotDrained)}if !b.remove_pdo(d.node){return Err(HotplugError::MethodFailed)}
 d.state=DeviceLifecycle::Removed;d.eject_in_progress=false;Ok(())
}
/// Arm firmware wake resources for an enumerated device and publish the state only on success.
pub fn arm_device_wake(d:&mut HotplugDevice,g:u32,b:&mut impl HotplugBackend)->Result<(),HotplugError>{
 if d.generation!=g{return Err(HotplugError::StaleGeneration)}if d.state!=DeviceLifecycle::Enumerated{return Err(HotplugError::InvalidTransition)}
 if d.wake_armed{return Ok(())}if !b.arm_wake(d.node){return Err(HotplugError::MethodFailed)}d.wake_armed=true;Ok(())
}
/// Disarm firmware wake resources and retain the armed state if the backend fails.
pub fn disarm_device_wake(d:&mut HotplugDevice,g:u32,b:&mut impl HotplugBackend)->Result<(),HotplugError>{
 if d.generation!=g{return Err(HotplugError::StaleGeneration)}if !d.wake_armed{return Ok(())}
 if !b.disarm_wake(d.node){return Err(HotplugError::MethodFailed)}d.wake_armed=false;Ok(())
}
#[cfg(test)]extern crate std;
#[cfg(test)]mod tests{
 use super::*;
 struct Fake{ej:bool,wake:bool,remove:bool,drained:bool,removed:bool,ost_count:usize,ej_calls:usize,arm_calls:usize,disarm_calls:usize}
 impl HotplugBackend for Fake{
  fn evaluate_ej0(&mut self,_:u16)->bool{self.ej_calls+=1;self.ej}
  fn report_ost(&mut self,_:u16,_:u32,_:u32){self.ost_count+=1}
  fn arm_wake(&mut self,_:u16)->bool{self.arm_calls+=1;self.wake}
  fn disarm_wake(&mut self,_:u16)->bool{self.disarm_calls+=1;self.wake}
  fn request_pnp_remove(&mut self,_:u16,_:bool)->bool{self.remove}
  fn pdo_references_drained(&mut self,_:u16)->bool{self.drained}
  fn remove_pdo(&mut self,_:u16)->bool{self.removed}
 }
 fn fake()->Fake{Fake{ej:true,wake:true,remove:true,drained:false,removed:true,ost_count:0,ej_calls:0,arm_calls:0,disarm_calls:0}}
 fn ready()->HotplugDevice{let mut d=HotplugDevice::EMPTY;device_arrived(&mut d,7,4).unwrap();mark_enumerated(&mut d,4).unwrap();d}
 #[test]fn notify_decode_is_bounded(){assert_eq!(decode_notify(3),Some(HotplugEventKind::EjectRequest));assert_eq!(decode_notify(128),None);}
 #[test]fn queue_fifo(){let mut q=HotplugQueue::new();q.push(HotplugEvent{node:1,generation:1,kind:HotplugEventKind::BusCheck}).unwrap();assert_eq!(q.pop().unwrap().node,1);assert_eq!(q.len(),0);}
 #[test]fn eject_waits_for_reference_rundown(){let mut d=ready();let mut b=fake();request_eject(&mut d,4,&mut b).unwrap();assert_eq!(d.state,DeviceLifecycle::RemovalPending);assert_eq!(finalize_removal(&mut d,4,&mut b),Err(HotplugError::RemovalNotDrained));b.drained=true;finalize_removal(&mut d,4,&mut b).unwrap();assert_eq!(d.state,DeviceLifecycle::Removed);}
 #[test]fn eject_failure_restores_state_and_wake(){let mut d=ready();d.wake_armed=true;let mut b=fake();b.ej=false;assert_eq!(request_eject(&mut d,4,&mut b),Err(HotplugError::MethodFailed));assert_eq!(d.state,DeviceLifecycle::Enumerated);assert!(d.wake_armed);assert_eq!(b.disarm_calls,1);assert_eq!(b.ej_calls,1);assert_eq!(b.arm_calls,1);}
 #[test]fn eject_disarms_wake_before_firmware_eject(){let mut d=ready();d.wake_armed=true;let mut b=fake();request_eject(&mut d,4,&mut b).unwrap();assert_eq!(b.disarm_calls,1);assert_eq!(b.arm_calls,0);assert_eq!(d.state,DeviceLifecycle::RemovalPending);assert!(!d.wake_armed);}
 #[test]fn eject_disarm_failure_does_not_call_firmware(){let mut d=ready();d.wake_armed=true;let mut b=fake();b.wake=false;assert_eq!(request_eject(&mut d,4,&mut b),Err(HotplugError::MethodFailed));assert_eq!(b.ej_calls,0);assert!(d.wake_armed);assert_eq!(d.state,DeviceLifecycle::Enumerated);}
 #[test]fn stale_generation_rejected(){let mut d=ready();let mut b=fake();assert_eq!(arm_device_wake(&mut d,3,&mut b),Err(HotplugError::StaleGeneration));}
 #[test]fn wake_arm_is_idempotent(){let mut d=ready();let mut b=fake();arm_device_wake(&mut d,4,&mut b).unwrap();arm_device_wake(&mut d,4,&mut b).unwrap();assert!(d.wake_armed);disarm_device_wake(&mut d,4,&mut b).unwrap();assert!(!d.wake_armed);}
 #[test]fn surprise_removal_skips_firmware_eject(){let mut d=ready();let mut b=fake();surprise_remove(&mut d,4,&mut b).unwrap();assert_eq!(d.state,DeviceLifecycle::RemovalPending);assert_eq!(b.ost_count,0);}
}
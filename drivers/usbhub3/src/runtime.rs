//! USB hub enumeration coordinator and child-PDO lifetime policy.
#![no_std]
use crate::{parse_device_descriptor,validate_configuration_descriptor,DescriptorError,Hub,HubError,PortEvent,PortState,UsbDeviceDescriptor};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EnumerationState{Idle,Debouncing,Resetting,ReadingDeviceDescriptor,ReadingConfiguration,CreatingPdo,Active,Removing,Failed}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EnumerationError{InvalidPort,InvalidState,TransferFailed,Descriptor(DescriptorError),Hub(HubError),StaleGeneration,PdoCreateFailed,PdoRemoveFailed,OutstandingReferences,InvalidTransferLength}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ChildPdo{pub port:u8,pub generation:u64,pub vendor_id:u16,pub product_id:u16,pub configuration_length:u16,pub active:bool,pub references:u16}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PortWork{pub port:u8,pub generation:u64,pub state:EnumerationState}
pub trait HubServices{
 fn reset_port(&mut self,port:u8,generation:u64)->Result<(),()>;
 fn get_device_descriptor(&mut self,port:u8,generation:u64,buffer:&mut [u8])->Result<usize,()>;
 fn get_configuration_descriptor(&mut self,port:u8,generation:u64,index:u8,buffer:&mut [u8])->Result<usize,()>;
 fn create_child_pdo(&mut self,child:ChildPdo)->Result<u32,()>;
 fn remove_child_pdo(&mut self,port:u8,generation:u64,pdo_id:u32)->Result<(),()>;
}
#[derive(Clone,Copy)]
struct PortRecord{state:EnumerationState,generation:u64,pdo_id:u32,child:Option<ChildPdo>}
impl PortRecord{const EMPTY:Self=Self{state:EnumerationState::Idle,generation:0,pdo_id:0,child:None};}
pub struct HubRuntime<const P:usize>{hub:Hub,ports:[PortRecord;P],port_count:u8}
impl<const P:usize> HubRuntime<P>{
 pub const fn new()->Self{Self{hub:Hub::new(),ports:[PortRecord::EMPTY;P],port_count:0}}
 pub fn configure(&mut self,count:u8)->Result<(),EnumerationError>{if count==0||count as usize>P{return Err(EnumerationError::InvalidPort)}self.hub.configure_ports(count).map_err(EnumerationError::Hub)?;self.port_count=count;Ok(())}
 fn index(&self,port:u8)->Result<usize,EnumerationError>{if port==0||port>self.port_count{Err(EnumerationError::InvalidPort)}else{Ok(port as usize-1)}}
 pub fn port_work(&self,port:u8)->Result<PortWork,EnumerationError>{let p=self.ports[self.index(port)?];Ok(PortWork{port,generation:p.generation,state:p.state})}
 /// Called by the port-change worker, not the ISR. A new connect event cannot
 /// replace a live PDO; disconnect/rundown must complete first.
 pub fn notify_connect(&mut self,port:u8)->Result<PortWork,EnumerationError>{
  let i=self.index(port)?;if self.ports[i].state!=EnumerationState::Idle{return Err(EnumerationError::InvalidState)}
  let generation=self.ports[i].generation.wrapping_add(1).max(1);
  self.ports[i]=PortRecord{state:EnumerationState::Debouncing,generation,pdo_id:0,child:None};
  let p=self.hub.port_mut(port).map_err(EnumerationError::Hub)?;
  if p.state()==PortState::Disconnected{p.transition(PortEvent::Connect).map_err(EnumerationError::Hub)?;}
  Ok(PortWork{port,generation,state:EnumerationState::Debouncing})
 }
 /// Passive-level enumeration sequence. The supplied callbacks submit
 /// asynchronous UCX control URBs and wait in a worker, never in a DPC.
 pub fn enumerate<S:HubServices>(&mut self,s:&mut S,w:PortWork,config_index:u8,config_buffer:&mut [u8])->Result<ChildPdo,EnumerationError>{
  let i=self.index(w.port)?;if self.ports[i].generation!=w.generation{return Err(EnumerationError::StaleGeneration)}
  if self.ports[i].state!=EnumerationState::Debouncing{return Err(EnumerationError::InvalidState)}
  self.ports[i].state=EnumerationState::Resetting;
  self.hub.port_mut(w.port).map_err(EnumerationError::Hub)?.transition(PortEvent::BeginReset).map_err(EnumerationError::Hub)?;
  if s.reset_port(w.port,w.generation).is_err(){self.ports[i].state=EnumerationState::Failed;return Err(EnumerationError::TransferFailed)}
  self.hub.port_mut(w.port).map_err(EnumerationError::Hub)?.transition(PortEvent::ResetComplete).map_err(EnumerationError::Hub)?;
  self.ports[i].state=EnumerationState::ReadingDeviceDescriptor;
  let mut db=[0u8;18];let got=s.get_device_descriptor(w.port,w.generation,&mut db).map_err(|_|EnumerationError::TransferFailed)?;
  if got<18{self.ports[i].state=EnumerationState::Failed;return Err(EnumerationError::Descriptor(DescriptorError::Truncated))}
  let d:UsbDeviceDescriptor=parse_device_descriptor(&db).map_err(EnumerationError::Descriptor)?;
  self.hub.port_mut(w.port).map_err(EnumerationError::Hub)?.transition(PortEvent::BeginEnumeration).map_err(EnumerationError::Hub)?;
  self.ports[i].state=EnumerationState::ReadingConfiguration;
  let n=s.get_configuration_descriptor(w.port,w.generation,config_index,config_buffer).map_err(|_|EnumerationError::TransferFailed)?;
  if n>config_buffer.len(){self.ports[i].state=EnumerationState::Failed;return Err(EnumerationError::InvalidTransferLength)}
  let total=validate_configuration_descriptor(&config_buffer[..n]).map_err(EnumerationError::Descriptor)?;
  if total>u16::MAX as usize{return Err(EnumerationError::Descriptor(DescriptorError::InvalidConfigurationLength))}
  self.ports[i].state=EnumerationState::CreatingPdo;
  let child=ChildPdo{port:w.port,generation:w.generation,vendor_id:d.vendor_id,product_id:d.product_id,configuration_length:total as u16,active:true,references:1};
  let pdo=s.create_child_pdo(child).map_err(|_|EnumerationError::PdoCreateFailed)?;
  self.ports[i].pdo_id=pdo;self.ports[i].child=Some(child);self.ports[i].state=EnumerationState::Active;
  self.hub.port_mut(w.port).map_err(EnumerationError::Hub)?.transition(PortEvent::EnumerationComplete).map_err(EnumerationError::Hub)?;
  Ok(child)
 }
 /// Invalidate the generation before cancelling endpoint queues. The kernel
 /// must pass zero only after PnP references and IRPs have drained.
 pub fn disconnect<S:HubServices>(&mut self,s:&mut S,port:u8,outstanding:u16)->Result<(),EnumerationError>{
  let i=self.index(port)?;let old=self.ports[i];self.ports[i].generation=self.ports[i].generation.wrapping_add(1).max(1);self.ports[i].state=EnumerationState::Removing;
  if let Ok(p)=self.hub.port_mut(port){let _=p.transition(PortEvent::Disconnect);}
  if old.child.is_some(){if outstanding!=0{return Err(EnumerationError::OutstandingReferences)}s.remove_child_pdo(port,old.generation,old.pdo_id).map_err(|_|EnumerationError::PdoRemoveFailed)?;}
  self.ports[i]=PortRecord{state:EnumerationState::Idle,generation:self.ports[i].generation,pdo_id:0,child:None};Ok(())
 }
 pub fn child(&self,port:u8)->Result<Option<ChildPdo>,EnumerationError>{Ok(self.ports[self.index(port)?].child)}
}
impl<const P:usize> Default for HubRuntime<P>{fn default()->Self{Self::new()}}
#[cfg(test)]mod tests{
 use super::*;
 struct Mock{reset:bool,pdo:bool,removed:bool}
 impl HubServices for Mock{
  fn reset_port(&mut self,_:u8,_:u64)->Result<(),()>{self.reset=true;Ok(())}
  fn get_device_descriptor(&mut self,_:u8,_:u64,b:&mut [u8])->Result<usize,()>{let d=[18,1,0,2,0,0,0,64,0x34,0x12,0x78,0x56,0,1,1,2,3,1];b[..18].copy_from_slice(&d);Ok(18)}
  fn get_configuration_descriptor(&mut self,_:u8,_:u64,_:u8,b:&mut [u8])->Result<usize,()>{let d=[9,2,9,0,0,1,0,0x80,50];b[..9].copy_from_slice(&d);Ok(9)}
  fn create_child_pdo(&mut self,_:ChildPdo)->Result<u32,()>{self.pdo=true;Ok(77)}
  fn remove_child_pdo(&mut self,_:u8,_:u64,_:u32)->Result<(),()>{self.removed=true;Ok(())}
 }
 #[test]fn enumerate_and_remove_child(){let mut h=HubRuntime::<8>::new();h.configure(4).unwrap();let w=h.notify_connect(2).unwrap();let mut m=Mock{reset:false,pdo:false,removed:false};let mut b=[0u8;64];let c=h.enumerate(&mut m,w,0,&mut b).unwrap();assert_eq!(c.vendor_id,0x1234);assert!(m.reset&&m.pdo);assert_eq!(h.disconnect(&mut m,2,1),Err(EnumerationError::OutstandingReferences));h.disconnect(&mut m,2,0).unwrap();assert!(m.removed);}
 #[test]fn stale_work_rejected(){let mut h=HubRuntime::<4>::new();h.configure(2).unwrap();let w=h.notify_connect(1).unwrap();let mut m=Mock{reset:false,pdo:false,removed:false};h.disconnect(&mut m,1,0).unwrap();let mut b=[0u8;32];assert_eq!(h.enumerate(&mut m,w,0,&mut b),Err(EnumerationError::StaleGeneration));}
}

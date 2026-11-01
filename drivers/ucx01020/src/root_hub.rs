//! UCX root-hub class control requests and interrupt-change bitmap.
#![no_std]
pub const USB_REQ_GET_STATUS:u8=0;
pub const USB_REQ_CLEAR_FEATURE:u8=1;
pub const USB_REQ_SET_FEATURE:u8=3;
pub const USB_REQ_GET_DESCRIPTOR:u8=6;
pub const USB_RT_CLASS_DEVICE_IN:u8=0xa0;
pub const USB_RT_CLASS_OTHER_IN:u8=0xa3;
pub const USB_RT_CLASS_OTHER_OUT:u8=0x23;
pub const USB_PORT_FEAT_ENABLE:u16=1;
pub const USB_PORT_FEAT_SUSPEND:u16=2;
pub const USB_PORT_FEAT_RESET:u16=4;
pub const USB_PORT_FEAT_POWER:u16=8;
pub const USB_PORT_FEAT_C_CONNECTION:u16=16;
pub const USB_PORT_FEAT_C_ENABLE:u16=17;
pub const USB_PORT_FEAT_C_SUSPEND:u16=18;
pub const USB_PORT_FEAT_C_OVER_CURRENT:u16=19;
pub const USB_PORT_FEAT_C_RESET:u16=20;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SetupPacket{pub request_type:u8,pub request:u8,pub value:u16,pub index:u16,pub length:u16}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct PortStatus{pub status:u16,pub change:u16}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RootHubError{InvalidRequest,InvalidPort,BufferTooSmall,HardwareFailure,UnsupportedFeature}
pub trait RootHubPortIo{
 fn read_hub_status(&mut self)->Result<PortStatus,()>{Ok(PortStatus::default())}
 fn read_port(&mut self,port:u8)->Result<PortStatus,()>;
 fn set_feature(&mut self,port:u8,feature:u16)->Result<(),()>;
 fn clear_feature(&mut self,port:u8,feature:u16)->Result<(),()>;
 fn get_descriptor(&mut self,descriptor_type:u8,buffer:&mut [u8])->Result<usize,()>;
}
/// Fixed-size notification state. Bit zero is the hub bit; port N is bit N.
pub struct RootHubNotifications{port_count:u8,generation:u32,bitmap:[u8;32],pending:bool}
impl RootHubNotifications{
 pub const fn new(port_count:u8)->Self{Self{port_count,generation:1,bitmap:[0;32],pending:false}}
 pub fn generation(&self)->u32{self.generation}
 pub fn notify_port(&mut self,port:u8)->Result<(),RootHubError>{if port==0||port>self.port_count{return Err(RootHubError::InvalidPort)}let bit=port as usize;self.bitmap[bit/8]|=1<<(bit%8);self.pending=true;self.generation=self.generation.wrapping_add(1).max(1);Ok(())}
 pub fn complete_interrupt(&mut self,out:&mut [u8])->Result<usize,RootHubError>{let n=(self.port_count as usize+1+7)/8;if out.len()<n{return Err(RootHubError::BufferTooSmall)}if !self.pending{return Ok(0)}out[..n].copy_from_slice(&self.bitmap[..n]);self.bitmap[..n].fill(0);self.pending=false;Ok(n)}
 pub fn has_pending(&self)->bool{self.pending}
}
/// Implements hub-class GET_STATUS, GET_DESCRIPTOR and port feature requests.
/// The caller validates transfer direction and executes this at PASSIVE_LEVEL.
pub fn handle_control<I:RootHubPortIo>(io:&mut I,setup:SetupPacket,port_count:u8,out:&mut [u8])->Result<usize,RootHubError>{
 if setup.length as usize>out.len(){return Err(RootHubError::BufferTooSmall)}
 match (setup.request_type,setup.request){
  (USB_RT_CLASS_DEVICE_IN,USB_REQ_GET_STATUS)=>{
   if setup.length<4{return Err(RootHubError::BufferTooSmall)}
   let s=io.read_hub_status().map_err(|_|RootHubError::HardwareFailure)?;let status=s.status.to_le_bytes();let change=s.change.to_le_bytes();out[..2].copy_from_slice(&status);out[2..4].copy_from_slice(&change);Ok(4)
  },
  (USB_RT_CLASS_DEVICE_IN,USB_REQ_GET_DESCRIPTOR)=>{
   let kind=(setup.value>>8) as u8;
   let limit=setup.length as usize;let n=io.get_descriptor(kind,&mut out[..limit]).map_err(|_|RootHubError::HardwareFailure)?;
   if n>out.len()||n>setup.length as usize{return Err(RootHubError::BufferTooSmall)}Ok(n)
  },
  (USB_RT_CLASS_OTHER_IN,USB_REQ_GET_STATUS)=>{
   let port=u8::try_from(setup.index).map_err(|_|RootHubError::InvalidPort)?;
   if port==0||port>port_count||setup.length<4{return Err(RootHubError::InvalidPort)}
   let s=io.read_port(port).map_err(|_|RootHubError::HardwareFailure)?;
   let status=s.status.to_le_bytes();let change=s.change.to_le_bytes();out[..2].copy_from_slice(&status);out[2..4].copy_from_slice(&change);Ok(4)
  },
  (USB_RT_CLASS_OTHER_OUT,USB_REQ_SET_FEATURE)=>{
   let port=u8::try_from(setup.index).map_err(|_|RootHubError::InvalidPort)?;
   if port==0||port>port_count{return Err(RootHubError::InvalidPort)}
   io.set_feature(port,setup.value).map_err(|_|RootHubError::HardwareFailure)?;Ok(0)
  },
  (USB_RT_CLASS_OTHER_OUT,USB_REQ_CLEAR_FEATURE)=>{
   let port=u8::try_from(setup.index).map_err(|_|RootHubError::InvalidPort)?;
   if port==0||port>port_count{return Err(RootHubError::InvalidPort)}
   match setup.value{USB_PORT_FEAT_ENABLE|USB_PORT_FEAT_SUSPEND|USB_PORT_FEAT_POWER|USB_PORT_FEAT_C_CONNECTION|USB_PORT_FEAT_C_ENABLE|USB_PORT_FEAT_C_SUSPEND|USB_PORT_FEAT_C_OVER_CURRENT|USB_PORT_FEAT_C_RESET=>{},_=>return Err(RootHubError::UnsupportedFeature)}
   io.clear_feature(port,setup.value).map_err(|_|RootHubError::HardwareFailure)?;Ok(0)
  },
  (USB_RT_CLASS_OTHER_OUT,USB_REQ_SET_FEATURE)=>{
   let port=u8::try_from(setup.index).map_err(|_|RootHubError::InvalidPort)?;
   if port==0||port>port_count{return Err(RootHubError::InvalidPort)}
   match setup.value{USB_PORT_FEAT_ENABLE|USB_PORT_FEAT_SUSPEND|USB_PORT_FEAT_RESET|USB_PORT_FEAT_POWER=>{},_=>return Err(RootHubError::UnsupportedFeature)}
   io.set_feature(port,setup.value).map_err(|_|RootHubError::HardwareFailure)?;Ok(0)
  },
  _=>Err(RootHubError::InvalidRequest),
 }
}
#[cfg(test)]mod tests{
 use super::*;
 struct Io;impl RootHubPortIo for Io{fn read_port(&mut self,_:u8)->Result<PortStatus,()>{Ok(PortStatus{status:3,change:1})}fn set_feature(&mut self,_:u8,_:u16)->Result<(),()>{Ok(())}fn clear_feature(&mut self,_:u8,_:u16)->Result<(),()>{Ok(())}fn get_descriptor(&mut self,_:u8,b:&mut [u8])->Result<usize,()>{b[0]=9;Ok(9)}}
 #[test]fn root_hub_status_and_port_requests(){let mut io=Io;let mut b=[0u8;16];let s=SetupPacket{request_type:USB_RT_CLASS_OTHER_IN,request:USB_REQ_GET_STATUS,value:0,index:1,length:4};assert_eq!(handle_control(&mut io,s,4,&mut b),Ok(4));assert_eq!(&b[..4],&[3,0,1,0]);}
 #[test]fn notification_bitmap_coalesces_changes(){let mut n=RootHubNotifications::new(8);n.notify_port(2).unwrap();n.notify_port(3).unwrap();let mut b=[0u8;2];assert_eq!(n.complete_interrupt(&mut b),Ok(2));assert_eq!(b[0],0x0c);assert!(!n.has_pending());}
}

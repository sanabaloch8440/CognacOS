//! WDM-facing USB device, PnP and IRP/URB dispatch contract.
#![allow(non_camel_case_types)]
use core::sync::atomic::{AtomicU32,Ordering};
use cognacos_nt_abi::ntstatus::NTSTATUS;
pub const STATUS_SUCCESS:NTSTATUS=0;
pub const STATUS_INVALID_PARAMETER:NTSTATUS=0xC000000D_u32 as i32;
pub const STATUS_INVALID_DEVICE_REQUEST:NTSTATUS=0xC0000010_u32 as i32;
pub const IRP_MJ_CREATE:u8=0x00; pub const IRP_MJ_CLOSE:u8=0x02; pub const IRP_MJ_READ:u8=0x03; pub const IRP_MJ_WRITE:u8=0x04; pub const IRP_MJ_FLUSH_BUFFERS:u8=0x09; pub const IRP_MJ_SHUTDOWN:u8=0x10; pub const IRP_MJ_CLEANUP:u8=0x12;
pub const IRP_MJ_PNP:u8=0x1b; pub const IRP_MJ_POWER:u8=0x16;
pub const IRP_MJ_INTERNAL_DEVICE_CONTROL:u8=0x0f; pub const IRP_MJ_DEVICE_CONTROL:u8=0x0e;
pub const IRP_MJ_SYSTEM_CONTROL:u8=0x17;
pub const IRP_MN_START_DEVICE:u8=0; pub const IRP_MN_QUERY_REMOVE_DEVICE:u8=1;
pub const IRP_MN_REMOVE_DEVICE:u8=2; pub const IRP_MN_STOP_DEVICE:u8=4;
pub const IRP_MN_SURPRISE_REMOVAL:u8=0x17;
pub const IOCTL_INTERNAL_USB_SUBMIT_URB:u32=0x00220003;
pub const URB_FUNCTION_SELECT_CONFIGURATION:u16=0;
pub const URB_FUNCTION_SELECT_INTERFACE:u16=1;
pub const URB_FUNCTION_ABORT_PIPE:u16=2;
pub const URB_FUNCTION_CONTROL_TRANSFER:u16=8;
pub const URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER:u16=9;
pub const URB_FUNCTION_ISOCH_TRANSFER:u16=0x0a;
pub const URB_FUNCTION_GET_DESCRIPTOR_FROM_DEVICE:u16=0x0b;
pub const URB_FUNCTION_GET_STATUS_FROM_DEVICE:u16=0x10;
pub const URB_FUNCTION_VENDOR_DEVICE:u16=0x17;
pub const URB_FUNCTION_CLASS_DEVICE:u16=0x1a;
pub const URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL:u16=0x30;
pub const URB_FUNCTION_SYNC_RESET_PIPE:u16=0x31;
pub const URB_FUNCTION_SYNC_CLEAR_STALL:u16=0x32;
#[derive(Clone,Copy,Debug,PartialEq,Eq)] #[repr(u8)]
pub enum DeviceKind{Controller=1,RootHub=2,ExternalHub=3,UsbFunction=4}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] #[repr(u8)]
pub enum PnpState{NotStarted,Started,StopPending,Stopped,RemovePending,SurpriseRemoved,Removed}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DispatchError{InvalidTransition,Removing,ChildrenPresent,RequestsOutstanding}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct DeviceHandle{pub id:u32,pub generation:u32,pub kind:DeviceKind}
#[derive(Clone,Copy)]
pub struct DeviceNode{pub handle:DeviceHandle,pub parent:Option<DeviceHandle>,pub state:PnpState,pub child_count:u16,pub outstanding_irps:u32,pub references:u32,pub surprise_removed:bool}
impl DeviceNode{
 pub const fn new(handle:DeviceHandle,parent:Option<DeviceHandle>)->Self{Self{handle,parent,state:PnpState::NotStarted,child_count:0,outstanding_irps:0,references:1,surprise_removed:false}}
 pub fn start(&mut self)->Result<(),DispatchError>{if self.state!=PnpState::NotStarted&&self.state!=PnpState::Stopped{return Err(DispatchError::InvalidTransition)}self.state=PnpState::Started;Ok(())}
 pub fn stop(&mut self)->Result<(),DispatchError>{if self.state!=PnpState::Started||self.outstanding_irps!=0{return Err(DispatchError::RequestsOutstanding)}self.state=PnpState::Stopped;Ok(())}
 pub fn query_remove(&mut self)->Result<(),DispatchError>{if self.state!=PnpState::Started{return Err(DispatchError::InvalidTransition)}if self.child_count!=0||self.outstanding_irps!=0{return Err(DispatchError::ChildrenPresent)}self.state=PnpState::RemovePending;Ok(())}
 pub fn attach_child(&mut self)->Result<(),DispatchError>{if self.state!=PnpState::Started{return Err(DispatchError::InvalidTransition)}self.child_count=self.child_count.checked_add(1).ok_or(DispatchError::ChildrenPresent)?;Ok(())}
 pub fn detach_child(&mut self)->Result<(),DispatchError>{if self.child_count==0{return Err(DispatchError::InvalidTransition)}self.child_count-=1;Ok(())}
 pub fn surprise_remove(&mut self){self.surprise_removed=true;self.state=PnpState::SurpriseRemoved;}
 pub fn remove(&mut self)->Result<(),DispatchError>{if self.child_count!=0||self.outstanding_irps!=0||self.references>1{return Err(DispatchError::RequestsOutstanding)}self.state=PnpState::Removed;Ok(())}
 pub fn acquire_irp(&mut self)->Result<(),DispatchError>{if matches!(self.state,PnpState::RemovePending|PnpState::SurpriseRemoved|PnpState::Removed){return Err(DispatchError::Removing)}self.outstanding_irps=self.outstanding_irps.checked_add(1).ok_or(DispatchError::RequestsOutstanding)?;Ok(())}
 pub fn release_irp(&mut self){self.outstanding_irps=self.outstanding_irps.saturating_sub(1);}
 pub fn reference(&mut self)->Result<(),DispatchError>{if self.state==PnpState::Removed{return Err(DispatchError::Removing)}self.references=self.references.checked_add(1).ok_or(DispatchError::RequestsOutstanding)?;Ok(())}
 pub fn dereference(&mut self){self.references=self.references.saturating_sub(1).max(1);}
}
#[repr(C)] #[derive(Clone,Copy,Debug,Default)]
pub struct UrbHeader{pub length:u16,pub function:u16,pub status:u32,pub usbd_device_handle:u64,pub usbd_flags:u32}
pub const URB_HEADER_SIZE:usize=core::mem::size_of::<UrbHeader>();
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum UrbError{Null,TooShort,TooLarge,UnsupportedFunction}
pub fn validate_urb_header(h:Option<&UrbHeader>,available:usize,max:usize)->Result<UrbHeader,UrbError>{
 let h=h.copied().ok_or(UrbError::Null)?;let n=h.length as usize;
 if n<URB_HEADER_SIZE||available<URB_HEADER_SIZE{return Err(UrbError::TooShort)}
 if n>available||n>max{return Err(UrbError::TooLarge)}
 match h.function{URB_FUNCTION_CONTROL_TRANSFER|URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER|URB_FUNCTION_ISOCH_TRANSFER|URB_FUNCTION_GET_DESCRIPTOR_FROM_DEVICE|URB_FUNCTION_GET_STATUS_FROM_DEVICE|URB_FUNCTION_VENDOR_DEVICE|URB_FUNCTION_CLASS_DEVICE|URB_FUNCTION_SELECT_CONFIGURATION|URB_FUNCTION_SELECT_INTERFACE|URB_FUNCTION_ABORT_PIPE|URB_FUNCTION_SYNC_RESET_PIPE_AND_CLEAR_STALL|URB_FUNCTION_SYNC_RESET_PIPE|URB_FUNCTION_SYNC_CLEAR_STALL=>Ok(h),_=>Err(UrbError::UnsupportedFunction)}
}
pub type DispatchRoutine=unsafe extern "win64" fn(device:u64,irp:u64)->NTSTATUS;
#[derive(Clone,Copy)]
pub struct DispatchTable{pub create:Option<DispatchRoutine>,pub close:Option<DispatchRoutine>,pub read:Option<DispatchRoutine>,pub write:Option<DispatchRoutine>,pub flush:Option<DispatchRoutine>,pub shutdown:Option<DispatchRoutine>,pub cleanup:Option<DispatchRoutine>,pub pnp:Option<DispatchRoutine>,pub power:Option<DispatchRoutine>,pub internal_ioctl:Option<DispatchRoutine>,pub device_ioctl:Option<DispatchRoutine>,pub system_control:Option<DispatchRoutine>}
impl DispatchTable{
 pub const fn empty()->Self{Self{create:None,close:None,read:None,write:None,flush:None,shutdown:None,cleanup:None,pnp:None,power:None,internal_ioctl:None,device_ioctl:None,system_control:None}}
 pub fn route(&self,major:u8)->Option<DispatchRoutine>{match major{IRP_MJ_CREATE=>self.create,IRP_MJ_CLOSE=>self.close,IRP_MJ_READ=>self.read,IRP_MJ_WRITE=>self.write,IRP_MJ_FLUSH_BUFFERS=>self.flush,IRP_MJ_SHUTDOWN=>self.shutdown,IRP_MJ_CLEANUP=>self.cleanup,IRP_MJ_PNP=>self.pnp,IRP_MJ_POWER=>self.power,IRP_MJ_INTERNAL_DEVICE_CONTROL=>self.internal_ioctl,IRP_MJ_DEVICE_CONTROL=>self.device_ioctl,IRP_MJ_SYSTEM_CONTROL=>self.system_control,_=>None}}
}
pub unsafe fn dispatch(t:&DispatchTable,major:u8,device:u64,irp:u64)->NTSTATUS{if device==0||irp==0{return STATUS_INVALID_PARAMETER}match t.route(major){Some(f)=>unsafe{f(device,irp)},None=>STATUS_INVALID_DEVICE_REQUEST}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum PowerState{D0,D1,D2,D3}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum PowerError{InvalidTransition,RequestsOutstanding,StaleRequest}
pub struct PowerGate{state:PowerState,generation:AtomicU32,accepting:bool,outstanding:u32}
impl PowerGate{
 pub const fn new()->Self{Self{state:PowerState::D3,generation:AtomicU32::new(1),accepting:false,outstanding:0}}
 pub fn state(&self)->PowerState{self.state}
 pub fn begin_io(&mut self)->Result<u32,PowerError>{if !self.accepting{return Err(PowerError::InvalidTransition)}self.outstanding=self.outstanding.checked_add(1).ok_or(PowerError::RequestsOutstanding)?;Ok(self.generation.load(Ordering::Acquire))}
 pub fn finish_io(&mut self,g:u32)->Result<(),PowerError>{if g!=self.generation.load(Ordering::Acquire){return Err(PowerError::StaleRequest)}self.outstanding=self.outstanding.saturating_sub(1);Ok(())}
 pub fn transition(&mut self,next:PowerState)->Result<(),PowerError>{if next==self.state{return Ok(())}if next==PowerState::D0{self.state=next;self.accepting=true;self.generation.fetch_add(1,Ordering::AcqRel);return Ok(())}self.accepting=false;if self.outstanding!=0{return Err(PowerError::RequestsOutstanding)}self.state=next;self.generation.fetch_add(1,Ordering::AcqRel);Ok(())}
}
#[cfg(test)] mod tests{use super::*;#[test]fn pnp_rundown(){let h=DeviceHandle{id:1,generation:1,kind:DeviceKind::ExternalHub};let mut n=DeviceNode::new(h,None);n.start().unwrap();n.attach_child().unwrap();assert!(n.query_remove().is_err());n.detach_child().unwrap();n.acquire_irp().unwrap();assert!(n.query_remove().is_err());n.release_irp();n.query_remove().unwrap();assert!(n.remove().is_ok());}
#[test]fn urb_validation(){let h=UrbHeader{length:URB_HEADER_SIZE as u16,function:URB_FUNCTION_CONTROL_TRANSFER,..UrbHeader::default()};assert!(validate_urb_header(Some(&h),URB_HEADER_SIZE,4096).is_ok());assert_eq!(validate_urb_header(Some(&h),4,4096),Err(UrbError::TooShort));}
#[test]fn power_rundown(){let mut p=PowerGate::new();p.transition(PowerState::D0).unwrap();let g=p.begin_io().unwrap();assert_eq!(p.transition(PowerState::D3),Err(PowerError::RequestsOutstanding));p.finish_io(g).unwrap();assert!(p.transition(PowerState::D3).is_ok());}}

//! Fixed-capacity USB bulk-stream object lifetime and cancellation.
#![no_std]
use super::{EndpointHandle,EndpointQuiesced,StreamHandle,UcxStatus};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum StreamState{Configured,Running,Aborting,Removed}
#[derive(Clone,Copy)]
struct Record{used:bool,handle:StreamHandle,state:StreamState,outstanding:u16,references:u16,max_transfer:u32}
impl Record{const EMPTY:Self=Self{used:false,handle:StreamHandle{id:0,generation:0,endpoint:EndpointHandle{id:0,generation:0,device:super::DeviceHandle{id:0,generation:0,controller:super::ControllerHandle{id:0,generation:0}}}},state:StreamState::Removed,outstanding:0,references:0,max_transfer:0};}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum StreamError{InvalidEndpoint,InvalidStream,InvalidState,QueueFull,Busy,InvalidLength}
pub struct StreamTable<const N:usize>{records:[Record;N],next_generation:u32}
impl<const N:usize> StreamTable<N>{
 pub const fn new()->Self{Self{records:[Record::EMPTY;N],next_generation:1}}
 fn find(&self,h:StreamHandle)->Result<usize,StreamError>{self.records.iter().position(|r|r.used&&r.handle==h).ok_or(StreamError::InvalidStream)}
 /// Streams are defined only for endpoints whose descriptor is bulk and whose
 /// xHCI endpoint context was configured with a nonzero MaxPStreams value.
 pub fn create(&mut self,endpoint:EndpointHandle,bulk_endpoint:bool,stream_id:u16,max_transfer:u32)->Result<StreamHandle,StreamError>{
  if !bulk_endpoint||stream_id==0{return Err(StreamError::InvalidEndpoint)}
  if max_transfer==0{return Err(StreamError::InvalidLength)}
  let i=self.records.iter().position(|r|!r.used).ok_or(StreamError::QueueFull)?;
  let h=StreamHandle{id:stream_id as u32,generation:self.next_generation,endpoint};self.next_generation=self.next_generation.wrapping_add(1).max(1);
  self.records[i]=Record{used:true,handle:h,state:StreamState::Configured,outstanding:0,references:1,max_transfer};Ok(h)
 }
 pub fn start(&mut self,h:StreamHandle)->Result<(),StreamError>{let i=self.find(h)?;if self.records[i].state!=StreamState::Configured{return Err(StreamError::InvalidState)}self.records[i].state=StreamState::Running;Ok(())}
 pub fn acquire(&mut self,h:StreamHandle)->Result<(),StreamError>{let i=self.find(h)?;if self.records[i].state==StreamState::Removed{return Err(StreamError::InvalidState)}self.records[i].references=self.records[i].references.checked_add(1).ok_or(StreamError::Busy)?;Ok(())}
 pub fn release(&mut self,h:StreamHandle)->Result<(),StreamError>{let i=self.find(h)?;self.records[i].references=self.records[i].references.saturating_sub(1).max(1);Ok(())}
 pub fn submit(&mut self,h:StreamHandle,length:u32)->Result<(),StreamError>{let i=self.find(h)?;let r=&mut self.records[i];if r.state!=StreamState::Running{return Err(StreamError::InvalidState)}if length==0||length>r.max_transfer{return Err(StreamError::InvalidLength)}r.outstanding=r.outstanding.checked_add(1).ok_or(StreamError::Busy)?;Ok(())}
 pub fn complete(&mut self,h:StreamHandle)->Result<(),StreamError>{let i=self.find(h)?;let r=&mut self.records[i];if r.outstanding==0{return Err(StreamError::InvalidState)}r.outstanding-=1;Ok(())}
 pub fn begin_abort(&mut self,h:StreamHandle)->Result<(),StreamError>{let i=self.find(h)?;if self.records[i].state!=StreamState::Running{return Err(StreamError::InvalidState)}self.records[i].state=StreamState::Aborting;Ok(())}
 pub fn finish_abort(&mut self,h:StreamHandle,_q:&EndpointQuiesced)->Result<(),StreamError>{let i=self.find(h)?;if self.records[i].state!=StreamState::Aborting||self.records[i].outstanding!=0{return Err(StreamError::Busy)}self.records[i].state=StreamState::Configured;Ok(())}
 pub fn remove(&mut self,h:StreamHandle,_q:&EndpointQuiesced)->Result<(),StreamError>{let i=self.find(h)?;let r=&mut self.records[i];if r.outstanding!=0||r.references!=1{return Err(StreamError::Busy)}r.state=StreamState::Removed;r.used=false;Ok(())}
 pub fn outstanding(&self,h:StreamHandle)->Result<u16,StreamError>{Ok(self.records[self.find(h)?].outstanding)}
}
impl<const N:usize> Default for StreamTable<N>{fn default()->Self{Self::new()}}
#[cfg(test)]mod tests{use super::*;#[test]fn stream_requires_bulk_and_quiescence(){let ep=EndpointHandle{id:2,generation:1,device:super::super::DeviceHandle{id:1,generation:1,controller:super::super::ControllerHandle{id:1,generation:1}}};let mut t=StreamTable::<4>::new();assert_eq!(t.create(ep,false,1,512),Err(StreamError::InvalidEndpoint));let h=t.create(ep,true,1,512).unwrap();t.start(h).unwrap();t.submit(h,128).unwrap();t.begin_abort(h).unwrap();let q=unsafe{EndpointQuiesced::after_hardware_stop_and_dma_fence()};assert_eq!(t.finish_abort(h,&q),Err(StreamError::Busy));t.complete(h).unwrap();t.finish_abort(h,&q).unwrap();t.remove(h,&q).unwrap();assert_eq!(t.outstanding(h),Err(StreamError::InvalidStream));}}

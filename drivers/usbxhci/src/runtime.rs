//! Bounded xHCI transfer submission and deferred completion engine.
#![no_std]
use crate::{ProducerRing,Trb,TransferError,TransferRequest,TransferResult,TransferTracker,TRB_TYPE_NORMAL,TRB_TYPE_SHIFT,TRB_IOC,TRB_TRANSFER_LENGTH_MASK,TRB_TYPE_TRANSFER_EVENT};
use core::sync::atomic::{fence,Ordering};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RuntimeError{InvalidRequest,QueueFull,Ring(crate::XhciError),Transfer(TransferError),PublishFailed,DoorbellFailed,StaleCompletion}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct TransferToken{pub request_id:u64,pub trb_dma:u64,pub slot_id:u8,pub endpoint_id:u8,pub generation:u32}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct RuntimeCompletion{pub token:TransferToken,pub result:TransferResult,pub generation:u32}
pub trait TransferHardware{
 fn publish_trb(&mut self,index:usize,trb_dma:u64,trb:Trb)->Result<(),()>;
 fn ring_endpoint_doorbell(&mut self,slot:u8,endpoint:u8)->Result<(),()>;
}
#[derive(Clone,Copy)]
struct Pending{used:bool,token:TransferToken}
impl Pending{const EMPTY:Self=Self{used:false,token:TransferToken{request_id:0,trb_dma:0,slot_id:0,endpoint_id:0,generation:0}};}
/// Caller serializes submit and DPC completion with the endpoint's spin lock.
pub struct TransferEngine<const R:usize,const N:usize>{ring:ProducerRing<N>,tracker:TransferTracker<R>,pending:[Pending;R],next_id:u64,generation:u32,stopped:bool}
impl<const R:usize,const N:usize> TransferEngine<R,N>{
 pub const fn new(ring_dma:u64,generation:u32)->Self{Self{ring:ProducerRing::new_with_dma_base(ring_dma),tracker:TransferTracker::new(),pending:[Pending::EMPTY;R],next_id:1,generation,stopped:false}}
 pub fn generation(&self)->u32{self.generation}
 pub fn outstanding(&self)->usize{self.tracker.len()}
 pub fn stop_submissions(&mut self){self.stopped=true;}
 pub fn restart(&mut self,generation:u32){self.generation=generation;self.stopped=false;}
 pub fn submit<H:TransferHardware>(&mut self,hw:&mut H,trb_dma:u64,buffer_dma:u64,length:u32,slot:u8,endpoint:u8)->Result<TransferToken,RuntimeError>{
  if self.stopped||R==0||length==0||length>TRB_TRANSFER_LENGTH_MASK||slot==0||endpoint==0||trb_dma==0||trb_dma&15!=0||buffer_dma==0||buffer_dma&15!=0{return Err(RuntimeError::InvalidRequest)}
  let p=self.pending.iter().position(|p|!p.used).ok_or(RuntimeError::QueueFull)?;
  let token=TransferToken{request_id:self.next_id,trb_dma,slot_id:slot,endpoint_id:endpoint,generation:self.generation};
  let req=TransferRequest{trb_dma,buffer_dma,length,endpoint_id:endpoint,slot_id:slot};
  self.tracker.reserve(req).map_err(RuntimeError::Transfer)?;
  let trb=Trb{parameter:buffer_dma,status:length,control:(TRB_TYPE_NORMAL<<TRB_TYPE_SHIFT)|TRB_IOC|((endpoint as u32)<<16)|((slot as u32)<<24)};
  let index=self.ring.push(trb).map_err(RuntimeError::Ring)?;
  if hw.publish_trb(index,trb_dma,trb).is_err(){return Err(RuntimeError::PublishFailed)}
  self.pending[p]=Pending{used:true,token};
  fence(Ordering::Release);
  if hw.ring_endpoint_doorbell(slot,endpoint).is_err(){return Err(RuntimeError::DoorbellFailed)}
  self.next_id=self.next_id.wrapping_add(1).max(1);Ok(token)
 }
 /// ISR acknowledges the interrupt; this method runs later at DPC level.
 pub fn complete_dpc(&mut self,event:&Trb)->Result<RuntimeCompletion,RuntimeError>{
  if event.trb_type()!=TRB_TYPE_TRANSFER_EVENT as u8{return Err(RuntimeError::StaleCompletion)}
  let i=self.pending.iter().position(|p|p.used&&p.token.trb_dma==event.parameter).ok_or(RuntimeError::StaleCompletion)?;
  let token=self.pending[i].token;
  if token.generation!=self.generation||token.slot_id!=event.slot_id()||token.endpoint_id!=event.endpoint_id(){return Err(RuntimeError::StaleCompletion)}
  let result=self.tracker.complete(event).map_err(RuntimeError::Transfer)?;
  self.pending[i]=Pending::EMPTY;Ok(RuntimeCompletion{token,result,generation:self.generation})
 }
 /// Called only after Stop Endpoint/Disable Slot, DPC synchronization and the
 /// requester-scoped IOMMU unmap/invalidation have all succeeded.
 pub fn retire_slot_after_quiescence(&mut self,slot:u8,new_generation:u32)->usize{
  let mut count=0;for p in self.pending.iter_mut(){if p.used&&p.token.slot_id==slot{*p=Pending::EMPTY;count+=1;}}
  self.generation=new_generation;count
 }
}

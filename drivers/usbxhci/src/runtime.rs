//! Bounded xHCI transfer submission and deferred completion engine.
#![no_std]
use crate::{ProducerRing,Trb,TransferError,TransferRequest,TransferResult,TransferTracker,TRB_TYPE_NORMAL,TRB_TYPE_SHIFT,TRB_IOC,TRB_TRANSFER_LENGTH_MASK,TRB_TYPE_TRANSFER_EVENT};
use core::sync::atomic::{fence,Ordering};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RuntimeError{InvalidRequest,QueueFull,Busy,Ring(crate::XhciError),Transfer(TransferError),PublishFailed,DoorbellFailed,StaleCompletion}

/// Proof that controller-wide reset/stop has completed, bus mastering is
/// disabled, and ISR/DPC processing can no longer touch the event or transfer rings.
pub struct ControllerDmaQuiesced { _private:() }
impl ControllerDmaQuiesced {
 /// # Safety
 /// Caller must establish controller halt, PCI bus-master disable, device DMA
 /// revocation/IOMMU invalidation, and interrupt/DPC synchronization.
 pub unsafe fn after_controller_halt_and_dma_fence()->Self{Self{_private:()}}
}
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
pub struct TransferEngine<const R:usize,const N:usize>{ring:ProducerRing<N>,ring_dma:u64,tracker:TransferTracker<R>,pending:[Pending;R],next_id:u64,generation:u32,stopped:bool}
impl<const R:usize,const N:usize> TransferEngine<R,N>{
 pub const fn new(ring_dma:u64,generation:u32)->Self{Self{ring:ProducerRing::new_with_dma_base(ring_dma),ring_dma,tracker:TransferTracker::new(),pending:[Pending::EMPTY;R],next_id:1,generation,stopped:false}}
 pub fn generation(&self)->u32{self.generation}
 pub fn outstanding(&self)->usize{self.tracker.len()}
 pub fn stop_submissions(&mut self){self.stopped=true;}
 pub fn restart(&mut self,generation:u32,_q:&ControllerDmaQuiesced)->Result<(),RuntimeError>{
  if self.tracker.len()!=0{return Err(RuntimeError::Busy)}
  self.pending=[Pending::EMPTY;R];self.generation=generation;self.stopped=false;Ok(())
 }
 pub fn submit<H:TransferHardware>(&mut self,hw:&mut H,buffer_dma:u64,length:u32,slot:u8,endpoint:u8,max_device_address:u64)->Result<TransferToken,RuntimeError>{
  if self.stopped||R==0||N<4||!N.is_power_of_two()||self.ring_dma==0||self.ring_dma&63!=0||self.tracker.len()>=N-1||length==0||length>TRB_TRANSFER_LENGTH_MASK||slot==0||endpoint==0||buffer_dma==0||buffer_dma&15!=0{return Err(RuntimeError::InvalidRequest)}
  let p=self.pending.iter().position(|p|!p.used).ok_or(RuntimeError::QueueFull)?;
  let index_expected=self.ring.producer_index();
  let trb_dma=self.ring_dma.checked_add((index_expected as u64)*16).ok_or(RuntimeError::InvalidRequest)?;
  if self.pending.iter().any(|p|p.used&&p.token.trb_dma==trb_dma){return Err(RuntimeError::QueueFull)}
  crate::validate_dma_span(self.ring_dma,(N as u64)*16,max_device_address).map_err(|_|RuntimeError::InvalidRequest)?;
  let token=TransferToken{request_id:self.next_id,trb_dma,slot_id:slot,endpoint_id:endpoint,generation:self.generation};
  crate::validate_dma_span(buffer_dma,length as u64,max_device_address).map_err(|_|RuntimeError::InvalidRequest)?;
  let req=TransferRequest{trb_dma,buffer_dma,length,endpoint_id:endpoint,slot_id:slot};
  self.tracker.reserve(req).map_err(RuntimeError::Transfer)?;
  let trb=Trb{parameter:buffer_dma,status:length,control:(TRB_TYPE_NORMAL<<TRB_TYPE_SHIFT)|TRB_IOC|((endpoint as u32)<<16)|((slot as u32)<<24)};
  let index=self.ring.push(trb).map_err(RuntimeError::Ring)?;
  if index!=index_expected{self.stopped=true;return Err(RuntimeError::InvalidRequest)}
  let published=self.ring.get(index).ok_or(RuntimeError::InvalidRequest)?;
  if hw.publish_trb(index,trb_dma,published).is_err(){self.stopped=true;return Err(RuntimeError::PublishFailed)}
  if index==N-2{
   let link=self.ring.get(N-1).ok_or(RuntimeError::InvalidRequest)?;
   let link_dma=self.ring_dma.checked_add(((N-1) as u64)*16).ok_or(RuntimeError::InvalidRequest)?;
   if hw.publish_trb(N-1,link_dma,link).is_err(){self.stopped=true;return Err(RuntimeError::PublishFailed)}
  }
  self.pending[p]=Pending{used:true,token};
  fence(Ordering::Release);
  if hw.ring_endpoint_doorbell(slot,endpoint).is_err(){self.stopped=true;return Err(RuntimeError::DoorbellFailed)}
  self.next_id=self.next_id.wrapping_add(1).max(1);Ok(token)
 }
 /// ISR acknowledges the interrupt; this method runs later at DPC level.
 pub fn complete_dpc(&mut self,event:&Trb)->Result<RuntimeCompletion,RuntimeError>{
  if event.trb_type()!=TRB_TYPE_TRANSFER_EVENT as u8{return Err(RuntimeError::StaleCompletion)}
  let i=self.pending.iter().position(|p|p.used&&p.token.trb_dma==event.parameter).ok_or(RuntimeError::StaleCompletion)?;
  let token=self.pending[i].token;
  if token.generation!=self.generation||token.slot_id!=event.slot_id()||token.endpoint_id!=event.endpoint_id(){return Err(RuntimeError::StaleCompletion)}
  let completed=self.tracker.complete(event);
  self.pending[i]=Pending::EMPTY;
  let result=completed.map_err(RuntimeError::Transfer)?;
  Ok(RuntimeCompletion{token,result,generation:self.generation})
 }
 /// Called only after Stop Endpoint/Disable Slot, DPC synchronization and the
 /// requester-scoped IOMMU unmap/invalidation have all succeeded.
 pub fn retire_slot_after_quiescence(&mut self,slot:u8,q:&crate::SlotDmaQuiesced)->usize{
  let count=self.tracker.cancel_slot(slot,q);
  for p in self.pending.iter_mut(){if p.used&&p.token.slot_id==slot{*p=Pending::EMPTY;}}
  count
 }
}

#[cfg(test)]
mod tests{
 use super::*;
 struct Mock{published:[Trb;4],doorbells:u32}
 impl TransferHardware for Mock{
  fn publish_trb(&mut self,index:usize,_dma:u64,trb:Trb)->Result<(),()>{self.published[index]=trb;Ok(())}
  fn ring_endpoint_doorbell(&mut self,_slot:u8,_ep:u8)->Result<(),()>{self.doorbells+=1;Ok(())}
 }
 fn event(token:TransferToken)->Trb{Trb{parameter:token.trb_dma,status:1u32<<24,control:(TRB_TYPE_TRANSFER_EVENT<<TRB_TYPE_SHIFT)|((token.endpoint_id as u32)<<16)|((token.slot_id as u32)<<24)}}
 #[test]fn submission_publishes_link_and_dpc_completes_once(){
  let mut e=TransferEngine::<4,4>::new(0x1000,1);
  let mut hw=Mock{published:[Trb::default();4],doorbells:0};
  let a=e.submit(&mut hw,0x8000,8,1,3,0xffff).unwrap();
  let b=e.submit(&mut hw,0x8010,8,1,3,0xffff).unwrap();
  let c=e.submit(&mut hw,0x8020,8,1,3,0xffff).unwrap();
  assert_eq!(hw.published[3].trb_type(),crate::TRB_TYPE_LINK as u8);
  assert_eq!(hw.doorbells,3);
  let done=e.complete_dpc(&event(c)).unwrap();assert_eq!(done.token,c);assert_eq!(done.result.bytes_transferred,8);
  assert_eq!(e.complete_dpc(&event(c)),Err(RuntimeError::StaleCompletion));
  assert_eq!(e.complete_dpc(&event(a)).unwrap().token,a);
  assert_eq!(e.complete_dpc(&event(b)).unwrap().token,b);
 }
 #[test]fn ring_slot_is_not_reused_until_that_slot_completes(){
  let mut e=TransferEngine::<4,4>::new(0x2000,1);
  let mut hw=Mock{published:[Trb::default();4],doorbells:0};
  let a=e.submit(&mut hw,0x8000,8,1,3,0xffff).unwrap();
  let _b=e.submit(&mut hw,0x8010,8,1,3,0xffff).unwrap();
  let _c=e.submit(&mut hw,0x8020,8,1,3,0xffff).unwrap();
  e.complete_dpc(&event(_c)).unwrap();
  assert_eq!(e.submit(&mut hw,0x8030,8,1,3,0xffff),Err(RuntimeError::QueueFull));
  e.complete_dpc(&event(a)).unwrap();
  assert!(e.submit(&mut hw,0x8030,8,1,3,0xffff).is_ok());
 }
}

//! Kernel-owned transactional resource arbitration for PnP bus drivers.
//!
//! Bus drivers publish requirements; the kernel arbiter selects a complete
//! alternative and reserves all resources atomically. Firmware programming
//! (for example ACPI _SRS) remains the bus driver's responsibility.
#![no_std]

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub const MAX_RESERVATIONS: usize = 256;
pub const MAX_REQUESTS_PER_ALTERNATIVE: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ResourceKind { Port = 1, Memory = 2, Interrupt = 3, Dma = 4, BusNumber = 5, SharedInterrupt = 6 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceRequest {
    pub kind: ResourceKind,
    /// Inclusive minimum and maximum permitted resource base.
    pub minimum: u64,
    pub maximum: u64,
    pub length: u64,
    pub alignment: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceAssignment {
    pub kind: ResourceKind,
    pub start: u64,
    pub length: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArbiterError {
    InvalidOwner, InvalidRange, InvalidAlignment, TooManyRequests,
    Capacity, Conflict, OwnerAlreadyReserved, OwnerNotFound,
}
#[derive(Clone, Copy)]
struct Reservation { owner:u64, assignment:ResourceAssignment }
impl Reservation {
    const EMPTY:Self=Self{owner:0,assignment:ResourceAssignment{kind:ResourceKind::Port,start:0,length:0}};
}
struct State { entries:[Option<Reservation>;MAX_RESERVATIONS], count:usize }
impl State { const fn new()->Self{Self{entries:[None;MAX_RESERVATIONS],count:0}} }

/// Fixed-capacity arbiter. The lock covers planning and commit so two CPUs
/// cannot both reserve the same range. Failed requests leave state unchanged.
pub struct ResourceArbiter { lock:AtomicBool, state:UnsafeCell<State> }
unsafe impl Sync for ResourceArbiter {}
impl ResourceArbiter {
    pub const fn new()->Self{Self{lock:AtomicBool::new(false),state:UnsafeCell::new(State::new())}}
    fn acquire(&self)->Guard<'_>{
        while self.lock.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}
        Guard{arbiter:self}
    }
    /// Select and reserve the first complete alternative that fits. Each
    /// alternative is a slice of requests; the output is written only after
    /// the whole alternative has been validated and committed.
    pub fn reserve_alternative(
        &self, owner:u64, alternatives:&[&[ResourceRequest]], output:&mut [ResourceAssignment],
    )->Result<usize,ArbiterError>{
        if owner==0{return Err(ArbiterError::InvalidOwner)}
        let _guard=self.acquire();
        let state=unsafe{&mut *self.state.get()};
        if state.entries.iter().flatten().any(|r|r.owner==owner){return Err(ArbiterError::OwnerAlreadyReserved)}
        for alternative in alternatives {
            if alternative.is_empty(){continue}
            if alternative.len()>MAX_REQUESTS_PER_ALTERNATIVE||alternative.len()>output.len(){return Err(ArbiterError::TooManyRequests)}
            let mut planned=[ResourceAssignment{kind:ResourceKind::Port,start:0,length:0};MAX_REQUESTS_PER_ALTERNATIVE];
            let mut valid=true;
            for (index,request) in alternative.iter().enumerate(){
                let Some(assignment)=Self::find_candidate(state,owner,&planned[..index],request)? else{valid=false;break};
                planned[index]=assignment;
            }
            if !valid{continue}
            if state.count+alternative.len()>MAX_RESERVATIONS{return Err(ArbiterError::Capacity)}
            for assignment in planned[..alternative.len()].iter().copied(){
                let slot=state.entries.iter_mut().find(|e|e.is_none()).ok_or(ArbiterError::Capacity)?;
                *slot=Some(Reservation{owner,assignment});state.count+=1;
            }
            output[..alternative.len()].copy_from_slice(&planned[..alternative.len()]);
            return Ok(alternative.len())
        }
        Err(ArbiterError::Conflict)
    }
    fn find_candidate(state:&State,owner:u64,planned:&[ResourceAssignment],r:&ResourceRequest)->Result<Option<ResourceAssignment>,ArbiterError>{
        if r.length==0||r.minimum>r.maximum{return Err(ArbiterError::InvalidRange)}
        if r.alignment==0||!r.alignment.is_power_of_two(){return Err(ArbiterError::InvalidAlignment)}
        // Interrupts, DMA channels and bus numbers are discrete resources.
        let alignment=if matches!(r.kind,ResourceKind::Interrupt|ResourceKind::SharedInterrupt|ResourceKind::Dma|ResourceKind::BusNumber){1}else{r.alignment};
        let mut candidate=r.minimum.checked_add(alignment-1).ok_or(ArbiterError::InvalidRange)?&!(alignment-1);
        while candidate<=r.maximum{
            let end=candidate.checked_add(r.length).ok_or(ArbiterError::InvalidRange)?;
            let overlaps=|a:ResourceAssignment|->bool{
                let shared_pair = a.kind==ResourceKind::SharedInterrupt
                    && r.kind==ResourceKind::SharedInterrupt;
                let interrupt_pair = matches!(a.kind,ResourceKind::Interrupt|ResourceKind::SharedInterrupt)
                    && matches!(r.kind,ResourceKind::Interrupt|ResourceKind::SharedInterrupt);
                if shared_pair && candidate==a.start { return false; }
                if !interrupt_pair && a.kind!=r.kind { return false; }
                let ae=a.start.saturating_add(a.length);
                candidate<ae&&a.start<end
            };
            let conflict=state.entries.iter().flatten().any(|entry|entry.owner!=owner&&overlaps(entry.assignment))
                ||planned.iter().copied().any(overlaps);
            if !conflict{return Ok(Some(ResourceAssignment{kind:r.kind,start:candidate,length:r.length}))}
            candidate=candidate.checked_add(alignment).ok_or(ArbiterError::InvalidRange)?;
        }
        Ok(None)
    }
    /// Release every reservation belonging to a device. Safe to call during
    /// failed-start rollback and surprise removal; a second release is an error.
    pub fn release_owner(&self,owner:u64)->Result<usize,ArbiterError>{
        if owner==0{return Err(ArbiterError::InvalidOwner)}
        let _guard=self.acquire();let state=unsafe{&mut *self.state.get()};let mut removed=0;
        for entry in state.entries.iter_mut(){if entry.as_ref().is_some_and(|r|r.owner==owner){*entry=None;removed+=1;}}
        if removed==0{return Err(ArbiterError::OwnerNotFound)}
        state.count-=removed;Ok(removed)
    }
    pub fn reservation_count(&self)->usize{let _guard=self.acquire();unsafe{(*self.state.get()).count}}
}
impl Default for ResourceArbiter{fn default()->Self{Self::new()}}
struct Guard<'a>{arbiter:&'a ResourceArbiter}
impl Drop for Guard<'_>{fn drop(&mut self){self.arbiter.lock.store(false,Ordering::Release)}}

/// System-wide arbiter used by PnP resource assignment paths.
pub static SYSTEM_RESOURCE_ARBITER:ResourceArbiter=ResourceArbiter::new();

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn selects_second_alternative_when_first_conflicts(){
  let arb=ResourceArbiter::new();let mut a=[ResourceAssignment{kind:ResourceKind::Port,start:0,length:0};2];
  let first=[ResourceRequest{kind:ResourceKind::Port,minimum:0x100,maximum:0x100,length:8,alignment:8}];
  let second=[ResourceRequest{kind:ResourceKind::Port,minimum:0x200,maximum:0x200,length:8,alignment:8}];
  assert_eq!(arb.reserve_alternative(1,&[&first],&mut a),Ok(1));
  assert_eq!(arb.reserve_alternative(2,&[&first,&second],&mut a),Ok(1));
  assert_eq!(a[0].start,0x200);
 }
 #[test]fn failed_multi_resource_request_rolls_back_everything(){
  let arb=ResourceArbiter::new();let mut out=[ResourceAssignment{kind:ResourceKind::Port,start:9,length:9};2];
  let busy=[ResourceRequest{kind:ResourceKind::Interrupt,minimum:5,maximum:5,length:1,alignment:1}];
  let pair=[ResourceRequest{kind:ResourceKind::Port,minimum:0x300,maximum:0x300,length:8,alignment:8},ResourceRequest{kind:ResourceKind::Interrupt,minimum:5,maximum:5,length:1,alignment:1}];
  assert_eq!(arb.reserve_alternative(1,&[&busy],&mut out),Ok(1));
  assert_eq!(arb.reserve_alternative(2,&[&pair],&mut out),Err(ArbiterError::Conflict));
  assert_eq!(arb.reservation_count(),1);assert_eq!(out[0].start,9);
 }
 #[test]fn release_makes_range_available_again(){
  let arb=ResourceArbiter::new();let req=[ResourceRequest{kind:ResourceKind::Memory,minimum:0x1000,maximum:0x1000,length:0x100,alignment:0x100}];
  let mut out=[ResourceAssignment{kind:ResourceKind::Memory,start:0,length:0};1];
  assert_eq!(arb.reserve_alternative(7,&[&req],&mut out),Ok(1));
  assert_eq!(arb.release_owner(7),Ok(1));assert_eq!(arb.reserve_alternative(8,&[&req],&mut out),Ok(1));
 }
 #[test]fn rejects_overflow_and_bad_alignment(){
  let arb=ResourceArbiter::new();let mut out=[ResourceAssignment{kind:ResourceKind::Port,start:0,length:0};1];
  let overflow=[ResourceRequest{kind:ResourceKind::Port,minimum:u64::MAX-3,maximum:u64::MAX-3,length:8,alignment:1}];
  assert_eq!(arb.reserve_alternative(1,&[&overflow],&mut out),Err(ArbiterError::InvalidRange));
  let bad=[ResourceRequest{kind:ResourceKind::Memory,minimum:0,maximum:32,length:4,alignment:3}];
  assert_eq!(arb.reserve_alternative(1,&[&bad],&mut out),Err(ArbiterError::InvalidAlignment));
 }
}

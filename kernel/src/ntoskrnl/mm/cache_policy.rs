//! Physical-page cache-type ownership registry; rejects conflicting aliases.
use core::sync::atomic::{AtomicBool,Ordering};
pub const MAX_CACHE_RANGES:usize=512;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub enum CacheType{Cached=0,NonCached=1,WriteCombined=2}
#[derive(Clone,Copy)]
struct Entry{start:u64,end:u64,kind:CacheType,refs:u32,live:bool}
impl Entry{const EMPTY:Self=Self{start:0,end:0,kind:CacheType::Cached,refs:0,live:false};}
struct Store(core::cell::UnsafeCell<[Entry;MAX_CACHE_RANGES]>);unsafe impl Sync for Store{}
static ENTRIES:Store=Store(core::cell::UnsafeCell::new([Entry::EMPTY;MAX_CACHE_RANGES]));
static LOCK:AtomicBool=AtomicBool::new(false);
pub struct Reservation{start:u64,end:u64,kind:CacheType,active:bool}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum CacheError{InvalidRange,Conflict,Capacity,Busy}
fn lock()->Result<Guard,CacheError>{if LOCK.compare_exchange(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){return Err(CacheError::Busy)}Ok(Guard)}
struct Guard;impl Drop for Guard{fn drop(&mut self){LOCK.store(false,Ordering::Release)}}
/// Reserve an aligned physical interval before installing any alias PTE.
pub fn reserve(start:u64,length:u64,kind:CacheType)->Result<Reservation,CacheError>{
 if length==0||start&0xfff!=0||length&0xfff!=0{return Err(CacheError::InvalidRange)}let end=start.checked_add(length).ok_or(CacheError::InvalidRange)?;let _g=lock()?;let es=unsafe{&mut *ENTRIES.0.get()};
 for e in es.iter().filter(|e|e.live){if start<e.end&&e.start<end&&e.kind!=kind{return Err(CacheError::Conflict)}}
 if let Some(e)=es.iter_mut().find(|e|e.live&&e.start==start&&e.end==end&&e.kind==kind){e.refs=e.refs.checked_add(1).ok_or(CacheError::Capacity)?}else{let e=es.iter_mut().find(|e|!e.live).ok_or(CacheError::Capacity)?;*e=Entry{start,end,kind,refs:1,live:true};}
 Ok(Reservation{start,end,kind,active:true})
}
impl Reservation{pub fn cache_type(&self)->CacheType{self.kind}pub fn range(&self)->(u64,u64){(self.start,self.end)}}
impl Drop for Reservation{fn drop(&mut self){if !self.active{return}if let Ok(_g)=lock(){let es=unsafe{&mut *ENTRIES.0.get()};if let Some(e)=es.iter_mut().find(|e|e.live&&e.start==self.start&&e.end==self.end&&e.kind==self.kind){e.refs=e.refs.saturating_sub(1);if e.refs==0{*e=Entry::EMPTY}}}}}
#[cfg(test)]mod tests{use super::*;#[test]fn conflicting_alias_types_are_rejected(){let a=reserve(0x1000,0x2000,CacheType::NonCached).unwrap();assert_eq!(reserve(0x2000,0x1000,CacheType::WriteCombined).err(),Some(CacheError::Conflict));drop(a);assert!(reserve(0x2000,0x1000,CacheType::WriteCombined).is_ok())}}

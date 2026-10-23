//! Small, bounded non-paged RTL string pool.
//! Independent of Rust's global allocator; blocks are 16-byte aligned, locked,
//! split on allocation, and coalesced on free. Intended for RTL string buffers.
use core::sync::atomic::{AtomicBool, Ordering};
const ARENA_SIZE: usize = 256 * 1024;
const ALIGN: usize = 16;
const NONE: usize = usize::MAX;
const HEADER_SIZE: usize = 32;
#[repr(C, align(16))]
struct Block { size: usize, next: usize, free: usize, _reserved: usize }
static LOCK: AtomicBool = AtomicBool::new(false);
static mut ARENA: [u8; ARENA_SIZE] = [0; ARENA_SIZE];
static mut INITIALIZED: bool = false;
struct Guard;
impl Guard { fn acquire() -> Self { while LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();} Self } }
impl Drop for Guard { fn drop(&mut self){LOCK.store(false,Ordering::Release);} }
#[inline] fn base()->*mut u8 { core::ptr::addr_of_mut!(ARENA).cast::<u8>() }
unsafe fn init_locked(){if unsafe{INITIALIZED}{return;}let h=base().cast::<Block>();unsafe{h.write(Block{size:ARENA_SIZE-HEADER_SIZE,next:NONE,free:1,_reserved:0});INITIALIZED=true;}}
/// Allocate a zero-uninitialized buffer from the bounded RTL pool.
pub(super) fn allocate(size:usize)->*mut u8 {
 if size==0||size>ARENA_SIZE-HEADER_SIZE{return core::ptr::null_mut();}
 let _guard=Guard::acquire();unsafe{init_locked();}
 let wanted=size.saturating_add(ALIGN-1)&!(ALIGN-1);let mut off=0usize;
 while off<ARENA_SIZE {let h=unsafe{&mut *base().add(off).cast::<Block>()};if h.free!=0&&h.size>=wanted{
  let remain=h.size-wanted;if remain>=HEADER_SIZE+ALIGN{let next_off=off+HEADER_SIZE+wanted;let next=unsafe{&mut *base().add(next_off).cast::<Block>()};*next=Block{size:remain-HEADER_SIZE,next:h.next,free:1,_reserved:0};h.next=next_off;h.size=wanted;}h.free=0;return unsafe{base().add(off+HEADER_SIZE)};}
  if h.next==NONE{break;}off=h.next;
 }core::ptr::null_mut()
}
/// Release only pointers owned by this pool; foreign pointers are rejected.
pub(super) fn deallocate(ptr:*mut u8)->bool {
 if ptr.is_null(){return true;}let start=base() as usize;let p=ptr as usize;
 if p<start+HEADER_SIZE||p>=start+ARENA_SIZE||(p-start-HEADER_SIZE)%ALIGN!=0{return false;}
 let _guard=Guard::acquire();unsafe{init_locked();}let target=p-start-HEADER_SIZE;let mut off=0usize;
 while off<ARENA_SIZE{let h=unsafe{&mut *base().add(off).cast::<Block>()};if off==target{if h.free!=0{return false;}h.free=1;let mut cur=0usize;
  while cur<ARENA_SIZE{let c=unsafe{&mut *base().add(cur).cast::<Block>()};if c.next==NONE{break;}let n_off=c.next;let n=unsafe{&mut *base().add(n_off).cast::<Block>()};if c.free!=0&&n.free!=0&&cur+HEADER_SIZE+c.size==n_off{c.size=c.size.saturating_add(HEADER_SIZE).saturating_add(n.size);c.next=n.next;}else{cur=n_off;}}return true;}
  if h.next==NONE{break;}off=h.next;
 }false
}
#[cfg(test)] mod tests {use super::*;#[test]fn pool_splits_and_reuses_freed_blocks(){let a=allocate(64);let b=allocate(128);assert!(!a.is_null()&&!b.is_null()&&a!=b);unsafe{core::ptr::write_bytes(a,0xA5,64);}assert!(deallocate(a));assert!(deallocate(b));let c=allocate(192);assert!(!c.is_null());assert!(deallocate(c));}}

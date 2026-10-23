//! Supervisor-only virtual mappings backed by independently allocated physical pages.
//! This is not a pagefile-backed pager: no eviction/demand paging is claimed.
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
const PAGE: u64 = 4096;
const P: u64 = 1;
const W: u64 = 2;
const PS: u64 = 1 << 7;
const NX: u64 = 1 << 63;
const ADDR: u64 = 0x000f_ffff_ffff_f000;
const BASE: u64 = 0xffff_8000_0000_0000;
const PAGES: usize = 1 << 20;
const WORDS: usize = PAGES / 64;
const MAX: usize = 4096;
// Match the NT pool's current 256 MiB maximum extent. A lock-protected scratch
// array lets free_pages invalidate all CPUs once before recycling any backing frame.
const MAX_EXTENT_PAGES: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { NotInitialized, Invalid, NoVirtualSpace, NoPhysicalMemory, AlreadyMapped, NotMapped, TableFull }
#[derive(Clone, Copy)]
struct Extent { start: usize, pages: usize, live: bool }
impl Extent { const EMPTY: Self = Self { start: 0, pages: 0, live: false }; }
struct Bits(core::cell::UnsafeCell<[u64; WORDS]>); unsafe impl Sync for Bits {}
struct Table(core::cell::UnsafeCell<[Extent; MAX]>); unsafe impl Sync for Table {}
static BITS: Bits = Bits(core::cell::UnsafeCell::new([0; WORDS]));
static TABLE: Table = Table(core::cell::UnsafeCell::new([Extent::EMPTY; MAX]));
static ROOT: AtomicU64 = AtomicU64::new(0);
struct ReclaimPages(core::cell::UnsafeCell<[u64; MAX_EXTENT_PAGES]>); unsafe impl Sync for ReclaimPages {}
static RECLAIM_PAGES: ReclaimPages = ReclaimPages(core::cell::UnsafeCell::new([0; MAX_EXTENT_PAGES]));
// Each parked AP polls this generation because external interrupts are not enabled yet.
static TLB_GENERATION: AtomicU64 = AtomicU64::new(0);
static TLB_ACK: [AtomicU64; crate::hal::MAX_CPUS] = [const { AtomicU64::new(0) }; crate::hal::MAX_CPUS];
#[cfg(target_os = "none")]
static LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
#[cfg(not(target_os = "none"))]
static LOCK: AtomicBool = AtomicBool::new(false);
struct Guard { #[cfg(target_os = "none")] _g: crate::hal::IrqSpinLockGuard<'static, ()> }
impl Guard {
 fn lock() -> Self {
  #[cfg(target_os = "none")] { Self { _g: LOCK.lock() } }
  #[cfg(not(target_os = "none"))] { while LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();} Self{} }
 }
}
#[cfg(not(target_os = "none"))]
impl Drop for Guard { fn drop(&mut self){LOCK.store(false,Ordering::Release);} }

pub fn initialize(root: u64) -> Result<(), Error> {
 if root == 0 || root >= 0x1_0000_0000 || root & (PAGE-1) != 0 { return Err(Error::Invalid); }
 let _g=Guard::lock(); let old=ROOT.load(Ordering::Acquire);
 if old != 0 && old != root { return Err(Error::Invalid); }
 ROOT.store(root,Ordering::Release); Ok(())
}
pub fn is_initialized()->bool { ROOT.load(Ordering::Acquire)!=0 }

/// Called by the interrupt-disabled AP idle loop. A generation change means shared
/// kernel page tables changed; reloading CR3 invalidates this CPU's non-global TLB.
pub fn smp_tlb_poll(logical_id: usize) {
 if logical_id == 0 || logical_id >= crate::hal::MAX_CPUS { return; }
 let generation = TLB_GENERATION.load(Ordering::Acquire);
 if TLB_ACK[logical_id].load(Ordering::Relaxed) != generation {
  unsafe { let cr3: u64; core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)); core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags)); }
  TLB_ACK[logical_id].store(generation, Ordering::Release);
 }
}

/// Invalidate shared kernel translations on every online AP before any backing
/// physical frame can be recycled. APs poll rather than use IPIs while IRQs are off.
fn synchronize_kernel_tlb() {
 if crate::hal::online_cpu_count() <= 1 { return; }
 let generation = TLB_GENERATION.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
 for cpu in 1..crate::hal::MAX_CPUS {
  if crate::hal::cpu_state(cpu).ok() != Some(crate::hal::CPU_ONLINE) { continue; }
  while TLB_ACK[cpu].load(Ordering::Acquire) != generation { core::hint::spin_loop(); }
 }
}
fn ix(v:u64)->[usize;4]{[((v>>39)&511)as usize,((v>>30)&511)as usize,((v>>21)&511)as usize,((v>>12)&511)as usize]}

unsafe fn map(root:u64, va:u64, pa:u64)->Result<(),Error>{
 let indexes=ix(va); let mut t=root as *mut u64;
 for i in indexes[..3].iter().copied(){
  let slot=unsafe{t.add(i)}; let mut e=unsafe{slot.read()};
  if e&P==0 { let p=unsafe{crate::memory::allocate_page()}.map_err(|_|Error::NoPhysicalMemory)?; e=p|P|W; unsafe{slot.write(e);} }
  else { if e&PS!=0 || e&4!=0{return Err(Error::Invalid)}; e|=W; unsafe{slot.write(e);} }
  t=(e&ADDR)as *mut u64;
 }
 let leaf=unsafe{t.add(indexes[3])}; if unsafe{leaf.read()}&P!=0{return Err(Error::AlreadyMapped)}
 unsafe{leaf.write((pa&ADDR)|P|W|NX); core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
 Ok(())
}
unsafe fn unmap(root:u64,va:u64)->Result<u64,Error>{
 let indexes=ix(va); let mut t=root as *mut u64;
 for i in indexes[..3].iter().copied(){let e=unsafe{t.add(i).read()};if e&P==0||e&PS!=0{return Err(Error::NotMapped)}t=(e&ADDR)as *mut u64;}
 let leaf=unsafe{t.add(indexes[3])};let e=unsafe{leaf.read()};if e&P==0{return Err(Error::NotMapped)}
 unsafe{leaf.write(0);core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));} Ok(e&ADDR)
}

/// Allocate a contiguous virtual extent mapped to zeroed, not-necessarily-contiguous physical pages.
pub fn allocate_pages(pages:usize)->Result<*mut u8,Error>{
 if pages==0||pages>MAX_EXTENT_PAGES{return Err(Error::Invalid)} let root=ROOT.load(Ordering::Acquire);if root==0{return Err(Error::NotInitialized)}
 let _g=Guard::lock();let table=unsafe{&mut *TABLE.0.get()};let ti=table.iter().position(|e|!e.live).ok_or(Error::TableFull)?;
 let bits=unsafe{&mut *BITS.0.get()};let mut start=0;let mut found=None;
 while start+pages<=PAGES {let mut n=0;while n<pages&&bits[(start+n)/64]&(1u64<<((start+n)%64))==0{n+=1}if n==pages{found=Some(start);break}start+=n+1;}
 let start=found.ok_or(Error::NoVirtualSpace)?;for n in 0..pages{bits[(start+n)/64]|=1u64<<((start+n)%64)}
 let va=BASE+start as u64*PAGE;let mut mapped=0;
 while mapped<pages {
  let pa=match unsafe{crate::memory::allocate_page()}{Ok(p)=>p,Err(_)=>break};
  if unsafe{map(root,va+mapped as u64*PAGE,pa)}.is_err(){let _=unsafe{crate::memory::free_page(pa)};break} mapped+=1;
 }
 if mapped!=pages {for n in 0..mapped{if let Ok(pa)=unsafe{unmap(root,va+n as u64*PAGE)}{synchronize_kernel_tlb();let _=unsafe{crate::memory::free_page(pa)};}}for n in 0..pages{bits[(start+n)/64]&=!(1u64<<((start+n)%64))}return Err(Error::NoPhysicalMemory)}
 synchronize_kernel_tlb();
 table[ti]=Extent{start,pages,live:true};Ok(va as *mut u8)
}

/// Free only an exact live allocation returned by allocate_pages.
pub fn free_pages(ptr:*mut u8,pages:usize)->Result<(),Error>{
 let va=ptr as u64;if ptr.is_null()||va<BASE||(va-BASE)&(PAGE-1)!=0{return Err(Error::Invalid)}
 let root=ROOT.load(Ordering::Acquire);if root==0{return Err(Error::NotInitialized)}
 let start=((va-BASE)/PAGE)as usize;let _g=Guard::lock();let table=unsafe{&mut *TABLE.0.get()};
 let i=table.iter().position(|e|e.live&&e.start==start&&e.pages==pages).ok_or(Error::Invalid)?;
 // Validate every leaf and capture all backing frames before mutating page tables.
 // The VM lock makes this scratch array private to this operation.
 let reclaim=unsafe{&mut *RECLAIM_PAGES.0.get()};
 for n in 0..pages {let indexes=ix(va+n as u64*PAGE);let mut t=root as *const u64;for j in indexes[..3].iter().copied(){let e=unsafe{t.add(j).read()};if e&P==0||e&PS!=0{return Err(Error::NotMapped)}t=(e&ADDR)as *const u64;}let pte=unsafe{t.add(indexes[3]).read()};if pte&P==0{return Err(Error::NotMapped)}reclaim[n]=pte&ADDR;}
 // Remove the whole extent first, then wait for every online AP to flush stale
 // translations before any backing frame can be recycled.
 for n in 0..pages {let _=unsafe{unmap(root,va+n as u64*PAGE)}?;}
 synchronize_kernel_tlb();
 let bits=unsafe{&mut *BITS.0.get()};
 for n in 0..pages { // A saturated allocator may quarantine a frame rather than risk reuse.
  let _=unsafe{crate::memory::free_page(reclaim[n])};
  bits[(start+n)/64]&=!(1u64<<((start+n)%64));
 }
 table[i]=Extent::EMPTY;Ok(())
}
/// Exercise a real high-half mapping and reclamation before the pool uses it.
pub fn self_test() -> bool {
 // Exercise distinct extent sizes and every page boundary, then verify exact
 // reclamation and arena reuse. This runs once during boot and again after AP startup.
 for pages in [1usize, 2, 3, 8, 16] {
  let p = match allocate_pages(pages) { Ok(p) => p, Err(_) => return false };
  unsafe {
   for page in 0..pages { p.add(page * PAGE as usize).write((0x31u8).wrapping_add(page as u8)); }
   for page in 0..pages {
    if p.add(page * PAGE as usize).read() != (0x31u8).wrapping_add(page as u8) {
     let _ = free_pages(p, pages); return false;
    }
   }
  }
  if free_pages(p, pages).is_err() || allocation_pages(p).is_some() { return false; }
 }
 true
}
pub fn allocation_pages(ptr:*const u8)->Option<usize>{let va=ptr as u64;if ptr.is_null()||va<BASE||(va-BASE)&(PAGE-1)!=0{return None}let start=((va-BASE)/PAGE)as usize;let _g=Guard::lock();unsafe{&*TABLE.0.get()}.iter().find(|e|e.live&&e.start==start).map(|e|e.pages)}
#[cfg(test)]mod tests{use super::*;#[test]fn arena_geometry(){assert_eq!(BASE&(PAGE-1),0);assert_eq!(PAGES*PAGE as usize,1usize<<32);assert_eq!(ix(BASE)[0],256);assert_eq!(ix(BASE+PAGE)[3],1);}}

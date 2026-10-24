//! Supervisor-only kernel virtual mappings with demand-zero and pagefile-backed pageable extents.
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
const PAGE: u64 = 4096;
const P: u64 = 1;
const W: u64 = 2;
const PS: u64 = 1 << 7;
const NX: u64 = 1 << 63;
const ACCESSED: u64 = 1 << 5;
const ADDR: u64 = 0x000f_ffff_ffff_f000;
const BASE: u64 = 0xffff_8000_0000_0000;
const PAGES: usize = 1 << 20;
const WORDS: usize = PAGES / 64;
const MAX: usize = 4096;
// Match the NT pool's current 256 MiB maximum extent. A lock-protected scratch
// array lets free_pages invalidate all CPUs once before recycling any backing frame.
const MAX_EXTENT_PAGES: usize = 65536;
const MAX_RECLAIM_TABLES: usize = 4096;
const DRIVER_SLOT_OFFSETS:[u64;4]=[0xc000_0000,0xd000_0000,0xe000_0000,0xf000_0000];
const DRIVER_SLOT_SIZE:u64=0x0800_0000;
const DRIVER_SLOT_PAGES:usize=(DRIVER_SLOT_SIZE/PAGE) as usize;
fn is_driver_slot_base(address:u64)->bool{DRIVER_SLOT_OFFSETS.iter().any(|offset|BASE+*offset==address)}
fn is_driver_reserved_page(page:usize)->bool{let offset=page as u64*PAGE;DRIVER_SLOT_OFFSETS.iter().any(|start|offset>=*start&&offset<*start+DRIVER_SLOT_SIZE)}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { NotInitialized, Invalid, NoVirtualSpace, NoPhysicalMemory, AlreadyMapped, NotMapped, TableFull }
#[derive(Clone, Copy)]
struct Extent { start: usize, pages: usize, pageable: bool, owns_frames: bool, live: bool }
impl Extent { const EMPTY: Self = Self { start: 0, pages: 0, pageable: false, owns_frames: false, live: false }; }
struct Bits(core::cell::UnsafeCell<[u64; WORDS]>); unsafe impl Sync for Bits {}
struct Table(core::cell::UnsafeCell<[Extent; MAX]>); unsafe impl Sync for Table {}
static BITS: Bits = Bits(core::cell::UnsafeCell::new([0; WORDS]));
static MAPPED_BITS: Bits = Bits(core::cell::UnsafeCell::new([0; WORDS]));
static SWAPPED_BITS: Bits = Bits(core::cell::UnsafeCell::new([0; WORDS]));
struct SwapSlots(core::cell::UnsafeCell<[u16; PAGES]>); unsafe impl Sync for SwapSlots {}
static SWAP_SLOTS: SwapSlots = SwapSlots(core::cell::UnsafeCell::new([u16::MAX; PAGES]));
struct ReclaimSlots(core::cell::UnsafeCell<[u32; MAX_EXTENT_PAGES]>); unsafe impl Sync for ReclaimSlots {}
static RECLAIM_SLOTS: ReclaimSlots = ReclaimSlots(core::cell::UnsafeCell::new([u32::MAX; MAX_EXTENT_PAGES]));
static TABLE: Table = Table(core::cell::UnsafeCell::new([Extent::EMPTY; MAX]));
static ROOT: AtomicU64 = AtomicU64::new(0);
// Serializes demand-created direct-map leafs independently of the pageable arena lock.
static DIRECT_MAP_LOCK: AtomicBool = AtomicBool::new(false);
struct DirectMapGuard;
impl DirectMapGuard {
 fn lock() -> Self {
  while DIRECT_MAP_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
   core::hint::spin_loop();
  }
  Self
 }
}
impl Drop for DirectMapGuard {
 fn drop(&mut self) { DIRECT_MAP_LOCK.store(false, Ordering::Release); }
}
static OWN_ROOT_SLOT: AtomicBool = AtomicBool::new(false);
struct ReclaimTables(core::cell::UnsafeCell<[u64; MAX_RECLAIM_TABLES]>); unsafe impl Sync for ReclaimTables {}
static RECLAIM_TABLES: ReclaimTables = ReclaimTables(core::cell::UnsafeCell::new([0; MAX_RECLAIM_TABLES]));
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
/// Holds pageable-pool mappings stable while MM captures and pins PFNs.
pub struct PageableMappingGuard { _guard: Guard }
pub fn lock_pageable_mappings() -> PageableMappingGuard {
 PageableMappingGuard { _guard: Guard::lock() }
}
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
 if old == 0 {
  // Reserve PML4 slot 256 for this arena when it is initially unused. Only
  // then may we reclaim intermediate tables created beneath that slot.
  let slot=unsafe{(root as *const u64).add(ix(BASE)[0]).read()};
  OWN_ROOT_SLOT.store(slot&P==0,Ordering::Release);
 }
 // Keep four 128 MiB image slots unavailable to ordinary kernel allocations.
 // Each fixed-base driver may occupy at most 32 MiB within its reserved slot.
 let bits=unsafe{&mut *BITS.0.get()};
 for offset in DRIVER_SLOT_OFFSETS { let start=(offset/PAGE) as usize;for n in start..start+DRIVER_SLOT_PAGES{bits[n/64]|=1u64<<(n%64);} }
 ROOT.store(root,Ordering::Release); Ok(())
}
pub fn is_initialized()->bool { ROOT.load(Ordering::Acquire)!=0 }

fn flush_and_ack(logical_id: usize) {
 if logical_id >= crate::hal::MAX_CPUS { return; }
 let generation = TLB_GENERATION.load(Ordering::Acquire);
 if TLB_ACK[logical_id].load(Ordering::Relaxed) != generation {
  unsafe { let cr3: u64; core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)); core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags)); }
  TLB_ACK[logical_id].store(generation, Ordering::Release);
 }
}

/// Polling fallback retained for CPUs with IF clear or an unavailable IPI path.
pub fn smp_tlb_poll(logical_id: usize) {
 if logical_id == 0 || logical_id >= crate::hal::MAX_CPUS { return; }
 flush_and_ack(logical_id);
}

/// Interrupt handler target: reload CR3 before acknowledging the generation.
/// It intentionally takes no VM or pool locks and is safe during lock contention.
pub fn handle_tlb_ipi() {
 if let Some(cpu) = crate::hal::current_cpu_local() {
  flush_and_ack(cpu.logical_id.load(Ordering::Acquire) as usize);
 }
}

/// Invalidate shared kernel translations on every online AP before any backing
/// physical frame can be recycled. Prefer fixed-vector IPIs; AP polling remains
/// a fallback for early boot and failed IPI delivery.
fn synchronize_kernel_tlb() {
 if crate::hal::online_cpu_count() <= 1 { return; }
 let current = crate::hal::current_cpu_local()
  .map(|cpu| cpu.logical_id.load(Ordering::Acquire) as usize).unwrap_or(0);
 let generation = TLB_GENERATION.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
 for cpu in 1..crate::hal::MAX_CPUS {
  if cpu == current || crate::hal::cpu_state(cpu).ok() != Some(crate::hal::CPU_ONLINE) { continue; }
  if crate::smp::ipi_shootdowns_enabled() && crate::smp::send_tlb_shootdown(cpu).is_err() {
   // Page tables are already changed. Reusing their frames without confirmed
   // invalidation is memory corruption, so fail-stop instead of guessing.
   unsafe { crate::halt(); }
  }
  while TLB_ACK[cpu].load(Ordering::Acquire) != generation { core::hint::spin_loop(); }
 }
}
fn ix(v:u64)->[usize;4]{[((v>>39)&511)as usize,((v>>30)&511)as usize,((v>>21)&511)as usize,((v>>12)&511)as usize]}

unsafe fn map_with_flags(root:u64, va:u64, pa:u64, leaf_flags:u64)->Result<(),Error>{
 let indexes=ix(va); let mut t=root as *mut u64;
 for i in indexes[..3].iter().copied(){
  let slot=unsafe{t.add(i)}; let mut e=unsafe{slot.read()};
  if e&P==0 { let p=unsafe{crate::memory::allocate_page()}.map_err(|_|Error::NoPhysicalMemory)?; e=p|P|W; unsafe{slot.write(e);} }
  else { if e&PS!=0 || e&4!=0{return Err(Error::Invalid)}; e|=W; unsafe{slot.write(e);} }
  t=(e&ADDR)as *mut u64;
 }
 let leaf=unsafe{t.add(indexes[3])}; if unsafe{leaf.read()}&P!=0{return Err(Error::AlreadyMapped)}
 unsafe{leaf.write((pa&ADDR)|P|leaf_flags); core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
 Ok(())
}
unsafe fn map(root:u64, va:u64, pa:u64)->Result<(),Error>{
 unsafe{map_with_flags(root,va,pa,W|NX)}
}
fn table_is_empty(table:*const u64)->bool {
 (0..512).all(|i| unsafe { table.add(i).read() } & P == 0)
}

/// Reclaim empty intermediate tables only when PML4 slot 256 was vacant at
/// initialization and is therefore exclusively owned by the kernel VM arena.
/// Removed table frames are deferred until the subsequent SMP TLB rendezvous.
unsafe fn prune_empty_tables(root:u64,va:u64,count:&mut usize) {
 if !OWN_ROOT_SLOT.load(Ordering::Acquire) { return; }
 let indexes=ix(va); let pml4=root as *mut u64;
 let pml4_slot=unsafe{pml4.add(indexes[0])}; let e4=unsafe{pml4_slot.read()};
 if e4&P==0 { return; }
 let pdpt=(e4&ADDR) as *mut u64;
 let pdpt_slot=unsafe{pdpt.add(indexes[1])}; let e3=unsafe{pdpt_slot.read()};
 if e3&P==0 || e3&PS!=0 { return; }
 let pd=(e3&ADDR) as *mut u64;
 let pd_slot=unsafe{pd.add(indexes[2])}; let e2=unsafe{pd_slot.read()};
 if e2&P==0 || e2&PS!=0 { return; }
 let pt_pa=e2&ADDR; let pt=pt_pa as *const u64;
 if !table_is_empty(pt) { return; }
 let reclaimed=unsafe{&mut *RECLAIM_TABLES.0.get()};
 if *count < MAX_RECLAIM_TABLES { reclaimed[*count]=pt_pa; *count+=1; unsafe{pd_slot.write(0)}; } else { return; }
 if table_is_empty(pd as *const u64) {
  let pd_pa=e3&ADDR;
  if *count < MAX_RECLAIM_TABLES { reclaimed[*count]=pd_pa; *count+=1; unsafe{pdpt_slot.write(0)}; } else { return; }
  if table_is_empty(pdpt as *const u64) {
   let pdpt_pa=e4&ADDR;
   if *count < MAX_RECLAIM_TABLES { reclaimed[*count]=pdpt_pa; *count+=1; unsafe{pml4_slot.write(0)}; }
  }
 }
}

unsafe fn unmap(root:u64,va:u64)->Result<u64,Error>{
 let indexes=ix(va); let mut t=root as *mut u64;
 for i in indexes[..3].iter().copied(){let e=unsafe{t.add(i).read()};if e&P==0||e&PS!=0{return Err(Error::NotMapped)}t=(e&ADDR)as *mut u64;}
 let leaf=unsafe{t.add(indexes[3])};let e=unsafe{leaf.read()};if e&P==0{return Err(Error::NotMapped)}
 unsafe{leaf.write(0);core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));} Ok(e&ADDR)
}

unsafe fn leaf_ptr(root: u64, va: u64) -> Option<*mut u64> {
 let indexes=ix(va); let mut table=root as *mut u64;
 for index in indexes[..3].iter().copied() {
  let entry=unsafe{table.add(index).read_volatile()};
  if entry&P==0||entry&PS!=0{return None}
  table=(entry&ADDR) as *mut u64;
 }
 Some(unsafe{table.add(indexes[3])})
}

/// Evict one pageable resident page. Callers hold the VM lock. A second-chance
/// accessed-bit scan is followed by write-protection, an SMP TLB rendezvous,
/// pagefile I/O, unmapping, a second rendezvous, and only then frame recycling.
fn reclaim_one_locked(root:u64)->bool {
 if !crate::pagefile::is_active(){return false}
 let table=unsafe{&*TABLE.0.get()};
 for extent in table.iter().copied().filter(|e|e.live&&e.pageable) {
  for page_index in extent.start..extent.start+extent.pages {
   let word=page_index/64;let mask=1u64<<(page_index%64);
   let mapped=unsafe{&mut *MAPPED_BITS.0.get()};
   let swapped=unsafe{&mut *SWAPPED_BITS.0.get()};
   if mapped[word]&mask==0||swapped[word]&mask!=0{continue}
   let va=BASE+page_index as u64*PAGE;
   let Some(leaf)=(unsafe{leaf_ptr(root,va)}) else{continue};
   let pte=unsafe{leaf.read_volatile()};
   if pte&P==0{continue}
   // An MDL-pinned frame must remain resident even under pagefile pressure.
   if crate::ntoskrnl::mm::runtime::is_page_locked(pte&ADDR) { continue; }
   if pte&ACCESSED!=0 {
    unsafe{leaf.write_volatile(pte&!ACCESSED);core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
    synchronize_kernel_tlb();continue
   }
   let slot=match crate::pagefile::allocate_slot(){Ok(v)=>v,Err(_)=>return false};
   unsafe{leaf.write_volatile(pte&!W);core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
   synchronize_kernel_tlb();
   let physical=pte&ADDR;
   let Some(source)=crate::memory::physical_access_pointer(physical) else {
    unsafe{leaf.write_volatile(pte);core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
    synchronize_kernel_tlb();let _=crate::pagefile::free_slot(slot);continue
   };
   if unsafe{crate::pagefile::write_page(slot,source as *const u8)}.is_err(){
    unsafe{leaf.write_volatile(pte);core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
    synchronize_kernel_tlb();let _=crate::pagefile::free_slot(slot);continue
   }
   unsafe{leaf.write_volatile(0);(&mut *SWAP_SLOTS.0.get())[page_index]=slot as u16;}
   mapped[word]&=!mask;swapped[word]|=mask;
   unsafe{core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
   synchronize_kernel_tlb();
   if unsafe{crate::memory::free_page(physical)}.is_err(){unsafe{crate::halt()}}
   return true
  }
 }
 false
}

/// Reclaim pageable pool pages under memory pressure.
pub fn reclaim_pageable_pages(target:usize)->usize {
 if target==0||ROOT.load(Ordering::Acquire)==0{return 0}
 let root=ROOT.load(Ordering::Acquire);let _g=Guard::lock();let mut count=0;
 while count<target {if reclaim_one_locked(root){count+=1}else if reclaim_one_locked(root){count+=1}else{break}}
 count
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
  let pa=match unsafe{crate::memory::allocate_data_page()} {Ok(p)=>p,Err(_)=>{if reclaim_one_locked(root)||reclaim_one_locked(root){match unsafe{crate::memory::allocate_data_page()}{Ok(p)=>p,Err(_)=>break}}else{break}}};
  if unsafe{map(root,va+mapped as u64*PAGE,pa)}.is_err(){let _=unsafe{crate::memory::free_page(pa)};break} mapped+=1;
 }
 if mapped!=pages {for n in 0..mapped{if let Ok(pa)=unsafe{unmap(root,va+n as u64*PAGE)}{synchronize_kernel_tlb();let _=unsafe{crate::memory::free_page(pa)};}}for n in 0..pages{bits[(start+n)/64]&=!(1u64<<((start+n)%64))}return Err(Error::NoPhysicalMemory)}
 synchronize_kernel_tlb();
 let mapped_bits=unsafe{&mut *MAPPED_BITS.0.get()};for n in 0..pages{mapped_bits[(start+n)/64]|=1u64<<((start+n)%64);}
 table[ti]=Extent{start,pages,pageable:false,owns_frames:true,live:true};Ok(va as *mut u8)
}
/// Allocate an image extent at an exact supervisor virtual address. This is
/// used for fixed-base native PE images whose headers currently omit relocations.
pub fn allocate_pages_at(address:u64,pages:usize)->Result<*mut u8,Error>{
 if pages==0||pages>MAX_EXTENT_PAGES||address<PAGE+BASE||address&(PAGE-1)!=0{return Err(Error::Invalid)}
 let root=ROOT.load(Ordering::Acquire);if root==0{return Err(Error::NotInitialized)}
 let start=usize::try_from((address-BASE)/PAGE).map_err(|_|Error::Invalid)?;
 if start.checked_add(pages).is_none_or(|end|end>PAGES){return Err(Error::NoVirtualSpace)}
 let _g=Guard::lock();let table=unsafe{&mut *TABLE.0.get()};let ti=table.iter().position(|e|!e.live).ok_or(Error::TableFull)?;
 let bits=unsafe{&mut *BITS.0.get()};let mapped_bits=unsafe{&*MAPPED_BITS.0.get()};
 let reserved=is_driver_slot_base(address)&&pages<=DRIVER_SLOT_PAGES;
 if (start..start+pages).any(|n|(bits[n/64]&(1u64<<(n%64))!=0&&!reserved)||mapped_bits[n/64]&(1u64<<(n%64))!=0){return Err(Error::AlreadyMapped)}
 for n in start..start+pages{bits[n/64]|=1u64<<(n%64);}
 let mut mapped=0usize;
 while mapped<pages{
  let pa=match unsafe{crate::memory::allocate_data_page()}{Ok(p)=>p,Err(_)=>break};
  if unsafe{map(root,address+mapped as u64*PAGE,pa)}.is_err(){let _=unsafe{crate::memory::free_page(pa)};break;}
  mapped+=1;
 }
 if mapped!=pages{
  for n in 0..mapped{if let Ok(pa)=unsafe{unmap(root,address+n as u64*PAGE)}{synchronize_kernel_tlb();let _=unsafe{crate::memory::free_page(pa)};}}
  for n in start..start+pages{if !is_driver_reserved_page(n){bits[n/64]&=!(1u64<<(n%64));}}
  return Err(Error::NoPhysicalMemory)
 }
 synchronize_kernel_tlb();let mapped_bits=unsafe{&mut *MAPPED_BITS.0.get()};
 for n in start..start+pages{mapped_bits[n/64]|=1u64<<(n%64);}
 table[ti]=Extent{start,pages,pageable:false,owns_frames:true,live:true};Ok(address as *mut u8)
}

/// Change permissions on a page-aligned subrange of a live kernel VM extent.
/// Read permission is implicit on x86-64; executable and writable are exclusive.
pub fn protect_pages(address:u64,pages:usize,writable:bool,executable:bool)->Result<(),Error>{
 if pages==0||address&(PAGE-1)!=0||writable&&executable{return Err(Error::Invalid)}
 let root=ROOT.load(Ordering::Acquire);if root==0{return Err(Error::NotInitialized)}
 let start=usize::try_from(address.checked_sub(BASE).ok_or(Error::Invalid)?/PAGE).map_err(|_|Error::Invalid)?;
 if start.checked_add(pages).is_none_or(|end|end>PAGES){return Err(Error::Invalid)}
 let _g=Guard::lock();let table=unsafe{&*TABLE.0.get()};
 if !table.iter().any(|e|e.live&&start>=e.start&&start+pages<=e.start+e.pages){return Err(Error::NotMapped)}
 for n in 0..pages{
  let va=address+n as u64*PAGE;let leaf=unsafe{leaf_ptr(root,va)}.ok_or(Error::NotMapped)?;
  let mut entry=unsafe{leaf.read_volatile()};if entry&P==0{return Err(Error::NotMapped)}
  entry&=!(W|NX);if writable{entry|=W;}if !executable{entry|=NX;}
  unsafe{leaf.write_volatile(entry);core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
 }
 synchronize_kernel_tlb();Ok(())
}

/// Map an arbitrary list of caller-owned physical pages into a contiguous
/// supervisor virtual extent. The caller retains ownership of every frame;
/// this extent only owns the virtual mapping and must be released with
/// unmap_external_pages. All frames must be firmware-described RAM.
pub fn map_external_pages(pfns: &[u64]) -> Result<*mut u8, Error> {
 if pfns.is_empty() || pfns.len() > MAX_EXTENT_PAGES { return Err(Error::Invalid); }
 if ROOT.load(Ordering::Acquire) == 0 { return Err(Error::NotInitialized); }
 for pfn in pfns {
  let pa = pfn.checked_mul(PAGE).ok_or(Error::Invalid)?;
  if pa & (PAGE-1) != 0 || !crate::memory::is_ram_physical_address(pa) { return Err(Error::Invalid); }
 }
 let root = ROOT.load(Ordering::Acquire);
 let _g = Guard::lock();
 let table = unsafe { &mut *TABLE.0.get() };
 let ti = table.iter().position(|e| !e.live).ok_or(Error::TableFull)?;
 let bits = unsafe { &mut *BITS.0.get() };
 let pages = pfns.len();
 let mut start = 0usize;
 let mut found = None;
 while start.checked_add(pages).is_some_and(|end| end <= PAGES) {
  let mut n = 0;
  while n < pages && bits[(start+n)/64] & (1u64 << ((start+n)%64)) == 0 { n += 1; }
  if n == pages { found = Some(start); break; }
  start += n + 1;
 }
 let start = found.ok_or(Error::NoVirtualSpace)?;
 for n in 0..pages { bits[(start+n)/64] |= 1u64 << ((start+n)%64); }
 let va = BASE + start as u64 * PAGE;
 let mut mapped = 0usize;
 while mapped < pages {
  let pa = pfns[mapped].checked_mul(PAGE).ok_or(Error::Invalid)?;
  if unsafe { map(root, va + mapped as u64 * PAGE, pa) }.is_err() { break; }
  mapped += 1;
 }
 if mapped != pages {
  let mut table_count = 0usize;
  for n in 0..mapped {
   let page_va = va+n as u64*PAGE;
   let _ = unsafe { unmap(root, page_va) };
   unsafe { prune_empty_tables(root, page_va, &mut table_count); }
  }
  if mapped != 0 { synchronize_kernel_tlb(); }
  let tables = unsafe { &*RECLAIM_TABLES.0.get() };
  for pa in tables.iter().take(table_count).copied() { let _ = unsafe { crate::memory::free_page(pa) }; }
  for n in 0..pages { bits[(start+n)/64] &= !(1u64 << ((start+n)%64)); }
  return Err(Error::NoPhysicalMemory);
 }
 synchronize_kernel_tlb();
 let mapped_bits = unsafe { &mut *MAPPED_BITS.0.get() };
 for n in 0..pages { mapped_bits[(start+n)/64] |= 1u64 << ((start+n)%64); }
 table[ti] = Extent { start, pages, pageable: false, owns_frames: false, live: true };
 Ok(va as *mut u8)
}

/// Tear down a mapping created by map_external_pages without freeing its PFNs.
pub fn unmap_external_pages(ptr: *mut u8, pages: usize) -> Result<(), Error> {
 let va = ptr as u64;
 if ptr.is_null() || va < BASE || (va-BASE) & (PAGE-1) != 0 || pages == 0 { return Err(Error::Invalid); }
 let root = ROOT.load(Ordering::Acquire);
 if root == 0 { return Err(Error::NotInitialized); }
 let start = ((va-BASE)/PAGE) as usize;
 let _g = Guard::lock();
 let table = unsafe { &mut *TABLE.0.get() };
 let index = table.iter().position(|e| e.live && e.start == start && e.pages == pages && !e.owns_frames && !e.pageable).ok_or(Error::Invalid)?;
 for n in 0..pages {
  let word = (start+n)/64; let mask = 1u64 << ((start+n)%64);
  if unsafe { &*MAPPED_BITS.0.get() }[word] & mask == 0 { return Err(Error::NotMapped); }
  let indexes = ix(va+n as u64*PAGE); let mut walk = root as *const u64;
  for j in indexes[..3].iter().copied() { let e = unsafe { walk.add(j).read() }; if e&P == 0 || e&PS != 0 { return Err(Error::NotMapped); } walk = (e&ADDR) as *const u64; }
  let pte = unsafe { walk.add(indexes[3]).read() };
  if pte&P == 0 { return Err(Error::NotMapped); }
 }
 let mut table_count = 0usize;
 for n in 0..pages {
  let page_va = va+n as u64*PAGE;
  let _ = unsafe { unmap(root, page_va) }?;
  unsafe { prune_empty_tables(root, page_va, &mut table_count); }
 }
 synchronize_kernel_tlb();
 let tables = unsafe { &*RECLAIM_TABLES.0.get() };
 for pa in tables.iter().take(table_count).copied() { let _ = unsafe { crate::memory::free_page(pa) }; }
 let bits = unsafe { &mut *BITS.0.get() }; let mapped = unsafe { &mut *MAPPED_BITS.0.get() };
 for n in 0..pages { let word=(start+n)/64; let mask=1u64<<((start+n)%64); bits[word]&=!mask; mapped[word]&=!mask; }
 table[index] = Extent::EMPTY;
 Ok(())
}

/// Fault in one tracked pageable-pool page before a caller probes or pins it.
/// The fault handler validates extent ownership and uses the normal page-in path.
pub fn ensure_page_resident(address: u64, write: bool) -> bool {
 if !is_pageable_address(address) { return false; }
 let page = address & !(PAGE-1);
 handle_page_fault(page, if write { 2 } else { 0 })
}

/// Reserve a virtual extent for demand-zero pageable pool memory. Physical
/// frames and leaf PTEs are created only when a page-fault handler first touches it.
pub fn allocate_pageable_pages(pages: usize) -> Result<*mut u8, Error> {
 if pages == 0 || pages > MAX_EXTENT_PAGES { return Err(Error::Invalid); }
 if ROOT.load(Ordering::Acquire) == 0 { return Err(Error::NotInitialized); }
 let _g=Guard::lock();
 let table=unsafe{&mut *TABLE.0.get()};
 let ti=table.iter().position(|e|!e.live).ok_or(Error::TableFull)?;
 let bits=unsafe{&mut *BITS.0.get()};let mut start=0usize;let mut found=None;
 while start+pages<=PAGES { let mut n=0;while n<pages&&bits[(start+n)/64]&(1u64<<((start+n)%64))==0{n+=1;}if n==pages{found=Some(start);break;}start+=n+1; }
 let start=found.ok_or(Error::NoVirtualSpace)?;
 for n in 0..pages { bits[(start+n)/64]|=1u64<<((start+n)%64); }
 table[ti]=Extent{start,pages,pageable:true,owns_frames:true,live:true};
 Ok((BASE+start as u64*PAGE) as *mut u8)
}

/// Handle supervisor non-present faults in the RAM direct-map window and the
/// pageable virtual pool. Pageable extents support demand-zero allocation and
/// pagefile-backed page-in; unrelated/protection faults remain fatal.
pub fn handle_page_fault(address:u64,error_code:u64)->bool {
 if error_code&((1<<2)|(1<<3)|(1<<4))!=0{return false}
 let page_va=address&!(PAGE-1);let root=ROOT.load(Ordering::Acquire);
 if root==0{return false}

 // A supervisor data fault in the high direct-map window may lazily establish
 // a writable, NX mapping, but only if the final UEFI map identifies the target
 // physical page as RAM. Device/MMIO and reserved ranges are never admitted.
 let direct_base=crate::memory::DIRECT_MAP_BASE;
 if page_va>=direct_base
     && page_va.wrapping_sub(direct_base)<crate::memory::DIRECT_MAP_PHYSICAL_LIMIT
 {
  if error_code&1!=0{return false} // not a protection fault: it must be absent
  let physical=page_va-direct_base;
  if !crate::memory::is_ram_physical_address(physical){return false}
  let _direct_guard=DirectMapGuard::lock();
  // Another CPU may have installed this leaf while this CPU waited.
  if unsafe{leaf_ptr(root,page_va)}.is_some_and(|leaf|unsafe{leaf.read_volatile()}&P!=0) {
   unsafe{core::arch::asm!("invlpg [{}]",in(reg)page_va,options(nostack,preserves_flags));}
   return true
  }
  let leaf_flags=NX | if crate::memory::direct_map_page_writable(physical) { W } else { 0 };
  return unsafe{map_with_flags(root,page_va,physical,leaf_flags)}.is_ok()
 }

 if page_va<BASE||page_va>=BASE+(PAGES as u64)*PAGE{return false}
 let page_index=((page_va-BASE)/PAGE) as usize;let _g=Guard::lock();
 if !unsafe{&*TABLE.0.get()}.iter().any(|e|e.live&&e.pageable&&page_index>=e.start&&page_index<e.start+e.pages){return false}
 let word=page_index/64;let mask=1u64<<(page_index%64);
 let mapped=unsafe{&mut *MAPPED_BITS.0.get()};let swapped=unsafe{&mut *SWAPPED_BITS.0.get()};
 let is_swapped=swapped[word]&mask!=0;
 if error_code&1!=0&&!is_swapped{return false}
 if mapped[word]&mask!=0 {
  if error_code&1!=0{return false}
  let Some(leaf)=(unsafe{leaf_ptr(root,page_va)}) else{return false};
  if unsafe{leaf.read_volatile()}&P==0{return false}
  unsafe{core::arch::asm!("invlpg [{}]",in(reg)page_va,options(nostack,preserves_flags));}return true
 }
 let pa=match unsafe{crate::memory::allocate_data_page()} {Ok(p)=>p,Err(_)=>{if !reclaim_one_locked(root)&&!reclaim_one_locked(root){return false}match unsafe{crate::memory::allocate_data_page()}{Ok(p)=>p,Err(_)=>return false}}};
 if is_swapped {
  let slot=unsafe{(&*SWAP_SLOTS.0.get())[page_index]};
  let Some(destination)=crate::memory::physical_access_pointer(pa) else {let _=unsafe{crate::memory::free_page(pa)};return false};
  if slot==u16::MAX||unsafe{crate::pagefile::read_page(slot as u32,destination)}.is_err(){let _=unsafe{crate::memory::free_page(pa)};return false}
  if unsafe{map(root,page_va,pa)}.is_err(){let _=unsafe{crate::memory::free_page(pa)};return false}
  mapped[word]|=mask;swapped[word]&=!mask;unsafe{(&mut *SWAP_SLOTS.0.get())[page_index]=u16::MAX;}
  synchronize_kernel_tlb();let _=crate::pagefile::free_slot(slot as u32);true
 }else{
  // allocate_data_page already zeroed the frame through its physical access alias.
  if unsafe{map(root,page_va,pa)}.is_err(){let _=unsafe{crate::memory::free_page(pa)};return false}
  mapped[word]|=mask;synchronize_kernel_tlb();true
 }
}

/// Free only an exact live allocation returned by allocate_pages.
pub fn free_pages(ptr:*mut u8,pages:usize)->Result<(),Error>{
 let va=ptr as u64;if ptr.is_null()||va<BASE||(va-BASE)&(PAGE-1)!=0{return Err(Error::Invalid)}
 let root=ROOT.load(Ordering::Acquire);if root==0{return Err(Error::NotInitialized)}
 let start=((va-BASE)/PAGE)as usize;let _g=Guard::lock();let table=unsafe{&mut *TABLE.0.get()};
 let i=table.iter().position(|e|e.live&&e.start==start&&e.pages==pages).ok_or(Error::Invalid)?;
 if !table[i].owns_frames { return Err(Error::Invalid); }
 // Validate mapped leaves and capture physical frames; nonresident pageable
 // pages have no backing frame to reclaim.
 let reclaim=unsafe{&mut *RECLAIM_PAGES.0.get()};
 let reclaim_slots=unsafe{&mut *RECLAIM_SLOTS.0.get()};
 let mapped=unsafe{&*MAPPED_BITS.0.get()};
 let swapped=unsafe{&*SWAPPED_BITS.0.get()};
 for n in 0..pages {
  let word=(start+n)/64;let mask=1u64<<((start+n)%64);
  if mapped[word]&mask==0 {
   if !table[i].pageable{return Err(Error::NotMapped)}
   reclaim[n]=0;
   if swapped[word]&mask!=0 {
    let slot=unsafe{(&*SWAP_SLOTS.0.get())[start+n]};
    if slot==u16::MAX{return Err(Error::NotMapped)}
    reclaim_slots[n]=slot as u32;
   }else{reclaim_slots[n]=u32::MAX;}
   continue;
  }
  let indexes=ix(va+n as u64*PAGE);let mut t=root as *const u64;
  for j in indexes[..3].iter().copied(){let e=unsafe{t.add(j).read()};if e&P==0||e&PS!=0{return Err(Error::NotMapped)}t=(e&ADDR)as *const u64;}
  let pte=unsafe{t.add(indexes[3]).read()};if pte&P==0{return Err(Error::NotMapped)}
  reclaim[n]=pte&ADDR;reclaim_slots[n]=u32::MAX;
  if crate::ntoskrnl::mm::runtime::is_page_locked(reclaim[n]) { return Err(Error::Invalid); }
 }
 // Unmap all resident pages before a single SMP rendezvous, then recycle frames.
 for n in 0..pages {if reclaim[n]!=0 {let _=unsafe{unmap(root,va+n as u64*PAGE)}?;}}
 let mut table_count=0usize;
 for n in 0..pages {if reclaim[n]!=0 {unsafe{prune_empty_tables(root,va+n as u64*PAGE,&mut table_count)}}}
 if reclaim[..pages].iter().any(|pa|*pa!=0) { synchronize_kernel_tlb(); }
 let bits=unsafe{&mut *BITS.0.get()};let mapped=unsafe{&mut *MAPPED_BITS.0.get()};
 for n in 0..pages {
  if reclaim[n]!=0 && table[i].owns_frames { let _=unsafe{crate::memory::free_page(reclaim[n])}; }
  if reclaim_slots[n]!=u32::MAX {
   let _=crate::pagefile::free_slot(reclaim_slots[n]);
   unsafe{(&mut *SWAP_SLOTS.0.get())[start+n]=u16::MAX;}
  }
  let word=(start+n)/64;let mask=1u64<<((start+n)%64);
  if !is_driver_reserved_page(start+n){bits[word]&=!mask;}mapped[word]&=!mask;
  unsafe{(&mut *SWAPPED_BITS.0.get())[word]&=!mask;}
 }
 let tables=unsafe{&*RECLAIM_TABLES.0.get()};
 for pa in tables.iter().take(table_count).copied() { let _=unsafe{crate::memory::free_page(pa)}; }
 table[i]=Extent::EMPTY;Ok(())
}
/// Exercise a real high-half mapping and reclamation before the pool uses it.
pub fn self_test() -> bool {
 let ipi_before=crate::smp::tlb_ipi_count();
 // If firmware reports conventional RAM above 4 GiB, touch one byte through
 // the direct map and verify the resulting leaf is supervisor, writable, NX,
 // and translates back to the expected physical page.
 if crate::memory::first_high_conventional_page().is_some() {
  let Some(physical)=crate::memory::first_high_conventional_page() else{return false};
  let Some(virtual_address)=crate::memory::direct_map_address(physical) else{return false};
  let _sample=unsafe{(virtual_address as *const u8).read_volatile()};
  let root=ROOT.load(Ordering::Acquire);
  let Ok(mapping)=(unsafe{crate::memory::inspect_mapping(root,virtual_address)}) else{return false};
  if mapping.physical_address!=physical || mapping.page_size!=PAGE
      || !mapping.writable || mapping.executable || mapping.user_accessible {return false}

  // Confirm the allocator itself can return, zero, access, and recycle a frame
  // above 4 GiB instead of merely exposing a direct-map address for one.
  let Ok(high_page)= (unsafe{crate::memory::allocate_data_page()}) else{return false};
  if high_page<0x1_0000_0000 {let _=unsafe{crate::memory::free_page(high_page)};return false}
  let Some(alias)=crate::memory::direct_map_address(high_page) else {
   let _=unsafe{crate::memory::free_page(high_page)};return false
  };
  let word=alias as *mut u64;
  if unsafe{word.read_volatile()}!=0 {let _=unsafe{crate::memory::free_page(high_page)};return false}
  unsafe{word.write_volatile(0x5244_4f54_4f53_484d);}
  if unsafe{word.read_volatile()}!=0x5244_4f54_4f53_484d {
   unsafe{word.write_volatile(0)};let _=unsafe{crate::memory::free_page(high_page)};return false
  }
  unsafe{word.write_volatile(0);}
  if unsafe{crate::memory::free_page(high_page)}.is_err(){return false}
 }
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
  if OWN_ROOT_SLOT.load(Ordering::Acquire) {
   let _g=Guard::lock();
   let no_live_extents=unsafe{&*TABLE.0.get()}.iter().all(|extent|!extent.live);
   if no_live_extents {
    let root=ROOT.load(Ordering::Acquire) as *const u64;
    if unsafe{root.add(ix(BASE)[0]).read()}&P!=0 { return false; }
   }
  }
 }
 // Fixed-address image extents must be allocatable and transition from
 // writable/NX staging to strict RX and RW/NX pages without W+X permissions.
 let fixed_va=BASE+0x0f000000;
 let fixed=match allocate_pages_at(fixed_va,2){Ok(p)=>p,Err(_)=>return false};
 if fixed as u64!=fixed_va{let _=free_pages(fixed,2);return false;}
 unsafe{fixed.write(0x51);fixed.add(PAGE as usize).write(0x62);}
 if protect_pages(fixed_va,2,false,false).is_err()
  ||protect_pages(fixed_va,1,false,true).is_err()
  ||protect_pages(fixed_va+PAGE,1,true,false).is_err(){let _=free_pages(fixed,2);return false;}
 let root=ROOT.load(Ordering::Acquire);
 let rx=unsafe{crate::memory::inspect_mapping(root,fixed_va)};
 let rw=unsafe{crate::memory::inspect_mapping(root,fixed_va+PAGE)};
 if !matches!(rx,Ok(m) if !m.writable&&m.executable&&!m.user_accessible)
  ||!matches!(rw,Ok(m) if m.writable&&!m.executable&&!m.user_accessible)
  ||unsafe{fixed.read()}!=0x51||unsafe{fixed.add(PAGE as usize).read()}!=0x62
  ||free_pages(fixed,2).is_err(){return false;}
 // Demand-zero pageable extents must fault pages in independently and still
 // participate in the same SMP-safe reclamation path.
 let p=match allocate_pageable_pages(3){Ok(p)=>p,Err(_)=>return false};
 if allocation_pages(p).is_none(){let _=free_pages(p,3);return false;}
 unsafe { p.write(0xA7);p.add(PAGE as usize+17).write(0x5C);p.add(2*PAGE as usize+31).write(0xE1); }
 if unsafe{p.read()}!=0xA7 || unsafe{p.add(PAGE as usize+17).read()}!=0x5C || unsafe{p.add(2*PAGE as usize+31).read()}!=0xE1 {let _=free_pages(p,3);return false;}
 let start=((p as u64-BASE)/PAGE) as usize;
 let all_resident={let _g=Guard::lock();let mapped=unsafe{&*MAPPED_BITS.0.get()};(0..3).all(|n|mapped[(start+n)/64]&(1u64<<((start+n)%64))!=0)};
 if !all_resident {let _=free_pages(p,3);return false;}
 if crate::pagefile::is_active() {
  if free_pages(p,3).is_err(){return false;}
  let swap_test=match allocate_pageable_pages(1){Ok(v)=>v,Err(_)=>{let _=free_pages(p,3);return false}};
  unsafe{swap_test.write(0xD3);swap_test.add(4095).write(0x6B);}
  if reclaim_pageable_pages(1)!=1 {let _=free_pages(swap_test,1);let _=free_pages(p,3);return false;}
  let swap_index=((swap_test as u64-BASE)/PAGE) as usize;
  let is_swapped={let _g=Guard::lock();let bits=unsafe{&*SWAPPED_BITS.0.get()};bits[swap_index/64]&(1u64<<(swap_index%64))!=0};
  if !is_swapped {let _=free_pages(swap_test,1);let _=free_pages(p,3);return false;}
  // These reads deliberately trigger a real protection/page-not-present fault
  // and must restore bytes from the ATA-backed pagefile slot.
  let restored=unsafe{swap_test.read()}==0xD3 && unsafe{swap_test.add(4095).read()}==0x6B;
  let is_resident={let _g=Guard::lock();let bits=unsafe{&*MAPPED_BITS.0.get()};let swaps=unsafe{&*SWAPPED_BITS.0.get()};bits[swap_index/64]&(1u64<<(swap_index%64))!=0 && swaps[swap_index/64]&(1u64<<(swap_index%64))==0};
  if !restored||!is_resident||free_pages(swap_test,1).is_err(){let _=free_pages(p,3);return false;}
 }
 let freed=if crate::pagefile::is_active(){allocation_pages(p).is_none()}else{free_pages(p,3).is_ok()&&allocation_pages(p).is_none()};
 freed && (!crate::smp::ipi_shootdowns_enabled() || crate::smp::tlb_ipi_count()>ipi_before)
}
/// Report whether a system address belongs to a pageable pool extent, even
/// while its current PTE happens to be resident.
pub fn is_pageable_address(address:u64)->bool {
 if address<BASE||address>=BASE+(PAGES as u64)*PAGE{return false}
 let page=((address-BASE)/PAGE) as usize;
 let _g=Guard::lock();
 unsafe{&*TABLE.0.get()}.iter().any(|e|e.live&&e.pageable&&page>=e.start&&page<e.start+e.pages)
}

pub fn allocation_pages(ptr:*const u8)->Option<usize>{let va=ptr as u64;if ptr.is_null()||va<BASE||(va-BASE)&(PAGE-1)!=0{return None}let start=((va-BASE)/PAGE)as usize;let _g=Guard::lock();unsafe{&*TABLE.0.get()}.iter().find(|e|e.live&&e.start==start).map(|e|e.pages)}
#[cfg(test)]mod tests{use super::*;#[test]fn arena_geometry(){assert_eq!(BASE&(PAGE-1),0);assert_eq!(PAGES*PAGE as usize,1usize<<32);assert_eq!(ix(BASE)[0],256);assert_eq!(ix(BASE+PAGE)[3],1);}}

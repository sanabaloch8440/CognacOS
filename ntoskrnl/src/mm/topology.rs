//! Bounded ACPI NUMA topology parser for x86-64 (SRAT and SLIT).
use core::sync::atomic::{AtomicBool, Ordering};
pub const MAX_NODES: usize = 64;
pub const MAX_RANGES: usize = 256;
/// Static SRAT ranges plus independently onlined hot-add extents.
pub const MAX_ALLOCATABLE_RANGES: usize = MAX_RANGES * 2;
const SRAT_SIG: &[u8;4] = b"SRAT";
const SLIT_SIG: &[u8;4] = b"SLIT";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryRange { pub start:u64, pub end:u64, pub node:u32, pub hot_pluggable:bool, pub non_volatile:bool }
impl MemoryRange { const EMPTY:Self=Self{start:0,end:0,node:u32::MAX,hot_pluggable:false,non_volatile:false}; }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProcessorAffinity { apic_id:u32, node:u32, enabled:bool }
impl ProcessorAffinity { const EMPTY:Self=Self{apic_id:u32::MAX,node:u32::MAX,enabled:false}; }
const MAX_PROCESSOR_AFFINITIES:usize=256;
struct ProcessorStore(core::cell::UnsafeCell<([ProcessorAffinity;MAX_PROCESSOR_AFFINITIES],usize)>);
unsafe impl Sync for ProcessorStore {}
static PROCESSORS:ProcessorStore=ProcessorStore(core::cell::UnsafeCell::new(([ProcessorAffinity::EMPTY;MAX_PROCESSOR_AFFINITIES],0)));
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyError { InvalidTable, Capacity, InvalidNode, Overlap }
struct Store(core::cell::UnsafeCell<([MemoryRange;MAX_RANGES],usize,[[u8;MAX_NODES];MAX_NODES],usize)>);
unsafe impl Sync for Store {}
static TOPOLOGY:Store=Store(core::cell::UnsafeCell::new(([MemoryRange::EMPTY;MAX_RANGES],0,[[0;MAX_NODES];MAX_NODES],0)));
static READY:AtomicBool=AtomicBool::new(false);
fn u32at(b:&[u8],o:usize)->Option<u32>{Some(u32::from_le_bytes(b.get(o..o+4)?.try_into().ok()?))}
fn u64at(b:&[u8],o:usize)->Option<u64>{Some(u64::from_le_bytes(b.get(o..o+8)?.try_into().ok()?))}
fn table_body<'a>(table:&'a [u8],sig:&[u8;4])->Result<&'a [u8],TopologyError>{
 if table.len()<36||table.get(0..4)!=Some(sig.as_slice()){return Err(TopologyError::InvalidTable)}
 let len=u32at(table,4).ok_or(TopologyError::InvalidTable)? as usize;
 if len<36||len>table.len(){return Err(TopologyError::InvalidTable)}
 let t=&table[..len]; if t.iter().fold(0u8,|a,b|a.wrapping_add(*b))!=0{return Err(TopologyError::InvalidTable)} Ok(t)
}
/// Parse a mapped ACPI SRAT table. Publishes only after full validation.
pub fn parse_srat(table:&[u8])->Result<usize,TopologyError>{
 // Share the ACPI driver's revision-aware SDT and SRAT record validation;
 // retain kernel-specific overlap, page-alignment and publication checks below.
 cognacos_acpi::parse_srat(table).map_err(|e|match e{cognacos_acpi::AcpiError::TooManyEntries=>TopologyError::Capacity,_=>TopologyError::InvalidTable})?;
 let t=table_body(table,SRAT_SIG)?; if t.len()<48{return Err(TopologyError::InvalidTable)}
 let mut ranges=[MemoryRange::EMPTY;MAX_RANGES]; let mut count=0usize;
 let mut processors=[ProcessorAffinity::EMPTY;MAX_PROCESSOR_AFFINITIES]; let mut processor_count=0usize;
 let mut pos=48usize;
 while pos<t.len(){if pos+2>t.len(){return Err(TopologyError::InvalidTable)} let ty=t[pos];let len=t[pos+1] as usize;
 if len<2||pos.checked_add(len).filter(|e|*e<=t.len()).is_none(){return Err(TopologyError::InvalidTable)}
 if ty==0 { // Processor Local APIC/SAPIC Affinity (ACPI SRAT type 0).
  if len<16{return Err(TopologyError::InvalidTable)}
  let flags=u32at(t,pos+4).ok_or(TopologyError::InvalidTable)?;
  let node=t[pos+2] as u32 | ((t[pos+9] as u32)<<8) | ((t[pos+10] as u32)<<16) | ((t[pos+11] as u32)<<24);
  let apic_id=t[pos+3] as u32;
  if flags&1!=0 { if node as usize>=MAX_NODES{return Err(TopologyError::InvalidNode)}
   if processor_count==MAX_PROCESSOR_AFFINITIES{return Err(TopologyError::Capacity)}
   if processors[..processor_count].iter().any(|p|p.enabled&&p.apic_id==apic_id){return Err(TopologyError::InvalidTable)}
   processors[processor_count]=ProcessorAffinity{apic_id,node,enabled:true};processor_count+=1;
  }
 } else if ty==2 { // Processor Local x2APIC Affinity (ACPI SRAT type 2).
  if len<24{return Err(TopologyError::InvalidTable)}
  let flags=u32at(t,pos+12).ok_or(TopologyError::InvalidTable)?;
  let node=u32at(t,pos+4).ok_or(TopologyError::InvalidTable)?;
  let apic_id=u32at(t,pos+8).ok_or(TopologyError::InvalidTable)?;
  if flags&1!=0 { if node as usize>=MAX_NODES{return Err(TopologyError::InvalidNode)}
   if processor_count==MAX_PROCESSOR_AFFINITIES{return Err(TopologyError::Capacity)}
   if processors[..processor_count].iter().any(|p|p.enabled&&p.apic_id==apic_id){return Err(TopologyError::InvalidTable)}
   processors[processor_count]=ProcessorAffinity{apic_id,node,enabled:true};processor_count+=1;
  }
 } else if ty==1 {if len<40{return Err(TopologyError::InvalidTable)} let flags=u32at(t,pos+28).ok_or(TopologyError::InvalidTable)?;
 if flags&1!=0 {let node=u32at(t,pos+2).ok_or(TopologyError::InvalidTable)?;let start=u64at(t,pos+8).ok_or(TopologyError::InvalidTable)?;let size=u64at(t,pos+16).ok_or(TopologyError::InvalidTable)?;let end=start.checked_add(size).ok_or(TopologyError::InvalidTable)?;
 if size!=0 {if node as usize>=MAX_NODES{return Err(TopologyError::InvalidNode)} if count==MAX_RANGES{return Err(TopologyError::Capacity)} if start&0xfff!=0||end&0xfff!=0{return Err(TopologyError::InvalidTable)} ranges[count]=MemoryRange{start,end,node,hot_pluggable:flags&2!=0,non_volatile:flags&4!=0};count+=1;}}} pos+=len;}
 for i in 0..count{for j in 0..i{if ranges[i].start<ranges[j].end&&ranges[j].start<ranges[i].end{return Err(TopologyError::Overlap)}}}
 ranges[..count].sort_unstable_by_key(|r|r.start);
 unsafe{let s=&mut *TOPOLOGY.0.get();s.0=ranges;s.1=count;let p=&mut *PROCESSORS.0.get();p.0=processors;p.1=processor_count;}
 READY.store(true,Ordering::Release);Ok(count)
}
/// Parse ACPI SLIT; SRAT and SLIT are independently validated.
pub fn parse_slit(table:&[u8])->Result<usize,TopologyError>{
 // The ACPI parser enforces exact matrix extent and architectural distance
 // constraints before this kernel-owned matrix is atomically published.
 cognacos_acpi::parse_slit(table).map_err(|e|match e{cognacos_acpi::AcpiError::TooManyEntries=>TopologyError::Capacity,_=>TopologyError::InvalidTable})?;
 let t=table_body(table,SLIT_SIG)?;let n=u64at(t,36).ok_or(TopologyError::InvalidTable)? as usize;
 let bytes=n.checked_mul(n).ok_or(TopologyError::Capacity)?;
 if n==0||n>MAX_NODES||44usize.checked_add(bytes).filter(|e|*e<=t.len()).is_none(){return Err(TopologyError::InvalidTable)}
 let mut matrix=[[0u8;MAX_NODES];MAX_NODES];let mut p=44;
 for i in 0..n{for j in 0..n{let d=t[p];if d==0||(i==j&&d!=10){return Err(TopologyError::InvalidTable)}matrix[i][j]=d;p+=1;}}
 unsafe{let s=&mut *TOPOLOGY.0.get();s.2=matrix;s.3=n;}Ok(n)
}
/// Initialize the UMA fallback used by Windows on systems with no SRAT/SLIT.
/// Ranges must be sorted, page-aligned, non-overlapping RAM ranges from firmware.
fn validate_single_node_ranges(ranges: &[(u64, u64)]) -> Result<([MemoryRange; MAX_RANGES], usize), TopologyError> {
 if ranges.is_empty() || ranges.len() > MAX_RANGES { return Err(TopologyError::Capacity); }
 let mut staged = [MemoryRange::EMPTY; MAX_RANGES];
 for (index, &(start, length)) in ranges.iter().enumerate() {
  if length == 0 || start & 0xfff != 0 || length & 0xfff != 0 {
   return Err(TopologyError::InvalidTable);
  }
  let end = start.checked_add(length).ok_or(TopologyError::InvalidTable)?;
  if index != 0 && start < staged[index - 1].end { return Err(TopologyError::Overlap); }
  staged[index] = MemoryRange { start, end, node: 0, hot_pluggable: false, non_volatile: false };
 }
 Ok((staged, ranges.len()))
}
pub fn initialize_single_node(ranges: &[(u64, u64)]) -> Result<usize, TopologyError> {
 if READY.load(Ordering::Acquire) { return Ok(memory_node_count()); }
 let (staged, count) = validate_single_node_ranges(ranges)?;
 let mut distances = [[0u8; MAX_NODES]; MAX_NODES];
 distances[0][0] = 10;
 unsafe {
  let store = &mut *TOPOLOGY.0.get();
  store.0 = staged;
  store.1 = count;
  store.2 = distances;
  store.3 = 1;
  let processors=&mut *PROCESSORS.0.get();
  processors.0=[ProcessorAffinity::EMPTY;MAX_PROCESSOR_AFFINITIES];
  processors.1=0;
 }
 READY.store(true, Ordering::Release);
 Ok(count)
}
pub fn is_ready()->bool{READY.load(Ordering::Acquire)}
/// Number of memory proximity domains actually described by accepted SRAT
/// ranges. This is not inferred from the SLIT matrix, which may include
/// processor-only domains with no allocatable memory.
pub fn memory_node_count() -> usize {
    if !is_ready() { return 0; }
    let s = unsafe { &*TOPOLOGY.0.get() };
    s.0.iter().take(s.1).map(|range| range.node as usize + 1).max().unwrap_or(0)
}

/// Returns a node only when the complete range has one explicit SRAT owner.
pub fn node_for_range(start:u64,length:u64)->Option<u32>{if !is_ready()||length==0{return None}let end=start.checked_add(length)?;let s=unsafe{&*TOPOLOGY.0.get()};let mut found=None;for r in s.0.iter().take(s.1){if start>=r.start&&end<=r.end{if found.is_some(){return None}found=Some(r.node)}}found}
/// Resolve the SRAT proximity domain for an APIC/x2APIC processor identifier.
/// Unknown processors stay unknown; callers may use node 0 only on UMA fallback.
pub fn node_for_apic_id(apic_id:u32)->Option<u32>{
 if !is_ready(){return None}
 let p=unsafe{&*PROCESSORS.0.get()};
 p.0.iter().take(p.1).find(|entry|entry.enabled&&entry.apic_id==apic_id).map(|entry|entry.node)
}
/// Copy validated SRAT memory ranges assigned to a node into caller storage.
/// This lets the physical allocator intersect firmware-owned free extents with
/// node ownership without scanning unrelated RAM page by page.
pub fn memory_ranges_for_node(node:u32, output:&mut [(u64,u64)]) -> usize {
 if !is_ready() { return 0; }
 let s=unsafe{&*TOPOLOGY.0.get()};
 let mut count=0usize;
 for range in s.0.iter().take(s.1).filter(|range|range.node==node) {
  if count==output.len() { break; }
  output[count]=(range.start,range.end-range.start);
  count+=1;
 }
 count
}

/// Copy only online-capable SRAT ranges. Hot-pluggable ranges are deliberately
/// withheld until the memory hot-add lifecycle can safely online and offline them.
pub fn allocatable_memory_ranges_for_node(node:u32, output:&mut [(u64,u64)]) -> usize {
 if !is_ready() { return 0; }
 let _online_guard=ONLINE_LOCK.acquire();
 let s=unsafe{&*TOPOLOGY.0.get()};
 let mut count=0usize;
 for range in s.0.iter().take(s.1).filter(|range|range.node==node&&!range.hot_pluggable) {
  if count==output.len() { break; }
  output[count]=(range.start,range.end-range.start);count+=1;
 }
 let online=unsafe{&*ONLINE_HOTPLUG.0.get()};
 for range in online.0.iter().take(online.1).filter(|range|range.2==node) {
  if count==output.len(){break}
  output[count]=(range.0,range.1-range.0);count+=1;
 }
 // Keep the result ordered for interval intersection in the allocator.
 output[..count].sort_unstable_by_key(|r|r.0);
 count
}



// Runtime state for SRAT hot-pluggable ranges. A range is not allocator-visible
// until the memory manager completes the online transaction.
struct OnlineStateLock {
 #[cfg(target_os = "none")]
 lock: crate::hal::IrqSpinLock<()>,
 #[cfg(not(target_os = "none"))]
 lock: core::sync::atomic::AtomicBool,
}
struct OnlineStateGuard<'a> {
 #[cfg(target_os = "none")]
 _irq: crate::hal::IrqSpinLockGuard<'a, ()>,
 #[cfg(not(target_os = "none"))]
 lock: &'a core::sync::atomic::AtomicBool,
}
impl Drop for OnlineStateGuard<'_> {
 fn drop(&mut self) {
  #[cfg(not(target_os = "none"))]
  self.lock.store(false, Ordering::Release);
 }
}
impl OnlineStateLock {
 const fn new()->Self {
  Self {
   #[cfg(target_os = "none")]
   lock: crate::hal::IrqSpinLock::new(()),
   #[cfg(not(target_os = "none"))]
   lock: core::sync::atomic::AtomicBool::new(false),
  }
 }
 fn acquire(&self)->OnlineStateGuard<'_> {
  #[cfg(target_os = "none")]
  { OnlineStateGuard{_irq:self.lock.lock()} }
  #[cfg(not(target_os = "none"))]
  {
   while self.lock.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err() {
    core::hint::spin_loop();
   }
   OnlineStateGuard{lock:&self.lock}
  }
 }
}
static ONLINE_LOCK:OnlineStateLock=OnlineStateLock::new();
struct OnlineHotplug(core::cell::UnsafeCell<([(u64,u64,u32);MAX_RANGES],usize)>);
unsafe impl Sync for OnlineHotplug {}
static ONLINE_HOTPLUG:OnlineHotplug=OnlineHotplug(core::cell::UnsafeCell::new(([(0,0,u32::MAX);MAX_RANGES],0)));

/// Enumerate SRAT hot-pluggable ranges for a proximity domain.
pub fn hotplug_memory_ranges_for_node(node:u32,output:&mut [(u64,u64)])->usize {
 if !is_ready(){return 0}
 let s=unsafe{&*TOPOLOGY.0.get()};let mut count=0;
 for r in s.0.iter().take(s.1).filter(|r|r.node==node&&r.hot_pluggable) {
  if count==output.len(){break}
  output[count]=(r.start,r.end-r.start);count+=1;
 }
 count
}

/// Validate that an entire aligned interval is firmware-designated hot-pluggable
/// memory owned by exactly one SRAT proximity domain.
pub fn hotplug_range_owner(start:u64,length:u64)->Option<u32> {
 if !is_ready()||length==0||start&0xfff!=0||length&0xfff!=0{return None}
 let end=start.checked_add(length)?;
 let s=unsafe{&*TOPOLOGY.0.get()};
 let mut owner=None;
 for r in s.0.iter().take(s.1).filter(|r|start<r.end&&r.start<end) {
  if !r.hot_pluggable||start<r.start||end>r.end||owner.is_some(){return None}
  owner=Some(r.node);
 }
 owner
}

/// Publish or withdraw a hot-plug range after the physical allocator has
/// completed the corresponding transaction. Callers serialize mutations.
pub fn set_hotplug_range_online(start:u64,length:u64,online:bool)->Result<(),TopologyError> {
 let node=hotplug_range_owner(start,length).ok_or(TopologyError::InvalidTable)?;
 let end=start.checked_add(length).ok_or(TopologyError::InvalidTable)?;
 let _guard=ONLINE_LOCK.acquire();
 let state=unsafe{&mut *ONLINE_HOTPLUG.0.get()};
 if online {
  if state.1==MAX_RANGES||state.0.iter().take(state.1).any(|r|start<r.1&&r.0<end) {
   return Err(TopologyError::Capacity)
  }
  state.0[state.1]=(start,end,node);state.1+=1;
 } else {
  let Some(index)=state.0.iter().take(state.1).position(|r|r.0==start&&r.1==end&&r.2==node) else {
   return Err(TopologyError::InvalidTable)
  };
  state.1-=1;state.0[index]=state.0[state.1];state.0[state.1]=(0,0,u32::MAX);
 }
 Ok(())
}

/// Query firmware attributes for a physical address. Returns None when SRAT
/// does not assign the address to exactly one accepted memory affinity range.
pub fn memory_attributes(address:u64)->Option<(u32,bool,bool)> {
 if !is_ready() { return None; }
 let s=unsafe{&*TOPOLOGY.0.get()};
 let mut found=None;
 for range in s.0.iter().take(s.1).filter(|range|address>=range.start&&address<range.end) {
  if found.is_some() { return None; }
  found=Some((range.node,range.hot_pluggable,range.non_volatile));
 }
 found
}

/// Produce a deterministic locality order: preferred node first, then ascending
/// SLIT distance. Missing SLIT entries sort last and use node-id order as a
/// stable fallback rather than inventing locality.
fn sort_allocation_nodes(
 nodes:&mut [u32],count:usize,preferred:u32,matrix:&[[u8;MAX_NODES];MAX_NODES],matrix_nodes:usize,
) {
 let distance_key=|node:u32|->u16 {
  if node==preferred{return 0}
  if (preferred as usize)<matrix_nodes&&(node as usize)<matrix_nodes {
   matrix[preferred as usize][node as usize] as u16+1
  } else {u16::MAX}
 };
 for i in 1..count {
  let value=nodes[i];let key=distance_key(value);let mut j=i;
  while j>0 {
   let previous=nodes[j-1];let previous_key=distance_key(previous);
   if previous_key<key||(previous_key==key&&previous<value){break}
   nodes[j]=previous;j-=1;
  }
  nodes[j]=value;
 }
}

/// Produce a deterministic locality order: preferred node first, then ascending
/// SLIT distance. Missing SLIT entries sort last with stable node-id ordering.
pub fn allocation_order(preferred:u32,output:&mut [u32])->usize {
 if !is_ready()||output.is_empty(){return 0}
 let s=unsafe{&*TOPOLOGY.0.get()};
 let _online_guard=ONLINE_LOCK.acquire();
 let online=unsafe{&*ONLINE_HOTPLUG.0.get()};
 let mut count=0usize;
 for r in s.0.iter().take(s.1) {
  let node=r.node;
  let mut available=!r.hot_pluggable;
  if !available {available=online.0.iter().take(online.1).any(|x|x.2==node);}
  if !available||output[..count].contains(&node){continue}
  if count==output.len(){break}
  output[count]=node;count+=1;
 }
 sort_allocation_nodes(output,count,preferred,&s.2,s.3);
 count
}

pub fn distance(from:u32,to:u32)->Option<u8>{let s=unsafe{&*TOPOLOGY.0.get()};if from as usize>=s.3||to as usize>=s.3{return None}Some(s.2[from as usize][to as usize])}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slit_fallback_orders_preferred_then_nearest_and_unknown_last() {
        let mut matrix=[[0u8;MAX_NODES];MAX_NODES];
        matrix[0][0]=10;matrix[1][1]=10;matrix[2][2]=10;
        matrix[0][1]=30;matrix[0][2]=20;
        let mut nodes=[2,1,0,3];
        sort_allocation_nodes(&mut nodes,4,0,&matrix,3);
        assert_eq!(nodes,[0,2,1,3]);
    }

    #[test]
    fn slit_fallback_is_stable_for_equal_and_missing_distances() {
        let matrix=[[10u8;MAX_NODES];MAX_NODES];
        let mut nodes=[5,3,4,2];
        sort_allocation_nodes(&mut nodes,4,9,&matrix,0);
        assert_eq!(nodes,[2,3,4,5]);
    }

    #[test]
    fn unknown_ranges_have_no_default_node() {
        assert_eq!(node_for_range(0x1000, 0x1000), None);
    }

    #[test]
    fn rejects_bad_signature() {
        assert_eq!(parse_srat(&[0; 64]), Err(TopologyError::InvalidTable));
    }

    #[test]
    fn single_node_fallback_validates_and_assigns_uma_ranges() {
        let ranges = [(0x10_0000, 0x20_0000), (0x40_0000, 0x10_0000)];
        let (staged, count) = validate_single_node_ranges(&ranges).unwrap();
        assert_eq!(count, 2);
        assert_eq!(staged[0], MemoryRange { start: 0x10_0000, end: 0x30_0000, node: 0, hot_pluggable: false, non_volatile: false });
        assert_eq!(staged[1], MemoryRange { start: 0x40_0000, end: 0x50_0000, node: 0, hot_pluggable: false, non_volatile: false });
        assert_eq!(validate_single_node_ranges(&[(0x10_0000, 0x20_0000), (0x20_0000, 0x10_0000)]), Err(TopologyError::Overlap));
        assert_eq!(validate_single_node_ranges(&[(0x10_0001, 0x1000)]), Err(TopologyError::InvalidTable));
        assert_eq!(validate_single_node_ranges(&[(u64::MAX - 0xfff, 0x2000)]), Err(TopologyError::InvalidTable));
    }

    #[test]
    fn parses_and_publishes_a_valid_memory_affinity_range() {
        // ACPI SDT header (36 bytes), SRAT fixed body (12 bytes), then one
        // 40-byte Memory Affinity structure.
        let mut table = [0u8; 128];
        table[0..4].copy_from_slice(b"SRAT");
        table[4..8].copy_from_slice(&128u32.to_le_bytes());
        table[48] = 1; // Memory Affinity
        table[49] = 40;
        table[50..54].copy_from_slice(&2u32.to_le_bytes()); // proximity domain
        table[56..64].copy_from_slice(&0x10_0000u64.to_le_bytes());
        table[64..72].copy_from_slice(&0x20_0000u64.to_le_bytes());
        table[76..80].copy_from_slice(&7u32.to_le_bytes()); // enabled, hot-pluggable, non-volatile
        table[88] = 0; // Processor Local APIC Affinity
        table[89] = 16;
        table[90] = 2; // proximity domain 2
        table[91] = 7; // APIC ID 7
        table[92..96].copy_from_slice(&1u32.to_le_bytes()); // enabled
        table[104] = 2; // Processor Local x2APIC Affinity
        table[105] = 24;
        table[108..112].copy_from_slice(&2u32.to_le_bytes()); // proximity domain at offset 4
        table[112..116].copy_from_slice(&0x123u32.to_le_bytes()); // x2APIC ID
        table[116..120].copy_from_slice(&1u32.to_le_bytes()); // enabled
        let checksum = table.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        table[9] = 0u8.wrapping_sub(checksum);

        assert_eq!(parse_srat(&table), Ok(1));
        assert_eq!(memory_node_count(), 3);
        assert_eq!(node_for_range(0x10_1000, 0x1000), Some(2));
        assert_eq!(memory_attributes(0x10_1000), Some((2, true, true)));
        assert_eq!(memory_attributes(0x40_0000), None);
        let mut all_ranges = [(0u64, 0u64); MAX_RANGES];
        assert_eq!(memory_ranges_for_node(2, &mut all_ranges), 1);
        let mut allocatable = [(0u64, 0u64); MAX_RANGES];
        assert_eq!(allocatable_memory_ranges_for_node(2, &mut allocatable), 0);
        assert_eq!(node_for_range(0x2f_f000, 0x2000), None);
        assert_eq!(node_for_apic_id(7), Some(2));
        assert_eq!(node_for_apic_id(8), None);
        assert_eq!(node_for_apic_id(0x123), Some(2));
    }
}

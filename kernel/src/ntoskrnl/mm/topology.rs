//! Bounded ACPI NUMA topology parser for x86-64 (SRAT and SLIT).
use core::sync::atomic::{AtomicBool, Ordering};
pub const MAX_NODES: usize = 64;
pub const MAX_RANGES: usize = 256;
const SRAT_SIG: &[u8;4] = b"SRAT";
const SLIT_SIG: &[u8;4] = b"SLIT";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryRange { pub start:u64, pub end:u64, pub node:u32 }
impl MemoryRange { const EMPTY:Self=Self{start:0,end:0,node:u32::MAX}; }
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
 let t=table_body(table,SRAT_SIG)?; if t.len()<48{return Err(TopologyError::InvalidTable)}
 let mut ranges=[MemoryRange::EMPTY;MAX_RANGES]; let mut count=0usize; let mut pos=48usize;
 while pos<t.len(){if pos+2>t.len(){return Err(TopologyError::InvalidTable)} let ty=t[pos];let len=t[pos+1] as usize;
 if len<2||pos.checked_add(len).filter(|e|*e<=t.len()).is_none(){return Err(TopologyError::InvalidTable)}
 if ty==1 {if len<40{return Err(TopologyError::InvalidTable)} let flags=u32at(t,pos+28).ok_or(TopologyError::InvalidTable)?;
 if flags&1!=0 {let node=u32at(t,pos+2).ok_or(TopologyError::InvalidTable)?;let start=u64at(t,pos+8).ok_or(TopologyError::InvalidTable)?;let size=u64at(t,pos+16).ok_or(TopologyError::InvalidTable)?;let end=start.checked_add(size).ok_or(TopologyError::InvalidTable)?;
 if size!=0 {if node as usize>=MAX_NODES{return Err(TopologyError::InvalidNode)} if count==MAX_RANGES{return Err(TopologyError::Capacity)} if start&0xfff!=0||end&0xfff!=0{return Err(TopologyError::InvalidTable)} ranges[count]=MemoryRange{start,end,node};count+=1;}}} pos+=len;}
 for i in 0..count{for j in 0..i{if ranges[i].start<ranges[j].end&&ranges[j].start<ranges[i].end{return Err(TopologyError::Overlap)}}}
 ranges[..count].sort_unstable_by_key(|r|r.start);unsafe{let s=&mut *TOPOLOGY.0.get();s.0=ranges;s.1=count;}READY.store(true,Ordering::Release);Ok(count)
}
/// Parse ACPI SLIT; SRAT and SLIT are independently validated.
pub fn parse_slit(table:&[u8])->Result<usize,TopologyError>{
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
  staged[index] = MemoryRange { start, end, node: 0 };
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
pub fn distance(from:u32,to:u32)->Option<u8>{let s=unsafe{&*TOPOLOGY.0.get()};if from as usize>=s.3||to as usize>=s.3{return None}Some(s.2[from as usize][to as usize])}
#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(staged[0], MemoryRange { start: 0x10_0000, end: 0x30_0000, node: 0 });
        assert_eq!(staged[1], MemoryRange { start: 0x40_0000, end: 0x50_0000, node: 0 });
        assert_eq!(validate_single_node_ranges(&[(0x10_0000, 0x20_0000), (0x20_0000, 0x10_0000)]), Err(TopologyError::Overlap));
        assert_eq!(validate_single_node_ranges(&[(0x10_0001, 0x1000)]), Err(TopologyError::InvalidTable));
        assert_eq!(validate_single_node_ranges(&[(u64::MAX - 0xfff, 0x2000)]), Err(TopologyError::InvalidTable));
    }

    #[test]
    fn parses_and_publishes_a_valid_memory_affinity_range() {
        // ACPI SDT header (36 bytes), SRAT fixed body (12 bytes), then one
        // 40-byte Memory Affinity structure.
        let mut table = [0u8; 88];
        table[0..4].copy_from_slice(b"SRAT");
        table[4..8].copy_from_slice(&88u32.to_le_bytes());
        table[48] = 1; // Memory Affinity
        table[49] = 40;
        table[50..54].copy_from_slice(&2u32.to_le_bytes()); // proximity domain
        table[56..64].copy_from_slice(&0x10_0000u64.to_le_bytes());
        table[64..72].copy_from_slice(&0x20_0000u64.to_le_bytes());
        table[76..80].copy_from_slice(&1u32.to_le_bytes()); // enabled
        let checksum = table.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        table[9] = 0u8.wrapping_sub(checksum);

        assert_eq!(parse_srat(&table), Ok(1));
        assert_eq!(memory_node_count(), 3);
        assert_eq!(node_for_range(0x10_1000, 0x1000), Some(2));
        assert_eq!(node_for_range(0x2f_f000, 0x2000), None);
    }
}

//! Intel VT-d register backend. Caller supplies validated DMAR unit and HAL/MM hooks.
//! Translation remains fail-closed until a requester context and second-level page
//! tables have been installed by the DMA domain manager.
use core::sync::atomic::{fence, Ordering};
const CAP: usize=0x08; const ECAP:usize=0x10; const GCMD:usize=0x18; const GSTS:usize=0x1c;
const RTADDR:usize=0x20; const CCMD:usize=0x28; const SRTP:u32=1<<30; const TE:u32=1<<31;
const RTPS:u32=1<<30; const TES:u32=1<<31; const ICC:u64=1<<63; const GLOBAL:u64=1<<61;
const IVT:u64=1<<63; const IIRG_GLOBAL:u64=1<<60;
pub const STATUS_SUCCESS:i32=0; pub const STATUS_INVALID_PARAMETER:i32=0xC000000Du32 as i32;
pub const STATUS_NOT_SUPPORTED:i32=0xC00000BBu32 as i32; pub const STATUS_DEVICE_NOT_READY:i32=0xC00000A3u32 as i32;
pub const STATUS_IO_TIMEOUT:i32=0xC00000B5u32 as i32; pub const STATUS_INSUFFICIENT_RESOURCES:i32=0xC000009Au32 as i32; pub const STATUS_DEVICE_BUSY:i32=0x80000011u32 as i32;

/// Platform hooks are owned by HAL/MM and must remain valid for kernel lifetime.
/// MM table pages must be zeroed, pinned, physically contiguous, and writable.
#[repr(C)] #[derive(Clone,Copy)]
pub struct PlatformOps {
 pub size:u32, pub context:*mut core::ffi::c_void,
 pub alloc_table:Option<unsafe extern "C" fn(*mut core::ffi::c_void,*mut u64,*mut *mut u64)->i32>,
 pub free_table:Option<unsafe extern "C" fn(*mut core::ffi::c_void,u64,*mut u64)->i32>,
 pub map_mmio:Option<unsafe extern "C" fn(*mut core::ffi::c_void,u64,u64,*mut u64)->i32>,
 pub unmap_mmio:Option<unsafe extern "C" fn(*mut core::ffi::c_void,u64,u64)->i32>,
 pub clean_cache:Option<unsafe extern "C" fn(*mut core::ffi::c_void,u64,u64)->i32>,
 pub quarantine_requester:Option<unsafe extern "C" fn(*mut core::ffi::c_void,u16,u8,u8,u8)->i32>,
}
unsafe impl Send for PlatformOps {} unsafe impl Sync for PlatformOps {}
#[derive(Clone,Copy)] struct Unit { segment:u16, next_domain:u16, mmio:u64, length:u64, ecap:u64, root_phys:u64, root_virt:*mut u64, max_domains:u32, ops:PlatformOps, enabled:bool }
unsafe impl Send for Unit {} unsafe impl Sync for Unit {}
const EMPTY_OPS:PlatformOps=PlatformOps{size:0,context:core::ptr::null_mut(),alloc_table:None,free_table:None,map_mmio:None,unmap_mmio:None,clean_cache:None,quarantine_requester:None};
const EMPTY_UNIT:Unit=Unit{segment:u16::MAX,next_domain:1,mmio:0,length:0,ecap:0,root_phys:0,root_virt:core::ptr::null_mut(),max_domains:0,ops:EMPTY_OPS,enabled:false};
struct UnitStore(core::cell::UnsafeCell<[Unit;8]>); unsafe impl Sync for UnitStore {}
static UNITS:UnitStore=UnitStore(core::cell::UnsafeCell::new([EMPTY_UNIT;8]));
fn unit_slot(segment:u16)->Option<usize>{unsafe{(&*UNITS.0.get()).iter().position(|u|u.enabled&&u.segment==segment)}}
#[derive(Clone,Copy)] struct BusContext { physical:u64, virtual_address:*mut u64 }
unsafe impl Send for BusContext {} unsafe impl Sync for BusContext {}
const EMPTY_CONTEXT:BusContext=BusContext{physical:0,virtual_address:core::ptr::null_mut()};
struct ContextStore(core::cell::UnsafeCell<[[BusContext;256];8]>); unsafe impl Sync for ContextStore {}
static CONTEXTS:ContextStore=ContextStore(core::cell::UnsafeCell::new([[EMPTY_CONTEXT;256];8]));
#[derive(Clone,Copy)] struct Domain { segment:u16, bus:u8, devfn:u8, id:u16, root_physical:u64, root_virtual:*mut u64, poisoned:bool }
unsafe impl Send for Domain {} unsafe impl Sync for Domain {}
const EMPTY_DOMAIN:Domain=Domain{segment:u16::MAX,bus:0,devfn:0,id:0,root_physical:0,root_virtual:core::ptr::null_mut(),poisoned:false};
struct DomainStore(core::cell::UnsafeCell<[Domain;256]>); unsafe impl Sync for DomainStore {}
static DOMAINS:DomainStore=DomainStore(core::cell::UnsafeCell::new([EMPTY_DOMAIN;256]));
static LOCK:crate::hal::IrqSpinLock<()>=crate::hal::IrqSpinLock::new(());
unsafe fn r32(b:u64,o:usize)->u32{unsafe{core::ptr::read_volatile((b as *const u8).add(o).cast())}}
unsafe fn w32(b:u64,o:usize,v:u32){unsafe{core::ptr::write_volatile((b as *mut u8).add(o).cast(),v)}}
unsafe fn r64(b:u64,o:usize)->u64{unsafe{core::ptr::read_volatile((b as *const u8).add(o).cast())}}
unsafe fn w64(b:u64,o:usize,v:u64){unsafe{core::ptr::write_volatile((b as *mut u8).add(o).cast(),v)}}
fn wait32(b:u64,o:usize,m:u32,set:bool)->bool{for _ in 0..2_000_000{if (unsafe{r32(b,o)}&m!=0)==set{return true} core::hint::spin_loop()}false}
fn wait64(b:u64,o:usize,m:u64)->bool{for _ in 0..2_000_000{if unsafe{r64(b,o)}&m==0{return true}core::hint::spin_loop()}false}

/// Validate the 48-bit, four-level translation format used by the mapper and
/// bound domain IDs to the hardware NDOMS capability and our fixed domain table.
fn supported_domain_count(cap:u64)->Option<u32>{
 let sagaw=((cap>>8)&0x1f) as u32;
 let mgaw=(((cap>>16)&0x3f)+1) as u32;
 if sagaw&(1<<2)==0||mgaw<48{return None}
 let ndoms=(cap&0x7) as u32;
 let hardware_domains=1u32.checked_shl(4+2*ndoms)?;
 Some(core::cmp::min(hardware_domains,256))
}

/// Intel VT-d root/context-table encodings. These helpers deliberately do
/// not publish entries by themselves: callers must serialize updates, clean the
/// table cache lines, and invalidate the appropriate context/IOTLB caches before
/// a device can be allowed to bus-master.
pub const VT_D_PAGE_MASK:u64=0x000f_ffff_ffff_f000;
pub const VT_D_PRESENT:u64=1;
pub const VT_D_CONTEXT_AW_4_LEVEL:u64=2;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct RequesterId { pub segment:u16, pub bus:u8, pub device:u8, pub function:u8 }
impl RequesterId {
    pub const fn new(bus:u8,device:u8,function:u8)->Option<Self>{Self::new_segment(0,bus,device,function)}
    pub const fn new_segment(segment:u16,bus:u8,device:u8,function:u8)->Option<Self>{
        if device<32&&function<8 {Some(Self{segment,bus,device,function})}else{None}
    }
    pub const fn devfn(self)->usize{((self.device as usize)<<3)|(self.function as usize)}
}

/// Encode a present root-table entry for a 4-KiB-aligned context table.
pub const fn encode_root_entry(context_table_physical:u64)->Option<u64>{
    if context_table_physical==0||context_table_physical&0xfff!=0||context_table_physical&!VT_D_PAGE_MASK!=0{return None}
    Some(context_table_physical|VT_D_PRESENT)
}

/// Encode the high 64 bits of a 4-level second-level context entry.
/// Translation type 0 selects multi-level translation; AW=2 selects 48-bit
/// guest-physical addresses. The low word is the SLPTPTR plus present bit.
pub const fn encode_context_entry(domain_id:u16,second_level_root:u64)->Option<(u64,u64)>{
    if domain_id==0||second_level_root==0||second_level_root&0xfff!=0||second_level_root&!VT_D_PAGE_MASK!=0{return None}
    let low=second_level_root|VT_D_PRESENT;
    let high=((domain_id as u64)<<8)|(VT_D_CONTEXT_AW_4_LEVEL&0x7);
    Some((low,high))
}

#[cfg(test)]
mod supported_capability_tests{
 use super::*;
 #[test]fn four_level_48bit_and_domain_limit_are_required(){
  let cap=(47u64<<16)|(1<<10)|2;
  assert_eq!(supported_domain_count(cap),Some(256));
  let cap_16_domains=(47u64<<16)|(1<<10);
  assert_eq!(supported_domain_count(cap_16_domains),Some(16));
  assert_eq!(supported_domain_count((47u64<<16)|(1<<9)|2),None);
  assert_eq!(supported_domain_count((39u64<<16)|(1<<10)|2),None);
 }
}

#[cfg(test)]
mod global_command_tests {
 use super::*;
 #[test]
 fn root_table_pointer_command_is_not_replayed_when_translation_is_enabled(){
  // Simulate a readback in which the write-one SRTP bit appears set/undefined.
  let old=0x8000_0000|SRTP|(1<<25);
  let base=old&!(TE|SRTP);
  let install_root=base|SRTP;
  let enable_translation=base|TE;
  assert_eq!(install_root&TE,0);
  assert_ne!(install_root&SRTP,0);
  assert_ne!(enable_translation&TE,0);
  assert_eq!(enable_translation&SRTP,0);
  assert_eq!(enable_translation&(1<<25),1<<25);
 }
}

#[cfg(test)]
mod context_encoding_tests {
    use super::*;
    #[test]
    fn requester_id_uses_pci_devfn_index(){
        let id=RequesterId::new(0,0x1f,7).unwrap();
        assert_eq!(id.devfn(),255);
        assert!(RequesterId::new(0,32,0).is_none());
        assert!(RequesterId::new(0,0,8).is_none());
        let other_segment=RequesterId::new_segment(7,0,0x1f,7).unwrap();
        assert_eq!(other_segment.segment,7);
        assert_ne!(id,other_segment);
        assert_eq!(RequesterId::new(0,0x1f,7).unwrap().segment,0);
    }
    #[test]
    fn root_entry_requires_aligned_representable_context_table(){
        assert_eq!(encode_root_entry(0x1234_5000),Some(0x1234_5001));
        assert_eq!(encode_root_entry(0),None);
        assert_eq!(encode_root_entry(0x1234_5001),None);
        assert_eq!(encode_root_entry(0x0010_0000_0000_0000),None);
    }
    #[test]
    fn context_entry_encodes_domain_and_four_level_translation(){
        assert_eq!(encode_context_entry(7,0x2345_6000),Some((0x2345_6001,(7<<8)|2)));
        assert_eq!(encode_context_entry(0,0x2345_6000),None);
        assert_eq!(encode_context_entry(1,0x2345_6001),None);
    }
}

/// Enable one firmware-described VT-d unit with an empty root table. Devices
/// must remain bus-master-disabled until per-requester context and second-level
/// mappings are installed. No identity mapping is created.
unsafe fn initialize_segment(ops:&PlatformOps,register_pa:u64,register_len:u64,segment:u16)->i32{
 if ops.size<core::mem::size_of::<PlatformOps>() as u32||ops.alloc_table.is_none()||ops.free_table.is_none()||ops.map_mmio.is_none()||ops.unmap_mmio.is_none()||ops.clean_cache.is_none()||ops.quarantine_requester.is_none()||register_pa==0||register_len<0x1000||register_pa.checked_add(register_len).is_none(){return STATUS_INVALID_PARAMETER}
 let _g=LOCK.lock();let units=unsafe{&mut*UNITS.0.get()};if units.iter().any(|u|u.enabled&&u.segment==segment){return STATUS_DEVICE_BUSY}let Some(unit_index)=units.iter().position(|u|!u.enabled) else{return STATUS_INSUFFICIENT_RESOURCES};let u=&mut units[unit_index];
 let mut mmio=0; if unsafe{(ops.map_mmio.unwrap())(ops.context,register_pa,register_len,&mut mmio)}!=0||mmio==0{return STATUS_NOT_SUPPORTED}
 let cap=unsafe{r64(mmio,CAP)};let ecap=unsafe{r64(mmio,ECAP)};let iro=((ecap>>8)&0x3ff) as usize*16;
 let Some(max_domains)=supported_domain_count(cap) else{unsafe{(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_NOT_SUPPORTED};
 if cap==0||ecap==0||max_domains<2||iro.checked_add(0x20).is_none_or(|e|e>register_len as usize){unsafe{(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_NOT_SUPPORTED}
 let(mut rp,mut rv)=(0u64,core::ptr::null_mut());
 if unsafe{(ops.alloc_table.unwrap())(ops.context,&mut rp,&mut rv)}!=0||rp==0||rp&0xfff!=0||rv.is_null(){
  if rp!=0&&!rv.is_null(){let _=unsafe{(ops.free_table.unwrap())(ops.context,rp,rv)};}
  unsafe{(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_INSUFFICIENT_RESOURCES
 }
 unsafe{core::ptr::write_bytes(rv,0,512)};
 if rp&!VT_D_PAGE_MASK!=0 {
  unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};
  return STATUS_NOT_SUPPORTED
 }
 if unsafe{(ops.clean_cache.unwrap())(ops.context,rp,4096)}!=0{unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_NOT_SUPPORTED}
 // SRTP is a write-one command, not a persistent GCMD state bit. Clear it
 // from the value read back from hardware and issue it exactly once while
 // translation is disabled. Replaying SRTP together with TE can cause
 // implementations to process TE first and only then reload the root table.
 let old=unsafe{r32(mmio,GCMD)};
 let base_gcmd=old&!(TE|SRTP);
 unsafe{w32(mmio,GCMD,base_gcmd)};
 if !wait32(mmio,GSTS,TES,false){unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_IO_TIMEOUT}
 unsafe{w64(mmio,RTADDR,rp);fence(Ordering::SeqCst);w32(mmio,GCMD,base_gcmd|SRTP)};
 if !wait32(mmio,GSTS,RTPS,true){unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_IO_TIMEOUT}
 unsafe{w64(mmio,CCMD,ICC|GLOBAL)};
 if !wait64(mmio,CCMD,ICC){unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_IO_TIMEOUT}
 // Root-table publication and the required cache invalidation are complete;
 // enable translation without replaying the one-shot SRTP command.
 unsafe{w32(mmio,GCMD,base_gcmd|TE)};
 if !wait32(mmio,GSTS,TES,true){unsafe{w32(mmio,GCMD,base_gcmd);let _=wait32(mmio,GSTS,TES,false);(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_IO_TIMEOUT}
 *u=Unit{segment,next_domain:1,mmio,length:register_len,ecap,root_phys:rp,root_virt:rv,max_domains,ops:*ops,enabled:true};
 drop(_g);
 let backend_status=if crate::dma::iommu::available(){STATUS_SUCCESS}else{unsafe{register_generic_dma_backend()}};
 if backend_status!=STATUS_SUCCESS{return backend_status}
 STATUS_SUCCESS
}
pub unsafe fn initialize(ops:&PlatformOps,register_pa:u64,register_len:u64)->i32{unsafe{initialize_segment(ops,register_pa,register_len,0)}}

/// Global IOTLB invalidation. A timeout is a hard failure: callers must retain
/// the DMA pages and must not report teardown as complete.
pub unsafe fn invalidate_iotlb()->i32{
 let _g=LOCK.lock();let units=unsafe{&mut*UNITS.0.get()};let mut any=false;
 for u in units.iter().filter(|u|u.enabled){any=true;let iro=((u.ecap>>8)&0x3ff) as usize*16;
  if iro.checked_add(0x20).is_none_or(|e|e>u.length as usize){return STATUS_NOT_SUPPORTED}
  unsafe{w64(u.mmio,iro+8,IVT|IIRG_GLOBAL);fence(Ordering::SeqCst)}
  if !wait64(u.mmio,iro+8,IVT){return STATUS_IO_TIMEOUT}
 }
 if any{STATUS_SUCCESS}else{STATUS_DEVICE_NOT_READY}
}
pub fn is_enabled()->bool{unsafe{(&*UNITS.0.get()).iter().any(|u|u.enabled)}}
pub fn segment_is_enabled(segment:u16)->bool{unit_slot(segment).is_some()}
pub fn root_table_physical()->Option<u64>{unsafe{(&*UNITS.0.get()).iter().find(|u|u.enabled).map(|u|u.root_phys)}}


/// Invalidate all cached requester contexts and translations. This is
/// intentionally global: the backend does not assume device-selective
/// invalidation support until the corresponding capability is validated.
unsafe fn invalidate_all_locked(u:&Unit)->i32{
 let iro=((u.ecap>>8)&0x3ff) as usize*16;
 if iro.checked_add(0x20).is_none_or(|e|e>u.length as usize){return STATUS_NOT_SUPPORTED}
 unsafe{w64(u.mmio,CCMD,ICC|GLOBAL);fence(Ordering::SeqCst)}
 if !wait64(u.mmio,CCMD,ICC){return STATUS_IO_TIMEOUT}
 unsafe{w64(u.mmio,iro+8,IVT|IIRG_GLOBAL);fence(Ordering::SeqCst)}
 if !wait64(u.mmio,iro+8,IVT){return STATUS_IO_TIMEOUT}
 STATUS_SUCCESS
}

/// Install an isolated, initially empty second-level address space for one PCI
/// requester. The caller must keep bus mastering disabled until mappings have
/// subsequently been installed by the DMA domain mapper.
///
/// The function allocates a per-bus context table and a private four-level
/// second-level root. It never creates identity mappings.
pub unsafe fn create_requester_domain(id:RequesterId)->Result<u16,i32>{
 let _guard=LOCK.lock();
 let unit_index=unit_slot(id.segment).ok_or(STATUS_DEVICE_NOT_READY)?;let units=unsafe{&mut*UNITS.0.get()};let u=&mut units[unit_index];
 let domains=unsafe{&mut*DOMAINS.0.get()};
 if domains.iter().any(|d|d.id!=0&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8){return Err(STATUS_INVALID_PARAMETER)}
 let slot=domains.iter().position(|d|d.id==0).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
 let contexts=unsafe{&mut*CONTEXTS.0.get()};
 let bus=id.bus as usize;
 if contexts[unit_index][bus].physical==0{
  let(mut pa,mut va)=(0u64,core::ptr::null_mut());
  if unsafe{(u.ops.alloc_table.unwrap())(u.ops.context,&mut pa,&mut va)}!=0||pa==0||pa&0xfff!=0||pa&!VT_D_PAGE_MASK!=0||va.is_null(){
   if pa!=0&&!va.is_null(){let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,pa,va)};}
   return Err(STATUS_INSUFFICIENT_RESOURCES)
  }
  unsafe{core::ptr::write_bytes(va,0,512)};
  if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,pa,4096)}!=0{
   let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,pa,va)};
   return Err(STATUS_NOT_SUPPORTED)
  }
  let Some(entry)=encode_root_entry(pa) else{let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,pa,va)};return Err(STATUS_INVALID_PARAMETER)};
  unsafe{core::ptr::write_volatile(u.root_virt.add(bus*2),entry)};
  if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,u.root_phys,4096)}!=0{
   // Retain the context page because the root entry may be visible to hardware
   // despite a cache-maintenance error.
   contexts[unit_index][bus]=BusContext{physical:pa,virtual_address:va};
   return Err(STATUS_NOT_SUPPORTED)
  }
  contexts[unit_index][bus]=BusContext{physical:pa,virtual_address:va};
 }
 let(mut root_pa,mut root_va)=(0u64,core::ptr::null_mut());
 if unsafe{(u.ops.alloc_table.unwrap())(u.ops.context,&mut root_pa,&mut root_va)}!=0||root_pa==0||root_pa&0xfff!=0||root_pa&!VT_D_PAGE_MASK!=0||root_va.is_null(){
  if root_pa!=0&&!root_va.is_null(){let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,root_pa,root_va)};}
  return Err(STATUS_INSUFFICIENT_RESOURCES)
 }
 unsafe{core::ptr::write_bytes(root_va,0,512)};
 let did=u.next_domain;
 if did==0||did as u32>=u.max_domains||did==u16::MAX{
  let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,root_pa,root_va)};
  return Err(STATUS_INSUFFICIENT_RESOURCES)
 }
 u.next_domain=u.next_domain.saturating_add(1);
 let Some((low,high))=encode_context_entry(did,root_pa) else{
  let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,root_pa,root_va)};
  return Err(STATUS_INVALID_PARAMETER)
 };
 let entry=unsafe{contexts[unit_index][bus].virtual_address.add(id.devfn()*2)};
 unsafe{core::ptr::write_volatile(entry,low);core::ptr::write_volatile(entry.add(1),high)};
 if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,contexts[unit_index][bus].physical,4096)}!=0{
  // Try to retract the context entry. Free the second-level root only after
  // the clear is visible and both context/IOTLB caches are invalidated.
  unsafe{core::ptr::write_volatile(entry,0);core::ptr::write_volatile(entry.add(1),0)};
  let clear_status=unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,contexts[unit_index][bus].physical,4096)};
  let invalidate_status=if clear_status==0{unsafe{invalidate_all_locked(u)}}else{STATUS_IO_TIMEOUT};
  if clear_status==0&&invalidate_status==STATUS_SUCCESS{
   let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,root_pa,root_va)};
   return Err(STATUS_NOT_SUPPORTED)
  }
  domains[slot]=Domain{segment:id.segment,bus:id.bus,devfn:id.devfn() as u8,id:did,root_physical:root_pa,root_virtual:root_va,poisoned:true};
  poison_domain(u,id);
  return Err(STATUS_IO_TIMEOUT)
 }
 domains[slot]=Domain{segment:id.segment,bus:id.bus,devfn:id.devfn() as u8,id:did,root_physical:root_pa,root_virtual:root_va,poisoned:false};
 if unsafe{invalidate_all_locked(u)}!=STATUS_SUCCESS{
  // Keep the domain and every published table page pinned. Quarantine the PCI
  // requester before returning so a caller cannot proceed with uncertain state.
  poison_domain(u,id);
  return Err(STATUS_IO_TIMEOUT)
 }
 Ok(did)
}



const SL_R:u64=1;
const SL_W:u64=2;
const SL_ADDR_MASK:u64=0x000f_ffff_ffff_f000;
const SL_MAX:u64=1u64<<48;
const SL_PAGE:u64=4096;

#[derive(Clone,Copy)] struct TablePage{used:bool,segment:u16,domain_id:u16,physical:u64,virtual_address:*mut u64}
unsafe impl Send for TablePage{} unsafe impl Sync for TablePage{}
const EMPTY_TABLE_PAGE:TablePage=TablePage{used:false,segment:u16::MAX,domain_id:0,physical:0,virtual_address:core::ptr::null_mut()};
struct TablePageStore(core::cell::UnsafeCell<[TablePage;4096]>); unsafe impl Sync for TablePageStore{}
static TABLE_PAGES:TablePageStore=TablePageStore(core::cell::UnsafeCell::new([EMPTY_TABLE_PAGE;4096]));

/// Walk or construct an intermediate second-level table. Allocated pages are
/// zeroed and cache-cleaned before their parent entry is made present.
unsafe fn walk_level(u:&Unit,segment:u16,domain_id:u16,table:*mut u64,table_pa:u64,index:usize,create:bool)->Result<*mut u64,i32>{
 let slot=unsafe{table.add(index)};
 let entry=unsafe{core::ptr::read_volatile(slot)};
 if entry&SL_R!=0{
  if entry&SL_W==0{return Err(STATUS_NOT_SUPPORTED)}
  let pa=entry&SL_ADDR_MASK;
  let pages=unsafe{&*TABLE_PAGES.0.get()};
  let Some(p)=pages.iter().find(|p|p.used&&p.segment==segment&&p.domain_id==domain_id&&p.physical==pa) else{return Err(STATUS_NOT_SUPPORTED)};
  return Ok(p.virtual_address)
 }
 if !create{return Err(STATUS_INVALID_PARAMETER)}
 let(mut pa,mut va)=(0u64,core::ptr::null_mut());
 if unsafe{(u.ops.alloc_table.unwrap())(u.ops.context,&mut pa,&mut va)}!=0
  ||pa==0||pa&0xfff!=0||pa&!SL_ADDR_MASK!=0||va.is_null(){
  if pa!=0&&!va.is_null(){let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,pa,va)};}
  return Err(STATUS_INSUFFICIENT_RESOURCES)
 }
 unsafe{core::ptr::write_bytes(va,0,512)};
 if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,pa,SL_PAGE)}!=0{
  let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,pa,va)};
  return Err(STATUS_NOT_SUPPORTED)
 }
 let pages=unsafe{&mut*TABLE_PAGES.0.get()};
 let Some(record)=pages.iter_mut().find(|p|!p.used) else{
  let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,pa,va)};
  return Err(STATUS_INSUFFICIENT_RESOURCES)
 };
 *record=TablePage{used:true,segment,domain_id,physical:pa,virtual_address:va};
 unsafe{core::ptr::write_volatile(slot,pa|SL_R|SL_W)};
 if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,table_pa,SL_PAGE)}!=0{
  // Keep the child page pinned because hardware may have observed its entry.
  return Err(STATUS_NOT_SUPPORTED)
 }
 Ok(va)
}

/// Map a contiguous physical range into a requester's IOVA space. Permissions
/// bit 0 grants device reads and bit 1 grants device writes. All addresses and
/// lengths are page-aligned. A global IOTLB invalidation completes publication.
fn poison_domain(u:&Unit,id:RequesterId){
 let domains=unsafe{&mut*DOMAINS.0.get()};
 if let Some(d)=domains.iter_mut().find(|d|d.id!=0&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8){d.poisoned=true;}
 // A failed page-table rollback is not recoverable by returning an error to a
 // caller that may release the backing pages. Require a confirmed PCI bus-master
 // revoke; if the platform cannot prove it, fail-stop before memory is reclaimed.
 let Some(quarantine)=u.ops.quarantine_requester else {
  loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
 };
 if unsafe{quarantine(u.ops.context,id.segment,id.bus,id.device,id.function)}!=0 {
  loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
 }
}
fn locate_leaf(u:&Unit,domain:&Domain,va:u64)->Result<(*mut u64,u64),i32>{
 let indexes=[((va>>39)&0x1ff)as usize,((va>>30)&0x1ff)as usize,((va>>21)&0x1ff)as usize,((va>>12)&0x1ff)as usize];
 let mut table=domain.root_virtual;let mut table_pa=domain.root_physical;
 for level in 0..3{
  let next=unsafe{walk_level(u,domain.segment,domain.id,table,table_pa,indexes[level],false)}?;
  let entry=unsafe{core::ptr::read_volatile(table.add(indexes[level]))};
  table_pa=entry&SL_ADDR_MASK;table=next;
 }
 Ok((unsafe{table.add(indexes[3])},table_pa))
}
/// Install a complete range transactionally. No leaf PTE becomes visible until
/// every intermediate path and every target leaf has passed preflight. If cache
/// publication or IOTLB invalidation fails, clear the entire new range and
/// invalidate again. An unsuccessful rollback poisons and quarantines the requester.
pub unsafe fn map_pages(id:RequesterId,iova:u64,physical:u64,length:u64,permissions:u8)->i32{
 if iova==0||physical==0||length==0||iova&0xfff!=0||physical&0xfff!=0||length&0xfff!=0
  ||permissions==0||permissions&!3!=0||iova.checked_add(length).is_none_or(|e|e>SL_MAX)
  ||physical.checked_add(length).is_none_or(|e|e>SL_MAX){return STATUS_INVALID_PARAMETER}
 let _guard=LOCK.lock();
 let Some(unit_index)=unit_slot(id.segment) else{return STATUS_DEVICE_NOT_READY};let units=unsafe{&*UNITS.0.get()};let u=&units[unit_index];
 let domains=unsafe{&*DOMAINS.0.get()};
 let Some(domain)=domains.iter().find(|d|d.id!=0&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8).copied() else{return STATUS_DEVICE_NOT_READY};
 if domain.poisoned{return STATUS_DEVICE_NOT_READY}
 // Preflight may allocate empty intermediate tables, but never installs a leaf.
 let mut offset=0;
 while offset<length{
  let va=iova+offset;
  let indexes=[((va>>39)&0x1ff)as usize,((va>>30)&0x1ff)as usize,((va>>21)&0x1ff)as usize,((va>>12)&0x1ff)as usize];
  let mut table=domain.root_virtual;let mut table_pa=domain.root_physical;
  for level in 0..3{
   let next=match unsafe{walk_level(u,domain.segment,domain.id,table,table_pa,indexes[level],true)}{Ok(v)=>v,Err(e)=>return e};
   let entry=unsafe{core::ptr::read_volatile(table.add(indexes[level]))};
   table_pa=entry&SL_ADDR_MASK;table=next;
  }
  if unsafe{core::ptr::read_volatile(table.add(indexes[3]))}&SL_R!=0{return STATUS_INVALID_PARAMETER}
  offset+=SL_PAGE;
 }
 // Publish leaves only after complete preflight.
 offset=0;
 while offset<length{
  let (leaf,_)=match locate_leaf(u,&domain,iova+offset){Ok(v)=>v,Err(e)=>{poison_domain(u,id);return e}};
  unsafe{core::ptr::write_volatile(leaf,((physical+offset)&SL_ADDR_MASK)|(permissions as u64));}
  offset+=SL_PAGE;
 }
 // Flush each leaf's containing PT page. Repeated flushes are conservative;
 // no mapping is returned to the caller until the global IOTLB invalidation ends.
 offset=0;let mut status=STATUS_SUCCESS;
 while offset<length{
  let (_,table_pa)=match locate_leaf(u,&domain,iova+offset){Ok(v)=>v,Err(e)=>{status=e;break}};
  if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,table_pa,SL_PAGE)}!=0{status=STATUS_NOT_SUPPORTED;break}
  offset+=SL_PAGE;
 }
 if status==STATUS_SUCCESS{status=unsafe{invalidate_all_locked(u)};}
 if status==STATUS_SUCCESS{return STATUS_SUCCESS}
 // Roll back every leaf; keep all intermediate tables allocated (empty tables
 // are safe and avoiding their reclamation prevents parent-pointer races).
 offset=0;let mut rollback_ok=true;
 while offset<length{
  match locate_leaf(u,&domain,iova+offset){
   Ok((leaf,table_pa))=>{
    unsafe{core::ptr::write_volatile(leaf,0);}
    if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,table_pa,SL_PAGE)}!=0{rollback_ok=false;}
   }
   Err(_)=>rollback_ok=false,
  }
  offset+=SL_PAGE;
 }
 if unsafe{invalidate_all_locked(u)}!=STATUS_SUCCESS{rollback_ok=false}
 if !rollback_ok{poison_domain(u,id);}
 status
}

/// Remove a complete mapping. The registry owner retains the backing allocation
/// on any error. A partial clear or failed invalidation poisons and quarantines
/// the requester, preventing later bus mastering against uncertain translations.
pub unsafe fn unmap_pages(id:RequesterId,iova:u64,length:u64)->i32{
 if iova==0||length==0||iova&0xfff!=0||length&0xfff!=0||iova.checked_add(length).is_none_or(|e|e>SL_MAX){return STATUS_INVALID_PARAMETER}
 let _guard=LOCK.lock();
 let units=unsafe{&*UNITS.0.get()};let Some(unit_index)=unit_slot(id.segment) else{return STATUS_DEVICE_NOT_READY};let u=&units[unit_index];
 let domains=unsafe{&*DOMAINS.0.get()};
 let Some(domain)=domains.iter().find(|d|d.id!=0&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8).copied() else{return STATUS_DEVICE_NOT_READY};
 if domain.poisoned{return STATUS_DEVICE_NOT_READY}
 // Validate the entire range before clearing its first PTE.
 let mut offset=0;
 while offset<length{
  let (leaf,_)=match locate_leaf(u,&domain,iova+offset){Ok(v)=>v,Err(e)=>return e};
  if unsafe{core::ptr::read_volatile(leaf)}&SL_R==0{return STATUS_INVALID_PARAMETER}
  offset+=SL_PAGE;
 }
 offset=0;let mut status=STATUS_SUCCESS;
 while offset<length{
  let (leaf,table_pa)=match locate_leaf(u,&domain,iova+offset){Ok(v)=>v,Err(e)=>{status=e;break}};
  unsafe{core::ptr::write_volatile(leaf,0);}
  if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,table_pa,SL_PAGE)}!=0{status=STATUS_NOT_SUPPORTED;break}
  offset+=SL_PAGE;
 }
 if status==STATUS_SUCCESS{status=unsafe{invalidate_all_locked(u)};}
 if status!=STATUS_SUCCESS{poison_domain(u,id);}
 status
}

/// Software page-table lookup for tests and diagnostics; not a hardware test.
pub fn lookup_mapping(id:RequesterId,iova:u64)->Option<(u64,u8)>{
 if iova>=SL_MAX{return None}
 let _guard=LOCK.lock();
 let domains=unsafe{&*DOMAINS.0.get()};
 let d=domains.iter().find(|d|d.id!=0&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8)?;
 let pages=unsafe{&*TABLE_PAGES.0.get()};
 let indexes=[((iova>>39)&0x1ff)as usize,((iova>>30)&0x1ff)as usize,((iova>>21)&0x1ff)as usize,((iova>>12)&0x1ff)as usize];
 let mut table=d.root_virtual;
 for (level,index) in indexes.iter().enumerate(){
  let e=unsafe{core::ptr::read_volatile(table.add(*index))};
  if e&SL_R==0{return None}
  if level==3{return Some(((e&SL_ADDR_MASK)|(iova&0xfff),(e&3)as u8))}
  let pa=e&SL_ADDR_MASK;
  table=pages.iter().find(|p|p.used&&p.segment==d.segment&&p.domain_id==d.id&&p.physical==pa)?.virtual_address;
 }
 None
}

/// Remove a requester domain only after all device DMA has been stopped. On
/// invalidation failure the domain and its page tables remain allocated.
pub unsafe fn destroy_requester_domain(id:RequesterId)->i32{
 // A firmware RMRR remains part of the live requester contract. It must be
 // explicitly unmapped after bus mastering is revoked before the context can go.
 if DMA_MAPS.lock().iter().any(|m|m.used&&m.cookie==0
    &&requester_for_device(m.device).is_some_and(|r|r.segment==id.segment&&r.bus==id.bus&&r.device==id.device&&r.function==id.function)) {
    return STATUS_DEVICE_BUSY;
 }
 let _guard=LOCK.lock();
 let Some(unit_index)=unit_slot(id.segment) else{return STATUS_DEVICE_NOT_READY};let units=unsafe{&mut*UNITS.0.get()};let u=&mut units[unit_index];
 let domains=unsafe{&mut*DOMAINS.0.get()};
 let Some(index)=domains.iter().position(|d|d.id!=0&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8) else{return STATUS_INVALID_PARAMETER};
 let d=domains[index];
 // Close the software admission gate before detaching any hardware context.
 // A failed free/invalidation then leaves an unusable but fully pinned domain.
 domains[index].poisoned=true;
 let contexts=unsafe{&mut*CONTEXTS.0.get()};
 let entry=unsafe{contexts[unit_index][id.bus as usize].virtual_address.add(id.devfn()*2)};
 unsafe{core::ptr::write_volatile(entry,0);core::ptr::write_volatile(entry.add(1),0)};
 if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,contexts[unit_index][id.bus as usize].physical,4096)}!=0{return STATUS_NOT_SUPPORTED}
 if unsafe{invalidate_all_locked(u)}!=STATUS_SUCCESS{return STATUS_IO_TIMEOUT}
 let pages=unsafe{&mut*TABLE_PAGES.0.get()};
 for page in pages.iter_mut().filter(|p|p.used&&p.segment==d.segment&&p.domain_id==d.id){
  if unsafe{(u.ops.free_table.unwrap())(u.ops.context,page.physical,page.virtual_address)}!=0{return STATUS_NOT_SUPPORTED}
  *page=EMPTY_TABLE_PAGE;
 }
 let status=unsafe{(u.ops.free_table.unwrap())(u.ops.context,d.root_physical,d.root_virtual)};
 if status!=0{return STATUS_NOT_SUPPORTED}
 domains[index]=EMPTY_DOMAIN;
 if !domains.iter().any(|x|x.id!=0&&x.segment==id.segment&&x.bus==id.bus){
  let c=contexts[unit_index][id.bus as usize];
  unsafe{core::ptr::write_volatile(u.root_virt.add(id.bus as usize*2),0)};
  if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,u.root_phys,4096)}!=0{return STATUS_NOT_SUPPORTED}
  if unsafe{invalidate_all_locked(u)}!=STATUS_SUCCESS{return STATUS_IO_TIMEOUT}
  if unsafe{(u.ops.free_table.unwrap())(u.ops.context,c.physical,c.virtual_address)}!=0{return STATUS_NOT_SUPPORTED}
  contexts[unit_index][id.bus as usize]=EMPTY_CONTEXT;
 }
 STATUS_SUCCESS
}

/// Return the private second-level root for diagnostics and DMA mapper setup.
/// The returned physical address is not a DMA address and must never be exposed
/// to a device.
pub fn requester_domain_exists(id:RequesterId)->bool{
 let _guard=LOCK.lock();let domains=unsafe{&*DOMAINS.0.get()};
 domains.iter().any(|d|d.id!=0&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8)
}
pub fn requester_domain_usable(id:RequesterId)->bool{
 let _guard=LOCK.lock();let domains=unsafe{&*DOMAINS.0.get()};
 domains.iter().any(|d|d.id!=0&&!d.poisoned&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8)
}
pub fn requester_domain_root(id:RequesterId)->Option<(u16,u64)>{
 let _guard=LOCK.lock();
 let domains=unsafe{&*DOMAINS.0.get()};
 domains.iter().find(|d|d.id!=0&&!d.poisoned&&d.segment==id.segment&&d.bus==id.bus&&d.devfn==id.devfn() as u8).map(|d|(d.id,d.root_physical))
}


// Generic DMA backend bridge. IOVAs are allocated from a private 32-bit window;
// the physical page offset is preserved for byte-granular WDM transfers.
const DMA_IOVA_START:u64=0x1_0000;
const DMA_IOVA_END:u64=0x1_0000_0000;
const MAX_DMA_MAPS:usize=512;
#[derive(Clone,Copy)]
struct DmaMap { used:bool, ready:bool, busy:bool, device:usize, iova:u64, span:u64, cookie:u64 }
impl DmaMap { const EMPTY:Self=Self{used:false,ready:false,busy:false,device:0,iova:0,span:0,cookie:0}; }
static DMA_MAPS:crate::hal::IrqSpinLock<[DmaMap;MAX_DMA_MAPS]>=
 crate::hal::IrqSpinLock::new([DmaMap::EMPTY;MAX_DMA_MAPS]);
static NEXT_DMA_IOVA:core::sync::atomic::AtomicU64=
 core::sync::atomic::AtomicU64::new(DMA_IOVA_START);

fn requester_for_device(device:usize)->Option<RequesterId>{
 let (segment,bus,devfn)=crate::dma::iommu::requester_identity(device)?;
 RequesterId::new_segment(segment,bus,devfn>>3,devfn&7)
}

unsafe extern "system" fn generic_dma_map(
 _context:*mut core::ffi::c_void,device:usize,physical:u64,length:u32,direction:u8,
 logical_out:*mut u64,cookie_out:*mut u64
)->i32{
 if logical_out.is_null()||cookie_out.is_null()||device==0||length==0{return STATUS_INVALID_PARAMETER}
 let Some(id)=requester_for_device(device) else{return STATUS_NOT_SUPPORTED};
 if !is_enabled(){return STATUS_DEVICE_NOT_READY}
 if requester_domain_exists(id){
  if !requester_domain_usable(id){return STATUS_DEVICE_NOT_READY}
 }else if unsafe{create_requester_domain(id)}.is_err(){return STATUS_NOT_SUPPORTED}
 let offset=physical&(SL_PAGE-1);
 let physical_base=physical&!(SL_PAGE-1);
 let span=match offset.checked_add(length as u64).and_then(|n|n.checked_add(SL_PAGE-1)){Some(n)=>n&!(SL_PAGE-1),None=>return STATUS_INVALID_PARAMETER};
 let span=span.max(SL_PAGE);
 let permissions=match direction{0=>2,1=>1,2=>3,_=>return STATUS_INVALID_PARAMETER};
 // Reserve a free IOVA and registry slot atomically. First-fit allocation
 // reuses holes after completed transfers and prevents concurrent overlap.
 let (slot_index,base)={
  let mut maps=DMA_MAPS.lock();
  let Some(index)=maps.iter().position(|m|!m.used) else{return STATUS_INSUFFICIENT_RESOURCES};
  let mut candidate=DMA_IOVA_START;
  loop{
   let mut next=None;
   for m in maps.iter().filter(|m|m.used){
    let candidate_end=match candidate.checked_add(span){Some(v)=>v,None=>return STATUS_INSUFFICIENT_RESOURCES};
    if candidate<m.iova.saturating_add(m.span)&&m.iova<candidate_end{
     let e=m.iova.saturating_add(m.span);
     next=Some(next.unwrap_or(0u64).max(e));
    }
   }
   match next{Some(e)=>candidate=match e.checked_add(SL_PAGE-1){Some(v)=>v&!(SL_PAGE-1),None=>return STATUS_INSUFFICIENT_RESOURCES},None=>break}
   if candidate.checked_add(span).is_none_or(|e|e>DMA_IOVA_END){return STATUS_INSUFFICIENT_RESOURCES}
  }
  if candidate.checked_add(span).is_none_or(|e|e>DMA_IOVA_END){return STATUS_INSUFFICIENT_RESOURCES}
  maps[index]=DmaMap{used:true,ready:false,busy:true,device,iova:candidate,span,cookie:candidate};
  (index,candidate)
 };
 let status=unsafe{map_pages(id,base,physical_base,span,permissions)};
 if status!=STATUS_SUCCESS{DMA_MAPS.lock()[slot_index]=DmaMap::EMPTY;return status}
 {let mut maps=DMA_MAPS.lock();maps[slot_index].ready=true;maps[slot_index].busy=false;}
 unsafe{logical_out.write(base+offset);cookie_out.write(base);}
 STATUS_SUCCESS
}

unsafe extern "system" fn generic_dma_unmap(
 _context:*mut core::ffi::c_void,device:usize,logical:u64,cookie:u64
)->i32{
 if device==0||logical==0||cookie==0{return STATUS_INVALID_PARAMETER}
 let Some(id)=requester_for_device(device) else{return STATUS_NOT_SUPPORTED};
 let (index,record)={
  let mut maps=DMA_MAPS.lock();
  let Some((i,m))=maps.iter_mut().enumerate().find(|(_,m)|m.used&&m.ready&&!m.busy&&m.device==device&&m.cookie==cookie
   &&logical>=m.iova&&logical<m.iova.saturating_add(m.span)) else{return STATUS_INVALID_PARAMETER};
  m.busy=true;(i,*m)
 };
 let status=unsafe{unmap_pages(id,record.iova,record.span)};
 let mut maps=DMA_MAPS.lock();
 if status==STATUS_SUCCESS{maps[index]=DmaMap::EMPTY;}else{maps[index].busy=false;}
 status
}

unsafe extern "system" fn generic_dma_sync(
 _context:*mut core::ffi::c_void,device:usize,logical:u64,length:u32,_direction:u8
)->i32{
 if device==0||logical==0||length==0{return STATUS_INVALID_PARAMETER}
 let end=match logical.checked_add(length as u64){Some(v)=>v,None=>return STATUS_INVALID_PARAMETER};
 let maps=DMA_MAPS.lock();
 if !maps.iter().any(|m|m.used&&m.ready&&!m.busy&&m.cookie!=0&&m.device==device
  &&logical>=m.iova&&end<=m.iova.saturating_add(m.span)){return STATUS_INVALID_PARAMETER}
 drop(maps);
 fence(Ordering::SeqCst);
 unsafe{core::arch::asm!("mfence",options(nostack,preserves_flags));}
 STATUS_SUCCESS
}


/// Install the ACPI DMAR reserved-memory identity mapping for one scoped device.
/// The physical pages remain owned by firmware/ACPI and are never allocated or
/// freed by this mapper. A cookie of zero marks the IOVA reservation so ordinary
/// DMA allocations cannot overlap it and ordinary DMA unmap cannot remove it.
pub unsafe fn map_rmrr_identity(device:usize,id:RequesterId,base:u64,length:u64)->i32 {
 if device==0||base==0||base&0xfff!=0||length==0||length&0xfff!=0
    ||base.checked_add(length).is_none_or(|end|end>SL_MAX){return STATUS_INVALID_PARAMETER}
 let Some(registered)=requester_for_device(device) else{return STATUS_NOT_SUPPORTED};
 if registered.segment!=id.segment||registered.bus!=id.bus||registered.device!=id.device||registered.function!=id.function {
    return STATUS_INVALID_PARAMETER;
 }
 if !requester_domain_exists(id) {
    if unsafe{create_requester_domain(id)}.is_err(){return STATUS_INSUFFICIENT_RESOURCES}
 } else if !requester_domain_usable(id) {return STATUS_DEVICE_NOT_READY}
 let index={
    let mut maps=DMA_MAPS.lock();
    if maps.iter().any(|m|m.used&&m.device==device&&m.cookie==0&&m.iova==base&&m.span==length&&m.ready) {
        return STATUS_SUCCESS;
    }
    let end=base+length;
    if maps.iter().any(|m|m.used&&base<m.iova.saturating_add(m.span)&&m.iova<end) {
        return STATUS_DEVICE_BUSY;
    }
    let Some(index)=maps.iter().position(|m|!m.used) else{return STATUS_INSUFFICIENT_RESOURCES};
    maps[index]=DmaMap{used:true,ready:false,busy:true,device,iova:base,span:length,cookie:0};
    index
 };
 let status=unsafe{map_pages(id,base,base,length,3)};
 let mut maps=DMA_MAPS.lock();
 if status==STATUS_SUCCESS {
    maps[index].ready=true;maps[index].busy=false;
 } else if requester_domain_usable(id) {
    maps[index]=DmaMap::EMPTY;
 } // Otherwise retain the reservation while the poisoned domain is quarantined.
 status
}

/// Report whether a PnP device still owns a firmware-reserved IOVA reservation.
/// The requester identity must not be unregistered while this is true.
pub fn requester_has_reserved_rmrr(device:usize)->bool {
 if device==0{return false}
 DMA_MAPS.lock().iter().any(|m|m.used&&m.device==device&&m.cookie==0)
}

/// Remove an RMRR identity mapping only after the caller has stopped the device
/// and revoked bus mastering. Firmware-owned pages are not freed by this API.
pub unsafe fn unmap_rmrr_identity(device:usize,id:RequesterId,base:u64,length:u64)->i32 {
 if device==0||base==0||length==0{return STATUS_INVALID_PARAMETER}
 let index={
    let mut maps=DMA_MAPS.lock();
    if maps.iter().any(|m|m.used&&m.device==device&&m.cookie!=0){return STATUS_DEVICE_BUSY}
    let Some((i,m))=maps.iter_mut().enumerate().find(|(_,m)|m.used&&m.device==device
        &&m.cookie==0&&m.iova==base&&m.span==length&&m.ready&&!m.busy) else {
        return STATUS_INVALID_PARAMETER
    };
    m.busy=true;i
 };
 let status=unsafe{unmap_pages(id,base,length)};
 let mut maps=DMA_MAPS.lock();
 if status==STATUS_SUCCESS{maps[index]=DmaMap::EMPTY;}else{maps[index].busy=false;}
 status
}

/// Register the VT-d mapper as the generic DMA backend. Call only after
/// initialize() has enabled translation; requester BDFs are registered by PnP.
/// The generic backend copies this descriptor into permanent kernel storage.
pub unsafe fn register_generic_dma_backend()->i32{
 if !is_enabled(){return STATUS_DEVICE_NOT_READY}
 let mut ops=crate::dma::iommu::IommuBackend{
  size:core::mem::size_of::<crate::dma::iommu::IommuBackend>() as u32,
  context:core::ptr::null_mut(),
  map:Some(generic_dma_map),
  unmap:Some(generic_dma_unmap),
  sync:Some(generic_dma_sync),
 };
 unsafe{crate::dma::iommu::RdotDmaRegisterIommuBackend(&mut ops)}
}


/// Initialize the segment-owned VT-d unit only after resolving the exact
/// firmware DRHD for the endpoint's complete PCI path. The PnP caller installs
/// every matching, firmware-reserved RMRR identity mapping before driver bind;
/// conventional or unverified ranges remain fail-closed.
pub unsafe fn initialize_for_dmar_path(
    ops:&PlatformOps,
    dmar:&cognacos_acpi::DmarInfo,
    segment:u16,
    root_bus:u8,
    endpoint_path:&[cognacos_acpi::DmarPath],
    register_len:u64,
)->i32 {
    // Domain and context-table stores are keyed by PCI segment as well as BDF.
    if endpoint_path.is_empty()||endpoint_path.len()>8 {
        return STATUS_NOT_SUPPORTED;
    }
    let unit=match cognacos_acpi::select_dmar_unit(dmar,segment,root_bus,endpoint_path) {
        Ok(v)=>v,Err(_)=>return STATUS_NOT_SUPPORTED,
    };
    // Matching RMRRs are installed as pinned identity mappings by the PnP
    // caller before the controller is handed to its driver.
    if unit.register_base==0||unit.register_base&0xfff!=0 {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe{initialize_segment(ops,unit.register_base,register_len,segment)}
}


/// PCI-topology adapter: carry the segment and the complete bridge path from
/// the PCI enumerator directly into DMAR ownership selection. The final path
/// element must be the endpoint BDF discovered by PCI, preventing a bridge BDF
/// or stale subordinate-bus route from being used as the requester.
pub unsafe fn initialize_for_pci_path(
    ops:&PlatformOps,
    dmar:&cognacos_acpi::DmarInfo,
    path:&cognacos_pci::PciRequesterPath,
    register_len:u64,
)->i32 {
    let count=path.path_count as usize;
    if count==0||count>8{return STATUS_INVALID_PARAMETER}
    let last=path.path[count-1];
    if last.device!=path.endpoint_device||last.function!=path.endpoint_function {
        return STATUS_INVALID_PARAMETER;
    }
    let mut scopes=[cognacos_acpi::DmarPath{device:0,function:0};8];
    for i in 0..count {
        scopes[i]=cognacos_acpi::DmarPath {
            device:path.path[i].device,function:path.path[i].function,
        };
    }
    unsafe {
        initialize_for_dmar_path(ops,dmar,path.segment,path.root_bus,
            &scopes[..count],register_len)
    }
}

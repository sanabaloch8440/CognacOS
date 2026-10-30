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
pub const STATUS_IO_TIMEOUT:i32=0xC00000B5u32 as i32; pub const STATUS_INSUFFICIENT_RESOURCES:i32=0xC000009Au32 as i32;

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
}
unsafe impl Send for PlatformOps {} unsafe impl Sync for PlatformOps {}
#[derive(Clone,Copy)] struct Unit { mmio:u64, length:u64, ecap:u64, root_phys:u64, root_virt:*mut u64, ops:PlatformOps, enabled:bool }
unsafe impl Send for Unit {} unsafe impl Sync for Unit {}
struct Store(core::cell::UnsafeCell<Unit>); unsafe impl Sync for Store {}
static UNIT:Store=Store(core::cell::UnsafeCell::new(Unit{mmio:0,length:0,ecap:0,root_phys:0,root_virt:core::ptr::null_mut(),ops:PlatformOps{size:0,context:core::ptr::null_mut(),alloc_table:None,free_table:None,map_mmio:None,unmap_mmio:None,clean_cache:None},enabled:false}));
#[derive(Clone,Copy)] struct BusContext { physical:u64, virtual_address:*mut u64 }
unsafe impl Send for BusContext {} unsafe impl Sync for BusContext {}
const EMPTY_CONTEXT:BusContext=BusContext{physical:0,virtual_address:core::ptr::null_mut()};
struct ContextStore(core::cell::UnsafeCell<[BusContext;256]>); unsafe impl Sync for ContextStore {}
static CONTEXTS:ContextStore=ContextStore(core::cell::UnsafeCell::new([EMPTY_CONTEXT;256]));
#[derive(Clone,Copy)] struct Domain { bus:u8, devfn:u8, id:u16, root_physical:u64, root_virtual:*mut u64 }
unsafe impl Send for Domain {} unsafe impl Sync for Domain {}
const EMPTY_DOMAIN:Domain=Domain{bus:0,devfn:0,id:0,root_physical:0,root_virtual:core::ptr::null_mut()};
struct DomainStore(core::cell::UnsafeCell<[Domain;256]>); unsafe impl Sync for DomainStore {}
static DOMAINS:DomainStore=DomainStore(core::cell::UnsafeCell::new([EMPTY_DOMAIN;256]));
static NEXT_DOMAIN:core::sync::atomic::AtomicU16=core::sync::atomic::AtomicU16::new(1);
static LOCK:crate::hal::IrqSpinLock<()>=crate::hal::IrqSpinLock::new(());
unsafe fn r32(b:u64,o:usize)->u32{unsafe{core::ptr::read_volatile((b as *const u8).add(o).cast())}}
unsafe fn w32(b:u64,o:usize,v:u32){unsafe{core::ptr::write_volatile((b as *mut u8).add(o).cast(),v)}}
unsafe fn r64(b:u64,o:usize)->u64{unsafe{core::ptr::read_volatile((b as *const u8).add(o).cast())}}
unsafe fn w64(b:u64,o:usize,v:u64){unsafe{core::ptr::write_volatile((b as *mut u8).add(o).cast(),v)}}
fn wait32(b:u64,o:usize,m:u32,set:bool)->bool{for _ in 0..2_000_000{if (unsafe{r32(b,o)}&m!=0)==set{return true} core::hint::spin_loop()}false}
fn wait64(b:u64,o:usize,m:u64)->bool{for _ in 0..2_000_000{if unsafe{r64(b,o)}&m==0{return true}core::hint::spin_loop()}false}

/// Intel VT-d root/context-table encodings. These helpers deliberately do
/// not publish entries by themselves: callers must serialize updates, clean the
/// table cache lines, and invalidate the appropriate context/IOTLB caches before
/// a device can be allowed to bus-master.
pub const VT_D_PAGE_MASK:u64=0x000f_ffff_ffff_f000;
pub const VT_D_PRESENT:u64=1;
pub const VT_D_CONTEXT_AW_4_LEVEL:u64=2;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct RequesterId { pub bus:u8, pub device:u8, pub function:u8 }
impl RequesterId {
    pub const fn new(bus:u8,device:u8,function:u8)->Option<Self>{
        if device<32&&function<8 {Some(Self{bus,device,function})}else{None}
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
    let high=((domain_id as u64)<<8)|(VT_D_CONTEXT_AW_4_LEVEL<<3);
    Some((low,high))
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
        assert_eq!(encode_context_entry(7,0x2345_6000),Some((0x2345_6001,(7<<8)|(2<<3))));
        assert_eq!(encode_context_entry(0,0x2345_6000),None);
        assert_eq!(encode_context_entry(1,0x2345_6001),None);
    }
}

/// Enable one firmware-described VT-d unit with an empty root table. Devices
/// must remain bus-master-disabled until per-requester context and second-level
/// mappings are installed. No identity mapping is created.
pub unsafe fn initialize(ops:&PlatformOps,register_pa:u64,register_len:u64)->i32{
 if ops.size<core::mem::size_of::<PlatformOps>() as u32||ops.alloc_table.is_none()||ops.free_table.is_none()||ops.map_mmio.is_none()||ops.unmap_mmio.is_none()||ops.clean_cache.is_none()||register_pa==0||register_len<0x1000||register_pa.checked_add(register_len).is_none(){return STATUS_INVALID_PARAMETER}
 let _g=LOCK.lock();let u=unsafe{&mut*UNIT.0.get()};if u.enabled{return STATUS_DEVICE_NOT_READY}
 let mut mmio=0; if unsafe{(ops.map_mmio.unwrap())(ops.context,register_pa,register_len,&mut mmio)}!=0||mmio==0{return STATUS_NOT_SUPPORTED}
 let cap=unsafe{r64(mmio,CAP)};let ecap=unsafe{r64(mmio,ECAP)};let iro=((ecap>>8)&0x3ff) as usize*16;
 if cap==0||ecap==0||iro.checked_add(0x20).is_none_or(|e|e>register_len as usize){unsafe{(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_NOT_SUPPORTED}
 let(mut rp,mut rv)=(0u64,core::ptr::null_mut());
 if unsafe{(ops.alloc_table.unwrap())(ops.context,&mut rp,&mut rv)}!=0||rp==0||rp&0xfff!=0||rv.is_null(){
  if rp!=0&&!rv.is_null(){let _=unsafe{(ops.free_table.unwrap())(ops.context,rp,rv)};}
  unsafe{(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_INSUFFICIENT_RESOURCES
 }
 unsafe{core::ptr::write_bytes(rv,0,512)};
 if unsafe{(ops.clean_cache.unwrap())(ops.context,rp,4096)}!=0{unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_NOT_SUPPORTED}
 let old=unsafe{r32(mmio,GCMD)};unsafe{w32(mmio,GCMD,old&!TE)};
 if !wait32(mmio,GSTS,TES,false){unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_IO_TIMEOUT}
 unsafe{w64(mmio,RTADDR,rp);fence(Ordering::SeqCst);w32(mmio,GCMD,(old&!TE)|SRTP)};
 if !wait32(mmio,GSTS,RTPS,true){unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_IO_TIMEOUT}
 unsafe{w64(mmio,CCMD,ICC|GLOBAL)};
 if !wait64(mmio,CCMD,ICC){unsafe{(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_IO_TIMEOUT}
 unsafe{w32(mmio,GCMD,(old&!TE)|SRTP|TE)};
 if !wait32(mmio,GSTS,TES,true){unsafe{w32(mmio,GCMD,(old&!TE)|SRTP);let _=wait32(mmio,GSTS,TES,false);(ops.free_table.unwrap())(ops.context,rp,rv);(ops.unmap_mmio.unwrap())(ops.context,mmio,register_len)};return STATUS_IO_TIMEOUT}
 *u=Unit{mmio,length:register_len,ecap,root_phys:rp,root_virt:rv,ops:*ops,enabled:true};STATUS_SUCCESS
}

/// Global IOTLB invalidation. A timeout is a hard failure: callers must retain
/// the DMA pages and must not report teardown as complete.
pub unsafe fn invalidate_iotlb()->i32{
 let _g=LOCK.lock();let u=unsafe{&mut*UNIT.0.get()};if !u.enabled{return STATUS_DEVICE_NOT_READY}
 let iro=((u.ecap>>8)&0x3ff) as usize*16;if iro.checked_add(0x20).is_none_or(|e|e>u.length as usize){return STATUS_NOT_SUPPORTED}
 unsafe{w64(u.mmio,iro+8,IVT|IIRG_GLOBAL);fence(Ordering::SeqCst)}
 if !wait64(u.mmio,iro+8,IVT){return STATUS_IO_TIMEOUT}STATUS_SUCCESS
}
pub fn is_enabled()->bool{unsafe{(&*UNIT.0.get()).enabled}}
pub fn root_table_physical()->Option<u64>{let u=unsafe{&*UNIT.0.get()};if u.enabled{Some(u.root_phys)}else{None}}


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
 let u=unsafe{&mut*UNIT.0.get()};
 if !u.enabled{return Err(STATUS_DEVICE_NOT_READY)}
 let domains=unsafe{&mut*DOMAINS.0.get()};
 if domains.iter().any(|d|d.id!=0&&d.bus==id.bus&&d.devfn==id.devfn() as u8){return Err(STATUS_INVALID_PARAMETER)}
 let slot=domains.iter().position(|d|d.id==0).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
 let contexts=unsafe{&mut*CONTEXTS.0.get()};
 let bus=id.bus as usize;
 let mut allocated_context=false;
 if contexts[bus].physical==0{
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
  unsafe{core::ptr::write_volatile(u.root_virt.add(bus),entry)};
  contexts[bus]=BusContext{physical:pa,virtual_address:va};
  allocated_context=true;
 }
 let(mut root_pa,mut root_va)=(0u64,core::ptr::null_mut());
 if unsafe{(u.ops.alloc_table.unwrap())(u.ops.context,&mut root_pa,&mut root_va)}!=0||root_pa==0||root_pa&0xfff!=0||root_pa&!VT_D_PAGE_MASK!=0||root_va.is_null(){
  if root_pa!=0&&!root_va.is_null(){let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,root_pa,root_va)};}
  if allocated_context{unsafe{core::ptr::write_volatile(u.root_virt.add(bus),0)};contexts[bus]=EMPTY_CONTEXT;}
  return Err(STATUS_INSUFFICIENT_RESOURCES)
 }
 unsafe{core::ptr::write_bytes(root_va,0,512)};
 let did=NEXT_DOMAIN.fetch_add(1,Ordering::AcqRel);
 if did==0||did==u16::MAX{
  let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,root_pa,root_va)};
  if allocated_context{unsafe{core::ptr::write_volatile(u.root_virt.add(bus),0)};contexts[bus]=EMPTY_CONTEXT;}
  return Err(STATUS_INSUFFICIENT_RESOURCES)
 }
 let Some((low,high))=encode_context_entry(did,root_pa) else{
  let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,root_pa,root_va)};
  return Err(STATUS_INVALID_PARAMETER)
 };
 let entry=unsafe{contexts[bus].virtual_address.add(id.devfn()*2)};
 unsafe{core::ptr::write_volatile(entry,low);core::ptr::write_volatile(entry.add(1),high)};
 if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,contexts[bus].physical,4096)}!=0{
  unsafe{core::ptr::write_volatile(entry,0);core::ptr::write_volatile(entry.add(1),0)};
  let _=unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,contexts[bus].physical,4096)};
  let _=unsafe{(u.ops.free_table.unwrap())(u.ops.context,root_pa,root_va)};
  return Err(STATUS_NOT_SUPPORTED)
 }
 domains[slot]=Domain{bus:id.bus,devfn:id.devfn() as u8,id:did,root_physical:root_pa,root_virtual:root_va};
 if unsafe{invalidate_all_locked(u)}!=STATUS_SUCCESS{
  // Keep the domain discoverable and all published table pages pinned: hardware
  // may have cached a partial context update, so freeing them is unsafe.
  return Err(STATUS_IO_TIMEOUT)
 }
 Ok(did)
}

/// Remove a requester domain only after all device DMA has been stopped. On
/// invalidation failure the domain and its page tables remain allocated.
pub unsafe fn destroy_requester_domain(id:RequesterId)->i32{
 let _guard=LOCK.lock();
 let u=unsafe{&mut*UNIT.0.get()};
 if !u.enabled{return STATUS_DEVICE_NOT_READY}
 let domains=unsafe{&mut*DOMAINS.0.get()};
 let Some(index)=domains.iter().position(|d|d.id!=0&&d.bus==id.bus&&d.devfn==id.devfn() as u8) else{return STATUS_INVALID_PARAMETER};
 let d=domains[index];
 let contexts=unsafe{&mut*CONTEXTS.0.get()};
 let entry=unsafe{contexts[id.bus as usize].virtual_address.add(id.devfn()*2)};
 unsafe{core::ptr::write_volatile(entry,0);core::ptr::write_volatile(entry.add(1),0)};
 if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,contexts[id.bus as usize].physical,4096)}!=0{return STATUS_NOT_SUPPORTED}
 if unsafe{invalidate_all_locked(u)}!=STATUS_SUCCESS{return STATUS_IO_TIMEOUT}
 let status=unsafe{(u.ops.free_table.unwrap())(u.ops.context,d.root_physical,d.root_virtual)};
 if status!=0{return STATUS_NOT_SUPPORTED}
 domains[index]=EMPTY_DOMAIN;
 if !domains.iter().any(|x|x.id!=0&&x.bus==id.bus){
  let c=contexts[id.bus as usize];
  unsafe{core::ptr::write_volatile(u.root_virt.add(id.bus as usize),0)};
  if unsafe{(u.ops.clean_cache.unwrap())(u.ops.context,u.root_phys,4096)}!=0{return STATUS_NOT_SUPPORTED}
  if unsafe{invalidate_all_locked(u)}!=STATUS_SUCCESS{return STATUS_IO_TIMEOUT}
  if unsafe{(u.ops.free_table.unwrap())(u.ops.context,c.physical,c.virtual_address)}!=0{return STATUS_NOT_SUPPORTED}
  contexts[id.bus as usize]=EMPTY_CONTEXT;
 }
 STATUS_SUCCESS
}

/// Return the private second-level root for diagnostics and DMA mapper setup.
/// The returned physical address is not a DMA address and must never be exposed
/// to a device.
pub fn requester_domain_root(id:RequesterId)->Option<(u16,u64)>{
 let _guard=LOCK.lock();
 let domains=unsafe{&*DOMAINS.0.get()};
 domains.iter().find(|d|d.id!=0&&d.bus==id.bus&&d.devfn==id.devfn() as u8).map(|d|(d.id,d.root_physical))
}

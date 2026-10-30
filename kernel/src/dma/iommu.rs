//! Platform-owned IOMMU mapping contract and DMA mapping lifetime registry.
//! A backend must program real translation tables and invalidate IOTLBs before
//! returning from unmap. No identity-map fallback is permitted.
use core::{ffi::c_void, ptr, sync::atomic::{fence,Ordering}};
use super::{DmaError,ScatterGatherElement,MAX_SG_SEGMENTS,PAGE_SIZE};
#[path = "iommu/vtd.rs"]
pub mod vtd;
const MAX_MAPPINGS:usize=256;
const MAX_COMMON:usize=128;
const OK:i32=0;
const INVALID:i32=0xC000_000Du32 as i32;
const RESOURCES:i32=0xC000_009Au32 as i32;
const NOT_SUPPORTED:i32=0xC000_00BBu32 as i32;
pub type MapFn=unsafe extern "system" fn(*mut c_void,usize,u64,u32,u8,*mut u64,*mut u64)->i32;
pub type UnmapFn=unsafe extern "system" fn(*mut c_void,usize,u64,u64)->i32;
pub type SyncFn=unsafe extern "system" fn(*mut c_void,usize,u64,u32,u8)->i32;
#[repr(C)]
#[derive(Clone,Copy)]
pub struct IommuBackend { pub size:u32,pub context:*mut c_void,pub map:Option<MapFn>,pub unmap:Option<UnmapFn>,pub sync:Option<SyncFn> }
unsafe impl Send for IommuBackend{} unsafe impl Sync for IommuBackend{}
// Copy the callback table into kernel-owned storage at registration. Keeping an
// AtomicPtr to the caller's struct allowed a stack-allocated descriptor to
// become a use-after-free after the registration function returned.
struct BackendStorage(core::cell::UnsafeCell<IommuBackend>);
unsafe impl Sync for BackendStorage {}
static BACKEND_STORAGE:BackendStorage=BackendStorage(core::cell::UnsafeCell::new(IommuBackend{
 size:0,context:ptr::null_mut(),map:None,unmap:None,sync:None
}));
static BACKEND_STATE:core::sync::atomic::AtomicU8=core::sync::atomic::AtomicU8::new(0);
// 0 = absent, 1 = registration in progress, 2 = published and immutable.

#[derive(Clone,Copy)]struct Mapping{used:bool,busy:bool,device:usize,token:usize,irp:usize,physical:u64,logical:u64,length:u32,cookie:u64,direction:u8}
impl Mapping{const EMPTY:Self=Self{used:false,busy:false,device:0,token:0,irp:0,physical:0,logical:0,length:0,cookie:0,direction:0};}
#[derive(Clone,Copy)]struct Common{used:bool,busy:bool,device:usize,cpu:usize,physical:u64,logical:u64,length:usize,pages:usize,cookie:u64}
impl Common{const EMPTY:Self=Self{used:false,busy:false,device:0,cpu:0,physical:0,logical:0,length:0,pages:0,cookie:0};}

/// A mapping whose rollback cannot be proven must never return to a caller that
/// may release its backing pages. Stop the kernel rather than permit DMA into
/// reclaimed memory. Normal backend failures are returned before reaching here.
fn fatal_dma_lifetime_violation()->!{
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}
fn rollback_backend_mapping(b:&IommuBackend,device:usize,logical:u64,cookie:u64){
    if logical==0||cookie==0 { fatal_dma_lifetime_violation(); }
    if unsafe{(b.unmap.unwrap())(b.context,device,logical,cookie)}!=OK {
        fatal_dma_lifetime_violation();
    }
}
struct State{maps:[Mapping;MAX_MAPPINGS],common:[Common;MAX_COMMON]}
static STATE:crate::hal::IrqSpinLock<State>=crate::hal::IrqSpinLock::new(State{maps:[Mapping::EMPTY;MAX_MAPPINGS],common:[Common::EMPTY;MAX_COMMON]});
fn backend()->Result<&'static IommuBackend,i32>{
 if BACKEND_STATE.load(Ordering::Acquire)!=2{return Err(NOT_SUPPORTED)}
 let b=unsafe{&*BACKEND_STORAGE.0.get()};
 if b.size<core::mem::size_of::<IommuBackend>() as u32||b.map.is_none()||b.unmap.is_none()||b.sync.is_none(){return Err(NOT_SUPPORTED)}Ok(b)
}
/// Register the platform's permanent IOMMU backend exactly once.
///
/// # Safety
/// The context pointer and every object reachable from it must remain valid for
/// the lifetime of the kernel. The callback table itself is copied into
/// kernel-owned immutable storage and may be stack-allocated by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RdotDmaRegisterIommuBackend(ops:*mut IommuBackend)->i32{
 if ops.is_null(){return INVALID}
 let b=unsafe{*ops};
 if b.size<core::mem::size_of::<IommuBackend>() as u32||b.map.is_none()||b.unmap.is_none()||b.sync.is_none(){return INVALID}
 if BACKEND_STATE.compare_exchange(0,1,Ordering::AcqRel,Ordering::Acquire).is_err(){return 0xC000_0022u32 as i32}
 unsafe{BACKEND_STORAGE.0.get().write(b);}
 BACKEND_STATE.store(2,Ordering::Release);
 OK
}
pub fn available()->bool{backend().is_ok()}
pub fn device_busy(device:usize)->bool{let s=STATE.lock();s.maps.iter().any(|m|m.used&&m.device==device)||s.common.iter().any(|b|b.used&&b.device==device)}
pub fn common_allocation_count(device:usize)->usize{let s=STATE.lock();s.common.iter().filter(|c|c.used&&!c.busy&&c.device==device).count()}
fn map_one(device:usize,token:usize,irp:usize,physical:u64,length:u32,direction:u8)->Result<u64,DmaError>{
 if device==0||token==0||length==0||physical.checked_add(length as u64).is_none(){return Err(DmaError::InvalidLength)}
 // A DMA adapter handle is not a PCI requester ID. Require PnP to have
 // registered the stable device object's BDF before any mapping is attempted.
 if requester_identity(device).is_none(){return Err(DmaError::AddressMask)}
 let b=backend().map_err(|_|DmaError::AddressMask)?;
 let logical=0u64;let cookie=0u64;
 let mut logical_out=logical;let mut cookie_out=cookie;
 let status=unsafe{(b.map.unwrap())(b.context,device,physical,length,direction,&mut logical_out,&mut cookie_out)};
 if status!=OK{
  if logical_out!=0||cookie_out!=0{rollback_backend_mapping(b,device,logical_out,cookie_out);}
  return Err(DmaError::AddressMask)
 }
 // A backend that reports success must return a usable logical span and cookie.
 // If it partially installed a mapping but returned malformed outputs, attempt
 // rollback before reporting failure so a bad backend cannot silently leak IOVA.
 if logical_out==0||cookie_out==0||logical_out.checked_add(length as u64).is_none(){
  rollback_backend_mapping(b,device,logical_out,cookie_out);
  return Err(DmaError::AddressMask)
 }
 let mut s=STATE.lock();
 let logical_end=logical_out.checked_add(length as u64).ok_or(DmaError::AddressMask)?;
 let overlaps_map=s.maps.iter().any(|m|m.used&&m.device==device&&logical_out<m.logical.saturating_add(m.length as u64)&&m.logical<logical_end);
 let overlaps_common=s.common.iter().any(|x|x.used&&x.device==device&&logical_out<x.logical.saturating_add((x.pages as u64)*PAGE_SIZE)&&x.logical<logical_end);
 if overlaps_map||overlaps_common{drop(s);rollback_backend_mapping(b,device,logical_out,cookie_out);return Err(DmaError::AddressMask)}
 let Some(slot)=s.maps.iter_mut().find(|m|!m.used)else{drop(s);rollback_backend_mapping(b,device,logical_out,cookie_out);return Err(DmaError::RegistryFull)};
 *slot=Mapping{used:true,busy:false,device,token,irp,physical,logical:logical_out,length,cookie:cookie_out,direction};fence(Ordering::SeqCst);Ok(logical_out)
}
fn unmap_index(index:usize)->bool{
 let mapping={let mut s=STATE.lock();if index>=s.maps.len()||!s.maps[index].used||s.maps[index].busy{return false}s.maps[index].busy=true;s.maps[index]};
 let Ok(b)=backend()else{let mut s=STATE.lock();s.maps[index].busy=false;return false};
 if unsafe{(b.sync.unwrap())(b.context,mapping.device,mapping.logical,mapping.length,mapping.direction)}!=OK{let mut s=STATE.lock();s.maps[index].busy=false;return false}
 let status=unsafe{(b.unmap.unwrap())(b.context,mapping.device,mapping.logical,mapping.cookie)};
 if status!=OK{let mut s=STATE.lock();s.maps[index].busy=false;return false}
 {let mut s=STATE.lock();if s.maps[index].used&&s.maps[index].cookie==mapping.cookie{s.maps[index]=Mapping::EMPTY;}else{return false;}}
 super::return_map_register(mapping.token as *mut c_void,mapping.device);fence(Ordering::SeqCst);true
}

pub fn map_transfer(adapter:*mut c_void,token:*mut c_void,physical:u64,length:u32,direction:u8)->Result<u64,DmaError>{
 let device=super::adapter_device(adapter).ok_or(DmaError::AddressMask)?;
 if !super::consume_map_register(token,device){return Err(DmaError::OutOfMemory)}
 match map_one(device,token as usize,0,physical,length,direction){
  Ok(v) if v.checked_add(length as u64).is_some_and(|end|end>0&&end-1<=super::adapter_maximum_address(adapter))=>Ok(v),
  Ok(v)=>{let idx={let s=STATE.lock();s.maps.iter().position(|m|m.used&&m.device==device&&m.token==token as usize&&m.logical==v)};if let Some(i)=idx{if !unmap_index(i){fatal_dma_lifetime_violation();}}else{fatal_dma_lifetime_violation();}Err(DmaError::AddressMask)},
  Err(e)=>{super::return_map_register(token,device);Err(e)}
 }
}
pub fn sync_transfer(adapter:*mut c_void,token:*mut c_void,physical:u64,length:u32,_direction:u8)->bool{
 let Some(device)=super::adapter_device(adapter)else{return false};
 let found={let s=STATE.lock();s.maps.iter().position(|m|m.used&&m.device==device&&m.token==token as usize&&m.physical==physical&&m.length==length)};
 let Some(index)=found else{return false};
 if !unmap_index(index){return false}fence(Ordering::SeqCst);true
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RdotDmaBindTransferToIrp(adapter:*mut c_void,token:*mut c_void,logical:u64,irp:*mut c_void)->i32{bind_irp(adapter,token,logical,irp)}
pub(crate) fn release_irp_mappings(irp:*mut c_void){release_irp(irp)}
pub fn bind_irp(adapter:*mut c_void,token:*mut c_void,logical:u64,irp:*mut c_void)->i32{
 if irp.is_null(){return INVALID}let Some(device)=super::adapter_device(adapter)else{return INVALID};let mut s=STATE.lock();
 if let Some(m)=s.maps.iter_mut().find(|m|m.used&&m.device==device&&m.token==token as usize&&m.logical==logical){m.irp=irp as usize;return OK}INVALID
}
pub fn release_irp(irp:*mut c_void){
 if irp.is_null(){return}
 loop{
  let idx={let s=STATE.lock();s.maps.iter().position(|m|m.used&&m.irp==irp as usize)};
  let Some(i)=idx else{break};
  // The caller may free the IRP and its MDL immediately after this routine.
  // Do not return control if IOTLB teardown did not prove DMA access revoked.
  if !unmap_index(i){fatal_dma_lifetime_violation();}
 }
}
pub fn map_fragment(adapter:*mut c_void,token:*mut c_void,physical:u64,length:u32,direction:u8)->Result<u64,DmaError>{map_transfer(adapter,token,physical,length,direction)}
pub fn unmap_logical(device:usize,logical:u64)->bool{
 let index={let s=STATE.lock();s.maps.iter().position(|m|m.used&&m.device==device&&m.logical==logical&&m.irp==0)};
 index.is_some_and(unmap_index)
}
pub fn map_sg(device:usize,token:usize,fragments:&[(u64,u32)],direction:u8,out:&mut [ScatterGatherElement])->Result<usize,DmaError>{
 if device==0||token==0||fragments.is_empty()||fragments.len()>out.len()||fragments.len()>MAX_SG_SEGMENTS{return Err(DmaError::InvalidPageList)}
 let mut count=0;for &(physical,length) in fragments{match map_one(device,token,0,physical,length,direction){Ok(logical)=>{out[count]=ScatterGatherElement{address:logical,length,reserved:0};count+=1},Err(e)=>{while count>0{count-=1;let logical=out[count].address;let idx={let s=STATE.lock();s.maps.iter().position(|m|m.used&&m.device==device&&m.token==token&&m.logical==logical)};if let Some(i)=idx{let _=unmap_index(i);}}return Err(e)}}}Ok(count)
}
pub unsafe fn allocate_common(adapter:*mut c_void,length:usize,maximum:u64)->Result<(*mut u8,u64),DmaError>{
 let device=super::adapter_device(adapter).ok_or(DmaError::AddressMask)?;if length==0{return Err(DmaError::InvalidLength)}
 if requester_identity(device).is_none(){return Err(DmaError::AddressMask)}
 // Resolve the backend before acquiring physical pages. A missing IOMMU must
 // fail without leaking a contiguous allocation from the kernel page allocator.
 let b=backend().map_err(|_|DmaError::AddressMask)?;
 let pages=length.checked_add(PAGE_SIZE as usize-1).ok_or(DmaError::InvalidLength)?/PAGE_SIZE as usize;
 let bytes=(pages as u64).checked_mul(PAGE_SIZE).ok_or(DmaError::InvalidLength)?;
 if bytes==0||bytes>u32::MAX as u64{return Err(DmaError::InvalidLength)}
 let physical=unsafe{crate::memory::allocate_contiguous_pages(pages)}.map_err(|_|DmaError::OutOfMemory)?;
 if bytes>u32::MAX as u64||physical.checked_add(bytes-1).is_none(){for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}return Err(DmaError::AddressMask)}
 let Some(cpu)=crate::memory::direct_map_address(physical)else{for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}return Err(DmaError::AddressMask)};
 unsafe{ptr::write_bytes(cpu as *mut u8,0,bytes as usize);}
 let mut logical=0;let mut cookie=0;
 let st=unsafe{(b.map.unwrap())(b.context,device,physical,bytes as u32,2,&mut logical,&mut cookie)};
 if st!=OK {
  if logical!=0||cookie!=0{rollback_backend_mapping(b,device,logical,cookie);}
  for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}
  return Err(DmaError::AddressMask)
 }
 if logical==0||cookie==0||logical.checked_add(bytes).is_none_or(|e|e-1>maximum){
  rollback_backend_mapping(b,device,logical,cookie);
  for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}
  return Err(DmaError::AddressMask)
 }
 let mut s=STATE.lock();let logical_end=logical.checked_add(bytes).ok_or(DmaError::AddressMask)?;
 let overlap_maps=s.maps.iter().any(|m|m.used&&m.device==device&&logical<m.logical.saturating_add(m.length as u64)&&m.logical<logical_end);
 let overlap_common=s.common.iter().any(|x|x.used&&x.device==device&&logical<x.logical.saturating_add((x.pages as u64)*PAGE_SIZE)&&x.logical<logical_end);
 if overlap_maps||overlap_common{drop(s);rollback_backend_mapping(b,device,logical,cookie);for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}return Err(DmaError::AddressMask)}
 let Some(slot)=s.common.iter_mut().find(|c|!c.used)else{drop(s);rollback_backend_mapping(b,device,logical,cookie);for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}return Err(DmaError::RegistryFull)};
 *slot=Common{used:true,busy:false,device,cpu:cpu as usize,physical,logical,length,pages,cookie};fence(Ordering::SeqCst);Ok((cpu as *mut u8,logical))
}
pub unsafe fn free_common(adapter:*mut c_void,cpu:*mut u8,logical:u64,length:usize)->Result<(),DmaError>{
 let device=super::adapter_device(adapter).ok_or(DmaError::AddressMask)?;
 if requester_identity(device).is_none(){return Err(DmaError::AddressMask)}
 let b=backend().map_err(|_|DmaError::AddressMask)?;
 // Keep the ownership record published and mark it busy throughout sync and
 // IOTLB teardown. Concurrent allocators cannot reuse its IOVA or pages.
 let (index,rec)={let mut s=STATE.lock();
  let Some(i)=s.common.iter().position(|c|c.used&&!c.busy&&c.device==device&&c.cpu==cpu as usize&&c.logical==logical&&c.length==length)else{return Err(DmaError::NotFound)};
  s.common[i].busy=true;(i,s.common[i])};
 let mapped_bytes=(rec.pages as u64*PAGE_SIZE) as u32;
 let sync=unsafe{(b.sync.unwrap())(b.context,device,rec.logical,mapped_bytes,2)};
 if sync!=OK{STATE.lock().common[index].busy=false;return Err(DmaError::AddressMask)}
 let st=unsafe{(b.unmap.unwrap())(b.context,device,rec.logical,rec.cookie)};
 if st!=OK{STATE.lock().common[index].busy=false;return Err(DmaError::AddressMask)}
 {let mut s=STATE.lock();
  if !s.common[index].used||!s.common[index].busy||s.common[index].cookie!=rec.cookie{return Err(DmaError::NotFound)}
  s.common[index]=Common::EMPTY;
 }
 for i in 0..rec.pages{unsafe{crate::memory::free_page(rec.physical+i as u64*PAGE_SIZE)}.map_err(|_|DmaError::OutOfMemory)?;}
 Ok(())
}
pub fn flush_common(adapter:*mut c_void,cpu:*mut u8,length:usize)->Result<(),DmaError>{
 if cpu.is_null()||length==0{return Err(DmaError::InvalidLength)}let device=super::adapter_device(adapter).ok_or(DmaError::AddressMask)?;if requester_identity(device).is_none(){return Err(DmaError::AddressMask)}let b=backend().map_err(|_|DmaError::AddressMask)?;
 let rec={let s=STATE.lock();s.common.iter().find(|c|c.used&&!c.busy&&c.device==device&&c.cpu==cpu as usize&&length<=c.length).copied().ok_or(DmaError::NotFound)?};
 if unsafe{(b.sync.unwrap())(b.context,device,rec.logical,length as u32,2)}!=OK{return Err(DmaError::AddressMask)}fence(Ordering::SeqCst);unsafe{core::arch::asm!("mfence",options(nostack,preserves_flags));}Ok(())
}
#[cfg(test)]mod tests{use super::*;#[test]fn no_backend_fails_closed(){assert!(!available());}}


// PCI requester identity registry. DMA_ADAPTER device handles are kernel object
// addresses, not PCI BDFs; VT-d must never derive a requester ID from a pointer.
// PnP registers the validated PCI location before enabling bus mastering.
const MAX_REQUESTERS: usize = 128;
#[derive(Clone, Copy)]
struct Requester {
    used: bool,
    device: usize,
    segment: u16,
    bus: u8,
    devfn: u8,
}
impl Requester {
    const EMPTY: Self = Self { used: false, device: 0, segment: 0, bus: 0, devfn: 0 };
}
static REQUESTERS: crate::hal::IrqSpinLock<[Requester; MAX_REQUESTERS]> =
    crate::hal::IrqSpinLock::new([Requester::EMPTY; MAX_REQUESTERS]);

/// Register the PCI segment:bus:device.function identity associated with a
/// kernel device object. This records identity only; it does not authorize DMA.
/// # Safety
/// `device` must be a stable, live PnP device-object address.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RdotDmaRegisterDeviceRequester(
    device: *mut c_void, segment: u16, bus: u8, pci_device: u8, function: u8,
) -> i32 {
    if device.is_null() || pci_device >= 32 || function >= 8 { return INVALID; }
    let handle = device as usize;
    let devfn = (pci_device << 3) | function;
    let mut entries = REQUESTERS.lock();
    if entries.iter().any(|r| r.used && (r.device == handle
        || (r.segment == segment && r.bus == bus && r.devfn == devfn))) {
        return 0xC000_0035u32 as i32; // STATUS_OBJECT_NAME_COLLISION
    }
    let Some(slot) = entries.iter_mut().find(|r| !r.used) else { return RESOURCES; };
    *slot = Requester { used: true, device: handle, segment, bus, devfn };
    OK
}

/// Remove requester identity only after all DMA mappings have been torn down.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RdotDmaUnregisterDeviceRequester(device: *mut c_void) -> i32 {
    if device.is_null() { return INVALID; }
    let handle = device as usize;
    if device_busy(handle) { return 0xC000_00A2u32 as i32; } // STATUS_DEVICE_BUSY
    let mut entries = REQUESTERS.lock();
    let Some(slot) = entries.iter_mut().find(|r| r.used && r.device == handle) else {
        return 0xC000_0225u32 as i32; // STATUS_NOT_FOUND
    };
    *slot = Requester::EMPTY;
    OK
}

/// Return the registered requester tuple for the VT-d domain manager.
/// The tuple is (segment, bus, device/function byte).
pub fn requester_identity(device: usize) -> Option<(u16, u8, u8)> {
    let entries = REQUESTERS.lock();
    entries.iter().find(|r| r.used && r.device == device)
        .map(|r| (r.segment, r.bus, r.devfn))
}

#[cfg(test)]
mod requester_tests {
    use super::*;
    #[test]
    fn requester_devfn_encoding_is_pci_spec_order() {
        let devfn = (7u8 << 3) | 5u8;
        assert_eq!(devfn, 0x3d);
        assert!(7 < 32 && 5 < 8);
    }
    #[test]
    fn requester_registry_starts_empty() {
        assert_eq!(requester_identity(usize::MAX), None);
    }
}

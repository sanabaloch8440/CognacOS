//! Platform-owned IOMMU mapping contract and DMA mapping lifetime registry.
//! A backend must program real translation tables and invalidate IOTLBs before
//! returning from unmap. No identity-map fallback is permitted.
use core::{ffi::c_void, ptr, sync::atomic::{fence,Ordering}};
use super::{DmaError,ScatterGatherElement,MAX_SG_SEGMENTS,PAGE_SIZE};
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
pub struct IommuBackend { pub size:u32,pub context:*mut c_void,pub map:Option<MapFn>,pub unmap:Option<UnmapFn>,pub sync:Option<SyncFn> }
unsafe impl Send for IommuBackend{} unsafe impl Sync for IommuBackend{}
static BACKEND:core::sync::atomic::AtomicPtr<IommuBackend>=core::sync::atomic::AtomicPtr::new(ptr::null_mut());
#[derive(Clone,Copy)]struct Mapping{used:bool,device:usize,token:usize,irp:usize,physical:u64,logical:u64,length:u32,cookie:u64,direction:u8}
impl Mapping{const EMPTY:Self=Self{used:false,device:0,token:0,irp:0,physical:0,logical:0,length:0,cookie:0,direction:0};}
#[derive(Clone,Copy)]struct Common{used:bool,device:usize,cpu:usize,physical:u64,logical:u64,length:usize,pages:usize,cookie:u64}
impl Common{const EMPTY:Self=Self{used:false,device:0,cpu:0,physical:0,logical:0,length:0,pages:0,cookie:0};}
struct State{maps:[Mapping;MAX_MAPPINGS],common:[Common;MAX_COMMON]}
static STATE:crate::hal::IrqSpinLock<State>=crate::hal::IrqSpinLock::new(State{maps:[Mapping::EMPTY;MAX_MAPPINGS],common:[Common::EMPTY;MAX_COMMON]});
fn backend()->Result<&'static IommuBackend,i32>{let p=BACKEND.load(Ordering::Acquire);if p.is_null(){return Err(NOT_SUPPORTED)}let b=unsafe{&*p};if b.size<core::mem::size_of::<IommuBackend>() as u32||b.map.is_none()||b.unmap.is_none()||b.sync.is_none(){return Err(NOT_SUPPORTED)}Ok(b)}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RdotDmaRegisterIommuBackend(ops:*mut IommuBackend)->i32{
 if ops.is_null(){return INVALID}let b=unsafe{&*ops};if b.size<core::mem::size_of::<IommuBackend>() as u32||b.map.is_none()||b.unmap.is_none()||b.sync.is_none(){return INVALID}
 if BACKEND.compare_exchange(ptr::null_mut(),ops,Ordering::AcqRel,Ordering::Acquire).is_err(){return 0xC000_0022u32 as i32}OK
}
pub fn available()->bool{backend().is_ok()}
pub fn device_busy(device:usize)->bool{let s=STATE.lock();s.maps.iter().any(|m|m.used&&m.device==device)||s.common.iter().any(|b|b.used&&b.device==device)}
fn map_one(device:usize,token:usize,irp:usize,physical:u64,length:u32,direction:u8)->Result<u64,DmaError>{
 if device==0||token==0||length==0||physical.checked_add(length as u64).is_none(){return Err(DmaError::InvalidLength)}
 let b=backend().map_err(|_|DmaError::AddressMask)?;
 let logical=0u64;let cookie=0u64;
 let mut logical_out=logical;let mut cookie_out=cookie;
 let status=unsafe{(b.map.unwrap())(b.context,device,physical,length,direction,&mut logical_out,&mut cookie_out)};
 if status!=OK||logical_out==0||cookie_out==0||logical_out.checked_add(length as u64).is_none(){return Err(DmaError::AddressMask)}
 let mut s=STATE.lock();let Some(slot)=s.maps.iter_mut().find(|m|!m.used)else{drop(s);unsafe{(b.unmap.unwrap())(b.context,device,logical_out,cookie_out)};return Err(DmaError::RegistryFull)};
 *slot=Mapping{used:true,device,token,irp,physical,logical:logical_out,length,cookie:cookie_out,direction};fence(Ordering::SeqCst);Ok(logical_out)
}
fn unmap_index(index:usize)->bool{
 let mapping={let mut s=STATE.lock();if index>=s.maps.len()||!s.maps[index].used{return false}let m=s.maps[index];s.maps[index]=Mapping::EMPTY;m};
 let Ok(b)=backend()else{return false};
 if unsafe{(b.sync.unwrap())(b.context,mapping.device,mapping.logical,mapping.length,mapping.direction)}!=OK{let mut s=STATE.lock();if let Some(slot)=s.maps.iter_mut().find(|x|!x.used){*slot=mapping;}return false}
 let status=unsafe{(b.unmap.unwrap())(b.context,mapping.device,mapping.logical,mapping.cookie)};
 if status!=OK{let mut s=STATE.lock();if let Some(slot)=s.maps.iter_mut().find(|x|!x.used){*slot=mapping;}return false}
 super::return_map_register(mapping.token as *mut c_void,mapping.device);fence(Ordering::SeqCst);true
}
pub fn map_transfer(adapter:*mut c_void,token:*mut c_void,physical:u64,length:u32,direction:u8)->Result<u64,DmaError>{
 let device=super::adapter_device(adapter).ok_or(DmaError::AddressMask)?;
 if !super::consume_map_register(token,device){return Err(DmaError::OutOfMemory)}
 match map_one(device,token as usize,0,physical,length,direction){Ok(v)=>Ok(v),Err(e)=>{super::return_map_register(token,device);Err(e)}}
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
 if irp.is_null(){return}loop{let idx={let s=STATE.lock();s.maps.iter().position(|m|m.used&&m.irp==irp as usize)};let Some(i)=idx else{break};if !unmap_index(i){break}}
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
 let pages=length.checked_add(PAGE_SIZE as usize-1).ok_or(DmaError::InvalidLength)?/PAGE_SIZE as usize;
 let physical=unsafe{crate::memory::allocate_contiguous_pages(pages)}.map_err(|_|DmaError::OutOfMemory)?;
 let bytes=(pages as u64).checked_mul(PAGE_SIZE).ok_or(DmaError::InvalidLength)?;
 if bytes>u32::MAX as u64||physical.checked_add(bytes-1).is_none_or(|end|end>maximum){for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}return Err(DmaError::AddressMask)}
 let Some(cpu)=crate::memory::direct_map_address(physical)else{for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}return Err(DmaError::AddressMask)};
 unsafe{ptr::write_bytes(cpu as *mut u8,0,bytes as usize);}
 let b=backend().map_err(|_|DmaError::AddressMask)?;let mut logical=0;let mut cookie=0;
 let st=unsafe{(b.map.unwrap())(b.context,device,physical,bytes as u32,2,&mut logical,&mut cookie)};
 if st!=OK||logical==0||cookie==0||logical.checked_add(bytes).is_none_or(|e|e-1>maximum){if st==OK&&cookie!=0{let _=unsafe{(b.unmap.unwrap())(b.context,device,logical,cookie)};}for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}return Err(DmaError::AddressMask)}
 let mut s=STATE.lock();let Some(slot)=s.common.iter_mut().find(|c|!c.used)else{drop(s);let _=unsafe{(b.unmap.unwrap())(b.context,device,logical,cookie)};for i in 0..pages{let _=unsafe{crate::memory::free_page(physical+i as u64*PAGE_SIZE)};}return Err(DmaError::RegistryFull)};
 *slot=Common{used:true,device,cpu:cpu as usize,physical,logical,length,pages,cookie};fence(Ordering::SeqCst);Ok((cpu as *mut u8,logical))
}
pub unsafe fn free_common(adapter:*mut c_void,cpu:*mut u8,logical:u64,length:usize)->Result<(),DmaError>{
 let device=super::adapter_device(adapter).ok_or(DmaError::AddressMask)?;
 let rec={let mut s=STATE.lock();let Some(i)=s.common.iter().position(|c|c.used&&c.device==device&&c.cpu==cpu as usize&&c.logical==logical&&c.length==length)else{return Err(DmaError::NotFound)};let c=s.common[i];s.common[i]=Common::EMPTY;c};
 let b=backend().map_err(|_|DmaError::AddressMask)?;let st=unsafe{(b.unmap.unwrap())(b.context,device,rec.logical,rec.cookie)};if st!=OK{let mut s=STATE.lock();if let Some(slot)=s.common.iter_mut().find(|c|!c.used){*slot=rec;}return Err(DmaError::AddressMask)}
 for i in 0..rec.pages{unsafe{crate::memory::free_page(rec.physical+i as u64*PAGE_SIZE)}.map_err(|_|DmaError::OutOfMemory)?;}Ok(())
}
pub fn flush_common(adapter:*mut c_void,cpu:*mut u8,length:usize)->Result<(),DmaError>{
 if cpu.is_null()||length==0{return Err(DmaError::InvalidLength)}let device=super::adapter_device(adapter).ok_or(DmaError::AddressMask)?;let b=backend().map_err(|_|DmaError::AddressMask)?;
 let rec={let s=STATE.lock();s.common.iter().find(|c|c.used&&c.device==device&&c.cpu==cpu as usize&&length<=c.length).copied().ok_or(DmaError::NotFound)?};
 if unsafe{(b.sync.unwrap())(b.context,device,rec.logical,length as u32,2)}!=OK{return Err(DmaError::AddressMask)}fence(Ordering::SeqCst);unsafe{core::arch::asm!("mfence",options(nostack,preserves_flags));}Ok(())
}
#[cfg(test)]mod tests{use super::*;#[test]fn no_backend_fails_closed(){assert!(!available());}}

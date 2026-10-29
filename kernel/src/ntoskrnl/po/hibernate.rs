//! Crash-consistent S4 hibernation image engine.
//! Requires a registered platform backend for storage, physical memory, CPU state,
//! device-stack quiescence, and the final S4 transition. Missing services fail closed.
use core::{ffi::c_void, ptr, cell::UnsafeCell, sync::atomic::{AtomicU8, Ordering}};
const MAGIC:&[u8;8]=b"RDOTHIB1";
const VERSION:u32=1;
const HEADER:usize=128;
const DESC:usize=40;
const MAX_REGIONS:usize=128;
const TABLE:u64=HEADER as u64;
const DATA:u64=(HEADER+DESC*MAX_REGIONS) as u64;
const CHUNK:usize=4096;
const SCRATCH:usize=8192;
const CPU_MAX:usize=4096;
const COMMIT:u64=0x5244_4F54_4849_4231;
const MAX_IMAGE:u64=1<<40;
const OK:i32=0;
const INVALID:i32=0xC000_000Du32 as i32;
const NOT_IMPLEMENTED:i32=0xC000_0002u32 as i32;
const NO_MEMORY:i32=0xC000_009Au32 as i32;
const DATA_ERROR:i32=0xC000_003Eu32 as i32;
const NOT_READY:i32=0xC000_00A3u32 as i32;
const UNSUCCESSFUL:i32=0xC000_0001u32 as i32;

#[repr(C)] #[derive(Clone,Copy)]
pub struct HiberMemoryRegion { pub physical_start:u64, pub source:*const u8, pub length:u64 }
unsafe impl Send for HiberMemoryRegion {}
pub type Simple=unsafe extern "system" fn(*mut c_void)->i32;
pub type Enumerate=unsafe extern "system" fn(*mut c_void,*mut HiberMemoryRegion,usize,*mut usize)->i32;
pub type CaptureCpu=unsafe extern "system" fn(*mut c_void,*mut u8,usize,*mut usize)->i32;
pub type RestoreCpu=unsafe extern "system" fn(*mut c_void,*const u8,usize)->i32;
pub type Write=unsafe extern "system" fn(*mut c_void,u32,u64,*const u8,usize)->i32;
pub type Read=unsafe extern "system" fn(*mut c_void,u32,u64,*mut u8,usize)->i32;
pub type Flush=unsafe extern "system" fn(*mut c_void,u32)->i32;
pub type RestoreMemory=unsafe extern "system" fn(*mut c_void,u64,*const u8,usize)->i32;

/// Callbacks must quiesce/drain every device stack, expose a stable memory map,
/// and restore memory through a resume-safe loader that will not overwrite its
/// active stack/code. restore_cpu_state must perform the final context transfer.
#[repr(C)]
#[derive(Clone,Copy)]
pub struct HiberPlatformOps {
 pub size:u32, pub context:*mut c_void,
 pub quiesce_devices:Option<Simple>, pub resume_devices:Option<Simple>,
 pub enumerate_memory:Option<Enumerate>, pub capture_cpu_state:Option<CaptureCpu>,
 pub restore_cpu_state:Option<RestoreCpu>, pub write_slot:Option<Write>,
 pub read_slot:Option<Read>, pub flush_slot:Option<Flush>,
 pub restore_memory:Option<RestoreMemory>, pub enter_s4:Option<Simple>,
}
unsafe impl Send for HiberPlatformOps {}
unsafe impl Sync for HiberPlatformOps {}
// The callback descriptor is copied to kernel-owned immutable storage. Its
// context and resources remain platform-owned and must outlive the kernel.
struct PlatformStorage(UnsafeCell<HiberPlatformOps>);
unsafe impl Sync for PlatformStorage {}
static PLATFORM_STORAGE:PlatformStorage=PlatformStorage(UnsafeCell::new(HiberPlatformOps{
 size:0,context:ptr::null_mut(),quiesce_devices:None,resume_devices:None,
 enumerate_memory:None,capture_cpu_state:None,restore_cpu_state:None,
 write_slot:None,read_slot:None,flush_slot:None,restore_memory:None,enter_s4:None
}));
static PLATFORM_STATE:AtomicU8=AtomicU8::new(0); // absent, registering, published

#[derive(Clone,Copy,Default)]
struct RegionDesc { physical:u64, length:u64, offset:u64, stored:u64, checksum:u32 }
impl RegionDesc {
 fn encode(self,b:&mut [u8]) { p64(b,0,self.physical);p64(b,8,self.length);p64(b,16,self.offset);p64(b,24,self.stored);p32(b,32,self.checksum);p32(b,36,0); }
 fn decode(b:&[u8])->Self { Self{physical:g64(b,0),length:g64(b,8),offset:g64(b,16),stored:g64(b,24),checksum:g32(b,32)} }
}
#[derive(Clone,Copy,Default)]
struct Header { generation:u64, count:u32, cpu_len:u32, cpu_offset:u64, total:u64, table_crc:u32, cpu_crc:u32 }
impl Header {
 fn encode(self,commit:bool)->[u8;HEADER] {
  let mut b=[0u8;HEADER];b[..8].copy_from_slice(MAGIC);p32(&mut b,8,VERSION);p32(&mut b,12,HEADER as u32);
  p64(&mut b,16,self.generation);p32(&mut b,24,self.count);p32(&mut b,28,self.cpu_len);p64(&mut b,32,self.cpu_offset);
  p64(&mut b,40,self.total);p32(&mut b,48,self.table_crc);p32(&mut b,52,self.cpu_crc);p64(&mut b,64,if commit{COMMIT}else{0});
  let c=crc(&b);p32(&mut b,56,c);b
 }
 fn decode(b:&[u8;HEADER])->Result<Self,i32> {
  if &b[..8]!=MAGIC||g32(b,8)!=VERSION||g32(b,12)!=HEADER as u32||g64(b,64)!=COMMIT{return Err(DATA_ERROR)}
  let want=g32(b,56);let mut c=*b;p32(&mut c,56,0);if crc(&c)!=want{return Err(DATA_ERROR)}
  let h=Self{generation:g64(b,16),count:g32(b,24),cpu_len:g32(b,28),cpu_offset:g64(b,32),total:g64(b,40),table_crc:g32(b,48),cpu_crc:g32(b,52)};
  if h.count==0||h.count as usize>MAX_REGIONS||h.cpu_len as usize>CPU_MAX||h.total>MAX_IMAGE||h.total<DATA||h.cpu_offset<DATA||h.cpu_offset.checked_add(h.cpu_len as u64).is_none_or(|x|x>h.total){return Err(DATA_ERROR)}
  Ok(h)
 }
}
fn p32(b:&mut [u8],o:usize,v:u32){b[o..o+4].copy_from_slice(&v.to_le_bytes())}
fn p64(b:&mut [u8],o:usize,v:u64){b[o..o+8].copy_from_slice(&v.to_le_bytes())}
fn g32(b:&[u8],o:usize)->u32{u32::from_le_bytes(b[o..o+4].try_into().unwrap_or([0;4]))}
fn g64(b:&[u8],o:usize)->u64{u64::from_le_bytes(b[o..o+8].try_into().unwrap_or([0;8]))}
fn crc_update(c:&mut u32,b:&[u8]){for x in b{*c^=*x as u32;for _ in 0..8{*c=(*c>>1)^(0xEDB8_8320u32&(0u32.wrapping_sub(*c&1)));}}}
fn crc(b:&[u8])->u32{let mut c=!0u32;crc_update(&mut c,b);!c}
fn rle_encode(input:&[u8],out:&mut [u8])->usize{let(mut i,mut o)=(0,0);while i<input.len(){let v=input[i];let mut n=1;while i+n<input.len()&&input[i+n]==v&&n<255{n+=1}if o+2>out.len(){return usize::MAX}out[o]=n as u8;out[o+1]=v;o+=2;i+=n;}o}
fn rle_decode(input:&[u8],out:&mut [u8],expected:usize)->Result<(),i32>{if input.len()%2!=0||expected>out.len(){return Err(DATA_ERROR)}let mut n=0usize;for p in input.chunks_exact(2){let k=p[0] as usize;if k==0||n.checked_add(k).is_none_or(|x|x>expected){return Err(DATA_ERROR)}out[n..n+k].fill(p[1]);n+=k;}if n!=expected{return Err(DATA_ERROR)}Ok(())}
fn platform()->Result<&'static HiberPlatformOps,i32>{
 if PLATFORM_STATE.load(Ordering::Acquire)!=2{return Err(NOT_IMPLEMENTED)}
 let o=unsafe{&*PLATFORM_STORAGE.0.get()};
 if o.size<core::mem::size_of::<HiberPlatformOps>() as u32||o.quiesce_devices.is_none()||o.resume_devices.is_none()||o.enumerate_memory.is_none()||o.capture_cpu_state.is_none()||o.restore_cpu_state.is_none()||o.write_slot.is_none()||o.read_slot.is_none()||o.flush_slot.is_none()||o.restore_memory.is_none()||o.enter_s4.is_none(){return Err(NOT_IMPLEMENTED)}Ok(o)
}
/// Register the permanent platform backend. The descriptor is copied, but its
/// context and referenced storage must remain valid for the kernel lifetime.
///
/// # Safety
/// Every callback must obey its documented ABI and memory-safety contract.
pub unsafe fn register_platform(p:*mut HiberPlatformOps)->i32{
 if p.is_null(){return INVALID}
 let o=unsafe{*p};
 if o.size<core::mem::size_of::<HiberPlatformOps>() as u32||o.quiesce_devices.is_none()||o.resume_devices.is_none()||o.enumerate_memory.is_none()||o.capture_cpu_state.is_none()||o.restore_cpu_state.is_none()||o.write_slot.is_none()||o.read_slot.is_none()||o.flush_slot.is_none()||o.restore_memory.is_none()||o.enter_s4.is_none(){return INVALID}
 if PLATFORM_STATE.compare_exchange(0,1,Ordering::AcqRel,Ordering::Acquire).is_err(){return 0xC000_0022u32 as i32}
 unsafe{PLATFORM_STORAGE.0.get().write(o);}
 PLATFORM_STATE.store(2,Ordering::Release);OK
}
fn rd(o:&HiberPlatformOps,s:u32,at:u64,b:&mut [u8])->i32{unsafe{(o.read_slot.unwrap())(o.context,s,at,b.as_mut_ptr(),b.len())}}
fn wr(o:&HiberPlatformOps,s:u32,at:u64,b:&[u8])->i32{unsafe{(o.write_slot.unwrap())(o.context,s,at,b.as_ptr(),b.len())}}
fn fl(o:&HiberPlatformOps,s:u32)->i32{unsafe{(o.flush_slot.unwrap())(o.context,s)}}
fn read_header(o:&HiberPlatformOps,s:u32)->Result<Header,i32>{let mut b=[0;HEADER];let x=rd(o,s,0,&mut b);if x!=OK{return Err(x)}Header::decode(&b)}
fn read_table(o:&HiberPlatformOps,s:u32,h:Header)->Result<[RegionDesc;MAX_REGIONS],i32>{
 let mut ds=[RegionDesc::default();MAX_REGIONS];let mut b=[0u8;DESC*MAX_REGIONS];let n=h.count as usize*DESC;
 let x=rd(o,s,TABLE,&mut b[..n]);if x!=OK{return Err(x)}if crc(&b[..n])!=h.table_crc{return Err(DATA_ERROR)}
 for i in 0..h.count as usize{ds[i]=RegionDesc::decode(&b[i*DESC..(i+1)*DESC]);let d=ds[i];if d.length==0||d.physical.checked_add(d.length).is_none()||d.offset<DATA||d.offset.checked_add(d.stored).is_none_or(|x|x>h.cpu_offset)||d.stored<12{return Err(DATA_ERROR)}for prev in ds.iter().take(i){if d.physical<prev.physical.saturating_add(prev.length)&&prev.physical<d.physical.saturating_add(d.length){return Err(DATA_ERROR)}}}
 Ok(ds)
}
fn process_region(o:&HiberPlatformOps,s:u32,d:RegionDesc,restore:bool,input:&mut [u8;SCRATCH],output:&mut [u8;CHUNK])->Result<(),i32>{
 let(mut at, end, mut raw, mut c)=(d.offset,d.offset.checked_add(d.stored).ok_or(DATA_ERROR)?,0u64,!0u32);
 while at<end{let mut bh=[0u8;12];if end-at<12||rd(o,s,at,&mut bh)!=OK{return Err(DATA_ERROR)}at+=12;let rn=g32(&bh,0) as usize;let sn=g32(&bh,4) as usize;let method=g32(&bh,8);if rn==0||rn>CHUNK||sn==0||sn>SCRATCH||at.checked_add(sn as u64).is_none_or(|x|x>end){return Err(DATA_ERROR)}if rd(o,s,at,&mut input[..sn])!=OK{return Err(DATA_ERROR)}at+=sn as u64;match method{0 if sn==rn=>output[..rn].copy_from_slice(&input[..sn]),1=>rle_decode(&input[..sn],output,rn)?,_=>return Err(DATA_ERROR)}crc_update(&mut c,&output[..rn]);if restore{let phys=d.physical.checked_add(raw).ok_or(DATA_ERROR)?;let x=unsafe{(o.restore_memory.unwrap())(o.context,phys,output.as_ptr(),rn)};if x!=OK{return Err(x)}}raw=raw.checked_add(rn as u64).ok_or(DATA_ERROR)?;if raw>d.length{return Err(DATA_ERROR)}}
 if at!=end||raw!=d.length||!c!=d.checksum{return Err(DATA_ERROR)}Ok(())
}
fn validate_slot(o:&HiberPlatformOps,s:u32)->Result<Header,i32>{
 let h=read_header(o,s)?;let ds=read_table(o,s,h)?;let mut input=[0u8;SCRATCH];let mut output=[0u8;CHUNK];
 for d in ds.iter().take(h.count as usize){process_region(o,s,*d,false,&mut input,&mut output)?}
 let mut cpu=[0u8;CPU_MAX];if h.cpu_len>0{if rd(o,s,h.cpu_offset,&mut cpu[..h.cpu_len as usize])!=OK||crc(&cpu[..h.cpu_len as usize])!=h.cpu_crc{return Err(DATA_ERROR)}}else if h.cpu_crc!=crc(&[]){return Err(DATA_ERROR)}Ok(h)
}
fn write_image(o:&HiberPlatformOps,s:u32,generation:u64)->Result<(),i32>{
 let zero=[0u8;HEADER];if wr(o,s,0,&zero)!=OK||fl(o,s)!=OK{return Err(UNSUCCESSFUL)}
 let mut regions=[HiberMemoryRegion{physical_start:0,source:ptr::null(),length:0};MAX_REGIONS];let mut count=0usize;
 let x=unsafe{(o.enumerate_memory.unwrap())(o.context,regions.as_mut_ptr(),MAX_REGIONS,&mut count)};if x!=OK{return Err(x)}if count==0||count>MAX_REGIONS{return Err(NO_MEMORY)}
 for i in 0..count{let r=regions[i];if r.source.is_null()||r.length==0||r.physical_start.checked_add(r.length).is_none()||r.length>usize::MAX as u64{return Err(INVALID)}for p in regions.iter().take(i){if r.physical_start<p.physical_start.saturating_add(p.length)&&p.physical_start<r.physical_start.saturating_add(r.length){return Err(INVALID)}}}
 let mut cpu=[0u8;CPU_MAX];let mut cpu_len=0usize;let x=unsafe{(o.capture_cpu_state.unwrap())(o.context,cpu.as_mut_ptr(),cpu.len(),&mut cpu_len)};if x!=OK{return Err(x)}if cpu_len>CPU_MAX{return Err(NO_MEMORY)}
 let mut ds=[RegionDesc::default();MAX_REGIONS];let mut input=[0u8;SCRATCH];let mut enc=[0u8;SCRATCH];let mut at=DATA;
 for i in 0..count{let r=regions[i];let start=at;let(mut consumed,mut c)=(0u64,!0u32);while consumed<r.length{let n=core::cmp::min(CHUNK as u64,r.length-consumed) as usize;let bytes=unsafe{core::slice::from_raw_parts(r.source.add(consumed as usize),n)};crc_update(&mut c,bytes);let z=rle_encode(bytes,&mut enc);let(method,payload,plen)=if z<n{(1u32,&enc[..z],z)}else{(0u32,bytes,n)};if at.checked_add(12+plen as u64).is_none_or(|v|v>MAX_IMAGE){return Err(NO_MEMORY)}let mut bh=[0u8;12];p32(&mut bh,0,n as u32);p32(&mut bh,4,plen as u32);p32(&mut bh,8,method);if wr(o,s,at,&bh)!=OK{return Err(UNSUCCESSFUL)}at+=12;if wr(o,s,at,payload)!=OK{return Err(UNSUCCESSFUL)}at+=plen as u64;consumed+=n as u64;}ds[i]=RegionDesc{physical:r.physical_start,length:r.length,offset:start,stored:at-start,checksum:!c};}
 let cpu_offset=at;if at.checked_add(cpu_len as u64).is_none_or(|v|v>MAX_IMAGE){return Err(NO_MEMORY)}if cpu_len>0{if wr(o,s,at,&cpu[..cpu_len])!=OK{return Err(UNSUCCESSFUL)}at+=cpu_len as u64}
 let mut table=[0u8;DESC*MAX_REGIONS];for i in 0..count{ds[i].encode(&mut table[i*DESC..(i+1)*DESC])}let table_len=count*DESC;
 if wr(o,s,TABLE,&table[..table_len])!=OK||fl(o,s)!=OK{return Err(UNSUCCESSFUL)}
 let h=Header{generation,count:count as u32,cpu_len:cpu_len as u32,cpu_offset,total:at,table_crc:crc(&table[..table_len]),cpu_crc:crc(&cpu[..cpu_len])};
 if wr(o,s,0,&h.encode(true))!=OK||fl(o,s)!=OK{return Err(UNSUCCESSFUL)}
 validate_slot(o,s)?;Ok(())
}

/// Quiesce stacks, persist a checksummed compressed image to the inactive slot,
/// commit its header last, then ask the platform to enter S4.
pub fn initiate_s4()->i32{
 let o=match platform(){Ok(v)=>v,Err(e)=>return e};let x=unsafe{(o.quiesce_devices.unwrap())(o.context)};if x!=OK{return x}
 let a=validate_slot(o,0);let b=validate_slot(o,1);
 let(generation,target)=match(a,b){(Ok(x),Ok(y))if x.generation>=y.generation=>(x.generation.saturating_add(1),1),(Ok(x),Ok(y))=>(y.generation.saturating_add(1),0),(Ok(x),Err(_))=>(x.generation.saturating_add(1),1),(Err(_),Ok(y))=>(y.generation.saturating_add(1),0),_=>(1,0)};
 if let Err(e)=write_image(o,target,generation){unsafe{(o.resume_devices.unwrap())(o.context)};return e}
 let x=unsafe{(o.enter_s4.unwrap())(o.context)};unsafe{(o.resume_devices.unwrap())(o.context)};x
}
/// Validate both slots completely before modifying memory; restore the newest
/// valid generation. A bootloader must call this only from its resume-safe path.
pub fn resume_from_hibernation()->i32{
 let o=match platform(){Ok(v)=>v,Err(e)=>return e};let a=validate_slot(o,0);let b=validate_slot(o,1);
 let(s,h)=match(a,b){(Ok(x),Ok(y))if x.generation>=y.generation=>(0,x),(Ok(_),Ok(y))=>(1,y),(Ok(x),Err(_))=>(0,x),(Err(_),Ok(y))=>(1,y),_=>(return NOT_READY)};
 let ds=match read_table(o,s,h){Ok(v)=>v,Err(e)=>return e};let mut input=[0u8;SCRATCH];let mut output=[0u8;CHUNK];
 for d in ds.iter().take(h.count as usize){if let Err(e)=process_region(o,s,*d,true,&mut input,&mut output){return e}}
 let mut cpu=[0u8;CPU_MAX];if h.cpu_len>0{let x=rd(o,s,h.cpu_offset,&mut cpu[..h.cpu_len as usize]);if x!=OK{return x}}
 unsafe{(o.restore_cpu_state.unwrap())(o.context,cpu.as_ptr(),h.cpu_len as usize)}
}
pub fn validate_hibernation_images()->i32{let o=match platform(){Ok(v)=>v,Err(e)=>return e};if validate_slot(o,0).is_ok()||validate_slot(o,1).is_ok(){OK}else{NOT_READY}}

#[cfg(test)]
mod tests{
 use super::*;
 use core::{cell::UnsafeCell,sync::atomic::{AtomicU8,Ordering}};
 #[test]fn rle_roundtrip_and_rejects_corruption(){let input=[0x5au8;1024];let mut enc=[0u8;2048];let n=rle_encode(&input,&mut enc);assert!(n<input.len());let mut out=[0u8;1024];assert_eq!(rle_decode(&enc[..n],&mut out,input.len()),Ok(()));assert_eq!(input,out);assert!(rle_decode(&[0,3],&mut out,2).is_err());assert!(rle_decode(&[3,3],&mut out,2).is_err());}
 #[test]fn torn_or_uncommitted_header_is_rejected(){let h=Header{generation:1,count:1,cpu_len:0,cpu_offset:DATA,total:DATA,table_crc:1,cpu_crc:crc(&[])};let b=h.encode(true);assert!(Header::decode(&b).is_ok());let mut torn=b;torn[64]^=1;assert!(Header::decode(&torn).is_err());assert!(Header::decode(&h.encode(false)).is_err());}

 struct Storage(UnsafeCell<[[u8;16384];2]>);
 unsafe impl Sync for Storage {}
 static STORAGE:Storage=Storage(UnsafeCell::new([[0;16384];2]));
 static MEMORY:[u8;4096]=[0x42;4096];
 static CPU:[u8;4]=[1,2,3,4];
 static RESTORED_FIRST:AtomicU8=AtomicU8::new(0);
 static CPU_RESTORED:AtomicU8=AtomicU8::new(0);
 unsafe extern "system" fn quiesce(_: *mut c_void)->i32{OK}
 unsafe extern "system" fn resume(_: *mut c_void)->i32{OK}
 unsafe extern "system" fn enter(_: *mut c_void)->i32{OK}
 unsafe extern "system" fn enumerate(_: *mut c_void,out:*mut HiberMemoryRegion,cap:usize,count:*mut usize)->i32{
  if out.is_null()||count.is_null()||cap<1{return INVALID}
  unsafe{out.write(HiberMemoryRegion{physical_start:0x100000,source:MEMORY.as_ptr(),length:MEMORY.len() as u64});count.write(1);}OK
 }
 unsafe extern "system" fn capture(_: *mut c_void,out:*mut u8,cap:usize,written:*mut usize)->i32{
  if out.is_null()||written.is_null()||cap<CPU.len(){return NO_MEMORY}
  unsafe{ptr::copy_nonoverlapping(CPU.as_ptr(),out,CPU.len());written.write(CPU.len());}OK
 }
 unsafe extern "system" fn restore_cpu(_: *mut c_void,data:*const u8,len:usize)->i32{
  if len!=CPU.len()||data.is_null(){return DATA_ERROR}
  let got=unsafe{core::slice::from_raw_parts(data,len)};
  if got!=CPU{return DATA_ERROR}
  CPU_RESTORED.store(1,Ordering::SeqCst);OK
 }
 unsafe extern "system" fn write(_: *mut c_void,slot:u32,offset:u64,data:*const u8,len:usize)->i32{
  if slot>1||data.is_null(){return INVALID}
  let start=offset as usize;let Some(end)=start.checked_add(len)else{return INVALID};if end>16384{return NO_MEMORY}
  unsafe{ptr::copy_nonoverlapping(data,(*STORAGE.0.get())[slot as usize].as_mut_ptr().add(start),len);}OK
 }
 unsafe extern "system" fn read(_: *mut c_void,slot:u32,offset:u64,out:*mut u8,len:usize)->i32{
  if slot>1||out.is_null(){return INVALID}
  let start=offset as usize;let Some(end)=start.checked_add(len)else{return INVALID};if end>16384{return NO_MEMORY}
  unsafe{ptr::copy_nonoverlapping((*STORAGE.0.get())[slot as usize].as_ptr().add(start),out,len);}OK
 }
 unsafe extern "system" fn flush(_: *mut c_void,_slot:u32)->i32{OK}
 unsafe extern "system" fn restore_mem(_: *mut c_void,physical:u64,data:*const u8,len:usize)->i32{
  if physical<0x100000||physical.checked_add(len as u64).is_none_or(|x|x>0x101000)||data.is_null()||len==0{return DATA_ERROR}
  RESTORED_FIRST.store(unsafe{*data},Ordering::SeqCst);OK
 }
 static OPS:HiberPlatformOps=HiberPlatformOps{size:core::mem::size_of::<HiberPlatformOps>() as u32,context:ptr::null_mut(),quiesce_devices:Some(quiesce),resume_devices:Some(resume),enumerate_memory:Some(enumerate),capture_cpu_state:Some(capture),restore_cpu_state:Some(restore_cpu),write_slot:Some(write),read_slot:Some(read),flush_slot:Some(flush),restore_memory:Some(restore_mem),enter_s4:Some(enter)};

 #[test]fn interrupted_newest_image_falls_back_to_previous_committed_generation(){
  assert_eq!(unsafe{register_platform((&OPS as *const HiberPlatformOps).cast_mut())},OK);
  assert_eq!(initiate_s4(),OK);
  assert_eq!(initiate_s4(),OK);
  assert!(validate_slot(platform().unwrap(),0).is_ok());
  assert!(validate_slot(platform().unwrap(),1).is_ok());
  // Simulate a torn/corrupted newest slot after power loss. The older committed
  // slot must still validate and be selected before any restore writes occur.
  unsafe{let storage=STORAGE.0.get();(*storage)[1][DATA as usize+13]^=0x80;}
  assert_eq!(resume_from_hibernation(),OK);
  assert_eq!(RESTORED_FIRST.load(Ordering::SeqCst),0x42);
  assert_eq!(CPU_RESTORED.load(Ordering::SeqCst),1);
 }
}

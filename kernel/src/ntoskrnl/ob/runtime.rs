//! Bounded NT-style Object Manager core.
//! This provides typed object registration, a case-insensitive namespace, generation-tagged
//! handles, generic access mapping, self-relative security-descriptor DACL checks, and
//! reference/handle rundown. It deliberately does not claim the private Windows OBJECT_HEADER
//! layout, arbitrary object allocation, impersonation tokens, auditing, or full WDK semantics.
use core::sync::atomic::{AtomicBool,Ordering};
use core::ffi::c_void;

pub const STATUS_SUCCESS:i32=0;
pub const STATUS_INVALID_PARAMETER:i32=0xC000_000Du32 as i32;
pub const STATUS_INVALID_HANDLE:i32=0xC000_0008u32 as i32;
pub const STATUS_OBJECT_TYPE_MISMATCH:i32=0xC000_0024u32 as i32;
pub const STATUS_OBJECT_NAME_NOT_FOUND:i32=0xC000_0034u32 as i32;
pub const STATUS_OBJECT_NAME_COLLISION:i32=0xC000_0035u32 as i32;
pub const STATUS_ACCESS_DENIED:i32=0xC000_0022u32 as i32;
pub const STATUS_INSUFFICIENT_RESOURCES:i32=0xC000_009Au32 as i32;
pub const STATUS_OBJECT_PATH_SYNTAX_BAD:i32=0xC000_003Bu32 as i32;
pub const STATUS_INFO_LENGTH_MISMATCH:i32=0xC000_0004u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
pub const STATUS_NOT_SUPPORTED:i32=0xC000_00BBu32 as i32;
pub const OBJ_CASE_INSENSITIVE:u32=0x40;
pub const OBJ_KERNEL_HANDLE:u32=0x200;
pub const GENERIC_READ:u32=0x8000_0000;
pub const GENERIC_WRITE:u32=0x4000_0000;
pub const GENERIC_EXECUTE:u32=0x2000_0000;
pub const GENERIC_ALL:u32=0x1000_0000;
const MAX_OBJECTS:usize=128;
const MAX_HANDLES:usize=256;
const MAX_NAME:usize=128;
const MAX_TYPES:usize=32;
const MAX_AUDIT_EVENTS:usize=64;
const MAX_TOKENS:usize=32;
const MAX_TOKEN_SIDS:usize=8;
const MAX_SID_BYTES:usize=68;
const HANDLE_TAG:usize=0x5244_4f54_0000_0000;
const HANDLE_TAG_MASK:usize=0xffff_ffff_0000_0000;
const HANDLE_SLOT_BITS:usize=8;
const HANDLE_SLOT_MASK:usize=(1<<HANDLE_SLOT_BITS)-1;
const ACCESS_SYSTEM_SECURITY:u32=0x0100_0000;
const MAXIMUM_ALLOWED:u32=0x0200_0000;

#[repr(C)] #[derive(Clone,Copy)]
pub struct UnicodeString { pub length:u16,pub maximum_length:u16,pub buffer:*const u16 }
#[repr(C)]
pub struct ObjectAttributes { pub length:u32,pub root_directory:*mut c_void,pub object_name:*const UnicodeString,pub attributes:u32,pub security_descriptor:*mut c_void,pub security_qos:*mut c_void }
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ObjectType { pub name:UnicodeString,pub type_index:u16,pub reserved:u16,pub generic_read:u32,pub generic_write:u32,pub generic_execute:u32,pub generic_all:u32,pub valid_access:u32 }

#[derive(Clone,Copy)]
struct TypeEntry { used:bool,id:u16,name:[u16;32],len:u8,gr:u32,gw:u32,gx:u32,ga:u32,valid:u32 }
impl TypeEntry { const EMPTY:Self=Self{used:false,id:0,name:[0;32],len:0,gr:0,gw:0,gx:0,ga:0,valid:u32::MAX}; }
#[derive(Clone,Copy)]
struct Object { used:bool, deleting:bool, inserted:bool, body:usize, type_id:u16, name:[u16;MAX_NAME], name_len:u16, refs:u32, handles:u32, permanent:bool, sd:[u8;256], sd_len:u16 }
impl Object { const EMPTY:Self=Self{used:false,deleting:false,inserted:false,body:0,type_id:0,name:[0;MAX_NAME],name_len:0,refs:0,handles:0,permanent:false,sd:[0;256],sd_len:0}; }
#[derive(Clone,Copy)]
struct HandleEntry { used:bool,generation:u16,object:usize,owner:usize,access:u32,attributes:u32 }
impl HandleEntry { const EMPTY:Self=Self{used:false,generation:1,object:0,owner:0,access:0,attributes:0}; }
struct Lock(AtomicBool);
impl Lock { const fn new()->Self{Self(AtomicBool::new(false))} fn lock(&self)->Guard<'_>{while self.0.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop()} Guard(&self.0)} }
struct Guard<'a>(&'a AtomicBool);
impl Drop for Guard<'_>{fn drop(&mut self){self.0.store(false,Ordering::Release)}}
#[derive(Clone,Copy)] struct TokenEntry { used:bool,owner:usize,count:u8,lengths:[u8;MAX_TOKEN_SIDS],sids:[[u8;MAX_SID_BYTES];MAX_TOKEN_SIDS] }
impl TokenEntry { const EMPTY:Self=Self{used:false,owner:0,count:0,lengths:[0;MAX_TOKEN_SIDS],sids:[[0;MAX_SID_BYTES];MAX_TOKEN_SIDS]}; }
#[derive(Clone,Copy)] pub struct AuditEvent { pub sequence:u64, pub object:usize, pub owner:usize, pub operation:u8, pub desired:u32, pub granted:u32, pub status:i32 }
impl AuditEvent { const EMPTY:Self=Self{sequence:0,object:0,owner:0,operation:0,desired:0,granted:0,status:0}; }
#[derive(Clone,Copy)] struct DirectoryBody { magic:u64 }
impl DirectoryBody { const EMPTY:Self=Self{magic:0}; }
struct State { objects:[Object;MAX_OBJECTS],handles:[HandleEntry;MAX_HANDLES],types:[TypeEntry;MAX_TYPES],type_objects:[ObjectType;MAX_TYPES],directories:[DirectoryBody;MAX_OBJECTS],tokens:[TokenEntry;MAX_TOKENS],audit:[AuditEvent;MAX_AUDIT_EVENTS],body_storage:[[u8;512];MAX_OBJECTS],audit_next:u64,next_type:u16 }
impl State { const fn new()->Self{Self{objects:[Object::EMPTY;MAX_OBJECTS],handles:[HandleEntry::EMPTY;MAX_HANDLES],types:[TypeEntry::EMPTY;MAX_TYPES],type_objects:[const { ObjectType{name:UnicodeString{length:0,maximum_length:0,buffer:core::ptr::null()},type_index:0,reserved:0,generic_read:0,generic_write:0,generic_execute:0,generic_all:0,valid_access:0} };MAX_TYPES],directories:[DirectoryBody::EMPTY;MAX_OBJECTS],tokens:[TokenEntry::EMPTY;MAX_TOKENS],audit:[AuditEvent::EMPTY;MAX_AUDIT_EVENTS],body_storage:[[0;512];MAX_OBJECTS],audit_next:1,next_type:1}} }
static LOCK:Lock=Lock::new();
static mut STATE:State=State::new();
fn state()->&'static mut State{unsafe{&mut *core::ptr::addr_of_mut!(STATE)}}
fn fold(c:u16)->u16{if (b'A' as u16..=b'Z' as u16).contains(&c){c+32}else{c}}
fn eq(a:&[u16],b:&[u16])->bool{a.len()==b.len()&&a.iter().zip(b).all(|(x,y)|fold(*x)==fold(*y))}
unsafe fn read_unicode(s:*const UnicodeString,out:&mut [u16;MAX_NAME])->Result<usize,i32>{
 if s.is_null(){return Err(STATUS_INVALID_PARAMETER)}let u=unsafe{&*s};
 if u.length%2!=0||u.length>u.maximum_length||u.length as usize/2>MAX_NAME||(u.length!=0&&u.buffer.is_null()){return Err(STATUS_INVALID_PARAMETER)}
 let n=u.length as usize/2;for i in 0..n{out[i]=unsafe{core::ptr::read_unaligned(u.buffer.add(i))};}
 if out[..n].iter().any(|c|*c==0){return Err(STATUS_INVALID_PARAMETER)}Ok(n)
}
fn map_access(t:&TypeEntry,mask:u32)->u32{let mut m=mask;if m&GENERIC_READ!=0{m=(m&!GENERIC_READ)|t.gr}if m&GENERIC_WRITE!=0{m=(m&!GENERIC_WRITE)|t.gw}if m&GENERIC_EXECUTE!=0{m=(m&!GENERIC_EXECUTE)|t.gx}if m&GENERIC_ALL!=0{m=(m&!GENERIC_ALL)|t.ga}m}
fn type_idx(s:&State,id:u16)->Option<usize>{s.types.iter().position(|t|t.used&&t.id==id)}
fn object_idx(s:&State,body:usize)->Option<usize>{s.objects.iter().position(|o|o.used&&o.body==body&&!o.deleting)}
fn find_name(s:&State,name:&[u16])->Option<usize>{s.objects.iter().position(|o|o.used&&!o.deleting&&o.inserted&&o.name_len as usize==name.len()&&eq(&o.name[..o.name_len as usize],name))}
fn encode_handle(i:usize,g:u16)->*mut c_void{(HANDLE_TAG|((g as usize)<<HANDLE_SLOT_BITS)|i) as *mut c_void}
fn decode_handle(h:*mut c_void)->Option<(usize,u16)>{let v=h as usize;if v&HANDLE_TAG_MASK!=HANDLE_TAG{return None}Some((v&HANDLE_SLOT_MASK,(v>>HANDLE_SLOT_BITS) as u16))}
fn handle_idx(s:&State,h:*mut c_void,owner:usize)->Option<usize>{let(i,g)=decode_handle(h)?;let e=s.handles.get(i)?;if e.used&&e.generation==g&&(e.owner==owner||owner==usize::MAX){Some(i)}else{None}}

/// Register a typed object body. Body storage/lifetime remains owned by its creator.
/// A namespace name is optional; names are case-insensitive and unique.
pub unsafe fn register_object(body:*mut c_void,type_id:u16,name:&[u16],permanent:bool,sd:*const u8,sd_len:usize)->Result<(),i32>{
 if body.is_null()||name.len()>MAX_NAME||sd_len>256||(sd_len!=0&&sd.is_null()){return Err(STATUS_INVALID_PARAMETER)}
 let _g=LOCK.lock();let s=state();if type_idx(s,type_id).is_none(){return Err(STATUS_OBJECT_TYPE_MISMATCH)}
 if raw_object_idx(s,body as usize).is_some(){return Err(STATUS_OBJECT_NAME_COLLISION)}
 if !name.is_empty()&&find_name(s,name).is_some(){return Err(STATUS_OBJECT_NAME_COLLISION)}
 let Some(i)=s.objects.iter().position(|o|!o.used)else{return Err(STATUS_INSUFFICIENT_RESOURCES)};
 let mut o=Object{used:true,body:body as usize,type_id,name_len:name.len() as u16,refs:1,permanent,inserted:true,..Object::EMPTY};o.name[..name.len()].copy_from_slice(name);
 if sd_len!=0{unsafe{core::ptr::copy_nonoverlapping(sd,o.sd.as_mut_ptr(),sd_len)}o.sd_len=sd_len as u16;}
 s.objects[i]=o;Ok(())
}
pub fn set_token_sids(owner:usize,sids:&[&[u8]])->Result<(),i32>{
 if sids.len()>MAX_TOKEN_SIDS{return Err(STATUS_INVALID_PARAMETER)}
 let mut entry=TokenEntry{used:true,owner,count:sids.len() as u8,..TokenEntry::EMPTY};
 for (i,sid) in sids.iter().enumerate(){if sid.len()<8||sid.len()>MAX_SID_BYTES||sid[0]!=1||sid[1] as usize*4+8!=sid.len(){return Err(STATUS_INVALID_PARAMETER)}
  entry.lengths[i]=sid.len() as u8;entry.sids[i][..sid.len()].copy_from_slice(sid);
 }
 let _g=LOCK.lock();let s=state();let i=s.tokens.iter().position(|t|t.used&&t.owner==owner).or_else(||s.tokens.iter().position(|t|!t.used)).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
 s.tokens[i]=entry;Ok(())
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotObSetProcessTokenSids(owner:usize,sid_ptrs:*const *const u8,lengths:*const u8,count:u32)->i32{
 if count as usize>MAX_TOKEN_SIDS||(count!=0&&(sid_ptrs.is_null()||lengths.is_null())){return STATUS_INVALID_PARAMETER}
 let mut owned=[[0u8;MAX_SID_BYTES];MAX_TOKEN_SIDS];let mut lens=[0usize;MAX_TOKEN_SIDS];
 for i in 0..count as usize{let p=unsafe{core::ptr::read_unaligned(sid_ptrs.add(i))};let n=unsafe{core::ptr::read_unaligned(lengths.add(i))} as usize;if p.is_null()||n>MAX_SID_BYTES||n<8{return STATUS_INVALID_PARAMETER}
  unsafe{core::ptr::copy_nonoverlapping(p,owned[i].as_mut_ptr(),n)}lens[i]=n;
 }
 let refs: [&[u8];MAX_TOKEN_SIDS]=core::array::from_fn(|i|&owned[i][..lens[i]]);
 set_token_sids(owner,&refs[..count as usize]).map_or_else(|e|e,|_|STATUS_SUCCESS)
}
fn sid_matches(s:&State,owner:usize,sid:&[u8])->bool{
 let everyone=sid.len()==12&&sid[0]==1&&sid[1]==1&&sid[2..7].iter().all(|x|*x==0)&&sid[7]==1&&sid[8..12]==[0,0,0,0];
 everyone||s.tokens.iter().find(|t|t.used&&t.owner==owner).is_some_and(|t|(0..t.count as usize).any(|i|t.lengths[i] as usize==sid.len()&&t.sids[i][..sid.len()]==*sid))
}
pub fn ensure_type(name:&[u16],gr:u32,gw:u32,gx:u32,ga:u32,valid:u32)->Result<u16,i32>{
 {let _g=LOCK.lock();let s=state();if let Some(t)=s.types.iter().find(|t|t.used&&eq(&t.name[..t.len as usize],name)){return Ok(t.id)}}
 register_type(name,gr,gw,gx,ga,valid)
}
pub unsafe fn register_external_object(body:*mut c_void,type_name:&[u16],name:&[u16],permanent:bool,sd:*const u8,sd_len:usize)->Result<u16,i32>{
 let ty=ensure_type(type_name,1,2,4,7,u32::MAX)?;unsafe{register_object(body,ty,name,permanent,sd,sd_len)?;}Ok(ty)
}
fn raw_object_idx(s:&State,body:usize)->Option<usize>{s.objects.iter().position(|o|o.used&&o.body==body)}
fn audit_locked(s:&mut State,object:usize,owner:usize,operation:u8,desired:u32,granted:u32,status:i32){
 let i=(s.audit_next as usize)%MAX_AUDIT_EVENTS;s.audit[i]=AuditEvent{sequence:s.audit_next,object,owner,operation,desired,granted,status};s.audit_next=s.audit_next.wrapping_add(1).max(1);
}
pub fn unregister_object(body:*mut c_void)->bool{
 if body.is_null(){return false}let _g=LOCK.lock();let s=state();let Some(i)=raw_object_idx(s,body as usize)else{return false};
 if s.objects[i].deleting{return true} s.objects[i].deleting=true;s.objects[i].permanent=false;s.objects[i].refs=s.objects[i].refs.saturating_sub(1);
 if s.objects[i].refs==0&&s.objects[i].handles==0{s.objects[i]=Object::EMPTY;}true
}
pub fn duplicate_handle(source:*mut c_void,source_owner:usize,target_owner:usize,desired:u32,same_access:bool,kernel_target:bool)->Result<*mut c_void,i32>{
 let _g=LOCK.lock();let s=state();let hi=handle_idx(s,source,source_owner).ok_or(STATUS_INVALID_HANDLE)?;let src=s.handles[hi];let granted=if same_access{src.access}else{if desired&!src.access!=0{return Err(STATUS_ACCESS_DENIED)}desired};
 let new_owner=if kernel_target{usize::MAX}else{target_owner};let ni=s.handles.iter().position(|h|!h.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
 if !s.objects[src.object].used||s.objects[src.object].deleting{return Err(STATUS_INVALID_HANDLE)}
 s.objects[src.object].refs=s.objects[src.object].refs.checked_add(1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;s.objects[src.object].handles=s.objects[src.object].handles.saturating_add(1);
 let generation=s.handles[ni].generation.max(1);s.handles[ni]=HandleEntry{used:true,generation,object:src.object,owner:new_owner,access:granted,attributes:src.attributes};
 audit_locked(s,s.objects[src.object].body,target_owner,5,desired,granted,STATUS_SUCCESS);Ok(encode_handle(ni,generation))
}
unsafe fn security_descriptor_length(sd:*const u8)->Result<usize,i32>{
 if sd.is_null(){return Err(STATUS_INVALID_PARAMETER)}let b=unsafe{core::slice::from_raw_parts(sd,20)};
 if b[0]!=1||read_u16(b,2).unwrap_or(0)&0x8000==0{return Err(STATUS_NOT_SUPPORTED)}
 let mut need=20usize;
 for off_at in [4usize,8usize]{let off=read_u32(b,off_at).unwrap_or(0) as usize;if off!=0{if off+8>256{return Err(STATUS_INVALID_PARAMETER)}let count=unsafe{*sd.add(off+1)} as usize;let len=8usize.checked_add(count.checked_mul(4).ok_or(STATUS_INVALID_PARAMETER)?).ok_or(STATUS_INVALID_PARAMETER)?;if off+len>256{return Err(STATUS_INVALID_PARAMETER)}need=need.max(off+len);}}
 for off_at in [12usize,16usize]{let off=read_u32(b,off_at).unwrap_or(0) as usize;if off!=0{if off+8>256{return Err(STATUS_INVALID_PARAMETER)}let len=unsafe{core::ptr::read_unaligned(sd.add(off+2).cast::<u16>())} as usize;if len<8||off.checked_add(len).is_none_or(|e|e>256){return Err(STATUS_INVALID_PARAMETER)}need=need.max(off+len);}}
 if need>256{return Err(STATUS_INVALID_PARAMETER)}Ok(need)
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObSetSecurityObject(object:*mut c_void,security_information:u32,sd:*const u8)->i32{
 if security_information==0||security_information&!0x0fu32!=0{return STATUS_INVALID_PARAMETER}
 let len=match unsafe{security_descriptor_length(sd)}{Ok(n)=>n,Err(e)=>return e};
 let mut copy=[0u8;256];unsafe{core::ptr::copy_nonoverlapping(sd,copy.as_mut_ptr(),len)}
 let _g=LOCK.lock();let s=state();let Some(i)=object_idx(s,object as usize)else{return STATUS_INVALID_PARAMETER};
 s.objects[i].sd=copy;s.objects[i].sd_len=len as u16;STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObQuerySecurityObject(object:*mut c_void,security_information:u32,sd:*mut u8,length:u32,needed:*mut u32)->i32{
 if security_information==0||security_information&!0x0fu32!=0||needed.is_null(){return STATUS_INVALID_PARAMETER}
 let _g=LOCK.lock();let s=state();let Some(i)=object_idx(s,object as usize)else{return STATUS_INVALID_PARAMETER};let n=s.objects[i].sd_len as usize;
 unsafe{needed.write(n as u32)}if n==0{return STATUS_OBJECT_NAME_NOT_FOUND}if sd.is_null()||(length as usize)<n{return STATUS_BUFFER_TOO_SMALL}
 unsafe{core::ptr::copy_nonoverlapping(s.objects[i].sd.as_ptr(),sd,n)}STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotObDuplicateHandle(source:*mut c_void,source_owner:usize,target_owner:usize,desired_access:u32,same_access:u8,kernel_target:u8,out:*mut *mut c_void)->i32{
 if out.is_null(){return STATUS_INVALID_PARAMETER}unsafe{out.write(core::ptr::null_mut())}
 match duplicate_handle(source,source_owner,target_owner,desired_access,same_access!=0,kernel_target!=0){Ok(h)=>{unsafe{out.write(h)}STATUS_SUCCESS},Err(e)=>e}
}
pub fn read_audit_events(out:&mut [AuditEvent])->usize{
 let _g=LOCK.lock();let s=state();let available=(s.audit_next.saturating_sub(1) as usize).min(MAX_AUDIT_EVENTS).min(out.len());
 let first=s.audit_next.saturating_sub(available as u64);
 for n in 0..available{let seq=first+n as u64;let i=(seq as usize)%MAX_AUDIT_EVENTS;out[n]=s.audit[i];}
 available
}
pub fn create_directory(name:&[u16],sd:*const u8,sd_len:usize)->Result<*mut c_void,i32>{
 if name.is_empty()||name.len()>MAX_NAME||sd_len>256||(sd_len!=0&&sd.is_null()){return Err(STATUS_INVALID_PARAMETER)}
 let ty=ensure_type(&[b'D' as u16,b'i' as u16,b'r' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'o' as u16,b'r' as u16,b'y' as u16],1,2,4,7,u32::MAX)?;
 let _g=LOCK.lock();let s=state();if find_name(s,name).is_some(){return Err(STATUS_OBJECT_NAME_COLLISION)}
 let i=s.objects.iter().position(|o|!o.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
 s.directories[i].magic=0x52444f5444495231;let body=core::ptr::addr_of_mut!(s.directories[i]).cast::<c_void>();
 let mut o=Object{used:true,body:body as usize,type_id:ty,name_len:name.len() as u16,refs:1,permanent:true,inserted:true,..Object::EMPTY};o.name[..name.len()].copy_from_slice(name);
 if sd_len!=0{unsafe{core::ptr::copy_nonoverlapping(sd,o.sd.as_mut_ptr(),sd_len)}o.sd_len=sd_len as u16;}s.objects[i]=o;Ok(body)
}
pub fn register_type(name:&[u16],gr:u32,gw:u32,gx:u32,ga:u32,valid:u32)->Result<u16,i32>{
 if name.is_empty()||name.len()>32{return Err(STATUS_INVALID_PARAMETER)}let _g=LOCK.lock();let s=state();
 if s.types.iter().any(|t|t.used&&eq(&t.name[..t.len as usize],name)){return Err(STATUS_OBJECT_NAME_COLLISION)}
 let Some(i)=s.types.iter().position(|t|!t.used)else{return Err(STATUS_INSUFFICIENT_RESOURCES)};
 let id=s.next_type;s.next_type=s.next_type.wrapping_add(1).max(1);let mut t=TypeEntry{used:true,id,len:name.len() as u8,gr,gw,gx,ga,valid,..TypeEntry::EMPTY};t.name[..name.len()].copy_from_slice(name);s.types[i]=t;s.type_objects[i]=ObjectType{name:UnicodeString{length:(name.len()*2) as u16,maximum_length:((name.len()+1)*2) as u16,buffer:s.types[i].name.as_ptr()},type_index:id,reserved:0,generic_read:gr,generic_write:gw,generic_execute:gx,generic_all:ga,valid_access:valid};Ok(id)
}
pub fn reference_body(body:*mut c_void,expected_type:u16,desired:u32,kernel_mode:bool)->Result<*mut c_void,i32>{
 if body.is_null(){return Err(STATUS_INVALID_PARAMETER)}let _g=LOCK.lock();let s=state();let i=object_idx(s,body as usize).ok_or(STATUS_INVALID_PARAMETER)?;
 let t=s.types[type_idx(s,s.objects[i].type_id).ok_or(STATUS_OBJECT_TYPE_MISMATCH)?];
 if expected_type!=0&&s.objects[i].type_id!=expected_type{return Err(STATUS_OBJECT_TYPE_MISMATCH)}
 let granted=map_access(&t,desired);if granted&!t.valid!=0{return Err(STATUS_ACCESS_DENIED)}
 if !kernel_mode&&!access_check(s,&s.objects[i],granted,0){return Err(STATUS_ACCESS_DENIED)}
 s.objects[i].refs=s.objects[i].refs.checked_add(1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;audit_locked(s,body as usize,0,4,desired,granted,STATUS_SUCCESS);Ok(body)
}
pub fn dereference_body(body:*mut c_void)->bool{
 if body.is_null(){return false}let _g=LOCK.lock();let s=state();let Some(i)=raw_object_idx(s,body as usize)else{return false};
 if s.objects[i].refs==0{return false}s.objects[i].refs-=1;
 if s.objects[i].refs==0&&s.objects[i].handles==0&&(!s.objects[i].permanent||s.objects[i].deleting){s.objects[i]=Object::EMPTY;}true
}
pub fn open_by_name(name:&[u16],desired:u32,owner:usize,kernel_mode:bool,attrs:u32)->Result<*mut c_void,i32>{
 let _g=LOCK.lock();let s=state();let i=find_name(s,name).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
 let ti=type_idx(s,s.objects[i].type_id).ok_or(STATUS_OBJECT_TYPE_MISMATCH)?;let t=s.types[ti];let requested=map_access(&t,desired&!MAXIMUM_ALLOWED);let allowed=if kernel_mode{t.valid}else{dacl_allowed(s,&s.objects[i],owner)&t.valid};let granted=if desired&MAXIMUM_ALLOWED!=0{allowed}else{requested};
 if granted&!t.valid!=0||requested&!allowed!=0{return Err(STATUS_ACCESS_DENIED)}
 let hi=s.handles.iter().position(|h|!h.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
 s.objects[i].refs=s.objects[i].refs.checked_add(1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;s.objects[i].handles+=1;
 let g=s.handles[hi].generation.max(1);s.handles[hi]=HandleEntry{used:true,generation:g,object:i,owner:if attrs&OBJ_KERNEL_HANDLE!=0{usize::MAX}else{owner},access:granted,attributes:attrs};audit_locked(s,s.objects[i].body,owner,1,desired,granted,STATUS_SUCCESS);Ok(encode_handle(hi,g))
}
pub fn open_by_pointer(body:*mut c_void,desired:u32,owner:usize,kernel_mode:bool,attrs:u32)->Result<*mut c_void,i32>{
 let _g=LOCK.lock();let s=state();let i=object_idx(s,body as usize).ok_or(STATUS_INVALID_PARAMETER)?;let t=s.types[type_idx(s,s.objects[i].type_id).ok_or(STATUS_OBJECT_TYPE_MISMATCH)?];let requested=map_access(&t,desired&!MAXIMUM_ALLOWED);let allowed=if kernel_mode{t.valid}else{dacl_allowed(s,&s.objects[i],owner)&t.valid};let granted=if desired&MAXIMUM_ALLOWED!=0{allowed}else{requested};
 if granted&!t.valid!=0||requested&!allowed!=0{return Err(STATUS_ACCESS_DENIED)}
 let hi=s.handles.iter().position(|h|!h.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;s.objects[i].refs=s.objects[i].refs.checked_add(1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;s.objects[i].handles+=1;let g=s.handles[hi].generation.max(1);s.handles[hi]=HandleEntry{used:true,generation:g,object:i,owner:if attrs&OBJ_KERNEL_HANDLE!=0{usize::MAX}else{owner},access:granted,attributes:attrs};audit_locked(s,s.objects[i].body,owner,2,desired,granted,STATUS_SUCCESS);Ok(encode_handle(hi,g))
}
pub fn reference_by_handle(h:*mut c_void,desired:u32,expected_type:u16,owner:usize,kernel_mode:bool)->Result<*mut c_void,i32>{
 let _g=LOCK.lock();let s=state();let hi=handle_idx(s,h,if kernel_mode{usize::MAX}else{owner}).ok_or(STATUS_INVALID_HANDLE)?;let e=s.handles[hi];let o=s.objects.get(e.object).ok_or(STATUS_INVALID_HANDLE)?;if !o.used||o.deleting{return Err(STATUS_INVALID_HANDLE)}if expected_type!=0&&o.type_id!=expected_type{return Err(STATUS_OBJECT_TYPE_MISMATCH)}
 let t=s.types[type_idx(s,o.type_id).ok_or(STATUS_OBJECT_TYPE_MISMATCH)?];let mapped=if desired&MAXIMUM_ALLOWED!=0{e.access}else{map_access(&t,desired)};if mapped&!e.access!=0{return Err(STATUS_ACCESS_DENIED)}if !kernel_mode&&!access_check(s,o,mapped,owner){return Err(STATUS_ACCESS_DENIED)}let body=o.body as *mut c_void;
 let s=state();s.objects[e.object].refs=s.objects[e.object].refs.checked_add(1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;audit_locked(s,body as usize,owner,3,desired,desired,STATUS_SUCCESS);let _=t;Ok(body)
}
pub fn close_handle(h:*mut c_void,owner:usize)->Result<(),i32>{
 let _g=LOCK.lock();let s=state();let hi=handle_idx(s,h,owner).ok_or(STATUS_INVALID_HANDLE)?;let oi=s.handles[hi].object;
 s.handles[hi].used=false;s.handles[hi].object=0;s.handles[hi].generation=s.handles[hi].generation.wrapping_add(1).max(1);
 if s.objects[oi].handles>0{s.objects[oi].handles-=1}if s.objects[oi].refs>0{s.objects[oi].refs-=1}
 if s.objects[oi].refs==0&&s.objects[oi].handles==0&&!s.objects[oi].permanent{s.objects[oi]=Object::EMPTY;}else if s.objects[oi].refs==0&&s.objects[oi].handles==0&&s.objects[oi].deleting{s.objects[oi]=Object::EMPTY;}Ok(())
}
pub fn reference_by_name(name:&[u16],expected_type:u16,desired:u32,kernel_mode:bool)->Result<*mut c_void,i32>{
 let _g=LOCK.lock();let s=state();let i=find_name(s,name).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
 if expected_type!=0&&s.objects[i].type_id!=expected_type{return Err(STATUS_OBJECT_TYPE_MISMATCH)}
 let t=s.types[type_idx(s,s.objects[i].type_id).ok_or(STATUS_OBJECT_TYPE_MISMATCH)?];let granted=map_access(&t,desired);
 if granted&!t.valid!=0||(!kernel_mode&&!access_check(s,&s.objects[i],granted,0)){return Err(STATUS_ACCESS_DENIED)}
 s.objects[i].refs=s.objects[i].refs.checked_add(1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;Ok(s.objects[i].body as *mut c_void)
}
pub fn object_name(body:*mut c_void,out:&mut [u16;MAX_NAME])->Result<usize,i32>{
 let _g=LOCK.lock();let s=state();let i=object_idx(s,body as usize).ok_or(STATUS_INVALID_PARAMETER)?;let n=s.objects[i].name_len as usize;out[..n].copy_from_slice(&s.objects[i].name[..n]);Ok(n)
}
pub fn make_temporary(body:*mut c_void){let _g=LOCK.lock();let s=state();if let Some(i)=object_idx(s,body as usize){s.objects[i].permanent=false;}}
pub fn type_id_from_pointer(ptr:*mut ObjectType)->Result<u16,i32>{expected_type_id(ptr).filter(|id|*id!=0).ok_or(STATUS_OBJECT_TYPE_MISMATCH)}

pub fn object_type(body:*mut c_void)->*mut ObjectType{let _g=LOCK.lock();let s=state();let Some(i)=object_idx(s,body as usize)else{return core::ptr::null_mut()};let Some(ti)=type_idx(s,s.objects[i].type_id)else{return core::ptr::null_mut()};core::ptr::addr_of_mut!(s.type_objects[ti])}
fn expected_type_id(ptr:*mut ObjectType)->Option<u16>{if ptr.is_null(){return Some(0)}let _g=LOCK.lock();let s=state();s.type_objects.iter().enumerate().find(|(i,t)|s.types[*i].used&&core::ptr::eq(*t as *const ObjectType,ptr as *const ObjectType)).map(|(i,_)|s.types[i].id)}

pub fn set_permanent(body:*mut c_void,permanent:bool)->Result<(),i32>{let _g=LOCK.lock();let s=state();let i=object_idx(s,body as usize).ok_or(STATUS_INVALID_PARAMETER)?;s.objects[i].permanent=permanent;Ok(())}
fn read_u16(b:&[u8],o:usize)->Option<u16>{Some(u16::from_le_bytes([*b.get(o)?,*b.get(o+1)?]))}
fn read_u32(b:&[u8],o:usize)->Option<u32>{Some(u32::from_le_bytes([*b.get(o)?,*b.get(o+1)?,*b.get(o+2)?,*b.get(o+3)?]))}
/// Evaluate a bounded Windows self-relative security descriptor DACL for the Everyone SID.
/// Kernel-mode references bypass DACL checks as in NT; user-mode token/SID evaluation is
/// not implemented, so only an explicit Everyone allow ACE can grant user-mode access.
fn dacl_allowed(s:&State,o:&Object,owner:usize)->u32{
 if o.sd_len==0{return 0}let b=&o.sd[..o.sd_len as usize];if b.len()<20||b[0]!=1{return 0}
 let control=read_u16(b,2).unwrap_or(0);if control&0x8000==0{return 0}
 if control&0x0004==0{return u32::MAX}
 let off=read_u32(b,16).unwrap_or(0) as usize;if off==0{return u32::MAX}
 if off+8>b.len(){return 0}let acl_size=read_u16(b,off+2).unwrap_or(0) as usize;let ace_count=read_u16(b,off+4).unwrap_or(0) as usize;
 if acl_size<8||off.checked_add(acl_size).is_none_or(|e|e>b.len()){return 0}
 let mut p=off+8;let mut granted=0u32;let mut denied=0u32;
 for _ in 0..ace_count{if p+8>off+acl_size{return 0}let ty=b[p];let size=read_u16(b,p+2).unwrap_or(0) as usize;if size<8||p.checked_add(size).is_none_or(|e|e>off+acl_size){return 0}
  let mask=read_u32(b,p+4).unwrap_or(0);let sid=&b[p+8..p+size];
  if sid_matches(s,owner,sid){if ty==1{denied|=mask}else if ty==0{granted|=mask}}p+=size;
 }
 granted&!denied
}
fn access_check(s:&State,o:&Object,desired:u32,owner:usize)->bool{dacl_allowed(s,o,owner)&desired==desired}

/// Windows x64 export: ObReferenceObjectByPointer.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObCreateObject(previous_mode:u8,object_type:*mut ObjectType,oa:*const ObjectAttributes,object_mode:u8,parse_context:*mut c_void,body_size:usize,paged_charge:usize,nonpaged_charge:usize,out_object:*mut *mut c_void)->i32{
 if out_object.is_null()||body_size>512{return STATUS_INVALID_PARAMETER}unsafe{out_object.write(core::ptr::null_mut())}
 if !parse_context.is_null()||object_mode>1||previous_mode>1{return STATUS_NOT_SUPPORTED}
 let ty=match expected_type_id(object_type){Some(id)if id!=0=>id,_=>return STATUS_OBJECT_TYPE_MISMATCH};
 let mut name=[0u16;MAX_NAME];let name_len=if oa.is_null(){0}else{
  let a=unsafe{&*oa};if (a.length as usize)<core::mem::size_of::<ObjectAttributes>()||!a.security_descriptor.is_null(){return STATUS_NOT_SUPPORTED}
  if a.object_name.is_null(){0}else{match unsafe{resolve_attributes_name(oa,&mut name)}{Ok(n)=>n,Err(e)=>return e}}
 };
 let _g=LOCK.lock();let s=state();let i=match s.objects.iter().position(|o|!o.used){Some(i)=>i,None=>return STATUS_INSUFFICIENT_RESOURCES};
 if name_len!=0&&find_name(s,&name[..name_len]).is_some(){return STATUS_OBJECT_NAME_COLLISION}
 s.body_storage[i]=[0;512];let body=core::ptr::addr_of_mut!(s.body_storage[i]).cast::<u8>().cast::<c_void>();
 let mut o=Object{used:true,body:body as usize,type_id:ty,name,name_len:name_len as u16,refs:1,permanent:false,inserted:false,..Object::EMPTY};
 if name_len!=0{o.name[..name_len].copy_from_slice(&name[..name_len]);}
 s.objects[i]=o;unsafe{out_object.write(body)}let _=(paged_charge,nonpaged_charge);STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObInsertObject(object:*mut c_void,access_state:*mut c_void,desired_access:u32,pointer_bias:u32,new_object:*mut *mut c_void,out_handle:*mut *mut c_void)->i32{
 if object.is_null()||out_handle.is_null(){return STATUS_INVALID_PARAMETER}unsafe{out_handle.write(core::ptr::null_mut())}
 if !access_state.is_null(){return STATUS_NOT_SUPPORTED}
 {
  let _g=LOCK.lock();let s=state();let Some(i)=object_idx(s,object as usize)else{return STATUS_INVALID_PARAMETER};
  if s.objects[i].inserted{return STATUS_OBJECT_NAME_COLLISION}
  if s.objects[i].name_len!=0&&find_name(s,&s.objects[i].name[..s.objects[i].name_len as usize]).is_some(){return STATUS_OBJECT_NAME_COLLISION}
  s.objects[i].inserted=true;
 }
 match open_by_pointer(object,desired_access,0,true,0){
  Ok(h)=>{if !new_object.is_null(){unsafe{new_object.write(object)}}unsafe{out_handle.write(h)}
   if pointer_bias>0{let _=reference_body(object,0,0,true);}STATUS_SUCCESS},
  Err(e)=>{let _g=LOCK.lock();let s=state();if let Some(i)=raw_object_idx(s,object as usize){s.objects[i].inserted=false;}e}
 }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObDeleteObject(object:*mut c_void){let _=unregister_object(object);}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObCreateObjectType(name:*const UnicodeString,initializer:*const u8,default_sd:*mut c_void,out_type:*mut *mut ObjectType)->i32{
 if out_type.is_null()||initializer.is_null(){return STATUS_INVALID_PARAMETER}unsafe{out_type.write(core::ptr::null_mut())}
 if !default_sd.is_null(){return STATUS_NOT_SUPPORTED}
 let mut units=[0u16;MAX_NAME];let n=match unsafe{read_unicode(name,&mut units)}{Ok(n)=>n,Err(e)=>return e};
 if n==0||n>32{return STATUS_INVALID_PARAMETER}
 let len=unsafe{core::ptr::read_unaligned(initializer.cast::<u16>())} as usize;
 if len<28||len>0x100{return STATUS_INVALID_PARAMETER}
 let gr=unsafe{core::ptr::read_unaligned(initializer.add(8).cast::<u32>())};
 let gw=unsafe{core::ptr::read_unaligned(initializer.add(12).cast::<u32>())};
 let gx=unsafe{core::ptr::read_unaligned(initializer.add(16).cast::<u32>())};
 let ga=unsafe{core::ptr::read_unaligned(initializer.add(20).cast::<u32>())};
 let valid=unsafe{core::ptr::read_unaligned(initializer.add(24).cast::<u32>())};
 let id=match register_type(&units[..n],gr,gw,gx,ga,valid){Ok(id)=>id,Err(e)=>return e};
 let _=id;let _g=LOCK.lock();let s=state();let Some(i)=s.types.iter().position(|t|t.used&&t.id==id)else{return STATUS_OBJECT_TYPE_MISMATCH};
 unsafe{out_type.write(core::ptr::addr_of_mut!(s.type_objects[i]))}STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObReferenceObjectByPointer(object:*mut c_void,desired_access:u32,object_type:*mut ObjectType,access_mode:u8)->i32{
 let expected=match expected_type_id(object_type){Some(v)=>v,None=>return STATUS_OBJECT_TYPE_MISMATCH};
 match reference_body(object,expected,desired_access,access_mode==0){Ok(_)=>STATUS_SUCCESS,Err(e)=>e}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObDereferenceObject(object:*mut c_void)->*mut c_void{let _=dereference_body(object);object}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObfReferenceObject(object:*mut c_void){let _=reference_body(object,0,0,true);}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObfDereferenceObject(object:*mut c_void){let _=dereference_body(object);}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObReferenceObjectByHandle(handle:*mut c_void,desired_access:u32,object_type:*mut ObjectType,access_mode:u8,out_object:*mut *mut c_void,_handle_information:*mut u32)->i32{
 if out_object.is_null(){return STATUS_INVALID_PARAMETER}unsafe{out_object.write(core::ptr::null_mut())}
 let expected=match expected_type_id(object_type){Some(v)=>v,None=>return STATUS_OBJECT_TYPE_MISMATCH};
 match reference_by_handle(handle,desired_access,expected,0,access_mode==0){Ok(o)=>{unsafe{out_object.write(o)}STATUS_SUCCESS},Err(e)=>e}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObCloseHandle(handle:*mut c_void,previous_mode:u8)->i32{close_handle(handle,if previous_mode==0{usize::MAX}else{0}).map_or_else(|e|e,|_|STATUS_SUCCESS)}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObGetObjectType(object:*mut c_void)->*mut ObjectType{object_type(object)}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObReferenceObjectByPointerWithTag(object:*mut c_void,desired_access:u32,object_type:*mut ObjectType,access_mode:u8,_tag:u32)->i32{
 let expected=match expected_type_id(object_type){Some(v)=>v,None=>return STATUS_OBJECT_TYPE_MISMATCH};
 match reference_body(object,expected,desired_access,access_mode==0){Ok(_)=>STATUS_SUCCESS,Err(e)=>e}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObReferenceObjectByHandleWithTag(handle:*mut c_void,desired_access:u32,object_type:*mut ObjectType,access_mode:u8,out_object:*mut *mut c_void,handle_information:*mut u32,tag:u32)->i32{
 let _=tag;unsafe{ObReferenceObjectByHandle(handle,desired_access,object_type,access_mode,out_object,handle_information)}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObOpenObjectByPointer(object:*mut c_void,handle_attributes:u32,passed_access_state:*mut c_void,desired_access:u32,object_type:*mut ObjectType,access_mode:u8,out_handle:*mut *mut c_void)->i32{
 if out_handle.is_null(){return STATUS_INVALID_PARAMETER}unsafe{out_handle.write(core::ptr::null_mut())}
 if !passed_access_state.is_null(){return STATUS_NOT_SUPPORTED}
 let expected=match expected_type_id(object_type){Some(v)=>v,None=>return STATUS_OBJECT_TYPE_MISMATCH};
 let actual={let _g=LOCK.lock();let s=state();object_idx(s,object as usize).map(|i|s.objects[i].type_id)};
 if (expected!=0&&actual!=Some(expected))||object.is_null(){return STATUS_OBJECT_TYPE_MISMATCH}
 match open_by_pointer(object,desired_access,0,access_mode==0,handle_attributes){Ok(h)=>{unsafe{out_handle.write(h)}STATUS_SUCCESS},Err(e)=>e}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObReferenceObjectByName(name:*const UnicodeString,attributes:u32,passed_access_state:*mut c_void,desired_access:u32,object_type:*mut ObjectType,access_mode:u8,parse_context:*mut c_void,out_object:*mut *mut c_void)->i32{
 if out_object.is_null(){return STATUS_INVALID_PARAMETER}unsafe{out_object.write(core::ptr::null_mut())}
 if !passed_access_state.is_null()||!parse_context.is_null(){return STATUS_NOT_SUPPORTED}
 let mut units=[0u16;MAX_NAME];let n=match unsafe{read_unicode(name,&mut units)}{Ok(n)=>n,Err(e)=>return e};
 let expected=match expected_type_id(object_type){Some(v)=>v,None=>return STATUS_OBJECT_TYPE_MISMATCH};
 let _=attributes;
 match reference_by_name(&units[..n],expected,desired_access,access_mode==0){Ok(o)=>{unsafe{out_object.write(o)}STATUS_SUCCESS},Err(e)=>e}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObQueryNameString(object:*mut c_void,buffer:*mut c_void,length:u32,return_length:*mut u32)->i32{
 if object.is_null()||buffer.is_null(){return STATUS_INVALID_PARAMETER}
 let mut name=[0u16;MAX_NAME];let n=match object_name(object,&mut name){Ok(n)=>n,Err(e)=>return e};
 let need=core::mem::size_of::<UnicodeString>()+(n+1)*2;
 if !return_length.is_null(){unsafe{return_length.write(need as u32)}}
 if (length as usize)<need{return STATUS_INFO_LENGTH_MISMATCH}
 let text=unsafe{buffer.cast::<u8>().add(core::mem::size_of::<UnicodeString>()).cast::<u16>()};
 unsafe{core::ptr::copy_nonoverlapping(name.as_ptr(),text,n);text.add(n).write(0);}
 let u=UnicodeString{length:(n*2) as u16,maximum_length:((n+1)*2) as u16,buffer:text};
 unsafe{core::ptr::write_unaligned(buffer.cast::<UnicodeString>(),u)}STATUS_SUCCESS
}
unsafe fn resolve_attributes_name(oa:*const ObjectAttributes,out:&mut [u16;MAX_NAME])->Result<usize,i32>{
 if oa.is_null(){return Err(STATUS_INVALID_PARAMETER)}let a=unsafe{&*oa};
 let mut leaf=[0u16;MAX_NAME];let n=unsafe{read_unicode(a.object_name,&mut leaf)}?;
 if n>0&&leaf[0]==b'\\' as u16 {out[..n].copy_from_slice(&leaf[..n]);return Ok(n)}
 if a.root_directory.is_null(){out[..n].copy_from_slice(&leaf[..n]);return Ok(n)}
 let dir_type=ensure_type(&[b'D' as u16,b'i' as u16,b'r' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'o' as u16,b'r' as u16,b'y' as u16],1,2,4,7,u32::MAX)?;
 let root=reference_by_handle(a.root_directory,0,dir_type,usize::MAX,true)?;
 let mut parent=[0u16;MAX_NAME];let pn=object_name(root,&mut parent)?;let _=dereference_body(root);
 if pn+n+1>MAX_NAME{return Err(STATUS_OBJECT_PATH_SYNTAX_BAD)}
 out[..pn].copy_from_slice(&parent[..pn]);let mut pos=pn;
 if pos==0||out[pos-1]!=b'\\' as u16{out[pos]=b'\\' as u16;pos+=1}
 out[pos..pos+n].copy_from_slice(&leaf[..n]);Ok(pos+n)
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwCreateDirectoryObject(out:*mut *mut c_void,desired_access:u32,oa:*const ObjectAttributes)->i32{
 if out.is_null(){return STATUS_INVALID_PARAMETER}unsafe{out.write(core::ptr::null_mut())}
 let mut name=[0u16;MAX_NAME];let n=match unsafe{resolve_attributes_name(oa,&mut name)}{Ok(n)=>n,Err(e)=>return e};
 let sd=if oa.is_null(){core::ptr::null()}else{unsafe{(*oa).security_descriptor.cast::<u8>()}};
 // The descriptor is accepted only when absent: its byte length is not carried by OBJECT_ATTRIBUTES.
 if !sd.is_null(){return STATUS_NOT_SUPPORTED}
 let body=match create_directory(&name[..n],core::ptr::null(),0){Ok(v)=>v,Err(e)=>return e};
 match open_by_pointer(body,desired_access,0,true,0){Ok(h)=>{unsafe{out.write(h)}STATUS_SUCCESS},Err(e)=>{let _=set_permanent(body,false);let _=dereference_body(body);e}}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwOpenDirectoryObject(out:*mut *mut c_void,desired_access:u32,oa:*const ObjectAttributes)->i32{
 if out.is_null(){return STATUS_INVALID_PARAMETER}unsafe{out.write(core::ptr::null_mut())}
 let mut name=[0u16;MAX_NAME];let n=match unsafe{resolve_attributes_name(oa,&mut name)}{Ok(n)=>n,Err(e)=>return e};
 match open_by_name(&name[..n],desired_access,0,true,0){Ok(h)=>{let dir=ensure_type(&[b'D' as u16,b'i' as u16,b'r' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'o' as u16,b'r' as u16,b'y' as u16],1,2,4,7,u32::MAX);if let Ok(ty)=dir{if let Ok(body)=reference_by_handle(h,0,ty,usize::MAX,true){let _=dereference_body(body);unsafe{out.write(h)}return STATUS_SUCCESS}}let _=close_handle(h,0);STATUS_OBJECT_TYPE_MISMATCH},Err(e)=>e}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwClose(handle:*mut c_void)->i32{close_handle(handle,usize::MAX).map_or_else(|e|e,|_|STATUS_SUCCESS)}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObQueryObjectAuditLog(out:*mut AuditEvent,capacity:u32,returned:*mut u32)->i32{
 if returned.is_null()||(capacity!=0&&out.is_null()){return STATUS_INVALID_PARAMETER}
 let mut events=[AuditEvent::EMPTY;MAX_AUDIT_EVENTS];let n=read_audit_events(&mut events[..(capacity as usize).min(MAX_AUDIT_EVENTS)]);
 if n!=0{unsafe{core::ptr::copy_nonoverlapping(events.as_ptr(),out,n)}}
 unsafe{returned.write(n as u32)}STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObMakeTemporaryObject(object:*mut c_void){make_temporary(object)}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ObIsKernelHandle(handle:*mut c_void)->u8{let _g=LOCK.lock();let s=state();let Some((i,g))=decode_handle(handle)else{return 0};s.handles.get(i).is_some_and(|e|e.used&&e.generation==g&&e.owner==usize::MAX) as u8}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn typed_names_handles_and_generation_rundown(){
  let ty=register_type(&[b'T' as u16,b'e' as u16,b's' as u16,b't' as u16],1,2,4,7,0xff).unwrap();
  let body=0x1234usize as *mut c_void;unsafe{register_object(body,ty,&[b'\\' as u16,b'D' as u16,b'e' as u16,b'v' as u16,b'i' as u16,b'c' as u16,b'e' as u16],false,core::ptr::null(),0).unwrap();}
  let h=open_by_name(&[b'\\' as u16,b'd' as u16,b'e' as u16,b'v' as u16,b'i' as u16,b'c' as u16,b'e' as u16],GENERIC_READ,0,true,0).unwrap();
  assert_eq!(reference_by_handle(h,1,ty,0,true).unwrap(),body);assert!(dereference_body(body));close_handle(h,0).unwrap();assert!(dereference_body(body));assert!(reference_body(body,ty,0,true).is_err());
 }
 #[test]fn self_relative_dacl_grants_only_explicit_everyone_mask(){
  let mut o=Object::EMPTY;o.used=true;o.sd_len=48;o.sd[0]=1;o.sd[2..4].copy_from_slice(&0x8004u16.to_le_bytes());o.sd[16..20].copy_from_slice(&20u32.to_le_bytes());
  o.sd[20]=2;o.sd[22..24].copy_from_slice(&28u16.to_le_bytes());o.sd[24..26].copy_from_slice(&1u16.to_le_bytes());
  o.sd[28]=0;o.sd[30..32].copy_from_slice(&20u16.to_le_bytes());o.sd[32..36].copy_from_slice(&1u32.to_le_bytes());o.sd[36]=1;o.sd[37]=1;o.sd[43]=1;
  let s=state();assert!(access_check(s,&o,1,0));assert!(!access_check(s,&o,2,0));
 }
 #[test]fn user_handles_are_owner_scoped(){
  let ty=register_type(&[90],1,2,4,7,7).unwrap();let body=0x3234usize as *mut c_void;let mut sd=[0u8;48];
  sd[0]=1;sd[2..4].copy_from_slice(&0x8004u16.to_le_bytes());sd[16..20].copy_from_slice(&20u32.to_le_bytes());
  sd[20]=2;sd[22..24].copy_from_slice(&28u16.to_le_bytes());sd[24..26].copy_from_slice(&1u16.to_le_bytes());
  sd[28]=0;sd[30..32].copy_from_slice(&20u16.to_le_bytes());sd[32..36].copy_from_slice(&1u32.to_le_bytes());sd[36]=1;sd[37]=1;sd[43]=1;
  unsafe{register_object(body,ty,&[91],false,sd.as_ptr(),sd.len()).unwrap();}
  let h=open_by_pointer(body,1,42,true,0).unwrap();assert_eq!(reference_by_handle(h,1,0,7,false).unwrap_err(),STATUS_INVALID_HANDLE);
  assert_eq!(reference_by_handle(h,1,0,42,false).unwrap(),body);assert!(dereference_body(body));close_handle(h,42).unwrap();assert!(dereference_body(body));
 }
 #[test]fn named_reference_is_case_insensitive_and_name_copy_is_stable(){
  let ty=register_type(&[93],1,2,4,7,7).unwrap();let body=0x4234usize as *mut c_void;
  unsafe{register_object(body,ty,&[92,93],false,core::ptr::null(),0).unwrap();}
  assert_eq!(reference_by_name(&[92,93],ty,1,true).unwrap(),body);assert!(dereference_body(body));
  let mut out=[0u16;MAX_NAME];assert_eq!(object_name(body,&mut out).unwrap(),2);assert_eq!(&out[..2],&[92,93]);assert!(dereference_body(body));
 }
 #[test]fn token_sid_dacl_grants_only_matching_owner(){
  let sid=[1u8,1,0,0,0,0,0,5,18,0,0,0];set_token_sids(777,&[&sid]).unwrap();
  let mut o=Object::EMPTY;o.used=true;o.sd_len=48;o.sd[0]=1;o.sd[2..4].copy_from_slice(&0x8004u16.to_le_bytes());o.sd[16..20].copy_from_slice(&20u32.to_le_bytes());
  o.sd[20]=2;o.sd[22..24].copy_from_slice(&28u16.to_le_bytes());o.sd[24..26].copy_from_slice(&1u16.to_le_bytes());
  o.sd[28]=0;o.sd[30..32].copy_from_slice(&20u16.to_le_bytes());o.sd[32..36].copy_from_slice(&1u32.to_le_bytes());o.sd[36..48].copy_from_slice(&sid);
  let s=state();assert!(access_check(s,&o,1,777));assert!(!access_check(s,&o,1,778));
 }
 #[test]fn created_type_pointer_is_stable_and_roundtrips(){
  let name=[b'O' as u16,b'b' as u16,b'T' as u16,b'y' as u16,b'p' as u16,b'e' as u16,b'X' as u16];
  let u=UnicodeString{length:(name.len()*2) as u16,maximum_length:((name.len()+1)*2) as u16,buffer:name.as_ptr()};
  let mut init=[0u8;0x78];init[..2].copy_from_slice(&0x78u16.to_le_bytes());init[8..12].copy_from_slice(&1u32.to_le_bytes());init[12..16].copy_from_slice(&2u32.to_le_bytes());init[16..20].copy_from_slice(&4u32.to_le_bytes());init[20..24].copy_from_slice(&7u32.to_le_bytes());init[24..28].copy_from_slice(&0xffu32.to_le_bytes());
  let mut out=core::ptr::null_mut();assert_eq!(unsafe{ObCreateObjectType(&u,init.as_ptr(),core::ptr::null_mut(),&mut out)},STATUS_SUCCESS);assert!(!out.is_null());assert!(type_id_from_pointer(out).is_ok());
 }
 #[test]fn directory_handles_resolve_relative_names(){
  let root_name=[b'\\' as u16,b'O' as u16,b'b' as u16,b'j' as u16,b'T' as u16,b'e' as u16,b's' as u16,b't' as u16,b'D' as u16,b'i' as u16,b'r' as u16];
  let root_u=UnicodeString{length:(root_name.len()*2) as u16,maximum_length:((root_name.len()+1)*2) as u16,buffer:root_name.as_ptr()};
  let oa=ObjectAttributes{length:core::mem::size_of::<ObjectAttributes>() as u32,root_directory:core::ptr::null_mut(),object_name:&root_u,attributes:0,security_descriptor:core::ptr::null_mut(),security_qos:core::ptr::null_mut()};
  let mut root=core::ptr::null_mut();assert_eq!(unsafe{ZwCreateDirectoryObject(&mut root,7,&oa)},STATUS_SUCCESS);
  let child_name=[b'C' as u16,b'h' as u16,b'i' as u16,b'l' as u16,b'd' as u16];
  let child_u=UnicodeString{length:(child_name.len()*2) as u16,maximum_length:((child_name.len()+1)*2) as u16,buffer:child_name.as_ptr()};
  let child_oa=ObjectAttributes{root_directory:root,object_name:&child_u,..oa};
  let mut child=core::ptr::null_mut();assert_eq!(unsafe{ZwCreateDirectoryObject(&mut child,7,&child_oa)},STATUS_SUCCESS);
  let mut reopened=core::ptr::null_mut();assert_eq!(unsafe{ZwOpenDirectoryObject(&mut reopened,1,&child_oa)},STATUS_SUCCESS);
  assert_eq!(unsafe{ZwClose(reopened)},STATUS_SUCCESS);assert_eq!(unsafe{ZwClose(child)},STATUS_SUCCESS);assert_eq!(unsafe{ZwClose(root)},STATUS_SUCCESS);
 }
 #[test]fn access_masks_and_deny_by_default(){let t=TypeEntry{used:true,id:1,gr:1,gw:2,gx:4,ga:7,valid:7,..TypeEntry::EMPTY};assert_eq!(map_access(&t,GENERIC_READ|GENERIC_WRITE),3);assert!(!access_check(state(),&Object::EMPTY,1,0));}
 #[test]fn stale_handle_cannot_alias_reused_slot(){let ty=register_type(&[88],1,2,4,7,7).unwrap();let body=0x2234usize as *mut c_void;unsafe{register_object(body,ty,&[89],false,core::ptr::null(),0).unwrap();}let h=open_by_pointer(body,1,0,true,0).unwrap();close_handle(h,0).unwrap();let h2=open_by_pointer(body,1,0,true,0).unwrap();assert_ne!(h,h2);assert_eq!(reference_by_handle(h,1,0,0,true).unwrap_err(),STATUS_INVALID_HANDLE);}
}

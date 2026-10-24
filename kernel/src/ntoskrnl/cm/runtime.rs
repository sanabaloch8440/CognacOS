//! Bounded in-memory Configuration Manager registry core.
//! This is an initial volatile registry, not a hive/file-backed implementation.
use core::sync::atomic::{AtomicBool, Ordering};

pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
pub const STATUS_OBJECT_NAME_NOT_FOUND: i32 = 0xC000_0034u32 as i32;
pub const STATUS_OBJECT_NAME_COLLISION: i32 = 0xC000_0035u32 as i32;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
pub const STATUS_ACCESS_DENIED: i32 = 0xC000_0022u32 as i32;
pub const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009Au32 as i32;
pub const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;

const MAX_KEYS: usize = 64;
const MAX_VALUES: usize = 128;
const MAX_NAME: usize = 96;
const MAX_DATA: usize = 256;
const HANDLE_BASE: usize = 0x5244_0000;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnicodeString { pub length: u16, pub maximum_length: u16, pub buffer: *const u16 }
#[repr(C)]
pub struct ObjectAttributes {
    pub length: u32, pub root_directory: *mut core::ffi::c_void,
    pub object_name: *const UnicodeString, pub attributes: u32,
    pub security_descriptor: *mut core::ffi::c_void,
    pub security_qos: *mut core::ffi::c_void,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IoStatusBlock { pub status_or_pointer: usize, pub information: usize }

#[derive(Clone, Copy)]
struct Key { used: bool, id: u32, parent: u32, name: [u16; MAX_NAME], name_len: u16, deleted: bool }
impl Key { const EMPTY: Self = Self { used:false,id:0,parent:0,name:[0;MAX_NAME],name_len:0,deleted:false }; }
#[derive(Clone, Copy)]
struct Value { used: bool, key_id: u32, name: [u16;MAX_NAME], name_len:u16, kind:u32, len:u16, data:[u8;MAX_DATA] }
impl Value { const EMPTY: Self = Self { used:false,key_id:0,name:[0;MAX_NAME],name_len:0,kind:0,len:0,data:[0;MAX_DATA] }; }
struct SpinLock(AtomicBool);
impl SpinLock {
 const fn new()->Self{Self(AtomicBool::new(false))}
 fn lock(&self)->Guard<'_>{while self.0.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}Guard(&self.0)}
}
struct Guard<'a>(&'a AtomicBool);
impl Drop for Guard<'_>{fn drop(&mut self){self.0.store(false,Ordering::Release);}}
struct State { keys:[Key;MAX_KEYS], values:[Value;MAX_VALUES], next_id:u32 }
impl State { const fn new()->Self{Self{keys:[Key::EMPTY;MAX_KEYS],values:[Value::EMPTY;MAX_VALUES],next_id:1}} }
static LOCK:SpinLock=SpinLock::new();
static mut STATE:State=State::new();

fn state()->&'static mut State { unsafe { &mut *core::ptr::addr_of_mut!(STATE) } }
unsafe fn read_name(s:*const UnicodeString,out:&mut [u16;MAX_NAME])->Result<usize,i32>{
 if s.is_null(){return Err(STATUS_INVALID_PARAMETER);}
 let u=unsafe{&*s}; if u.length%2!=0||u.length>u.maximum_length||u.length as usize/2>MAX_NAME|| (u.length!=0&&u.buffer.is_null()){return Err(STATUS_INVALID_PARAMETER);}
 let n=u.length as usize/2;
 for i in 0..n { out[i]=unsafe{core::ptr::read_unaligned(u.buffer.add(i))}; }
 if out[..n].iter().any(|c|*c==0){return Err(STATUS_INVALID_PARAMETER);}
 Ok(n)
}
fn eq(a:&[u16],b:&[u16])->bool{a.len()==b.len()&&a.iter().zip(b).all(|(x,y)|fold(*x)==fold(*y))}
fn fold(c:u16)->u16{if (b'A' as u16..=b'Z' as u16).contains(&c){c+32}else{c}}
fn key_path(s:&State,id:u32,out:&mut [u16;MAX_NAME])->Option<usize>{
 let mut chain=[0u32;MAX_KEYS];let mut count=0usize;let mut current=id;
 while current!=0 {
  let k=s.keys.iter().find(|k|k.used&&!k.deleted&&k.id==current)?;
  if count==MAX_KEYS{return None}chain[count]=current;count+=1;current=k.parent;
 }
 let prefix=[b'\\' as u16,b'R' as u16,b'e' as u16,b'g' as u16,b'i' as u16,b's' as u16,b't' as u16,b'r' as u16,b'y' as u16];
 let mut n=0usize;out[..prefix.len()].copy_from_slice(&prefix);n+=prefix.len();
 for idx in (0..count).rev(){
  let k=s.keys.iter().find(|k|k.used&&!k.deleted&&k.id==chain[idx])?;
  if n>=MAX_NAME{return None}out[n]=b'\\' as u16;n+=1;
  let len=k.name_len as usize;if n+len>MAX_NAME{return None}
  out[n..n+len].copy_from_slice(&k.name[..len]);n+=len;
 }
 Some(n)
}
fn key_from_handle(s:&State,h:*mut core::ffi::c_void)->Option<u32>{
 #[cfg(test)] {let v=h as usize;if v<HANDLE_BASE{return None;}let id=(v-HANDLE_BASE) as u32;return s.keys.iter().find(|k|k.used&&!k.deleted&&k.id==id).map(|k|k.id);}
 #[cfg(not(test))] {
  let ty=crate::ntoskrnl::ob::runtime::ensure_type(&[b'R' as u16,b'e' as u16,b'g' as u16,b'i' as u16,b's' as u16,b't' as u16,b'r' as u16,b'y' as u16,b'K' as u16,b'e' as u16,b'y' as u16],1,2,4,7,u32::MAX).ok()?;
  let body=crate::ntoskrnl::ob::runtime::reference_by_handle(h,0,ty,crate::ntoskrnl::ob::runtime::current_owner(),true).ok()?;
  let result=s.keys.iter().find(|k|k.used&&!k.deleted&&core::ptr::addr_of!(**k) as *mut Key as *mut core::ffi::c_void==body).map(|k|k.id);
  let _=crate::ntoskrnl::ob::runtime::dereference_body(body);result
 }
}
fn handle(id:u32)->*mut core::ffi::c_void{(HANDLE_BASE+id as usize) as *mut core::ffi::c_void}
unsafe fn object_path(oa:*const ObjectAttributes)->Result<([u16;MAX_NAME],usize,u32),i32>{
 if oa.is_null(){return Err(STATUS_INVALID_PARAMETER);}
 let o=unsafe{&*oa};if (o.length as usize)<core::mem::size_of::<ObjectAttributes>()||o.object_name.is_null(){return Err(STATUS_INVALID_PARAMETER);}
 let mut name=[0u16;MAX_NAME];let n=unsafe{read_name(o.object_name,&mut name)}?;
 let s=state();let parent=if o.root_directory.is_null(){0}else{key_from_handle(s,o.root_directory).ok_or(STATUS_INVALID_PARAMETER)?};
 Ok((name,n,parent))
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwCreateKey(out:*mut *mut core::ffi::c_void,desired_access:u32,oa:*const ObjectAttributes,_title_index:u32,_class:*const UnicodeString,options:u32,disposition:*mut u32)->i32{
 if out.is_null(){return STATUS_INVALID_PARAMETER;}if options&!0x0000_0002!=0{return STATUS_INVALID_PARAMETER;}
 let _g=LOCK.lock();
 let (name,n,parent)=match unsafe{object_path(oa)}{Ok(v)=>v,Err(e)=>return e};
 let s=state();
 if let Some(k)=s.keys.iter().find(|k|k.used&&!k.deleted&&k.parent==parent&&eq(&k.name[..k.name_len as usize],&name[..n])){unsafe{*out=handle(k.id);if !disposition.is_null(){*disposition=2;}}return STATUS_SUCCESS;}
 let Some(index)=s.keys.iter().position(|k|!k.used||k.deleted)else{return STATUS_INSUFFICIENT_RESOURCES;};
 let id=s.next_id;s.next_id=s.next_id.wrapping_add(1).max(1);
 s.keys[index]=Key{used:true,id,parent,name,name_len:n as u16,deleted:false};
 let mut object_name=[0u16;MAX_NAME];let object_name_len=match key_path(s,id,&mut object_name){Some(v)=>v,None=>{s.keys[index]=Key::EMPTY;return STATUS_INVALID_PARAMETER;}};
 let object_type=[b'R' as u16,b'e' as u16,b'g' as u16,b'i' as u16,b's' as u16,b't' as u16,b'r' as u16,b'y' as u16,b'K' as u16,b'e' as u16,b'y' as u16];
 let body=core::ptr::addr_of_mut!(s.keys[index]).cast::<core::ffi::c_void>();
 #[cfg(not(test))] {
  if let Err(status)=unsafe{crate::ntoskrnl::ob::runtime::register_external_object(body,&object_type,&object_name[..object_name_len],false,core::ptr::null(),0)}{s.keys[index]=Key::EMPTY;return status;}
  match crate::ntoskrnl::ob::runtime::open_by_pointer(body,desired_access,crate::ntoskrnl::ob::runtime::current_owner(),true,0){
   Ok(h)=>unsafe{*out=h},Err(status)=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);s.keys[index]=Key::EMPTY;return status;}
  }
 }
 #[cfg(test)] unsafe{*out=handle(id);}
 if !disposition.is_null(){unsafe{*disposition=1;}}
 let _=_title_index; STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwOpenKey(out:*mut *mut core::ffi::c_void,access:u32,oa:*const ObjectAttributes)->i32{
 if out.is_null(){return STATUS_INVALID_PARAMETER;}let _g=LOCK.lock();
 let (name,n,parent)=match unsafe{object_path(oa)}{Ok(v)=>v,Err(e)=>return e};
 let s=state();
 let found=s.keys.iter().find(|k|k.used&&!k.deleted&&k.parent==parent&&eq(&k.name[..k.name_len as usize],&name[..n]));
 if let Some(k)=found{
  #[cfg(test)] unsafe{*out=handle(k.id);}
  #[cfg(not(test))] {let body=core::ptr::addr_of!(**k) as *mut Key as *mut core::ffi::c_void;match crate::ntoskrnl::ob::runtime::open_by_pointer(body,access,crate::ntoskrnl::ob::runtime::current_owner(),true,0){Ok(h)=>unsafe{*out=h},Err(e)=>return e}}
  STATUS_SUCCESS
 }else{STATUS_OBJECT_NAME_NOT_FOUND}
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwSetValueKey(h:*mut core::ffi::c_void,name:*const UnicodeString,title:u32,kind:u32,data:*const core::ffi::c_void,len:u32)->i32{
 let mut nbuf=[0u16;MAX_NAME];let n=match unsafe{read_name(name,&mut nbuf)}{Ok(n)=>n,Err(e)=>return e};
 if len as usize>MAX_DATA||(len!=0&&data.is_null()){return STATUS_INVALID_PARAMETER;}
 let _g=LOCK.lock();let s=state();let Some(id)=key_from_handle(s,h)else{return STATUS_INVALID_PARAMETER;};
 let existing=s.values.iter().position(|v|v.used&&v.key_id==id&&eq(&v.name[..v.name_len as usize],&nbuf[..n]));
 let idx=match existing{Some(i)=>i,None=>match s.values.iter().position(|v|!v.used){Some(i)=>i,None=>return STATUS_INSUFFICIENT_RESOURCES}};
 let mut v=Value{used:true,key_id:id,name:nbuf,name_len:n as u16,kind,len:len as u16,data:[0;MAX_DATA]};
 if len!=0{unsafe{core::ptr::copy_nonoverlapping(data.cast::<u8>(),v.data.as_mut_ptr(),len as usize);}}
 s.values[idx]=v;let _=title;STATUS_SUCCESS
}
#[repr(C)]
pub struct KeyValuePartialInformation { pub title_index:u32,pub kind:u32,pub data_length:u32,pub data:[u8;1] }
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwQueryValueKey(h:*mut core::ffi::c_void,name:*const UnicodeString,class:u32,out:*mut core::ffi::c_void,out_len:u32,result_len:*mut u32)->i32{
 if class!=2||out.is_null(){return STATUS_NOT_SUPPORTED;}let mut nbuf=[0u16;MAX_NAME];let n=match unsafe{read_name(name,&mut nbuf)}{Ok(n)=>n,Err(e)=>return e};
 let _g=LOCK.lock();let s=state();let Some(id)=key_from_handle(s,h)else{return STATUS_INVALID_PARAMETER;};
 let Some(v)=s.values.iter().find(|v|v.used&&v.key_id==id&&eq(&v.name[..v.name_len as usize],&nbuf[..n]))else{return STATUS_OBJECT_NAME_NOT_FOUND;};
 let need=core::mem::offset_of!(KeyValuePartialInformation,data)+v.len as usize;
 if !result_len.is_null(){unsafe{*result_len=need as u32;}}
 if (out_len as usize)<need{return STATUS_BUFFER_TOO_SMALL;}
 unsafe{let p=out.cast::<KeyValuePartialInformation>();(*p).title_index=0;(*p).kind=v.kind;(*p).data_length=v.len as u32;core::ptr::copy_nonoverlapping(v.data.as_ptr(),(*p).data.as_mut_ptr(),v.len as usize);}
 STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwDeleteValueKey(h:*mut core::ffi::c_void,name:*const UnicodeString)->i32{
 let mut nb=[0u16;MAX_NAME];let n=match unsafe{read_name(name,&mut nb)}{Ok(n)=>n,Err(e)=>return e};
 let _g=LOCK.lock();let s=state();let Some(id)=key_from_handle(s,h)else{return STATUS_INVALID_PARAMETER;};
 let Some(v)=s.values.iter_mut().find(|v|v.used&&v.key_id==id&&eq(&v.name[..v.name_len as usize],&nb[..n]))else{return STATUS_OBJECT_NAME_NOT_FOUND;};
 *v=Value::EMPTY;STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwDeleteKey(h:*mut core::ffi::c_void)->i32{
 let _g=LOCK.lock();let s=state();let Some(id)=key_from_handle(s,h)else{return STATUS_INVALID_PARAMETER;};
 if s.keys.iter().any(|k|k.used&&!k.deleted&&k.parent==id)||s.values.iter().any(|v|v.used&&v.key_id==id){return STATUS_ACCESS_DENIED;}
 let Some(index)=s.keys.iter().position(|k|k.used&&k.id==id)else{return STATUS_OBJECT_NAME_NOT_FOUND;};
 let body=core::ptr::addr_of_mut!(s.keys[index]).cast::<core::ffi::c_void>();s.keys[index].deleted=true;
 #[cfg(not(test))] { let _=crate::ntoskrnl::ob::runtime::unregister_object(body); } let _=body;STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ZwFlushKey(h:*mut core::ffi::c_void)->i32{
 let _g=LOCK.lock();let s=state();if key_from_handle(s,h).is_some(){STATUS_SUCCESS}else{STATUS_INVALID_PARAMETER}
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn name_comparison_is_case_insensitive(){assert!(eq(&[b'S' as u16,b'y' as u16],[b's' as u16,b'Y' as u16].as_slice()));}
 #[test] fn status_values_match_ntstatus(){assert_eq!(STATUS_SUCCESS,0);assert_eq!(STATUS_OBJECT_NAME_NOT_FOUND,0xC0000034u32 as i32);}
 #[test] fn create_set_and_query_value_round_trip(){
  let key_name:[u16;19]=[82,101,103,105,115,116,114,121,82,117,110,116,105,109,101,84,101,115,116];
  let key_us=UnicodeString{length:(key_name.len()*2) as u16,maximum_length:(key_name.len()*2) as u16,buffer:key_name.as_ptr()};
  let oa=ObjectAttributes{length:core::mem::size_of::<ObjectAttributes>() as u32,root_directory:core::ptr::null_mut(),object_name:&key_us,attributes:0,security_descriptor:core::ptr::null_mut(),security_qos:core::ptr::null_mut()};
  let mut key=core::ptr::null_mut();let mut disposition=0;
  assert_eq!(unsafe{ZwCreateKey(&mut key,0,&oa,0,core::ptr::null(),0,&mut disposition)},STATUS_SUCCESS);
  assert_eq!(disposition,1);
  let value_name:[u16;4]=[68,119,111,114];let value_us=UnicodeString{length:8,maximum_length:8,buffer:value_name.as_ptr()};
  let value:u32=0x1234_abcd;
  assert_eq!(unsafe{ZwSetValueKey(key,&value_us,0,4,(&value as *const u32).cast(),4)},STATUS_SUCCESS);
  let mut output=[0u32;16];let mut required=0;
  assert_eq!(unsafe{ZwQueryValueKey(key,&value_us,2,output.as_mut_ptr().cast(),core::mem::size_of_val(&output) as u32,&mut required)},STATUS_SUCCESS);
  assert_eq!(required,16);
  assert_eq!(output[1],4);
  assert_eq!(output[2],4);
  assert_eq!(output[3],value);
 }
}

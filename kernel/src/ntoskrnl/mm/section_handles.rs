//! Object-Manager-backed NT Section handles layered over the MM section store.
//! The opaque MM section ID never crosses the user ABI: callers receive a typed
//! Object Manager handle, and ALPC takes an independent MM reference when it
//! imports a caller-supplied Section handle.
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool,AtomicU32,AtomicU64,Ordering};

const STATUS_SUCCESS:u64=0;
const STATUS_INVALID_PARAMETER:u64=0xC000_000D;
const STATUS_ACCESS_VIOLATION:u64=0xC000_0005;
const STATUS_INVALID_HANDLE:u64=0xC000_0008;
const STATUS_INSUFFICIENT_RESOURCES:u64=0xC000_009A;
const ACCESS_READ:u32=1;
const ACCESS_WRITE:u32=2;
const ACCESS_CONTROL:u32=4;
const MAX_SECTION_HANDLES:usize=64;
static SECTION_TYPE:AtomicU32=AtomicU32::new(0);
static DEFAULT_SD:[u8;48]=[
    1,0,4,128,0,0,0,0,0,0,0,0,0,0,0,0,20,0,0,0,
    2,0,28,0,1,0,0,0,0,0,20,0,7,0,0,0,1,1,0,0,0,0,0,1,0,0,0,0
];
#[repr(C)]
struct SectionObject {
    section_id:AtomicU64,
    owner_root:AtomicU64,
    size:AtomicU64,
    owner_pid:AtomicU64,
    references:AtomicU32,
    protection:AtomicU32,
    active:AtomicBool,
    allocated:AtomicBool,
}
impl SectionObject {
    const fn new()->Self { Self{section_id:AtomicU64::new(0),owner_root:AtomicU64::new(0),size:AtomicU64::new(0),owner_pid:AtomicU64::new(0),references:AtomicU32::new(0),protection:AtomicU32::new(4),active:AtomicBool::new(false),allocated:AtomicBool::new(false)} }
}
static OBJECTS:[SectionObject;MAX_SECTION_HANDLES]=[const{SectionObject::new()};MAX_SECTION_HANDLES];

fn section_type()->Result<u16,u64>{
    let old=SECTION_TYPE.load(Ordering::Acquire);
    if old!=0{return Ok(old as u16)}
    let name=[b'S' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'i' as u16,b'o' as u16,b'n' as u16];
    match crate::ntoskrnl::ob::runtime::ensure_type(&name,ACCESS_READ,ACCESS_WRITE,0,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL){
        Ok(id)=>{SECTION_TYPE.store(id as u32,Ordering::Release);Ok(id)}
        Err(_)=>Err(STATUS_INSUFFICIENT_RESOURCES)
    }
}
unsafe extern "win64" fn finalizer(body:*mut c_void){
    if body.is_null(){return}
    let s=unsafe{&*(body as *const SectionObject)};
    if s.active.swap(false,Ordering::AcqRel){
        let id=s.section_id.load(Ordering::Acquire);
        let root=s.owner_root.load(Ordering::Acquire);
        let refs=s.references.swap(0,Ordering::AcqRel);
        for _ in 0..refs { let _=crate::ntoskrnl::mm::section::dereference(id,root); }
    }
    s.allocated.store(false,Ordering::Release);
}
fn resolve(handle:u64,access:u32)->Result<(&'static SectionObject,*mut c_void),u64>{
    let ty=section_type()?;
    let body=crate::ntoskrnl::ob::runtime::reference_by_handle(handle as *mut c_void,access,ty,crate::ntoskrnl::ob::runtime::current_owner(),false).map_err(|e|e as u32 as u64)?;
    let ptr=body as *const SectionObject;
    if !OBJECTS.iter().any(|s|core::ptr::eq(s as *const SectionObject,ptr)){
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(STATUS_INVALID_HANDLE)
    }
    let s=unsafe{&*ptr};
    if !s.active.load(Ordering::Acquire){let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(STATUS_INVALID_HANDLE)}
    Ok((s,body))
}

/// Kernel half of the private create-section service. Returns a typed handle.
pub fn create(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{ create_with_protection_and_access(args,4,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL) }
fn create_with_protection(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS],protection:u32)->u64{ create_with_protection_and_access(args,protection,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL) }
fn create_with_protection_and_access(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS],protection:u32,granted_access:u32)->u64{
    if args[1]==0||args[0]==0{return STATUS_INVALID_PARAMETER}
    let process=crate::process::eprocess_pointer_for_pid(crate::syscall::current_process_id());
    if process.is_null(){return STATUS_INVALID_HANDLE}
    let root=unsafe{(*process).address_space_root};
    if root==0{return STATUS_INVALID_HANDLE}
    if let Err(s)=section_type(){return s}
    let mut id=0u64;
    let status=crate::ntoskrnl::mm::section::create(args[0],root,&mut id);
    if status<0{return status as u32 as u64}
    let Some(slot)=OBJECTS.iter().position(|s|!s.allocated.load(Ordering::Acquire)) else{
        let _=crate::ntoskrnl::mm::section::dereference(id,root);return STATUS_INSUFFICIENT_RESOURCES
    };
    let s=&OBJECTS[slot];
    if s.allocated.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){
        let _=crate::ntoskrnl::mm::section::dereference(id,root);return STATUS_INSUFFICIENT_RESOURCES
    }
    let rounded=match args[0].checked_add(4095){Some(v)=>v&!4095,None=>{s.allocated.store(false,Ordering::Release);let _=crate::ntoskrnl::mm::section::dereference(id,root);return STATUS_INVALID_PARAMETER}};
    s.section_id.store(id,Ordering::Release);s.owner_root.store(root,Ordering::Release);s.size.store(rounded,Ordering::Release);s.owner_pid.store(crate::syscall::current_process_id(),Ordering::Release);s.references.store(1,Ordering::Release);s.protection.store(protection,Ordering::Release);s.active.store(true,Ordering::Release);
    let name=[0u16;0];let type_name=[b'S' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'i' as u16,b'o' as u16,b'n' as u16];
    let body=s as *const SectionObject as *mut c_void;
    if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(body,&type_name,&name,false,DEFAULT_SD.as_ptr(),DEFAULT_SD.len(),finalizer)}.is_err(){
        s.active.store(false,Ordering::Release);s.allocated.store(false,Ordering::Release);let _=crate::ntoskrnl::mm::section::dereference(id,root);return 0xC000_0035
    }
    let handle=match crate::ntoskrnl::ob::runtime::open_by_pointer(body,granted_access,crate::ntoskrnl::ob::runtime::current_owner(),false,0){
        Ok(h)=>h as usize,Err(e)=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);return e as u32 as u64}
    };
    if !crate::syscall::copy_current_user_to(args[1],&handle.to_ne_bytes()){
        let _=crate::ntoskrnl::ob::runtime::close_handle(handle as *mut c_void,crate::ntoskrnl::ob::runtime::current_owner());return STATUS_ACCESS_VIOLATION
    }
    STATUS_SUCCESS
}
pub fn reference(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (s,body)=match resolve(args[0],ACCESS_READ){Ok(v)=>v,Err(e)=>return e};
    let old=s.references.fetch_add(1,Ordering::AcqRel);
    if old==u32::MAX{s.references.fetch_sub(1,Ordering::AcqRel);let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_INSUFFICIENT_RESOURCES}
    let status=crate::ntoskrnl::mm::section::reference(s.section_id.load(Ordering::Acquire),s.owner_root.load(Ordering::Acquire)) as u32 as u64;
    if status!=STATUS_SUCCESS{s.references.fetch_sub(1,Ordering::AcqRel)}
    let _=crate::ntoskrnl::ob::runtime::dereference_body(body);status
}
pub fn dereference(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (s,body)=match resolve(args[0],ACCESS_READ){Ok(v)=>v,Err(e)=>return e};
    let old=s.references.load(Ordering::Acquire);
    if old==0{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_INVALID_PARAMETER}
    let id=s.section_id.load(Ordering::Acquire);let root=s.owner_root.load(Ordering::Acquire);
    let status=crate::ntoskrnl::mm::section::dereference(id,root) as u32 as u64;
    if status==STATUS_SUCCESS{
        let left=s.references.fetch_sub(1,Ordering::AcqRel)-1;
        if left==0{
            s.active.store(false,Ordering::Release);
            let owner=crate::ntoskrnl::ob::runtime::current_owner();
            let _=crate::ntoskrnl::ob::runtime::unregister_object(body);
            let _=crate::ntoskrnl::ob::runtime::close_handle(args[0] as *mut c_void,owner);
        }
    }
    let _=crate::ntoskrnl::ob::runtime::dereference_body(body);status
}
pub fn copy(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (s,body)=match resolve(args[0],if args[4]==1{ACCESS_WRITE}else{ACCESS_READ}){Ok(v)=>v,Err(e)=>return e};
    let root=s.owner_root.load(Ordering::Acquire);
    let length=args[3] as usize;
    if length>16*1024*1024||args[4]>1{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_INVALID_PARAMETER}
    let mut done=0usize;let mut scratch=[0u8;4096];
    while done<length{
        let n=(length-done).min(scratch.len());
        let Some(offset)=args[1].checked_add(done as u64) else{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_INVALID_PARAMETER};
        let Some(user)=args[2].checked_add(done as u64) else{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_ACCESS_VIOLATION};
        if args[4]==1&&!crate::syscall::copy_current_user_from(user,&mut scratch[..n]){let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_ACCESS_VIOLATION}
        let status=crate::ntoskrnl::mm::section::copy(s.section_id.load(Ordering::Acquire),root,offset,&mut scratch[..n],args[4]==1);
        if status<0{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return status as u32 as u64}
        if args[4]==0&&!crate::syscall::copy_current_user_to(user,&scratch[..n]){let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_ACCESS_VIOLATION}
        done+=n;
    }
    let _=crate::ntoskrnl::ob::runtime::dereference_body(body);STATUS_SUCCESS
}
/// Close the backing references of Section objects owned by a terminating process.
pub fn rundown_process(pid:u64){
    if pid==0{return}
    for s in OBJECTS.iter(){
        if s.allocated.load(Ordering::Acquire)&&s.owner_pid.load(Ordering::Acquire)==pid{
            if s.active.swap(false,Ordering::AcqRel){
                let id=s.section_id.load(Ordering::Acquire);let root=s.owner_root.load(Ordering::Acquire);
                let refs=s.references.swap(0,Ordering::AcqRel);
                for _ in 0..refs{let _=crate::ntoskrnl::mm::section::dereference(id,root);}
            }
            let _=crate::ntoskrnl::ob::runtime::unregister_object(s as *const SectionObject as *mut c_void);
        }
    }
}
/// Windows NtCreateSection ABI bridge. Supports committed anonymous sections;
/// file-backed sections and named OBJECT_ATTRIBUTES are rejected explicitly.
pub fn create_nt(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    if args[0]==0||args[3]==0{return STATUS_INVALID_PARAMETER}
    if args[2]!=0||args[6]!=0{return 0xC000_0002}
    let desired=args[1] as u32;
    let mut granted=0u32;
    if desired==0x0200_0000 {granted=ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL;} else {
        if desired&(0x0000_0001|0x0000_0004|0x0000_0008|0x0002_0000)!=0 {granted|=ACCESS_READ;}
        if desired&(0x0000_0002|0x0000_0010)!=0 {granted|=ACCESS_WRITE;}
        if desired&(0x0001_0000|0x0004_0000|0x0008_0000)!=0 {granted|=ACCESS_CONTROL;}
    }
    if granted==0{return STATUS_INVALID_PARAMETER}
    let mut raw=[0u8;8];
    if !crate::syscall::copy_current_user_from(args[3],&mut raw){return STATUS_ACCESS_VIOLATION}
    let size=i64::from_ne_bytes(raw);
    if size<=0{return STATUS_INVALID_PARAMETER}
    if args[5]!=0x0800_0000{return STATUS_INVALID_PARAMETER}
    let protection=args[4] as u32;
    if !matches!(protection,0x02|0x04|0x20){return STATUS_INVALID_PARAMETER}
    let mut local=[0u64;crate::syscall::NT_MAX_ARGUMENTS];
    local[0]=size as u64;local[1]=args[0];
    create_with_protection_and_access(&local,protection,granted)
}

/// Import a caller-supplied Object Manager Section handle for an ALPC port.
/// Takes an independent MM reference so closing the original handle is safe.
pub fn reference_for_alpc(handle:u64)->Result<(u64,u64,u64,u32),u64>{
    let (s,body)=resolve(handle,ACCESS_READ)?;
    let id=s.section_id.load(Ordering::Acquire);let root=s.owner_root.load(Ordering::Acquire);let size=s.size.load(Ordering::Acquire);let protection=s.protection.load(Ordering::Acquire);
    let status=crate::ntoskrnl::mm::section::reference(id,root);
    let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
    if status<0{return Err(status as u32 as u64)}
    Ok((id,root,size,protection))
}

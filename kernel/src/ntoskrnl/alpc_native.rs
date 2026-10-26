//! Native ALPC syscall adapter. Port handles are real Object Manager handles
//! with a typed ALPC Port body. Operations that need a VM section mapping or a
//! captured security token fail closed until those subsystems expose the required
//! primitives; metadata alone is never presented as a mapped view/token.
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

const STATUS_SUCCESS:u64=0;
const STATUS_NOT_IMPLEMENTED:u64=0xC000_0002;
const STATUS_INVALID_PARAMETER:u64=0xC000_000D;
const STATUS_ACCESS_VIOLATION:u64=0xC000_0005;
const STATUS_INSUFFICIENT_RESOURCES:u64=0xC000_009A;
const STATUS_OBJECT_TYPE_MISMATCH:u64=0xC000_0024;
const ACCESS_READ:u32=1;
const ACCESS_WRITE:u32=2;
const ACCESS_CONTROL:u32=4;
const MAX_PORT_OBJECTS:usize=32;
const MAX_NAME:usize=48;
/// Minimal self-relative security descriptor with an Everyone allow ACE for
/// the ALPC-specific access bits. Handle granted-access masks are still checked
/// on every reference; caller-supplied security descriptors are not yet captured.
static ALPC_DEFAULT_SD:[u8;48]=[
    1,0, 4,128, 0,0,0,0, 0,0,0,0, 0,0,0,0, 20,0,0,0,
    2,0, 28,0, 1,0, 0,0,
    0,0, 20,0, 7,0,0,0,
    1,1,0,0,0,0,0,1, 0,0,0,0
];

#[repr(C)]
#[derive(Clone,Copy)]
struct UnicodeString { length:u16, maximum_length:u16, buffer:*const u16 }
#[repr(C)]
#[derive(Clone,Copy)]
struct ObjectAttributes { length:u32, root_directory:usize, object_name:*const UnicodeString, attributes:u32, security_descriptor:usize, security_qos:usize }
#[repr(C)]
#[derive(Clone,Copy)]
struct PortAttributes { flags:u32, qos:[u8;12], max_message_length:usize, _rest:[usize;5], dup_object_types:u32, reserved:u32 }

#[repr(C)]
struct PortObject { port_id:AtomicU32, owner_pid:AtomicU64, active:AtomicBool, allocated:AtomicBool }
impl PortObject { const fn new()->Self { Self { port_id:AtomicU32::new(0), owner_pid:AtomicU64::new(0), active:AtomicBool::new(false), allocated:AtomicBool::new(false) } } }
static PORT_OBJECTS:[PortObject;MAX_PORT_OBJECTS]=[const { PortObject::new() };MAX_PORT_OBJECTS];
static ALPC_PORT_TYPE:AtomicU32=AtomicU32::new(0);
static ALPC_SECTION_TYPE:AtomicU32=AtomicU32::new(0);
static ALPC_ENDPOINT_TYPE:AtomicU32=AtomicU32::new(0);
static ALPC_SECURITY_CONTEXT_TYPE:AtomicU32=AtomicU32::new(0);
#[repr(C)]
struct SecurityContextObject { context_id:AtomicU32, owner_pid:AtomicU64, port_id:AtomicU32, active:AtomicBool, allocated:AtomicBool }
impl SecurityContextObject { const fn new()->Self { Self { context_id:AtomicU32::new(0), owner_pid:AtomicU64::new(0), port_id:AtomicU32::new(0), active:AtomicBool::new(false), allocated:AtomicBool::new(false) } } }
static SECURITY_CONTEXT_OBJECTS:[SecurityContextObject;MAX_PORT_OBJECTS]=[const { SecurityContextObject::new() };MAX_PORT_OBJECTS];
#[repr(C,align(16))]
struct CapturedTokenStorage { words:[u64;8], allocated:AtomicBool }
impl CapturedTokenStorage { const fn new()->Self { Self { words:[0;8], allocated:AtomicBool::new(false) } } }
static CAPTURED_TOKENS:[CapturedTokenStorage;MAX_PORT_OBJECTS]=[const { CapturedTokenStorage::new() };MAX_PORT_OBJECTS];
#[repr(C)]
#[derive(Clone,Copy)]
struct SecurityQualityOfService { length:u32, impersonation_level:u32, context_tracking_mode:u8, effective_only:u8, _padding:[u8;2] }
#[repr(C)]
#[derive(Clone,Copy)]
struct SecurityAttribute { flags:u32, _padding:u32, qos:usize, context_handle:usize }

#[repr(C)]
struct EndpointObject { endpoint_id:AtomicU32, owner_pid:AtomicU64, active:AtomicBool, allocated:AtomicBool }
impl EndpointObject { const fn new()->Self { Self { endpoint_id:AtomicU32::new(0), owner_pid:AtomicU64::new(0), active:AtomicBool::new(false), allocated:AtomicBool::new(false) } } }
static ENDPOINT_OBJECTS:[EndpointObject;MAX_PORT_OBJECTS]=[const { EndpointObject::new() };MAX_PORT_OBJECTS];
#[repr(C)]
#[derive(Clone,Copy)]
struct PortMessage { data_length:u16, total_length:u16, message_type:u16, data_info_offset:u16, client_id:[usize;2], message_id:u32, client_view_size:usize }

#[repr(C)]
struct PortSectionObject { section_id:AtomicU64, owner_root:AtomicU64, owner_pid:AtomicU64, port_id:AtomicU32, active:AtomicBool, allocated:AtomicBool }
impl PortSectionObject { const fn new()->Self { Self { section_id:AtomicU64::new(0), owner_root:AtomicU64::new(0), owner_pid:AtomicU64::new(0), port_id:AtomicU32::new(0), active:AtomicBool::new(false), allocated:AtomicBool::new(false) } } }
static PORT_SECTIONS:[PortSectionObject;MAX_PORT_OBJECTS]=[const { PortSectionObject::new() };MAX_PORT_OBJECTS];
#[repr(C)]
#[derive(Clone,Copy)]
struct DataViewAttributes { flags:u32, section_handle:usize, view_base:usize, view_size:usize }

fn port_type()->Result<u16,u64>{
    let old=ALPC_PORT_TYPE.load(Ordering::Acquire);
    if old!=0 { return Ok(old as u16); }
    let name=[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'P' as u16,b'o' as u16,b'r' as u16,b't' as u16];
    match crate::ntoskrnl::ob::runtime::ensure_type(&name,ACCESS_READ,ACCESS_WRITE,0,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL) {
        Ok(id)=>{ALPC_PORT_TYPE.store(id as u32,Ordering::Release);Ok(id)}
        Err(_)=>Err(STATUS_INSUFFICIENT_RESOURCES)
    }
}
unsafe extern "win64" fn port_finalizer(body:*mut c_void) {
    if body.is_null(){return}
    let p=unsafe{&*(body as *const PortObject)};
    if p.active.swap(false,Ordering::AcqRel) {
        let id=p.port_id.load(Ordering::Acquire);
        let owner=p.owner_pid.load(Ordering::Acquire);
        let _=crate::ntoskrnl::alpc::close_port(id,owner);
    }
    p.allocated.store(false,Ordering::Release);
}
fn current_pid()->u64 { crate::syscall::current_process_id() }


fn security_context_type()->Result<u16,u64>{
    let old=ALPC_SECURITY_CONTEXT_TYPE.load(Ordering::Acquire);
    if old!=0{return Ok(old as u16)}
    let name=[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'S' as u16,b'e' as u16,b'c' as u16,b'u' as u16,b'r' as u16,b'i' as u16,b't' as u16,b'y' as u16,b' ' as u16,b'C' as u16,b't' as u16,b'x' as u16];
    match crate::ntoskrnl::ob::runtime::ensure_type(&name,ACCESS_READ,ACCESS_WRITE,0,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL){
        Ok(id)=>{ALPC_SECURITY_CONTEXT_TYPE.store(id as u32,Ordering::Release);Ok(id)}
        Err(_)=>Err(STATUS_INSUFFICIENT_RESOURCES)
    }
}
unsafe extern "win64" fn security_context_finalizer(body:*mut c_void){
    if body.is_null(){return}
    let ctx=unsafe{&*(body as *const SecurityContextObject)};
    if ctx.active.swap(false,Ordering::AcqRel){
        let _=crate::ntoskrnl::alpc_resources::delete_security_context(ctx.context_id.load(Ordering::Acquire),ctx.owner_pid.load(Ordering::Acquire));
    }
    ctx.allocated.store(false,Ordering::Release);
}
fn security_context_from_handle(handle:u64,port_id:u32)->Result<(&'static SecurityContextObject,*mut c_void),u64>{
    let owner=crate::ntoskrnl::ob::runtime::current_owner();
    let body=crate::ntoskrnl::ob::runtime::reference_by_handle(handle as *mut c_void,ACCESS_READ,ALPC_SECURITY_CONTEXT_TYPE.load(Ordering::Acquire) as u16,owner,false).map_err(|e|e as u32 as u64)?;
    let ptr=body as *const SecurityContextObject;
    if !SECURITY_CONTEXT_OBJECTS.iter().any(|c|core::ptr::eq(c as *const SecurityContextObject,ptr)){
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(STATUS_OBJECT_TYPE_MISMATCH)
    }
    let ctx=unsafe{&*ptr};
    if !ctx.active.load(Ordering::Acquire){let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(0xC000_0008)}
    if ctx.owner_pid.load(Ordering::Acquire)!=current_pid()||ctx.port_id.load(Ordering::Acquire)!=port_id{
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(0xC000_0022)
    }
    Ok((ctx,body))
}
fn create_security_context(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (port,port_body)=match port_from_handle(args[0],ACCESS_CONTROL){Ok(v)=>v,Err(s)=>return s};
    if args[1]!=0||args[2]==0 {let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}
    let mut attr:SecurityAttribute=match read_user(args[2]){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    // ALPC_SECFLG_CREATE_HANDLE; other flags require the full token/QoS implementation.
    if attr.flags!=0x0002_0000||attr.qos==0 {let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_NOT_IMPLEMENTED}
    let qos:SecurityQualityOfService=match read_user(attr.qos as u64){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    if (qos.length as usize)<core::mem::size_of::<SecurityQualityOfService>()||qos.impersonation_level>2 {
        let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER
    }
    if let Err(s)=security_context_type(){let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}
    let pid=current_pid();
    let id=match crate::ntoskrnl::alpc_resources::create_security_context(pid,pid,ACCESS_READ|ACCESS_CONTROL,qos.impersonation_level as u8){Ok(id)=>id,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s as u64}};
    let Some(slot)=SECURITY_CONTEXT_OBJECTS.iter().position(|s|!s.allocated.load(Ordering::Acquire)) else {
        let _=crate::ntoskrnl::alpc_resources::delete_security_context(id,pid);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INSUFFICIENT_RESOURCES
    };
    let ctx=&SECURITY_CONTEXT_OBJECTS[slot];
    if ctx.allocated.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){
        let _=crate::ntoskrnl::alpc_resources::delete_security_context(id,pid);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INSUFFICIENT_RESOURCES
    }
    ctx.context_id.store(id,Ordering::Release);ctx.owner_pid.store(pid,Ordering::Release);ctx.port_id.store(port.port_id.load(Ordering::Acquire),Ordering::Release);ctx.active.store(true,Ordering::Release);
    let type_name=[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'S' as u16,b'e' as u16,b'c' as u16,b'u' as u16,b'r' as u16,b'i' as u16,b't' as u16,b'y' as u16,b' ' as u16,b'C' as u16,b't' as u16,b'x' as u16];
    let name=[0u16;0];let body=ctx as *const SecurityContextObject as *mut c_void;
    if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(body,&type_name,&name,false,ALPC_DEFAULT_SD.as_ptr(),ALPC_DEFAULT_SD.len(),security_context_finalizer)}.is_err(){
        ctx.active.store(false,Ordering::Release);ctx.allocated.store(false,Ordering::Release);let _=crate::ntoskrnl::alpc_resources::delete_security_context(id,pid);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return 0xC000_0035
    }
    let handle=match crate::ntoskrnl::ob::runtime::open_by_pointer(body,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,crate::ntoskrnl::ob::runtime::current_owner(),false,0){
        Ok(h)=>h as usize,Err(e)=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return e as u32 as u64}
    };
    attr.context_handle=handle;
    let result=if write_user(args[2],&attr).is_err(){let _=crate::ntoskrnl::ob::runtime::unregister_object(body);let _=crate::ntoskrnl::ob::runtime::close_handle(handle as *mut c_void,crate::ntoskrnl::ob::runtime::current_owner());STATUS_ACCESS_VIOLATION}else{STATUS_SUCCESS};
    let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);result
}

/// Capture the peer's registered primary access token, duplicate its security
/// identity as an impersonation token, and assign it to the current KTHREAD.
/// Unknown/missing tokens fail closed; no PID supplied by user memory is trusted.
fn impersonate_client_of_port(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64 {
    if args[1]==0 || args[2]!=0 { return STATUS_INVALID_PARAMETER; }
    let (endpoint,body)=match endpoint_from_handle(args[0],ACCESS_CONTROL) {
        Ok(v)=>v, Err(s)=>return s
    };
    let peer=match crate::ntoskrnl::alpc::peer_process_id(endpoint.endpoint_id.load(Ordering::Acquire),current_pid()) {
        Ok(pid)=>pid, Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return s as u64}
    };
    // Validate the message header is readable. Identity comes from the
    // connection object above, not its mutable ClientId fields.
    let _message:PortMessage=match read_user(args[1]) {
        Ok(m)=>m, Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return s}
    };
    let mut level=2u32; // SecurityImpersonation, capped to the token's registered level.
    for ctx in SECURITY_CONTEXT_OBJECTS.iter().filter(|c|c.active.load(Ordering::Acquire)
        && c.owner_pid.load(Ordering::Acquire)==current_pid()) {
        let id=ctx.context_id.load(Ordering::Acquire);
        if let Ok(info)=crate::ntoskrnl::alpc_resources::query_security_context(id,current_pid()) {
            level=level.min(info.impersonation_level as u32);
        }
    }
    if level==0 {
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        return 0xC000_0022;
    }
    let source=crate::process::reference_primary_token(peer);
    if source.is_null() {
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        return 0xC000_0022;
    }
    let Some(slot)=CAPTURED_TOKENS.iter().position(|t|!t.allocated.load(Ordering::Acquire)) else {
        let _=unsafe{crate::ntoskrnl::se::runtime::se_internal_DereferenceToken(source)};
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        return STATUS_INSUFFICIENT_RESOURCES;
    };
    let storage=&CAPTURED_TOKENS[slot];
    if storage.allocated.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err() {
        let _=unsafe{crate::ntoskrnl::se::runtime::se_internal_DereferenceToken(source)};
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    let destination=storage.words.as_ptr() as *mut c_void;
    let duplicate=unsafe{crate::ntoskrnl::se::runtime::se_internal_DuplicateTokenSecurity(source,destination,2,level)};
    let _=unsafe{crate::ntoskrnl::se::runtime::se_internal_DereferenceToken(source)};
    if duplicate<0 {
        storage.allocated.store(false,Ordering::Release);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        return duplicate as u32 as u64;
    }
    let thread=crate::ntoskrnl::ke::thread::current_thread_object();
    let status=crate::ntoskrnl::ke::thread::assign_thread_impersonation_token(thread,destination);
    if status<0 {
        let _=unsafe{crate::ntoskrnl::se::runtime::se_internal_UnregisterTokenMetadata(destination)};
        storage.allocated.store(false,Ordering::Release);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        return status as u32 as u64;
    }
    let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
    STATUS_SUCCESS
}

/// Called after KTHREAD drops its impersonation-token reference during rundown.
pub(crate) fn release_captured_impersonation_token(token:*mut c_void) {
    if token.is_null(){return}
    for storage in CAPTURED_TOKENS.iter() {
        if storage.words.as_ptr() as *mut c_void == token
            && storage.allocated.load(Ordering::Acquire)
            && crate::ntoskrnl::se::runtime::se_internal_TokenReferenceCount(token)==0 {
            let _=unsafe{crate::ntoskrnl::se::runtime::se_internal_UnregisterTokenMetadata(token)};
            storage.allocated.store(false,Ordering::Release);
            return;
        }
    }
}

/// Revert the current thread to its process primary token by clearing its
/// impersonation token. Token metadata is reclaimed after the thread releases it.
/// Tear down ALPC resources before a process address space is reaped.
/// Resources are released once; Object Manager finalizers only release slots.
pub(crate) fn rundown_process(pid:u64) {
    if pid==0{return}
    let process=crate::process::eprocess_pointer_for_pid(pid);
    let root=if process.is_null(){0}else{unsafe{(*process).address_space_root}};
    while let Some(view)=crate::ntoskrnl::alpc_resources::take_section_view_for_rundown(pid) {
        if root!=0 { let _=crate::ntoskrnl::mm::section::unmap_view(root,view.base); }
    }
    for p in PORT_OBJECTS.iter() {
        if p.allocated.load(Ordering::Acquire)&&p.owner_pid.load(Ordering::Acquire)==pid {
            if p.active.swap(false,Ordering::AcqRel) {
                let _=crate::ntoskrnl::alpc::close_port(p.port_id.load(Ordering::Acquire),pid);
            }
            let _=crate::ntoskrnl::ob::runtime::unregister_object(p as *const PortObject as *mut c_void);
        }
    }
    for e in ENDPOINT_OBJECTS.iter() {
        if e.allocated.load(Ordering::Acquire)&&e.owner_pid.load(Ordering::Acquire)==pid {
            if e.active.swap(false,Ordering::AcqRel) {
                let _=crate::ntoskrnl::alpc::disconnect(e.endpoint_id.load(Ordering::Acquire),pid);
            }
            let _=crate::ntoskrnl::ob::runtime::unregister_object(e as *const EndpointObject as *mut c_void);
        }
    }
    for s in PORT_SECTIONS.iter() {
        if s.allocated.load(Ordering::Acquire)&&s.owner_pid.load(Ordering::Acquire)==pid {
            if s.active.swap(false,Ordering::AcqRel) {
                let _=crate::ntoskrnl::mm::section::dereference(s.section_id.load(Ordering::Acquire),s.owner_root.load(Ordering::Acquire));
            }
            let _=crate::ntoskrnl::ob::runtime::unregister_object(s as *const PortSectionObject as *mut c_void);
        }
    }
    for c in SECURITY_CONTEXT_OBJECTS.iter() {
        if c.allocated.load(Ordering::Acquire)&&c.owner_pid.load(Ordering::Acquire)==pid {
            if c.active.swap(false,Ordering::AcqRel) {
                let _=crate::ntoskrnl::alpc_resources::delete_security_context(c.context_id.load(Ordering::Acquire),pid);
            }
            let _=crate::ntoskrnl::ob::runtime::unregister_object(c as *const SecurityContextObject as *mut c_void);
        }
    }
}

pub(crate) fn revert_current_impersonation()->u64 {
    let thread=crate::ntoskrnl::ke::thread::current_thread_object();
    let current=crate::ntoskrnl::ke::thread::reference_current_impersonation_token();
    if current.is_null(){return STATUS_SUCCESS}
    let status=crate::ntoskrnl::ke::thread::assign_thread_impersonation_token(thread,core::ptr::null_mut());
    let _=unsafe{crate::ntoskrnl::se::runtime::se_internal_DereferenceToken(current)};
    if status<0{return status as u32 as u64}
    release_captured_impersonation_token(current);
    STATUS_SUCCESS
}

fn delete_security_context(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (port,port_body)=match port_from_handle(args[0],ACCESS_CONTROL){Ok(v)=>v,Err(s)=>return s};
    if args[1]!=0 {let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}
    let (ctx,ctx_body)=match security_context_from_handle(args[2],port.port_id.load(Ordering::Acquire)){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    let id=ctx.context_id.load(Ordering::Acquire);let pid=ctx.owner_pid.load(Ordering::Acquire);
    let status=crate::ntoskrnl::alpc_resources::delete_security_context(id,pid).map(|_|STATUS_SUCCESS).unwrap_or(0xC000_0008);
    ctx.active.store(false,Ordering::Release);
    let owner=crate::ntoskrnl::ob::runtime::current_owner();
    let _=crate::ntoskrnl::ob::runtime::unregister_object(ctx_body);
    let _=crate::ntoskrnl::ob::runtime::close_handle(args[2] as *mut c_void,owner);
    let _=crate::ntoskrnl::ob::runtime::dereference_body(ctx_body);
    let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);status
}
fn revoke_security_context(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (port,port_body)=match port_from_handle(args[0],ACCESS_CONTROL){Ok(v)=>v,Err(s)=>return s};
    if args[1]!=0 {let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}
    let (ctx,ctx_body)=match security_context_from_handle(args[2],port.port_id.load(Ordering::Acquire)){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    let status=crate::ntoskrnl::alpc_resources::revoke_security_context(ctx.context_id.load(Ordering::Acquire),ctx.owner_pid.load(Ordering::Acquire)).map(|_|STATUS_SUCCESS).unwrap_or(0xC000_0008);
    let _=crate::ntoskrnl::ob::runtime::dereference_body(ctx_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);status
}
fn endpoint_type()->Result<u16,u64>{
    let old=ALPC_ENDPOINT_TYPE.load(Ordering::Acquire);
    if old!=0{return Ok(old as u16)}
    let name=[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'E' as u16,b'n' as u16,b'd' as u16,b'p' as u16,b'o' as u16,b'i' as u16,b'n' as u16,b't' as u16];
    match crate::ntoskrnl::ob::runtime::ensure_type(&name,ACCESS_READ,ACCESS_WRITE,0,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL){
        Ok(id)=>{ALPC_ENDPOINT_TYPE.store(id as u32,Ordering::Release);Ok(id)}
        Err(_)=>Err(STATUS_INSUFFICIENT_RESOURCES)
    }
}
unsafe extern "win64" fn endpoint_finalizer(body:*mut c_void){
    if body.is_null(){return}
    let e=unsafe{&*(body as *const EndpointObject)};
    if e.active.swap(false,Ordering::AcqRel){
        let _=crate::ntoskrnl::alpc::disconnect(e.endpoint_id.load(Ordering::Acquire),e.owner_pid.load(Ordering::Acquire));
    }
    e.allocated.store(false,Ordering::Release);
}
fn endpoint_from_handle(handle:u64,required:u32)->Result<(&'static EndpointObject,*mut c_void),u64>{
    let owner=crate::ntoskrnl::ob::runtime::current_owner();
    let body=crate::ntoskrnl::ob::runtime::reference_by_handle(handle as *mut c_void,required,ALPC_ENDPOINT_TYPE.load(Ordering::Acquire) as u16,owner,false).map_err(|e|e as u32 as u64)?;
    let ptr=body as *const EndpointObject;
    if !ENDPOINT_OBJECTS.iter().any(|e|core::ptr::eq(e as *const EndpointObject,ptr)){
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(STATUS_OBJECT_TYPE_MISMATCH)
    }
    let e=unsafe{&*ptr};
    if !e.active.load(Ordering::Acquire){let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(0xC000_0037)}
    Ok((e,body))
}
fn make_endpoint_handle(endpoint_id:u32,owner_pid:u64,output:u64)->u64{
    if let Err(s)=endpoint_type(){let _=crate::ntoskrnl::alpc::disconnect(endpoint_id,owner_pid);return s}
    let Some(slot)=ENDPOINT_OBJECTS.iter().position(|e|!e.allocated.load(Ordering::Acquire)) else{let _=crate::ntoskrnl::alpc::disconnect(endpoint_id,owner_pid);return STATUS_INSUFFICIENT_RESOURCES};
    let endpoint=&ENDPOINT_OBJECTS[slot];
    if endpoint.allocated.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){let _=crate::ntoskrnl::alpc::disconnect(endpoint_id,owner_pid);return STATUS_INSUFFICIENT_RESOURCES}
    endpoint.endpoint_id.store(endpoint_id,Ordering::Release);endpoint.owner_pid.store(owner_pid,Ordering::Release);endpoint.active.store(true,Ordering::Release);
    let type_name=[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'E' as u16,b'n' as u16,b'd' as u16,b'p' as u16,b'o' as u16,b'i' as u16,b'n' as u16,b't' as u16];
    let name=[0u16;0];let body=endpoint as *const EndpointObject as *mut c_void;
    if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(body,&type_name,&name,false,ALPC_DEFAULT_SD.as_ptr(),ALPC_DEFAULT_SD.len(),endpoint_finalizer)}.is_err(){
        endpoint.active.store(false,Ordering::Release);endpoint.allocated.store(false,Ordering::Release);let _=crate::ntoskrnl::alpc::disconnect(endpoint_id,owner_pid);return 0xC000_0035
    }
    let handle=match crate::ntoskrnl::ob::runtime::open_by_pointer(body,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,crate::ntoskrnl::ob::runtime::current_owner(),false,0){
        Ok(h)=>h as usize,Err(e)=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);return e as u32 as u64}
    };
    if let Err(s)=write_user(output,&handle){let _=crate::ntoskrnl::ob::runtime::unregister_object(body);let _=crate::ntoskrnl::ob::runtime::close_handle(handle as *mut c_void,crate::ntoskrnl::ob::runtime::current_owner());return s}
    STATUS_SUCCESS
}
fn connect_port(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    if args[0]==0||args[1]==0||args[4]!=0||args[5]!=0||args[6]!=0||args[8]!=0||args[9]!=0||args[10]!=0{return STATUS_NOT_IMPLEMENTED}
    let us:UnicodeString=match read_user(args[1]){Ok(v)=>v,Err(s)=>return s};
    if us.length==0||us.length%2!=0||us.length>us.maximum_length||us.length as usize/2>=MAX_NAME||us.buffer.is_null(){return STATUS_INVALID_PARAMETER}
    let n=us.length as usize/2;let mut name=[0u16;MAX_NAME];
    let bytes=unsafe{core::slice::from_raw_parts_mut(name.as_mut_ptr() as *mut u8,n*2)};
    if !crate::syscall::copy_current_user_from(us.buffer as u64,bytes){return STATUS_ACCESS_VIOLATION}
    if name[..n].iter().any(|c|*c==0){return STATUS_INVALID_PARAMETER}
    for unit in &mut name[..n] { if (b'A' as u16..=b'Z' as u16).contains(unit) { *unit += 32; } }
    let endpoint=match crate::ntoskrnl::alpc::connect(&name[..n],current_pid()){Ok(id)=>id,Err(s)=>return s as u32 as u64};
    make_endpoint_handle(endpoint,current_pid(),args[0])
}
fn connect_port_ex(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    if args[0]==0||args[1]==0||args[2]!=0||args[3]!=0||args[4]!=0||args[5]!=0||args[6]!=0||args[7]!=0||args[8]!=0||args[9]!=0||args[10]!=0{return STATUS_NOT_IMPLEMENTED}
    let pid=current_pid();let mut name=[0u16;MAX_NAME];
    let n=match object_name(args[1],&mut name,pid,0){Ok(n)=>n,Err(s)=>return s};
    let endpoint=match crate::ntoskrnl::alpc::connect(&name[..n],pid){Ok(id)=>id,Err(s)=>return s as u32 as u64};
    make_endpoint_handle(endpoint,pid,args[0])
}
fn accept_connect_port(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    if args[0]==0||args[2]!=0||args[3]!=0||args[4]!=0||args[5]!=0||args[6]==0||args[7]!=0{return STATUS_NOT_IMPLEMENTED}
    let (port,port_body)=match port_from_handle(args[1],ACCESS_WRITE){Ok(v)=>v,Err(s)=>return s};
    let msg:PortMessage=match read_user(args[6]){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    let accept=args[8]!=0;
    let endpoint=crate::ntoskrnl::alpc::accept(port.port_id.load(Ordering::Acquire),current_pid(),msg.message_id,accept);
    let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);
    match endpoint{Ok(id)=>make_endpoint_handle(id,current_pid(),args[0]),Err(s)=>s as u32 as u64}
}
fn send_wait_receive_port(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    if args[1]!=0||args[3]!=0||args[6]!=0||args[7]!=0{return STATUS_NOT_IMPLEMENTED}
    let send_ptr=args[2];let receive_ptr=args[4];
    if send_ptr==0&&receive_ptr==0{return STATUS_INVALID_PARAMETER}
    if send_ptr!=0{
        let (endpoint,body)=match endpoint_from_handle(args[0],ACCESS_WRITE){Ok(v)=>v,Err(s)=>return s};
        let header:PortMessage=match read_user(send_ptr){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return s}};
        let length=header.data_length as usize;
        if length==0||length>512||(header.total_length as usize)<(core::mem::size_of::<PortMessage>()+length){
            let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_INVALID_PARAMETER
        }
        let mut data=[0u8;512];
        let address=match send_ptr.checked_add(core::mem::size_of::<PortMessage>() as u64){Some(v)=>v,None=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_ACCESS_VIOLATION}};
        if !crate::syscall::copy_current_user_from(address,&mut data[..length]){
            let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_ACCESS_VIOLATION
        }
        let status=crate::ntoskrnl::alpc::send(endpoint.endpoint_id.load(Ordering::Acquire),current_pid(),&data[..length]).map(|_|STATUS_SUCCESS).unwrap_or_else(|s|s as u64);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        if status!=STATUS_SUCCESS{return status}
    }
    if receive_ptr==0{return STATUS_SUCCESS}
    if args[5]==0{return STATUS_INVALID_PARAMETER}
    let buffer_length:usize=match read_user(args[5]){Ok(v)=>v,Err(s)=>return s};
    if buffer_length<core::mem::size_of::<PortMessage>(){let _=write_user(args[5],&core::mem::size_of::<PortMessage>());return 0xC000_0023}
    let owner=crate::ntoskrnl::ob::runtime::current_owner();
    if let Ok((endpoint,body))=endpoint_from_handle(args[0],ACCESS_READ){
        let capacity=(buffer_length-core::mem::size_of::<PortMessage>()).min(512);
        let mut data=[0u8;512];
        let result=crate::ntoskrnl::alpc::receive(endpoint.endpoint_id.load(Ordering::Acquire),current_pid(),&mut data[..capacity]);
        let out=match result{Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return s as u64}};
        let total=core::mem::size_of::<PortMessage>()+out.0;
        let header=PortMessage{data_length:out.0 as u16,total_length:total as u16,message_type:1,data_info_offset:0,client_id:[0;2],message_id:out.1,client_view_size:0};
        let Some(data_address)=receive_ptr.checked_add(core::mem::size_of::<PortMessage>() as u64) else {let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_ACCESS_VIOLATION};
        if write_user(receive_ptr,&header).is_err()||!crate::syscall::copy_current_user_to(data_address,&data[..out.0]){
            let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return STATUS_ACCESS_VIOLATION
        }
        let _=write_user(args[5],&total);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        return STATUS_SUCCESS
    }
    let (port,body)=match port_from_handle(args[0],ACCESS_READ){Ok(v)=>v,Err(s)=>return s};
    let port_id=port.port_id.load(Ordering::Acquire);
    let _=crate::ntoskrnl::alpc::listen(port_id,current_pid());
    let request=crate::ntoskrnl::alpc::listen_next(port_id,current_pid());
    let request=match request{Ok(id)=>id,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return s as u64}};
    let header=PortMessage{data_length:0,total_length:core::mem::size_of::<PortMessage>() as u16,message_type:10,data_info_offset:0,client_id:[0;2],message_id:request,client_view_size:0};
    let result=if write_user(receive_ptr,&header).is_err(){STATUS_ACCESS_VIOLATION}else{let len=core::mem::size_of::<PortMessage>();let _=write_user(args[5],&len);STATUS_SUCCESS};
    let _=crate::ntoskrnl::ob::runtime::dereference_body(body);result
}
fn section_type()->Result<u16,u64>{
    let old=ALPC_SECTION_TYPE.load(Ordering::Acquire);
    if old!=0 { return Ok(old as u16); }
    let name=[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'S' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'i' as u16,b'o' as u16,b'n' as u16];
    match crate::ntoskrnl::ob::runtime::ensure_type(&name,ACCESS_READ,ACCESS_WRITE,0,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL) {
        Ok(id)=>{ALPC_SECTION_TYPE.store(id as u32,Ordering::Release);Ok(id)}
        Err(_)=>Err(STATUS_INSUFFICIENT_RESOURCES)
    }
}
unsafe extern "win64" fn section_finalizer(body:*mut c_void) {
    if body.is_null(){return}
    let section=unsafe{&*(body as *const PortSectionObject)};
    if section.active.swap(false,Ordering::AcqRel) {
        let id=section.section_id.load(Ordering::Acquire);
        let root=section.owner_root.load(Ordering::Acquire);
        let _=crate::ntoskrnl::mm::section::dereference(id,root);
    }
    section.allocated.store(false,Ordering::Release);
}
fn section_from_handle(handle:u64)->Result<(&'static PortSectionObject,*mut c_void),u64>{
    let owner=crate::ntoskrnl::ob::runtime::current_owner();
    let body=crate::ntoskrnl::ob::runtime::reference_by_handle(handle as *mut c_void,ACCESS_READ,ALPC_SECTION_TYPE.load(Ordering::Acquire) as u16,owner,false).map_err(|e|e as u32 as u64)?;
    let ptr=body as *const PortSectionObject;
    if !PORT_SECTIONS.iter().any(|p|core::ptr::eq(p as *const PortSectionObject,ptr)) {
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(STATUS_OBJECT_TYPE_MISMATCH);
    }
    let section=unsafe{&*ptr};
    if !section.active.load(Ordering::Acquire) { let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(0xC000_0008); }
    Ok((section,body))
}
fn root_for_current()->Result<u64,u64>{
    let process=crate::process::eprocess_pointer_for_pid(current_pid());
    if process.is_null(){return Err(0xC000_0008)}
    let root=unsafe{(*process).address_space_root};
    if root==0{return Err(0xC000_0008)}
    Ok(root)
}
fn create_port_section(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (port,port_body)=match port_from_handle(args[0],ACCESS_WRITE){Ok(v)=>v,Err(s)=>return s};
    if args[1]!=0 || args[2]!=0 || args[3]==0 || args[4]==0 || args[5]==0 {
        let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);
        return if args[2]!=0 {STATUS_NOT_IMPLEMENTED}else{STATUS_INVALID_PARAMETER};
    }
    if let Err(s)=section_type(){let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}
    let root=match root_for_current(){Ok(r)=>r,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    let mut section_id=0u64;
    let status=crate::ntoskrnl::mm::section::create(args[3],root,&mut section_id);
    if status<0 {let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return status as u32 as u64;}
    let Some(slot)=PORT_SECTIONS.iter().position(|s|!s.allocated.load(Ordering::Acquire)) else {
        let _=crate::ntoskrnl::mm::section::dereference(section_id,root);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INSUFFICIENT_RESOURCES;
    };
    let section=&PORT_SECTIONS[slot];
    if section.allocated.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){let _=crate::ntoskrnl::mm::section::dereference(section_id,root);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INSUFFICIENT_RESOURCES;}
    section.section_id.store(section_id,Ordering::Release);section.active.store(true,Ordering::Release);
    section.owner_root.store(root,Ordering::Release);
    section.owner_pid.store(current_pid(),Ordering::Release);
    section.port_id.store(port.port_id.load(Ordering::Acquire),Ordering::Release);
    let name=[0u16;0];
    let type_name=[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'S' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'i' as u16,b'o' as u16,b'n' as u16];
    let body=section as *const PortSectionObject as *mut c_void;
    if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(body,&type_name,&name,false,ALPC_DEFAULT_SD.as_ptr(),ALPC_DEFAULT_SD.len(),section_finalizer)}.is_err(){
        section.active.store(false,Ordering::Release);section.allocated.store(false,Ordering::Release);let _=crate::ntoskrnl::mm::section::dereference(section_id,root);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return 0xC000_0035;
    }
    let handle=match crate::ntoskrnl::ob::runtime::open_by_pointer(body,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,crate::ntoskrnl::ob::runtime::current_owner(),true,0){
        Ok(h)=>h as usize,Err(e)=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return e as u32 as u64}
    };
    let actual=match args[3].checked_add(4095){Some(n)=>n&!4095,None=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);let _=crate::ntoskrnl::ob::runtime::close_handle(handle as *mut c_void,crate::ntoskrnl::ob::runtime::current_owner());let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}};
    let result=if write_user(args[4],&handle).is_err()||write_user(args[5],&actual).is_err(){let _=crate::ntoskrnl::ob::runtime::unregister_object(body);let _=crate::ntoskrnl::ob::runtime::close_handle(handle as *mut c_void,crate::ntoskrnl::ob::runtime::current_owner());STATUS_ACCESS_VIOLATION}else{STATUS_SUCCESS};
    let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);
    result
}
fn find_free_view_base(root:u64,length:u64)->Result<u64,u64>{
    let mut space=unsafe{crate::ntoskrnl::mm::address_space::AddressSpace::attach_existing(root)}.map_err(|_|STATUS_INVALID_PARAMETER)?;
    let mut candidate=0x0000_0001_0000_0000u64;
    while candidate.checked_add(length).is_some_and(|end|end<0x0000_8000_0000_0000) {
        let mut free=true;let mut offset=0;
        while offset<length {
            if space.query_region(candidate+offset).is_some_and(|r|r.7){free=false;break}
            offset+=4096;
        }
        if free{return Ok(candidate)}
        candidate=match candidate.checked_add(length.max(0x10000)){Some(v)=>v,None=>break};
    }
    Err(STATUS_INSUFFICIENT_RESOURCES)
}
fn create_section_view(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (port,port_body)=match port_from_handle(args[0],ACCESS_WRITE){Ok(v)=>v,Err(s)=>return s};
    if args[1]!=0 || args[2]==0 {let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}
    let mut view:DataViewAttributes=match read_user(args[2]){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    if view.flags!=0 || view.view_base!=0 || view.view_size==0 {let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}
    let (section,section_body)=match section_from_handle(view.section_handle as u64){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    let pid=current_pid();let root=match root_for_current(){Ok(r)=>r,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    if section.port_id.load(Ordering::Acquire)!=port.port_id.load(Ordering::Acquire) {
        let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return 0xC000_0022;
    }
    let length=match (view.view_size as u64).checked_add(4095){Some(n)=>n&!4095,None=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}};
    let base=match find_free_view_base(root,length){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    let section_id=section.section_id.load(Ordering::Acquire);
    if let Err(e)=crate::ntoskrnl::mm::section::map_view(section_id,root,base,0,length,true,false){let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return e as u32 as u64;}
    let view_id=match crate::ntoskrnl::alpc_resources::register_section_view(pid,port.port_id.load(Ordering::Acquire),section_id,base,length){Ok(id)=>id,Err(s)=>{let _=crate::ntoskrnl::mm::section::unmap_view(root,base);let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s as u64}};
    view.view_base=base as usize;view.view_size=length as usize;
    let result=if write_user(args[2],&view).is_err(){let _=crate::ntoskrnl::mm::section::unmap_view(root,base);let _=crate::ntoskrnl::alpc_resources::unregister_section_view(view_id,pid);STATUS_ACCESS_VIOLATION}else{STATUS_SUCCESS};
    let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);
    result
}
fn delete_section_view(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (port,port_body)=match port_from_handle(args[0],ACCESS_WRITE){Ok(v)=>v,Err(s)=>return s};
    if args[1]!=0||args[2]==0{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}
    let pid=current_pid();let root=match root_for_current(){Ok(r)=>r,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    let base=args[2];let view=match crate::ntoskrnl::alpc_resources::find_section_view(pid,port.port_id.load(Ordering::Acquire),base){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s as u64}};
    let status=crate::ntoskrnl::mm::section::unmap_view(root,base) as u32 as u64;
    if status==STATUS_SUCCESS {let _=crate::ntoskrnl::alpc_resources::unregister_section_view(view.id,pid);}
    let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);status
}
fn delete_port_section(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (port,port_body)=match port_from_handle(args[0],ACCESS_WRITE){Ok(v)=>v,Err(s)=>return s};
    if args[1]!=0 {let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}
    let (section,section_body)=match section_from_handle(args[2]){Ok(v)=>v,Err(s)=>{let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return s}};
    if section.port_id.load(Ordering::Acquire)!=port.port_id.load(Ordering::Acquire) || section.owner_pid.load(Ordering::Acquire)!=current_pid() {
        let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return 0xC000_0022;
    }
    let owner=crate::ntoskrnl::ob::runtime::current_owner();
    let _=crate::ntoskrnl::ob::runtime::unregister_object(section_body);
    let _=crate::ntoskrnl::ob::runtime::close_handle(args[2] as *mut c_void,owner);
    let _=crate::ntoskrnl::ob::runtime::dereference_body(section_body);
    let _=section;
    let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);
    STATUS_SUCCESS
}

fn read_user<T:Copy>(address:u64)->Result<T,u64>{
    if address==0{return Err(STATUS_ACCESS_VIOLATION)}
    let mut out=core::mem::MaybeUninit::<T>::uninit();
    let bytes=unsafe{core::slice::from_raw_parts_mut(out.as_mut_ptr() as *mut u8,core::mem::size_of::<T>())};
    if !crate::syscall::copy_current_user_from(address,bytes){return Err(STATUS_ACCESS_VIOLATION)}
    Ok(unsafe{out.assume_init()})
}
fn write_user<T:Copy>(address:u64,value:&T)->Result<(),u64>{
    if address==0{return Err(STATUS_ACCESS_VIOLATION)}
    let bytes=unsafe{core::slice::from_raw_parts(value as *const T as *const u8,core::mem::size_of::<T>())};
    if !crate::syscall::copy_current_user_to(address,bytes){return Err(STATUS_ACCESS_VIOLATION)}
    Ok(())
}
fn port_from_handle(handle:u64,required:u32)->Result<(&'static PortObject,*mut c_void),u64>{
    let owner=crate::ntoskrnl::ob::runtime::current_owner();
    let body=crate::ntoskrnl::ob::runtime::reference_by_handle(handle as *mut c_void,required,ALPC_PORT_TYPE.load(Ordering::Acquire) as u16,owner,false).map_err(|e|e as u32 as u64)?;
    let ptr=body as *const PortObject;
    let found=PORT_OBJECTS.iter().any(|p|core::ptr::eq(p as *const PortObject,ptr));
    if !found { let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(STATUS_OBJECT_TYPE_MISMATCH); }
    let p=unsafe{&*ptr};
    if !p.active.load(Ordering::Acquire){let _=crate::ntoskrnl::ob::runtime::dereference_body(body);return Err(0xC000_0037);}
    Ok((p,body))
}
fn object_name(attrs:u64, out:&mut [u16;MAX_NAME], fallback_pid:u64, slot:usize)->Result<usize,u64>{
    if attrs==0 {
        let prefix=[b'\\' as u16,b'C' as u16,b'o' as u16,b'g' as u16,b'n' as u16,b'a' as u16,b'c' as u16,b'O' as u16,b'S' as u16,b'\\' as u16,b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b'-' as u16];
        out[..prefix.len()].copy_from_slice(&prefix);
        let mut n=prefix.len(); n=write_decimal(fallback_pid,out,n); out[n]=b'-' as u16;n+=1; n=write_decimal(slot as u64,out,n); return Ok(n);
    }
    let oa: ObjectAttributes=read_user(attrs)?;
    if (oa.length as usize) < core::mem::size_of::<ObjectAttributes>() || oa.object_name.is_null() {return Err(STATUS_INVALID_PARAMETER)}
    if oa.root_directory!=0 || oa.security_descriptor!=0 || oa.security_qos!=0 {return Err(STATUS_NOT_IMPLEMENTED)}
    let us:UnicodeString=read_user(oa.object_name as u64)?;
    if us.length==0||us.length%2!=0||us.length>us.maximum_length||us.length as usize/2>MAX_NAME||us.buffer.is_null(){return Err(STATUS_INVALID_PARAMETER)}
    let n=us.length as usize/2;
    let dst=unsafe{core::slice::from_raw_parts_mut(out.as_mut_ptr(),n)};
    let bytes=unsafe{core::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u8,n*2)};
    if !crate::syscall::copy_current_user_from(us.buffer as u64,bytes){return Err(STATUS_ACCESS_VIOLATION)}
    if out[..n].iter().any(|u|*u==0){return Err(STATUS_INVALID_PARAMETER)}
    for unit in &mut out[..n] { if (b'A' as u16..=b'Z' as u16).contains(unit) { *unit += 32; } }
    Ok(n)
}
fn write_decimal(mut value:u64,out:&mut [u16;MAX_NAME],mut at:usize)->usize{
    let start=at; if value==0 {out[at]=b'0' as u16;return at+1}
    while value>0 && at<MAX_NAME {out[at]=(b'0'+(value%10) as u8) as u16;at+=1;value/=10;}
    out[start..at].reverse();at
}

pub fn dispatch(number:u64,args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    match number {
        crate::syscall::NT_SYSCALL_ALPC_CREATE_PORT=>create_port(args),
        crate::syscall::NT_SYSCALL_ALPC_CREATE_PORT_SECTION=>create_port_section(args),
        crate::syscall::NT_SYSCALL_ALPC_DELETE_PORT_SECTION=>delete_port_section(args),
        crate::syscall::NT_SYSCALL_ALPC_CREATE_SECTION_VIEW=>create_section_view(args),
        crate::syscall::NT_SYSCALL_ALPC_CREATE_SECURITY_CONTEXT=>create_security_context(args),
        crate::syscall::NT_SYSCALL_ALPC_DELETE_SECURITY_CONTEXT=>delete_security_context(args),
        crate::syscall::NT_SYSCALL_ALPC_REVOKE_SECURITY_CONTEXT=>revoke_security_context(args),
        crate::syscall::NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_OF_PORT=>impersonate_client_of_port(args),
        crate::syscall::NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_CONTAINER_OF_PORT=>impersonate_client_of_port(args),
        crate::syscall::NT_SYSCALL_ALPC_DELETE_SECTION_VIEW=>delete_section_view(args),
        crate::syscall::NT_SYSCALL_ALPC_DISCONNECT_PORT=>disconnect_port(args),
        crate::syscall::NT_SYSCALL_ALPC_CONNECT_PORT=>connect_port(args),
        crate::syscall::NT_SYSCALL_ALPC_CONNECT_PORT_EX=>connect_port_ex(args),
        crate::syscall::NT_SYSCALL_ALPC_ACCEPT_CONNECT_PORT=>accept_connect_port(args),
        crate::syscall::NT_SYSCALL_ALPC_SEND_WAIT_RECEIVE_PORT=>send_wait_receive_port(args),
        crate::syscall::NT_SYSCALL_ALPC_CREATE_SECTION_VIEW
        |crate::syscall::NT_SYSCALL_ALPC_DELETE_SECTION_VIEW
        |crate::syscall::NT_SYSCALL_ALPC_CREATE_SECURITY_CONTEXT
        |crate::syscall::NT_SYSCALL_ALPC_DELETE_SECURITY_CONTEXT
        |crate::syscall::NT_SYSCALL_ALPC_REVOKE_SECURITY_CONTEXT
        |crate::syscall::NT_SYSCALL_ALPC_CREATE_RESOURCE_RESERVE
        |crate::syscall::NT_SYSCALL_ALPC_DELETE_RESOURCE_RESERVE
        |crate::syscall::NT_SYSCALL_ALPC_CONNECT_PORT
        |crate::syscall::NT_SYSCALL_ALPC_ACCEPT_CONNECT_PORT
        |crate::syscall::NT_SYSCALL_ALPC_SEND_WAIT_RECEIVE_PORT
        |crate::syscall::NT_SYSCALL_ALPC_CREATE_PORT_SECTION
        |crate::syscall::NT_SYSCALL_ALPC_DELETE_PORT_SECTION
        |crate::syscall::NT_SYSCALL_ALPC_QUERY_INFORMATION
        |crate::syscall::NT_SYSCALL_ALPC_SET_INFORMATION
        |crate::syscall::NT_SYSCALL_ALPC_QUERY_INFORMATION_MESSAGE
        |crate::syscall::NT_SYSCALL_ALPC_CANCEL_MESSAGE
        |crate::syscall::NT_SYSCALL_ALPC_OPEN_SENDER_PROCESS
        |crate::syscall::NT_SYSCALL_ALPC_OPEN_SENDER_THREAD
        |crate::syscall::NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_OF_PORT
        |crate::syscall::NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_CONTAINER_OF_PORT => STATUS_NOT_IMPLEMENTED,
        _=>STATUS_NOT_IMPLEMENTED
    }
}
fn create_port(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    if args[0]==0{return STATUS_INVALID_PARAMETER}
    let pid=current_pid(); if pid==0{return 0xC000_0008}
    let mut tyname=[0u16;9];tyname.copy_from_slice(&[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'P' as u16,b'o' as u16,b'r' as u16,b't' as u16]);
    if let Err(s)=port_type(){return s}
    let slot=match PORT_OBJECTS.iter().position(|p|!p.allocated.load(Ordering::Acquire)){Some(i)=>i,None=>return STATUS_INSUFFICIENT_RESOURCES};
    if PORT_OBJECTS[slot].allocated.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return STATUS_INSUFFICIENT_RESOURCES;}
    let mut name=[0u16;MAX_NAME];
    let name_len=match object_name(args[1],&mut name,pid,slot){Ok(n)=>n,Err(s)=>{PORT_OBJECTS[slot].allocated.store(false,Ordering::Release);return s}};
    let max_msg=if args[2]==0 {512} else {match read_user::<PortAttributes>(args[2]){Ok(p)=>if p.max_message_length==0{512}else{p.max_message_length.min(512)},Err(s)=>{PORT_OBJECTS[slot].allocated.store(false,Ordering::Release);return s}}};
    if max_msg<sizeof_port_message(){PORT_OBJECTS[slot].allocated.store(false,Ordering::Release);return STATUS_INVALID_PARAMETER}
    let id=match crate::ntoskrnl::alpc::create_port(&name[..name_len],pid,max_msg){Ok(id)=>id,Err(s)=>{PORT_OBJECTS[slot].allocated.store(false,Ordering::Release);return s as u32 as u64}};
    let p=&PORT_OBJECTS[slot];p.port_id.store(id,Ordering::Release);p.owner_pid.store(pid,Ordering::Release);p.active.store(true,Ordering::Release);
    let body=&PORT_OBJECTS[slot] as *const PortObject as *mut c_void;
    let name_for_ob:&[u16]=&name[..name_len];
    if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(body,&tyname,name_for_ob,false,ALPC_DEFAULT_SD.as_ptr(),ALPC_DEFAULT_SD.len(),port_finalizer)}.is_err(){
        p.active.store(false,Ordering::Release);p.allocated.store(false,Ordering::Release);let _=crate::ntoskrnl::alpc::close_port(id,pid);return 0xC000_0035;
    }
    let handle=match crate::ntoskrnl::ob::runtime::open_by_pointer(body,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,crate::ntoskrnl::ob::runtime::current_owner(),true,0){
        Ok(h)=>h as u64,Err(e)=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);return e as u32 as u64}
    };
    if let Err(s)=write_user(args[0],&(handle as usize)){let _=crate::ntoskrnl::ob::runtime::unregister_object(body);let _=crate::ntoskrnl::ob::runtime::close_handle(handle as *mut c_void,crate::ntoskrnl::ob::runtime::current_owner());return s}
    STATUS_SUCCESS
}
fn disconnect_port(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    if args[1]!=0{return STATUS_INVALID_PARAMETER}
    if let Ok((endpoint,body))=endpoint_from_handle(args[0],ACCESS_WRITE){
        let result=crate::ntoskrnl::alpc::disconnect(endpoint.endpoint_id.load(Ordering::Acquire),current_pid()).map(|_|STATUS_SUCCESS).unwrap_or(0xC000_0037);
        endpoint.active.store(false,Ordering::Release);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
        let _=crate::ntoskrnl::ob::runtime::unregister_object(body);
        return result
    }
    let (p,body)=match port_from_handle(args[0],ACCESS_WRITE){Ok(v)=>v,Err(s)=>return s};
    let id=p.port_id.load(Ordering::Acquire);let owner=p.owner_pid.load(Ordering::Acquire);
    let status=crate::ntoskrnl::alpc::close_port(id,owner).map(|_|STATUS_SUCCESS).unwrap_or(0xC000_0037);
    p.active.store(false,Ordering::Release);
    let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
    let _=crate::ntoskrnl::ob::runtime::unregister_object(body);
    status
}
const fn sizeof_port_message()->usize{40}

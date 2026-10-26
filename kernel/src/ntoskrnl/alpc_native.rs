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
struct PortObject { port_id:AtomicU32, owner_pid:AtomicU64, active:AtomicBool }
impl PortObject { const fn new()->Self { Self { port_id:AtomicU32::new(0), owner_pid:AtomicU64::new(0), active:AtomicBool::new(false) } } }
static PORT_OBJECTS:[PortObject;MAX_PORT_OBJECTS]=[const { PortObject::new() };MAX_PORT_OBJECTS];
static ALPC_PORT_TYPE:AtomicU32=AtomicU32::new(0);
static ALPC_SECTION_TYPE:AtomicU32=AtomicU32::new(0);
#[repr(C)]
struct PortSectionObject { section_id:AtomicU64, owner_root:AtomicU64, owner_pid:AtomicU64, port_id:AtomicU32, active:AtomicBool }
impl PortSectionObject { const fn new()->Self { Self { section_id:AtomicU64::new(0), owner_root:AtomicU64::new(0), owner_pid:AtomicU64::new(0), port_id:AtomicU32::new(0), active:AtomicBool::new(false) } } }
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
}
fn current_pid()->u64 { crate::syscall::current_process_id() }
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
}
fn section_from_handle(handle:u64)->Result<(&'static PortSectionObject,*mut c_void),u64>{
    let owner=crate::ntoskrnl::ob::runtime::current_owner();
    let body=crate::ntoskrnl::ob::runtime::reference_by_handle(handle as *mut c_void,ACCESS_READ,ALPC_SECTION_TYPE.load(Ordering::Acquire) as u16,owner,true).map_err(|e|e as u32 as u64)?;
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
    let Some(slot)=PORT_SECTIONS.iter().position(|s|!s.active.load(Ordering::Acquire)) else {
        let _=crate::ntoskrnl::mm::section::dereference(section_id,root);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INSUFFICIENT_RESOURCES;
    };
    let section=&PORT_SECTIONS[slot];
    section.section_id.store(section_id,Ordering::Release);
    section.owner_root.store(root,Ordering::Release);
    section.owner_pid.store(current_pid(),Ordering::Release);
    section.port_id.store(port.port_id.load(Ordering::Acquire),Ordering::Release);
    let name=[0u16;0];
    let type_name=[b'A' as u16,b'L' as u16,b'P' as u16,b'C' as u16,b' ' as u16,b'S' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'i' as u16,b'o' as u16,b'n' as u16];
    let body=section as *const PortSectionObject as *mut c_void;
    if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(body,&type_name,&name,false,core::ptr::null(),0,section_finalizer)}.is_err(){
        let _=crate::ntoskrnl::mm::section::dereference(section_id,root);
        let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return 0xC000_0035;
    }
    let handle=match crate::ntoskrnl::ob::runtime::open_by_pointer(body,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,crate::ntoskrnl::ob::runtime::current_owner(),true,0){
        Ok(h)=>h as usize,Err(e)=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return e as u32 as u64}
    };
    section.active.store(true,Ordering::Release);
    let actual=match args[3].checked_add(4095){Some(n)=>n&!4095,None=>{let _=crate::ntoskrnl::ob::runtime::close_handle(handle as *mut c_void,crate::ntoskrnl::ob::runtime::current_owner());let _=crate::ntoskrnl::ob::runtime::dereference_body(port_body);return STATUS_INVALID_PARAMETER}};
    let result=if write_user(args[4],&handle).is_err()||write_user(args[5],&actual).is_err(){STATUS_ACCESS_VIOLATION}else{STATUS_SUCCESS};
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
    if section.owner_pid.load(Ordering::Acquire)!=pid || section.port_id.load(Ordering::Acquire)!=port.port_id.load(Ordering::Acquire) || section.owner_root.load(Ordering::Acquire)!=root {
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
    let body=crate::ntoskrnl::ob::runtime::reference_by_handle(handle as *mut c_void,required,ALPC_PORT_TYPE.load(Ordering::Acquire) as u16,owner,true).map_err(|e|e as u32 as u64)?;
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
    if (oa.length as usize) < core::mem::size_of::<ObjectAttributes>() || oa.object_name==0 {return Err(STATUS_INVALID_PARAMETER)}
    let us:UnicodeString=read_user(oa.object_name as u64)?;
    if us.length==0||us.length%2!=0||us.length>us.maximum_length||us.length as usize/2>MAX_NAME||us.buffer.is_null(){return Err(STATUS_INVALID_PARAMETER)}
    let n=us.length as usize/2;
    let dst=unsafe{core::slice::from_raw_parts_mut(out.as_mut_ptr(),n)};
    let bytes=unsafe{core::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u8,n*2)};
    if !crate::syscall::copy_current_user_from(us.buffer as u64,bytes){return Err(STATUS_ACCESS_VIOLATION)}
    if out[..n].iter().any(|u|*u==0){return Err(STATUS_INVALID_PARAMETER)}
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
        crate::syscall::NT_SYSCALL_ALPC_DELETE_SECTION_VIEW=>delete_section_view(args),
        crate::syscall::NT_SYSCALL_ALPC_DISCONNECT_PORT=>disconnect_port(args),
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
    let slot=match PORT_OBJECTS.iter().position(|p|!p.active.load(Ordering::Acquire)){Some(i)=>i,None=>return STATUS_INSUFFICIENT_RESOURCES};
    let mut name=[0u16;MAX_NAME];
    let name_len=match object_name(args[1],&mut name,pid,slot){Ok(n)=>n,Err(s)=>return s};
    let max_msg=if args[2]==0 {512} else {match read_user::<PortAttributes>(args[2]){Ok(p)=>if p.max_message_length==0{512}else{p.max_message_length.min(512)},Err(s)=>return s}};
    if max_msg<sizeof_port_message(){return STATUS_INVALID_PARAMETER}
    let id=match crate::ntoskrnl::alpc::create_port(&name[..name_len],pid,max_msg){Ok(id)=>id,Err(s)=>return s as u32 as u64};
    let body=&PORT_OBJECTS[slot] as *const PortObject as *mut c_void;
    let name_for_ob:&[u16]=&name[..name_len];
    if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(body,&tyname,name_for_ob,false,core::ptr::null(),0,port_finalizer)}.is_err(){
        let _=crate::ntoskrnl::alpc::close_port(id,pid);return 0xC000_0035;
    }
    let handle=match crate::ntoskrnl::ob::runtime::open_by_pointer(body,ACCESS_READ|ACCESS_WRITE|ACCESS_CONTROL,crate::ntoskrnl::ob::runtime::current_owner(),true,0){
        Ok(h)=>h as u64,Err(e)=>{let _=crate::ntoskrnl::ob::runtime::unregister_object(body);return e as u32 as u64}
    };
    let p=&PORT_OBJECTS[slot];p.port_id.store(id,Ordering::Release);p.owner_pid.store(pid,Ordering::Release);p.active.store(true,Ordering::Release);
    if let Err(s)=write_user(args[0],&(handle as usize)){let _=crate::ntoskrnl::ob::runtime::close_handle(handle as *mut c_void,crate::ntoskrnl::ob::runtime::current_owner());return s}
    STATUS_SUCCESS
}
fn disconnect_port(args:&[u64;crate::syscall::NT_MAX_ARGUMENTS])->u64{
    let (p,body)=match port_from_handle(args[0],ACCESS_WRITE){Ok(v)=>v,Err(s)=>return s};
    let id=p.port_id.load(Ordering::Acquire);let owner=p.owner_pid.load(Ordering::Acquire);
    let status=crate::ntoskrnl::alpc::close_port(id,owner).map(|_|STATUS_SUCCESS).unwrap_or(0xC000_0037);
    p.active.store(false,Ordering::Release);
    let _=crate::ntoskrnl::ob::runtime::dereference_body(body);
    let _=crate::ntoskrnl::ob::runtime::unregister_object(body);
    status
}
const fn sizeof_port_message()->usize{40}

//! Native NT virtual-memory syscall bridge for the current process.
//! This deliberately supports only NT_CURRENT_PROCESS until process handles,
//! referenced EPROCESS lifetimes, and VM-operation access checks are implemented.
use super::address_space::{self, AddressSpace, VmError, USER_START, USER_END};
const PAGE: u64 = 4096;
const STATUS_SUCCESS: u64 = 0;
const STATUS_NOT_IMPLEMENTED: u64 = 0xC000_0002;
const STATUS_INVALID_HANDLE: u64 = 0xC000_0008;
const STATUS_INVALID_PARAMETER: u64 = 0xC000_000D;
const STATUS_NO_MEMORY: u64 = 0xC000_0017;
const STATUS_ACCESS_VIOLATION: u64 = 0xC000_0005;
const CURRENT_PROCESS: u64 = u64::MAX;
fn status(e: VmError) -> u64 {
    match e {
        VmError::OutOfMemory => STATUS_NO_MEMORY,
        VmError::InvalidAddress | VmError::Unaligned => STATUS_INVALID_PARAMETER,
        VmError::PermissionDenied => STATUS_ACCESS_VIOLATION,
        _ => STATUS_INVALID_PARAMETER,
    }
}
fn current_root() -> Result<u64, u64> {
    let pid = crate::syscall::current_process_id();
    let process = crate::process::eprocess_pointer_for_pid(pid);
    if process.is_null() { return Err(STATUS_INVALID_HANDLE); }
    let root = unsafe { (*process).address_space_root };
    if root == 0 || root != address_space::active_root_physical() { return Err(STATUS_INVALID_HANDLE); }
    Ok(root)
}
fn attached(root: u64) -> Result<AddressSpace, u64> {
    unsafe { AddressSpace::attach_existing(root) }.map_err(status)
}
fn region_is_free(space: &AddressSpace, start: u64, length: u64) -> bool {
    let mut offset = 0;
    while offset < length {
        if space.probe_user_range(start + offset, 1, false, false).is_ok() { return false; }
        offset += PAGE;
    }
    true
}
/// Args follow the x64 NT ABI: process, base*, zeroBits, size*, type, protect.
pub fn allocate(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != CURRENT_PROCESS { return STATUS_INVALID_HANDLE; }
    if args[1] == 0 || args[3] == 0 || args[2] != 0 { return STATUS_INVALID_PARAMETER; }
    let root = match current_root() { Ok(r) => r, Err(s) => return s };
    let mut base_bytes = [0u8; 8];
    let mut size_bytes = [0u8; 8];
    if unsafe { address_space::copy_from_user_root(root,args[1],&mut base_bytes) }.is_err()
        || unsafe { address_space::copy_from_user_root(root,args[3],&mut size_bytes) }.is_err() { return STATUS_ACCESS_VIOLATION; }
    let requested = u64::from_ne_bytes(base_bytes);
    let size = u64::from_ne_bytes(size_bytes);
    if size == 0 || size > (256 * 1024 * 1024) || args[4] != 0x3000 { return STATUS_INVALID_PARAMETER; }
    let rounded = match size.checked_add(PAGE-1) { Some(v) => v & !(PAGE-1), None => return STATUS_INVALID_PARAMETER };
    let (start, end) = if requested != 0 {
        let start = requested & !(PAGE-1);
        let Some(end) = start.checked_add(rounded) else { return STATUS_INVALID_PARAMETER };
        if start < USER_START || end > USER_END { return STATUS_INVALID_PARAMETER; }
        let space = match attached(root) { Ok(s)=>s, Err(s)=>return s };
        if !region_is_free(&space,start,rounded) { return 0xC000_0018; } // STATUS_CONFLICTING_ADDRESSES
        (start,end)
    } else {
        let mut candidate = 0x0000_0001_0000_0000u64;
        let mut found = None;
        while candidate.checked_add(rounded).is_some_and(|e| e < USER_END) {
            let space = match attached(root) { Ok(s)=>s, Err(s)=>return s };
            if region_is_free(&space,candidate,rounded) {
                found = Some((candidate,candidate+rounded)); break;
            }
            candidate = match candidate.checked_add(rounded.max(0x10000)) { Some(v)=>v, None=>break };
        }
        let Some(pair)=found else { return STATUS_NO_MEMORY };
        pair
    };
    let writable = matches!(args[5] as u32, 0x04 | 0x40 | 0x08 | 0x80);
    let executable = matches!(args[5] as u32, 0x10 | 0x20 | 0x40 | 0x80);
    if args[5] & !0xFF != 0 || !matches!(args[5] as u32,0x02|0x04|0x08|0x10|0x20|0x40|0x80) || writable && executable {
        return 0xC000_0045; // STATUS_INVALID_PAGE_PROTECTION
    }
    let space = match attached(root) { Ok(s)=>s, Err(s)=>return s };
    if let Err(e)=space.register_demand_zero_region(start,end-start,writable,executable) { return status(e); }
    let out_base=start.to_ne_bytes(); let out_size=(end-start).to_ne_bytes();
    if unsafe { address_space::copy_to_user_root(root,args[1],&out_base) }.is_err()
        || unsafe { address_space::copy_to_user_root(root,args[3],&out_size) }.is_err() {
        let _=space.unregister_demand_zero_region(start,end-start);
        return STATUS_ACCESS_VIOLATION;
    }
    STATUS_SUCCESS
}
/// Read memory from the current process into its caller's writable user buffer.
pub fn read(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0]!=CURRENT_PROCESS { return STATUS_INVALID_HANDLE; }
    let length=args[3] as usize;
    if length>16*1024*1024 { return STATUS_INVALID_PARAMETER; }
    let root=match current_root(){Ok(r)=>r,Err(s)=>return s};
    let mut copied=0usize;
    let mut scratch=[0u8;4096];
    while copied<length {
        let n=(length-copied).min(scratch.len());
        let src=match args[1].checked_add(copied as u64){Some(v)=>v,None=>return STATUS_ACCESS_VIOLATION};
        let dst=match args[2].checked_add(copied as u64){Some(v)=>v,None=>return STATUS_ACCESS_VIOLATION};
        if unsafe{address_space::copy_from_user_root(root,src,&mut scratch[..n])}.is_err()
            || unsafe{address_space::copy_to_user_root(root,dst,&scratch[..n])}.is_err() {
            if args[4]!=0 { let _=unsafe{address_space::copy_to_user_root(root,args[4],&(copied as usize).to_ne_bytes())}; }
            return STATUS_ACCESS_VIOLATION;
        }
        copied+=n;
    }
    if args[4]!=0 && unsafe{address_space::copy_to_user_root(root,args[4],&copied.to_ne_bytes())}.is_err(){return STATUS_ACCESS_VIOLATION}
    STATUS_SUCCESS
}
/// Write caller-buffer bytes into the current process's writable user memory.
pub fn write(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0]!=CURRENT_PROCESS { return STATUS_INVALID_HANDLE; }
    let length=args[3] as usize;
    if length>16*1024*1024 { return STATUS_INVALID_PARAMETER; }
    let root=match current_root(){Ok(r)=>r,Err(s)=>return s};
    let mut copied=0usize; let mut scratch=[0u8;4096];
    while copied<length {
        let n=(length-copied).min(scratch.len());
        let src=match args[2].checked_add(copied as u64){Some(v)=>v,None=>return STATUS_ACCESS_VIOLATION};
        let dst=match args[1].checked_add(copied as u64){Some(v)=>v,None=>return STATUS_ACCESS_VIOLATION};
        if unsafe{address_space::copy_from_user_root(root,src,&mut scratch[..n])}.is_err()
            || unsafe{address_space::copy_to_user_root(root,dst,&scratch[..n])}.is_err() {
            if args[4]!=0 { let _=unsafe{address_space::copy_to_user_root(root,args[4],&(copied as usize).to_ne_bytes())}; }
            return STATUS_ACCESS_VIOLATION;
        }
        copied+=n;
    }
    if args[4]!=0 && unsafe{address_space::copy_to_user_root(root,args[4],&copied.to_ne_bytes())}.is_err(){return STATUS_ACCESS_VIOLATION}
    STATUS_SUCCESS
}
pub fn free(_args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 { STATUS_NOT_IMPLEMENTED }
pub fn protect(_args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 { STATUS_NOT_IMPLEMENTED }
pub fn query(_args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 { STATUS_NOT_IMPLEMENTED }

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
    if space.demand_area_overlaps(start, length) { return false; }
    let mut offset = 0;
    while offset < length {
        // Reservation metadata alone misses resident pages such as the PEB,
        // TEB, stacks, and loader data. Inspect the actual user page tables too.
        match space.translate_user(start + offset, false) {
            Ok(_) => return false,
            Err(crate::ntoskrnl::mm::address_space::VmError::NotMapped) => {}
            Err(_) => return false,
        }
        offset += PAGE;
    }
    true
}
/// Args follow the x64 NT ABI: process, base*, zeroBits, size*, type, protect.
pub fn allocate(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != CURRENT_PROCESS {
        unsafe {
            crate::serial_write(b"[DBG][MM][NtAllocateVirtualMemory] invalid process handle=");
            crate::serial_write_hex(args[0]);
            crate::serial_write(b"\r\n");
        }
        return STATUS_INVALID_HANDLE;
    }
    if args[1] == 0 || args[3] == 0 || args[2] != 0 {
        unsafe {
            crate::serial_write(b"[DBG][MM][NtAllocateVirtualMemory] invalid base/size/zeroBits=");
            for value in [args[1], args[3], args[2]] {
                crate::serial_write_hex(value);
                crate::serial_write(b" ");
            }
            crate::serial_write(b"\r\n");
        }
        return STATUS_INVALID_PARAMETER;
    }
    let root = match current_root() { Ok(r) => r, Err(s) => return s };
    let mut base_bytes = [0u8; 8];
    let mut size_bytes = [0u8; 8];
    if unsafe { address_space::copy_from_user_root(root,args[1],&mut base_bytes) }.is_err()
        || unsafe { address_space::copy_from_user_root(root,args[3],&mut size_bytes) }.is_err() { return STATUS_ACCESS_VIOLATION; }
    let requested = u64::from_ne_bytes(base_bytes);
    let size = u64::from_ne_bytes(size_bytes);
    if size == 0 || size > (256 * 1024 * 1024) || args[4] != 0x3000 {
        unsafe {
            crate::serial_write(b"[DBG][MM][NtAllocateVirtualMemory] invalid size/allocationType=");
            crate::serial_write_hex(size as u64);
            crate::serial_write(b" ");
            crate::serial_write_hex(args[4]);
            crate::serial_write(b" protect=");
            crate::serial_write_hex(args[5]);
            crate::serial_write(b"\r\n");
        }
        return STATUS_INVALID_PARAMETER;
    }
    let rounded = match size.checked_add(PAGE-1) { Some(v) => v & !(PAGE-1), None => return STATUS_INVALID_PARAMETER };
    let (start, end) = if requested != 0 {
        let start = requested & !(PAGE-1);
        let Some(end) = start.checked_add(rounded) else { return STATUS_INVALID_PARAMETER };
        if start < USER_START || end > USER_END { return STATUS_INVALID_PARAMETER; }
        let space = match attached(root) { Ok(s)=>s, Err(s)=>return s };
        if !region_is_free(&space,start,rounded) { return 0xC000_0018; } // STATUS_CONFLICTING_ADDRESSES
        (start,end)
    } else {
        // Start inside CognacOS's canonical Windows user VA range; low 4 GiB
        // addresses are intentionally not user-mappable in this layout.
        let mut candidate = USER_START;
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
    if let Err(e)=space.register_demand_zero_region(start,end-start,writable,executable,args[5] as u32) {
        let code = status(e);
        unsafe {
            crate::serial_write(b"[DBG][MM][NtAllocateVirtualMemory] reservation rejected base/size/status=");
            crate::serial_write_hex(start);
            crate::serial_write(b" ");
            crate::serial_write_hex(end-start);
            crate::serial_write(b" ");
            crate::serial_write_hex(code);
            crate::serial_write(b"\r\n");
        }
        return code;
    }
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

const STATUS_CONFLICTING_ADDRESSES: u64 = 0xC000_0018;
const STATUS_INVALID_PAGE_PROTECTION: u64 = 0xC000_0045;
const STATUS_INFO_LENGTH_MISMATCH: u64 = 0xC000_0004;


const STATUS_LOCK_NOT_GRANTED: u64 = 0xC000_0055;
const MAP_PROCESS: u64 = 1;
const MAX_LOCKED_VM_PAGES: usize = 4096;
const MAX_VM_LOCK_RANGE_PAGES: usize = 256;
#[derive(Clone, Copy)]
struct NativeVmLockedPage { root: u64, va: u64, physical_page: u64, live: bool }
impl NativeVmLockedPage { const EMPTY: Self = Self { root: 0, va: 0, physical_page: 0, live: false }; }
static NATIVE_VM_LOCKED_PAGES: crate::hal::IrqSpinLock<[NativeVmLockedPage; MAX_LOCKED_VM_PAGES]> =
    crate::hal::IrqSpinLock::new([NativeVmLockedPage::EMPTY; MAX_LOCKED_VM_PAGES]);

/// Pin resident pages in the current process. The kernel currently rejects
/// nonresident pages instead of faulting them in; successful locks prevent
/// physical-frame reclamation/page-out until the matching unlock call.
pub fn lock(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != CURRENT_PROCESS { return STATUS_INVALID_HANDLE; }
    if args[1] == 0 || args[2] == 0 || args[3] != MAP_PROCESS { return STATUS_INVALID_PARAMETER; }
    let root = match current_root() { Ok(r) => r, Err(s) => return s };
    let base = match read_u64(root, args[1]) { Ok(v) => v, Err(s) => return s };
    let size = match read_u64(root, args[2]) { Ok(v) => v, Err(s) => return s };
    let (start, end) = match round_range(base, size) { Ok(v) => v, Err(s) => return s };
    let pages = ((end - start) / PAGE) as usize;
    if pages == 0 || pages > MAX_VM_LOCK_RANGE_PAGES { return STATUS_INVALID_PARAMETER; }
    if unsafe { address_space::copy_to_user_root(root, args[1], &base.to_ne_bytes()) }.is_err()
        || unsafe { address_space::copy_to_user_root(root, args[2], &size.to_ne_bytes()) }.is_err() {
        return STATUS_ACCESS_VIOLATION;
    }
    let space = match attached(root) { Ok(s) => s, Err(s) => return s };
    let Some((region_start, region_end, _, _, _, _, _, reserved, committed)) = space.query_region(start) else { return STATUS_INVALID_PARAMETER };
    if !reserved || !committed || start < region_start || end > region_end { return STATUS_INVALID_PARAMETER; }
    let mut physical_pages = [0u64; MAX_VM_LOCK_RANGE_PAGES];
    let mut va = start;
    for slot in physical_pages.iter_mut().take(pages) {
        let physical = match space.translate_user(va, false) { Ok(p) => p, Err(_) => return STATUS_LOCK_NOT_GRANTED };
        *slot = (physical & !(PAGE - 1)) / PAGE;
        va += PAGE;
    }
    let mut records = NATIVE_VM_LOCKED_PAGES.lock();
    if records.iter().filter(|p| !p.live).count() < pages { return STATUS_NO_MEMORY; }
    if !crate::ntoskrnl::mm::runtime::lock_pfns(&physical_pages[..pages]) { return STATUS_NO_MEMORY; }
    va = start;
    let mut added_indices = [usize::MAX; MAX_VM_LOCK_RANGE_PAGES];
    for (n, pfn) in physical_pages.iter().take(pages).copied().enumerate() {
        let index = records.iter().position(|p| !p.live).expect("preflighted native VM lock slots");
        records[index] = NativeVmLockedPage { root, va, physical_page: pfn * PAGE, live: true };
        added_indices[n] = index;
        va += PAGE;
    }
    if write_u64(root, args[1], start).is_err() || write_u64(root, args[2], end - start).is_err() {
        // These pointers were prevalidated above; a concurrent unmap race must
        // roll back only this call's records, not earlier locks on the same VA.
        for index in added_indices.iter().take(pages).copied() { records[index] = NativeVmLockedPage::EMPTY; }
        crate::ntoskrnl::mm::runtime::unlock_pfns(&physical_pages[..pages]);
        return STATUS_ACCESS_VIOLATION;
    }
    STATUS_SUCCESS
}

/// Release one matching lock reference for every page in the requested range.
pub fn unlock(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != CURRENT_PROCESS { return STATUS_INVALID_HANDLE; }
    if args[1] == 0 || args[2] == 0 || args[3] != MAP_PROCESS { return STATUS_INVALID_PARAMETER; }
    let root = match current_root() { Ok(r) => r, Err(s) => return s };
    let base = match read_u64(root, args[1]) { Ok(v) => v, Err(s) => return s };
    let size = match read_u64(root, args[2]) { Ok(v) => v, Err(s) => return s };
    let (start, end) = match round_range(base, size) { Ok(v) => v, Err(s) => return s };
    let pages = ((end - start) / PAGE) as usize;
    if pages == 0 || pages > MAX_VM_LOCK_RANGE_PAGES { return STATUS_INVALID_PARAMETER; }
    if unsafe { address_space::copy_to_user_root(root, args[1], &start.to_ne_bytes()) }.is_err()
        || unsafe { address_space::copy_to_user_root(root, args[2], &(end - start).to_ne_bytes()) }.is_err() {
        return STATUS_ACCESS_VIOLATION;
    }
    let mut records = NATIVE_VM_LOCKED_PAGES.lock();
    let mut indices = [usize::MAX; MAX_VM_LOCK_RANGE_PAGES];
    let mut pfns = [0u64; MAX_VM_LOCK_RANGE_PAGES];
    for i in 0..pages {
        let va = start + i as u64 * PAGE;
        let Some((index, record)) = records.iter().enumerate().find(|(_, p)| p.live && p.root == root && p.va == va) else {
            return STATUS_INVALID_PARAMETER;
        };
        indices[i] = index;
        pfns[i] = record.physical_page / PAGE;
    }
    crate::ntoskrnl::mm::runtime::unlock_pfns(&pfns[..pages]);
    for index in indices.iter().take(pages).copied() { records[index] = NativeVmLockedPage::EMPTY; }
    if write_u64(root, args[1], start).is_err() || write_u64(root, args[2], end - start).is_err() {
        return STATUS_ACCESS_VIOLATION;
    }
    STATUS_SUCCESS
}

const MEM_DECOMMIT: u64 = 0x4000;
const MEM_RELEASE: u64 = 0x8000;
const PAGE_READONLY: u32 = 0x02;
const PAGE_READWRITE: u32 = 0x04;
const PAGE_EXECUTE: u32 = 0x10;
const PAGE_EXECUTE_READ: u32 = 0x20;
const PAGE_GUARD: u32 = 0x100;
const PAGE_NOCACHE: u32 = 0x200;
const PAGE_WRITECOMBINE: u32 = 0x400;

fn read_u64(root: u64, address: u64) -> Result<u64, u64> {
    let mut bytes = [0u8; 8];
    unsafe { address_space::copy_from_user_root(root, address, &mut bytes) }
        .map_err(|_| STATUS_ACCESS_VIOLATION)?;
    Ok(u64::from_ne_bytes(bytes))
}
fn write_u64(root: u64, address: u64, value: u64) -> Result<(), u64> {
    unsafe { address_space::copy_to_user_root(root, address, &value.to_ne_bytes()) }
        .map_err(|_| STATUS_ACCESS_VIOLATION)
}
fn write_u32(root: u64, address: u64, value: u32) -> Result<(), u64> {
    unsafe { address_space::copy_to_user_root(root, address, &value.to_ne_bytes()) }
        .map_err(|_| STATUS_ACCESS_VIOLATION)
}
fn round_range(base: u64, size: u64) -> Result<(u64, u64), u64> {
    if size == 0 { return Err(STATUS_INVALID_PARAMETER); }
    let start = base & !(PAGE - 1);
    let offset = base.checked_sub(start).ok_or(STATUS_INVALID_PARAMETER)?;
    let covered = offset.checked_add(size).ok_or(STATUS_INVALID_PARAMETER)?;
    let rounded = covered.checked_add(PAGE - 1).ok_or(STATUS_INVALID_PARAMETER)? & !(PAGE - 1);
    let end = start.checked_add(rounded).ok_or(STATUS_INVALID_PARAMETER)?;
    if start < USER_START || end > USER_END { return Err(STATUS_INVALID_PARAMETER); }
    Ok((start, end))
}
fn nt_protection(value: u64) -> Result<(bool, bool, u32), u64> {
    if value > u32::MAX as u64 { return Err(STATUS_INVALID_PAGE_PROTECTION); }
    let value = value as u32;
    if value & (PAGE_GUARD | PAGE_NOCACHE | PAGE_WRITECOMBINE) != 0 {
        return Err(STATUS_INVALID_PAGE_PROTECTION);
    }
    let (write, execute) = match value {
        PAGE_READONLY => (false, false),
        PAGE_READWRITE => (true, false),
        PAGE_EXECUTE => (false, true),
        PAGE_EXECUTE_READ => (false, true),
        _ => return Err(STATUS_INVALID_PAGE_PROTECTION),
    };
    Ok((write, execute, value))
}
/// NT_FREE_VIRTUAL_MEMORY: only the current process is supported. MEM_RELEASE
/// releases a complete reservation and requires RegionSize=0; MEM_DECOMMIT
/// decommits a page-rounded subrange while preserving the reservation.
pub fn free(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != CURRENT_PROCESS { return STATUS_INVALID_HANDLE; }
    if args[1] == 0 || args[2] == 0 { return STATUS_INVALID_PARAMETER; }
    if args[3] != MEM_RELEASE && args[3] != MEM_DECOMMIT { return STATUS_INVALID_PARAMETER; }
    let root = match current_root() { Ok(r) => r, Err(s) => return s };
    let base = match read_u64(root, args[1]) { Ok(v) => v, Err(s) => return s };
    let size = match read_u64(root, args[2]) { Ok(v) => v, Err(s) => return s };
    if unsafe { address_space::copy_to_user_root(root, args[1], &base.to_ne_bytes()) }.is_err()
        || unsafe { address_space::copy_to_user_root(root, args[2], &size.to_ne_bytes()) }.is_err() { return STATUS_ACCESS_VIOLATION; }
    let mut space = match attached(root) { Ok(s) => s, Err(s) => return s };
    let (start, length) = if args[3] == MEM_RELEASE {
        if size != 0 { return STATUS_INVALID_PARAMETER; }
        let Some((region_start, region_end, allocation_base, _allocation_protect, _current_protect, _, _, reserved, _committed)) = space.query_region(base) else { return STATUS_INVALID_PARAMETER };
        if !reserved || base != allocation_base { return STATUS_INVALID_PARAMETER; }
        (region_start, region_end - region_start)
    } else {
        let (start, end) = match round_range(base, size) { Ok(v) => v, Err(s) => return s };
        let Some((region_start, region_end, _, _, _, _, _, reserved, committed)) = space.query_region(start) else { return STATUS_INVALID_PARAMETER };
        if !reserved || !committed || start < region_start || end > region_end { return STATUS_INVALID_PARAMETER; }
        (start, end - start)
    };
    if let Err(e) = space.decommit_demand_region(start, length, args[3] == MEM_RELEASE) { return status(e); }
    if let Err(s) = write_u64(root, args[1], start) { return s; }
    if let Err(s) = write_u64(root, args[2], if args[3] == MEM_RELEASE { 0 } else { length }) { return s; }
    STATUS_SUCCESS
}

/// NT_PROTECT_VIRTUAL_MEMORY currently changes protection for a complete
/// tracked reservation; partial-reservation metadata splitting is not yet supported.
pub fn protect(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != CURRENT_PROCESS { return STATUS_INVALID_HANDLE; }
    if args[1] == 0 || args[2] == 0 { return STATUS_INVALID_PARAMETER; }
    let (writable, executable, normalized) = match nt_protection(args[3]) { Ok(v) => v, Err(s) => return s };
    if writable && executable { return STATUS_INVALID_PAGE_PROTECTION; }
    let root = match current_root() { Ok(r) => r, Err(s) => return s };
    let base = match read_u64(root, args[1]) { Ok(v) => v, Err(s) => return s };
    let size = match read_u64(root, args[2]) { Ok(v) => v, Err(s) => return s };
    let (start, end) = match round_range(base, size) { Ok(v) => v, Err(s) => return s };
    // Validate all output pointers before changing PTEs or region metadata.
    if unsafe { address_space::copy_to_user_root(root, args[1], &base.to_ne_bytes()) }.is_err()
        || unsafe { address_space::copy_to_user_root(root, args[2], &size.to_ne_bytes()) }.is_err() {
        return STATUS_ACCESS_VIOLATION;
    }
    if args[4] != 0 {
        let mut old_bytes = [0u8; 4];
        if unsafe { address_space::copy_from_user_root(root, args[4], &mut old_bytes) }.is_err()
            || unsafe { address_space::copy_to_user_root(root, args[4], &old_bytes) }.is_err() {
            return STATUS_ACCESS_VIOLATION;
        }
    }
    let mut space = match attached(root) { Ok(s) => s, Err(s) => return s };
    let Some((region_start, region_end, _, _allocation_protect, old_current, old_write, old_exec, reserved, committed)) = space.query_region(start) else { return STATUS_INVALID_PARAMETER };
    if !reserved || !committed || start != region_start || end != region_end { return STATUS_INVALID_PARAMETER; }
    let old = old_current;
    // Keep the rollback journal small enough for a kernel stack and reject oversized ranges before any PTE changes.
    if (end - start) / PAGE > 512 { return STATUS_INVALID_PARAMETER; }
    let mut changed = [0u64; 512];
    let mut changed_count = 0usize;
    let mut va = start;
    while va < end {
        match space.translate_user(va, false) {
            Ok(_) => {
                if changed_count == changed.len() { return STATUS_INVALID_PARAMETER; }
                if let Err(e) = unsafe { space.protect_user_page(va, writable, executable) } {
                    for old_va in changed[..changed_count].iter().copied() {
                        let _ = unsafe { space.protect_user_page(old_va, old_write, old_exec) };
                    }
                    return status(e);
                }
                changed[changed_count] = va;
                changed_count += 1;
            }
            Err(VmError::NotMapped) => {},
            Err(e) => return status(e),
        }
        va += PAGE;
    }
    if let Err(e) = space.set_demand_region_protection(start, end - start, writable, executable, normalized) {
        for old_va in changed[..changed_count].iter().copied() {
            let _ = unsafe { space.protect_user_page(old_va, old_write, old_exec) };
        }
        return status(e);
    }
    if let Err(s) = write_u64(root, args[1], start) { return s; }
    if let Err(s) = write_u64(root, args[2], end - start) { return s; }
    if args[4] != 0 {
        let old_bytes = old.to_ne_bytes();
        if unsafe { address_space::copy_to_user_root(root, args[4], &old_bytes) }.is_err() { return STATUS_ACCESS_VIOLATION; }
    }
    let _ = normalized;
    STATUS_SUCCESS
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MemoryBasicInformation {
    base_address: u64,
    allocation_base: u64,
    allocation_protect: u32,
    alignment: u32,
    region_size: u64,
    state: u32,
    protect: u32,
    kind: u32,
    padding: u32,
}
const MEM_COMMIT: u32 = 0x1000;
const MEM_FREE: u32 = 0x10000;
const MEM_PRIVATE: u32 = 0x20000;
const _: [(); 48] = [(); core::mem::size_of::<MemoryBasicInformation>()];

/// NT_QUERY_VIRTUAL_MEMORY (MemoryBasicInformation class). Returns the region
/// containing the requested address, including the surrounding free range.
pub fn query(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != CURRENT_PROCESS { return STATUS_INVALID_HANDLE; }
    if args[2] != 0 { return 0xC000_0003; } // STATUS_INVALID_INFO_CLASS
    if args[3] == 0 { return STATUS_ACCESS_VIOLATION; }
    if args[4] < core::mem::size_of::<MemoryBasicInformation>() as u64 {
        if args[5] != 0 { if let Ok(root) = current_root() { let _ = write_u64(root, args[5], core::mem::size_of::<MemoryBasicInformation>() as u64); } }
        return STATUS_INFO_LENGTH_MISMATCH;
    }
    let root = match current_root() { Ok(r) => r, Err(s) => return s };
    if args[1] < USER_START || args[1] >= USER_END { return STATUS_INVALID_PARAMETER; }
    let space = match attached(root) { Ok(s) => s, Err(s) => return s };
    let Some((start, end, allocation_base, allocation_protect, protection, _writable, _executable, reserved, committed)) = space.query_region(args[1]) else { return STATUS_INVALID_PARAMETER };
    let info = MemoryBasicInformation {
        base_address: start,
        allocation_base,
        allocation_protect: if reserved { allocation_protect } else { 0 },
        alignment: 0,
        region_size: end - start,
        state: if committed { MEM_COMMIT } else if reserved { 0x2000 } else { MEM_FREE },
        protect: if committed { protection } else { 0 },
        kind: if reserved { MEM_PRIVATE } else { 0 },
        padding: 0,
    };
    let bytes = unsafe { core::slice::from_raw_parts((&info as *const MemoryBasicInformation).cast::<u8>(), core::mem::size_of::<MemoryBasicInformation>()) };
    if unsafe { address_space::copy_to_user_root(root, args[3], bytes) }.is_err() { return STATUS_ACCESS_VIOLATION; }
    if args[5] != 0 {
        if let Err(s) = write_u64(root, args[5], core::mem::size_of::<MemoryBasicInformation>() as u64) { return s; }
    }
    STATUS_SUCCESS
}


fn physical_user_range(root: u64, base: u64, length: u64, require_write: bool) -> Result<u64,u64> {
    if length == 0 || base < USER_START || base.checked_add(length).is_none_or(|end| end > USER_END) { return Err(STATUS_INVALID_PARAMETER); }
    let space = attached(root)?;
    space.probe_user_range(base,length,require_write,false).map_err(status)?;
    let first = space.translate_user(base,require_write).map_err(status)?;
    let first_page = first & !(PAGE-1);
    let start_va = base & !(PAGE-1);
    let end_va = (base + length - 1) & !(PAGE-1);
    let mut va = start_va;
    loop {
        let physical = space.translate_user(va,require_write).map_err(status)? & !(PAGE-1);
        let expected = first_page.checked_add(va - start_va).ok_or(STATUS_INVALID_PARAMETER)?;
        if physical != expected { return Err(STATUS_INVALID_PARAMETER); }
        if va == end_va { break; }
        va = va.checked_add(PAGE).ok_or(STATUS_INVALID_PARAMETER)?;
    }
    Ok(first)
}

/// Acquire a persistence token only when the caller's mapped user pages are
/// contiguous and firmware explicitly marked the physical range EFI_MEMORY_NV.
pub fn get_nonvolatile_token(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    let root = match current_root() { Ok(r)=>r, Err(s)=>return s };
    let base = args[0]; let length = args[1];
    if base & (PAGE-1) != 0 || length == 0 { return STATUS_INVALID_PARAMETER; }
    let physical = match physical_user_range(root,base,length,true) { Ok(p)=>p, Err(s)=>return s };
    let mut token = 0u64;
    let result = crate::ntoskrnl::mm::persistent::get_token(physical,length,root,&mut token);
    if result < 0 { return result as u32 as u64; }
    if let Err(s) = write_u64(root,args[2],token) {
        let _ = crate::ntoskrnl::mm::persistent::free_token(token,root);
        return s;
    }
    STATUS_SUCCESS
}
pub fn free_nonvolatile_token(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    crate::ntoskrnl::mm::persistent::free_token(args[0],current_root().unwrap_or(0)) as u32 as u64
}
pub fn flush_nonvolatile_memory(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    let root = match current_root() { Ok(r)=>r, Err(s)=>return s };
    let length = args[2];
    if args[3] & !1 != 0 { return STATUS_INVALID_PARAMETER; }
    let physical = match physical_user_range(root,args[1],length,false) { Ok(p)=>p, Err(s)=>return s };
    let offset = match crate::ntoskrnl::mm::persistent::offset_for_physical(args[0],physical,length,root) { Ok(v)=>v, Err(s)=>return s as u32 as u64 };
    crate::ntoskrnl::mm::persistent::flush(args[0],root,offset,length,args[3] & 1 != 0) as u32 as u64
}
pub fn write_nonvolatile_memory(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    let root = match current_root() { Ok(r)=>r, Err(s)=>return s };
    let length = args[3] as usize;
    if length == 0 || length > 16*1024*1024 || args[4] != 0 { return STATUS_INVALID_PARAMETER; }
    let physical = match physical_user_range(root,args[1],length as u64,true) { Ok(p)=>p, Err(s)=>return s };
    let offset = match crate::ntoskrnl::mm::persistent::offset_for_physical(args[0],physical,length as u64,root) { Ok(v)=>v, Err(s)=>return s as u32 as u64 };
    let mut copied = 0usize;
    let mut scratch = [0u8;4096];
    while copied < length {
        let count = (length-copied).min(scratch.len());
        let src = match args[2].checked_add(copied as u64) { Some(v)=>v, None=>return STATUS_ACCESS_VIOLATION };
        if unsafe { address_space::copy_from_user_root(root,src,&mut scratch[..count]) }.is_err() { return STATUS_ACCESS_VIOLATION; }
        let result = crate::ntoskrnl::mm::persistent::write(args[0],root,offset+copied as u64,&scratch[..count],false);
        if result < 0 { return result as u32 as u64; }
        copied += count;
    }
    STATUS_SUCCESS
}
pub fn fill_nonvolatile_memory(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    let root = match current_root() { Ok(r)=>r, Err(s)=>return s };
    let length = args[2];
    let flags = args[4];
    if length == 0 || length > 16*1024*1024 || flags & !0x0f != 0 { return STATUS_INVALID_PARAMETER; }
    let no_drain = flags & 8 != 0;
    if no_drain && (flags & 1 == 0 || flags & (2 | 4) != 0) { return STATUS_INVALID_PARAMETER; }
    let physical = match physical_user_range(root,args[1],length,true) { Ok(p)=>p, Err(s)=>return s };
    let offset = match crate::ntoskrnl::mm::persistent::offset_for_physical(args[0],physical,length,root) { Ok(v)=>v, Err(s)=>return s as u32 as u64 };
    let result = crate::ntoskrnl::mm::persistent::fill(args[0],root,offset,length,args[3] as u8,false);
    if result < 0 { return result as u32 as u64; }
    if flags & (1 | 4) != 0 { crate::ntoskrnl::mm::persistent::flush(args[0],root,offset,length,no_drain) as u32 as u64 } else { STATUS_SUCCESS }
}


/// Check membership against the effective thread/process token. This private
/// bridge intentionally rejects non-null token handles until the object manager
/// provides a real user-handle-to-token reference path.
pub fn check_token_membership(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != 0 { return STATUS_INVALID_HANDLE; }
    if args[1] == 0 || args[2] == 0 || args[3] != 0 { return STATUS_INVALID_PARAMETER; }
    let root = match current_root() { Ok(r)=>r, Err(s)=>return s };
    let mut sid = [0u8;68];
    if unsafe { address_space::copy_from_user_root(root,args[1],&mut sid[..8]) }.is_err() { return STATUS_ACCESS_VIOLATION; }
    if sid[0] != 1 || sid[1] > 15 { return STATUS_INVALID_PARAMETER; }
    let sid_length = 8 + sid[1] as usize * 4;
    if unsafe { address_space::copy_from_user_root(root,args[1],&mut sid[..sid_length]) }.is_err() { return STATUS_ACCESS_VIOLATION; }
    let mut token = crate::thread::reference_current_impersonation_token();
    if token.is_null() { token = crate::process::reference_primary_token(crate::thread::current_process_id()); }
    if token.is_null() { return 0xC000_007Cu32 as u64; } // STATUS_NO_TOKEN
    let member = crate::ntoskrnl::se::runtime::se_internal_TokenContainsSid(token,&sid[..sid_length]);
    let _ = unsafe { crate::ntoskrnl::se::runtime::se_internal_DereferenceToken(token) };
    if unsafe { address_space::copy_to_user_root(root,args[2],&(member as u8).to_ne_bytes()) }.is_err() { return STATUS_ACCESS_VIOLATION; }
    STATUS_SUCCESS
}

/// Create a bounded pagefile-backed section object for in-kernel and RTL stream use.
pub fn create_section(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    crate::ntoskrnl::mm::section_handles::create(args)
}
pub fn dereference_section(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    crate::ntoskrnl::mm::section_handles::dereference(args)
}
pub fn section_copy(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    crate::ntoskrnl::mm::section_handles::copy(args)
}
pub fn reference_section(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    crate::ntoskrnl::mm::section_handles::reference(args)
}

//! ABI-correct, bounded core NT memory-manager routines.
//!
//! These helpers deliberately validate page-table translations before touching
//! memory. They are not a substitute for pageable process address spaces or a
//! complete Windows Memory Manager.
use core::ffi::c_void;

const PAGE: u64 = 4096;
const PRESENT: u64 = 1;
const WRITE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const LARGE: u64 = 1 << 7;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
const LARGE_2M_MASK: u64 = 0x000f_ffff_ffe0_0000;
const LARGE_1G_MASK: u64 = 0x000f_ffff_c000_0000;
const STATUS_SUCCESS: i32 = 0;
const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005u32 as i32;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_PARTIAL_COPY: i32 = 0x8000_000Du32 as i32;
const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;

#[inline]
fn entry_allowed(entry: u64, require_write: bool) -> bool {
    entry & PRESENT != 0 && entry & USER == 0 && (!require_write || entry & WRITE != 0)
}

/// Translate a canonical virtual address through the active x86-64 page tables.
/// The bootstrap maps the physical pages used by page tables into the low 4 GiB.
fn translate(va: u64, require_write: bool) -> Option<u64> {
    // Accept only canonical 48-bit virtual addresses.
    let high = va >> 48;
    let sign = (va >> 47) & 1;
    if (sign == 0 && high != 0) || (sign == 1 && high != 0xffff) { return None; }
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)); }
    let root = (cr3 & ADDRESS_MASK) as *const u64;
    if root.is_null() { return None; }
    let i4 = ((va >> 39) & 0x1ff) as usize;
    let i3 = ((va >> 30) & 0x1ff) as usize;
    let i2 = ((va >> 21) & 0x1ff) as usize;
    let i1 = ((va >> 12) & 0x1ff) as usize;
    let e4 = unsafe { root.add(i4).read_volatile() };
    if !entry_allowed(e4, require_write) { return None; }
    let pdpt = (e4 & ADDRESS_MASK) as *const u64;
    let e3 = unsafe { pdpt.add(i3).read_volatile() };
    if !entry_allowed(e3, require_write) { return None; }
    if e3 & LARGE != 0 {
        // Bits 13..29 are reserved in a 1 GiB leaf (bit 12 is PAT).
        if e3 & 0x0000_0000_3fff_e000 != 0 { return None; }
        return Some((e3 & LARGE_1G_MASK) | (va & ((1u64 << 30) - 1)));
    }
    let pd = (e3 & ADDRESS_MASK) as *const u64;
    let e2 = unsafe { pd.add(i2).read_volatile() };
    if !entry_allowed(e2, require_write) { return None; }
    if e2 & LARGE != 0 {
        // Bits 13..20 are reserved in a 2 MiB leaf (bit 12 is PAT).
        if e2 & 0x0000_0000_001f_e000 != 0 { return None; }
        return Some((e2 & LARGE_2M_MASK) | (va & ((1u64 << 21) - 1)));
    }
    let pt = (e2 & ADDRESS_MASK) as *const u64;
    let e1 = unsafe { pt.add(i1).read_volatile() };
    if !entry_allowed(e1, require_write) { return None; }
    Some((e1 & ADDRESS_MASK) | (va & (PAGE - 1)))
}

#[inline]
pub unsafe extern "win64" fn mm_get_physical_address(address: *const c_void) -> u64 {
    let va = address as u64;
    if va < 0xffff_8000_0000_0000 || crate::kernel_vm::is_pageable_address(va) { return 0; }
    translate(va, false).unwrap_or(0)
}

#[inline]
pub unsafe extern "win64" fn mm_get_virtual_for_physical(address: u64) -> *mut c_void {
    // Prefer the existing low identity alias; high RAM is represented only
    // through the validated, demand-mapped RAM direct-map window.
    if address < 0x1_0000_0000 && translate(address, false).is_some() {
        return address as *mut c_void;
    }
    if let Some(alias) = crate::memory::direct_map_address(address) {
        return alias as *mut c_void;
    }
    core::ptr::null_mut()
}

#[inline]
pub unsafe extern "win64" fn mm_is_address_valid(address: *const c_void) -> u8 {
    let va = address as u64;
    (va >= 0xffff_8000_0000_0000 && translate(va, false).is_some()) as u8
}

#[inline]
pub unsafe extern "win64" fn mm_is_non_paged_system_address_valid(address: *const c_void) -> u8 {
    let va = address as u64;
    if va < 0xffff_8000_0000_0000 || crate::kernel_vm::is_pageable_address(va) { return 0; }
    (translate(va, false).is_some()) as u8
}

/// MmCopyMemory(MM_COPY_ADDRESS is an 8-byte union): flags 1 = physical source,
/// flags 2 = virtual source (the documented MM_COPY_MEMORY_* flag values). Destination must be a currently writable mapping.
/// Copies stop at the first invalid translation and report the completed prefix.
pub unsafe extern "win64" fn mm_copy_memory(
    destination: *mut c_void,
    source_address: u64,
    length: u64,
    flags: u32,
    bytes_transferred: *mut u64,
) -> i32 {
    if !bytes_transferred.is_null() { unsafe { bytes_transferred.write(0); } }
    if destination.is_null() || (destination as u64) < 0xffff_8000_0000_0000 || crate::kernel_vm::is_pageable_address(destination as u64) || length == 0 || (flags != 1 && flags != 2) {
        return if length == 0 && !destination.is_null() && (flags == 1 || flags == 2) {
            STATUS_SUCCESS
        } else { STATUS_INVALID_PARAMETER };
    }
    let Some(end) = source_address.checked_add(length) else { return STATUS_INVALID_PARAMETER; };
    if flags == 1 && end > crate::memory::DIRECT_MAP_PHYSICAL_LIMIT {
        return STATUS_INVALID_PARAMETER;
    }
    if (destination as u64).checked_add(length).is_none() { return STATUS_INVALID_PARAMETER; }
    let mut done = 0u64;
    while done < length {
        let source = match source_address.checked_add(done) { Some(v) => v, None => break };
        let dst_va = match (destination as u64).checked_add(done) { Some(v) => v, None => break };
        let (src_ptr, source_page_offset) = if flags == 1 {
            // Never dereference a physical frame number as a virtual pointer.
            // Low pages may use their verified identity alias; high RAM must
            // use the RAM-only direct map and is rejected for MMIO/reserved ranges.
            let pointer = if source < 0x1_0000_0000 {
                if translate(source, false).is_none() { break; }
                source
            } else {
                match crate::memory::direct_map_address(source) { Some(v) => v, None => break }
            };
            (pointer as *const u8, source & (PAGE - 1))
        } else {
            if source < 0xffff_8000_0000_0000 || crate::kernel_vm::is_pageable_address(source) { break; }
            if translate(source, false).is_none() { break; }
            (source as *const u8, source & (PAGE - 1))
        };
        if translate(dst_va, true).is_none() { break; }
        let count = core::cmp::min(
            length - done,
            core::cmp::min(PAGE - source_page_offset, PAGE - (dst_va & (PAGE - 1))),
        ) as usize;
        if count == 0 { break; }
        unsafe { core::ptr::copy(src_ptr, dst_va as *mut u8, count); }
        done += count as u64;
    }
    if !bytes_transferred.is_null() { unsafe { bytes_transferred.write(done); } }
    if done == length { STATUS_SUCCESS } else { STATUS_PARTIAL_COPY }
}

const PAGE_SIZE: u64 = 4096;
const MDL_HEADER_SIZE: u64 = 48;
const MDL_PAGES_LOCKED: i16 = 0x0002;
const MDL_SOURCE_IS_NONPAGED_POOL: i16 = 0x0004;
const MDL_MAPPED_TO_SYSTEM_VA: i16 = 0x0001;

#[repr(C)]
struct MdlHeader {
    next: *mut MdlHeader,
    size: i16,
    flags: i16,
    _padding: u32,
    process: *mut c_void,
    mapped_system_va: *mut c_void,
    start_va: *mut c_void,
    byte_count: u32,
    byte_offset: u32,
}

fn mdl_page_count(byte_offset: u64, byte_count: u64) -> Option<u64> {
    if byte_offset >= PAGE_SIZE { return None; }
    if byte_count == 0 { return Some(0); }
    byte_offset.checked_add(byte_count)?
        .checked_add(PAGE_SIZE - 1)
        .map(|span| span / PAGE_SIZE)
}

#[inline]
fn mdl_range_is_canonical(start_va: u64, byte_offset: u64, byte_count: u64) -> bool {
    if start_va & (PAGE_SIZE-1) != 0 || byte_offset >= PAGE_SIZE { return false; }
    let Some(start) = start_va.checked_add(byte_offset) else { return false; };
    let Some(end) = start.checked_add(byte_count) else { return false; };
    let canonical = |address: u64| {
        let high = address >> 48;
        if (address >> 47) & 1 == 0 { high == 0 } else { high == 0xffff }
    };
    if !canonical(start) || (byte_count != 0 && !canonical(end-1)) { return false; }
    if start < 0x0000_8000_0000_0000 {
        end <= 0x0000_8000_0000_0000
    } else {
        start >= 0xffff_8000_0000_0000
            && (byte_count == 0 || end == 0 || end-1 >= 0xffff_8000_0000_0000)
    }
}

const MDL_LOCKED: i16 = 0x0002;
const MDL_NONPAGED_POOL: i16 = 0x0004;
const MDL_MAPPED_SYSTEM_VA: i16 = 0x0001;
const MDL_PARTIAL: i16 = 0x0010;
const MAX_LOCKED_PAGES: usize = 8192;
const MAX_OWNED_MDLS: usize = 256;
#[derive(Clone, Copy)]
struct OwnedMdl { address: u64, live: bool }
impl OwnedMdl { const EMPTY: Self = Self { address: 0, live: false }; }
static OWNED_MDLS: crate::hal::IrqSpinLock<[OwnedMdl; MAX_OWNED_MDLS]> =
    crate::hal::IrqSpinLock::new([OwnedMdl::EMPTY; MAX_OWNED_MDLS]);

fn register_owned_mdl(address: u64) -> bool {
    let mut owned = OWNED_MDLS.lock();
    let Some(slot) = owned.iter_mut().find(|entry| !entry.live) else { return false; };
    *slot = OwnedMdl { address, live: true };
    true
}
fn is_owned_mdl(address: u64) -> bool {
    OWNED_MDLS.lock().iter().any(|entry| entry.live && entry.address == address)
}
fn unregister_owned_mdl(address: u64) -> bool {
    let mut owned = OWNED_MDLS.lock();
    let Some(slot) = owned.iter_mut().find(|entry| entry.live && entry.address == address) else { return false; };
    *slot = OwnedMdl::EMPTY;
    true
}
#[derive(Clone, Copy)]
struct LockedPage { pfn: u64, references: u32, live: bool }
impl LockedPage { const EMPTY: Self = Self { pfn: 0, references: 0, live: false }; }
static LOCKED_PAGES: crate::hal::IrqSpinLock<[LockedPage; MAX_LOCKED_PAGES]> =
    crate::hal::IrqSpinLock::new([LockedPage::EMPTY; MAX_LOCKED_PAGES]);


const MAX_USER_MDL_MAPPINGS: usize = 256;
static MDL_MAPPING_LIFETIME_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
#[derive(Clone, Copy)]
struct UserMdlMapping { mdl: u64, process: u64, root: u64, base: u64, pages: usize, live: bool }
impl UserMdlMapping { const EMPTY: Self = Self { mdl: 0, process: 0, root: 0, base: 0, pages: 0, live: false }; }
static USER_MDL_MAPPINGS: crate::hal::IrqSpinLock<[UserMdlMapping; MAX_USER_MDL_MAPPINGS]> =
    crate::hal::IrqSpinLock::new([UserMdlMapping::EMPTY; MAX_USER_MDL_MAPPINGS]);

/// Used by the physical allocator to prevent recycling frames still pinned by an MDL.
pub fn is_page_locked(physical_page: u64) -> bool {
    let pfn = physical_page / PAGE_SIZE;
    LOCKED_PAGES.lock().iter().any(|p| p.live && p.pfn == pfn && p.references != 0)
}
fn lock_pfns(pfns: &[u64]) -> bool {
    let mut table = LOCKED_PAGES.lock();
    let mut new_entries = 0usize;
    for (i, pfn) in pfns.iter().copied().enumerate() {
        if pfns[..i].contains(&pfn) { continue; }
        let occurrences = pfns.iter().filter(|candidate| **candidate == pfn).count() as u32;
        if let Some(entry) = table.iter().find(|entry| entry.live && entry.pfn == pfn) {
            if entry.references.checked_add(occurrences).is_none() { return false; }
        } else { new_entries += 1; }
    }
    if table.iter().filter(|entry| !entry.live).count() < new_entries { return false; }
    for pfn in pfns.iter().copied() {
        if let Some(entry) = table.iter_mut().find(|entry| entry.live && entry.pfn == pfn) {
            entry.references += 1;
        } else if let Some(entry) = table.iter_mut().find(|entry| !entry.live) {
            *entry = LockedPage { pfn, references: 1, live: true };
        } else { return false; }
    }
    true
}
fn unlock_pfns(pfns: &[u64]) {
    let mut table = LOCKED_PAGES.lock();
    for pfn in pfns.iter().copied() {
        if let Some(entry) = table.iter_mut().find(|entry| entry.live && entry.pfn == pfn) {
            if entry.references > 1 { entry.references -= 1; }
            else { *entry = LockedPage::EMPTY; }
        }
    }
}
/// Translate a user-mode VA in the currently active process. Only the active CR3 is known.
fn translate_current_user(va: u64, require_write: bool) -> Option<u64> {
    if !crate::ntoskrnl::mm::address_space::is_user_range(va, 1) { return None; }
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)); }
    let root = (cr3 & ADDRESS_MASK) as *const u64;
    if root.is_null() { return None; }
    let indexes = [
        ((va >> 39) & 0x1ff) as usize, ((va >> 30) & 0x1ff) as usize,
        ((va >> 21) & 0x1ff) as usize, ((va >> 12) & 0x1ff) as usize,
    ];
    let mut table = root;
    for (level, index) in indexes.iter().copied().enumerate() {
        let entry = unsafe { table.add(index).read_volatile() };
        if entry & (PRESENT | USER) != (PRESENT | USER)
            || (require_write && entry & WRITE == 0) { return None; }
        if level == 3 { return Some((entry & ADDRESS_MASK) | (va & (PAGE_SIZE - 1))); }
        if entry & LARGE != 0 { return None; }
        table = (entry & ADDRESS_MASK) as *const u64;
    }
    None
}
fn mdl_header_is_writable(address: u64) -> bool {
    if address & 7 != 0 || address < 0xffff_8000_0000_0000
        || crate::kernel_vm::is_pageable_address(address) { return false; }
    let Some(end) = address.checked_add(MDL_HEADER_SIZE) else { return false; };
    let mut cursor = address;
    while cursor < end {
        if translate(cursor, true).is_none() { return false; }
        cursor = core::cmp::min(end, (cursor & !(PAGE_SIZE - 1)).saturating_add(PAGE_SIZE));
    }
    true
}
/// Build an MDL in caller storage, or allocate a nonpaged MDL when mdl is null.
pub unsafe extern "win64" fn mm_create_mdl(mdl: *mut c_void, base: *mut c_void, length: u64) -> *mut c_void {
    if base.is_null() || length > u32::MAX as u64
        || (base as u64).checked_add(length).is_none()
        || !mdl_range_is_canonical((base as u64) & !(PAGE_SIZE-1), (base as u64) & (PAGE_SIZE-1), length) {
        return core::ptr::null_mut();
    }
    let size = unsafe { mm_size_of_mdl(base, length) };
    if size == 0 || size > i16::MAX as u32 { return core::ptr::null_mut(); }
    let allocated = mdl.is_null();
    let address = if allocated {
        crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(size as usize, 0x4c444d4d, true)
    } else { mdl.cast::<u8>() };
    if address.is_null() { return core::ptr::null_mut(); }
    let raw = address as u64;
    if raw < 0xffff_8000_0000_0000 || !mdl_header_is_writable(raw) {
        if allocated { let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(address, Some(0x4c444d4d)); }
        return core::ptr::null_mut();
    }
    if allocated && !register_owned_mdl(raw) {
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(address, Some(0x4c444d4d));
        return core::ptr::null_mut();
    }
    let start = (base as u64) & !(PAGE_SIZE - 1);
    let header = unsafe { &mut *address.cast::<MdlHeader>() };
    header.next = core::ptr::null_mut();
    header.size = size as i16;
    header.flags = 0;
    header._padding = 0;
    header.process = core::ptr::null_mut();
    header.mapped_system_va = core::ptr::null_mut();
    header.start_va = start as *mut c_void;
    header.byte_count = length as u32;
    header.byte_offset = ((base as u64) & (PAGE_SIZE - 1)) as u32;
    address.cast()
}
/// Probe and pin the current process's user pages or resident supervisor pages.
pub unsafe extern "win64" fn mm_probe_and_lock_pages(mdl: *mut c_void, access_mode: i8, operation: u32) {
    let _lifetime_guard = MDL_MAPPING_LIFETIME_LOCK.lock();
    if mdl.is_null() || operation > 2 || (access_mode != 0 && access_mode != 1) { return; }
    let address = mdl as u64;
    if address < 0xffff_8000_0000_0000 || !mdl_header_is_writable(address) { return; }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    if header.flags & MDL_LOCKED != 0 || header.flags & MDL_NONPAGED_POOL != 0 { return; }
    let start = header.start_va as u64;
    let offset = header.byte_offset as u64;
    let length = header.byte_count as u64;
    let Some(pages) = mdl_page_count(offset, length) else { return; };
    let Some(span) = offset.checked_add(length) else { return; };
    if start & (PAGE_SIZE - 1) != 0 || (length != 0 && start.checked_add(span).is_none())
        || !mdl_range_is_canonical(start, offset, length) { return; }
    if length == 0 { return; }
    let required = match MDL_HEADER_SIZE.checked_add(pages.saturating_mul(8)) { Some(v) => v, None => return };
    if header.size < 0 || (header.size as u64) < required { return; }
    let pfn_address = match address.checked_add(MDL_HEADER_SIZE) { Some(v) => v, None => return };
    let pfn_end = match pfn_address.checked_add(pages.saturating_mul(8)) { Some(v) => v, None => return };
    let mut cursor = pfn_address;
    while cursor < pfn_end {
        if translate(cursor, true).is_none() { return; }
        cursor = core::cmp::min(pfn_end, (cursor & !(PAGE_SIZE - 1)).saturating_add(PAGE_SIZE));
    }
    if pages as usize > MAX_LOCKED_PAGES { return; }
    let write_access = operation != 0;
    let destination = pfn_address as *mut u64;
    for index in 0..pages as usize {
        let Some(va) = start.checked_add(index as u64 * PAGE_SIZE) else { return; };
        if access_mode == 1 && !crate::ntoskrnl::mm::address_space::is_user_range(va, 1) { return; }
        let pa = if crate::ntoskrnl::mm::address_space::is_user_range(va, 1) {
            // No user demand-paging metadata exists yet; only present user
            // mappings can be probed safely.
            translate_current_user(va, write_access)
        } else if va >= 0xffff_8000_0000_0000 {
            if crate::kernel_vm::is_pageable_address(va)
                && !crate::kernel_vm::ensure_page_resident(va, write_access) { return; }
            translate(va, write_access)
        } else { None };
        let Some(pa) = pa else { return; };
        if !crate::memory::is_ram_physical_address(pa & !(PAGE_SIZE-1)) { return; }
    }
    // Exclude pageable eviction while re-translating, writing PFNs and pinning
    // them. If a page was evicted in the gap above, fail closed.
    let _pageable_guard = crate::kernel_vm::lock_pageable_mappings();
    // Preflight every translation again while eviction is excluded, before
    // writing even the first PFN into the caller's descriptor.
    for index in 0..pages as usize {
        let va = start + index as u64 * PAGE_SIZE;
        let pa = if crate::ntoskrnl::mm::address_space::is_user_range(va, 1) {
            translate_current_user(va, write_access)
        } else { translate(va, write_access) };
        let Some(pa) = pa else { return; };
        if !crate::memory::is_ram_physical_address(pa & !(PAGE_SIZE-1)) { return; }
    }
    for index in 0..pages as usize {
        let va = start + index as u64 * PAGE_SIZE;
        let pa = if crate::ntoskrnl::mm::address_space::is_user_range(va, 1) {
            translate_current_user(va, write_access)
        } else { translate(va, write_access) };
        let Some(pa) = pa else { return; };
        unsafe { destination.add(index).write(pa / PAGE_SIZE); }
    }
    let pfns = unsafe { core::slice::from_raw_parts(pfn_address as *const u64, pages as usize) };
    if !lock_pfns(pfns) {
        for index in 0..pages as usize { unsafe { destination.add(index).write(0); } }
        return;
    }
    if crate::ntoskrnl::mm::address_space::is_user_range(start, 1) {
        let root = crate::ntoskrnl::mm::address_space::active_root_physical();
        let Some(process) = crate::process::reference_process_for_root(root) else {
            unlock_pfns(pfns);
            for index in 0..pages as usize { unsafe { destination.add(index).write(0); } }
            return;
        };
        header.process = process;
    }
    header.flags |= MDL_LOCKED;
}
/// Release physical-frame references acquired by MmProbeAndLockPages.
pub unsafe extern "win64" fn mm_unlock_pages(mdl: *mut c_void) {
    let _lifetime_guard = MDL_MAPPING_LIFETIME_LOCK.lock();
    if mdl.is_null() { return; }
    let address = mdl as u64;
    if address < 0xffff_8000_0000_0000 || !mdl_header_is_writable(address) { return; }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    if header.flags & MDL_LOCKED == 0 || header.flags & MDL_NONPAGED_POOL != 0 { return; }
    let Some(pages) = mdl_page_count(header.byte_offset as u64, header.byte_count as u64) else { return; };
    if pages as usize > MAX_LOCKED_PAGES { return; }
    let required = match MDL_HEADER_SIZE.checked_add(pages.saturating_mul(8)) { Some(v) => v, None => return };
    if header.size < 0 || (header.size as u64) < required { return; }
    let pfn_address = match address.checked_add(MDL_HEADER_SIZE) { Some(v) => v, None => return };
    if pfn_address.checked_add(pages.saturating_mul(8)).is_none() { return; }
    for index in 0..pages as usize {
        let at = match pfn_address.checked_add(index as u64 * 8) { Some(v) => v, None => return };
        if translate(at, false).is_none() { return; }
    }
    let pfns = unsafe { core::slice::from_raw_parts(pfn_address as *const u64, pages as usize) };
    if USER_MDL_MAPPINGS.lock().iter().any(|m| m.live && m.mdl == address) { return; }
    unlock_pfns(pfns);
    if !header.process.is_null() {
        let _ = crate::process::dereference_eprocess(header.process);
        header.process = core::ptr::null_mut();
    }
    header.flags &= !(MDL_LOCKED | MDL_MAPPED_SYSTEM_VA);
    header.mapped_system_va = core::ptr::null_mut();
}

/// Advance an MDL's represented byte range while releasing references to pages
/// no longer covered by a locked MDL. The backing allocation remains caller-owned.
pub unsafe extern "win64" fn mm_advance_mdl(mdl: *mut c_void, number_of_bytes: u32) {
    if mdl.is_null() { return; }
    let address = mdl as u64;
    if !mdl_header_is_writable(address) { return; }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    let old_count = header.byte_count as u64;
    if header.flags & (MDL_LOCKED | MDL_NONPAGED_POOL) == 0
        || number_of_bytes as u64 > old_count { return; }
    if number_of_bytes == 0 { return; }
    let old_offset = header.byte_offset as u64;
    let Some(old_pages) = mdl_page_count(old_offset, old_count) else { return; };
    let required = match MDL_HEADER_SIZE.checked_add(old_pages.saturating_mul(8)) { Some(v) => v, None => return };
    if header.size < 0 || (header.size as u64) < required || old_pages as usize > MAX_LOCKED_PAGES { return; }
    let pfn_address = match address.checked_add(MDL_HEADER_SIZE) { Some(v) => v, None => return };
    if pfn_address.checked_add(old_pages.saturating_mul(8)).is_none() { return; }
    for index in 0..old_pages as usize {
        if translate(pfn_address + index as u64 * 8, false).is_none() { return; }
    }
    let consumed = match old_offset.checked_add(number_of_bytes as u64) { Some(v) => v, None => return };
    let dropped = consumed / PAGE_SIZE;
    let new_offset = consumed % PAGE_SIZE;
    let new_count = old_count - number_of_bytes as u64;
    let Some(new_pages) = mdl_page_count(new_offset, new_count) else { return; };
    if dropped > old_pages || new_pages > old_pages - dropped { return; }
    let Some(new_start) = (header.start_va as u64).checked_add(dropped * PAGE_SIZE) else { return; };
    let pfns = pfn_address as *mut u64;
    if header.flags & MDL_LOCKED != 0 && header.flags & MDL_NONPAGED_POOL == 0 {
        let removed = unsafe { core::slice::from_raw_parts(pfns as *const u64, dropped as usize) };
        unlock_pfns(removed);
    }
    unsafe { core::ptr::copy(pfns.add(dropped as usize), pfns, new_pages as usize); }
    for index in new_pages as usize..old_pages as usize {
        unsafe { pfns.add(index).write(0); }
    }
    header.start_va = new_start as *mut c_void;
    header.byte_offset = new_offset as u32;
    header.byte_count = new_count as u32;
    header.flags &= !MDL_MAPPED_SYSTEM_VA;
    header.mapped_system_va = core::ptr::null_mut();
}

/// Reset a reusable MDL without releasing its caller-owned header allocation.
pub unsafe extern "win64" fn mm_prepare_mdl_for_reuse(mdl: *mut c_void) {
    if mdl.is_null() { return; }
    let address = mdl as u64;
    if address < 0xffff_8000_0000_0000 || !mdl_header_is_writable(address) { return; }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    if header.size < 0 || header.size < MDL_HEADER_SIZE as i16 { return; }
    let pages = match mdl_page_count(header.byte_offset as u64, header.byte_count as u64) {
        Some(value) => value,
        None => return,
    };
    let required = match MDL_HEADER_SIZE.checked_add(pages.saturating_mul(8)) {
        Some(value) => value,
        None => return,
    };
    if (header.size as u64) < required { return; }
    let pfn_address = match address.checked_add(MDL_HEADER_SIZE) { Some(value) => value, None => return };
    for index in 0..pages {
        let Some(at) = pfn_address.checked_add(index * 8) else { return; };
        if translate(at, true).is_none() { return; }
    }
    if header.flags & MDL_LOCKED != 0 && header.flags & MDL_NONPAGED_POOL == 0 {
        let pfns = unsafe { core::slice::from_raw_parts(pfn_address as *const u64, pages as usize) };
        unlock_pfns(pfns);
    }
    for index in 0..pages as usize {
        unsafe { ((pfn_address as *mut u64).add(index)).write(0); }
    }
    header.next = core::ptr::null_mut();
    header.flags = 0;
    header.process = core::ptr::null_mut();
    header.mapped_system_va = core::ptr::null_mut();
    // Preserve StartVa/ByteOffset/ByteCount: callers reuse the descriptor for
    // the same buffer range and may immediately probe/lock it again.
}

/// Free only MDLs allocated by MmCreateMdl; caller-owned IoAllocateMdl-style
/// storage is never returned to the pool by this routine.
pub unsafe extern "win64" fn mm_free_mdl(mdl: *mut c_void) {
    if mdl.is_null() { return; }
    let address = mdl as u64;
    if !is_owned_mdl(address) || address < 0xffff_8000_0000_0000 || !mdl_header_is_writable(address) { return; }
    let header = unsafe { &*mdl.cast::<MdlHeader>() };
    if header.flags & MDL_LOCKED != 0 && header.flags & MDL_NONPAGED_POOL == 0 {
        unsafe { mm_unlock_pages(mdl); }
    }
    let header_after = unsafe { &*mdl.cast::<MdlHeader>() };
    if header_after.flags & (MDL_LOCKED | MDL_MAPPED_SYSTEM_VA) != 0 { return; }
    if !unregister_owned_mdl(address) { return; }
    let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(mdl.cast(), Some(0x4c444d4d));
}

/// Construct a partial MDL referencing a validated subrange of a source MDL.
/// Locked source PFNs acquire independent pin references so either MDL can be
/// released first. Nonpaged-pool source buffers retain their nonpaged semantics.
pub unsafe extern "win64" fn mm_build_partial_mdl(
    source_mdl: *mut c_void, target_mdl: *mut c_void,
    virtual_address: *mut c_void, length: u32,
) {
    if source_mdl.is_null() || target_mdl.is_null() || virtual_address.is_null() { return; }
    let src_addr = source_mdl as u64;
    let dst_addr = target_mdl as u64;
    if src_addr < 0xffff_8000_0000_0000 || dst_addr < 0xffff_8000_0000_0000
        || !mdl_header_is_writable(src_addr) || !mdl_header_is_writable(dst_addr) { return; }
    let src = unsafe { &*source_mdl.cast::<MdlHeader>() };
    if src.flags & (MDL_LOCKED | MDL_NONPAGED_POOL) == 0 || src.size < 0 { return; }
    let src_start = src.start_va as u64;
    let src_offset = src.byte_offset as u64;
    let src_len = src.byte_count as u64;
    let Some(src_pages) = mdl_page_count(src_offset, src_len) else { return; };
    let src_required = match MDL_HEADER_SIZE.checked_add(src_pages.saturating_mul(8)) {
        Some(value) => value,
        None => return,
    };
    if (src.size as u64) < src_required { return; }
    let request = virtual_address as u64;
    let Some(src_begin) = src_start.checked_add(src_offset) else { return; };
    let Some(src_end) = src_begin.checked_add(src_len) else { return; };
    let Some(req_end) = request.checked_add(length as u64) else { return; };
    if request < src_begin || req_end > src_end { return; }
    let relative = request - src_begin;
    let absolute = src_offset.checked_add(relative).unwrap_or(u64::MAX);
    let first_page = absolute / PAGE_SIZE;
    let new_offset = request & (PAGE_SIZE - 1);
    let Some(pages) = mdl_page_count(new_offset, length as u64) else { return; };
    if first_page.checked_add(pages).map_or(true, |end| end > src_pages) { return; }
    let required = match MDL_HEADER_SIZE.checked_add(pages.saturating_mul(8)) { Some(v) => v, None => return };
    let dst = unsafe { &mut *target_mdl.cast::<MdlHeader>() };
    if dst_addr == src_addr || dst.size < 0 || (dst.size as u64) < required { return; }
    let dst_pfns = match dst_addr.checked_add(MDL_HEADER_SIZE) { Some(v) => v, None => return };
    for index in 0..pages {
        if translate(dst_pfns + index * 8, true).is_none() { return; }
        if translate(src_addr + MDL_HEADER_SIZE + (first_page + index) * 8, false).is_none() { return; }
    }
    for index in 0..pages {
        let source_pfn = unsafe { ((src_addr + MDL_HEADER_SIZE + (first_page + index) * 8) as *const u64).read() };
        unsafe { ((dst_pfns + index * 8) as *mut u64).write(source_pfn); }
    }
    let dst_start = request & !(PAGE_SIZE - 1);
    dst.next = core::ptr::null_mut();
    dst._padding = 0;
    dst.process = src.process;
    dst.mapped_system_va = core::ptr::null_mut();
    dst.start_va = dst_start as *mut c_void;
    dst.byte_offset = new_offset as u32;
    dst.byte_count = length;
    dst.flags = (src.flags & MDL_NONPAGED_POOL) | MDL_PARTIAL;
    if src.flags & MDL_NONPAGED_POOL != 0 {
        dst.flags |= MDL_LOCKED | MDL_MAPPED_SYSTEM_VA;
        dst.mapped_system_va = virtual_address;
    } else {
        let pfns = unsafe { core::slice::from_raw_parts(dst_pfns as *const u64, pages as usize) };
        if !lock_pfns(pfns) {
            for index in 0..pages { unsafe { ((dst_pfns + index * 8) as *mut u64).write(0); } }
            dst.flags = 0;
            dst.process = core::ptr::null_mut();
            dst.start_va = core::ptr::null_mut();
            dst.byte_count = 0;
            dst.byte_offset = 0;
            return;
        }
        dst.flags |= MDL_LOCKED;
    }
}

/// Fault-in requests are intentionally bounded to the current resident mappings;
/// the kernel has no pageable-buffer fault completion path yet.

/// Map locked PFNs through a contiguous supervisor VA range. Physical pages
/// may be non-contiguous; the mapping owns only its virtual extent, never the
/// caller's frames. User-mode mappings are rejected until per-process user VA
/// reservations and teardown tracking are available.
pub unsafe extern "win64" fn mm_map_locked_pages_specify_cache(
    mdl: *mut c_void, access_mode: i8, cache_type: i32, requested_address: *mut c_void,
    _bug_check_on_failure: u8, _priority: u32,
) -> *mut c_void {
    let _lifetime_guard = MDL_MAPPING_LIFETIME_LOCK.lock();
    if mdl.is_null() || (access_mode != 0 && access_mode != 1) || cache_type != 1 || !requested_address.is_null() {
        return core::ptr::null_mut();
    }
    let address = mdl as u64;
    if address < 0xffff_8000_0000_0000 || !mdl_header_is_writable(address) {
        return core::ptr::null_mut();
    }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    if header.flags & (MDL_LOCKED | MDL_NONPAGED_POOL) == 0 { return core::ptr::null_mut(); }
    if header.flags & MDL_MAPPED_SYSTEM_VA != 0 && !header.mapped_system_va.is_null() {
        return header.mapped_system_va;
    }
    let Some(pages) = mdl_page_count(header.byte_offset as u64, header.byte_count as u64) else {
        return core::ptr::null_mut();
    };
    if pages == 0 || pages as usize > MAX_LOCKED_PAGES { return core::ptr::null_mut(); }
    let required = match MDL_HEADER_SIZE.checked_add(pages.saturating_mul(8)) {
        Some(value) => value, None => return core::ptr::null_mut(),
    };
    if header.size < 0 || (header.size as u64) < required { return core::ptr::null_mut(); }
    let pfn_address = match address.checked_add(MDL_HEADER_SIZE) {
        Some(value) => value, None => return core::ptr::null_mut(),
    };
    let pfn_end = match pfn_address.checked_add(pages.saturating_mul(8)) {
        Some(value) => value, None => return core::ptr::null_mut(),
    };
    let mut cursor = pfn_address;
    while cursor < pfn_end {
        if translate(cursor, false).is_none() { return core::ptr::null_mut(); }
        cursor = core::cmp::min(pfn_end, (cursor & !(PAGE_SIZE-1)).saturating_add(PAGE_SIZE));
    }
    let pfns = unsafe { core::slice::from_raw_parts(pfn_address as *const u64, pages as usize) };
    for pfn in pfns.iter().copied() {
        let Some(pa) = pfn.checked_mul(PAGE_SIZE) else { return core::ptr::null_mut(); };
        if !crate::memory::is_ram_physical_address(pa) { return core::ptr::null_mut(); }
    }
    if access_mode == 1 {
        let root = crate::ntoskrnl::mm::address_space::active_root_physical();
        let process = header.process;
        if process.is_null() || !crate::process::process_matches_root(process, root) {
            return core::ptr::null_mut();
        }
        if USER_MDL_MAPPINGS.lock().iter().any(|m| m.live && m.mdl == address) {
            return core::ptr::null_mut();
        }
        let mapping = match unsafe { crate::ntoskrnl::mm::address_space::map_external_user_pages_active(pfns, true) } {
            Ok(value) => value, Err(_) => return core::ptr::null_mut(),
        };
        let Some(mapped) = mapping.checked_add(header.byte_offset as u64) else {
            let _ = unsafe { crate::ntoskrnl::mm::address_space::unmap_external_user_pages_active(mapping, pfns) };
            return core::ptr::null_mut();
        };
        let mut mappings = USER_MDL_MAPPINGS.lock();
        let Some(slot) = mappings.iter_mut().find(|m| !m.live) else {
            drop(mappings);
            let _ = unsafe { crate::ntoskrnl::mm::address_space::unmap_external_user_pages_active(mapping, pfns) };
            return core::ptr::null_mut();
        };
        *slot = UserMdlMapping { mdl: address, process: process as u64, root, base: mapping, pages: pages as usize, live: true };
        header.mapped_system_va = mapped as *mut c_void;
        header.flags |= MDL_MAPPED_SYSTEM_VA;
        return header.mapped_system_va;
    }
    let mapping = match crate::kernel_vm::map_external_pages(pfns) {
        Ok(value) => value, Err(_) => return core::ptr::null_mut(),
    };
    let Some(mapped) = (mapping as u64).checked_add(header.byte_offset as u64) else {
        let _ = crate::kernel_vm::unmap_external_pages(mapping, pages as usize);
        return core::ptr::null_mut();
    };
    header.mapped_system_va = mapped as *mut c_void;
    header.flags |= MDL_MAPPED_SYSTEM_VA;
    header.mapped_system_va
}
pub unsafe extern "win64" fn mm_map_locked_pages(mdl: *mut c_void, access_mode: i8) -> *mut c_void {
    unsafe { mm_map_locked_pages_specify_cache(mdl, access_mode, 1, core::ptr::null_mut(), 0, 0) }
}
pub unsafe extern "win64" fn mm_unmap_locked_pages(base: *mut c_void, mdl: *mut c_void) {
    let _lifetime_guard = MDL_MAPPING_LIFETIME_LOCK.lock();
    if mdl.is_null() || base.is_null() { return; }
    let address = mdl as u64;
    if address < 0xffff_8000_0000_0000 || !mdl_header_is_writable(address) { return; }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    if header.flags & MDL_MAPPED_SYSTEM_VA == 0 || header.mapped_system_va != base { return; }
    // User aliases must be torn down in the same active process/root.
    let base_va = base as u64;
    if base_va < 0x0000_8000_0000_0000 {
        let offset = header.byte_offset as u64;
        let aligned = match base_va.checked_sub(offset) { Some(v) => v, None => return };
        let root = crate::ntoskrnl::mm::address_space::active_root_physical();
        let mut mappings = USER_MDL_MAPPINGS.lock();
        let Some(index) = mappings.iter().position(|m| m.live && m.mdl == address
            && m.process == header.process as u64 && m.root == root
            && m.base == aligned && header.process as u64 != 0) else { return; };
        let record = mappings[index];
        let pfn_address = match address.checked_add(MDL_HEADER_SIZE) { Some(v) => v, None => return };
        let required = match MDL_HEADER_SIZE.checked_add((record.pages as u64).saturating_mul(8)) { Some(v) => v, None => return };
        if header.size < 0 || (header.size as u64) < required
            || mdl_page_count(header.byte_offset as u64, header.byte_count as u64) != Some(record.pages as u64) {
            return;
        }
        let pfn_end = match pfn_address.checked_add((record.pages as u64).saturating_mul(8)) { Some(v) => v, None => return };
        let mut cursor = pfn_address;
        while cursor < pfn_end {
            if translate(cursor, false).is_none() { return; }
            cursor = core::cmp::min(pfn_end, (cursor & !(PAGE_SIZE-1)).saturating_add(PAGE_SIZE));
        }
        let pfns = unsafe { core::slice::from_raw_parts(pfn_address as *const u64, record.pages) };
        if unsafe { crate::ntoskrnl::mm::address_space::unmap_external_user_pages_active(record.base, pfns) }.is_err() { return; }
        mappings[index] = UserMdlMapping::EMPTY;
        drop(mappings);
        header.flags &= !MDL_MAPPED_SYSTEM_VA;
        header.mapped_system_va = core::ptr::null_mut();
        return;
    }
    // Nonpaged-pool MDLs may describe an existing VA rather than an MM-owned
    // alias. Only release addresses inside the dedicated kernel mapping arena.
    if base_va >= 0xffff_8000_0000_0000 && crate::kernel_vm::is_pageable_address(base_va) {
        // Pageable pool extents are not external MDL mappings; don't free them.
        return;
    }
    if base_va >= 0xffff_8000_0000_0000 {
        let offset = header.byte_offset as u64;
        let aligned = match base_va.checked_sub(offset) { Some(v) => v as *mut u8, None => return };
        let pages = match mdl_page_count(offset, header.byte_count as u64) { Some(v) if v > 0 && v <= MAX_LOCKED_PAGES as u64 => v as usize, _ => return };
        if crate::kernel_vm::unmap_external_pages(aligned, pages).is_err() { return; }
    } else { return; }
    header.flags &= !MDL_MAPPED_SYSTEM_VA;
    header.mapped_system_va = core::ptr::null_mut();
}
/// Copy between registered process address spaces while pinning both EPROCESS
/// objects and holding the address-space lifetime lock. Page-table mutations
/// and teardown use the same lock, so translated frames cannot be reclaimed
/// or remapped between validation and the actual copy.
pub unsafe extern "win64" fn mm_copy_virtual_memory(
    source_process: *mut c_void, source_address: *const c_void,
    target_process: *mut c_void, target_address: *mut c_void,
    length: u64, previous_mode: i8, return_size: *mut u64,
) -> i32 {
    if !return_size.is_null() {
        let output = return_size as u64;
        if output < 0xffff_8000_0000_0000 || crate::kernel_vm::is_pageable_address(output)
            || translate(output, true).is_none() { return STATUS_INVALID_PARAMETER; }
        unsafe { return_size.write(0); }
    }
    if source_process.is_null() || target_process.is_null()
        || (source_address as u64) == 0 || (target_address as u64) == 0
        || length > 0x0000_8000_0000_0000 || (previous_mode != 0 && previous_mode != 1) {
        return STATUS_INVALID_PARAMETER;
    }
    if length == 0 { return STATUS_SUCCESS; }
    let src = source_address as u64;
    let dst = target_address as u64;
    if src.checked_add(length).is_none() || dst.checked_add(length).is_none() {
        return STATUS_INVALID_PARAMETER;
    }
    let user_only = previous_mode == 1;
    let copied = crate::process::with_registered_process_roots(
        source_process, target_process, |source_root, target_root| {
            let mut done = 0u64;
            while done < length {
                let Some(sva) = src.checked_add(done) else { break; };
                let Some(dva) = dst.checked_add(done) else { break; };
                let Some(spa) = crate::ntoskrnl::mm::address_space::translate_in_root_locked(source_root, sva, false, user_only) else { break; };
                let Some(dpa) = crate::ntoskrnl::mm::address_space::translate_in_root_locked(target_root, dva, true, user_only) else { break; };
                let Some(source_ptr) = crate::memory::physical_access_pointer(spa & !(PAGE_SIZE-1)) else { break; };
                let Some(target_ptr) = crate::memory::physical_access_pointer(dpa & !(PAGE_SIZE-1)) else { break; };
                let source_ptr = unsafe { source_ptr.add((spa & (PAGE_SIZE-1)) as usize) };
                let target_ptr = unsafe { target_ptr.add((dpa & (PAGE_SIZE-1)) as usize) };
                let count = core::cmp::min(length - done, core::cmp::min(
                    PAGE_SIZE - (sva & (PAGE_SIZE - 1)),
                    PAGE_SIZE - (dva & (PAGE_SIZE - 1)))) as usize;
                if count == 0 { break; }
                // copy permits overlap for same-process moves and avoids the
                // undefined behavior of copy_nonoverlapping for aliased pages.
                unsafe { core::ptr::copy(source_ptr, target_ptr, count); }
                done += count as u64;
            }
            done
        });
    let Some(done) = copied else { return STATUS_INVALID_PARAMETER; };
    if !return_size.is_null() { unsafe { return_size.write(done); } }
    if done == length { STATUS_SUCCESS } else { STATUS_PARTIAL_COPY }
}

/// Populate an MDL created by IoAllocateMdl for a nonpaged kernel buffer.
/// Preflights every mapping and the complete PFN destination before writing,
/// so malformed ranges never leave a partially populated PFN array.
pub unsafe extern "win64" fn mm_build_mdl_for_non_paged_pool(mdl: *mut c_void) {
    let address = mdl as u64;
    if mdl.is_null() || address & 7 != 0 || address < 0xffff_8000_0000_0000
        || crate::kernel_vm::is_pageable_address(address) {
        return;
    }
    let Some(header_end) = address.checked_add(MDL_HEADER_SIZE) else { return; };
    let mut header_cursor = address;
    while header_cursor < header_end {
        if translate(header_cursor, true).is_none() { return; }
        header_cursor = core::cmp::min(header_end,
            (header_cursor & !(PAGE_SIZE - 1)).saturating_add(PAGE_SIZE));
    }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    let start = header.start_va as u64;
    let count = header.byte_count as u64;
    let offset = header.byte_offset as u64;
    let Some(pages) = mdl_page_count(offset, count) else { return; };
    let Some(span) = offset.checked_add(count) else { return; };
    let Some(end) = start.checked_add(span) else { return; };
    if start < 0xffff_8000_0000_0000 || start & (PAGE_SIZE - 1) != 0
        || end < start || (count != 0 && crate::kernel_vm::is_pageable_address(start)) {
        return;
    }
    let pfn_bytes = match pages.checked_mul(core::mem::size_of::<u64>() as u64) {
        Some(n) => n,
        None => return,
    };
    let required_size = match MDL_HEADER_SIZE.checked_add(pfn_bytes) {
        Some(n) => n,
        None => return,
    };
    if header.size < 0 || (header.size as u64) < required_size { return; }
    let pfn_start = match address.checked_add(MDL_HEADER_SIZE) { Some(v) => v, None => return };
    let pfn_end = match pfn_start.checked_add(pfn_bytes) { Some(v) => v, None => return };
    if pfn_end < pfn_start { return; }
    let mut cursor = pfn_start;
    while cursor < pfn_end {
        if translate(cursor, true).is_none() { return; }
        cursor = core::cmp::min(pfn_end, (cursor & !(PAGE_SIZE - 1)).saturating_add(PAGE_SIZE));
    }
    for index in 0..pages {
        let Some(va) = start.checked_add(index * PAGE_SIZE) else { return; };
        if va < 0xffff_8000_0000_0000 || crate::kernel_vm::is_pageable_address(va)
            || translate(va, false).is_none() {
            return;
        }
    }
    let pfns = pfn_start as *mut u64;
    for index in 0..pages {
        let va = start + index * PAGE_SIZE;
        let Some(pa) = translate(va, false) else { return; };
        unsafe { pfns.add(index as usize).write(pa / PAGE_SIZE); }
    }
    header.mapped_system_va = (start + offset) as *mut c_void;
    header.flags |= MDL_PAGES_LOCKED | MDL_SOURCE_IS_NONPAGED_POOL | MDL_MAPPED_TO_SYSTEM_VA;
}

const MAX_CONTIGUOUS_ALLOCS: usize = 128;
#[derive(Clone, Copy)]
struct ContiguousAllocation { base: u64, pages: usize, requested: u64, live: bool }
impl ContiguousAllocation { const EMPTY: Self = Self { base: 0, pages: 0, requested: 0, live: false }; }
static CONTIGUOUS_ALLOCS: crate::hal::IrqSpinLock<[ContiguousAllocation; MAX_CONTIGUOUS_ALLOCS]> =
    crate::hal::IrqSpinLock::new([ContiguousAllocation::EMPTY; MAX_CONTIGUOUS_ALLOCS]);

fn allocate_contiguous(bytes: u64, highest: u64, boundary: u64, cache_type: u32) -> *mut c_void {
    // Only the default cached policy is implemented. Do not silently return
    // cached memory for callers that explicitly require UC or WC mappings.
    if bytes == 0 || cache_type != 1 { return core::ptr::null_mut(); }
    let Some(rounded) = bytes.checked_add(PAGE_SIZE - 1) else { return core::ptr::null_mut(); };
    let pages = (rounded / PAGE_SIZE) as usize;
    let base = match unsafe { crate::memory::allocate_contiguous_pages(pages) } {
        Ok(p) => p, Err(_) => return core::ptr::null_mut(),
    };
    let Some(end) = base.checked_add((pages as u64) * PAGE_SIZE) else {
        let _ = unsafe { crate::memory::free_contiguous_pages(base, pages) };
        return core::ptr::null_mut();
    };
    if end == 0 || end - 1 > highest
        || (boundary != 0 && base / boundary != (end - 1) / boundary)
    {
        let _ = unsafe { crate::memory::free_contiguous_pages(base, pages) };
        return core::ptr::null_mut();
    }
    let mut table = CONTIGUOUS_ALLOCS.lock();
    let Some(index) = table.iter().position(|entry| !entry.live) else {
        drop(table);
        let _ = unsafe { crate::memory::free_contiguous_pages(base, pages) };
        return core::ptr::null_mut();
    };
    table[index] = ContiguousAllocation { base, pages, requested: bytes, live: true };
    base as *mut c_void
}

#[inline]
pub unsafe extern "win64" fn mm_allocate_contiguous_memory(bytes: u64, highest: u64) -> *mut c_void {
    allocate_contiguous(bytes, highest, 0, 1)
}

#[inline]
pub unsafe extern "win64" fn mm_allocate_contiguous_memory_specify_cache(
    bytes: u64, highest: u64, boundary: u64, cache_type: u32,
) -> *mut c_void {
    allocate_contiguous(bytes, highest, boundary, cache_type)
}

unsafe fn free_contiguous(base: *mut c_void, expected_bytes: Option<u64>, cache_type: Option<u32>) {
    if base.is_null() { return; }
    let allocation = {
        let mut table = CONTIGUOUS_ALLOCS.lock();
        let Some(entry) = table.iter_mut().find(|e| e.live && e.base == base as u64) else { return; };
        if expected_bytes.map_or(false, |n| n != entry.requested)
            || cache_type.map_or(false, |c| c != 1)
            || (0..entry.pages).any(|index| is_page_locked(entry.base + index as u64 * PAGE_SIZE)) {
            return;
        }
        let found = *entry;
        *entry = ContiguousAllocation::EMPTY;
        found
    };
    let _ = unsafe { crate::memory::free_contiguous_pages(allocation.base, allocation.pages) };
}

pub unsafe extern "win64" fn mm_free_contiguous_memory(base: *mut c_void) {
    unsafe { free_contiguous(base, None, None); }
}

pub unsafe extern "win64" fn mm_free_contiguous_memory_specify_cache(
    base: *mut c_void, bytes: u64, cache_type: u32,
) {
    unsafe { free_contiguous(base, Some(bytes), Some(cache_type)); }
}

/// Calculate an x64 MDL header plus PFN array without allocating it.
/// This is useful to drivers preparing an MDL in their own nonpaged storage.
pub unsafe extern "win64" fn mm_size_of_mdl(base: *const c_void, length: u64) -> u32 {
    let offset = (base as u64) & (PAGE_SIZE - 1);
    let Some(span) = offset.checked_add(length) else { return 0; };
    let Some(rounded) = span.checked_add(PAGE_SIZE - 1) else { return 0; };
    let pages = rounded / PAGE_SIZE;
    let Some(pfn_bytes) = pages.checked_mul(core::mem::size_of::<u64>() as u64) else { return 0; };
    let Some(total) = MDL_HEADER_SIZE.checked_add(pfn_bytes) else { return 0; };
    if total > u32::MAX as u64 { 0 } else { total as u32 }
}

/// Boot-time validation uses two real kernel VM pages, checks their translations,
/// exercises the MM copy path, and always releases the test extents.
pub fn self_test() -> bool {
    let source = match crate::kernel_vm::allocate_pages(1) { Ok(p) => p, Err(_) => return false };
    let destination = match crate::kernel_vm::allocate_pages(1) {
        Ok(p) => p,
        Err(_) => { let _ = crate::kernel_vm::free_pages(source, 1); return false; }
    };
    unsafe {
        source.write(0x39);
        source.add(127).write(0xA6);
        destination.write(0);
        destination.add(127).write(0);
    }
    let physical = unsafe { mm_get_physical_address(source.cast()) };
    #[repr(C)]
    struct TestMdl { header: MdlHeader, pfns: [u64; 1] }
    let mut test_mdl = TestMdl {
        header: MdlHeader {
            next: core::ptr::null_mut(), size: (MDL_HEADER_SIZE + 8) as i16,
            flags: 0, _padding: 0, process: core::ptr::null_mut(),
            mapped_system_va: core::ptr::null_mut(), start_va: source.cast(),
            byte_count: 128, byte_offset: 0,
        },
        pfns: [0],
    };
    unsafe { mm_build_mdl_for_non_paged_pool((&mut test_mdl as *mut TestMdl).cast()); }
    let mdl_ok = test_mdl.pfns[0] == physical / PAGE_SIZE
        && test_mdl.header.mapped_system_va == source.cast()
        && test_mdl.header.flags & (MDL_PAGES_LOCKED | MDL_SOURCE_IS_NONPAGED_POOL | MDL_MAPPED_TO_SYSTEM_VA)
            == (MDL_PAGES_LOCKED | MDL_SOURCE_IS_NONPAGED_POOL | MDL_MAPPED_TO_SYSTEM_VA);
    let mut copied = 0u64;
    let status = unsafe { mm_copy_memory(destination.cast(), source as u64, 128, 2, &mut copied) };
    let valid = physical != 0
        && unsafe { mm_is_address_valid(source.cast()) } != 0
        && unsafe { mm_is_non_paged_system_address_valid(source.cast()) } != 0
        && {
            let alias = unsafe { mm_get_virtual_for_physical(physical) } as u64;
            alias != 0 && translate(alias, false) == Some(physical)
        }
        && status == STATUS_SUCCESS && copied == 128
        && unsafe { destination.read() } == 0x39
        && unsafe { destination.add(127).read() } == 0xA6;
    let freed = crate::kernel_vm::free_pages(destination, 1).is_ok()
        && crate::kernel_vm::free_pages(source, 1).is_ok();
    let contiguous = unsafe { mm_allocate_contiguous_memory(5000, 0xffff_ffff) };
    let contiguous_ok = if contiguous.is_null() {
        false
    } else {
        unsafe {
            (contiguous as *mut u8).write(0x42);
            (contiguous as *mut u8).add(4999).write(0x91);
        }
        let mdl_size = unsafe { mm_size_of_mdl(contiguous, 5000) };
        let bytes_ok = unsafe { (contiguous as *const u8).read() == 0x42
            && (contiguous as *const u8).add(4999).read() == 0x91 };
        unsafe { mm_free_contiguous_memory(contiguous); }
        bytes_ok && mdl_size == 64
    };
    valid && mdl_ok && freed && contiguous_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mdl_page_count_handles_offsets_boundaries_and_overflow() {
        assert_eq!(mdl_page_count(0, 0), Some(0));
        assert_eq!(mdl_page_count(0, 1), Some(1));
        assert_eq!(mdl_page_count(PAGE_SIZE - 1, 1), Some(1));
        assert_eq!(mdl_page_count(PAGE_SIZE - 1, 2), Some(2));
        assert_eq!(mdl_page_count(PAGE_SIZE, 1), None);
        assert_eq!(mdl_page_count(0, u64::MAX), None);
    }

    #[test]
    fn mdl_range_validation_rejects_noncanonical_and_cross_boundary_ranges() {
        assert!(mdl_range_is_canonical(0x0000_0000_0000_1000, 0xfff, 1));
        assert!(!mdl_range_is_canonical(0x0000_0000_0000_1000, 0xfff, 2));
        assert!(mdl_range_is_canonical(0xffff_8000_0000_1000, 0, 4096));
        assert!(!mdl_range_is_canonical(0x0000_7fff_ffff_f000, 0, 8192));
        assert!(!mdl_range_is_canonical(0x0000_8000_0000_0000, 0, 1));
        assert!(!mdl_range_is_canonical(0xffff_8000_0000_1001, 0, 1));
    }

    #[test]
    fn mdl_header_matches_x64_abi_size() {
        assert_eq!(core::mem::size_of::<MdlHeader>(), MDL_HEADER_SIZE as usize);
        assert_eq!(core::mem::offset_of!(MdlHeader, byte_count), 40);
        assert_eq!(core::mem::offset_of!(MdlHeader, byte_offset), 44);
    }

    #[test]
    fn status_values_match_nt_contract() {
        assert_eq!(STATUS_SUCCESS, 0);
        assert_eq!(STATUS_ACCESS_VIOLATION as u32, 0xC000_0005);
        assert_eq!(STATUS_INVALID_PARAMETER as u32, 0xC000_000D);
        assert_eq!(STATUS_PARTIAL_COPY as u32, 0x8000_000D);
    }

    #[test]
    fn descriptor_permissions_are_supervisor_only() {
        // Every paging level must reject user-accessible entries for kernel
        // translation, even when the leaf itself is supervisor-only.
        assert!(entry_allowed(PRESENT, false));
        assert!(!entry_allowed(PRESENT | USER, false));
        assert!(!entry_allowed(PRESENT | USER | WRITE, true));
        assert!(!entry_allowed(PRESENT, true));
        assert!(entry_allowed(PRESENT | WRITE, true));
    }

    #[test]
    fn mdl_size_rejects_arithmetic_overflow() {
        assert_eq!(unsafe { mm_size_of_mdl(core::ptr::null(), u64::MAX) }, 0);
        assert_eq!(unsafe { mm_size_of_mdl(1usize as *const c_void, u64::MAX - 1) }, 0);
    }

    #[test]
    fn zero_length_mdl_has_header_only() {
        assert_eq!(unsafe { mm_size_of_mdl(core::ptr::null(), 0) }, MDL_HEADER_SIZE as u32);
    }

    #[test]
    fn copy_memory_rejects_bad_flags_and_null_destination_without_touching_source() {
        let mut transferred = 99u64;
        let bad_flags = unsafe {
            mm_copy_memory(core::ptr::null_mut(), u64::MAX, 8, 0, &mut transferred)
        };
        assert_eq!(bad_flags, STATUS_INVALID_PARAMETER);
        assert_eq!(transferred, 0);

        transferred = 99;
        let null_destination = unsafe {
            mm_copy_memory(core::ptr::null_mut(), u64::MAX, 8, 2, &mut transferred)
        };
        assert_eq!(null_destination, STATUS_INVALID_PARAMETER);
        assert_eq!(transferred, 0);
    }

    #[test]
    fn owned_mdl_registry_tracks_only_pool_owned_headers() {
        let address = 0xffff_8000_1234_5000;
        assert!(!is_owned_mdl(address));
        assert!(register_owned_mdl(address));
        assert!(is_owned_mdl(address));
        assert!(!register_owned_mdl(address + 8));
        assert!(unregister_owned_mdl(address));
        assert!(!is_owned_mdl(address));
        assert!(!unregister_owned_mdl(address));
    }

    #[test]
    fn mdl_page_pin_registry_is_reference_counted() {
        let pfn = 0x5a17_0001u64;
        assert!(!is_page_locked(pfn * PAGE_SIZE));
        assert!(lock_pfns(&[pfn, pfn]));
        assert!(is_page_locked(pfn * PAGE_SIZE));
        assert!(lock_pfns(&[pfn]));
        unlock_pfns(&[pfn, pfn]);
        assert!(is_page_locked(pfn * PAGE_SIZE));
        unlock_pfns(&[pfn]);
        assert!(!is_page_locked(pfn * PAGE_SIZE));
    }

    #[test]
    fn copy_memory_accepts_empty_copy_only_with_valid_flag_and_destination() {
        let mut transferred = 7u64;
        let invalid = unsafe {
            mm_copy_memory(core::ptr::null_mut(), 0, 0, 2, &mut transferred)
        };
        assert_eq!(invalid, STATUS_INVALID_PARAMETER);
        assert_eq!(transferred, 0);
    }
}

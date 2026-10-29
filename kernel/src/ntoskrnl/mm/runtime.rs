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

/// Internal write-access probe used before kernel APIs modify opaque caller records.
#[inline]
pub unsafe extern "win64" fn mm_is_non_paged_system_address_writable(address: *mut c_void) -> u8 {
    let va = address as u64;
    if va < 0xffff_8000_0000_0000 || crate::kernel_vm::is_pageable_address(va) { return 0; }
    (translate(va, true).is_some()) as u8
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
/// Nonblocking ownership probe used by physical-page rollback. Lock contention
/// is treated as "still pinned" so the caller fails safely without waiting.
pub fn try_is_page_locked(physical_page: u64) -> bool {
    let pfn = physical_page / PAGE_SIZE;
    let Some(table) = LOCKED_PAGES.try_lock() else { return true; };
    table.iter().any(|p| p.live && p.pfn == pfn && p.references != 0)
}
pub fn lock_pfns(pfns: &[u64]) -> bool {
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
pub fn unlock_pfns(pfns: &[u64]) {
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

// Physical pages allocated specifically for MmAllocatePagesForMdl[Ex] are
// tracked separately from ordinary MmCreateMdl allocations. Their PFNs live
// in the MDL itself; the registry proves ownership before any frame is freed.
const MAX_PAGE_ALLOCATION_MDLS: usize = 128;
#[derive(Clone, Copy)]
struct PageAllocationMdl { address: u64, pages: usize, live: bool }
impl PageAllocationMdl { const EMPTY: Self = Self { address: 0, pages: 0, live: false }; }
static PAGE_ALLOCATION_MDLS: crate::hal::IrqSpinLock<[PageAllocationMdl; MAX_PAGE_ALLOCATION_MDLS]> =
    crate::hal::IrqSpinLock::new([PageAllocationMdl::EMPTY; MAX_PAGE_ALLOCATION_MDLS]);

fn register_page_allocation_mdl(address: u64, pages: usize) -> bool {
    let mut table = PAGE_ALLOCATION_MDLS.lock();
    let Some(slot) = table.iter_mut().find(|entry| !entry.live) else { return false; };
    *slot = PageAllocationMdl { address, pages, live: true };
    true
}
fn try_register_page_allocation_mdl(address: u64, pages: usize) -> bool {
    let Some(mut table) = PAGE_ALLOCATION_MDLS.try_lock() else { return false; };
    let Some(slot) = table.iter_mut().find(|entry| !entry.live) else { return false; };
    *slot = PageAllocationMdl { address, pages, live: true };
    true
}
fn page_allocation_mdl_pages(address: u64) -> Option<usize> {
    PAGE_ALLOCATION_MDLS.lock().iter().find(|entry| entry.live && entry.address == address).map(|entry| entry.pages)
}
fn unregister_page_allocation_mdl(address: u64) -> bool {
    let mut table = PAGE_ALLOCATION_MDLS.lock();
    let Some(slot) = table.iter_mut().find(|entry| entry.live && entry.address == address) else { return false; };
    *slot = PageAllocationMdl::EMPTY;
    true
}

const MM_DONT_ZERO_ALLOCATION: u32 = 0x01;
const MM_ALLOCATE_FROM_LOCAL_NODE_ONLY: u32 = 0x02;
const MM_ALLOCATE_FULLY_REQUIRED: u32 = 0x04;
const MM_ALLOCATE_NO_WAIT: u32 = 0x08;
const MM_ALLOCATE_PREFER_CONTIGUOUS: u32 = 0x10;
const MM_ALLOCATE_REQUIRE_CONTIGUOUS_CHUNKS: u32 = 0x20;
const MM_ALLOCATE_KNOWN_FLAGS: u32 = 0x3f;
const STATUS_INVALID_PAGE_PROTECTION: i32 = 0xC000_0045u32 as i32;
const STATUS_NOT_MAPPED_VIEW: i32 = 0xC000_0019u32 as i32;

/// Allocate a nonpaged MDL and physical pages. This implementation supports
/// ordinary page allocations in the allocator's natural low/high RAM pools.
/// Unsupported address-window/alignment requests fail rather than returning
/// PFNs outside the caller's constraints.
unsafe fn allocate_pages_for_mdl(
    low_address: u64, high_address: u64, skip_bytes: u64, total_bytes: u64,
    cache_type: u32, flags: u32,
) -> *mut c_void {
    // Local-node-only requests are delegated to the node-aware implementation.
    // Its node-specific allocation and rollback locks must be try-lock based too.
    // Windows interprets FROM_LOCAL_NODE_ONLY as the current thread's ideal
    // node. Route this flag through the node-aware allocator instead of
    // rejecting it or silently allocating from an arbitrary physical range.
    if flags & MM_ALLOCATE_FROM_LOCAL_NODE_ONLY != 0 {
        let ideal_node = unsafe { crate::ntoskrnl::ke::runtime::ke_get_current_node_number() } as u32;
        return unsafe {
            mm_allocate_node_pages_for_mdl_ex(
                low_address, high_address, skip_bytes, total_bytes,
                cache_type, ideal_node, flags,
            )
        };
    }
    let require_contiguous = flags & MM_ALLOCATE_REQUIRE_CONTIGUOUS_CHUNKS != 0;
    if total_bytes == 0 || total_bytes > (u32::MAX as u64 - PAGE_SIZE)
        || high_address < low_address
        || (skip_bytes != 0 && (skip_bytes < PAGE_SIZE || skip_bytes % PAGE_SIZE != 0))
        || (require_contiguous && skip_bytes != 0
            && (total_bytes % skip_bytes != 0 || !skip_bytes.is_power_of_two()))
        || (flags & 0x40 != 0) // FAST_LARGE_PAGES requires a large-page cache.
        || (flags & 0x100 != 0) // AND_HOT_REMOVE requires memory hot-remove support.
        || cache_type != 1
        || flags & !MM_ALLOCATE_KNOWN_FLAGS != 0
        || flags & MM_ALLOCATE_FROM_LOCAL_NODE_ONLY != 0 {
        return core::ptr::null_mut();
    }
    let Some(rounded) = total_bytes.checked_add(PAGE_SIZE - 1) else { return core::ptr::null_mut(); };
    let requested_pages = (rounded / PAGE_SIZE) as usize;
    // MDL.Size is a signed 16-bit field in the x64 NT ABI.
    if requested_pages == 0 || requested_pages > ((i16::MAX as u64 - MDL_HEADER_SIZE) / 8) as usize {
        return core::ptr::null_mut();
    }
    let mdl_bytes = (MDL_HEADER_SIZE + requested_pages as u64 * 8) as usize;
    let no_wait = flags & MM_ALLOCATE_NO_WAIT != 0;
    let mdl = if no_wait {
        crate::ntoskrnl::rtl_runtime::pool_try_allocate_tagged(mdl_bytes, 0x4c444d4d, true)
    } else {
        crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(mdl_bytes, 0x4c444d4d, true)
    };
    if mdl.is_null() { return core::ptr::null_mut(); }

    let mut allocated = 0usize;
    let pfns = unsafe { mdl.cast::<u8>().add(MDL_HEADER_SIZE as usize).cast::<u64>() };
    let prefer_contiguous = flags & MM_ALLOCATE_PREFER_CONTIGUOUS != 0;
    // With SkipBytes == 0, REQUIRE_CONTIGUOUS_CHUNKS requires one run.
    // With nonzero SkipBytes, each returned chunk is exactly SkipBytes bytes,
    // aligned to a SkipBytes boundary; chunks may be physically disjoint.
    if require_contiguous && skip_bytes == 0 {
        if let Ok(base) = unsafe {
            if no_wait {
                crate::memory::try_allocate_contiguous_pages_in_range(requested_pages, low_address, high_address, 0)
            } else {
                crate::memory::allocate_contiguous_pages_in_range(requested_pages, low_address, high_address, 0)
            }
        } {
            for index in 0..requested_pages {
                unsafe { pfns.add(index).write(base / PAGE_SIZE + index as u64); }
            }
            allocated = requested_pages;
        } else {
            if no_wait {
                let _ = crate::ntoskrnl::rtl_runtime::pool_try_free_tagged(mdl, Some(0x4c444d4d));
            } else {
                let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(mdl, Some(0x4c444d4d));
            }
            return core::ptr::null_mut();
        }
    } else if require_contiguous {
        let pages_per_chunk = (skip_bytes / PAGE_SIZE) as usize;
        let chunks = requested_pages / pages_per_chunk;
        for chunk in 0..chunks {
            let offset = match (chunk as u64).checked_mul(skip_bytes) {
                Some(v) => v,
                None => break,
            };
            let (Some(low), Some(high)) = (low_address.checked_add(offset), high_address.checked_add(offset)) else { break; };
            let first = chunk * pages_per_chunk;
            match unsafe { if no_wait {
                crate::memory::try_allocate_contiguous_pages_in_range(pages_per_chunk, low, high, skip_bytes)
            } else {
                crate::memory::allocate_contiguous_pages_in_range(pages_per_chunk, low, high, skip_bytes)
            } } {
                Ok(base) => {
                    for page in 0..pages_per_chunk {
                        unsafe { pfns.add(first + page).write(base / PAGE_SIZE + page as u64); }
                    }
                    allocated += pages_per_chunk;
                }
                Err(_) => break, // Only complete chunks are returned.
            }
        }
    } else {
        // PREFER_CONTIGUOUS is a fragmentation hint: attempt a full run before
        // falling back to ordinary page allocation, but never require a run.
        if prefer_contiguous && skip_bytes == 0 {
            if let Ok(base) = unsafe {
                if no_wait {
                    crate::memory::try_allocate_contiguous_pages_in_range(requested_pages, low_address, high_address, 0)
                } else {
                    crate::memory::allocate_contiguous_pages_in_range(requested_pages, low_address, high_address, 0)
                }
            } {
                for index in 0..requested_pages {
                    unsafe { pfns.add(index).write(base / PAGE_SIZE + index as u64); }
                }
                allocated = requested_pages;
            }
        }
        if allocated == 0 {
            // SkipBytes defines successive address windows, not a per-page
            // alignment boundary. Search each window before giving up.
            'pages: for index in 0..requested_pages {
                let mut window = 0u64;
                loop {
                    let offset = match window.checked_mul(skip_bytes) { Some(v) => v, None => break };
                    let (Some(low), Some(high)) = (low_address.checked_add(offset), high_address.checked_add(offset)) else { break };
                    match unsafe { if no_wait {
                        crate::memory::try_allocate_page_in_range(low, high, 0)
                    } else {
                        crate::memory::allocate_page_in_range(low, high, 0)
                    } } {
                        Ok(value) => {
                            unsafe { pfns.add(index).write(value / PAGE_SIZE); }
                            allocated += 1;
                            continue 'pages;
                        }
                        Err(_) => {
                            if skip_bytes == 0 { break; }
                            window = match window.checked_add(1) { Some(v) => v, None => break };
                        }
                    }
                }
                break;
            }
        }
    }
    if allocated == 0 || (flags & MM_ALLOCATE_FULLY_REQUIRED != 0 && allocated != requested_pages) {
        for index in 0..allocated {
            let physical = unsafe { pfns.add(index).read() } * PAGE_SIZE;
            let _ = unsafe {
                if no_wait { crate::memory::try_free_page(physical) }
                else { crate::memory::free_page(physical) }
            };
        }
        if no_wait {
            let _ = crate::ntoskrnl::rtl_runtime::pool_try_free_tagged(mdl, Some(0x4c444d4d));
        } else {
            let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(mdl, Some(0x4c444d4d));
        }
        return core::ptr::null_mut();
    }
    let registered = if no_wait {
        try_register_page_allocation_mdl(mdl as u64, allocated)
    } else {
        register_page_allocation_mdl(mdl as u64, allocated)
    };
    if !registered {
        for index in 0..allocated {
            let physical = unsafe { pfns.add(index).read() } * PAGE_SIZE;
            let _ = unsafe {
                if no_wait { crate::memory::try_free_page(physical) }
                else { crate::memory::free_page(physical) }
            };
        }
        if no_wait {
            let _ = crate::ntoskrnl::rtl_runtime::pool_try_deallocate_tagged(mdl, Some(0x4c444d4d));
        } else {
            let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(mdl, Some(0x4c444d4d));
        }
        return core::ptr::null_mut();
    }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    header.next = core::ptr::null_mut();
    header.size = (MDL_HEADER_SIZE + allocated as u64 * 8) as i16;
    header.flags = MDL_LOCKED;
    header._padding = 0;
    header.process = core::ptr::null_mut();
    header.mapped_system_va = core::ptr::null_mut();
    header.start_va = core::ptr::null_mut();
    header.byte_count = core::cmp::min(total_bytes, allocated as u64 * PAGE_SIZE) as u32;
    header.byte_offset = 0;
    mdl.cast()
}

/// MmAllocatePagesForMdl: cached, zero-filled physical pages.
pub unsafe extern "win64" fn mm_allocate_pages_for_mdl(
    low_address: u64, high_address: u64, skip_bytes: u64, total_bytes: u64,
) -> *mut c_void {
    unsafe { allocate_pages_for_mdl(low_address, high_address, skip_bytes, total_bytes, 1, 0) }
}

/// Extended page allocation with explicit cache type and allocation flags.
pub unsafe extern "win64" fn mm_allocate_pages_for_mdl_ex(
    low_address: u64, high_address: u64, skip_bytes: u64, total_bytes: u64,
    cache_type: u32, flags: u32,
) -> *mut c_void {
    unsafe { allocate_pages_for_mdl(low_address, high_address, skip_bytes, total_bytes, cache_type, flags) }
}

/// Allocate an MDL from an explicit NUMA node using SRAT-owned physical ranges.
/// NO_WAIT uses reserve-backed pool storage and try-lock physical/registry paths;
/// it can fail immediately when reserves, eligible low pages, or a lock is busy.
pub unsafe extern "win64" fn mm_allocate_node_pages_for_mdl_ex(
    low_address: u64, high_address: u64, skip_bytes: u64, total_bytes: u64,
    cache_type: u32, ideal_node: u32, flags: u32,
) -> *mut c_void {
    let no_wait = flags & MM_ALLOCATE_NO_WAIT != 0;
    let require_contiguous = flags & MM_ALLOCATE_REQUIRE_CONTIGUOUS_CHUNKS != 0;
    let prefer_contiguous = flags & MM_ALLOCATE_PREFER_CONTIGUOUS != 0;
    if total_bytes == 0 || total_bytes > (u32::MAX as u64 - PAGE_SIZE)
        || high_address < low_address
        || (skip_bytes != 0 && (skip_bytes < PAGE_SIZE || skip_bytes % PAGE_SIZE != 0))
        || (require_contiguous && skip_bytes != 0
            && (!skip_bytes.is_power_of_two() || total_bytes % skip_bytes != 0))
        || cache_type != 1
        || flags & !MM_ALLOCATE_KNOWN_FLAGS != 0
        || (flags & 0x40 != 0) // FAST_LARGE_PAGES requires a dedicated large-page cache.
        || (flags & 0x100 != 0) // Hot-remove is not implemented.
    {
        return core::ptr::null_mut();
    }
    let Some(rounded) = total_bytes.checked_add(PAGE_SIZE - 1) else { return core::ptr::null_mut(); };
    let requested_pages = (rounded / PAGE_SIZE) as usize;
    if requested_pages == 0
        || requested_pages > ((i16::MAX as u64 - MDL_HEADER_SIZE) / 8) as usize {
        return core::ptr::null_mut();
    }
    let node_count = crate::ntoskrnl::mm::topology::memory_node_count();
    if node_count == 0 || ideal_node as usize >= node_count {
        return core::ptr::null_mut();
    }
    let mdl_bytes = (MDL_HEADER_SIZE + requested_pages as u64 * 8) as usize;
    let mdl = if no_wait {
        crate::ntoskrnl::rtl_runtime::pool_try_allocate_tagged(mdl_bytes, 0x4c444d4d, true)
    } else {
        crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(mdl_bytes, 0x4c444d4d, true)
    };
    if mdl.is_null() { return core::ptr::null_mut(); }
    let pfns = unsafe { core::slice::from_raw_parts_mut(
        mdl.cast::<u8>().add(MDL_HEADER_SIZE as usize).cast::<u64>(), requested_pages,
    ) };
    let local_only = flags & MM_ALLOCATE_FROM_LOCAL_NODE_ONLY != 0;
    let mut allocated = 0usize;

    // REQUIRE_CONTIGUOUS_CHUNKS with SkipBytes == 0 means one contiguous run.
    // For nonzero SkipBytes each chunk is independently contiguous, exactly
    // SkipBytes bytes long, and aligned to that physical boundary.
    if require_contiguous && skip_bytes == 0 {
        let mut node_order = [0u32; crate::ntoskrnl::mm::topology::MAX_NODES];
        node_order[0] = ideal_node;
        let mut nodes = 1usize;
        if !local_only {
            for node in 0..node_count as u32 {
                if node != ideal_node { node_order[nodes] = node; nodes += 1; }
            }
        }
        for &node in &node_order[..nodes] {
            if let Ok(base) = unsafe {
                if no_wait {
                    crate::ntoskrnl::mm::physical::try_allocate_contiguous_pages_in_node_range(
                        requested_pages, node, low_address, high_address, 0,
                    )
                } else {
                    crate::ntoskrnl::mm::physical::allocate_contiguous_pages_in_node_range(
                        requested_pages, node, low_address, high_address, 0,
                    )
                }
            } {
                for index in 0..requested_pages {
                    pfns[index] = base / PAGE_SIZE + index as u64;
                }
                allocated = requested_pages;
                break;
            }
        }
    } else if require_contiguous {
        let pages_per_chunk = (skip_bytes / PAGE_SIZE) as usize;
        let chunks = requested_pages / pages_per_chunk;
        for chunk in 0..chunks {
            let Some(offset) = (chunk as u64).checked_mul(skip_bytes) else { break; };
            let (Some(low), Some(high)) = (low_address.checked_add(offset), high_address.checked_add(offset)) else { break; };
            let mut node_order = [0u32; crate::ntoskrnl::mm::topology::MAX_NODES];
            node_order[0] = ideal_node;
            let mut nodes = 1usize;
            if !local_only {
                for node in 0..node_count as u32 {
                    if node != ideal_node { node_order[nodes] = node; nodes += 1; }
                }
            }
            let mut chunk_base = None;
            for &node in &node_order[..nodes] {
                if let Ok(base) = unsafe {
                    if no_wait {
                        crate::ntoskrnl::mm::physical::try_allocate_contiguous_pages_in_node_range(
                            pages_per_chunk, node, low, high, skip_bytes,
                        )
                    } else {
                        crate::ntoskrnl::mm::physical::allocate_contiguous_pages_in_node_range(
                            pages_per_chunk, node, low, high, skip_bytes,
                        )
                    }
                } {
                    chunk_base = Some(base);
                    break;
                }
            }
            let Some(base) = chunk_base else { break; };
            let first = chunk * pages_per_chunk;
            for page in 0..pages_per_chunk {
                pfns[first + page] = base / PAGE_SIZE + page as u64;
            }
            allocated += pages_per_chunk;
        }
    } else {
        // PREFER_CONTIGUOUS is a hint only. Try one run on the preferred node,
        // then fall back to ordinary pages if no node can satisfy the run.
        if prefer_contiguous && skip_bytes == 0 {
            let mut node_order = [0u32; crate::ntoskrnl::mm::topology::MAX_NODES];
            node_order[0] = ideal_node;
            let mut nodes = 1usize;
            if !local_only {
                for node in 0..node_count as u32 {
                    if node != ideal_node { node_order[nodes] = node; nodes += 1; }
                }
            }
            for &node in &node_order[..nodes] {
                if let Ok(base) = unsafe {
                    if no_wait {
                        crate::ntoskrnl::mm::physical::try_allocate_contiguous_pages_in_node_range(
                            requested_pages, node, low_address, high_address, 0,
                        )
                    } else {
                        crate::ntoskrnl::mm::physical::allocate_contiguous_pages_in_node_range(
                            requested_pages, node, low_address, high_address, 0,
                        )
                    }
                } {
                    for index in 0..requested_pages {
                        pfns[index] = base / PAGE_SIZE + index as u64;
                    }
                    allocated = requested_pages;
                    break;
                }
            }
        }
        'pages: while allocated < requested_pages {
            let mut window = 0u64;
            loop {
                let offset = match window.checked_mul(skip_bytes) { Some(v) => v, None => break };
                let (Some(low), Some(high)) = (low_address.checked_add(offset), high_address.checked_add(offset)) else { break; };
                if low >= crate::ntoskrnl::mm::physical::DIRECT_MAP_PHYSICAL_LIMIT { break; }
                let mut node_order = [0u32; crate::ntoskrnl::mm::topology::MAX_NODES];
                node_order[0] = ideal_node;
                let mut nodes = 1usize;
                if !local_only {
                    for node in 0..node_count as u32 {
                        if node != ideal_node { node_order[nodes] = node; nodes += 1; }
                    }
                }
                let mut selected = None;
                for &node in &node_order[..nodes] {
                    if unsafe {
                        if no_wait {
                            crate::ntoskrnl::mm::physical::try_allocate_pages_in_node_range(
                                &mut pfns[allocated..allocated + 1], node, low, high, false,
                            )
                        } else {
                            crate::ntoskrnl::mm::physical::allocate_pages_in_node_range(
                                &mut pfns[allocated..allocated + 1], node, low, high, false,
                            )
                        }
                    }.is_ok() {
                        selected = Some(());
                        break;
                    }
                }
                if selected.is_some() {
                    allocated += 1;
                    continue 'pages;
                }
                if skip_bytes == 0 { break; }
                window = match window.checked_add(1) { Some(v) => v, None => break };
            }
            break;
        }
    }

    if allocated == 0 || (flags & MM_ALLOCATE_FULLY_REQUIRED != 0 && allocated != requested_pages) {
        for &pfn in pfns[..allocated].iter().rev() {
            let _ = unsafe {
                if no_wait { crate::memory::try_free_page(pfn * PAGE_SIZE) }
                else { crate::memory::free_page(pfn * PAGE_SIZE) }
            };
        }
        if no_wait {
            let _ = crate::ntoskrnl::rtl_runtime::pool_try_free_tagged(mdl, Some(0x4c444d4d));
        } else {
            let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(mdl, Some(0x4c444d4d));
        }
        return core::ptr::null_mut();
    }
    let registered = if no_wait {
        try_register_page_allocation_mdl(mdl as u64, allocated)
    } else {
        register_page_allocation_mdl(mdl as u64, allocated)
    };
    if !registered {
        for &pfn in pfns[..allocated].iter().rev() {
            let _ = unsafe { crate::memory::free_page(pfn * PAGE_SIZE) };
        }
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(mdl, Some(0x4c444d4d));
        return core::ptr::null_mut();
    }
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    header.next = core::ptr::null_mut();
    header.size = (MDL_HEADER_SIZE + allocated as u64 * 8) as i16;
    header.flags = MDL_LOCKED;
    header._padding = 0;
    header.process = core::ptr::null_mut();
    header.mapped_system_va = core::ptr::null_mut();
    header.start_va = core::ptr::null_mut();
    header.byte_count = core::cmp::min(total_bytes, allocated as u64 * PAGE_SIZE) as u32;
    header.byte_offset = 0;
    mdl.cast()
}

/// Release only physical frames owned by an MmAllocatePagesForMdl MDL.
/// The MDL storage itself remains allocated for the caller to free separately.
pub unsafe extern "win64" fn mm_free_pages_from_mdl(mdl: *mut c_void) {
    if mdl.is_null() { return; }
    let address = mdl as u64;
    let Some(pages) = page_allocation_mdl_pages(address) else { return; };
    if pages == 0 || pages > ((i16::MAX as u64 - MDL_HEADER_SIZE) / 8) as usize
        || !mdl_header_is_writable(address) { return; }
    let header = unsafe { &*mdl.cast::<MdlHeader>() };
    if header.flags & MDL_MAPPED_SYSTEM_VA != 0 { return; }
    let pfns = unsafe { mdl.cast::<u8>().add(MDL_HEADER_SIZE as usize).cast::<u64>() };
    // Validate the whole list before mutating allocator state.
    for index in 0..pages {
        let physical = match unsafe { pfns.add(index).read() }.checked_mul(PAGE_SIZE) {
            Some(value) if value != 0 && value & (PAGE_SIZE - 1) == 0 => value,
            _ => return,
        };
        if !crate::memory::is_ram_physical_address(physical) || is_page_locked(physical) { return; }
    }
    for index in 0..pages {
        let physical = unsafe { pfns.add(index).read() } * PAGE_SIZE;
        if unsafe { crate::memory::free_page(physical) }.is_err() { return; }
    }
    let _ = unregister_page_allocation_mdl(address);
    let header = unsafe { &mut *mdl.cast::<MdlHeader>() };
    header.flags &= !(MDL_LOCKED | MDL_MAPPED_SYSTEM_VA);
    header.mapped_system_va = core::ptr::null_mut();
}
/// Compatibility export: currently the extended free semantics are identical.
pub unsafe extern "win64" fn mm_free_pages_from_mdl_ex(mdl: *mut c_void) {
    unsafe { mm_free_pages_from_mdl(mdl); }
}

/// Change protection only for a live system mapping created for this MDL.
pub unsafe extern "win64" fn mm_protect_mdl_system_address(mdl: *mut c_void, new_protect: u32) -> i32 {
    if mdl.is_null() || !mdl_header_is_writable(mdl as u64) { return STATUS_INVALID_PARAMETER; }
    let header = unsafe { &*mdl.cast::<MdlHeader>() };
    if header.flags & MDL_MAPPED_SYSTEM_VA == 0 || header.mapped_system_va.is_null() {
        return STATUS_NOT_MAPPED_VIEW;
    }
    let (writable, executable, present, allow_wx) = match new_protect {
        0x01 => (false, false, false, false), // PAGE_NOACCESS
        0x02 => (false, false, true, false),  // PAGE_READONLY
        0x04 => (true, false, true, false),   // PAGE_READWRITE
        0x10 => (false, true, true, false),   // PAGE_EXECUTE
        0x20 => (false, true, true, false),   // PAGE_EXECUTE_READ
        0x40 => (true, true, true, true),     // PAGE_EXECUTE_READWRITE
        _ => return STATUS_INVALID_PAGE_PROTECTION,
    };
    let pages = match mdl_page_count(header.byte_offset as u64, header.byte_count as u64) {
        Some(value) if value != 0 && value <= usize::MAX as u64 => value as usize,
        _ => return STATUS_INVALID_PARAMETER,
    };
    if crate::kernel_vm::protect_pages_with_access(header.mapped_system_va as u64 & !(PAGE_SIZE - 1), pages, writable, executable, present, allow_wx).is_err() {
        return STATUS_NOT_MAPPED_VIEW;
    }
    STATUS_SUCCESS
}

/// Return a terminated PHYSICAL_MEMORY_RANGE array allocated from nonpaged pool.
pub unsafe extern "win64" fn mm_get_physical_memory_ranges() -> *mut c_void {
    const MAX_RANGES: usize = 256;
    #[repr(C)]
    struct PhysicalMemoryRange { base: u64, length: u64 }
    let bytes = (MAX_RANGES + 1) * core::mem::size_of::<PhysicalMemoryRange>();
    let result = crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(bytes, 0x524d4d4d, true);
    if result.is_null() { return core::ptr::null_mut(); }
    let ranges = unsafe { core::slice::from_raw_parts_mut(result.cast::<(u64, u64)>(), MAX_RANGES + 1) };
    let count = crate::memory::physical_memory_ranges(&mut ranges[..MAX_RANGES]);
    if count == 0 {
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(result, Some(0x524d4d4d));
        return core::ptr::null_mut();
    }
    ranges[count] = (0, 0);
    result.cast()
}

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
    // Exercise the newly wired page-MDL allocation, mapping, protection,
    // release, and physical-range enumeration paths as one ownership cycle.
    let page_mdl = unsafe { mm_allocate_pages_for_mdl(0, u64::MAX, 0, PAGE_SIZE * 2) };
    let page_mdl_ok = if page_mdl.is_null() {
        false
    } else {
        let mapped = unsafe { mm_map_locked_pages(page_mdl, 0) };
        let protection_ok = !mapped.is_null()
            && unsafe { mm_protect_mdl_system_address(page_mdl, 0x02) } == STATUS_SUCCESS
            && unsafe { mm_protect_mdl_system_address(page_mdl, 0x01) } == STATUS_SUCCESS
            && unsafe { mm_protect_mdl_system_address(page_mdl, 0x04) } == STATUS_SUCCESS
            && unsafe { mm_protect_mdl_system_address(page_mdl, 0x40) } == STATUS_SUCCESS
            && unsafe { mm_protect_mdl_system_address(page_mdl, 0x04) } == STATUS_SUCCESS;
        if !mapped.is_null() { unsafe { mm_unmap_locked_pages(mapped, page_mdl); } }
        unsafe { mm_free_pages_from_mdl(page_mdl); }
        let mdl_storage_freed = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            page_mdl.cast(), Some(0x4c444d4d),
        );
        protection_ok && mdl_storage_freed
    };
    // Exercise nonzero LowAddress, an inclusive HighAddress, and the
    // Windows rule that REQUIRE_CONTIGUOUS_CHUNKS with nonzero SkipBytes
    // requires TotalBytes to be an exact multiple of SkipBytes. Use one
    // 8-KiB contiguous chunk, aligned to its 8-KiB boundary.
    let bounded_mdl = unsafe {
        mm_allocate_pages_for_mdl_ex(
            0x10_0000, 0xffff_ffff, PAGE_SIZE * 2, PAGE_SIZE * 2, 1,
            MM_ALLOCATE_FULLY_REQUIRED | MM_ALLOCATE_REQUIRE_CONTIGUOUS_CHUNKS,
        )
    };
    let bounded_ok = if bounded_mdl.is_null() {
        false
    } else {
        let header = unsafe { &*bounded_mdl.cast::<MdlHeader>() };
        let pfns = unsafe { bounded_mdl.cast::<u8>().add(MDL_HEADER_SIZE as usize).cast::<u64>() };
        let first = unsafe { pfns.read() } * PAGE_SIZE;
        let second = unsafe { pfns.add(1).read() } * PAGE_SIZE;
        let last = second.saturating_add(PAGE_SIZE - 1);
        let valid = header.byte_count as u64 == PAGE_SIZE * 2
            && second == first.saturating_add(PAGE_SIZE)
            && first >= 0x10_0000 && last <= 0xffff_ffff
            && first % (PAGE_SIZE * 2) == 0
            && first / (PAGE_SIZE * 2) == last / (PAGE_SIZE * 2);
        unsafe { mm_free_pages_from_mdl(bounded_mdl); }
        let storage_freed = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            bounded_mdl.cast(), Some(0x4c444d4d),
        );
        valid && storage_freed
    };
    // Exercise explicit-node allocation from SRAT-owned, previously
    // unallocated conventional-memory extents (not only returned PFN lists).
    let node_mdl = unsafe {
        mm_allocate_node_pages_for_mdl_ex(
            0x10_0000, 0xffff_ffff, 0, PAGE_SIZE * 2, 1, 0,
            MM_ALLOCATE_FULLY_REQUIRED | MM_ALLOCATE_FROM_LOCAL_NODE_ONLY,
        )
    };
    let node_ok = if node_mdl.is_null() {
        #[cfg(target_os = "none")]
        unsafe {
            crate::serial_write(b"MM-SELFTEST node-mdl=null node-count=");
            crate::serial_write_decimal(crate::ntoskrnl::mm::topology::memory_node_count() as usize);
            crate::serial_write(b" ranges=");
            let mut debug_ranges = [(0u64, 0u64); 4];
            let debug_count = crate::ntoskrnl::mm::topology::memory_ranges_for_node(0, &mut debug_ranges);
            crate::serial_write_decimal(debug_count as usize);
            crate::serial_write(b"\\r\\n");
        }
        false
    } else {
        let header = unsafe { &*node_mdl.cast::<MdlHeader>() };
        let pfns = unsafe { node_mdl.cast::<u8>().add(MDL_HEADER_SIZE as usize).cast::<u64>() };
        let first_pfn = unsafe { pfns.read() };
        let second_pfn = unsafe { pfns.add(1).read() };
        let first = first_pfn * PAGE_SIZE;
        let second = second_pfn * PAGE_SIZE;
        let first_node = crate::ntoskrnl::mm::topology::node_for_range(first, PAGE_SIZE);
        let second_node = crate::ntoskrnl::mm::topology::node_for_range(second, PAGE_SIZE);
        let byte_count = header.byte_count as u32;
        let valid = byte_count as u64 == PAGE_SIZE * 2
            && [first, second].iter().all(|pa| *pa >= 0x10_0000 && *pa <= 0xffff_ffff)
            && first_node == Some(0) && second_node == Some(0);
        unsafe { mm_free_pages_from_mdl(node_mdl); }
        let storage_freed = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            node_mdl.cast(), Some(0x4c444d4d),
        );
        #[cfg(target_os = "none")]
        if !valid || !storage_freed {
            unsafe {
                crate::serial_write(b"MM-SELFTEST node-mdl first=");
                crate::serial_write_hex(first);
                crate::serial_write(b" node=");
                crate::serial_write_decimal(first_node.unwrap_or(u32::MAX) as usize);
                crate::serial_write(b" second=");
                crate::serial_write_hex(second);
                crate::serial_write(b" node=");
                crate::serial_write_decimal(second_node.unwrap_or(u32::MAX) as usize);
                crate::serial_write(b" bytes=");
                crate::serial_write_decimal(byte_count as usize);
                crate::serial_write(b" storage-freed=");
                crate::serial_write(if storage_freed { b"1" } else { b"0" });
                crate::serial_write(b"\\r\\n");
            }
        }
        valid && storage_freed
    };
    // Explicit-node contiguous chunks must honor the same SkipBytes alignment
    // and exact chunk-size rules as the generic Windows MDL allocator.
    let node_chunk_mdl = unsafe {
        mm_allocate_node_pages_for_mdl_ex(
            0x10_0000, 0xffff_ffff, PAGE_SIZE * 2, PAGE_SIZE * 2, 1, 0,
            MM_ALLOCATE_FULLY_REQUIRED | MM_ALLOCATE_FROM_LOCAL_NODE_ONLY
                | MM_ALLOCATE_REQUIRE_CONTIGUOUS_CHUNKS,
        )
    };
    let node_chunk_ok = if node_chunk_mdl.is_null() {
        false
    } else {
        let header = unsafe { &*node_chunk_mdl.cast::<MdlHeader>() };
        let pfns = unsafe { node_chunk_mdl.cast::<u8>().add(MDL_HEADER_SIZE as usize).cast::<u64>() };
        let first = unsafe { pfns.read() } * PAGE_SIZE;
        let second = unsafe { pfns.add(1).read() } * PAGE_SIZE;
        let last = second.saturating_add(PAGE_SIZE - 1);
        let valid = header.byte_count as u64 == PAGE_SIZE * 2
            && second == first.saturating_add(PAGE_SIZE)
            && first % (PAGE_SIZE * 2) == 0
            && first / (PAGE_SIZE * 2) == last / (PAGE_SIZE * 2)
            && crate::ntoskrnl::mm::topology::node_for_range(first, PAGE_SIZE * 2) == Some(0);
        unsafe { mm_free_pages_from_mdl(node_chunk_mdl); }
        let storage_freed = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            node_chunk_mdl.cast(), Some(0x4c444d4d),
        );
        valid && storage_freed
    };
    // The no-wait path must complete from preallocated metadata and try-lock
    // physical/registry paths, without falling back to a blocking allocator.
    let no_wait_mdl = unsafe {
        mm_allocate_pages_for_mdl_ex(
            0x10_0000, 0xffff_ffff, 0, PAGE_SIZE, 1,
            MM_ALLOCATE_FULLY_REQUIRED | MM_ALLOCATE_NO_WAIT,
        )
    };
    let no_wait_ok = if no_wait_mdl.is_null() {
        false
    } else {
        let header = unsafe { &*no_wait_mdl.cast::<MdlHeader>() };
        let pfn = unsafe {
            no_wait_mdl.cast::<u8>().add(MDL_HEADER_SIZE as usize).cast::<u64>().read()
        };
        let valid = header.byte_count as u64 == PAGE_SIZE
            && pfn * PAGE_SIZE >= 0x10_0000
            && pfn * PAGE_SIZE + PAGE_SIZE - 1 <= 0xffff_ffff;
        unsafe { mm_free_pages_from_mdl(no_wait_mdl); }
        let storage_freed = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            no_wait_mdl.cast(), Some(0x4c444d4d),
        );
        valid && storage_freed
    };
    let ranges = unsafe { mm_get_physical_memory_ranges() };
    let ranges_ok = if ranges.is_null() {
        false
    } else {
        let values = ranges.cast::<u64>();
        let mut terminated = false;
        for index in 0..=256 {
            if unsafe { values.add(index * 2).read() } == 0
                && unsafe { values.add(index * 2 + 1).read() } == 0 {
                terminated = true;
                break;
            }
        }
        let freed = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            ranges.cast(), Some(0x524d4d4d),
        );
        terminated && freed
    };
    // Keep boot diagnostics actionable: a single aggregate boolean used to
    // hide which MM contract regressed when QEMU halted before driver loading.
    unsafe {
        if !valid { crate::serial_write(b"MM self-test failed: translation/copy\\r\\n"); }
        if !mdl_ok { crate::serial_write(b"MM self-test failed: nonpaged MDL build\\r\\n"); }
        if !freed { crate::serial_write(b"MM self-test failed: temporary page release\\r\\n"); }
        if !contiguous_ok { crate::serial_write(b"MM self-test failed: contiguous allocation\\r\\n"); }
        if !page_mdl_ok { crate::serial_write(b"MM self-test failed: page-MDL map/protection/free\\r\\n"); }
        if !bounded_ok { crate::serial_write(b"MM self-test failed: bounded contiguous chunks\\r\\n"); }
        if !node_ok { crate::serial_write(b"MM self-test failed: NUMA node allocation\\r\\n"); }
        if !node_chunk_ok { crate::serial_write(b"MM self-test failed: NUMA contiguous chunks\\r\\n"); }
        if !no_wait_ok { crate::serial_write(b"MM self-test failed: MM_ALLOCATE_NO_WAIT\\r\\n"); }
        if !ranges_ok { crate::serial_write(b"MM self-test failed: physical ranges enumeration\\r\\n"); }
    }
    let all_ok = valid && mdl_ok && freed && contiguous_ok && page_mdl_ok && bounded_ok && node_ok && node_chunk_ok && no_wait_ok && ranges_ok;
    #[cfg(target_os = "none")]
    if !all_ok {
        // Keep the boot-time failure actionable: report each independent
        // compatibility probe rather than collapsing every failure to FALSE.
        unsafe {
            crate::serial_write(b"MM-SELFTEST valid/mdl/freed/contig/page-mdl/bounded/node/node-chunk/no-wait/ranges=");
            for value in [valid, mdl_ok, freed, contiguous_ok, page_mdl_ok, bounded_ok, node_ok, node_chunk_ok, no_wait_ok, ranges_ok] {
                crate::serial_write(if value { b"1" } else { b"0" });
                crate::serial_write(b"/");
            }
            crate::serial_write(b"\\r\\n");
        }
    }
    all_ok
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


// ---- Driver image section residency and paging ------------------------------
// The PE driver loader registers exact page-aligned section ranges. Non-pageable
// sections are pinned for the image lifetime; pageable sections are pinned only
// while MmLockPagableDataSection references their section handle. The exported
// lock routine still returns handles for non-pageable sections.
const MAX_PAGEABLE_IMAGE_LOCKS: usize = 256;
#[derive(Clone, Copy)]
struct PageableImageLock {
    used: bool,
    section_base: u64,
    section_end: u64,
    image_base: u64,
    references: u32,
    pageable: bool,
    permanent_pin: bool,
}
impl PageableImageLock {
    const EMPTY: Self = Self { used: false, section_base: 0, section_end: 0,
        image_base: 0, references: 0, pageable: false, permanent_pin: false };
}
unsafe impl Send for PageableImageLock {}
static PAGEABLE_IMAGE_LOCKS: crate::hal::IrqSpinLock<[PageableImageLock; MAX_PAGEABLE_IMAGE_LOCKS]> =
    crate::hal::IrqSpinLock::new([PageableImageLock::EMPTY; MAX_PAGEABLE_IMAGE_LOCKS]);

fn aligned_image_range(base: u64, size: u64) -> Option<(u64, u64)> {
    if size == 0 { return None; }
    let start = base & !(PAGE_SIZE - 1);
    let raw_end = base.checked_add(size)?;
    let end = raw_end.checked_add(PAGE_SIZE - 1)? & !(PAGE_SIZE - 1);
    if start < 0xffff_8000_0000_0000 || end <= start { return None; }
    Some((start, end))
}

/// Pin all resident physical frames backing a kernel image section. Pageable
/// pages are faulted in first; the VM mapping lock then prevents eviction while
/// PFNs are translated and reference-counted by the physical-page pin registry.
fn pin_kernel_image_range(base: u64, end: u64) -> bool {
    if end <= base || base & (PAGE_SIZE - 1) != 0 || end & (PAGE_SIZE - 1) != 0 { return false; }
    let mut va = base;
    while va < end {
        if crate::kernel_vm::is_pageable_address(va)
            && !crate::kernel_vm::ensure_page_resident(va, false) { return false; }
        va += PAGE_SIZE;
    }
    let _guard = crate::kernel_vm::lock_pageable_mappings();
    let mut va = base;
    while va < end {
        let Some(pa) = translate(va, false) else {
            let mut undo = base;
            while undo < va {
                if let Some(old_pa) = translate(undo, false) { unlock_pfns(&[old_pa & !(PAGE_SIZE - 1)]); }
                undo += PAGE_SIZE;
            }
            return false;
        };
        if !crate::memory::is_ram_physical_address(pa & !(PAGE_SIZE - 1))
            || !lock_pfns(&[pa & !(PAGE_SIZE - 1)]) {
            let mut undo = base;
            while undo < va {
                if let Some(old_pa) = translate(undo, false) { unlock_pfns(&[old_pa & !(PAGE_SIZE - 1)]); }
                undo += PAGE_SIZE;
            }
            return false;
        }
        va += PAGE_SIZE;
    }
    true
}
fn unpin_kernel_image_range(base: u64, end: u64) {
    if end <= base { return; }
    let _guard = crate::kernel_vm::lock_pageable_mappings();
    let mut va = base;
    while va < end {
        if let Some(pa) = translate(va, false) { unlock_pfns(&[pa & !(PAGE_SIZE - 1)]); }
        va += PAGE_SIZE;
    }
}

/// Register one validated PE section after its pages have been mapped and
/// final protections applied. Non-pageable sections are pinned immediately.
pub fn register_driver_image_section(image_base: u64, section_base: u64, section_size: u64, pageable: bool) -> bool {
    let Some((base, end)) = aligned_image_range(section_base, section_size) else { return false; };
    let mut locks = PAGEABLE_IMAGE_LOCKS.lock();
    if locks.iter().any(|entry| entry.used && base < entry.section_end && entry.section_base < end) { return false; }
    let Some(index) = locks.iter().position(|entry| !entry.used) else { return false; };
    if !pageable && !pin_kernel_image_range(base, end) { return false; }
    locks[index] = PageableImageLock { used: true, section_base: base, section_end: end,
        image_base, references: 0, pageable, permanent_pin: !pageable };
    true
}

/// Remove all section metadata for a driver image after its code has stopped
/// executing and its unload callback has completed.
pub fn unregister_driver_image_sections(image_base: u64) {
    let mut release = [(0u64, 0u64); MAX_PAGEABLE_IMAGE_LOCKS];
    let count = {
        let mut locks = PAGEABLE_IMAGE_LOCKS.lock();
        let mut count = 0;
        for entry in locks.iter_mut().filter(|entry| entry.used && entry.image_base == image_base) {
            if entry.permanent_pin || entry.references != 0 {
                release[count] = (entry.section_base, entry.section_end);
                count += 1;
            }
            *entry = PageableImageLock::EMPTY;
        }
        count
    };
    for (base, end) in release[..count].iter().copied() { unpin_kernel_image_range(base, end); }
}

/// PVOID MmLockPagableDataSection(PVOID AddressWithinSection).
/// Return a stable, reference-counted section handle for any address in a
/// loader-registered PE section. Non-pageable sections are already pinned for
/// the image lifetime; pageable sections acquire one PFN pin set on first lock.
#[unsafe(export_name = "MmLockPagableDataSection")]
pub unsafe extern "win64" fn mm_lock_pagable_data_section(address: *const c_void) -> *mut c_void {
    let va = address as u64;
    let mut locks = PAGEABLE_IMAGE_LOCKS.lock();
    let Some(index) = locks.iter().position(|entry| entry.used
        && va >= entry.section_base && va < entry.section_end) else { return core::ptr::null_mut(); };
    let base = locks[index].section_base;
    let end = locks[index].section_end;
    let pageable = locks[index].pageable;
    if pageable && locks[index].references == 0 && !pin_kernel_image_range(base, end) {
        return core::ptr::null_mut();
    }
    let Some(next) = locks[index].references.checked_add(1) else {
        if pageable && locks[index].references == 0 {
            drop(locks);
            unpin_kernel_image_range(base, end);
        }
        return core::ptr::null_mut();
    };
    locks[index].references = next;
    core::ptr::addr_of_mut!(locks[index]).cast()
}

/// VOID MmUnlockPagableImageSection(PVOID ImageSectionHandle).
#[unsafe(export_name = "MmUnlockPagableImageSection")]
pub unsafe extern "win64" fn mm_unlock_pagable_image_section(handle: *mut c_void) {
    if handle.is_null() { return; }
    let release = {
        let mut locks = PAGEABLE_IMAGE_LOCKS.lock();
        let Some(index) = locks.iter().position(|entry| core::ptr::eq((entry as *const PageableImageLock).cast::<c_void>(), handle)) else { return; };
        let entry = &mut locks[index];
        if !entry.used || entry.references == 0 { return; }
        entry.references -= 1;
        if entry.pageable && entry.references == 0 {
            Some((entry.section_base, entry.section_end))
        } else {
            None
        }
    };
    if let Some((base, end)) = release { unpin_kernel_image_range(base, end); }
}

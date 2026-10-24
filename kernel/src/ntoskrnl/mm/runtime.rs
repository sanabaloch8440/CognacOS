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

/// Populate an MDL created by IoAllocateMdl for a nonpaged kernel buffer.
/// Preflights every mapping and the complete PFN destination before writing,
/// so malformed ranges never leave a partially populated PFN array.
pub unsafe extern "win64" fn mm_build_mdl_for_non_paged_pool(mdl: *mut c_void) {
    let address = mdl as u64;
    if mdl.is_null() || address < 0xffff_8000_0000_0000
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
            || cache_type.map_or(false, |c| c != 1) { return; }
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
    fn copy_memory_accepts_empty_copy_only_with_valid_flag_and_destination() {
        let mut transferred = 7u64;
        let invalid = unsafe {
            mm_copy_memory(core::ptr::null_mut(), 0, 0, 2, &mut transferred)
        };
        assert_eq!(invalid, STATUS_INVALID_PARAMETER);
        assert_eq!(transferred, 0);
    }
}

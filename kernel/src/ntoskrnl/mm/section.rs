//! Bounded section objects backed by owned physical pages.
//!
//! This is an initial in-kernel section primitive, not yet the complete NT
//! SECTION object/handle/security-manager ABI. Views are mapped transactionally;
//! each successful view owns a reference to the backing object.
use core::sync::atomic::{AtomicBool, Ordering};
use super::address_space::{AddressSpace, VmError};

const PAGE: u64 = 4096;
const MAX_SECTIONS: usize = 16;
const MAX_SECTION_PAGES: usize = 64;
const MAX_VIEWS: usize = 64;
const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;
const STATUS_NO_MEMORY: i32 = 0xC000_0017u32 as i32;
const STATUS_CONFLICTING_ADDRESSES: i32 = 0xC000_0018u32 as i32;
const STATUS_ACCESS_DENIED: i32 = 0xC000_0022u32 as i32;

#[derive(Clone, Copy)]
struct Section {
    live: bool,
    generation: u32,
    owner_root: u64,
    size: u64,
    references: u32,
    page_count: usize,
    pages: [u64; MAX_SECTION_PAGES],
}
impl Section {
    const EMPTY: Self = Self { live:false, generation:1, owner_root:0, size:0, references:0, page_count:0, pages:[0; MAX_SECTION_PAGES] };
}
#[derive(Clone, Copy)]
struct View {
    live: bool,
    section_slot: usize,
    generation: u32,
    root: u64,
    base: u64,
    length: u64,
    first_page: usize,
    page_count: usize,
}
impl View {
    const EMPTY: Self = Self { live:false, section_slot:0, generation:0, root:0, base:0, length:0, first_page:0, page_count:0 };
}
struct RegistryLock(AtomicBool);
impl RegistryLock {
    const fn new() -> Self { Self(AtomicBool::new(false)) }
    fn lock(&self) -> RegistryGuard<'_> {
        while self.0.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); }
        RegistryGuard(&self.0)
    }
}
struct RegistryGuard<'a>(&'a AtomicBool);
impl Drop for RegistryGuard<'_> { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }
static LOCK: RegistryLock = RegistryLock::new();
static mut SECTIONS: [Section; MAX_SECTIONS] = [Section::EMPTY; MAX_SECTIONS];
static mut VIEWS: [View; MAX_VIEWS] = [View::EMPTY; MAX_VIEWS];

fn decode_handle(handle: u64) -> Option<(usize, u32)> {
    let low = (handle & 0xff) as usize;
    let generation = (handle >> 8) as u32;
    if low == 0 || low > MAX_SECTIONS || generation == 0 { None } else { Some((low - 1, generation)) }
}
fn status(e: VmError) -> i32 {
    match e { VmError::OutOfMemory => STATUS_NO_MEMORY, VmError::PermissionDenied => STATUS_ACCESS_DENIED, _ => STATUS_INVALID_PARAMETER }
}
unsafe fn release_pages(pages: &[u64]) {
    for page in pages.iter().copied().filter(|p| *p != 0) { let _ = unsafe { super::physical::free_page(page) }; }
}
fn maybe_destroy_locked(sections: &mut [Section; MAX_SECTIONS], views: &[View; MAX_VIEWS], slot: usize) -> Option<[u64; MAX_SECTION_PAGES]> {
    if sections[slot].live && sections[slot].references == 0 && !views.iter().any(|v| v.live && v.section_slot == slot) {
        let pages = sections[slot].pages;
        let generation = sections[slot].generation.wrapping_add(1).max(1);
        sections[slot] = Section { generation, ..Section::EMPTY };
        Some(pages)
    } else { None }
}

/// Create a zero-initialized section object. Returns an opaque generation-tagged handle.
pub fn create(size: u64, owner_root: u64, handle_out: &mut u64) -> i32 {
    if size == 0 || owner_root == 0 { return STATUS_INVALID_PARAMETER; }
    let Some(rounded) = size.checked_add(PAGE - 1).map(|v| v & !(PAGE - 1)) else { return STATUS_INVALID_PARAMETER; };
    let pages = (rounded / PAGE) as usize;
    if pages == 0 || pages > MAX_SECTION_PAGES { return STATUS_INVALID_PARAMETER; }
    let mut backing = [0u64; MAX_SECTION_PAGES];
    for page in backing.iter_mut().take(pages) {
        match unsafe { super::physical::allocate_page() } {
            Ok(p) => *page = p,
            Err(_) => { unsafe { release_pages(&backing[..pages]); } return STATUS_NO_MEMORY; }
        }
    }
    let _guard = LOCK.lock();
    let sections = unsafe { &mut *core::ptr::addr_of_mut!(SECTIONS) };
    let views = unsafe { &*core::ptr::addr_of!(VIEWS) };
    let Some(slot) = sections.iter().position(|s| !s.live) else {
        drop(_guard); unsafe { release_pages(&backing[..pages]); } return STATUS_NO_MEMORY;
    };
    let generation = sections[slot].generation.max(1);
    sections[slot] = Section { live:true, generation, owner_root, size:rounded, references:1, page_count:pages, pages:backing };
    *handle_out = ((generation as u64) << 8) | (slot as u64 + 1);
    let _ = views;
    STATUS_SUCCESS
}

/// Add one object reference. Invalid/stale handles fail closed.
pub fn reference(handle: u64, owner_root: u64) -> i32 {
    let Some((slot, generation)) = decode_handle(handle) else { return STATUS_INVALID_HANDLE; };
    let _guard = LOCK.lock();
    let sections = unsafe { &mut *core::ptr::addr_of_mut!(SECTIONS) };
    let Some(section) = sections.get_mut(slot).filter(|s| s.live && s.generation == generation && s.owner_root == owner_root) else { return STATUS_INVALID_HANDLE; };
    let Some(next) = section.references.checked_add(1) else { return STATUS_NO_MEMORY; };
    section.references = next;
    STATUS_SUCCESS
}

/// Drop one object reference; mapped views keep the backing pages alive.
pub fn dereference(handle: u64, owner_root: u64) -> i32 {
    let Some((slot, generation)) = decode_handle(handle) else { return STATUS_INVALID_HANDLE; };
    let _guard = LOCK.lock();
    let sections = unsafe { &mut *core::ptr::addr_of_mut!(SECTIONS) };
    let views = unsafe { &mut *core::ptr::addr_of_mut!(VIEWS) };
    let Some(section) = sections.get_mut(slot).filter(|s| s.live && s.generation == generation && s.owner_root == owner_root) else { return STATUS_INVALID_HANDLE; };
    if section.references == 0 { return STATUS_INVALID_PARAMETER; }
    section.references -= 1;
    let pages = maybe_destroy_locked(sections, views, slot);
    drop(_guard);
    if let Some(pages) = pages { unsafe { release_pages(&pages); } }
    STATUS_SUCCESS
}

/// Map an aligned subrange into a target address space. On any mapping failure,
/// all pages installed by this call are removed before the error is returned.
pub fn map_view(handle: u64, root: u64, base: u64, offset: u64, length: u64, writable: bool, executable: bool) -> Result<u64, i32> {
    let Some((slot, generation)) = decode_handle(handle) else { return Err(STATUS_INVALID_HANDLE); };
    if root == 0 || base == 0 || base & (PAGE-1) != 0 || offset & (PAGE-1) != 0 || length == 0 || length & (PAGE-1) != 0 || (writable && executable) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let end_offset = offset.checked_add(length).ok_or(STATUS_INVALID_PARAMETER)?;
    let pages = (length / PAGE) as usize;
    let _guard = LOCK.lock();
    let sections = unsafe { &mut *core::ptr::addr_of_mut!(SECTIONS) };
    let views = unsafe { &mut *core::ptr::addr_of_mut!(VIEWS) };
    let section = sections.get(slot).filter(|s| s.live && s.generation == generation && s.owner_root == root).ok_or(STATUS_INVALID_HANDLE)?;
    if end_offset > section.size || end_offset / PAGE > section.page_count as u64 { return Err(STATUS_INVALID_PARAMETER); }
    let view_slot = views.iter().position(|v| !v.live).ok_or(STATUS_NO_MEMORY)?;
    let new_refs = section.references.checked_add(1).ok_or(STATUS_NO_MEMORY)?;
    let mut space = unsafe { AddressSpace::attach_existing(root) }.map_err(status)?;
    for i in 0..pages {
        let va = base.checked_add(i as u64 * PAGE).ok_or(STATUS_INVALID_PARAMETER)?;
        if let Err(e) = unsafe { space.map_user_page(va, section.pages[(offset / PAGE) as usize + i], writable, executable) } {
            for undo in (0..i).rev() { let _ = unsafe { space.unmap_user_page(base + undo as u64 * PAGE) }; }
            return Err(status(e));
        }
    }
    views[view_slot] = View { live:true, section_slot:slot, generation, root, base, length, first_page:(offset/PAGE) as usize, page_count:pages };
    sections[slot].references = new_refs;
    Ok(base)
}

/// Unmap exactly the base returned by map_view; partial-view unmaps are rejected.
pub fn unmap_view(root: u64, base: u64) -> i32 {
    let _guard = LOCK.lock();
    let sections = unsafe { &mut *core::ptr::addr_of_mut!(SECTIONS) };
    let views = unsafe { &mut *core::ptr::addr_of_mut!(VIEWS) };
    let Some(index) = views.iter().position(|v| v.live && v.root == root && v.base == base) else { return STATUS_INVALID_PARAMETER; };
    let view = views[index];
    let mut space = match unsafe { AddressSpace::attach_existing(root) } { Ok(s) => s, Err(e) => return status(e) };
    // Preflight every page before mutating, so an inconsistent view remains
    // registered and can be diagnosed/retried instead of half-releasing refs.
    for i in 0..view.page_count {
        if space.translate_user(view.base + i as u64 * PAGE, false).is_err() { return STATUS_INVALID_PARAMETER; }
    }
    for i in (0..view.page_count).rev() {
        if unsafe { space.unmap_user_page(view.base + i as u64 * PAGE) }.is_err() { return STATUS_INVALID_PARAMETER; }
    }
    views[index] = View::EMPTY;
    if let Some(section) = sections.get_mut(view.section_slot).filter(|s| s.live && s.generation == view.generation) {
        section.references = section.references.saturating_sub(1);
    }
    let pages = maybe_destroy_locked(sections, views, view.section_slot);
    drop(_guard);
    if let Some(pages) = pages { unsafe { release_pages(&pages); } }
    STATUS_SUCCESS
}

/// Copy bytes to/from section backing with bounds and overflow checks.
pub fn copy(handle: u64, owner_root: u64, offset: u64, buffer: &mut [u8], write: bool) -> i32 {
    let Some((slot, generation)) = decode_handle(handle) else { return STATUS_INVALID_HANDLE; };
    let Some(end) = offset.checked_add(buffer.len() as u64) else { return STATUS_INVALID_PARAMETER; };
    let _guard = LOCK.lock();
    let sections = unsafe { &*core::ptr::addr_of!(SECTIONS) };
    let Some(section) = sections.get(slot).filter(|s| s.live && s.generation == generation && s.owner_root == owner_root) else { return STATUS_INVALID_HANDLE; };
    if end > section.size { return STATUS_INVALID_PARAMETER; }
    let mut done = 0usize;
    while done < buffer.len() {
        let absolute = offset as usize + done;
        let page_index = absolute / PAGE as usize;
        let in_page = absolute % PAGE as usize;
        let count = (buffer.len() - done).min(PAGE as usize - in_page);
        let Some(pointer) = super::physical::physical_access_pointer(section.pages[page_index]) else { return STATUS_INVALID_PARAMETER; };
        unsafe {
            if write { core::ptr::copy_nonoverlapping(buffer.as_ptr().add(done), pointer.add(in_page), count); }
            else { core::ptr::copy_nonoverlapping(pointer.add(in_page), buffer.as_mut_ptr().add(done), count); }
        }
        done += count;
    }
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_section_ranges_and_handles() {
        let mut handle = 0;
        assert_eq!(create(0, &mut handle), STATUS_INVALID_PARAMETER);
        assert_eq!(reference(0,1), STATUS_INVALID_HANDLE);
        assert_eq!(dereference(0,1), STATUS_INVALID_HANDLE);
    }
}

//! NT Cache Manager stream metadata and clean page-backed read cache.
//!
//! This first architecture step removes the fixed global cache-map registry and
//! fixed per-stream page arrays. Cache maps and page descriptors have explicit
//! pool ownership and linked lifetimes. Data pages remain Memory Manager owned.
//!
//! This is still a clean-page cache: writes remain write-through until the
//! filesystem stream-provider and dirty-page writeback stages are implemented.
use core::ffi::c_void;
#[cfg(not(target_os = "none"))]
use core::sync::atomic::{AtomicBool, Ordering};

use crate::io::{FileObject, IoStatusBlock};

const STATUS_SUCCESS: i32 = 0;
const CACHE_PAGE_SIZE: usize = 4096;
const CACHE_MAP_TAG: u32 = u32::from_le_bytes(*b"CcMp");
const CACHE_PAGE_TAG: u32 = u32::from_le_bytes(*b"CcPg");

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CcFileSizes {
    pub allocation_size: i64,
    pub file_size: i64,
    pub valid_data_length: i64,
}

pub type CcAcquireForLazyWrite = Option<unsafe extern "win64" fn(*mut c_void, u8) -> u8>;
pub type CcReleaseFromLazyWrite = Option<unsafe extern "win64" fn(*mut c_void)>;
pub type CcAcquireForReadAhead = Option<unsafe extern "win64" fn(*mut c_void, u8) -> u8>;
pub type CcReleaseFromReadAhead = Option<unsafe extern "win64" fn(*mut c_void)>;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CacheManagerCallbacks {
    pub acquire_for_lazy_write: CcAcquireForLazyWrite,
    pub release_from_lazy_write: CcReleaseFromLazyWrite,
    pub acquire_for_read_ahead: CcAcquireForReadAhead,
    pub release_from_read_ahead: CcReleaseFromReadAhead,
}

struct CachePage {
    next: *mut CachePage,
    file_offset: u64,
    data: *mut u8,
    valid_start: u16,
    valid_end: u16,
}
unsafe impl Send for CachePage {}

struct CacheFileLink {
    next: *mut CacheFileLink,
    file: *mut FileObject,
}
unsafe impl Send for CacheFileLink {}

struct CacheMap {
    next: *mut CacheMap,
    // SECTION_OBJECT_POINTERS is the identity shared by file objects for a stream.
    // If absent, retain the conservative per-FILE_OBJECT identity.
    stream_identity: *mut c_void,
    files: *mut CacheFileLink,
    sizes: CcFileSizes,
    callbacks: CacheManagerCallbacks,
    lazy_write_context: *mut c_void,
    read_ahead_granularity: u32,
    pages: *mut CachePage,
}
unsafe impl Send for CacheMap {}

static mut MAPS_HEAD: *mut CacheMap = core::ptr::null_mut();

#[cfg(target_os = "none")]
static LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
#[cfg(not(target_os = "none"))]
static LOCK: AtomicBool = AtomicBool::new(false);

struct Guard {
    #[cfg(target_os = "none")]
    _guard: crate::hal::IrqSpinLockGuard<'static, ()>,
}
impl Guard {
    fn acquire() -> Self {
        #[cfg(target_os = "none")]
        { Self { _guard: LOCK.lock() } }
        #[cfg(not(target_os = "none"))]
        {
            while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
                core::hint::spin_loop();
            }
            Self {}
        }
    }
}
#[cfg(not(target_os = "none"))]
impl Drop for Guard {
    fn drop(&mut self) { LOCK.store(false, Ordering::Release); }
}

fn valid_sizes(s: CcFileSizes) -> bool {
    s.allocation_size >= 0 && s.file_size >= 0 && s.valid_data_length >= 0
        && s.valid_data_length <= s.file_size && s.file_size <= s.allocation_size
}

unsafe fn stream_identity(file: *mut FileObject) -> *mut c_void {
    if file.is_null() { return core::ptr::null_mut(); }
    let section = unsafe { (*file).section_object_pointer };
    if section.is_null() { file.cast() } else { section }
}

unsafe fn find_stream(identity: *mut c_void) -> *mut CacheMap {
    if identity.is_null() { return core::ptr::null_mut(); }
    let mut map = unsafe { MAPS_HEAD };
    while !map.is_null() {
        if unsafe { (*map).stream_identity == identity } { return map; }
        map = unsafe { (*map).next };
    }
    core::ptr::null_mut()
}

unsafe fn find_map(file: *mut FileObject) -> *mut CacheMap {
    if file.is_null() { return core::ptr::null_mut(); }
    let map = unsafe { find_stream(stream_identity(file)) };
    if map.is_null() { return map; }
    let mut link = unsafe { (*map).files };
    while !link.is_null() {
        if unsafe { (*link).file == file } { return map; }
        link = unsafe { (*link).next };
    }
    core::ptr::null_mut()
}

const CACHE_FILE_LINK_TAG: u32 = u32::from_le_bytes(*b"CcFl");

unsafe fn allocate_file_link(file: *mut FileObject) -> *mut CacheFileLink {
    let link = crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(
        core::mem::size_of::<CacheFileLink>(), CACHE_FILE_LINK_TAG, true,
    ).cast::<CacheFileLink>();
    if !link.is_null() {
        unsafe { link.write(CacheFileLink { next: core::ptr::null_mut(), file }); }
    }
    link
}

unsafe fn free_file_link(link: *mut CacheFileLink) {
    if !link.is_null() {
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            link.cast(), Some(CACHE_FILE_LINK_TAG),
        );
    }
}

// A single valid interval cannot represent disjoint cached fragments. Merge
// only overlapping or adjacent ranges; otherwise leave the new fragment uncached.
fn can_extend_valid_range(page: &CachePage, start: usize, end: usize) -> bool {
    start <= page.valid_end as usize && end >= page.valid_start as usize
}

fn extend_valid_range(page: &mut CachePage, start: usize, end: usize) {
    page.valid_start = page.valid_start.min(start as u16);
    page.valid_end = page.valid_end.max(end as u16);
}

unsafe fn set_section_cache_map(file: *mut FileObject, map: *mut CacheMap) {
    if file.is_null() { return; }
    let section = unsafe {
        (*file).section_object_pointer.cast::<crate::nt_abi::io::SECTION_OBJECT_POINTERS>()
    };
    if !section.is_null() {
        unsafe { (*section).shared_cache_map = map.cast(); }
    }
}

unsafe fn read_sizes(ptr: *const CcFileSizes) -> Option<CcFileSizes> {
    if ptr.is_null() { return None; }
    let s = unsafe { ptr.read_unaligned() };
    valid_sizes(s).then_some(s)
}

unsafe fn allocate_map(identity: *mut c_void, sizes: CcFileSizes,
    callbacks: CacheManagerCallbacks, context: *mut c_void) -> *mut CacheMap {
    let p = crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(
        core::mem::size_of::<CacheMap>(), CACHE_MAP_TAG, true,
    ).cast::<CacheMap>();
    if p.is_null() { return p; }
    unsafe {
        p.write(CacheMap {
            next: core::ptr::null_mut(), stream_identity: identity,
            files: core::ptr::null_mut(), sizes, callbacks,
            lazy_write_context: context, read_ahead_granularity: 0x10000,
            pages: core::ptr::null_mut(),
        });
    }
    p
}

unsafe fn free_page_list(mut page: *mut CachePage) {
    while !page.is_null() {
        let next = unsafe { (*page).next };
        let data = unsafe { (*page).data };
        if !data.is_null() { let _ = crate::ntoskrnl::mm::kernel_space::free_pages(data, 1); }
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(page.cast(), Some(CACHE_PAGE_TAG));
        page = next;
    }
}

unsafe fn allocate_page_descriptor(offset: u64, data: *mut u8,
    valid_start: usize, valid_end: usize) -> *mut CachePage {
    let p = crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(
        core::mem::size_of::<CachePage>(), CACHE_PAGE_TAG, true,
    ).cast::<CachePage>();
    if p.is_null() { return p; }
    unsafe {
        p.write(CachePage {
            next: core::ptr::null_mut(), file_offset: offset, data,
            valid_start: valid_start as u16, valid_end: valid_end as u16,
        });
    }
    p
}

/// VOID CcInitializeCacheMap(PFILE_OBJECT, PCC_FILE_SIZES, BOOLEAN,
///                           PCACHE_MANAGER_CALLBACKS, PVOID).
#[unsafe(export_name = "CcInitializeCacheMap")]
pub unsafe extern "win64" fn cc_initialize_cache_map(
    file: *mut FileObject,
    sizes: *const CcFileSizes,
    pin_access: u8,
    callbacks: *const CacheManagerCallbacks,
    lazy_write_context: *mut c_void,
) {
    if file.is_null() || pin_access > 1 { return; }
    let Some(sizes) = (unsafe { read_sizes(sizes) }) else { return; };
    let callbacks = if callbacks.is_null() { CacheManagerCallbacks::default() }
        else { unsafe { callbacks.read_unaligned() } };

    // Allocate candidate metadata before taking the lock. File links allow
    // multiple FILE_OBJECTs for one SECTION_OBJECT_POINTERS stream to share
    // the same cache map and cached pages.
    let identity = unsafe { stream_identity(file) };
    let candidate = unsafe { allocate_map(identity, sizes, callbacks, lazy_write_context) };
    if candidate.is_null() { return; }
    let candidate_link = unsafe { allocate_file_link(file) };
    if candidate_link.is_null() {
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(candidate.cast(), Some(CACHE_MAP_TAG));
        return;
    }

    let mut discard_map = false;
    let mut discard_link = false;
    {
        let _guard = Guard::acquire();
        let existing = unsafe { find_stream(identity) };
        if !existing.is_null() {
            let mut attached = unsafe { (*existing).files };
            while !attached.is_null() && unsafe { (*attached).file != file } {
                attached = unsafe { (*attached).next };
            }
            unsafe {
                (*existing).sizes = sizes;
                (*existing).callbacks = callbacks;
                (*existing).lazy_write_context = lazy_write_context;
                (*file).private_cache_map = existing.cast();
                if attached.is_null() {
                    (*candidate_link).next = (*existing).files;
                    (*existing).files = candidate_link;
                } else {
                    discard_link = true;
                }
                set_section_cache_map(file, existing);
            }
            discard_map = true;
        } else {
            unsafe {
                (*candidate).files = candidate_link;
                (*candidate).next = MAPS_HEAD;
                MAPS_HEAD = candidate;
                (*file).private_cache_map = candidate.cast();
                set_section_cache_map(file, candidate);
            }
        }
    }
    if discard_link { unsafe { free_file_link(candidate_link); } }
    if discard_map {
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(candidate.cast(), Some(CACHE_MAP_TAG));
    }
}

/// BOOLEAN CcUninitializeCacheMap(PFILE_OBJECT, PLARGE_INTEGER,
///                                PCACHE_UNINITIALIZE_EVENT).
#[unsafe(export_name = "CcUninitializeCacheMap")]
pub unsafe extern "win64" fn cc_uninitialize_cache_map(
    file: *mut FileObject,
    _truncate_size: *const i64,
    _uninitialize_event: *mut c_void,
) -> u8 {
    if file.is_null() { return 0; }
    let (map_to_free, link_to_free) = {
        let _guard = Guard::acquire();
        let mut map_link = core::ptr::addr_of_mut!(MAPS_HEAD);
        let mut map = unsafe { *map_link };
        let mut found_link = core::ptr::null_mut::<CacheFileLink>();
        let mut previous = core::ptr::null_mut::<CacheFileLink>();
        while !map.is_null() {
            let mut link = unsafe { (*map).files };
            previous = core::ptr::null_mut();
            while !link.is_null() && unsafe { (*link).file != file } {
                previous = link;
                link = unsafe { (*link).next };
            }
            if !link.is_null() { found_link = link; break; }
            map_link = unsafe { core::ptr::addr_of_mut!((*map).next) };
            map = unsafe { *map_link };
        }
        if map.is_null() || found_link.is_null() { return 1; }
        unsafe {
            if previous.is_null() { (*map).files = (*found_link).next; }
            else { (*previous).next = (*found_link).next; }
            (*file).private_cache_map = core::ptr::null_mut();
        }
        let last_file = unsafe { (*map).files.is_null() };
        if last_file {
            unsafe {
                *map_link = (*map).next;
                let section = (*file).section_object_pointer.cast::<crate::nt_abi::io::SECTION_OBJECT_POINTERS>();
                if !section.is_null() && (*section).shared_cache_map == map.cast() {
                    (*section).shared_cache_map = core::ptr::null_mut();
                }
            }
            (map, found_link)
        } else {
            (core::ptr::null_mut(), found_link)
        }
    };
    unsafe { free_file_link(link_to_free); }
    if !map_to_free.is_null() {
        unsafe {
            free_page_list((*map_to_free).pages);
            let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(map_to_free.cast(), Some(CACHE_MAP_TAG));
        }
    }
    1
}

/// VOID CcSetFileSizes(PFILE_OBJECT, PCC_FILE_SIZES).
#[unsafe(export_name = "CcSetFileSizes")]
pub unsafe extern "win64" fn cc_set_file_sizes(file: *mut FileObject, sizes: *const CcFileSizes) {
    if file.is_null() { return; }
    let Some(sizes) = (unsafe { read_sizes(sizes) }) else { return; };
    let mut release = core::ptr::null_mut::<CachePage>();
    {
        let _guard = Guard::acquire();
        let map = unsafe { find_map(file) };
        if map.is_null() { return; }
        unsafe { (*map).sizes = sizes; }
        let mut link = unsafe { core::ptr::addr_of_mut!((*map).pages) };
        let mut page = unsafe { *link };
        while !page.is_null() {
            let next = unsafe { (*page).next };
            let page_end = unsafe { (*page).file_offset }.saturating_add(CACHE_PAGE_SIZE as u64);
            // Cached clean bytes must never survive beyond VDL, even when
            // EOF is larger. This prevents a later read from exposing bytes
            // invalidated by a filesystem size/VDL update.
            let cache_limit = core::cmp::min(sizes.file_size, sizes.valid_data_length) as u64;
            let page_offset = unsafe { (*page).file_offset };
            let remove = page_offset >= cache_limit;
            if remove {
                unsafe {
                    *link = next;
                    (*page).next = release;
                    release = page;
                }
            } else {
                if page_end > cache_limit {
                    let valid_end = (cache_limit - page_offset) as usize;
                    unsafe { (*page).valid_end = (*page).valid_end.min(valid_end as u16); }
                }
                let empty_interval = unsafe { (*page).valid_start >= (*page).valid_end };
                if empty_interval {
                    unsafe {
                        *link = next;
                        (*page).next = release;
                        release = page;
                    }
                } else {
                    link = unsafe { core::ptr::addr_of_mut!((*page).next) };
                }
            }
            page = next;
        }
    }
    unsafe { free_page_list(release); }
}

/// BOOLEAN CcCopyRead(...). A miss requests the ordinary filesystem IRP path.
#[unsafe(export_name = "CcCopyRead")]
pub unsafe extern "win64" fn cc_copy_read(
    file: *mut FileObject, offset: *const i64, length: u32, wait: u8,
    buffer: *mut c_void, iosb: *mut IoStatusBlock,
) -> u8 {
    if file.is_null() || offset.is_null() || (length != 0 && buffer.is_null())
        || iosb.is_null() || wait > 1 { return 0; }
    let offset = unsafe { offset.read_unaligned() };
    if offset < 0 { return 0; }
    let length = length as usize;
    let _guard = Guard::acquire();
    let map = unsafe { find_map(file) };
    if map.is_null() { return 0; }
    let sizes = unsafe { (*map).sizes };
    let end = match (offset as u64).checked_add(length as u64) { Some(v) => v, None => return 0 };
    if end > sizes.valid_data_length as u64 || end > sizes.file_size as u64 { return 0; }

    let mut done = 0usize;
    while done < length {
        let absolute = offset as u64 + done as u64;
        let page_offset = absolute & !((CACHE_PAGE_SIZE as u64) - 1);
        let within = (absolute - page_offset) as usize;
        let amount = core::cmp::min(CACHE_PAGE_SIZE - within, length - done);
        let mut page = unsafe { (*map).pages };
        while !page.is_null() {
            let hit = unsafe { (*page).file_offset == page_offset && !(*page).data.is_null()
                && within >= (*page).valid_start as usize
                && within + amount <= (*page).valid_end as usize };
            if hit { break; }
            page = unsafe { (*page).next };
        }
        if page.is_null() { return 0; }
        done += amount;
    }

    done = 0;
    while done < length {
        let absolute = offset as u64 + done as u64;
        let page_offset = absolute & !((CACHE_PAGE_SIZE as u64) - 1);
        let within = (absolute - page_offset) as usize;
        let amount = core::cmp::min(CACHE_PAGE_SIZE - within, length - done);
        let mut page = unsafe { (*map).pages };
        while !page.is_null() && unsafe { (*page).file_offset != page_offset } {
            page = unsafe { (*page).next };
        }
        if page.is_null() { return 0; }
        unsafe { core::ptr::copy_nonoverlapping((*page).data.add(within), buffer.cast::<u8>().add(done), amount); }
        done += amount;
    }
    unsafe {
        core::ptr::write_unaligned(iosb.cast::<u8>().cast::<i32>(), STATUS_SUCCESS);
        core::ptr::write_unaligned(iosb.cast::<u8>().add(8).cast::<usize>(), length);
    }
    1
}

/// BOOLEAN CcCopyWrite(...). Write-through remains active until dirty writeback exists.
#[unsafe(export_name = "CcCopyWrite")]
pub unsafe extern "win64" fn cc_copy_write(
    file: *mut FileObject, offset: *const i64, length: u32, wait: u8, buffer: *const c_void,
) -> u8 {
    if file.is_null() || offset.is_null() || (length != 0 && buffer.is_null()) || wait > 1 { return 0; }
    0
}

/// VOID CcFlushCache(...). No dirty pages are currently admitted.
#[unsafe(export_name = "CcFlushCache")]
pub unsafe extern "win64" fn cc_flush_cache(
    _section_object_pointers: *mut c_void, _file_offset: *const i64,
    _length: u32, iosb: *mut IoStatusBlock,
) {
    if !iosb.is_null() {
        unsafe {
            core::ptr::write_unaligned(iosb.cast::<u8>().cast::<i32>(), STATUS_SUCCESS);
            core::ptr::write_unaligned(iosb.cast::<u8>().add(8).cast::<usize>(), 0);
        }
    }
}

/// VOID CcSetReadAheadGranularity(PFILE_OBJECT, ULONG).
#[unsafe(export_name = "CcSetReadAheadGranularity")]
pub unsafe extern "win64" fn cc_set_read_ahead_granularity(file: *mut FileObject, granularity: u32) {
    if file.is_null() || granularity == 0 || !granularity.is_power_of_two() { return; }
    let _guard = Guard::acquire();
    let map = unsafe { find_map(file) };
    if !map.is_null() { unsafe { (*map).read_ahead_granularity = granularity.min(1 << 20); } }
}

/// BOOLEAN CcCanIWrite(PFILE_OBJECT, ULONG, BOOLEAN, BOOLEAN).
#[unsafe(export_name = "CcCanIWrite")]
pub unsafe extern "win64" fn cc_can_i_write(file: *mut FileObject, _bytes: u32, _wait: u8, _retrying: u8) -> u8 {
    if file.is_null() { return 0; }
    let _guard = Guard::acquire();
    (!unsafe { find_map(file) }.is_null()) as u8
}

/// Insert clean filesystem data into an MM-owned cache page. Descriptor and data
/// allocation occur outside the cache metadata lock; insertion rechecks identity.
pub(crate) fn cache_insert_clean(file: *mut FileObject, offset: u64, data: &[u8]) {
    if file.is_null() || data.is_empty()
        || unsafe { crate::ntoskrnl::ke::runtime::ke_get_current_irql() } > 1 { return; }
    // Never populate cache beyond the current file size or valid-data
    // boundary. In particular, zero-filled page tails must not become visible
    // later if VDL is extended without a corresponding filesystem read.
    let cache_limit = {
        let _guard = Guard::acquire();
        let map = unsafe { find_map(file) };
        if map.is_null() { return; }
        let sizes = unsafe { (*map).sizes };
        core::cmp::min(sizes.file_size, sizes.valid_data_length) as u64
    };
    if offset >= cache_limit { return; }
    let Some(request_end) = offset.checked_add(data.len() as u64) else { return; };
    let bounded_len = core::cmp::min(request_end, cache_limit).saturating_sub(offset) as usize;
    if bounded_len == 0 { return; }
    let data = &data[..bounded_len];

    let mut done = 0usize;
    while done < data.len() {
        let Some(absolute) = offset.checked_add(done as u64) else { return; };
        let page_offset = absolute & !((CACHE_PAGE_SIZE as u64) - 1);
        let within = (absolute - page_offset) as usize;
        let amount = core::cmp::min(CACHE_PAGE_SIZE - within, data.len() - done);
        let chunk = &data[done..done + amount];

        {
            let _guard = Guard::acquire();
            let map = unsafe { find_map(file) };
            if map.is_null() { return; }
            let mut page = unsafe { (*map).pages };
            while !page.is_null() && unsafe { (*page).file_offset != page_offset } {
                page = unsafe { (*page).next };
            }
            if !page.is_null() {
                unsafe {
                    if can_extend_valid_range(&*page, within, within + amount) {
                        core::ptr::copy_nonoverlapping(chunk.as_ptr(), (*page).data.add(within), amount);
                        extend_valid_range(&mut *page, within, within + amount);
                    }
                }
                // This descriptor tracks one contiguous interval. A disjoint
                // fragment is left uncached rather than making the gap valid.
                done += amount;
                continue;
            }
        }

        let Ok(new_data) = crate::ntoskrnl::mm::kernel_space::allocate_pages(1) else { return; };
        unsafe { core::ptr::write_bytes(new_data, 0, CACHE_PAGE_SIZE); }
        unsafe { core::ptr::copy_nonoverlapping(chunk.as_ptr(), new_data.add(within), amount); }
        let descriptor = unsafe { allocate_page_descriptor(page_offset, new_data, within, within + amount) };
        if descriptor.is_null() {
            let _ = crate::ntoskrnl::mm::kernel_space::free_pages(new_data, 1);
            return;
        }

        let inserted = {
            let _guard = Guard::acquire();
            let map = unsafe { find_map(file) };
            if map.is_null() { false } else {
                let mut page = unsafe { (*map).pages };
                while !page.is_null() && unsafe { (*page).file_offset != page_offset } {
                    page = unsafe { (*page).next };
                }
                if page.is_null() {
                    unsafe {
                        (*descriptor).next = (*map).pages;
                        (*map).pages = descriptor;
                    }
                    true
                } else {
                    unsafe {
                        if can_extend_valid_range(&*page, within, within + amount) {
                            core::ptr::copy_nonoverlapping(chunk.as_ptr(), (*page).data.add(within), amount);
                            extend_valid_range(&mut *page, within, within + amount);
                        }
                    }
                    false
                }
            }
        };
        if !inserted {
            unsafe { free_page_list(descriptor); }
        }
        done += amount;
    }
}

/// Invalidate clean cached pages overlapping a write before issuing the write IRP.
pub(crate) fn cache_invalidate_range(file: *mut FileObject, offset: u64, length: usize) {
    if file.is_null() || length == 0 { return; }
    let Some(end) = offset.checked_add(length as u64) else { return; };
    let mut release = core::ptr::null_mut::<CachePage>();
    {
        let _guard = Guard::acquire();
        let map = unsafe { find_map(file) };
        if map.is_null() { return; }
        let mut link = unsafe { core::ptr::addr_of_mut!((*map).pages) };
        let mut page = unsafe { *link };
        while !page.is_null() {
            let next = unsafe { (*page).next };
            let overlaps = unsafe { (*page).file_offset < end
                && (*page).file_offset.saturating_add(CACHE_PAGE_SIZE as u64) > offset };
            if overlaps {
                unsafe { *link = next; (*page).next = release; release = page; }
            } else {
                link = unsafe { core::ptr::addr_of_mut!((*page).next) };
            }
            page = next;
        }
    }
    unsafe { free_page_list(release); }
}

pub(crate) fn cache_map_count() -> usize {
    let _guard = Guard::acquire();
    let mut count = 0usize;
    let mut map = unsafe { MAPS_HEAD };
    while !map.is_null() { count += 1; map = unsafe { (*map).next }; }
    count
}

#[cfg(test)]
fn cache_page_count(file: *mut FileObject) -> usize {
    let _guard = Guard::acquire();
    let map = unsafe { find_map(file) };
    if map.is_null() { return 0; }
    let mut count = 0usize;
    let mut page = unsafe { (*map).pages };
    while !page.is_null() { count += 1; page = unsafe { (*page).next }; }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicBool, Ordering};

    static TEST_LOCK: AtomicBool = AtomicBool::new(false);
    struct TestGuard;
    impl TestGuard {
        fn acquire() -> Self {
            while TEST_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
                core::hint::spin_loop();
            }
            Self
        }
    }
    impl Drop for TestGuard {
        fn drop(&mut self) { TEST_LOCK.store(false, Ordering::Release); }
    }

    #[repr(align(4096))]
    struct TestCachePage([u8; CACHE_PAGE_SIZE]);
    static mut TEST_CACHE_PAGE: TestCachePage = TestCachePage([0; CACHE_PAGE_SIZE]);

    #[test]
    fn copy_read_hits_a_dynamically_described_cache_page() {
        let _test_guard = TestGuard::acquire();
        let mut file: FileObject = unsafe { core::mem::zeroed() };
        let sizes = CcFileSizes { allocation_size: 4096, file_size: 4096, valid_data_length: 4096 };
        unsafe { cc_initialize_cache_map(&mut file, &sizes, 0, core::ptr::null(), core::ptr::null_mut()); }
        let expected = b"cached-data";
        let page_data = core::ptr::addr_of_mut!(TEST_CACHE_PAGE.0).cast::<u8>();
        unsafe { core::ptr::copy_nonoverlapping(expected.as_ptr(), page_data.add(32), expected.len()); }
        let descriptor = unsafe {
            allocate_page_descriptor(0, page_data, 32, 32 + expected.len())
        };
        assert!(!descriptor.is_null());
        {
            let _guard = Guard::acquire();
            let map = unsafe { find_map(&mut file) };
            assert!(!map.is_null());
            unsafe { (*map).pages = descriptor; }
        }

        let offset = 32i64;
        let mut output = [0u8; 11];
        let mut iosb = IoStatusBlock {
            status: crate::nt_abi::io::IO_STATUS_BLOCK_STATUS { status: 0 },
            information: 0,
        };
        assert_eq!(unsafe {
            cc_copy_read(&mut file, &offset, output.len() as u32, 1,
                output.as_mut_ptr().cast(), &mut iosb)
        }, 1);
        assert_eq!(&output, expected);
        assert_eq!(iosb.information, expected.len());

        // The backing bytes are static test storage, not MM-owned pages.
        // Detach the descriptor before uninitializing the map.
        {
            let _guard = Guard::acquire();
            let map = unsafe { find_map(&mut file) };
            unsafe { (*map).pages = core::ptr::null_mut(); }
        }
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            descriptor.cast(), Some(CACHE_PAGE_TAG),
        );
        unsafe { cc_uninitialize_cache_map(&mut file, core::ptr::null(), core::ptr::null_mut()); }
    }

    #[test]
    fn cache_map_lifecycle_uses_dynamically_owned_metadata() {
        let _test_guard = TestGuard::acquire();
        let baseline = cache_map_count();
        let mut file: FileObject = unsafe { core::mem::zeroed() };
        let mut section = crate::nt_abi::io::SECTION_OBJECT_POINTERS::default();
        file.section_object_pointer = &mut section as *mut _ as *mut c_void;
        let sizes = CcFileSizes { allocation_size: 8192, file_size: 4096, valid_data_length: 2048 };
        unsafe { cc_initialize_cache_map(&mut file, &sizes, 0, core::ptr::null(), core::ptr::null_mut()); }
        assert!(!file.private_cache_map.is_null());
        assert_eq!(cache_map_count(), baseline + 1);
        assert_eq!(section.shared_cache_map, file.private_cache_map);
        let changed = CcFileSizes { allocation_size: 8192, file_size: 2048, valid_data_length: 1024 };
        unsafe { cc_set_file_sizes(&mut file, &changed); }
        unsafe { cc_set_read_ahead_granularity(&mut file, 0x20000); }
        assert_eq!(unsafe { (*file.private_cache_map.cast::<CacheMap>()).read_ahead_granularity }, 0x20000);
        assert_eq!(unsafe { (*file.private_cache_map.cast::<CacheMap>()).sizes.file_size }, 2048);
        assert_eq!(unsafe { cc_uninitialize_cache_map(&mut file, core::ptr::null(), core::ptr::null_mut()) }, 1);
        assert!(file.private_cache_map.is_null());
        assert_eq!(section.shared_cache_map, core::ptr::null_mut());
        assert_eq!(cache_map_count(), baseline);
    }

    #[test]
    fn file_objects_for_same_section_share_one_stream_cache_map() {
        let _test_guard = TestGuard::acquire();
        let baseline = cache_map_count();
        let mut first: FileObject = unsafe { core::mem::zeroed() };
        let mut second: FileObject = unsafe { core::mem::zeroed() };
        let mut section = crate::nt_abi::io::SECTION_OBJECT_POINTERS::default();
        first.section_object_pointer = &mut section as *mut _ as *mut c_void;
        second.section_object_pointer = &mut section as *mut _ as *mut c_void;
        let sizes = CcFileSizes { allocation_size: 8192, file_size: 4096, valid_data_length: 4096 };

        unsafe {
            cc_initialize_cache_map(&mut first, &sizes, 0, core::ptr::null(), core::ptr::null_mut());
            cc_initialize_cache_map(&mut second, &sizes, 0, core::ptr::null(), core::ptr::null_mut());
        }

        assert!(!first.private_cache_map.is_null());
        assert_eq!(first.private_cache_map, second.private_cache_map);
        assert_eq!(section.shared_cache_map, first.private_cache_map);
        assert_eq!(cache_map_count(), baseline + 1);

        // A page populated through the first FILE_OBJECT must be visible
        // through the second FILE_OBJECT because both attach to one stream map.
        let expected = b"shared-stream";
        let page_data = core::ptr::addr_of_mut!(TEST_CACHE_PAGE.0).cast::<u8>();
        unsafe { core::ptr::copy_nonoverlapping(expected.as_ptr(), page_data.add(64), expected.len()); }
        let descriptor = unsafe { allocate_page_descriptor(0, page_data, 64, 64 + expected.len()) };
        assert!(!descriptor.is_null());
        {
            let _guard = Guard::acquire();
            let map = unsafe { find_map(&mut first) };
            assert!(!map.is_null());
            unsafe { (*map).pages = descriptor; }
        }
        let offset = 64i64;
        let mut output = [0u8; 13];
        let mut iosb = IoStatusBlock {
            status: crate::nt_abi::io::IO_STATUS_BLOCK_STATUS { status: 0 },
            information: 0,
        };
        assert_eq!(unsafe { cc_copy_read(&mut second, &offset, output.len() as u32, 1,
            output.as_mut_ptr().cast(), &mut iosb) }, 1);
        assert_eq!(&output, expected);

        assert_eq!(unsafe { cc_uninitialize_cache_map(&mut first, core::ptr::null(), core::ptr::null_mut()) }, 1);
        assert!(first.private_cache_map.is_null());
        assert_eq!(second.private_cache_map, section.shared_cache_map);
        assert_eq!(cache_map_count(), baseline + 1);

        output.fill(0);
        assert_eq!(unsafe { cc_copy_read(&mut second, &offset, output.len() as u32, 1,
            output.as_mut_ptr().cast(), &mut iosb) }, 1);
        assert_eq!(&output, expected);

        // The backing bytes are static test storage, not MM-owned pages.
        {
            let _guard = Guard::acquire();
            let map = unsafe { find_map(&mut second) };
            unsafe { (*map).pages = core::ptr::null_mut(); }
        }
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(
            descriptor.cast(), Some(CACHE_PAGE_TAG),
        );

        assert_eq!(unsafe { cc_uninitialize_cache_map(&mut second, core::ptr::null(), core::ptr::null_mut()) }, 1);
        assert!(second.private_cache_map.is_null());
        assert!(section.shared_cache_map.is_null());
        assert_eq!(cache_map_count(), baseline);
    }

    #[test]
    fn cache_metadata_has_no_128_stream_or_eight_page_ceiling() {
        let _test_guard = TestGuard::acquire();
        const STREAMS: usize = 160;
        let baseline = cache_map_count();
        let sizes = CcFileSizes { allocation_size: 1 << 20, file_size: 1 << 20, valid_data_length: 1 << 20 };
        let mut files: [core::mem::MaybeUninit<FileObject>; STREAMS] =
            [const { core::mem::MaybeUninit::uninit() }; STREAMS];
        for entry in files.iter_mut() {
            let file = entry.write(unsafe { core::mem::zeroed() }) as *mut FileObject;
            unsafe { cc_initialize_cache_map(file, &sizes, 0, core::ptr::null(), core::ptr::null_mut()); }
            assert!(!unsafe { (*file).private_cache_map }.is_null());
        }
        assert_eq!(cache_map_count(), baseline + STREAMS);

        let file = files[0].as_mut_ptr();
        let mut head = core::ptr::null_mut::<CachePage>();
        for index in 0..12usize {
            let page = unsafe { allocate_page_descriptor((index * CACHE_PAGE_SIZE) as u64, core::ptr::null_mut(), 0, CACHE_PAGE_SIZE) };
            assert!(!page.is_null());
            unsafe { (*page).next = head; }
            head = page;
        }
        {
            let _guard = Guard::acquire();
            let map = unsafe { find_map(file) };
            assert!(!map.is_null());
            unsafe { (*map).pages = head; }
        }
        assert_eq!(cache_page_count(file), 12);

        for entry in files.iter_mut() {
            let file = entry.as_mut_ptr();
            assert_eq!(unsafe { cc_uninitialize_cache_map(file, core::ptr::null(), core::ptr::null_mut()) }, 1);
        }
        assert_eq!(cache_map_count(), baseline);
    }

    #[test]
    fn copy_read_preserves_buffer_on_vdl_miss_and_accepts_zero_length() {
        let _test_guard = TestGuard::acquire();
        let mut file: FileObject = unsafe { core::mem::zeroed() };
        let sizes = CcFileSizes {
            allocation_size: 4096, file_size: 4096, valid_data_length: 4,
        };
        unsafe { cc_initialize_cache_map(&mut file, &sizes, 0, core::ptr::null(), core::ptr::null_mut()); }

        let mut output = [0xA5u8; 2];
        let mut iosb = IoStatusBlock {
            status: crate::nt_abi::io::IO_STATUS_BLOCK_STATUS { status: 0x1234 },
            information: 77,
        };
        let crossing_vdl = 3i64;
        assert_eq!(unsafe {
            cc_copy_read(&mut file, &crossing_vdl, 2, 1, output.as_mut_ptr().cast(), &mut iosb)
        }, 0);
        assert_eq!(output, [0xA5; 2], "a failed cache read must not partially modify the caller buffer");
        assert_eq!(unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(iosb).cast::<i32>()) }, 0x1234);
        assert_eq!(iosb.information, 77);

        let zero_offset = 0i64;
        assert_eq!(unsafe {
            cc_copy_read(&mut file, &zero_offset, 0, 1, core::ptr::null_mut(), &mut iosb)
        }, 1);
        assert_eq!(unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(iosb).cast::<i32>()) }, STATUS_SUCCESS);
        assert_eq!(iosb.information, 0);

        let negative_offset = -1i64;
        assert_eq!(unsafe {
            cc_copy_read(&mut file, &negative_offset, 1, 1, output.as_mut_ptr().cast(), &mut iosb)
        }, 0);
        assert_eq!(output, [0xA5; 2]);

        assert_eq!(unsafe { cc_uninitialize_cache_map(&mut file, core::ptr::null(), core::ptr::null_mut()) }, 1);
    }

    #[test]
    fn clean_insert_never_caches_bytes_beyond_vdl() {
        let _test_guard = TestGuard::acquire();
        let mut file: FileObject = unsafe { core::mem::zeroed() };
        let sizes = CcFileSizes {
            allocation_size: 4096, file_size: 4096, valid_data_length: 4,
        };
        unsafe { cc_initialize_cache_map(&mut file, &sizes, 0, core::ptr::null(), core::ptr::null_mut()); }
        let bytes = [0x5Au8; 16];
        cache_insert_clean(&mut file, 0, &bytes);
        let _guard = Guard::acquire();
        let map = unsafe { find_map(&mut file) };
        assert!(!map.is_null());
        let page = unsafe { (*map).pages };
        assert!(!page.is_null());
        assert_eq!(unsafe { (*page).valid_start }, 0);
        assert_eq!(unsafe { (*page).valid_end }, 4);
        assert_eq!(unsafe { (*page).data.add(3).read() }, 0x5A);
        assert_eq!(unsafe { (*page).data.add(4).read() }, 0);
        assert!(unsafe { (*page).next.is_null() });
    }

    #[test]
    fn set_file_sizes_clips_cached_pages_to_valid_data_length() {
        let _test_guard = TestGuard::acquire();
        let mut file: FileObject = unsafe { core::mem::zeroed() };
        let initial = CcFileSizes {
            allocation_size: 12288, file_size: 12288, valid_data_length: 12288,
        };
        unsafe { cc_initialize_cache_map(&mut file, &initial, 0, core::ptr::null(), core::ptr::null_mut()); }

        let first = unsafe { allocate_page_descriptor(0, core::ptr::null_mut(), 0, 4096) };
        let second = unsafe { allocate_page_descriptor(4096, core::ptr::null_mut(), 0, 4096) };
        let third = unsafe { allocate_page_descriptor(8192, core::ptr::null_mut(), 0, 4096) };
        assert!(!first.is_null() && !second.is_null() && !third.is_null());
        unsafe {
            (*first).next = second;
            (*second).next = third;
            let _guard = Guard::acquire();
            let map = find_map(&mut file);
            (*map).pages = first;
        }

        let shrunk = CcFileSizes {
            allocation_size: 12288, file_size: 12288, valid_data_length: 5000,
        };
        unsafe { cc_set_file_sizes(&mut file, &shrunk); }

        {
            let _guard = Guard::acquire();
            let map = unsafe { find_map(&mut file) };
            assert!(!map.is_null());
            let pages = unsafe { (*map).pages };
            assert!(!pages.is_null());
            assert_eq!(unsafe { (*pages).file_offset }, 0);
            assert_eq!(unsafe { (*pages).valid_end }, 4096);
            let tail = unsafe { (*pages).next };
            assert!(!tail.is_null());
            assert_eq!(unsafe { (*tail).file_offset }, 4096);
            assert_eq!(unsafe { (*tail).valid_end }, 904);
            assert!(unsafe { (*tail).next.is_null() });
        }

        assert_eq!(unsafe { cc_uninitialize_cache_map(&mut file, core::ptr::null(), core::ptr::null_mut()) }, 1);
    }

    #[test]
    fn valid_interval_never_bridges_a_hole() {
        let mut page = CachePage {
            next: core::ptr::null_mut(), file_offset: 0,
            data: core::ptr::null_mut(), valid_start: 100, valid_end: 200,
        };
        assert!(can_extend_valid_range(&page, 200, 240));
        assert!(can_extend_valid_range(&page, 60, 100));
        assert!(!can_extend_valid_range(&page, 240, 260));
        assert!(!can_extend_valid_range(&page, 40, 60));
        extend_valid_range(&mut page, 200, 240);
        assert_eq!(page.valid_start, 100);
        assert_eq!(page.valid_end, 240);
    }

    #[test]
    fn cache_file_sizes_reject_inconsistent_ranges() {
        let _test_guard = TestGuard::acquire();
        assert!(valid_sizes(CcFileSizes { allocation_size: 4096, file_size: 2048, valid_data_length: 1024 }));
        assert!(!valid_sizes(CcFileSizes { allocation_size: 1024, file_size: 2048, valid_data_length: 1024 }));
        assert!(!valid_sizes(CcFileSizes { allocation_size: 4096, file_size: 1024, valid_data_length: 2048 }));
    }
}

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

struct CacheMap {
    next: *mut CacheMap,
    file: *mut FileObject,
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

unsafe fn find_map(file: *mut FileObject) -> *mut CacheMap {
    if file.is_null() { return core::ptr::null_mut(); }
    let mut map = unsafe { MAPS_HEAD };
    while !map.is_null() {
        if unsafe { (*map).file == file } { return map; }
        map = unsafe { (*map).next };
    }
    core::ptr::null_mut()
}

unsafe fn read_sizes(ptr: *const CcFileSizes) -> Option<CcFileSizes> {
    if ptr.is_null() { return None; }
    let s = unsafe { ptr.read_unaligned() };
    valid_sizes(s).then_some(s)
}

unsafe fn allocate_map(file: *mut FileObject, sizes: CcFileSizes,
    callbacks: CacheManagerCallbacks, context: *mut c_void) -> *mut CacheMap {
    let p = crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(
        core::mem::size_of::<CacheMap>(), CACHE_MAP_TAG, true,
    ).cast::<CacheMap>();
    if p.is_null() { return p; }
    unsafe {
        p.write(CacheMap {
            next: core::ptr::null_mut(), file, sizes, callbacks,
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

    // Allocate before taking the metadata lock: pool allocation can fault and
    // must never occur while the Cache Manager spin lock is held.
    let candidate = unsafe { allocate_map(file, sizes, callbacks, lazy_write_context) };
    if candidate.is_null() { return; }
    let inserted = {
        let _guard = Guard::acquire();
        let existing = unsafe { find_map(file) };
        if !existing.is_null() {
            unsafe {
                (*existing).sizes = sizes;
                (*existing).callbacks = callbacks;
                (*existing).lazy_write_context = lazy_write_context;
            }
            false
        } else {
            unsafe {
                (*candidate).next = MAPS_HEAD;
                MAPS_HEAD = candidate;
                (*file).private_cache_map = candidate.cast::<c_void>();
                let section = (*file).section_object_pointer.cast::<u8>();
                if !section.is_null() {
                    core::ptr::write_unaligned(section.add(8).cast::<*mut c_void>(), candidate.cast());
                }
            }
            true
        }
    };
    if !inserted {
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
    let map = {
        let _guard = Guard::acquire();
        let mut link = core::ptr::addr_of_mut!(MAPS_HEAD);
        let mut current = unsafe { *link };
        while !current.is_null() && unsafe { (*current).file != file } {
            link = unsafe { core::ptr::addr_of_mut!((*current).next) };
            current = unsafe { *link };
        }
        if current.is_null() { return 1; }
        unsafe {
            *link = (*current).next;
            (*file).private_cache_map = core::ptr::null_mut();
            let section = (*file).section_object_pointer.cast::<u8>();
            if !section.is_null()
                && core::ptr::read_unaligned(section.add(8).cast::<*mut c_void>()) == current.cast() {
                core::ptr::write_unaligned(section.add(8).cast::<*mut c_void>(), core::ptr::null_mut());
            }
        }
        current
    };
    unsafe {
        free_page_list((*map).pages);
        let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(map.cast(), Some(CACHE_MAP_TAG));
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
            if unsafe { (*page).file_offset } >= sizes.file_size as u64 {
                unsafe {
                    *link = next;
                    (*page).next = release;
                    release = page;
                }
            } else {
                if page_end > sizes.file_size as u64 {
                    let valid_end = (sizes.file_size as u64 - unsafe { (*page).file_offset }) as usize;
                    unsafe { (*page).valid_end = (*page).valid_end.min(valid_end as u16); }
                }
                link = unsafe { core::ptr::addr_of_mut!((*page).next) };
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
                    core::ptr::copy_nonoverlapping(chunk.as_ptr(), (*page).data.add(within), amount);
                    (*page).valid_start = (*page).valid_start.min(within as u16);
                    (*page).valid_end = (*page).valid_end.max((within + amount) as u16);
                }
                done += amount;
                continue;
            }
        }

        let Ok(new_data) = crate::ntoskrnl::mm::kernel_space::allocate_pages(1) else { return; };
        unsafe { core::ptr::write_bytes(new_data, 0, CACHE_PAGE_SIZE); }
        core::ptr::copy_nonoverlapping(chunk.as_ptr(), unsafe { new_data.add(within) }, amount);
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
                        core::ptr::copy_nonoverlapping(chunk.as_ptr(), (*page).data.add(within), amount);
                        (*page).valid_start = (*page).valid_start.min(within as u16);
                        (*page).valid_end = (*page).valid_end.max((within + amount) as u16);
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

    #[test]
    fn cache_map_lifecycle_uses_dynamically_owned_metadata() {
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
    fn cache_metadata_has_no_128_stream_or_eight_page_ceiling() {
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
    fn cache_file_sizes_reject_inconsistent_ranges() {
        assert!(valid_sizes(CcFileSizes { allocation_size: 4096, file_size: 2048, valid_data_length: 1024 }));
        assert!(!valid_sizes(CcFileSizes { allocation_size: 1024, file_size: 2048, valid_data_length: 1024 }));
        assert!(!valid_sizes(CcFileSizes { allocation_size: 4096, file_size: 1024, valid_data_length: 2048 }));
    }
}

//! NT Cache Manager file-size lifecycle and clean page-backed read cache.
//!
//! Filesystem reads populate Memory Manager pages after successful IRP completion.
//! CcCopyRead serves complete cached ranges; misses fall back to ordinary IRPs.
//! Writes remain write-through, so this cache does not claim dirty-page writeback.
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::io::{FileObject, IoStatusBlock};

const STATUS_SUCCESS: i32 = 0;
const MAX_CACHE_MAPS: usize = 128;
const MAX_CACHE_PAGES: usize = 8;
const CACHE_PAGE_SIZE: usize = 4096;

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

#[derive(Clone, Copy)]
struct CachePage {
    used: bool,
    file_offset: u64,
    data: *mut u8,
    valid_start: u16,
    valid_end: u16,
}
unsafe impl Send for CachePage {}
impl CachePage {
    const EMPTY: Self = Self { used: false, file_offset: 0, data: core::ptr::null_mut(), valid_start: 0, valid_end: 0 };
}

#[derive(Clone, Copy)]
struct CacheMap {
    used: bool,
    file: *mut FileObject,
    sizes: CcFileSizes,
    callbacks: CacheManagerCallbacks,
    lazy_write_context: *mut c_void,
    read_ahead_granularity: u32,
    pages: [CachePage; MAX_CACHE_PAGES],
}
unsafe impl Send for CacheMap {}
impl CacheMap {
    const EMPTY: Self = Self {
        used: false, file: core::ptr::null_mut(), sizes: CcFileSizes {
            allocation_size: 0, file_size: 0, valid_data_length: 0,
        }, callbacks: CacheManagerCallbacks {
            acquire_for_lazy_write: None, release_from_lazy_write: None,
            acquire_for_read_ahead: None, release_from_read_ahead: None,
        }, lazy_write_context: core::ptr::null_mut(), read_ahead_granularity: 0x10000,
        pages: [CachePage::EMPTY; MAX_CACHE_PAGES],
    };
}
struct CacheState(UnsafeCell<[CacheMap; MAX_CACHE_MAPS]>);
unsafe impl Sync for CacheState {}
static STATE: CacheState = CacheState(UnsafeCell::new([CacheMap::EMPTY; MAX_CACHE_MAPS]));
static LOCK: AtomicBool = AtomicBool::new(false);
struct Guard;
impl Guard {
    fn acquire() -> Self {
        while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) { LOCK.store(false, Ordering::Release); }
}
fn valid_sizes(s: CcFileSizes) -> bool {
    s.allocation_size >= 0 && s.file_size >= 0 && s.valid_data_length >= 0
        && s.valid_data_length <= s.file_size && s.file_size <= s.allocation_size
}
unsafe fn find_map(file: *mut FileObject) -> Option<usize> {
    if file.is_null() { return None; }
    let maps = unsafe { &*STATE.0.get() };
    maps.iter().position(|m| m.used && m.file == file)
}
unsafe fn read_sizes(ptr: *const CcFileSizes) -> Option<CcFileSizes> {
    if ptr.is_null() { return None; }
    let s = unsafe { ptr.read_unaligned() };
    valid_sizes(s).then_some(s)
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
    let _guard = Guard::acquire();
    let maps = unsafe { &mut *STATE.0.get() };
    if let Some(slot) = maps.iter_mut().find(|m| m.used && m.file == file) {
        slot.sizes = sizes;
        slot.callbacks = callbacks;
        slot.lazy_write_context = lazy_write_context;
        return;
    }
    let Some(slot) = maps.iter_mut().find(|m| !m.used) else { return; };
    *slot = CacheMap { used: true, file, sizes, callbacks, lazy_write_context, read_ahead_granularity: 0x10000, pages: [CachePage::EMPTY; MAX_CACHE_PAGES] };
    unsafe {
        let map_ptr = (slot as *mut CacheMap).cast::<c_void>();
        (*file).private_cache_map = map_ptr;
        let section = (*file).section_object_pointer.cast::<u8>();
        if !section.is_null() {
            core::ptr::write_unaligned(section.add(8).cast::<*mut c_void>(), map_ptr);
        }
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
    let pages = {
        let _guard = Guard::acquire();
        let maps = unsafe { &mut *STATE.0.get() };
        let Some(slot) = maps.iter_mut().find(|m| m.used && m.file == file) else { return 1; };
        let pages = slot.pages;
        let map_ptr = (slot as *mut CacheMap).cast::<c_void>();
        *slot = CacheMap::EMPTY;
        unsafe {
            (*file).private_cache_map = core::ptr::null_mut();
            let section = (*file).section_object_pointer.cast::<u8>();
            if !section.is_null() && core::ptr::read_unaligned(section.add(8).cast::<*mut c_void>()) == map_ptr {
                core::ptr::write_unaligned(section.add(8).cast::<*mut c_void>(), core::ptr::null_mut());
            }
        }
        pages
    };
    for page in pages {
        if page.used && !page.data.is_null() {
            let _ = crate::ntoskrnl::mm::kernel_space::free_pages(page.data, 1);
        }
    }
    1
}

/// VOID CcSetFileSizes(PFILE_OBJECT, PCC_FILE_SIZES).
#[unsafe(export_name = "CcSetFileSizes")]
pub unsafe extern "win64" fn cc_set_file_sizes(file: *mut FileObject, sizes: *const CcFileSizes) {
    if file.is_null() { return; }
    let Some(sizes) = (unsafe { read_sizes(sizes) }) else { return; };
    let mut release = [core::ptr::null_mut::<u8>(); MAX_CACHE_PAGES];
    let mut count = 0usize;
    {
        let _guard = Guard::acquire();
        if let Some(index) = unsafe { find_map(file) } {
            let map = unsafe { &mut (&mut *STATE.0.get())[index] };
            map.sizes = sizes;
            for page in map.pages.iter_mut() {
                if !page.used { continue; }
                let end = page.file_offset.saturating_add(CACHE_PAGE_SIZE as u64);
                if page.file_offset >= sizes.file_size as u64 {
                    if !page.data.is_null() { release[count] = page.data; count += 1; }
                    *page = CachePage::EMPTY;
                } else if end > sizes.file_size as u64 {
                    let valid_end = (sizes.file_size as u64 - page.file_offset) as usize;
                    page.valid_end = page.valid_end.min(valid_end as u16);
                    if page.valid_start >= page.valid_end {
                        if !page.data.is_null() { release[count] = page.data; count += 1; }
                        *page = CachePage::EMPTY;
                    }
                }
            }
        }
    }
    for page in release.into_iter().take(count) {
        let _ = crate::ntoskrnl::mm::kernel_space::free_pages(page, 1);
    }
}

/// BOOLEAN CcCopyRead(...). FALSE deliberately requests the ordinary IRP path
/// until cache pages and filesystem callback-backed population are available.
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
    let Some(index) = (unsafe { find_map(file) }) else { return 0; };
    let map = unsafe { (&*STATE.0.get())[index] };
    let end = match (offset as u64).checked_add(length as u64) { Some(v) => v, None => return 0 };
    if end > map.sizes.valid_data_length as u64 || end > map.sizes.file_size as u64 { return 0; }
    let mut done = 0usize;
    while done < length {
        let absolute = offset as u64 + done as u64;
        let page_offset = absolute & !((CACHE_PAGE_SIZE as u64) - 1);
        let within = (absolute - page_offset) as usize;
        let amount = core::cmp::min(CACHE_PAGE_SIZE - within, length - done);
        if !map.pages.iter().any(|page| {
            page.used && page.file_offset == page_offset && !page.data.is_null()
                && within >= page.valid_start as usize
                && within + amount <= page.valid_end as usize
        }) { return 0; }
        done += amount;
    }
    done = 0;
    while done < length {
        let absolute = offset as u64 + done as u64;
        let page_offset = absolute & !((CACHE_PAGE_SIZE as u64) - 1);
        let within = (absolute - page_offset) as usize;
        let amount = core::cmp::min(CACHE_PAGE_SIZE - within, length - done);
        let Some(page) = map.pages.iter().find(|page| {
            page.used && page.file_offset == page_offset && !page.data.is_null()
                && within >= page.valid_start as usize
                && within + amount <= page.valid_end as usize
        }) else { return 0; };
        unsafe {
            core::ptr::copy_nonoverlapping(page.data.add(within), buffer.cast::<u8>().add(done), amount);
        }
        done += amount;
    }
    unsafe {
        core::ptr::write_unaligned(iosb.cast::<u8>().cast::<i32>(), STATUS_SUCCESS);
        core::ptr::write_unaligned(iosb.cast::<u8>().add(8).cast::<usize>(), length);
    }
    1
}

/// BOOLEAN CcCopyWrite(...). FALSE routes the caller to the filesystem IRP path.
#[unsafe(export_name = "CcCopyWrite")]
pub unsafe extern "win64" fn cc_copy_write(
    file: *mut FileObject, offset: *const i64, length: u32, wait: u8, buffer: *const c_void,
) -> u8 {
    if file.is_null() || offset.is_null() || (length != 0 && buffer.is_null()) || wait > 1 { return 0; }
    0
}

/// VOID CcFlushCache(PSECTION_OBJECT_POINTERS, PLARGE_INTEGER, ULONG, PIO_STATUS_BLOCK).
/// A clean metadata-only map flushes successfully; dirty writeback is not claimed
/// until a real lazy-writer and filesystem write path is connected.
#[unsafe(export_name = "CcFlushCache")]
pub unsafe extern "win64" fn cc_flush_cache(
    section_object_pointers: *mut c_void,
    _file_offset: *const i64,
    _length: u32,
    iosb: *mut IoStatusBlock,
) {
    if !iosb.is_null() {
        unsafe {
            core::ptr::write_unaligned(iosb.cast::<u8>().cast::<i32>(), STATUS_SUCCESS);
            core::ptr::write_unaligned(iosb.cast::<u8>().add(8).cast::<usize>(), 0);
        }
    }
    let _ = section_object_pointers;
}

/// VOID CcSetReadAheadGranularity(PFILE_OBJECT, ULONG).
#[unsafe(export_name = "CcSetReadAheadGranularity")]
pub unsafe extern "win64" fn cc_set_read_ahead_granularity(file: *mut FileObject, granularity: u32) {
    if file.is_null() || granularity == 0 || !granularity.is_power_of_two() { return; }
    let _guard = Guard::acquire();
    if let Some(index) = unsafe { find_map(file) } {
        unsafe { (&mut *STATE.0.get())[index].read_ahead_granularity = granularity.min(1 << 20); }
    }
}

/// BOOLEAN CcCanIWrite(PFILE_OBJECT, ULONG, BOOLEAN, BOOLEAN).
#[unsafe(export_name = "CcCanIWrite")]
pub unsafe extern "win64" fn cc_can_i_write(file: *mut FileObject, _bytes: u32, _wait: u8, _retrying: u8) -> u8 {
    if file.is_null() { return 0; }
    let _guard = Guard::acquire();
    unsafe { find_map(file).is_some() as u8 }
}

/// Insert clean file data into the page-backed cache after a successful filesystem IRP.
/// Pages are allocated from the Memory Manager and remain clean because writes are write-through.
pub(crate) fn cache_insert_clean(file: *mut FileObject, offset: u64, data: &[u8]) {
    if file.is_null() || data.is_empty()
        || unsafe { crate::ntoskrnl::ke::runtime::ke_get_current_irql() } > 1 {
        return;
    }
    let mut done = 0usize;
    while done < data.len() {
        let Some(absolute) = offset.checked_add(done as u64) else { return; };
        let page_offset = absolute & !((CACHE_PAGE_SIZE as u64) - 1);
        let within = (absolute - page_offset) as usize;
        let amount = core::cmp::min(CACHE_PAGE_SIZE - within, data.len() - done);
        let chunk = &data[done..done + amount];
        let existing = {
            let _guard = Guard::acquire();
            let Some(index) = (unsafe { find_map(file) }) else { return; };
            let map = unsafe { &mut (&mut *STATE.0.get())[index] };
            if let Some(page) = map.pages.iter_mut().find(|p| p.used && p.file_offset == page_offset && !p.data.is_null()) {
                unsafe { core::ptr::copy_nonoverlapping(chunk.as_ptr(), page.data.add(within), amount); }
                if page.valid_start == page.valid_end || within > page.valid_end as usize || within + amount < page.valid_start as usize {
                    page.valid_start = within as u16;
                    page.valid_end = (within + amount) as u16;
                } else {
                    page.valid_start = page.valid_start.min(within as u16);
                    page.valid_end = page.valid_end.max((within + amount) as u16);
                }
                true
            } else { false }
        };
        if !existing {
            let Ok(new_page) = crate::ntoskrnl::mm::kernel_space::allocate_pages(1) else { return; };
            let keep = {
                let _guard = Guard::acquire();
                let Some(index) = (unsafe { find_map(file) }) else {
                    let _ = crate::ntoskrnl::mm::kernel_space::free_pages(new_page, 1);
                    return;
                };
                let map = unsafe { &mut (&mut *STATE.0.get())[index] };
                if let Some(page) = map.pages.iter_mut().find(|p| p.used && p.file_offset == page_offset && !p.data.is_null()) {
                    unsafe { core::ptr::copy_nonoverlapping(chunk.as_ptr(), page.data.add(within), amount); }
                    if page.valid_start == page.valid_end || within > page.valid_end as usize || within + amount < page.valid_start as usize {
                        page.valid_start = within as u16;
                        page.valid_end = (within + amount) as u16;
                    } else {
                        page.valid_start = page.valid_start.min(within as u16);
                        page.valid_end = page.valid_end.max((within + amount) as u16);
                    }
                    false
                } else if let Some(page) = map.pages.iter_mut().find(|p| !p.used) {
                    unsafe { core::ptr::copy_nonoverlapping(chunk.as_ptr(), new_page.add(within), amount); }
                    *page = CachePage { used: true, file_offset: page_offset, data: new_page, valid_start: within as u16, valid_end: (within + amount) as u16 };
                    true
                } else { false }
            };
            if !keep { let _ = crate::ntoskrnl::mm::kernel_space::free_pages(new_page, 1); }
        }
        done += amount;
    }
}

/// Invalidate cached pages overlapping a write before issuing the filesystem write IRP.
pub(crate) fn cache_invalidate_range(file: *mut FileObject, offset: u64, length: usize) {
    if file.is_null() || length == 0 { return; }
    let Some(end) = offset.checked_add(length as u64) else { return; };
    let mut release = [core::ptr::null_mut::<u8>(); MAX_CACHE_PAGES];
    let mut count = 0usize;
    {
        let _guard = Guard::acquire();
        if let Some(index) = unsafe { find_map(file) } {
            let map = unsafe { &mut (&mut *STATE.0.get())[index] };
            for page in map.pages.iter_mut() {
                if page.used && page.file_offset < end
                    && page.file_offset.saturating_add(CACHE_PAGE_SIZE as u64) > offset {
                    if !page.data.is_null() { release[count] = page.data; count += 1; }
                    *page = CachePage::EMPTY;
                }
            }
        }
    }
    for page in release.into_iter().take(count) {
        let _ = crate::ntoskrnl::mm::kernel_space::free_pages(page, 1);
    }
}

/// ULONG CcGetFileSizePointer(PFILE_OBJECT) is not a WDK export; internal helper.
pub(crate) fn cache_map_count() -> usize {
    let _guard = Guard::acquire();
    unsafe { (&*STATE.0.get()).iter().filter(|m| m.used).count() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(align(4096))]
    struct TestCachePage([u8; CACHE_PAGE_SIZE]);
    static mut TEST_CACHE_PAGE: TestCachePage = TestCachePage([0; CACHE_PAGE_SIZE]);

    #[test]
    fn copy_read_hits_memory_manager_backed_page_cache() {
        let mut file: FileObject = unsafe { core::mem::zeroed() };
        let sizes = CcFileSizes { allocation_size: 4096, file_size: 4096, valid_data_length: 4096 };
        unsafe { cc_initialize_cache_map(&mut file, &sizes, 0, core::ptr::null(), core::ptr::null_mut()); }
        let expected = b"cached-data";
        unsafe {
            let page = core::ptr::addr_of_mut!(TEST_CACHE_PAGE.0).cast::<u8>();
            core::ptr::copy_nonoverlapping(expected.as_ptr(), page.add(32), expected.len());
            let _guard = Guard::acquire();
            let maps = &mut *STATE.0.get();
            let index = maps.iter().position(|m| m.used && m.file == &mut file).unwrap();
            maps[index].pages[0] = CachePage {
                used: true, file_offset: 0, data: page, valid_start: 32,
                valid_end: (32 + expected.len()) as u16,
            };
        }
        let offset = 32i64;
        let mut output = [0u8; 11];
        let mut iosb = IoStatusBlock { status: crate::nt_abi::io::IO_STATUS_BLOCK_STATUS { status: 0 }, information: 0 };
        assert_eq!(unsafe { cc_copy_read(&mut file, &offset, output.len() as u32, 1, output.as_mut_ptr().cast(), &mut iosb) }, 1);
        assert_eq!(&output, expected);
        assert_eq!(iosb.information, expected.len());
        {
            let _guard = Guard::acquire();
            let maps = unsafe { &mut *STATE.0.get() };
            let index = maps.iter().position(|m| m.used && m.file == &mut file).unwrap();
            maps[index].pages[0] = CachePage::EMPTY;
        }
        unsafe { cc_uninitialize_cache_map(&mut file, core::ptr::null(), core::ptr::null_mut()); }
    }

    #[test]
    fn cache_map_lifecycle_tracks_file_object_and_sizes() {
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
        assert_eq!(unsafe { find_map(&mut file) }.map(|i| unsafe { (&*STATE.0.get())[i].read_ahead_granularity }), Some(0x20000));
        assert_eq!(unsafe { find_map(&mut file) }.map(|i| unsafe { (&*STATE.0.get())[i].sizes.file_size }), Some(2048));
        assert_eq!(unsafe { cc_uninitialize_cache_map(&mut file, core::ptr::null(), core::ptr::null_mut()) }, 1);
        assert!(file.private_cache_map.is_null());
        assert_eq!(section.shared_cache_map, core::ptr::null_mut());
        assert_eq!(cache_map_count(), baseline);
    }

    #[test]
    fn cache_file_sizes_reject_inconsistent_ranges() {
        assert!(valid_sizes(CcFileSizes { allocation_size: 4096, file_size: 2048, valid_data_length: 1024 }));
        assert!(!valid_sizes(CcFileSizes { allocation_size: 1024, file_size: 2048, valid_data_length: 1024 }));
        assert!(!valid_sizes(CcFileSizes { allocation_size: 4096, file_size: 1024, valid_data_length: 2048 }));
    }
}

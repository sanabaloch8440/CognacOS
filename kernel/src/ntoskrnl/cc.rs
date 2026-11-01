//! Initial NT Cache Manager state and file-size lifecycle.
//!
//! This layer deliberately does not report cache hits until page-backed cache
//! storage, filesystem read/write callbacks, and dirty-page writeback are wired.
//! CcCopyRead/CcCopyWrite therefore return FALSE so the caller can issue the
//! ordinary filesystem IRP path; this is the documented fast-I/O fallback.
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::ntoskrnl::io::FileObject;
use crate::ntoskrnl::io::IoStatusBlock;

const STATUS_SUCCESS: i32 = 0;
const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002_u32 as i32;
const MAX_CACHE_MAPS: usize = 128;

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
struct CacheMap {
    used: bool,
    file: *mut FileObject,
    sizes: CcFileSizes,
    callbacks: CacheManagerCallbacks,
    lazy_write_context: *mut c_void,
    read_ahead_granularity: u32,
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
    *slot = CacheMap { used: true, file, sizes, callbacks, lazy_write_context, read_ahead_granularity: 0x10000 };
    unsafe { (*file).private_cache_map = (slot as *mut CacheMap).cast(); }
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
    let _guard = Guard::acquire();
    let maps = unsafe { &mut *STATE.0.get() };
    let Some(slot) = maps.iter_mut().find(|m| m.used && m.file == file) else { return 1; };
    *slot = CacheMap::EMPTY;
    unsafe { (*file).private_cache_map = core::ptr::null_mut(); }
    1
}

/// VOID CcSetFileSizes(PFILE_OBJECT, PCC_FILE_SIZES).
#[unsafe(export_name = "CcSetFileSizes")]
pub unsafe extern "win64" fn cc_set_file_sizes(file: *mut FileObject, sizes: *const CcFileSizes) {
    if file.is_null() { return; }
    let Some(sizes) = (unsafe { read_sizes(sizes) }) else { return; };
    let _guard = Guard::acquire();
    if let Some(index) = unsafe { find_map(file) } {
        unsafe { (&mut *STATE.0.get())[index].sizes = sizes; }
    }
}

/// BOOLEAN CcCopyRead(...). FALSE deliberately requests the ordinary IRP path
/// until cache pages and filesystem callback-backed population are available.
#[unsafe(export_name = "CcCopyRead")]
pub unsafe extern "win64" fn cc_copy_read(
    file: *mut FileObject, offset: *const i64, length: u32, wait: u8,
    buffer: *mut c_void, iosb: *mut IoStatusBlock,
) -> u8 {
    if file.is_null() || offset.is_null() || (length != 0 && buffer.is_null()) || wait > 1 { return 0; }
    let _ = iosb; // Fast-I/O miss leaves IOSB ownership to the IRP fallback path.
    0
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

/// BOOLEAN CcCanIWrite(PFILE_OBJECT, ULONG, BOOLEAN, BOOLEAN).
#[unsafe(export_name = "CcCanIWrite")]
pub unsafe extern "win64" fn cc_can_i_write(file: *mut FileObject, _bytes: u32, _wait: u8, _retrying: u8) -> u8 {
    if file.is_null() { return 0; }
    let _guard = Guard::acquire();
    unsafe { find_map(file).is_some() as u8 }
}

/// ULONG CcGetFileSizePointer(PFILE_OBJECT) is not a WDK export; internal helper.
pub(crate) fn cache_map_count() -> usize {
    let _guard = Guard::acquire();
    unsafe { (&*STATE.0.get()).iter().filter(|m| m.used).count() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_map_lifecycle_tracks_file_object_and_sizes() {
        let mut file: FileObject = unsafe { core::mem::zeroed() };
        let sizes = CcFileSizes { allocation_size: 8192, file_size: 4096, valid_data_length: 2048 };
        unsafe { cc_initialize_cache_map(&mut file, &sizes, 0, core::ptr::null(), core::ptr::null_mut()); }
        assert!(!file.private_cache_map.is_null());
        assert_eq!(cache_map_count(), 1);
        let changed = CcFileSizes { allocation_size: 8192, file_size: 2048, valid_data_length: 1024 };
        unsafe { cc_set_file_sizes(&mut file, &changed); }
        assert_eq!(unsafe { find_map(&mut file) }.map(|i| unsafe { (&*STATE.0.get())[i].sizes.file_size }), Some(2048));
        assert_eq!(unsafe { cc_uninitialize_cache_map(&mut file, core::ptr::null(), core::ptr::null_mut()) }, 1);
        assert!(file.private_cache_map.is_null());
        assert_eq!(cache_map_count(), 0);
    }

    #[test]
    fn cache_file_sizes_reject_inconsistent_ranges() {
        assert!(valid_sizes(CcFileSizes { allocation_size: 4096, file_size: 2048, valid_data_length: 1024 }));
        assert!(!valid_sizes(CcFileSizes { allocation_size: 1024, file_size: 2048, valid_data_length: 1024 }));
        assert!(!valid_sizes(CcFileSizes { allocation_size: 4096, file_size: 1024, valid_data_length: 2048 }));
    }
}

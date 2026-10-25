//! Source-first Rust port of the architecture-independent helpers in Wine's
//! `dlls/ntdll/misc.c`.
//!
//! The sorting/searching APIs preserve the C callback ABI. ETW and SQM entry
//! points below intentionally preserve Wine's current stub return behavior;
//! they are compatibility placeholders, not an event-tracing implementation.
//! The debug-only `debugstr_us` helper and C variadic `EtwTraceMessage`
//! wrapper are omitted because they depend on Wine's debug runtime / C varargs.
//! This module is not linked into the freestanding CognacOS PE32+ build yet.

#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;

pub type SIZE_T = usize;
pub type SSIZE_T = isize;
pub type ULONG = u32;
pub type DWORD = u32;
pub type UCHAR = u8;
pub type BOOLEAN = u8;
pub type BOOL = i32;
pub type NTSTATUS = i32;
pub type HANDLE = *mut c_void;
pub type REGHANDLE = u64;
pub type TRACEHANDLE = u64;
pub type GUID = [u8; 16];

pub const ERROR_SUCCESS: ULONG = 0;
pub const ERROR_INVALID_PARAMETER: ULONG = 87;
pub const ERROR_CALL_NOT_IMPLEMENTED: ULONG = 120;
pub const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
pub const INVALID_HANDLE_VALUE: HANDLE = usize::MAX as HANDLE;
pub const INVALID_PROCESSTRACE_HANDLE: TRACEHANDLE = u64::MAX;
pub const TRACE_LEVEL_VERBOSE: UCHAR = 5;

pub type CompareS = unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void) -> i32;
pub type Compare = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;

/// Overflow-safe byte offset helper for element arrays.
#[inline]
fn offset(index: usize, size: usize) -> Option<usize> {
    index.checked_mul(size)
}

#[inline]
unsafe fn swap_bytes(a: *mut u8, b: *mut u8, size: usize) {
    if a == b || size == 0 { return; }
    for i in 0..size {
        // SAFETY: caller guarantees both element ranges are valid for size bytes.
        unsafe { ptr::swap(a.add(i), b.add(i)); }
    }
}

/// Sort an array using a context-aware comparator. Invalid arguments are ignored,
/// matching Wine's defensive early returns.
pub unsafe fn qsort_s(
    base: *mut c_void,
    nmemb: SIZE_T,
    size: SIZE_T,
    compar: Option<CompareS>,
    context: *mut c_void,
) {
    let Some(cmp) = compar else { return };
    if (base.is_null() && nmemb != 0) || size == 0 || nmemb < 2 || nmemb.checked_mul(size).is_none() {
        return;
    }
    // Introspective quicksort is unnecessary here; heapsort guarantees O(n log n)
    // and bounded stack use even for adversarial input.
    let bytes = base.cast::<u8>();
    unsafe {
        let less = |i: usize, j: usize| cmp(context, bytes.add(i * size).cast(), bytes.add(j * size).cast()) < 0;
        let sift_down = |mut root: usize, end: usize| {
            loop {
                let child = root.saturating_mul(2).saturating_add(1);
                if child >= end { break; }
                let mut larger = child;
                if child + 1 < end && less(child, child + 1) { larger = child + 1; }
                if !less(root, larger) { break; }
                swap_bytes(bytes.add(root * size), bytes.add(larger * size), size);
                root = larger;
            }
        };
        for start in (0..(nmemb / 2)).rev() { sift_down(start, nmemb); }
        for end in (1..nmemb).rev() {
            swap_bytes(bytes, bytes.add(end * size), size);
            sift_down(0, end);
        }
    }
}

/// C `qsort` adapter for a two-argument comparator.
pub unsafe fn qsort(
    base: *mut c_void,
    nmemb: SIZE_T,
    size: SIZE_T,
    compar: Option<Compare>,
) {
    let Some(cmp) = compar else { return };
    unsafe extern "C" fn adapter(
        ctx: *mut c_void,
        a: *const c_void,
        b: *const c_void,
    ) -> i32 {
        // The context stores the function pointer passed by qsort.
        let cmp = unsafe { *(ctx.cast::<Option<Compare>>()) };
        match cmp { Some(f) => unsafe { f(a, b) }, None => 0 }
    }
    let mut cmp_ctx = compar;
    unsafe { qsort_s(base, nmemb, size, Some(adapter), (&mut cmp_ctx as *mut Option<Compare>).cast()); }
}

/// Context-aware binary search. Returns a pointer to a matching element or null.
pub unsafe fn bsearch_s(
    key: *const c_void,
    base: *const c_void,
    nmemb: SIZE_T,
    size: SIZE_T,
    compare: Option<CompareS>,
    ctx: *mut c_void,
) -> *mut c_void {
    let Some(cmp) = compare else { return ptr::null_mut() };
    if size == 0 || (base.is_null() && nmemb != 0) || (key.is_null() && nmemb != 0) {
        return ptr::null_mut();
    }
    let mut low = 0usize;
    let mut high = nmemb;
    let bytes = base.cast::<u8>();
    while low < high {
        let mid = low + (high - low) / 2;
        let Some(off) = offset(mid, size) else { return ptr::null_mut() };
        let element = unsafe { bytes.add(off).cast::<c_void>() };
        let result = unsafe { cmp(ctx, key, element) };
        if result == 0 { return element; }
        if result < 0 { high = mid; } else { low = mid + 1; }
    }
    ptr::null_mut()
}

/// Two-argument comparator variant of binary search.
pub unsafe fn bsearch(
    key: *const c_void,
    base: *const c_void,
    nmemb: SIZE_T,
    size: SIZE_T,
    compar: Option<Compare>,
) -> *mut c_void {
    let Some(cmp) = compar else { return ptr::null_mut() };
    unsafe extern "C" fn adapter(
        ctx: *mut c_void,
        a: *const c_void,
        b: *const c_void,
    ) -> i32 {
        let cmp = unsafe { *(ctx.cast::<Option<Compare>>()) };
        match cmp { Some(f) => unsafe { f(a, b) }, None => 0 }
    }
    let mut cmp_ctx = compar;
    unsafe { bsearch_s(key, base, nmemb, size, Some(adapter), (&mut cmp_ctx as *mut Option<Compare>).cast()) }
}

/// Linear search using the C comparator. The element count is not modified.
pub unsafe fn _lfind(
    key: *const c_void,
    base: *const c_void,
    nmemb: *const u32,
    size: SIZE_T,
    compar: Option<Compare>,
) -> *mut c_void {
    let Some(cmp) = compar else { return ptr::null_mut() };
    if nmemb.is_null() || (base.is_null() && unsafe { *nmemb } != 0) || size == 0 {
        return ptr::null_mut();
    }
    let count = unsafe { *nmemb as usize };
    let bytes = base.cast::<u8>();
    for i in 0..count {
        let Some(off) = offset(i, size) else { return ptr::null_mut() };
        let item = unsafe { bytes.add(off).cast::<c_void>() };
        if unsafe { cmp(key, item) } == 0 { return item; }
    }
    ptr::null_mut()
}

// SQM exports: these are deliberate Wine-compatible stubs, not telemetry storage.
pub unsafe fn WinSqmEndSession(_session: HANDLE) -> NTSTATUS { STATUS_NOT_IMPLEMENTED }
pub unsafe fn WinSqmIncrementDWORD(_a: DWORD, _b: DWORD, _c: DWORD) {}
pub unsafe fn WinSqmIsOptedIn() -> BOOL { 0 }
pub unsafe fn WinSqmStartSession(_guid: *mut GUID, _session_id: DWORD, _unknown: DWORD) -> HANDLE {
    INVALID_HANDLE_VALUE
}
pub unsafe fn WinSqmSetDWORD(_session: HANDLE, _id: DWORD, _value: DWORD) {}
pub unsafe fn WinSqmSetIfMaxDWORD(_a: DWORD, _b: DWORD, _c: DWORD) {}

// ETW exports preserve the behavior of the current Wine stubs.
pub unsafe fn EtwEventActivityIdControl(_code: ULONG, _guid: *mut GUID) -> ULONG { ERROR_SUCCESS }
pub unsafe fn EtwEventProviderEnabled(_handle: REGHANDLE, _level: UCHAR, _keyword: u64) -> BOOLEAN { 0 }
pub unsafe fn EtwEventRegister(
    _provider: *const GUID, _callback: *mut c_void, _context: *mut c_void, handle: *mut REGHANDLE,
) -> ULONG {
    if handle.is_null() { return ERROR_INVALID_PARAMETER; }
    unsafe { *handle = 0xdead_beef; }
    ERROR_SUCCESS
}
pub unsafe fn EtwEventUnregister(_handle: REGHANDLE) -> ULONG { ERROR_SUCCESS }
pub unsafe fn EtwEventSetInformation(_handle: REGHANDLE, _class: u32, _info: *mut c_void, _length: ULONG) -> ULONG { ERROR_SUCCESS }
pub unsafe fn EtwEventWriteString(_handle: REGHANDLE, _level: UCHAR, _keyword: u64, _string: *const u16) -> ULONG { ERROR_SUCCESS }
pub unsafe fn EtwEventWriteTransfer(
    _handle: REGHANDLE, _descriptor: *const c_void, _activity: *const GUID, _related: *const GUID,
    _count: ULONG, _data: *const c_void,
) -> ULONG { ERROR_SUCCESS }
pub unsafe fn EtwRegisterTraceGuidsW(
    _request: *mut c_void, _context: *mut c_void, _control: *const GUID, guid_count: ULONG,
    registrations: *mut TraceGuidRegistration, _image_path: *const u16, _resource_name: *const u16,
    registration_handle: *mut TRACEHANDLE,
) -> ULONG {
    if registration_handle.is_null() { return ERROR_INVALID_PARAMETER; }
    if !registrations.is_null() {
        for i in 0..guid_count as usize {
            unsafe { (*registrations.add(i)).reg_handle = 0xdead_beef as usize as *mut c_void; }
        }
    }
    unsafe { *registration_handle = 0xdead_beef; }
    ERROR_SUCCESS
}
pub unsafe fn EtwRegisterTraceGuidsA(
    _request: *mut c_void, _context: *mut c_void, _control: *const GUID, _guid_count: ULONG,
    _registrations: *mut TraceGuidRegistration, _image_path: *const i8, _resource_name: *const i8,
    _registration_handle: *mut TRACEHANDLE,
) -> ULONG { ERROR_SUCCESS }
pub unsafe fn EtwUnregisterTraceGuids(handle: TRACEHANDLE) -> ULONG {
    if handle == 0 { ERROR_INVALID_PARAMETER } else { ERROR_SUCCESS }
}
pub unsafe fn EtwEventEnabled(_handle: REGHANDLE, _descriptor: *const c_void) -> BOOLEAN { 0 }
pub unsafe fn EtwEventWrite(_handle: REGHANDLE, _descriptor: *const c_void, _count: ULONG, _data: *mut c_void) -> ULONG { ERROR_SUCCESS }
pub unsafe fn EtwEventWriteEx(
    _handle: REGHANDLE, _descriptor: *const c_void, _filter: u64, _flags: ULONG,
    _activity: *const GUID, _related: *const GUID, _count: ULONG, _data: *mut c_void,
) -> ULONG { ERROR_SUCCESS }
pub unsafe fn EtwGetTraceEnableFlags(_handle: TRACEHANDLE) -> ULONG { 0 }
pub unsafe fn EtwGetTraceEnableLevel(_handle: TRACEHANDLE) -> UCHAR { TRACE_LEVEL_VERBOSE }
pub unsafe fn EtwGetTraceLoggerHandle(_buffer: *mut c_void) -> TRACEHANDLE { INVALID_PROCESSTRACE_HANDLE }
pub unsafe fn EtwLogTraceEvent(_session: TRACEHANDLE, _event: *mut c_void) -> ULONG { ERROR_CALL_NOT_IMPLEMENTED }
pub unsafe fn EtwTraceMessageVa(_handle: TRACEHANDLE, _flags: ULONG, _guid: *mut GUID, _number: u16, _args: *mut c_void) -> ULONG { ERROR_SUCCESS }

#[repr(C)]
pub struct TraceGuidRegistration {
    pub guid: *const GUID,
    pub reg_handle: *mut c_void,
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn cmp_i32(a: *const c_void, b: *const c_void) -> i32 {
        let a = unsafe { *a.cast::<i32>() };
        let b = unsafe { *b.cast::<i32>() };
        a.cmp(&b) as i32
    }

    unsafe extern "C" fn cmp_i32_ctx(_ctx: *mut c_void, a: *const c_void, b: *const c_void) -> i32 {
        unsafe { cmp_i32(a, b) }
    }

    #[test]
    fn qsort_orders_values_and_handles_duplicates() {
        let mut values = [7i32, -1, 4, 4, 0, i32::MIN, i32::MAX];
        unsafe { qsort(values.as_mut_ptr().cast(), values.len(), core::mem::size_of::<i32>(), Some(cmp_i32)); }
        assert_eq!(values, [i32::MIN, -1, 0, 4, 4, 7, i32::MAX]);
    }

    #[test]
    fn qsort_s_empty_and_invalid_arguments_are_noops() {
        let mut values = [2i32, 1];
        unsafe { qsort_s(values.as_mut_ptr().cast(), 2, 4, Some(cmp_i32_ctx), ptr::null_mut()); }
        assert_eq!(values, [1, 2]);
        unsafe { qsort_s(values.as_mut_ptr().cast(), 2, 0, Some(cmp_i32_ctx), ptr::null_mut()); }
        assert_eq!(values, [1, 2]);
        unsafe { qsort_s(ptr::null_mut(), 3, 4, Some(cmp_i32_ctx), ptr::null_mut()); }
    }

    #[test]
    fn binary_search_finds_values_and_rejects_empty_or_invalid_input() {
        let values = [-8i32, -1, 3, 9, 20];
        let key = 9i32;
        let found = unsafe { bsearch((&key as *const i32).cast(), values.as_ptr().cast(), values.len(), 4, Some(cmp_i32)) };
        assert_eq!(found.cast::<i32>().read(), 9);
        let absent = 10i32;
        assert!(unsafe { bsearch((&absent as *const i32).cast(), values.as_ptr().cast(), values.len(), 4, Some(cmp_i32)) }.is_null());
        assert!(unsafe { bsearch((&key as *const i32).cast(), values.as_ptr().cast(), 0, 4, Some(cmp_i32)) }.is_null());
        assert!(unsafe { bsearch((&key as *const i32).cast(), values.as_ptr().cast(), 5, 0, Some(cmp_i32)) }.is_null());
    }

    #[test]
    fn linear_search_returns_first_matching_element() {
        let values = [5i32, 2, 5, 7];
        let key = 5i32;
        let count = values.len() as u32;
        let found = unsafe { _lfind((&key as *const i32).cast(), values.as_ptr().cast(), &count, 4, Some(cmp_i32)) };
        assert_eq!(found.cast::<i32>().read(), 5);
        let missing = 99i32;
        assert!(unsafe { _lfind((&missing as *const i32).cast(), values.as_ptr().cast(), &count, 4, Some(cmp_i32)) }.is_null());
    }

    #[test]
    fn etw_registration_stub_validates_output_pointer() {
        assert_eq!(unsafe { EtwEventRegister(ptr::null(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) }, ERROR_INVALID_PARAMETER);
        let mut handle = 0;
        assert_eq!(unsafe { EtwEventRegister(ptr::null(), ptr::null_mut(), ptr::null_mut(), &mut handle) }, ERROR_SUCCESS);
        assert_eq!(handle, 0xdead_beef);
    }
}

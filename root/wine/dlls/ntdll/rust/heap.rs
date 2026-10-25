//! Source-first Rust port of the non-allocation portions of Wine's ntdll/heap.c.
//!
//! The allocator core already lives in rtl_heap.rs as CognacOS's bounded bootstrap
//! heap. This file ports process-heap enumeration and heap-compatibility queries
//! without pretending that the current fixed arena implements Wine's full
//! region/subheap/LFH allocator. Integrate only after reconciling duplicate exports
//! and sharing a single heap metadata definition.
//!
//! Source reference: root/wine/dlls/ntdll/heap.c (Wine LGPL-2.1-or-later).
//! This is not a complete Windows heap implementation.

#![allow(non_camel_case_types, non_snake_case)]

use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};

pub type HANDLE = *mut core::ffi::c_void;
pub type NTSTATUS = i32;
pub type PVOID = *mut core::ffi::c_void;
pub type SIZE_T = usize;
pub type ULONG = u32;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const STATUS_INVALID_INFO_CLASS: NTSTATUS = 0xC000_0003u32 as i32;
const STATUS_UNSUCCESSFUL: NTSTATUS = 0xC000_0001u32 as i32;
const HEAP_STD: ULONG = 0;
const HEAP_LFH: ULONG = 2;
const HEAP_COMPATIBILITY_INFORMATION: ULONG = 0;

extern "system" {
    fn RtlGetProcessHeap() -> HANDLE;
}

/// The current CognacOS bootstrap has one process heap. Once additional heaps are
/// implemented, this registry must be replaced by the real process heap list.
static HEAP_COMPATIBILITY: AtomicU32 = AtomicU32::new(HEAP_STD);

#[inline]
unsafe fn is_bootstrap_heap(handle: HANDLE) -> bool {
    // SAFETY: RtlGetProcessHeap is the existing bootstrap export. This comparison
    // does not dereference the caller-provided handle.
    !handle.is_null() && handle == unsafe { RtlGetProcessHeap() }
}

/// Wine heap.c: RtlGetProcessHeaps. Returns the total heap count. As with the
/// source routine, the destination is written only when it can hold all handles.
#[no_mangle]
pub unsafe extern "system" fn RtlGetProcessHeaps(count: ULONG, heaps: *mut HANDLE) -> ULONG {
    // The current bootstrap heap registry contains only the process heap.
    if count >= 1 && !heaps.is_null() {
        // SAFETY: Caller promises an output array of count HANDLE entries.
        unsafe { ptr::write(heaps, RtlGetProcessHeap()) };
    }
    1
}

/// Wine heap.c: RtlQueryHeapInformation, supporting only
/// HeapCompatibilityInformation (class zero), matching the source's information
/// size reporting and STATUS_BUFFER_TOO_SMALL behavior.
#[no_mangle]
pub unsafe extern "system" fn RtlQueryHeapInformation(
    handle: HANDLE,
    info_class: ULONG,
    info: PVOID,
    size_in: SIZE_T,
    size_out: *mut SIZE_T,
) -> NTSTATUS {
    if info_class != HEAP_COMPATIBILITY_INFORMATION {
        return STATUS_INVALID_INFO_CLASS;
    }
    // Wine reports invalid heap handles here as STATUS_ACCESS_VIOLATION; the
    // bootstrap implementation uses INVALID_HANDLE rather than dereferencing an
    // untrusted pointer. This is a deliberate safety adaptation.
    if !unsafe { is_bootstrap_heap(handle) } {
        return STATUS_INVALID_HANDLE;
    }
    if !size_out.is_null() {
        // SAFETY: Optional size output is a caller-provided writable SIZE_T.
        unsafe { ptr::write(size_out, core::mem::size_of::<ULONG>()) };
    }
    if size_in < core::mem::size_of::<ULONG>() {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if info.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    // SAFETY: The caller provides at least four writable bytes when size_in >= 4.
    unsafe {
        ptr::write(
            info.cast::<ULONG>(),
            HEAP_COMPATIBILITY.load(Ordering::Acquire),
        );
    }
    STATUS_SUCCESS
}

/// Wine heap.c: RtlSetHeapInformation, supporting the standard and low-fragmentation
/// heap compatibility values. The transition is one-way from HEAP_STD, as in Wine.
#[no_mangle]
pub unsafe extern "system" fn RtlSetHeapInformation(
    handle: HANDLE,
    info_class: ULONG,
    info: PVOID,
    size: SIZE_T,
) -> NTSTATUS {
    if info_class != HEAP_COMPATIBILITY_INFORMATION {
        // Wine's current implementation returns success for unknown set classes.
        return STATUS_SUCCESS;
    }
    if size < core::mem::size_of::<ULONG>() {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if !unsafe { is_bootstrap_heap(handle) } {
        return STATUS_INVALID_HANDLE;
    }
    if info.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    // Current bootstrap allocator is serialized by its internal spin lock; it does
    // not implement HEAP_NO_SERIALIZE flags, so that flag cannot be inspected here.
    // SAFETY: Caller supplies at least one readable ULONG.
    let requested = unsafe { ptr::read(info.cast::<ULONG>()) };
    if requested != HEAP_STD && requested != HEAP_LFH {
        return STATUS_UNSUCCESSFUL;
    }
    match HEAP_COMPATIBILITY.compare_exchange(
        HEAP_STD,
        requested,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => STATUS_SUCCESS,
        Err(_) => STATUS_UNSUCCESSFUL,
    }
}

/// Host-side checks for the fixed bootstrap contract and query/set status paths.
/// These tests are not a substitute for ABI or QEMU validation.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_information_class_is_zero() {
        assert_eq!(HEAP_COMPATIBILITY_INFORMATION, 0);
        assert_eq!(HEAP_STD, 0);
        assert_eq!(HEAP_LFH, 2);
    }

    #[test]
    fn status_values_match_ntstatus_contract() {
        assert_eq!(STATUS_SUCCESS, 0);
        assert_eq!(STATUS_BUFFER_TOO_SMALL as u32, 0xC000_0023);
        assert_eq!(STATUS_INVALID_INFO_CLASS as u32, 0xC000_0003);
    }
}

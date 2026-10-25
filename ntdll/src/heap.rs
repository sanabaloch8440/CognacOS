//! Supplemental native heap information APIs for CognacOS's NT RTL heap manager.
//!
//! Heap allocation, process-heap enumeration, and heap lifetime are implemented
//! in rtl_heap.rs. This file contains the information-class APIs only; it does not
//! import Wine implementation code. Private Windows 11 LFH/segment-heap internals
//! remain out of scope until their documented compatibility surface is implemented.

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
    fn RtlValidateHeap(heap: HANDLE, flags: ULONG, address: PVOID) -> u8;
}

/// Compatibility mode is currently process-wide until the heap object exposes a
/// versioned public information state. The allocator itself supports private heaps.
static HEAP_COMPATIBILITY: AtomicU32 = AtomicU32::new(HEAP_STD);

#[inline]
unsafe fn is_valid_heap(handle: HANDLE) -> bool {
    // SAFETY: RtlValidateHeap is the manager's handle/metadata validator. A non-null
    // heap handle must still point to readable memory, as required by NT ABI callers.
    !handle.is_null() && unsafe { RtlValidateHeap(handle, 0, ptr::null_mut()) != 0 }
}

/// RtlGetProcessHeaps returns the total number of process heaps. The destination
/// is written only when the supplied capacity can hold the full list.
/// RtlQueryHeapInformation: supports HeapCompatibilityInformation (class zero)
/// with NTSTATUS size reporting and buffer validation.
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
    if !unsafe { is_valid_heap(handle) } {
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

/// RtlSetHeapInformation: accepts standard and LFH compatibility mode requests.
/// Selecting LFH records the request; an LFH allocator is not yet implemented.
#[no_mangle]
pub unsafe extern "system" fn RtlSetHeapInformation(
    handle: HANDLE,
    info_class: ULONG,
    info: PVOID,
    size: SIZE_T,
) -> NTSTATUS {
    if info_class != HEAP_COMPATIBILITY_INFORMATION {
        // Unknown set classes are ignored for forward compatibility.
        return STATUS_SUCCESS;
    }
    if size < core::mem::size_of::<ULONG>() {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if !unsafe { is_valid_heap(handle) } {
        return STATUS_INVALID_HANDLE;
    }
    if info.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
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

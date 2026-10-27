//! Native heap diagnostics and bulk operations for CognacOS's RTL heap manager.
//! Public routines follow documented NT heap contracts where available. Private
//! diagnostics whose Windows 11 contract is undocumented remain conservative.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::{ffi::c_void, ptr, sync::atomic::{AtomicU32, Ordering}};
type HANDLE = *mut c_void;
type PVOID = *mut c_void;
type ULONG = u32;
type SIZE_T = usize;
type NTSTATUS = i32;
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008u32 as i32;
const STATUS_UNSUCCESSFUL: NTSTATUS = 0xC000_0001u32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const STATUS_INVALID_INFO_CLASS: NTSTATUS = 0xC000_0003u32 as i32;
const HEAP_STD: ULONG = 0;
const HEAP_LFH: ULONG = 2;
const HEAP_COMPATIBILITY_INFORMATION: ULONG = 0;
static NEXT_TAG: AtomicU32 = AtomicU32::new(1);

#[repr(C)]
pub struct RTL_HEAP_TAG_INFO {
    pub NumberOfAllocations: u32,
    pub NumberOfFrees: u32,
    pub BytesAllocated: usize,
    pub TagIndex: u16,
    pub CreatorBackTraceIndex: u16,
    pub TagName: [u16; 24],
}

pub type HEAP_ENUMERATION_ROUTINE = Option<unsafe extern "system" fn(HANDLE, PVOID) -> u8>;

#[no_mangle]
pub unsafe extern "system" fn RtlEnumProcessHeaps(callback: HEAP_ENUMERATION_ROUTINE, parameter: PVOID) -> NTSTATUS {
    let Some(callback) = callback else { return STATUS_INVALID_PARAMETER; };
    let count = unsafe { crate::rtl_heap::RtlGetProcessHeaps(0, ptr::null_mut()) } as usize;
    if count == 0 { return STATUS_SUCCESS; }
    if count > 4096 { return STATUS_UNSUCCESSFUL; }
    let mut heaps = [ptr::null_mut::<c_void>(); 4096];
    let actual = unsafe { crate::rtl_heap::RtlGetProcessHeaps(heaps.len() as u32, heaps.as_mut_ptr()) } as usize;
    for heap in heaps.iter().take(actual.min(count)) {
        if unsafe { callback(*heap, parameter) } == 0 { break; }
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlProtectHeap(heap: HANDLE, protect: u8) -> u8 {
    unsafe { crate::rtl_heap::RtlSetHeapProtected(heap, protect) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlushHeaps() -> u8 {
    let count = unsafe { crate::rtl_heap::RtlGetProcessHeaps(0, ptr::null_mut()) } as usize;
    if count > 4096 { return 0; }
    let mut heaps = [ptr::null_mut::<c_void>(); 4096];
    let actual = unsafe { crate::rtl_heap::RtlGetProcessHeaps(heaps.len() as u32, heaps.as_mut_ptr()) } as usize;
    for heap in heaps.iter().take(actual.min(count)) {
        if unsafe { crate::rtl_heap::RtlValidateHeap(*heap, 0, ptr::null_mut()) } == 0 { return 0; }
        let _ = unsafe { crate::rtl_heap::RtlCompactHeap(*heap, 0) };
    }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateTagHeap(heap: HANDLE, flags: ULONG, _prefix: *const u16, _names: *const u16) -> ULONG {
    if flags != 0 || unsafe { crate::rtl_heap::RtlValidateHeap(heap, 0, ptr::null_mut()) } == 0 { return 0; }
    let tag = NEXT_TAG.fetch_add(1, Ordering::AcqRel);
    if tag == 0 || tag > u16::MAX as u32 { 0 } else { tag }
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryTagHeap(heap: HANDLE, flags: ULONG, tag_index: u16, info: *mut RTL_HEAP_TAG_INFO) -> *mut RTL_HEAP_TAG_INFO {
    if flags != 0 || info.is_null() || unsafe { crate::rtl_heap::RtlValidateHeap(heap, 0, ptr::null_mut()) } == 0 { return ptr::null_mut(); }
    // Tag accounting is not implemented yet; only the default, untracked tag is queryable.
    if tag_index != 0 { return ptr::null_mut(); }
    unsafe { ptr::write(info, RTL_HEAP_TAG_INFO { NumberOfAllocations: 0, NumberOfFrees: 0, BytesAllocated: 0, TagIndex: 0, CreatorBackTraceIndex: 0, TagName: [0; 24] }); }
    info
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryProcessHeapInformation() -> ULONG {
    unsafe { crate::rtl_heap::RtlValidateProcessHeaps() }
}

#[no_mangle]
pub unsafe extern "system" fn RtlMultipleAllocateHeap(heap: HANDLE, flags: ULONG, size: usize, count: ULONG, array: *mut PVOID) -> ULONG {
    if count == 0 { return 0; }
    if array.is_null() || (count as usize) > isize::MAX as usize / core::mem::size_of::<PVOID>() { return 0; }
    let mut done = 0;
    while done < count {
        let block = unsafe { crate::rtl_heap::RtlAllocateHeap(heap, flags, size) };
        unsafe { ptr::write(array.add(done as usize), block); }
        if block.is_null() { break; }
        done += 1;
    }
    done
}

#[no_mangle]
pub unsafe extern "system" fn RtlMultipleFreeHeap(heap: HANDLE, flags: ULONG, count: ULONG, array: *mut PVOID) -> ULONG {
    if count == 0 { return 0; }
    if array.is_null() || (count as usize) > isize::MAX as usize / core::mem::size_of::<PVOID>() { return 0; }
    let mut done = 0;
    for index in 0..count {
        let block = unsafe { ptr::read(array.add(index as usize)) };
        if unsafe { crate::rtl_heap::RtlFreeHeap(heap, flags, block) } != 0 { done += 1; }
    }
    done
}

#[no_mangle]
pub unsafe extern "system" fn RtlZeroHeap(heap: HANDLE, _flags: ULONG) -> NTSTATUS {
    let mut entry = crate::rtl_heap::RTL_HEAP_WALK_ENTRY {
        lpData: ptr::null_mut(), cbData: 0, cbOverhead: 0, iRegionIndex: 0, wFlags: 0, reserved: [0; 3],
    };
    let mut status = unsafe { crate::rtl_heap::RtlWalkHeap(heap, (&mut entry as *mut crate::rtl_heap::RTL_HEAP_WALK_ENTRY).cast()) };
    while status == STATUS_SUCCESS {
        if entry.wFlags & 1 != 0 && !entry.lpData.is_null() && entry.cbData != 0 {
            unsafe { ptr::write_bytes(entry.lpData.cast::<u8>(), 0, entry.cbData); }
        }
        if entry.lpData.is_null() { break; }
        status = unsafe { crate::rtl_heap::RtlWalkHeap(heap, (&mut entry as *mut crate::rtl_heap::RTL_HEAP_WALK_ENTRY).cast()) };
    }
    if status == 0 || status as u32 == 0x8000_001A { STATUS_SUCCESS } else { status }
}

// Undocumented/private Windows diagnostics: deliberately report conservative
// results instead of pretending to provide undocumented leak-tracking behavior.
#[no_mangle]
pub unsafe extern "system" fn RtlDetectHeapLeaks() -> u8 { 0 }
#[no_mangle]
pub unsafe extern "system" fn RtlDisownModuleHeapAllocation(_address: PVOID) -> NTSTATUS { STATUS_UNSUCCESSFUL }
#[no_mangle]
pub unsafe extern "system" fn RtlHeapTrkInitialize(_flags: ULONG) -> NTSTATUS { STATUS_UNSUCCESSFUL }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_values_are_ntstatus_values() {
        assert_eq!(STATUS_SUCCESS, 0);
        assert_eq!(STATUS_INVALID_PARAMETER as u32, 0xC000_000D);
        assert_eq!(STATUS_INVALID_HANDLE as u32, 0xC000_0008);
    }
    #[test]
    fn tag_info_x64_layout() {
        assert_eq!(core::mem::offset_of!(RTL_HEAP_TAG_INFO, BytesAllocated), 8);
        assert_eq!(core::mem::offset_of!(RTL_HEAP_TAG_INFO, TagName), 20);
        assert_eq!(core::mem::size_of::<RTL_HEAP_TAG_INFO>(), 72);
    }
}

// Merged from the former legacy heap module: information-class APIs.
/// Compatibility mode is currently process-wide until the heap object exposes a
/// versioned public information state. The allocator itself supports private heaps.
static HEAP_COMPATIBILITY: AtomicU32 = AtomicU32::new(HEAP_STD);

#[inline]
unsafe fn is_valid_heap(handle: HANDLE) -> bool {
    // SAFETY: RtlValidateHeap is the manager's handle/metadata validator. A non-null
    // heap handle must still point to readable memory, as required by NT ABI callers.
    !handle.is_null() && unsafe { crate::rtl_heap::RtlValidateHeap(handle, 0, ptr::null_mut()) != 0 }
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


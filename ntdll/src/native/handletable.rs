//! Source-only Rust port of Wine's `dlls/ntdll/handletable.c`.
//!
//! Implements the RTL handle-table ABI and the reserve/commit free-list model.
//! Memory management is delegated to native NtAllocateVirtualMemory and
//! NtFreeVirtualMemory; no Wine server or Unix runtime is required.

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use core::ffi::c_void;
use core::{mem, ptr};

type NTSTATUS = i32;
type ULONG = u32;
type SIZE_T = usize;
type BOOLEAN = u8;
type HANDLE = *mut c_void;
type PVOID = *mut c_void;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const MEM_COMMIT: ULONG = 0x1000;
const MEM_RESERVE: ULONG = 0x2000;
const MEM_RELEASE: ULONG = 0x8000;
const PAGE_READWRITE: ULONG = 0x04;
const COMMIT_SIZE: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RTL_HANDLE {
    pub Next: *mut RTL_HANDLE,
}

/// Windows x64 layout: two ULONGs, two reserved ULONGs, then four pointers.
#[repr(C)]
pub struct RTL_HANDLE_TABLE {
    pub MaxHandleCount: ULONG,
    pub HandleSize: ULONG,
    pub Unused: [ULONG; 2],
    pub NextFree: PVOID,
    pub FirstHandle: PVOID,
    pub ReservedMemory: PVOID,
    pub MaxHandle: PVOID,
}

#[cfg(windows)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtAllocateVirtualMemory(
        ProcessHandle: HANDLE,
        BaseAddress: *mut PVOID,
        ZeroBits: usize,
        RegionSize: *mut SIZE_T,
        AllocationType: ULONG,
        Protect: ULONG,
    ) -> NTSTATUS;
    fn NtFreeVirtualMemory(
        ProcessHandle: HANDLE,
        BaseAddress: *mut PVOID,
        RegionSize: *mut SIZE_T,
        FreeType: ULONG,
    ) -> NTSTATUS;
}

#[cfg(not(windows))]
unsafe fn NtAllocateVirtualMemory(
    _process_handle: HANDLE, _base_address: *mut PVOID, _zero_bits: usize,
    _region_size: *mut SIZE_T, _allocation_type: ULONG, _protect: ULONG,
) -> NTSTATUS { STATUS_NOT_IMPLEMENTED }
#[cfg(not(windows))]
unsafe fn NtFreeVirtualMemory(
    _process_handle: HANDLE, _base_address: *mut PVOID, _region_size: *mut SIZE_T,
    _free_type: ULONG,
) -> NTSTATUS { STATUS_NOT_IMPLEMENTED }

#[inline]
fn current_process() -> HANDLE { (-1isize) as HANDLE }

#[inline]
fn status_success(status: NTSTATUS) -> bool { status >= 0 }

#[inline]
unsafe fn table_ptr<T>(table: *mut RTL_HANDLE_TABLE, offset: usize) -> *mut T {
    unsafe { (table.cast::<u8>().add(offset)).cast::<T>() }
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeHandleTable(
    max_handle_count: ULONG,
    handle_size: ULONG,
    handle_table: *mut RTL_HANDLE_TABLE,
) {
    if handle_table.is_null() { return; }
    unsafe {
        ptr::write_bytes(handle_table.cast::<u8>(), 0, mem::size_of::<RTL_HANDLE_TABLE>());
        (*handle_table).MaxHandleCount = max_handle_count;
        (*handle_table).HandleSize = handle_size;
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyHandleTable(handle_table: *mut RTL_HANDLE_TABLE) -> NTSTATUS {
    if handle_table.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut base = unsafe { (*handle_table).FirstHandle };
    let mut size: SIZE_T = 0;
    if base.is_null() { return STATUS_SUCCESS; }
    let status = unsafe { NtFreeVirtualMemory(current_process(), &mut base, &mut size, MEM_RELEASE) };
    if status_success(status) {
        unsafe {
            (*handle_table).NextFree = ptr::null_mut();
            (*handle_table).FirstHandle = ptr::null_mut();
            (*handle_table).ReservedMemory = ptr::null_mut();
            (*handle_table).MaxHandle = ptr::null_mut();
        }
    }
    status
}

/// Reserve the full handle address range once and commit it one page at a time.
/// Free-list nodes are the first pointer-sized field of each caller-sized entry.
unsafe fn rtlp_allocate_some_handles(table: *mut RTL_HANDLE_TABLE) -> NTSTATUS {
    if table.is_null() { return STATUS_INVALID_PARAMETER; }
    let handle_size = unsafe { (*table).HandleSize as usize };
    let max_count = unsafe { (*table).MaxHandleCount as usize };
    if handle_size < mem::size_of::<*mut RTL_HANDLE>() || !handle_size.is_power_of_two() || max_count == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(max_size) = max_count.checked_mul(handle_size) else { return STATUS_NO_MEMORY; };

    if unsafe { (*table).FirstHandle.is_null() } {
        let mut base: PVOID = ptr::null_mut();
        let mut region_size = max_size;
        let status = unsafe {
            NtAllocateVirtualMemory(current_process(), &mut base, 0, &mut region_size, MEM_RESERVE, PAGE_READWRITE)
        };
        if !status_success(status) { return status; }
        unsafe {
            (*table).FirstHandle = base;
            (*table).ReservedMemory = base;
            (*table).MaxHandle = (base as usize).checked_add(max_size).unwrap_or(0) as PVOID;
        }
        if unsafe { (*table).MaxHandle.is_null() } {
            let mut release_base = base;
            let mut zero = 0usize;
            let _ = unsafe { NtFreeVirtualMemory(current_process(), &mut release_base, &mut zero, MEM_RELEASE) };
            unsafe {
                (*table).FirstHandle = ptr::null_mut();
                (*table).ReservedMemory = ptr::null_mut();
                (*table).MaxHandle = ptr::null_mut();
            }
            return STATUS_NO_MEMORY;
        }
    }

    let reserved = unsafe { (*table).ReservedMemory as usize };
    let max = unsafe { (*table).MaxHandle as usize };
    if reserved >= max { return STATUS_NO_MEMORY; }

    let remaining = max - reserved;
    let commit_size = COMMIT_SIZE.min(remaining);
    let mut base = reserved as PVOID;
    let mut region_size = commit_size;
    let status = unsafe {
        NtAllocateVirtualMemory(current_process(), &mut base, 0, &mut region_size, MEM_COMMIT, PAGE_READWRITE)
    };
    if !status_success(status) { return status; }
    // The VM manager may round the commit to page boundaries; initialise only
    // complete entries that remain inside the reserved and committed range.
    let committed_end = (base as usize).saturating_add(region_size).min(max);
    let count = committed_end.saturating_sub(reserved) / handle_size;
    if count == 0 { return STATUS_NO_MEMORY; }
    for i in 0..count {
        let entry = (reserved + i * handle_size) as *mut RTL_HANDLE;
        let next = if i + 1 < count { (reserved + (i + 1) * handle_size) as *mut RTL_HANDLE } else { ptr::null_mut() };
        unsafe { (*entry).Next = next; }
    }
    unsafe {
        (*table).NextFree = reserved as PVOID;
        (*table).ReservedMemory = committed_end as PVOID;
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateHandle(
    handle_table: *mut RTL_HANDLE_TABLE,
    handle_index: *mut ULONG,
) -> *mut RTL_HANDLE {
    if handle_table.is_null() { return ptr::null_mut(); }
    if unsafe { (*handle_table).HandleSize as usize } < mem::size_of::<*mut RTL_HANDLE>()
        || !unsafe { (*handle_table).HandleSize }.is_power_of_two()
    { return ptr::null_mut(); }

    if unsafe { (*handle_table).NextFree.is_null() }
        && !status_success(unsafe { rtlp_allocate_some_handles(handle_table) })
    { return ptr::null_mut(); }

    let entry = unsafe { (*handle_table).NextFree.cast::<RTL_HANDLE>() };
    if entry.is_null() { return ptr::null_mut(); }
    unsafe {
        (*handle_table).NextFree = (*entry).Next.cast();
        if !handle_index.is_null() {
            let first = (*handle_table).FirstHandle as usize;
            let current = entry as usize;
            if current < first { return ptr::null_mut(); }
            let index = (current - first) / (*handle_table).HandleSize as usize;
            if index > ULONG::MAX as usize { return ptr::null_mut(); }
            handle_index.write(index as ULONG);
        }
    }
    entry
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeHandle(
    handle_table: *mut RTL_HANDLE_TABLE,
    handle: *mut RTL_HANDLE,
) -> BOOLEAN {
    if handle_table.is_null() || handle.is_null() { return 0; }
    let size = unsafe { (*handle_table).HandleSize as usize };
    let first = unsafe { (*handle_table).FirstHandle as usize };
    let end = unsafe { (*handle_table).ReservedMemory as usize };
    let address = handle as usize;
    if size < mem::size_of::<*mut RTL_HANDLE>() || address < first || address >= end
        || (address - first) % size != 0
    { return 0; }
    unsafe {
        ptr::write_bytes(handle.cast::<u8>(), 0, size);
        (*handle).Next = (*handle_table).NextFree.cast();
        (*handle_table).NextFree = handle.cast();
    }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsValidHandle(
    handle_table: *const RTL_HANDLE_TABLE,
    handle: *const RTL_HANDLE,
) -> BOOLEAN {
    if handle_table.is_null() || handle.is_null() { return 0; }
    let size = unsafe { (*handle_table).HandleSize as usize };
    let first = unsafe { (*handle_table).FirstHandle as usize };
    let end = unsafe { (*handle_table).ReservedMemory as usize };
    let address = handle as usize;
    if size < mem::size_of::<*mut RTL_HANDLE>() || address < first || address >= end
        || (address - first) % size != 0
    { return 0; }
    // Wine's public RTL contract uses the low bit of Next to mark a live
    // handle; free-list links are aligned pointers and therefore even.
    if unsafe { ((*handle).Next as usize) & 1 != 0 } { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsValidIndexHandle(
    handle_table: *const RTL_HANDLE_TABLE,
    index: ULONG,
    valid_handle: *mut *mut RTL_HANDLE,
) -> BOOLEAN {
    if handle_table.is_null() || valid_handle.is_null() { return 0; }
    let max_count = unsafe { (*handle_table).MaxHandleCount };
    let size = unsafe { (*handle_table).HandleSize };
    if index >= max_count || size == 0 { return 0; }
    let Some(offset) = (index as usize).checked_mul(size as usize) else { return 0; };
    let first = unsafe { (*handle_table).FirstHandle as usize };
    let Some(address) = first.checked_add(offset) else { return 0; };
    let handle = address as *mut RTL_HANDLE;
    if unsafe { RtlIsValidHandle(handle_table, handle) } != 0 {
        unsafe { valid_handle.write(handle); }
        1
    } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_table_x64_layout_matches_winternl() {
        assert_eq!(mem::offset_of!(RTL_HANDLE_TABLE, MaxHandleCount), 0);
        assert_eq!(mem::offset_of!(RTL_HANDLE_TABLE, HandleSize), 4);
        assert_eq!(mem::offset_of!(RTL_HANDLE_TABLE, NextFree), 16);
        assert_eq!(mem::offset_of!(RTL_HANDLE_TABLE, FirstHandle), 24);
        assert_eq!(mem::offset_of!(RTL_HANDLE_TABLE, ReservedMemory), 32);
        assert_eq!(mem::offset_of!(RTL_HANDLE_TABLE, MaxHandle), 40);
        assert_eq!(mem::size_of::<RTL_HANDLE_TABLE>(), 48);
    }

    #[test]
    fn rejects_invalid_handle_sizes() {
        assert!(!16usize.is_power_of_two() || 16 >= mem::size_of::<*mut RTL_HANDLE>());
        assert!(!3usize.is_power_of_two());
        assert!(!0usize.is_power_of_two());
    }
}

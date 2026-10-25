//! Windows 11 x64 NTDLL memory export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;
pub type NTSTATUS = i32;
pub type HANDLE = *mut c_void;
pub type PVOID = *mut c_void;
pub type PSIZE_T = *mut usize;
pub type ULONG = u32;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;
const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;

#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, zero_bits, region_size, allocation_type, protect);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemoryEx(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG, extended_parameters: PVOID, parameter_count: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, allocation_type, protect, extended_parameters, parameter_count);
    STATUS_NOT_IMPLEMENTED
}
#[repr(C)]
pub struct IO_STATUS_BLOCK { pub status_or_pointer: usize, pub information: usize }
#[no_mangle]
pub unsafe extern "system" fn NtFlushVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, io_status_block: *mut IO_STATUS_BLOCK) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, io_status_block);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtFreeVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, free_type);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtLockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, map_type);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtProtectVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: *mut ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, new_protect, old_protect);
    STATUS_NOT_IMPLEMENTED
}
#[repr(C)]
pub struct MEMORY_BASIC_INFORMATION {
    pub base_address: PVOID, pub allocation_base: PVOID, pub allocation_protect: ULONG,
    pub partition_id: u16, pub region_size: usize, pub state: ULONG, pub protect: ULONG, pub kind: ULONG,
}
#[no_mangle]
pub unsafe extern "system" fn NtQueryVirtualMemory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: *mut SIZE_T) -> NTSTATUS {
    let _ = (process_handle, base_address, memory_information_class, memory_information, memory_information_length, return_length);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtReadVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: *mut SIZE_T) -> NTSTATUS {
    let _ = (process_handle, base_address, buffer, buffer_size, number_of_bytes_read);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtReadVirtualMemoryEx(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: *mut SIZE_T, flags: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, buffer, buffer_size, number_of_bytes_read, flags);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtUnlockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, map_type);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtWriteVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: *mut SIZE_T) -> NTSTATUS {
    let _ = (process_handle, base_address, buffer, buffer_size, number_of_bytes_written);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn ZwAllocateVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS {
    unsafe { NtAllocateVirtualMemory(process_handle, base_address, zero_bits, region_size, allocation_type, protect) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwAllocateVirtualMemoryEx(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG, extended_parameters: PVOID, parameter_count: ULONG) -> NTSTATUS {
    unsafe { NtAllocateVirtualMemoryEx(process_handle, base_address, region_size, allocation_type, protect, extended_parameters, parameter_count) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwFlushVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, io_status_block: *mut IO_STATUS_BLOCK) -> NTSTATUS {
    unsafe { NtFlushVirtualMemory(process_handle, base_address, region_size, io_status_block) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwFreeVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS {
    unsafe { NtFreeVirtualMemory(process_handle, base_address, region_size, free_type) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwLockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    unsafe { NtLockVirtualMemory(process_handle, base_address, region_size, map_type) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwProtectVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: *mut ULONG) -> NTSTATUS {
    unsafe { NtProtectVirtualMemory(process_handle, base_address, region_size, new_protect, old_protect) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwQueryVirtualMemory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: *mut SIZE_T) -> NTSTATUS {
    unsafe { NtQueryVirtualMemory(process_handle, base_address, memory_information_class, memory_information, memory_information_length, return_length) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: *mut SIZE_T) -> NTSTATUS {
    unsafe { NtReadVirtualMemory(process_handle, base_address, buffer, buffer_size, number_of_bytes_read) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemoryEx(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: *mut SIZE_T, flags: ULONG) -> NTSTATUS {
    unsafe { NtReadVirtualMemoryEx(process_handle, base_address, buffer, buffer_size, number_of_bytes_read, flags) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwUnlockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    unsafe { NtUnlockVirtualMemory(process_handle, base_address, region_size, map_type) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwWriteVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: *mut SIZE_T) -> NTSTATUS {
    unsafe { NtWriteVirtualMemory(process_handle, base_address, buffer, buffer_size, number_of_bytes_written) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddRefMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenMembership() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenMembershipEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCloneMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCmDecodeMemIoResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCmEncodeMemIoResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCommitMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyMappedMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

/// Functional non-temporal-copy fallback; callers must provide valid, non-overlapping ranges.
#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemoryNonTemporal(destination: PVOID, source: *const c_void, length: SIZE_T) {
    if length == 0 || destination.cast_const() == source { return; }
    unsafe { ptr::copy_nonoverlapping(source.cast::<u8>(), destination.cast::<u8>(), length); }
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemoryStreamTo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyOutOfProcessMemoryStreamTo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeregisterSecureMemoryCacheCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlExtendMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlExtendMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

/// Functional non-temporal-fill fallback; callers must provide a writable range.
#[no_mangle]
pub unsafe extern "system" fn RtlFillMemoryNonTemporal(destination: PVOID, length: SIZE_T, fill: u8) {
    if length == 0 { return; }
    unsafe { ptr::write_bytes(destination.cast::<u8>(), fill, length); }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFillNonVolatileMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFinalReleaseOutOfProcessMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemoryRanges() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlushSecureMemoryCache() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitOutOfProcessMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIoDecodeMemIoResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIoEncodeMemIoResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryStreamRegion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryInterfaceMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReadMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReadOutOfProcessMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRegisterSecureMemoryCacheCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleaseMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlResetMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlResetMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRevertMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSeekMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetMemoryStreamSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlStatMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryStreamRegion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWriteMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWriteNonVolatileMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAllocateVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAllocateVirtualMemoryEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFlushVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFreeVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwLockVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwProtectVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemoryEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwUnlockVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWriteVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

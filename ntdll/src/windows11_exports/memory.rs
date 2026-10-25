//! Windows x64 NTDLL virtual-memory exports.
//!
//! The implemented Nt* calls enter CognacOS through its private vector 0x83
//! native-service ABI. This is an in-tree OS bridge, not a Windows syscall
//! ordinal table; the kernel validates pointers and currently supports only
//! NT_CURRENT_PROCESS for these operations.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;
use core::arch::global_asm;

pub type NTSTATUS = i32;
pub type HANDLE = *mut c_void;
pub type PVOID = *mut c_void;
pub type PSIZE_T = *mut usize;
pub type PULONG = *mut ULONG;
pub type ULONG = u32;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;

const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_SUCCESS: NTSTATUS = 0;
const COGNAC_NT_CURRENT_PROCESS: HANDLE = usize::MAX as HANDLE;
const MEM_DECOMMIT: ULONG = 0x4000;
const MEM_RELEASE: ULONG = 0x8000;
const MEMORY_BASIC_INFORMATION_CLASS: ULONG = 0;

#[repr(C)]
pub struct IO_STATUS_BLOCK { pub status_or_pointer: usize, pub information: usize }

/// Windows x64 MEMORY_BASIC_INFORMATION (48-byte layout).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MEMORY_BASIC_INFORMATION {
    pub base_address: PVOID,
    pub allocation_base: PVOID,
    pub allocation_protect: ULONG,
    pub partition_id: u16,
    pub _alignment: u16,
    pub region_size: SIZE_T,
    pub state: ULONG,
    pub protect: ULONG,
    pub kind: ULONG,
}

global_asm!(r#"
    .text
    .p2align 4
    .global cognacos_nt_allocate_virtual_memory
cognacos_nt_allocate_virtual_memory:
    mov r10, rcx
    mov eax, 0x109
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_free_virtual_memory
cognacos_nt_free_virtual_memory:
    mov r10, rcx
    mov eax, 0x10a
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_protect_virtual_memory
cognacos_nt_protect_virtual_memory:
    mov r10, rcx
    mov eax, 0x10b
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_query_virtual_memory
cognacos_nt_query_virtual_memory:
    mov r10, rcx
    mov eax, 0x10c
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_read_virtual_memory
cognacos_nt_read_virtual_memory:
    mov r10, rcx
    mov eax, 0x10d
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_write_virtual_memory
cognacos_nt_write_virtual_memory:
    mov r10, rcx
    mov eax, 0x10e
    int 0x83
    ret
"#);

unsafe extern "system" {
    fn cognacos_nt_allocate_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS;
    fn cognacos_nt_free_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS;
    fn cognacos_nt_protect_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS;
    fn cognacos_nt_query_virtual_memory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_read_virtual_memory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_write_virtual_memory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: PSIZE_T) -> NTSTATUS;
}

#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_allocate_virtual_memory(process_handle, base_address, zero_bits, region_size, allocation_type, protect) }
}
#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemoryEx(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG, extended_parameters: PVOID, parameter_count: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, allocation_type, protect, extended_parameters, parameter_count);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtFlushVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, io_status_block: *mut IO_STATUS_BLOCK) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, io_status_block);
    // This VM currently has no section/file-backed dirty-page flush contract.
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtFreeVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if free_type != MEM_DECOMMIT && free_type != MEM_RELEASE { return STATUS_INVALID_PARAMETER; }
    unsafe { cognacos_nt_free_virtual_memory(process_handle, base_address, region_size, free_type) }
}
#[no_mangle]
pub unsafe extern "system" fn NtLockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, map_type);
    // Pinning/lock accounting and locked-page lifetime are not implemented in MM.
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtProtectVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_protect_virtual_memory(process_handle, base_address, region_size, new_protect, old_protect) }
}
#[no_mangle]
pub unsafe extern "system" fn NtQueryVirtualMemory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS {
    if memory_information.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if memory_information_class != MEMORY_BASIC_INFORMATION_CLASS { return 0xC000_0003u32 as i32; }
    unsafe { cognacos_nt_query_virtual_memory(process_handle, base_address, memory_information_class, memory_information, memory_information_length, return_length) }
}
#[no_mangle]
pub unsafe extern "system" fn NtReadVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T) -> NTSTATUS {
    if buffer_size != 0 && (base_address.is_null() || buffer.is_null()) { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_read_virtual_memory(process_handle, base_address, buffer, buffer_size, number_of_bytes_read) }
}
#[no_mangle]
pub unsafe extern "system" fn NtReadVirtualMemoryEx(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T, flags: ULONG) -> NTSTATUS {
    if flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { NtReadVirtualMemory(process_handle, base_address, buffer, buffer_size, number_of_bytes_read) }
}
#[no_mangle]
pub unsafe extern "system" fn NtUnlockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, map_type);
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtWriteVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: PSIZE_T) -> NTSTATUS {
    if buffer_size != 0 && (base_address.is_null() || buffer.is_null()) { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_write_virtual_memory(process_handle, base_address, buffer, buffer_size, number_of_bytes_written) }
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
pub unsafe extern "system" fn ZwProtectVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS {
    unsafe { NtProtectVirtualMemory(process_handle, base_address, region_size, new_protect, old_protect) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwQueryVirtualMemory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS {
    unsafe { NtQueryVirtualMemory(process_handle, base_address, memory_information_class, memory_information, memory_information_length, return_length) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T) -> NTSTATUS {
    unsafe { NtReadVirtualMemory(process_handle, base_address, buffer, buffer_size, number_of_bytes_read) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemoryEx(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T, flags: ULONG) -> NTSTATUS {
    unsafe { NtReadVirtualMemoryEx(process_handle, base_address, buffer, buffer_size, number_of_bytes_read, flags) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwUnlockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    unsafe { NtUnlockVirtualMemory(process_handle, base_address, region_size, map_type) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwWriteVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: PSIZE_T) -> NTSTATUS {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_temporal_copy_copies_exact_range() {
        let source = [0x10u8, 0x20, 0x30, 0x40, 0x50];
        let mut destination = [0u8; 7];
        unsafe {
            RtlCopyMemoryNonTemporal(destination.as_mut_ptr().cast(), source.as_ptr().cast(), source.len());
        }
        assert_eq!(&destination[..5], &source);
        assert_eq!(&destination[5..], &[0, 0]);
    }

    #[test]
    fn non_temporal_fill_fills_exact_range() {
        let mut destination = [0u8; 6];
        unsafe { RtlFillMemoryNonTemporal(destination.as_mut_ptr().cast(), 4, 0xA5); }
        assert_eq!(destination, [0xA5, 0xA5, 0xA5, 0, 0, 0]);
    }

    #[test]
    fn non_temporal_helpers_accept_zero_length_null_ranges() {
        unsafe {
            RtlCopyMemoryNonTemporal(core::ptr::null_mut(), core::ptr::null(), 0);
            RtlFillMemoryNonTemporal(core::ptr::null_mut(), 0, 0xFF);
        }
    }

    #[test]
    fn memory_basic_information_matches_windows_x64_layout() {
        assert_eq!(core::mem::size_of::<MEMORY_BASIC_INFORMATION>(), 48);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, region_size), 24);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, state), 32);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, kind), 40);
    }

    #[test]
    fn unsupported_vm_operations_fail_explicitly() {
        assert_eq!(
            unsafe { NtFlushVirtualMemory(core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut()) },
            STATUS_NOT_IMPLEMENTED
        );
        assert_eq!(
            unsafe { NtLockVirtualMemory(core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut(), 0) },
            STATUS_NOT_IMPLEMENTED
        );
    }
}

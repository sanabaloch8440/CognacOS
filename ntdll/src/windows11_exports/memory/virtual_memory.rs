//! Windows x64 NTDLL virtual-memory exports.
//!
//! The six canonical virtual-memory Nt* entry points are exact Windows x64 SYSCALL
//! stubs on native Windows-target builds. Host/test builds retain the vector
//! 0x83 implementation so the same ABI tests can run without a PE linker.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;
use core::arch::global_asm;
#[cfg(target_os = "windows")]
use super::super::syscall_stubs::{NtAllocateVirtualMemory, NtFreeVirtualMemory, NtProtectVirtualMemory, NtQueryVirtualMemory, NtReadVirtualMemory, NtWriteVirtualMemory};

pub type NTSTATUS = i32;
pub type HANDLE = *mut c_void;
pub type PVOID = *mut c_void;
pub type PSIZE_T = *mut usize;
pub type PULONG = *mut ULONG;
pub type ULONG = u32;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;

pub(super) const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
pub(super) const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
pub(super) const STATUS_SUCCESS: NTSTATUS = 0;
pub(super) const COGNAC_NT_CURRENT_PROCESS: HANDLE = usize::MAX as HANDLE;
const MEM_DECOMMIT: ULONG = 0x4000;
pub(super) const MEM_RELEASE: ULONG = 0x8000;
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
    int 0x82
    ret

    .p2align 4
    .global cognacos_nt_free_virtual_memory
cognacos_nt_free_virtual_memory:
    mov r10, rcx
    mov eax, 0x10a
    int 0x82
    ret

    .p2align 4
    .global cognacos_nt_protect_virtual_memory
cognacos_nt_protect_virtual_memory:
    mov r10, rcx
    mov eax, 0x10b
    int 0x82
    ret

    .p2align 4
    .global cognacos_nt_query_virtual_memory
cognacos_nt_query_virtual_memory:
    mov r10, rcx
    mov eax, 0x10c
    int 0x82
    ret

    .p2align 4
    .global cognacos_nt_read_virtual_memory
cognacos_nt_read_virtual_memory:
    mov r10, rcx
    mov eax, 0x10d
    int 0x82
    ret

    .p2align 4
    .global cognacos_nt_write_virtual_memory
cognacos_nt_write_virtual_memory:
    mov r10, rcx
    mov eax, 0x10e
    int 0x82
    ret

    .p2align 4
    .global cognacos_nt_lock_virtual_memory
cognacos_nt_lock_virtual_memory:
    mov r10, rcx
    mov eax, 0x10f
    int 0x82
    ret

    .p2align 4
    .global cognacos_nt_unlock_virtual_memory
cognacos_nt_unlock_virtual_memory:
    mov r10, rcx
    mov eax, 0x110
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_get_nonvolatile_token
cognacos_rtl_get_nonvolatile_token:
    mov r10, rcx
    mov eax, 0x111
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_free_nonvolatile_token
cognacos_rtl_free_nonvolatile_token:
    mov r10, rcx
    mov eax, 0x112
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_flush_nonvolatile_memory
cognacos_rtl_flush_nonvolatile_memory:
    mov r10, rcx
    mov eax, 0x113
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_write_nonvolatile_memory
cognacos_rtl_write_nonvolatile_memory:
    mov r10, rcx
    mov eax, 0x114
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_fill_nonvolatile_memory
cognacos_rtl_fill_nonvolatile_memory:
    mov r10, rcx
    mov eax, 0x115
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_check_token_membership
cognacos_rtl_check_token_membership:
    mov r10, rcx
    mov eax, 0x116
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_create_section
cognacos_rtl_create_section:
    mov r10, rcx
    mov eax, 0x117
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_dereference_section
cognacos_rtl_dereference_section:
    mov r10, rcx
    mov eax, 0x118
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_section_copy
cognacos_rtl_section_copy:
    mov r10, rcx
    mov eax, 0x119
    int 0x82
    ret

    .p2align 4
    .global cognacos_rtl_reference_section
cognacos_rtl_reference_section:
    mov r10, rcx
    mov eax, 0x11a
    int 0x82
    ret

    .p2align 4
    .global cognacos_nt_create_section
cognacos_nt_create_section:
    mov r10, rcx
    mov eax, 0x132
    int 0x82
    ret
"#);

unsafe extern "system" {
    pub(super) fn cognacos_nt_allocate_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS;
    pub(super) fn cognacos_nt_free_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS;
    fn cognacos_nt_protect_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS;
    fn cognacos_nt_query_virtual_memory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS;
    pub(super) fn cognacos_nt_read_virtual_memory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_write_virtual_memory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_lock_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS;
    fn cognacos_nt_unlock_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS;
    pub(super) fn cognacos_rtl_get_nonvolatile_token(base: PVOID, size: SIZE_T, token_out: *mut PVOID) -> NTSTATUS;
    pub(super) fn cognacos_rtl_free_nonvolatile_token(token: PVOID) -> NTSTATUS;
    pub(super) fn cognacos_rtl_flush_nonvolatile_memory(token: PVOID, buffer: PVOID, size: SIZE_T, flags: ULONG) -> NTSTATUS;
    pub(super) fn cognacos_rtl_write_nonvolatile_memory(token: PVOID, destination: PVOID, source: *const c_void, size: SIZE_T, flags: ULONG) -> NTSTATUS;
    pub(super) fn cognacos_rtl_fill_nonvolatile_memory(token: PVOID, destination: PVOID, size: SIZE_T, value: u8, flags: ULONG) -> NTSTATUS;
    pub(super) fn cognacos_rtl_check_token_membership(token: HANDLE, sid: PVOID, member: *mut u8, flags: ULONG) -> NTSTATUS;
    pub(super) fn cognacos_rtl_create_section(size: SIZE_T, handle_out: *mut PVOID) -> NTSTATUS;
    pub(super) fn cognacos_rtl_dereference_section(handle: PVOID) -> NTSTATUS;
    fn cognacos_rtl_section_copy(handle: PVOID, offset: u64, buffer: PVOID, length: SIZE_T, write: ULONG) -> NTSTATUS;
    pub(super) fn cognacos_rtl_reference_section(handle: PVOID) -> NTSTATUS;
    pub(super) fn cognacos_rtl_section_copy(handle: PVOID, offset: u64, buffer: PVOID, length: SIZE_T, write: ULONG) -> NTSTATUS;
    fn cognacos_nt_create_section(section_handle: *mut HANDLE, desired_access: ULONG, object_attributes: PVOID, maximum_size: *const i64, page_protection: ULONG, allocation_attributes: ULONG, file_handle: HANDLE) -> NTSTATUS;
}

#[cfg(not(target_os = "windows"))]
#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_allocate_virtual_memory(process_handle, base_address, zero_bits, region_size, allocation_type, protect) }
}
#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemoryEx(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG, extended_parameters: PVOID, parameter_count: ULONG) -> NTSTATUS {
    if parameter_count == 0 && extended_parameters.is_null() {
        return unsafe { NtAllocateVirtualMemory(process_handle, base_address, 0, region_size, allocation_type, protect) };
    }
    // Unknown/unsupported extended parameter types must not be silently ignored.
    if parameter_count != 0 && extended_parameters.is_null() { return STATUS_INVALID_PARAMETER; }
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtFlushVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, io_status_block: *mut IO_STATUS_BLOCK) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, io_status_block);
    // This VM currently has no section/file-backed dirty-page flush contract.
    STATUS_NOT_IMPLEMENTED
}
#[cfg(not(target_os = "windows"))]
#[no_mangle]
pub unsafe extern "system" fn NtFreeVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if free_type != MEM_DECOMMIT && free_type != MEM_RELEASE { return STATUS_INVALID_PARAMETER; }
    let old_base = unsafe { ptr::read_unaligned(base_address) };
    let mut callback_size = unsafe { ptr::read_unaligned(region_size) };
    if free_type == MEM_RELEASE && callback_size == 0 && !old_base.is_null() {
        let mut info = core::mem::MaybeUninit::<MEMORY_BASIC_INFORMATION>::zeroed();
        let query_status = unsafe { NtQueryVirtualMemory(process_handle, old_base, MEMORY_BASIC_INFORMATION_CLASS, info.as_mut_ptr().cast(), core::mem::size_of::<MEMORY_BASIC_INFORMATION>(), ptr::null_mut()) };
        if query_status >= 0 { callback_size = unsafe { (*info.as_ptr()).region_size }; }
    }
    let status = unsafe { cognacos_nt_free_virtual_memory(process_handle, base_address, region_size, free_type) };
    if status >= 0 { notify_secure_cache_callbacks(old_base, callback_size); }
    status
}
#[no_mangle]
pub unsafe extern "system" fn NtLockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if map_type != 1 { return STATUS_INVALID_PARAMETER; }
    unsafe { cognacos_nt_lock_virtual_memory(process_handle, base_address, region_size, map_type) }
}
#[cfg(not(target_os = "windows"))]
#[no_mangle]
pub unsafe extern "system" fn NtProtectVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    let status = unsafe { cognacos_nt_protect_virtual_memory(process_handle, base_address, region_size, new_protect, old_protect) };
    if status >= 0 {
        let changed_base = unsafe { ptr::read_unaligned(base_address) };
        let changed_size = unsafe { ptr::read_unaligned(region_size) };
        notify_secure_cache_callbacks(changed_base, changed_size);
    }
    status
}
#[cfg(not(target_os = "windows"))]
#[no_mangle]
pub unsafe extern "system" fn NtQueryVirtualMemory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS {
    if memory_information.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if memory_information_class != MEMORY_BASIC_INFORMATION_CLASS { return 0xC000_0003u32 as i32; }
    unsafe { cognacos_nt_query_virtual_memory(process_handle, base_address, memory_information_class, memory_information, memory_information_length, return_length) }
}
#[cfg(not(target_os = "windows"))]
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
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if map_type != 1 { return STATUS_INVALID_PARAMETER; }
    unsafe { cognacos_nt_unlock_virtual_memory(process_handle, base_address, region_size, map_type) }
}
#[cfg(not(target_os = "windows"))]
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


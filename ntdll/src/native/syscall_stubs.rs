//! Windows x64 canonical SYSCALL entry points required by native process startup.
#![allow(non_snake_case)]

use core::ffi::c_void;

core::arch::global_asm!(r#"
    .text
    .p2align 4
    .globl NtClose
NtClose:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x0f, %eax
    syscall
    ret

    .p2align 4
    .globl NtOpenKey
NtOpenKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x12, %eax
    syscall
    ret

    .p2align 4
    .globl NtQueryValueKey
NtQueryValueKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x17, %eax
    syscall
    ret

    .p2align 4
    .globl NtOpenFile
NtOpenFile:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x33, %eax
    syscall
    ret

    .p2align 4
    .globl NtReadFile
NtReadFile:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x06, %eax
    syscall
    ret

    .p2align 4
    .globl NtAllocateVirtualMemory
NtAllocateVirtualMemory:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x18, %eax
    syscall
    ret

    .p2align 4
    .globl NtFreeVirtualMemory
NtFreeVirtualMemory:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x1e, %eax
    syscall
    ret

    .p2align 4
    .globl NtProtectVirtualMemory
NtProtectVirtualMemory:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x50, %eax
    syscall
    ret

    .p2align 4
    .globl NtQueryVirtualMemory
NtQueryVirtualMemory:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x23, %eax
    syscall
    ret

    .p2align 4
    .globl NtReadVirtualMemory
NtReadVirtualMemory:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x3f, %eax
    syscall
    ret

    .p2align 4
    .globl NtWriteVirtualMemory
NtWriteVirtualMemory:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x3a, %eax
    syscall
    ret

    .p2align 4
    .globl NtYieldExecution
NtYieldExecution:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x46, %eax
    syscall
    ret

    .p2align 4
    .globl NtQueryInformationProcess
NtQueryInformationProcess:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x19, %eax
    syscall
    ret

    .p2align 4
    .globl NtQueryInformationThread
NtQueryInformationThread:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x25, %eax
    syscall
    ret

    .p2align 4
    .globl NtQuerySystemInformation
NtQuerySystemInformation:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x36, %eax
    syscall
    ret

    # CognacOS service ordinals are read back from these canonical Windows
    # x64 stubs by the kernel at boot; callers use the standard SYSCALL ABI.
    .p2align 4
    .globl NtCreateKey
NtCreateKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x145, %eax
    syscall
    ret

    .p2align 4
    .globl NtSetValueKey
NtSetValueKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x146, %eax
    syscall
    ret

    .p2align 4
    .globl NtDeleteValueKey
NtDeleteValueKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x147, %eax
    syscall
    ret

    .p2align 4
    .globl NtDeleteKey
NtDeleteKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x148, %eax
    syscall
    ret

    .p2align 4
    .globl NtFlushKey
NtFlushKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x149, %eax
    syscall
    ret

    .p2align 4
    .globl NtEnumerateKey
NtEnumerateKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x14a, %eax
    syscall
    ret

    .p2align 4
    .globl NtEnumerateValueKey
NtEnumerateValueKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x14b, %eax
    syscall
    ret

    .p2align 4
    .globl NtQueryKey
NtQueryKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x14c, %eax
    syscall
    ret
"#, options(att_syntax));

unsafe extern "system" {
    pub fn NtClose(handle: *mut c_void) -> i32;
    pub fn NtOpenKey(key: *mut *mut c_void, access: u32, attributes: *const c_void) -> i32;
    pub fn NtQueryValueKey(key: *mut c_void, name: *const c_void, class: u32, information: *mut c_void, length: u32, return_length: *mut u32) -> i32;
    pub fn NtOpenFile(file_handle: *mut *mut c_void, desired_access: u32, object_attributes: *const c_void, io_status_block: *mut c_void, share_access: u32, open_options: u32) -> i32;
    pub fn NtReadFile(file_handle: *mut c_void, event: *mut c_void, apc_routine: *mut c_void, apc_context: *mut c_void, io_status_block: *mut c_void, buffer: *mut c_void, length: u32, byte_offset: *mut i64, key: *mut u32) -> i32;
    pub fn NtAllocateVirtualMemory(process: *mut c_void, base_address: *mut *mut c_void, zero_bits: usize, region_size: *mut usize, allocation_type: u32, protect: u32) -> i32;
    pub fn NtFreeVirtualMemory(process: *mut c_void, base_address: *mut *mut c_void, region_size: *mut usize, free_type: u32) -> i32;
    pub fn NtProtectVirtualMemory(process: *mut c_void, base_address: *mut *mut c_void, region_size: *mut usize, new_protect: u32, old_protect: *mut u32) -> i32;
    pub fn NtQueryVirtualMemory(process: *mut c_void, base_address: *mut c_void, class: u32, information: *mut c_void, length: usize, return_length: *mut usize) -> i32;
    pub fn NtReadVirtualMemory(process: *mut c_void, base_address: *mut c_void, buffer: *mut c_void, length: usize, bytes_read: *mut usize) -> i32;
    pub fn NtWriteVirtualMemory(process: *mut c_void, base_address: *mut c_void, buffer: *mut c_void, length: usize, bytes_written: *mut usize) -> i32;
    pub fn NtYieldExecution() -> i32;
    pub fn NtQueryInformationProcess(process: *mut c_void, class: u32, information: *mut c_void, length: u32, return_length: *mut u32) -> i32;
    pub fn NtQueryInformationThread(thread: *mut c_void, class: u32, information: *mut c_void, length: u32, return_length: *mut u32) -> i32;
    pub fn NtQuerySystemInformation(class: u32, information: *mut c_void, length: u32, return_length: *mut u32) -> i32;
    pub fn NtCreateKey(key: *mut *mut c_void, access: u32, attributes: *const crate::reg::OBJECT_ATTRIBUTES, title: u32, class: *const crate::reg::UNICODE_STRING, options: u32, disposition: *mut u32) -> i32;
    pub fn NtSetValueKey(key: *mut c_void, name: *const crate::reg::UNICODE_STRING, title: u32, value_type: u32, data: *mut c_void, data_size: u32) -> i32;
    pub fn NtDeleteValueKey(key: *mut c_void, name: *const crate::reg::UNICODE_STRING) -> i32;
    pub fn NtDeleteKey(key: *mut c_void) -> i32;
    pub fn NtFlushKey(key: *mut c_void) -> i32;
    pub fn NtEnumerateKey(key: *mut c_void, index: u32, class: u32, information: *mut c_void, length: u32, result_length: *mut u32) -> i32;
    pub fn NtEnumerateValueKey(key: *mut c_void, index: u32, class: u32, information: *mut c_void, length: u32, result_length: *mut u32) -> i32;
    pub fn NtQueryKey(key: *mut c_void, class: u32, information: *mut c_void, length: u32, result_length: *mut u32) -> i32;
}

//! Windows x64 canonical SYSCALL entry points required by native process startup.
#![allow(non_snake_case)]

use core::ffi::c_void;

core::arch::global_asm!(
    include_str!(concat!(env!("OUT_DIR"), "/ntdll_syscall_stubs.S")),
    options(att_syntax)
);

unsafe extern "system" {
    pub fn NtClose(handle: *mut c_void) -> i32;
    pub fn NtCreateEvent(event_handle: *mut *mut c_void, desired_access: u32, object_attributes: *const c_void, event_type: u32, initial_state: u32) -> i32;
    pub fn NtOpenEvent(event_handle: *mut *mut c_void, desired_access: u32, object_attributes: *const c_void) -> i32;
    pub fn NtSetEvent(event_handle: *mut c_void, previous_state: *mut i32) -> i32;
    pub fn NtResetEvent(event_handle: *mut c_void, previous_state: *mut i32) -> i32;
    pub fn NtWaitForSingleObject(handle: *mut c_void, alertable: u8, timeout: *const i64) -> i32;
    pub fn NtOpenProcess(process_handle: *mut *mut c_void, desired_access: u32, object_attributes: *const c_void, client_id: *const c_void) -> i32;
    pub fn NtGetNextProcess(process_handle: *mut c_void, desired_access: u32, handle_attributes: u32, flags: u32, new_process_handle: *mut *mut c_void) -> i32;
    pub fn NtOpenKey(key: *mut *mut c_void, access: u32, attributes: *const crate::reg::OBJECT_ATTRIBUTES) -> i32;
    pub fn NtQueryValueKey(key: *mut c_void, name: *const crate::reg::UNICODE_STRING, class: u32, information: *mut c_void, length: u32, return_length: *mut u32) -> i32;
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

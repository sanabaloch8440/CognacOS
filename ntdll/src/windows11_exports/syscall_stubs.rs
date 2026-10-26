//! Windows x64 canonical SYSCALL entry points required by native process startup.
#![allow(non_snake_case)]

use core::ffi::c_void;

core::arch::global_asm!(r#"
    .text
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
"#, options(att_syntax));

unsafe extern "system" {
    pub fn NtYieldExecution() -> i32;
    pub fn NtQueryInformationProcess(process: *mut c_void, class: u32, information: *mut c_void, length: u32, return_length: *mut u32) -> i32;
    pub fn NtQueryInformationThread(thread: *mut c_void, class: u32, information: *mut c_void, length: u32, return_length: *mut u32) -> i32;
    pub fn NtQuerySystemInformation(class: u32, information: *mut c_void, length: u32, return_length: *mut u32) -> i32;
}

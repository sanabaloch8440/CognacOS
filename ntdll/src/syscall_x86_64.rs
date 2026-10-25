//! Rust source representation of CognacOS's x86-64 native syscall stubs.
//!
//! Source correspondence: root/wine/dlls/ntdll/cognac_syscall_x86_64.S.
//! These are Windows x64 ABI entry stubs: R10 receives RCX, EAX receives the
//! service number, then SYSCALL transfers control to the kernel. The numeric
//! IDs below preserve the current C/assembly defaults and are version-specific;
//! production builds should generate/pin them against the intended Windows
//! syscall table. This source is intentionally not wired into a build yet.

#![cfg(target_arch = "x86_64")]

use core::arch::global_asm;

// These service numbers mirror the existing CognacOS assembly source. They
// are not universal Windows ABI constants and may differ by Windows build.
pub const COGNAC_NT_CLOSE_ID: u32 = 0x0f;
pub const COGNAC_NT_YIELD_EXECUTION_ID: u32 = 0x46;
pub const COGNAC_NT_QUERY_INFORMATION_PROCESS_ID: u32 = 0x19;
pub const COGNAC_NT_QUERY_INFORMATION_THREAD_ID: u32 = 0x25;
pub const COGNAC_NT_QUERY_SYSTEM_INFORMATION_ID: u32 = 0x36;
pub const COGNAC_NT_OPEN_KEY_ID: u32 = 0x12;
pub const COGNAC_NT_QUERY_VALUE_KEY_ID: u32 = 0x17;

// Keep these as assembly entry points rather than Rust functions: the syscall
// ABI requires the exact mov-r10-rcx, mov-eax-immediate, syscall, ret sequence.
// global_asm! uses LLVM's integrated assembler and lets the target object
// format select the platform's symbol representation.
global_asm!(
    r#"
    .text
    .p2align 4
    .globl NtClose
NtClose:
    .byte 0x4c, 0x8b, 0xd1
    movl $0x0f, %eax
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
"#
);

/// NTSTATUS-compatible return value used by these syscall entry points.
pub type NTSTATUS = i32;

/// Declaration for the native syscall stub. This is only a signature; the
/// implementation is the assembly symbol emitted above.
unsafe extern "system" {
    pub fn NtClose(handle: *mut core::ffi::c_void) -> NTSTATUS;
    pub fn NtYieldExecution() -> NTSTATUS;
    pub fn NtQueryInformationProcess(
        process_handle: *mut core::ffi::c_void,
        process_information_class: u32,
        process_information: *mut core::ffi::c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> NTSTATUS;
    pub fn NtQueryInformationThread(
        thread_handle: *mut core::ffi::c_void,
        thread_information_class: u32,
        thread_information: *mut core::ffi::c_void,
        thread_information_length: u32,
        return_length: *mut u32,
    ) -> NTSTATUS;
    pub fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut core::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> NTSTATUS;
    pub fn NtOpenKey(key: *mut *mut core::ffi::c_void, access: u32, attributes: *const core::ffi::c_void) -> NTSTATUS;
    pub fn NtQueryValueKey(
        key: *mut core::ffi::c_void,
        value_name: *const core::ffi::c_void,
        information_class: u32,
        information: *mut core::ffi::c_void,
        information_length: u32,
        result_length: *mut u32,
    ) -> NTSTATUS;
}

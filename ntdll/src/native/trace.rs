//! Low-level NTDLL trace channel shared by loader/runtime modules.
//!
//! Events use the shared Windows x64 SYSCALL entry with the internal trace
//! service selector 0x144. The kernel rate-limits output and prints event IDs
//! plus two machine-word arguments for serial,
//! WinDBG serial capture, and GDB/QEMU log correlation.
#![allow(dead_code)]

pub const TRACE_LOADER_ENTER: u64 = 0x1001;
pub const TRACE_LOADER_IMAGE: u64 = 0x1002;
pub const TRACE_LOADER_IMPORT: u64 = 0x1003;
pub const TRACE_LOADER_RELOC: u64 = 0x1004;
pub const TRACE_LOADER_TLS: u64 = 0x1005;
pub const TRACE_LOADER_DLLMAIN: u64 = 0x1006;
pub const TRACE_EXCEPTION_ENTER: u64 = 0x2001;
pub const TRACE_HEAP_ALLOC: u64 = 0x3001;
pub const TRACE_HEAP_FREE: u64 = 0x3002;
pub const TRACE_SYSCALL_BEGIN: u64 = 0x4001;
pub const TRACE_SYSCALL_END: u64 = 0x4002;
pub const TRACE_INIT_STAGE: u64 = 0x5001;

/// Emit a structured NTDLL diagnostic event. The two values can be addresses,
/// sizes, statuses, or module-specific values; event IDs stay stable across builds.
#[inline(never)]
pub fn trace(event: u64, arg0: u64, arg1: u64) {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    unsafe {
        let status: u64;
        core::arch::asm!(
            "mov eax, 0x144",
            "syscall",
            in("r10") event,
            in("rdx") arg0,
            in("r8") arg1,
            lateout("rax") status,
            options(preserves_flags),
        );
        let _ = status;
    }
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    { let _ = (event, arg0, arg1); }
}

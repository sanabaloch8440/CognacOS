//! CognacOS kernel32.dll: Rust x64 export-compatible placeholder layer.
#![cfg_attr(target_os = "windows", no_std)]
#![allow(non_snake_case, dead_code)]

use cognacos_nt_abi::windows::{HANDLE, ULONG, ULONG_PTR};

const ERROR_CALL_NOT_IMPLEMENTED: ULONG = 120;

/// Thread-local last-error storage is not implemented yet.
#[no_mangle]
pub extern "system" fn GetLastError() -> ULONG { ERROR_CALL_NOT_IMPLEMENTED }

/// Placeholder until per-thread Win32 last-error storage is wired up.
#[no_mangle]
pub extern "system" fn SetLastError(_error: ULONG) {}

/// Windows x64 pseudo-handle for the current process.
#[no_mangle]
pub extern "system" fn GetCurrentProcess() -> HANDLE { usize::MAX as HANDLE }

/// Windows x64 pseudo-handle for the current thread.
#[no_mangle]
pub extern "system" fn GetCurrentThread() -> HANDLE { (usize::MAX - 1) as HANDLE }

include!(concat!(env!("OUT_DIR"), "/kernel32_export_stubs.rs"));

#[cfg(target_os = "windows")]
core::arch::global_asm!(r#"
    .text
    .p2align 4
    .globl DllMain
DllMain:
    movl $1, %eax
    ret
"#, options(att_syntax));

#[cfg(target_os = "windows")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! { loop { core::hint::spin_loop(); } }

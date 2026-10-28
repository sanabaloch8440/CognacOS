//! CognacOS shell32.dll: Rust x64 shell API placeholder layer.
#![cfg_attr(target_os = "windows", no_std)]
#![allow(non_snake_case, dead_code)]

use cognacos_nt_abi::windows::{NTSTATUS, ULONG_PTR};

// HRESULT E_NOTIMPL. NTSTATUS is used only as the shared 32-bit signed ABI carrier.
const HRESULT_E_NOTIMPL: NTSTATUS = 0x8000_4001u32 as i32;

include!(concat!(env!("OUT_DIR"), "/shell32_export_stubs.rs"));

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

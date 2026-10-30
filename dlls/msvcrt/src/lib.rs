//! CognacOS msvcrt.dll: Windows 11 x64 export-compatible Rust stub DLL.
//! Export presence is not behavioral CRT compatibility; generated entries return zero.
#![cfg_attr(target_os = "windows", no_std)]
#![allow(non_snake_case, dead_code)]

include!(concat!(env!("OUT_DIR"), "/msvcrt_exports.rs"));

#[cfg(target_os = "windows")]
core::arch::global_asm!(
    include_str!(concat!(env!("OUT_DIR"), "/msvcrt_runtime.S")),
    options(att_syntax)
);

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

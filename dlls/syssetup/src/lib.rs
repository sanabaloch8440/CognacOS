//! CognacOS syssetup.dll: Windows setup export-compatibility layer.
#![cfg_attr(target_os = "windows", no_std)]
#![allow(non_snake_case, dead_code)]
use core::ffi::c_void;
include!(concat!(env!("OUT_DIR"), "/syssetup_export_stubs.rs"));
#[cfg(target_os = "windows")]
core::arch::global_asm!(r#"
    .text
    .p2align 4
    .globl cognac_syssetup_dll_entry
cognac_syssetup_dll_entry:
    movl $1, %eax
    ret
"#, options(att_syntax));
#[cfg(target_os = "windows")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! { loop { core::hint::spin_loop(); } }

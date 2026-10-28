//! CognacOS kernelbase.dll Rust placeholder layer.
#![cfg_attr(target_os="windows",no_std)]
#![allow(non_snake_case,dead_code)]
use cognacos_nt_abi::windows::{HANDLE,ULONG,ULONG_PTR};
const ERROR_CALL_NOT_IMPLEMENTED:ULONG=120;
#[no_mangle] pub extern "system" fn GetLastError()->ULONG{ERROR_CALL_NOT_IMPLEMENTED}
#[no_mangle] pub extern "system" fn SetLastError(_error:ULONG){}
#[no_mangle] pub extern "system" fn GetCurrentProcess()->HANDLE{usize::MAX as HANDLE}
#[no_mangle] pub extern "system" fn GetCurrentThread()->HANDLE{(usize::MAX-1) as HANDLE}
include!(concat!(env!("OUT_DIR"),"/kernelbase_exports.rs"));
#[cfg(target_os="windows")] core::arch::global_asm!(r#"
.text
.p2align 4
.globl DllMain
DllMain:
 movl $1, %eax
 ret
"#,options(att_syntax));
#[cfg(target_os="windows")] #[panic_handler] fn panic(_info:&core::panic::PanicInfo<'_>)->!{loop{core::hint::spin_loop();}}

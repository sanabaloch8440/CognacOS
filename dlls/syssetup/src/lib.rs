//! CognacOS syssetup.dll: Windows setup export-compatibility layer.
#![cfg_attr(target_os = "windows", no_std)]
#![allow(non_snake_case, dead_code)]
use core::ffi::c_void;
include!(concat!(env!("OUT_DIR"), "/syssetup_export_stubs.rs"));
#[no_mangle]
pub extern "system" fn DllMain(_module: *mut c_void, _reason: u32, _reserved: *mut c_void) -> i32 { 1 }
#[cfg(target_os = "windows")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! { loop { core::hint::spin_loop(); } }

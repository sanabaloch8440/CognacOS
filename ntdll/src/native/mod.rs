//! Native Windows 11 x64 NTDLL implementation modules.
#![allow(non_snake_case, clippy::missing_safety_doc)]
#[cfg(target_os = "windows")]
pub(crate) mod syscall_stubs;
mod syscall;
mod runtime;
mod crt;
mod misc;
mod process_snapshot;
mod appcompat;
mod telemetry;
pub(crate) mod etw;
mod csr;
mod threadpool;
mod prefix;
mod debug;
pub mod resource;
mod bitmap;
mod peb;
pub mod printf;
pub mod alpc;
pub mod atom;
pub mod env;
pub mod version;
pub mod exception;
pub mod time;
mod strings;
pub mod crypt;
pub mod path;
mod loader;
pub mod heap;
mod memory;
mod security;
mod registry;
#[path = "process/mod.rs"]
pub mod process;
pub mod debugbuffer;
pub mod handletable;
pub mod math;
pub mod rbtree;
pub mod reg;
pub mod rtl;
pub mod rtl_bitmap;
pub mod rtl_error;
pub mod rtl_heap;
pub mod rtl_large_int;
pub mod rtl_loader;
pub mod rtl_memory;
pub mod rtl_native;
pub mod rtl_peb;
pub mod rtl_string;
pub mod string;
pub mod trace;
#[cfg(target_os = "windows")]
pub(crate) unsafe fn initialize_native_process() {
    let _ = unsafe { exception::RtlRegisterExceptionDispatcher() };
}
#[path = "process/parameters.rs"]
pub mod process_parameters;
#[path = "legacy_misc.rs"]
pub mod legacy_misc;
#[path = "legacy_path.rs"]
pub mod legacy_path;
#[path = "legacy_printf.rs"]
pub mod legacy_printf;
#[path = "legacy_resource.rs"]
pub mod legacy_resource;
#[path = "legacy_time.rs"]
pub mod legacy_time;

//! CognacOS native NTDLL Rust port collection.
//!
//! Modules are kept separate while the C-to-Rust ports are validated. This
//! crate currently provides a compilation/integration boundary; it does not
//! yet replace the freestanding PE32+ bootstrap in root/wine/dlls/ntdll.
//! Export collisions and ABI wiring must be resolved before linking these
//! modules into a single production DLL.
#![allow(non_camel_case_types, non_snake_case, dead_code, improper_ctypes)]

pub mod alpc;
pub mod atom;
pub mod crypt;
pub mod debugbuffer;
pub mod env;
pub mod exception;
pub mod handletable;
pub mod heap;
pub mod math;
pub mod misc;
pub mod path;
pub mod printf;
pub mod process;
pub mod rbtree;
pub mod reg;
pub mod resource;
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
pub mod syscall_x86_64;
pub mod time;
pub mod version;

#[cfg(feature = "relay-port")]
pub mod relay;

// Generate ABI placeholders for exports not yet implemented by the Rust ports.
include!(concat!(env!("OUT_DIR"), "/ntdll_export_stubs.rs"));

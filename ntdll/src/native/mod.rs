//! Native Windows 11 x64 NTDLL implementation modules.
//!
//! This is the single in-crate implementation tree for the Windows 11 export
//! surface. Subsystems are organized by responsibility; public symbols are
//! linked into the same ntdll.dll as the established RTL/runtime modules.
//! Names in the pinned export inventory do not imply complete semantics until
//! the corresponding implementation and ABI tests are complete.
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
mod resource;
mod bitmap;
mod peb;

mod printf;
mod alpc;
mod atom;
mod env;
mod version;

pub mod exception;
mod time;
mod strings;
mod crypt;
mod path;

mod loader;
pub mod heap;
mod memory;
mod security;
mod registry;
pub mod process;

// Consolidated runtime/RTL modules migrated from the former src/ root.
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
#[cfg(not(target_os = "windows"))]
pub mod syscall_x86_64;
pub mod trace;

/// Process-attach initialization invoked by the PE DLL entry point.
#[cfg(target_os = "windows")]
pub(crate) unsafe fn initialize_native_process() {
    let _ = unsafe { exception::RtlRegisterExceptionDispatcher() };
}

// Previous root-level implementations retained under native/legacy_* while
// duplicate symbols are reconciled subsystem-by-subsystem.
#[path = "legacy_atom.rs"]
pub mod legacy_atom;
#[path = "legacy_env.rs"]
pub mod legacy_env;
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
#[path = "legacy_version.rs"]
pub mod legacy_version;

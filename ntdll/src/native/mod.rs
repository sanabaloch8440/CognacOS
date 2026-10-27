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
mod etw;
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

mod exception;
mod time;
mod strings;
mod crypt;
mod path;

mod loader;
mod heap;
mod memory;
mod security;
mod registry;
mod process;

/// Process-attach initialization invoked by the PE DLL entry point.
#[cfg(target_os = "windows")]
pub(crate) unsafe fn initialize_native_process() {
    let _ = unsafe { exception::RtlRegisterExceptionDispatcher() };
}

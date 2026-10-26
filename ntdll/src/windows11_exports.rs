//! Windows 11 x64 NTDLL named-export surface.
//!
//! This file is the module index for the pinned Windows 11 x64 export surface.
//! Implemented routines live in the established NTDLL source modules; remaining
//! name-level placeholders are grouped into focused subsystem files below.
//! Their generic zero-argument signatures are not ABI-safe Windows replacements.
#![allow(non_snake_case, clippy::missing_safety_doc)]

#[cfg(target_os = "windows")]
pub(crate) mod syscall_stubs;
mod syscall;
mod runtime;
mod crt;
mod misc;
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
mod registry;
mod process;

/// Process-attach initialization invoked by the PE DLL entry point.
#[cfg(target_os = "windows")]
pub(crate) unsafe fn initialize_native_process() {
    let _ = unsafe { exception::RtlRegisterExceptionDispatcher() };
}

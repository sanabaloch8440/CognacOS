//! Host-side harness for the bounded Se runtime. The process/thread hooks are
//! intentionally inert here; tests exercise descriptor, token metadata, DACL and
//! privilege behavior without booting the kernel.
#![allow(dead_code)]

mod process {
    use core::ffi::c_void;
    pub unsafe fn copy_from_current_user(_source: u64, _destination: &mut [u8]) -> Result<(), ()> {
        Err(())
    }
    pub fn reference_primary_token(_pid: u64) -> *mut c_void { core::ptr::null_mut() }
}
mod thread {
    use core::ffi::c_void;
    pub fn current_process_id() -> u64 { 0 }
    pub fn reference_current_impersonation_token() -> *mut c_void { core::ptr::null_mut() }
}
#[path = "../kernel/src/ntoskrnl/se/runtime.rs"]
mod runtime;

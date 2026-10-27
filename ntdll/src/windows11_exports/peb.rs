//! Windows 11 x64 NTDLL peb export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;

/// Return the active process's Windows x64 PEB through the TEB GS slot.
#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentPeb() -> *mut c_void {
    let peb: *mut c_void;
    unsafe { core::arch::asm!("mov {}, gs:[0x60]", out(reg) peb, options(nostack, preserves_flags, readonly)); }
    // The kernel creates a zeroed PEB. Lazily initialize its default heap the
    // first time NTDLL exposes the PEB, then publish the real heap object using
    // the same Windows PEB.ProcessHeap field used by normal user-mode callers.
    if !peb.is_null() {
        let slot = unsafe { peb.cast::<u8>().add(0x30).cast::<*mut c_void>() };
        if unsafe { slot.read() }.is_null() {
            let heap = unsafe { crate::rtl_heap::RtlGetProcessHeap() };
            if !heap.is_null() { unsafe { slot.write(heap) }; }
        }
    }
    peb
}


#[no_mangle]
pub unsafe extern "system" fn RtlGetNtGlobalFlags() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTryAcquirePebLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

//! Windows 11 x64 NTDLL peb export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

#[no_mangle]
pub unsafe extern "system" fn RtlGetNtGlobalFlags() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTryAcquirePebLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

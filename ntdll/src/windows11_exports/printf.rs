//! Windows 11 x64 NTDLL formatting compatibility exports.
//! Only names present in the repository's pinned Windows 11 x64 NTDLL export
//! inventory belong here. These legacy entry points remain explicit
//! STATUS_NOT_IMPLEMENTED fallbacks until ABI-correct varargs/va_list bridges
//! are available; they must not be mistaken for working CRT formatters.
#![allow(non_snake_case, clippy::missing_safety_doc)]

#[no_mangle]
pub unsafe extern "system" fn _vscprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _vscwprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn sprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn sprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn swprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn swprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

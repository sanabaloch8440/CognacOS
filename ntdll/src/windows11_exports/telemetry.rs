//! Windows SQM/telemetry exports with ABI-typed fallback behavior.
//!
//! The public signatures below are based on Wine's ntdll/misc.c declarations.
//! Telemetry is optional in this runtime; no provider/session is fabricated.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;

type HANDLE = *mut c_void;
type DWORD = u32;
type BOOL = i32;
type NTSTATUS = i32;

const STATUS_SUCCESS: NTSTATUS = 0;
const INVALID_HANDLE_VALUE: HANDLE = usize::MAX as HANDLE;

#[repr(C)]
pub struct GUID {
    pub Data1: u32,
    pub Data2: u16,
    pub Data3: u16,
    pub Data4: [u8; 8],
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmEndSession(_session: HANDLE) -> NTSTATUS {
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmIncrementDWORD(_unknown1: DWORD, _unknown2: DWORD, _unknown3: DWORD) {
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmIsOptedIn() -> BOOL {
    0
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmStartSession(
    _session_guid: *mut GUID,
    _session_id: DWORD,
    _unknown: DWORD,
) -> HANDLE {
    INVALID_HANDLE_VALUE
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetDWORD(
    _session: HANDLE,
    _datapoint_id: DWORD,
    _datapoint_value: DWORD,
) {
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetIfMaxDWORD(_unknown1: DWORD, _unknown2: DWORD, _unknown3: DWORD) {
}

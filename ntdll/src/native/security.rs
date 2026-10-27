//! NTDLL token-membership queries.
#![allow(non_snake_case, clippy::missing_safety_doc)]
use super::memory::{native_check_token_membership, HANDLE, PVOID, ULONG, NTSTATUS};
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;

#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenMembership(token: HANDLE, sid: PVOID, member: *mut u8) -> NTSTATUS {
    if sid.is_null() || member.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { native_check_token_membership(token, sid, member, 0) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenMembershipEx(token: HANDLE, sid: PVOID, flags: ULONG, member: *mut u8) -> NTSTATUS {
    if sid.is_null() || member.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { native_check_token_membership(token, sid, member, flags) }
}

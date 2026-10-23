//! Numeric conversion and counted-string RTL entry points with Windows x64 ABI.
use super::{AnsiString, UnicodeString};

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC0000023u32 as i32;
const STATUS_NOT_IMPLEMENTED: i32 = 0xC0000002u32 as i32;

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_free_unicode_string(string: *mut UnicodeString) {
    if string.is_null() { return; }
    let s = unsafe { &mut *string };
    if !s.buffer.is_null() { let _ = super::pool::deallocate(s.buffer.cast::<u8>()); }
    s.length = 0; s.maximum_length = 0; s.buffer = core::ptr::null_mut();
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_free_ansi_string(string: *mut AnsiString) {
    if string.is_null() { return; }
    let s = unsafe { &mut *string };
    if !s.buffer.is_null() { let _ = super::pool::deallocate(s.buffer); }
    s.length = 0; s.maximum_length = 0; s.buffer = core::ptr::null_mut();
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_free_oem_string(string: *mut AnsiString) {
    unsafe { rtl_free_ansi_string(string); }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_free_utf8_string(string: *mut AnsiString) {
    unsafe { rtl_free_ansi_string(string); }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_get_enabled_extended_features(mask: u64) -> u64 {
    // No extended-state feature is advertised until the scheduler and context
    // switcher explicitly save/restore it. Returning zero is safer than claiming support.
    let _ = mask;
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_get_product_info(
    major: u32, minor: u32, service_pack_major: u32, service_pack_minor: u32,
    product_type: *mut u32
) -> u8 {
    if product_type.is_null() { return 0; }
    if major != 10 || minor != 0 || service_pack_major != 0 || service_pack_minor != 0 { return 0; }
    // Windows 11 Pro-compatible product type; RdotOS currently reports a single SKU.
    unsafe { *product_type = 0x00000030; }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_query_elevation_flags(flags: *mut u32) -> i32 {
    if flags.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { *flags = 0; }
    STATUS_NOT_IMPLEMENTED
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_local_time_to_system_time(
    local_time: *const i64, system_time: *mut i64
) -> i32 {
    if local_time.is_null() || system_time.is_null() { return STATUS_INVALID_PARAMETER; }
    let value = unsafe { *local_time };
    match super::timezone::local_to_system(value) {
        Some(converted) => { unsafe { *system_time = converted; } STATUS_SUCCESS }
        None => STATUS_INVALID_PARAMETER,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_system_time_to_local_time(
    system_time: *const i64, local_time: *mut i64
) -> i32 {
    if system_time.is_null() || local_time.is_null() { return STATUS_INVALID_PARAMETER; }
    let value = unsafe { *system_time };
    match super::timezone::system_to_local(value) {
        Some(converted) => { unsafe { *local_time = converted; } STATUS_SUCCESS }
        None => STATUS_INVALID_PARAMETER,
    }
}


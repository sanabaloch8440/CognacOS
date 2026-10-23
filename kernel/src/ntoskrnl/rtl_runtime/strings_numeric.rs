//! Numeric conversion and counted-string RTL entry points with Windows x64 ABI.
use super::{AnsiString, UnicodeString};
use core::ffi::c_void;

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC0000023u32 as i32;
const STATUS_NOT_IMPLEMENTED: i32 = 0xC0000002u32 as i32;

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_unicode_string_to_integer(
    string: *const UnicodeString, base: u32, value: *mut u32
) -> i32 {
    if string.is_null() || value.is_null() { return STATUS_INVALID_PARAMETER; }
    let s = unsafe { &*string };
    if s.length == 0 || (s.length & 1) != 0 || s.buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let units = unsafe { core::slice::from_raw_parts(s.buffer, (s.length / 2) as usize) };
    let mut i = 0usize;
    while i < units.len() && (units[i] == 0x20 || (0x09..=0x0d).contains(&units[i])) { i += 1; }
    let mut negative = false;
    if i < units.len() && (units[i] == b'-' as u16 || units[i] == b'+' as u16) {
        negative = units[i] == b'-' as u16; i += 1;
    }
    let mut radix = base;
    if radix == 0 {
        radix = 10;
        if i + 1 < units.len() && units[i] == b'0' as u16 {
            match units[i + 1] {
                120 | 88 => { radix = 16; i += 2; }
                111 | 79 => { radix = 8; i += 2; }
                98 | 66 => { radix = 2; i += 2; }
                _ => {}
            }
        }
    }
    if !(2..=36).contains(&radix) { return STATUS_INVALID_PARAMETER; }
    let mut acc: u64 = 0;
    let mut digits = 0usize;
    while i < units.len() {
        let c = units[i];
        let d = match c {
            48..=57 => (c - 48) as u32,
            65..=90 => (c - 65 + 10) as u32,
            97..=122 => (c - 97 + 10) as u32,
            _ => break,
        };
        if d >= radix { break; }
        acc = match acc.checked_mul(radix as u64).and_then(|x| x.checked_add(d as u64)) {
            Some(v) => v, None => return STATUS_INVALID_PARAMETER
        };
        digits += 1; i += 1;
    }
    if digits == 0 { unsafe { *value = 0; } return STATUS_SUCCESS; }
    let signed = if negative { (0u64).wrapping_sub(acc) as u32 } else { acc as u32 };
    unsafe { *value = signed; }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_integer_to_unicode_string(
    value: u32, base: u32, string: *mut UnicodeString
) -> i32 {
    if string.is_null() { return STATUS_INVALID_PARAMETER; }
    if base != 0 && !(2..=36).contains(&base) { return STATUS_INVALID_PARAMETER; }
    let radix = if base == 0 { 10 } else { base };
    let dst = unsafe { &mut *string };
    if dst.buffer.is_null() || dst.maximum_length < 2 { return STATUS_INVALID_PARAMETER; }
    let mut n = value;
    let mut temp = [0u16; 32];
    let mut count = 0usize;
    loop {
        let d = (n % radix) as u8;
        temp[count] = if d < 10 { b'0' as u16 + d as u16 } else { b'A' as u16 + (d - 10) as u16 };
        count += 1; n /= radix;
        if n == 0 { break; }
    }
    let capacity = (dst.maximum_length as usize / 2).saturating_sub(1);
    if count > capacity { dst.length = 0; return STATUS_BUFFER_TOO_SMALL; }
    for j in 0..count { unsafe { *dst.buffer.add(j) = temp[count - j - 1]; } }
    unsafe { *dst.buffer.add(count) = 0; }
    dst.length = (count * 2) as u16;
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_ansi_string_to_unicode_size(string: *const AnsiString) -> u32 {
    if string.is_null() { return 0; }
    let s = unsafe { &*string };
    (s.length as u32).saturating_add(1).saturating_mul(2)
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_unicode_string_to_ansi_size(string: *const UnicodeString) -> u32 {
    if string.is_null() { return 0; }
    let s = unsafe { &*string };
    (s.length as u32 / 2).saturating_add(1)
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_upper_string(destination: *mut AnsiString, source: *const AnsiString) {
    if destination.is_null() || source.is_null() { return; }
    let d = unsafe { &mut *destination };
    let s = unsafe { &*source };
    if d.buffer.is_null() || (s.length != 0 && s.buffer.is_null()) { d.length = 0; return; }
    let n = core::cmp::min(s.length, d.maximum_length);
    for i in 0..n as usize {
        let c = unsafe { *s.buffer.add(i) };
        unsafe { *d.buffer.add(i) = c.to_ascii_uppercase(); }
    }
    d.length = n;
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_free_unicode_string(string: *mut UnicodeString) {
    if string.is_null() { return; }
    // RdotOS does not yet provide the Windows RTL pool allocator contract.
    // Only reset caller-visible metadata; never free memory with an unknown allocator.
    let s = unsafe { &mut *string };
    s.length = 0;
    s.maximum_length = 0;
    s.buffer = core::ptr::null_mut();
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_free_ansi_string(string: *mut AnsiString) {
    if string.is_null() { return; }
    let s = unsafe { &mut *string };
    s.length = 0;
    s.maximum_length = 0;
    s.buffer = core::ptr::null_mut();
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
    // No configured timezone bias exists yet; local time cannot be converted safely.
    STATUS_NOT_IMPLEMENTED
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_system_time_to_local_time(
    system_time: *const i64, local_time: *mut i64
) -> i32 {
    if system_time.is_null() || local_time.is_null() { return STATUS_INVALID_PARAMETER; }
    STATUS_NOT_IMPLEMENTED
}

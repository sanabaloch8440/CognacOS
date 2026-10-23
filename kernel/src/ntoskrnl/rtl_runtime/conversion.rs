//! Integer/string conversion routines from the Windows RTL contract.
use super::UnicodeString;

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
const STATUS_INVALID_PARAMETER_2: i32 = 0xC000_00F0u32 as i32;

fn radix_or_default(base: u32) -> Option<u32> {
    let b = if base == 0 { 10 } else { base };
    matches!(b, 2 | 8 | 10 | 16).then_some(b)
}

/// Parses a counted UTF-16 string into an unsigned 32-bit integer.
/// The input need not be NUL-terminated; Length is measured in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_unicode_string_to_integer(
    string: *const UnicodeString, base: u32, value: *mut u32,
) -> i32 {
    if string.is_null() || value.is_null() { return STATUS_INVALID_PARAMETER; }
    let s = unsafe { &*string };
    if s.length % 2 != 0 || (s.length != 0 && s.buffer.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }
    if s.length == 0 { return STATUS_INVALID_PARAMETER; }
    let units = (s.length / 2) as usize;
    let mut i = 0usize;
    while i < units && matches!(unsafe { *s.buffer.add(i) }, 0x20 | 0x09 | 0x0A | 0x0D) { i += 1; }
    let mut negative = false;
    if i < units {
        match unsafe { *s.buffer.add(i) } {
            0x2B => i += 1,
            0x2D => { negative = true; i += 1; },
            _ => {}
        }
    }
    let mut radix = base;
    if radix == 0 {
        if i + 1 < units && unsafe { *s.buffer.add(i) } == 0x30
            && matches!(unsafe { *s.buffer.add(i + 1) }, 0x78 | 0x58) {
            radix = 16; i += 2;
        } else if i < units && unsafe { *s.buffer.add(i) } == 0x30 {
            radix = 8;
        } else { radix = 10; }
    }
    let Some(radix) = radix_or_default(radix) else { return STATUS_INVALID_PARAMETER_2; };
    let mut result = 0u32;
    let mut digits = 0usize;
    while i < units {
        let c = unsafe { *s.buffer.add(i) };
        let digit = match c {
            0x30..=0x39 => (c - 0x30) as u32,
            0x41..=0x5A => (c - 0x41 + 10) as u32,
            0x61..=0x7A => (c - 0x61 + 10) as u32,
            _ => break,
        };
        if digit >= radix { break; }
        let Some(next) = result.checked_mul(radix).and_then(|v| v.checked_add(digit)) else {
            return 0xC000_0095u32 as i32;
        };
        result = next; digits += 1; i += 1;
    }
    while i < units && matches!(unsafe { *s.buffer.add(i) }, 0x20 | 0x09 | 0x0A | 0x0D) { i += 1; }
    if digits == 0 || i != units { return STATUS_INVALID_PARAMETER; }
    if negative { result = result.wrapping_neg(); }
    unsafe { *value = result; }
    STATUS_SUCCESS
}

/// Writes an unsigned integer to a caller-provided UNICODE_STRING buffer.
/// Does not allocate memory; on failure Length is left unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_integer_to_unicode_string(
    value: u32, base: u32, string: *mut UnicodeString,
) -> i32 {
    if string.is_null() { return STATUS_INVALID_PARAMETER; }
    let s = unsafe { &mut *string };
    if s.buffer.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(radix) = radix_or_default(base) else { return STATUS_INVALID_PARAMETER_2; };
    let mut n = value;
    let mut digits = [0u16; 32];
    let mut count = 0usize;
    loop {
        let d = (n % radix) as u8;
        digits[count] = if d < 10 { 0x30 + d as u16 } else { 0x41 + (d - 10) as u16 };
        count += 1;
        n /= radix;
        if n == 0 { break; }
    }
    let bytes = count * 2;
    if (s.maximum_length as usize) < bytes + 2 { return STATUS_BUFFER_TOO_SMALL; }
    for i in 0..count { unsafe { *s.buffer.add(i) = digits[count - i - 1]; } }
    unsafe { *s.buffer.add(count) = 0; }
    s.length = bytes as u16;
    STATUS_SUCCESS
}

/// Writes an integer to a caller-provided ANSI buffer (including its NUL).
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_integer_to_char(
    value: u32, base: u32, buffer_length: u32, buffer: *mut u8,
) -> i32 {
    if buffer.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(radix) = radix_or_default(base) else { return STATUS_INVALID_PARAMETER_2; };
    let mut n = value;
    let mut digits = [0u8; 32];
    let mut count = 0usize;
    loop {
        let d = (n % radix) as u8;
        digits[count] = if d < 10 { b'0' + d } else { b'A' + d - 10 };
        count += 1;
        n /= radix;
        if n == 0 { break; }
    }
    if (buffer_length as usize) < count + 1 { return STATUS_BUFFER_TOO_SMALL; }
    for i in 0..count { unsafe { *buffer.add(i) = digits[count - i - 1]; } }
    unsafe { *buffer.add(count) = 0; }
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integer_unicode_round_trip() {
        let mut chars = [0u16; 16];
        let mut out = UnicodeString { length: 0, maximum_length: 32, buffer: chars.as_mut_ptr() };
        assert_eq!(unsafe { rtl_integer_to_unicode_string(0xBEEF, 16, &mut out) }, STATUS_SUCCESS);
        assert_eq!(out.length, 8);
        let mut value = 0;
        assert_eq!(unsafe { rtl_unicode_string_to_integer(&out, 16, &mut value) }, STATUS_SUCCESS);
        assert_eq!(value, 0xBEEF);
    }
    #[test]
    fn integer_char_formats_and_checks_capacity() {
        let mut buf = [0u8; 8];
        assert_eq!(unsafe { rtl_integer_to_char(255, 16, buf.len() as u32, buf.as_mut_ptr()) }, STATUS_SUCCESS);
        assert_eq!(&buf[..3], b"FF\0");
        assert_eq!(unsafe { rtl_integer_to_char(255, 16, 2, buf.as_mut_ptr()) }, STATUS_BUFFER_TOO_SMALL);
    }
}

/// Returns the required UTF-16 byte count for an ANSI_STRING, including the terminator.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_ansi_string_to_unicode_size(string: *const super::AnsiString) -> u32 {
    if string.is_null() { return 0; }
    let s = unsafe { &*string };
    // Windows' documented size helper includes space for a trailing WCHAR NUL.
    (s.length as u32).saturating_add(1).saturating_mul(2)
}

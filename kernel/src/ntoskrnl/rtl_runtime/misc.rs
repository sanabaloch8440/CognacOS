use super::{AnsiString, UnicodeString, Luid};

/// Windows RTL's deterministic 15-bit linear-congruential pseudo-random routine.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_random(seed: *mut u32) -> u32 {
    if seed.is_null() { return 0; }
    let next = unsafe { *seed }.wrapping_mul(214013).wrapping_add(2531011);
    unsafe { *seed = next; }
    (next >> 16) & 0x7fff
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_random_ex(seed: *mut u32) -> u32 {
    if seed.is_null() { return 0; }
    const MODULUS: u64 = 0x7fff_ffff;
    let current = unsafe { *seed } as u64;
    let next = (current * 0x7fff_ffed + 0x7fff_ffc3) % MODULUS;
    unsafe { *seed = next as u32; }
    next as u32
}
/// Parse an unsigned integer from an ANSI string. Supports bases 2-36 and base autodetection.
/// Parse a NUL-terminated ANSI integer using the RTL base-selection rules.
/// Accepts leading/trailing ASCII whitespace and an optional sign. The output
/// is written only after the entire input has been validated.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_char_to_integer(string: *const u8, base: u32, value: *mut u32) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_INTEGER_OVERFLOW: i32 = 0xC000_0095u32 as i32;
    if string.is_null() || value.is_null() { return STATUS_INVALID_PARAMETER; }

    let mut i = 0usize;
    while matches!(unsafe { *string.add(i) }, b' ' | b'\\t' | b'\\r' | b'\\n') { i += 1; }
    let negative = match unsafe { *string.add(i) } {
        b'+' => { i += 1; false }
        b'-' => { i += 1; true }
        _ => false,
    };

    let mut radix = base;
    if radix == 0 {
        if unsafe { *string.add(i) } == b'0'
            && matches!(unsafe { *string.add(i + 1) }, b'x' | b'X') {
            radix = 16;
            i += 2;
        } else if unsafe { *string.add(i) } == b'0' {
            radix = 8;
        } else {
            radix = 10;
        }
    }
    if !(2..=36).contains(&radix) { return STATUS_INVALID_PARAMETER; }

    let mut result = 0u32;
    let mut digits = 0usize;
    loop {
        let c = unsafe { *string.add(i) };
        let digit = match c {
            b'0'..=b'9' => (c - b'0') as u32,
            b'a'..=b'z' => (c - b'a' + 10) as u32,
            b'A'..=b'Z' => (c - b'A' + 10) as u32,
            _ => break,
        };
        if digit >= radix { break; }
        let Some(next) = result.checked_mul(radix).and_then(|v| v.checked_add(digit)) else {
            return STATUS_INTEGER_OVERFLOW;
        };
        result = next;
        digits += 1;
        i += 1;
    }
    while matches!(unsafe { *string.add(i) }, b' ' | b'\\t' | b'\\r' | b'\\n') { i += 1; }
    if digits == 0 || unsafe { *string.add(i) } != 0 { return STATUS_INVALID_PARAMETER; }

    if negative { result = result.wrapping_neg(); }
    unsafe { *value = result; }
    0
}

#[cfg(test)]
mod char_to_integer_tests {
    use super::rtl_char_to_integer;

    fn parse(text: &[u8], base: u32) -> (i32, u32) {
        let mut value = 0xA5A5_A5A5;
        let status = unsafe { rtl_char_to_integer(text.as_ptr(), base, &mut value) };
        (status, value)
    }

    #[test]
    fn char_to_integer_supports_sign_whitespace_and_radix_prefix() {
        assert_eq!(parse(b"  -0x2A \\0", 0), (0, (-42i32) as u32));
        assert_eq!(parse(b"077\\0", 0), (0, 63));
        assert_eq!(parse(b"+101\\0", 2), (0, 5));
    }

    #[test]
    fn char_to_integer_rejects_invalid_input_without_changing_output() {
        assert_eq!(parse(b"12z\\0", 10), (0xC000_000Du32 as i32, 0xA5A5_A5A5));
        assert_eq!(parse(b"\\0", 10), (0xC000_000Du32 as i32, 0xA5A5_A5A5));
        assert_eq!(parse(b"4294967296\\0", 10), (0xC000_0095u32 as i32, 0xA5A5_A5A5));
    }
}

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
    let next = unsafe { *seed }.wrapping_mul(214013).wrapping_add(2531011);
    unsafe { *seed = next; }
    (next >> 16) & 0x7fff
}
/// Parse an unsigned integer from an ANSI string. Supports bases 2-36 and base autodetection.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_char_to_integer(string: *const u8, base: u32, value: *mut u32) -> i32 {
    if string.is_null() || value.is_null() { return 0; }
    let mut i = 0usize;
    while unsafe { *string.add(i) } == b' ' || unsafe { *string.add(i) } == b'\t' { i += 1; }
    let mut radix = base;
    if radix == 0 {
        if unsafe { *string.add(i) } == b'0' && (unsafe { *string.add(i+1) } == b'x' || unsafe { *string.add(i+1) } == b'X') { radix = 16; i += 2; }
        else if unsafe { *string.add(i) } == b'0' { radix = 8; }
        else { radix = 10; }
    }
    if !(2..=36).contains(&radix) { return 0; }
    let mut result = 0u32; let mut digits = 0usize;
    loop {
        let c = unsafe { *string.add(i) };
        let d = match c { b'0'..=b'9' => (c-b'0') as u32, b'a'..=b'z' => (c-b'a'+10) as u32, b'A'..=b'Z' => (c-b'A'+10) as u32, _ => break };
        if d >= radix { break; }
        let Some(next) = result.checked_mul(radix).and_then(|v| v.checked_add(d)) else { return 0; };
        result = next; digits += 1; i += 1;
    }
    if digits == 0 { return 0; }
    unsafe { *value = result; }
    1
}

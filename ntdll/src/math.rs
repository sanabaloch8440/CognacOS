//! Small C-runtime compatibility helpers required by the pinned Windows 11 x64 NTDLL inventory.
//!
//! This module is not yet linked into the native PE32+ bootstrap.

#![allow(non_snake_case)]

/// Mirrors Wine's NTDLL abs helper for ordinary signed integers.
///
/// C leaves abs(INT_MIN) undefined. Wrapping negation avoids introducing a
/// Rust debug-build panic for that same edge case; callers should not rely on
/// a defined mathematical result for INT_MIN.
#[no_mangle]
pub extern "C" fn abs(value: i32) -> i32 {
    if value >= 0 { value } else { value.wrapping_neg() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_returns_magnitude_for_representable_values() {
        assert_eq!(abs(0), 0);
        assert_eq!(abs(1), 1);
        assert_eq!(abs(-1), 1);
        assert_eq!(abs(i32::MAX), i32::MAX);
        assert_eq!(abs(-123456), 123456);
    }

    #[test]
    fn abs_minimum_integer_does_not_panic() {
        // C's abs(INT_MIN) is undefined; this port explicitly uses wrapping
        // negation to keep behavior deterministic on Rust debug and release.
        assert_eq!(abs(i32::MIN), i32::MIN);
    }


}

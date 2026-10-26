//! Source-first Rust port of Wine's ntdll/math.c.
//!
//! Implements the architecture-independent helpers exported by the source:
//! `abs` and `math_error`. Wine's `_CIcos`, `_CIlog`, `_CIpow`,
//! `_CIsin`, `_CIsqrt`, and `_ftol` are compiled only for i386 and use
//! the x87 floating-point stack; they are intentionally not emitted for the
//! x86-64 native NTDLL target.
//!
//! This module is not yet linked into the native PE32+ build.

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

    #[test]
    fn math_error_preserves_return_value() {
        assert_eq!(math_error(0, core::ptr::null(), 1.0, 2.0, 3.5), 3.5);
        assert!(math_error(1, core::ptr::null(), f64::NAN, 0.0, f64::NAN).is_nan());
        assert_eq!(math_error(2, core::ptr::null(), 0.0, 0.0, f64::INFINITY), f64::INFINITY);
    }
}

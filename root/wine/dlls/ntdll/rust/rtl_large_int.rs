//! Rust port of the x64-active conversion routines from Wine's ntdll/large_int.c.
//!
//! The legacy arithmetic exports in large_int.c are guarded by !WIN64 and are
//! therefore not part of CognacOS's x64 NTDLL surface. This module ports the
//! two conversion routines that are active on x64. It is source-first until
//! the freestanding Rust NTDLL build links Rust modules.

#![allow(non_snake_case)]

use core::ptr;

pub type ULONG = u32;
pub type USHORT = u16;
pub type WCHAR = u16;
pub type NTSTATUS = i32;

pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000D_u32 as i32;
pub const STATUS_ACCESS_VIOLATION: NTSTATUS = 0xC000_0005_u32 as i32;
pub const STATUS_BUFFER_OVERFLOW: NTSTATUS = 0x8000_0005_u32 as i32;

/// Windows x64 UNICODE_STRING layout.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UNICODE_STRING {
    pub Length: USHORT,
    pub MaximumLength: USHORT,
    pub Buffer: *mut WCHAR,
}

#[inline]
fn valid_base(base: ULONG) -> bool {
    matches!(base, 2 | 8 | 10 | 16)
}

#[inline]
fn digit_to_ascii(digit: u64) -> u8 {
    if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 }
}

/// Converts an unsigned 64-bit integer to an ANSI string.
///
/// A terminator is written only when length exceeds the digit count; an
/// exact-size buffer receives digits without a trailing NUL. Base zero is
/// treated as decimal, matching the in-tree C source's documented behavior.
#[no_mangle]
pub unsafe extern "system" fn RtlLargeIntegerToChar(
    value_ptr: *const u64,
    base: ULONG,
    length: ULONG,
    output: *mut u8,
) -> NTSTATUS {
    if value_ptr.is_null() { return STATUS_ACCESS_VIOLATION; }
    let base = if base == 0 { 10 } else { base };
    if !valid_base(base) { return STATUS_INVALID_PARAMETER; }

    // SAFETY: The caller supplies a readable u64 pointer.
    let mut value = unsafe { value_ptr.read() };
    let mut buffer = [0u8; 64];
    let mut cursor = buffer.len();
    loop {
        cursor -= 1;
        let digit = value % base as u64;
        value /= base as u64;
        buffer[cursor] = digit_to_ascii(digit);
        if value == 0 { break; }
    }

    let digits = &buffer[cursor..];
    let digit_count = digits.len();
    if digit_count > length as usize { return STATUS_BUFFER_OVERFLOW; }
    if output.is_null() { return STATUS_ACCESS_VIOLATION; }

    // SAFETY: The caller promises length writable bytes at output.
    unsafe {
        ptr::copy_nonoverlapping(digits.as_ptr(), output, digit_count);
        if digit_count < length as usize { output.add(digit_count).write(0); }
    }
    STATUS_SUCCESS
}

/// Converts an unsigned 64-bit integer to a NUL-terminated UTF-16 string.
///
/// Length is set to the required digit count in bytes before checking
/// destination capacity, matching the source behavior on buffer overflow.
#[no_mangle]
pub unsafe extern "system" fn RtlInt64ToUnicodeString(
    mut value: u64,
    base: ULONG,
    destination: *mut UNICODE_STRING,
) -> NTSTATUS {
    let base = if base == 0 { 10 } else { base };
    if !valid_base(base) { return STATUS_INVALID_PARAMETER; }
    if destination.is_null() { return STATUS_ACCESS_VIOLATION; }

    let mut buffer = [0u16; 64];
    let mut cursor = buffer.len();
    loop {
        cursor -= 1;
        let digit = value % base as u64;
        value /= base as u64;
        buffer[cursor] = digit_to_ascii(digit) as u16;
        if value == 0 { break; }
    }

    let digits = &buffer[cursor..];
    let required_bytes = digits.len() * core::mem::size_of::<WCHAR>();
    // SAFETY: A non-null destination must point to a writable descriptor.
    let descriptor = unsafe { &mut *destination };
    descriptor.Length = required_bytes as USHORT;

    if required_bytes + 2 > descriptor.MaximumLength { return STATUS_BUFFER_OVERFLOW; }
    if descriptor.Buffer.is_null() { return STATUS_ACCESS_VIOLATION; }

    // SAFETY: MaximumLength has room for the digits plus a UTF-16 NUL.
    unsafe {
        ptr::copy_nonoverlapping(digits.as_ptr(), descriptor.Buffer, digits.len());
        descriptor.Buffer.add(digits.len()).write(0);
    }
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_decimal_and_exact_length_behavior() {
        let value = 123456789u64;
        let mut output = [0xaau8; 10];
        let status = unsafe { RtlLargeIntegerToChar(&value, 10, output.len() as u32, output.as_mut_ptr()) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(&output[..9], b"123456789");
        assert_eq!(output[9], 0);

        let mut exact = [0u8; 9];
        let status = unsafe { RtlLargeIntegerToChar(&value, 10, exact.len() as u32, exact.as_mut_ptr()) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(&exact, b"123456789");
    }

    #[test]
    fn ansi_bases_and_invalid_parameters() {
        let value = 255u64;
        let mut output = [0u8; 17];
        for (base, expected) in [(2, b"11111111".as_slice()), (8, b"377".as_slice()), (10, b"255".as_slice()), (16, b"FF".as_slice())] {
            let status = unsafe { RtlLargeIntegerToChar(&value, base, output.len() as u32, output.as_mut_ptr()) };
            assert_eq!(status, STATUS_SUCCESS);
            assert_eq!(&output[..expected.len()], expected);
            assert_eq!(output[expected.len()], 0);
        }
        assert_eq!(unsafe { RtlLargeIntegerToChar(&value, 3, output.len() as u32, output.as_mut_ptr()) }, STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe { RtlLargeIntegerToChar(&value, 10, 2, output.as_mut_ptr()) }, STATUS_BUFFER_OVERFLOW);
    }

    #[test]
    fn unicode_conversion_sets_required_length_on_overflow() {
        let mut storage = [0xaaaa_u16; 4];
        let mut descriptor = UNICODE_STRING { Length: 0, MaximumLength: 6, Buffer: storage.as_mut_ptr() };
        let status = unsafe { RtlInt64ToUnicodeString(1234, 10, &mut descriptor) };
        assert_eq!(status, STATUS_BUFFER_OVERFLOW);
        assert_eq!(descriptor.Length, 8);

        descriptor.MaximumLength = 10;
        let status = unsafe { RtlInt64ToUnicodeString(1234, 10, &mut descriptor) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(descriptor.Length, 8);
        assert_eq!(&storage, &[b'1' as u16, b'2' as u16, b'3' as u16, b'4' as u16]);
    }

    #[test]
    fn unicode_hex_and_zero() {
        let mut storage = [0u16; 17];
        let mut descriptor = UNICODE_STRING { Length: 0, MaximumLength: (storage.len() * 2) as u16, Buffer: storage.as_mut_ptr() };
        assert_eq!(unsafe { RtlInt64ToUnicodeString(0, 0, &mut descriptor) }, STATUS_SUCCESS);
        assert_eq!(descriptor.Length, 2);
        assert_eq!(storage[0], b'0' as u16);
        assert_eq!(storage[1], 0);

        assert_eq!(unsafe { RtlInt64ToUnicodeString(0xfeed, 16, &mut descriptor) }, STATUS_SUCCESS);
        assert_eq!(descriptor.Length, 8);
        assert_eq!(&storage[..5], &[b'F' as u16, b'E' as u16, b'E' as u16, b'D' as u16, 0]);
    }
}

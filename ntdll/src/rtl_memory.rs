//! Rust port of CognacOS's shared NTDLL RTL memory routines.
//!
//! Source correspondence: root/wine/dlls/ntdll/rtl_memory.c.
//! This is a source-first ABI port; build integration is intentionally deferred.
//! These exports use the Windows x64 ABI and have no allocation, Wine server,
//! or Unix-companion dependencies.
//!
//! # Safety
//! As with the C originals, callers must provide valid buffers for the requested
//! lengths, except where a routine explicitly handles null/zero-length input.

#![allow(non_snake_case)]

use core::ptr;

pub type BYTE = u8;
pub type UCHAR = u8;
pub type ULONG = u32;
pub type BOOLEAN = u8;
pub type SIZE_T = usize;

/// Copies bytes forward. Overlapping ranges require RtlMoveMemory.
#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemory(
    destination: *mut u8,
    source: *const u8,
    length: SIZE_T,
) {
    let mut index: SIZE_T = 0;
    while index < length {
        // SAFETY: Valid, non-overlapping ranges are caller obligations.
        unsafe { destination.add(index).write(source.add(index).read()) };
        index += 1;
    }
}

/// Copies bytes with memmove semantics, including overlapping ranges.
#[no_mangle]
pub unsafe extern "system" fn RtlMoveMemory(
    destination: *mut u8,
    source: *const u8,
    length: SIZE_T,
) {
    if length == 0 || ptr::eq(destination.cast_const(), source) {
        return;
    }

    let destination_address = destination as usize;
    let source_address = source as usize;
    if destination_address < source_address {
        let mut index: SIZE_T = 0;
        while index < length {
            // SAFETY: Caller supplies valid ranges; forward copy is overlap-safe
            // when the destination starts before the source.
            unsafe { destination.add(index).write(source.add(index).read()) };
            index += 1;
        }
    } else if destination_address > source_address {
        let mut remaining = length;
        while remaining != 0 {
            remaining -= 1;
            // SAFETY: Caller supplies valid ranges; reverse copy preserves bytes
            // when the destination starts inside the source range.
            unsafe {
                destination
                    .add(remaining)
                    .write(source.add(remaining).read())
            };
        }
    }
}

/// Fills a byte range with the requested byte value.
#[no_mangle]
pub unsafe extern "system" fn RtlFillMemory(
    destination: *mut u8,
    length: SIZE_T,
    fill: BYTE,
) {
    let mut index: SIZE_T = 0;
    while index < length {
        // SAFETY: Destination must be writable for length bytes.
        unsafe { destination.add(index).write(fill) };
        index += 1;
    }
}

/// Zeroes a byte range using volatile writes, matching the C implementation.
#[no_mangle]
pub unsafe extern "system" fn RtlZeroMemory(destination: *mut u8, length: SIZE_T) {
    let mut index: SIZE_T = 0;
    while index < length {
        // SAFETY: Destination must be writable for length bytes. Volatile writes
        // prevent the compiler from eliding the zeroing operation.
        unsafe { destination.add(index).write_volatile(0) };
        index += 1;
    }
}

/// Returns the matching prefix length, not a Boolean equality result.
#[no_mangle]
pub unsafe extern "system" fn RtlCompareMemory(
    source1: *const u8,
    source2: *const u8,
    length: SIZE_T,
) -> SIZE_T {
    let mut index: SIZE_T = 0;
    while index < length {
        // SAFETY: Both buffers must be readable for length bytes.
        let (left, right) = unsafe {
            (source1.add(index).read(), source2.add(index).read())
        };
        if left != right {
            break;
        }
        index += 1;
    }
    index
}

/// Compares bytes with the repeating native representation of a 32-bit value.
/// CognacOS targets x86-64, so this matches the original little-endian behavior.
#[no_mangle]
pub unsafe extern "system" fn RtlCompareMemoryUlong(
    source: *mut core::ffi::c_void,
    length: SIZE_T,
    pattern: ULONG,
) -> ULONG {
    let bytes = source.cast::<u8>();
    let pattern_bytes = pattern.to_ne_bytes();
    let mut index: SIZE_T = 0;
    while index < length {
        // SAFETY: Source must be readable for length bytes.
        let actual = unsafe { bytes.add(index).read() };
        if actual != pattern_bytes[index & 3] {
            break;
        }
        index += 1;
    }
    // The Windows export returns ULONG even though the input length is SIZE_T.
    index as ULONG
}

/// Computes reflected CRC-32 using polynomial 0xEDB88320.
#[no_mangle]
pub unsafe extern "system" fn RtlComputeCrc32(
    initial: ULONG,
    buffer: *const UCHAR,
    length: ULONG,
) -> ULONG {
    // Preserve CognacOS's current C implementation's null-buffer behavior.
    if buffer.is_null() && length != 0 {
        return initial;
    }

    let mut crc = !initial;
    let mut index: ULONG = 0;
    while index < length {
        // SAFETY: A non-null buffer must be readable for length bytes.
        let byte = unsafe { buffer.add(index as usize).read() };
        crc ^= byte as ULONG;

        let mut bit = 0;
        while bit < 8 {
            crc = if (crc & 1) != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        index += 1;
    }
    !crc
}

/// Alias preserving the separate RtlCrc32 export.
#[no_mangle]
pub unsafe extern "system" fn RtlCrc32(
    initial: ULONG,
    buffer: *const UCHAR,
    length: ULONG,
) -> ULONG {
    // SAFETY: The caller contract is identical to RtlComputeCrc32.
    unsafe { RtlComputeCrc32(initial, buffer, length) }
}

/// Returns TRUE when every byte is zero. Zero length is true even for null;
/// null with nonzero length is false.
#[no_mangle]
pub unsafe extern "system" fn RtlIsZeroMemory(
    buffer: *mut core::ffi::c_void,
    length: SIZE_T,
) -> BOOLEAN {
    if length == 0 {
        return 1;
    }
    if buffer.is_null() {
        return 0;
    }

    let bytes = buffer.cast::<u8>();
    let mut index: SIZE_T = 0;
    while index < length {
        // SAFETY: Buffer must be readable for length bytes.
        if unsafe { bytes.add(index).read_volatile() } != 0 {
            return 0;
        }
        index += 1;
    }
    1
}


// C runtime memory entry points used by the Windows x64 NTDLL export surface.
// These are real byte-wise primitives, not STATUS_NOT_IMPLEMENTED placeholders.
#[no_mangle]
pub unsafe extern "C" fn memcmp(left: *const core::ffi::c_void, right: *const core::ffi::c_void, count: usize) -> i32 {
    let left = left.cast::<u8>();
    let right = right.cast::<u8>();
    for index in 0..count {
        let a = unsafe { core::ptr::read(left.add(index)) };
        let b = unsafe { core::ptr::read(right.add(index)) };
        if a != b { return a as i32 - b as i32; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn memcpy(destination: *mut core::ffi::c_void, source: *const core::ffi::c_void, count: usize) -> *mut core::ffi::c_void {
    unsafe { core::ptr::copy_nonoverlapping(source.cast::<u8>(), destination.cast::<u8>(), count); }
    destination
}

#[no_mangle]
pub unsafe extern "C" fn memmove(destination: *mut core::ffi::c_void, source: *const core::ffi::c_void, count: usize) -> *mut core::ffi::c_void {
    unsafe { core::ptr::copy(source.cast::<u8>(), destination.cast::<u8>(), count); }
    destination
}

#[no_mangle]
pub unsafe extern "C" fn memset(destination: *mut core::ffi::c_void, value: i32, count: usize) -> *mut core::ffi::c_void {
    unsafe { core::ptr::write_bytes(destination.cast::<u8>(), value as u8, count); }
    destination
}

#[no_mangle]
pub unsafe extern "C" fn strlen(string: *const core::ffi::c_char) -> usize {
    let mut length = 0usize;
    while unsafe { core::ptr::read(string.add(length)) } != 0 { length += 1; }
    length
}

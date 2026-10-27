//! Rust port of CognacOS's freestanding NTDLL string helpers.
//!
//! Source correspondence: root/wine/dlls/ntdll/cognac_rtl_string.c.
//! This preserves the current in-tree C subset and its explicitly limited
//! ASCII-only case folding. It is not yet a full Unicode-table implementation.
//! Build integration is deferred while source files are being ported one at a time.

#![allow(non_snake_case)]

pub use crate::nt_abi::types::{BOOLEAN, LONG, ULONG, ULONG_PTR, UCHAR, USHORT, WCHAR};
pub use crate::nt_abi::ntstatus::{
    STATUS_BUFFER_OVERFLOW, STATUS_BUFFER_TOO_SMALL, STATUS_INVALID_PARAMETER, STATUS_SUCCESS,
};

/// Compatibility names alias the canonical shared NT ABI layouts.
pub type COGNAC_ANSI_STRING = crate::nt_abi::object::ANSI_STRING;
pub type COGNAC_UNICODE_STRING = crate::nt_abi::object::UNICODE_STRING;
pub type PCOGNAC_ANSI_STRING = crate::nt_abi::object::PANSI_STRING;
pub type PCCOGNAC_ANSI_STRING = *const crate::nt_abi::object::ANSI_STRING;
pub type PCOGNAC_UNICODE_STRING = crate::nt_abi::object::PUNICODE_STRING;
pub type PCCOGNAC_UNICODE_STRING = *const crate::nt_abi::object::UNICODE_STRING;

/// ASCII-only fold matching the current C source's case-insensitive behavior.
#[inline]
fn fold_ansi(value: UCHAR, ignore_case: BOOLEAN) -> UCHAR {
    if ignore_case != 0 && (b'A'..=b'Z').contains(&value) {
        value + (b'a' - b'A')
    } else {
        value
    }
}

/// ASCII-only fold for UTF-16 code units; non-ASCII characters are unchanged.
#[inline]
fn fold_unicode(value: WCHAR, ignore_case: BOOLEAN) -> WCHAR {
    if ignore_case != 0 && (b'A' as WCHAR..=b'Z' as WCHAR).contains(&value) {
        value + (b'a' as WCHAR - b'A' as WCHAR)
    } else {
        value
    }
}

/// Initializes an ANSI_STRING view over a NUL-terminated source string.
///
/// A null source produces an empty descriptor. The source memory is not copied.
#[no_mangle]
pub unsafe extern "system" fn RtlInitAnsiString(
    destination: PCOGNAC_ANSI_STRING,
    source: *const u8,
) {
    if destination.is_null() {
        return;
    }
    // SAFETY: A non-null destination must point to a writable descriptor.
    let descriptor = unsafe { &mut *destination };
    if source.is_null() {
        descriptor.length = 0;
        descriptor.maximum_length = 0;
        descriptor.buffer = core::ptr::null_mut();
        return;
    }

    let mut length: usize = 0;
    while length < 0xfffe && unsafe { source.add(length).read() } != 0 {
        length += 1;
    }
    descriptor.length = length as USHORT;
    descriptor.maximum_length = (length + 1) as USHORT;
    descriptor.buffer = source.cast_mut();
}

/// Initializes a UNICODE_STRING view over a NUL-terminated UTF-16 string.
#[no_mangle]
pub unsafe extern "system" fn RtlInitUnicodeString(
    destination: PCOGNAC_UNICODE_STRING,
    source: *const WCHAR,
) {
    if destination.is_null() {
        return;
    }
    // SAFETY: A non-null destination must point to a writable descriptor.
    let descriptor = unsafe { &mut *destination };
    if source.is_null() {
        descriptor.length = 0;
        descriptor.maximum_length = 0;
        descriptor.buffer = core::ptr::null_mut();
        return;
    }

    let mut length: usize = 0;
    while length < 0x7ffe && unsafe { source.add(length).read() } != 0 {
        length += 1;
    }
    descriptor.length = (length * core::mem::size_of::<WCHAR>()) as USHORT;
    descriptor.maximum_length =
        descriptor.length + core::mem::size_of::<WCHAR>() as USHORT;
    descriptor.buffer = source.cast_mut();
}

/// Lexicographically compares two ANSI_STRING descriptors.
#[no_mangle]
pub unsafe extern "system" fn RtlCompareString(
    first: PCCOGNAC_ANSI_STRING,
    second: PCCOGNAC_ANSI_STRING,
    ignore_case: BOOLEAN,
) -> LONG {
    if first.is_null() || second.is_null() {
        return if first.is_null() {
            if second.is_null() { 0 } else { -1 }
        } else {
            1
        };
    }
    // SAFETY: Non-null pointers must reference readable descriptors.
    let (first, second) = unsafe { (&*first, &*second) };
    if (first.length != 0 && first.buffer.is_null())
        || (second.length != 0 && second.buffer.is_null())
    {
        return if first.length != 0 {
            1
        } else if second.length != 0 {
            -1
        } else {
            0
        };
    }

    let limit = core::cmp::min(first.length, second.length) as usize;
    for index in 0..limit {
        // SAFETY: Each descriptor's buffer must be readable for Length bytes.
        let (a, b) = unsafe {
            (
                fold_ansi(first.buffer.add(index).read(), ignore_case),
                fold_ansi(second.buffer.add(index).read(), ignore_case),
            )
        };
        if a != b {
            return a as LONG - b as LONG;
        }
    }
    first.length as LONG - second.length as LONG
}

/// Returns nonzero when both ANSI strings have equal lengths and contents.
#[no_mangle]
pub unsafe extern "system" fn RtlEqualString(
    first: PCCOGNAC_ANSI_STRING,
    second: PCCOGNAC_ANSI_STRING,
    ignore_case: BOOLEAN,
) -> BOOLEAN {
    if first.is_null() || second.is_null() {
        return 0;
    }
    // SAFETY: Non-null pointers must reference readable descriptors.
    if unsafe { (*first).length != (*second).length } {
        return 0;
    }
    (unsafe { RtlCompareString(first, second, ignore_case) == 0 }) as BOOLEAN
}

/// Lexicographically compares UTF-16 UNICODE_STRING descriptors.
#[no_mangle]
pub unsafe extern "system" fn RtlCompareUnicodeString(
    first: PCCOGNAC_UNICODE_STRING,
    second: PCCOGNAC_UNICODE_STRING,
    ignore_case: BOOLEAN,
) -> LONG {
    if first.is_null() || second.is_null() {
        return if first.is_null() {
            if second.is_null() { 0 } else { -1 }
        } else {
            1
        };
    }
    // SAFETY: Non-null pointers must reference readable descriptors.
    let (first, second) = unsafe { (&*first, &*second) };
    if (first.length & 1) != 0 || (second.length & 1) != 0 {
        return if (first.length & 1) != 0 { 1 } else { -1 };
    }
    if (first.length != 0 && first.buffer.is_null())
        || (second.length != 0 && second.buffer.is_null())
    {
        return if first.length != 0 {
            1
        } else if second.length != 0 {
            -1
        } else {
            0
        };
    }

    let first_count = (first.length / 2) as usize;
    let second_count = (second.length / 2) as usize;
    let limit = core::cmp::min(first_count, second_count);
    for index in 0..limit {
        // SAFETY: Buffers must be readable for their declared UTF-16 lengths.
        let (a, b) = unsafe {
            (
                fold_unicode(first.buffer.add(index).read(), ignore_case),
                fold_unicode(second.buffer.add(index).read(), ignore_case),
            )
        };
        if a != b {
            return a as LONG - b as LONG;
        }
    }
    first_count as LONG - second_count as LONG
}

/// Returns nonzero when both Unicode strings have equal lengths and contents.
#[no_mangle]
pub unsafe extern "system" fn RtlEqualUnicodeString(
    first: PCCOGNAC_UNICODE_STRING,
    second: PCCOGNAC_UNICODE_STRING,
    ignore_case: BOOLEAN,
) -> BOOLEAN {
    if first.is_null() || second.is_null() {
        return 0;
    }
    // SAFETY: Non-null pointers must reference readable descriptors.
    if unsafe { (*first).length != (*second).length } {
        return 0;
    }
    (unsafe { RtlCompareUnicodeString(first, second, ignore_case) == 0 }) as BOOLEAN
}

/// Copies a Unicode string, truncating to the destination capacity and NUL-terminating.
#[no_mangle]
pub unsafe extern "system" fn RtlCopyUnicodeString(
    destination: PCOGNAC_UNICODE_STRING,
    source: PCCOGNAC_UNICODE_STRING,
) {
    if destination.is_null() {
        return;
    }
    // SAFETY: Destination must point to a writable descriptor.
    let destination = unsafe { &mut *destination };
    destination.length = 0;
    if destination.buffer.is_null() || destination.maximum_length < 2 {
        return;
    }
    if source.is_null() {
        unsafe { destination.buffer.write(0) };
        return;
    }
    // SAFETY: Source must point to a readable descriptor.
    let source = unsafe { &*source };
    if source.buffer.is_null() || source.length == 0 {
        unsafe { destination.buffer.write(0) };
        return;
    }

    let mut count = (source.length / 2) as usize;
    count = core::cmp::min(
        count,
        (destination.maximum_length as usize / 2).saturating_sub(1),
    );

    let destination_address = destination.buffer as ULONG_PTR;
    let source_address = source.buffer as ULONG_PTR;
    if destination_address < source_address {
        for index in 0..count {
            // SAFETY: Caller provides readable source and writable destination ranges.
            unsafe { destination.buffer.add(index).write(source.buffer.add(index).read()) };
        }
    } else if destination_address > source_address {
        let mut index = count;
        while index != 0 {
            index -= 1;
            // SAFETY: Reverse copy handles overlapping ranges.
            unsafe { destination.buffer.add(index).write(source.buffer.add(index).read()) };
        }
    }
    // SAFETY: MaximumLength >= 2 and count is bounded to leave one WCHAR.
    unsafe { destination.buffer.add(count).write(0) };
    destination.length = (count * 2) as USHORT;
}

#[inline]
fn unicode_digit(value: WCHAR) -> i32 {
    match value {
        0x30..=0x39 => (value - 0x30) as i32,
        0x61..=0x7a => (value - 0x61 + 10) as i32,
        0x41..=0x5a => (value - 0x41 + 10) as i32,
        _ => -1,
    }
}

#[inline]
unsafe fn is_ascii_space(value: WCHAR) -> bool {
    matches!(value, 0x20 | 0x09 | 0x0d | 0x0a)
}

/// Parses a UTF-16 string into a 32-bit integer, preserving current C behavior.
#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeStringToInteger(
    string: PCCOGNAC_UNICODE_STRING,
    mut base: ULONG,
    output: *mut ULONG,
) -> LONG {
    if string.is_null() || output.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    // SAFETY: Non-null pointers must reference readable/writable descriptors.
    let string = unsafe { &*string };
    if (string.length & 1) != 0 || (string.length != 0 && string.buffer.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }

    let count = (string.length / 2) as usize;
    let mut index = 0usize;
    let mut value: ULONG = 0;
    let mut sign: i32 = 1;
    let mut found = false;

    while index < count && unsafe { is_ascii_space(string.buffer.add(index).read()) } {
        index += 1;
    }
    if index < count {
        let ch = unsafe { string.buffer.add(index).read() };
        if ch == b'+' as WCHAR || ch == b'-' as WCHAR {
            if ch == b'-' as WCHAR {
                sign = -1;
            }
            index += 1;
        }
    }
    if base == 0 {
        base = 10;
    }
    if base == 16 && index + 1 < count {
        let first = unsafe { string.buffer.add(index).read() };
        let second = unsafe { string.buffer.add(index + 1).read() };
        if first == b'0' as WCHAR && (second == b'x' as WCHAR || second == b'X' as WCHAR) {
            index += 2;
        }
    }
    if !(2..=36).contains(&base) {
        return STATUS_INVALID_PARAMETER;
    }

    while index < count {
        let ch = unsafe { string.buffer.add(index).read() };
        let digit = unicode_digit(ch);
        if digit < 0 {
            if unsafe { is_ascii_space(ch) } {
                while index < count
                    && unsafe { is_ascii_space(string.buffer.add(index).read()) }
                {
                    index += 1;
                }
                if index == count {
                    break;
                }
            }
            return STATUS_INVALID_PARAMETER;
        }
        let digit = digit as ULONG;
        if digit >= base || value > (ULONG::MAX - digit) / base {
            return STATUS_INVALID_PARAMETER;
        }
        value = value * base + digit;
        found = true;
        index += 1;
    }

    if !found {
        return STATUS_INVALID_PARAMETER;
    }
    // SAFETY: output was checked non-null and must be writable.
    unsafe { output.write(if sign < 0 { 0u32.wrapping_sub(value) } else { value }) };
    STATUS_SUCCESS
}

/// Formats an unsigned integer as UTF-16 digits in base 2, 8, 10, or 16.
#[no_mangle]
pub unsafe extern "system" fn RtlIntegerToUnicodeString(
    mut value: ULONG,
    mut base: ULONG,
    destination: PCOGNAC_UNICODE_STRING,
) -> LONG {
    if destination.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    // SAFETY: Destination must point to a writable descriptor.
    let destination = unsafe { &mut *destination };
    if destination.buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    destination.length = 0;
    if base == 0 {
        base = 10;
    }
    if base != 2 && base != 8 && base != 10 && base != 16 {
        return STATUS_INVALID_PARAMETER;
    }

    let mut reversed = [0u16; 32];
    let mut count = 0usize;
    loop {
        let digit = value % base;
        reversed[count] = if digit < 10 {
            b'0' as WCHAR + digit as WCHAR
        } else {
            b'A' as WCHAR + (digit - 10) as WCHAR
        };
        count += 1;
        value /= base;
        if value == 0 || count == reversed.len() {
            break;
        }
    }

    if (count + 1) * 2 > destination.maximum_length as usize {
        return STATUS_BUFFER_TOO_SMALL;
    }
    for index in 0..count {
        // SAFETY: Capacity was checked above; buffer must be writable.
        unsafe { destination.buffer.add(index).write(reversed[count - index - 1]) };
    }
    // SAFETY: Capacity check reserves the terminating WCHAR.
    unsafe { destination.buffer.add(count).write(0) };
    destination.length = (count * 2) as USHORT;
    STATUS_SUCCESS
}


pub const STATUS_NAME_TOO_LONG: LONG = 0xC000_0106_u32 as LONG;
pub const STATUS_BUFFER_OVERFLOW: LONG = 0x8000_0005_u32 as LONG;

/// Checked ANSI descriptor initialization; leaves the descriptor untouched on overflow.
#[no_mangle]
pub unsafe extern "system" fn RtlInitAnsiStringEx(
    destination: PCOGNAC_ANSI_STRING,
    source: *const u8,
) -> LONG {
    if destination.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if source.is_null() {
        unsafe {
            (*destination).length = 0;
            (*destination).maximum_length = 0;
            (*destination).buffer = core::ptr::null_mut();
        }
        return STATUS_SUCCESS;
    }
    let mut length = 0usize;
    while length < 0xffff && unsafe { source.add(length).read() } != 0 {
        length += 1;
    }
    if length + 1 > USHORT::MAX as usize {
        return STATUS_NAME_TOO_LONG;
    }
    unsafe {
        (*destination).length = length as USHORT;
        (*destination).maximum_length = (length + 1) as USHORT;
        (*destination).buffer = source.cast_mut();
    }
    STATUS_SUCCESS
}

/// Checked UTF-16 descriptor initialization, returning STATUS_NAME_TOO_LONG rather than truncating.
#[no_mangle]
pub unsafe extern "system" fn RtlInitUnicodeStringEx(
    destination: PCOGNAC_UNICODE_STRING,
    source: *const WCHAR,
) -> LONG {
    if destination.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if source.is_null() {
        unsafe {
            (*destination).length = 0;
            (*destination).maximum_length = 0;
            (*destination).buffer = core::ptr::null_mut();
        }
        return STATUS_SUCCESS;
    }
    let mut count = 0usize;
    while count <= 0x7fff && unsafe { source.add(count).read() } != 0 {
        count += 1;
    }
    let length = count * 2;
    if length > 0xfffc {
        return STATUS_NAME_TOO_LONG;
    }
    unsafe {
        (*destination).length = length as USHORT;
        (*destination).maximum_length = (length + 2) as USHORT;
        (*destination).buffer = source.cast_mut();
    }
    STATUS_SUCCESS
}

/// Copies the source byte string into the destination up to MaximumLength bytes.
#[no_mangle]
pub unsafe extern "system" fn RtlCopyString(
    destination: PCOGNAC_ANSI_STRING,
    source: PCCOGNAC_ANSI_STRING,
) {
    if destination.is_null() {
        return;
    }
    let dst = unsafe { &mut *destination };
    let Some(src) = (if source.is_null() { None } else { Some(unsafe { &*source }) }) else {
        dst.length = 0;
        return;
    };
    if dst.buffer.is_null() || src.buffer.is_null() {
        dst.length = 0;
        return;
    }
    let count = core::cmp::min(src.length, dst.maximum_length) as usize;
    unsafe { core::ptr::copy(src.buffer, dst.buffer, count) };
    dst.length = count as USHORT;
}

/// Tests whether the first ANSI string is a prefix of the second.
#[no_mangle]
pub unsafe extern "system" fn RtlPrefixString(
    prefix: PCCOGNAC_ANSI_STRING,
    value: PCCOGNAC_ANSI_STRING,
    ignore_case: BOOLEAN,
) -> BOOLEAN {
    if prefix.is_null() || value.is_null() {
        return 0;
    }
    let (prefix, value) = unsafe { (&*prefix, &*value) };
    if prefix.length > value.length {
        return 0;
    }
    if prefix.length == 0 {
        return 1;
    }
    if prefix.buffer.is_null() || value.buffer.is_null() {
        return 0;
    }
    for i in 0..prefix.length as usize {
        let (a, b) = unsafe { (prefix.buffer.add(i).read(), value.buffer.add(i).read()) };
        if fold_ansi(a, ignore_case) != fold_ansi(b, ignore_case) {
            return 0;
        }
    }
    1
}

/// Appends a NUL-terminated ASCII string to an ANSI_STRING without exceeding its capacity.
#[no_mangle]
pub unsafe extern "system" fn RtlAppendAsciizToString(
    destination: PCOGNAC_ANSI_STRING,
    source: *const u8,
) -> LONG {
    if destination.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if source.is_null() {
        return STATUS_SUCCESS;
    }
    let dst = unsafe { &mut *destination };
    if dst.length > dst.maximum_length || (dst.length != 0 && dst.buffer.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }
    let mut count = 0usize;
    while unsafe { source.add(count).read() } != 0 {
        count += 1;
    }
    if count > (dst.maximum_length - dst.length) as usize {
        return STATUS_BUFFER_OVERFLOW;
    }
    if count != 0 {
        if dst.buffer.is_null() { return STATUS_INVALID_PARAMETER; }
        unsafe { core::ptr::copy(source, dst.buffer.add(dst.length as usize), count) };
    }
    dst.length += count as USHORT;
    STATUS_SUCCESS
}

/// Appends an ANSI_STRING to another ANSI_STRING.
#[no_mangle]
pub unsafe extern "system" fn RtlAppendStringToString(
    destination: PCOGNAC_ANSI_STRING,
    source: PCCOGNAC_ANSI_STRING,
) -> LONG {
    if destination.is_null() || source.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let dst = unsafe { &mut *destination };
    let src = unsafe { &*source };
    if dst.length > dst.maximum_length || src.length > 0 && src.buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if src.length > dst.maximum_length - dst.length {
        return STATUS_BUFFER_OVERFLOW;
    }
    if src.length != 0 {
        if dst.buffer.is_null() { return STATUS_INVALID_PARAMETER; }
        unsafe {
            core::ptr::copy(src.buffer, dst.buffer.add(dst.length as usize), src.length as usize);
        }
    }
    dst.length += src.length;
    STATUS_SUCCESS
}

/// Appends a counted UTF-16 string to another UNICODE_STRING.
#[no_mangle]
pub unsafe extern "system" fn RtlAppendUnicodeStringToString(
    destination: PCOGNAC_UNICODE_STRING,
    source: PCCOGNAC_UNICODE_STRING,
) -> LONG {
    if destination.is_null() || source.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let dst = unsafe { &mut *destination };
    let src = unsafe { &*source };
    if (dst.length | src.length) & 1 != 0
        || dst.length > dst.maximum_length
        || (src.length != 0 && src.buffer.is_null())
    {
        return STATUS_INVALID_PARAMETER;
    }
    if src.length > dst.maximum_length - dst.length {
        return STATUS_BUFFER_OVERFLOW;
    }
    if src.length != 0 {
        if dst.buffer.is_null() { return STATUS_INVALID_PARAMETER; }
        unsafe {
            core::ptr::copy(
                src.buffer,
                dst.buffer.add((dst.length / 2) as usize),
                (src.length / 2) as usize,
            );
        }
    }
    dst.length += src.length;
    STATUS_SUCCESS
}

/// Appends a NUL-terminated UTF-16 string to a UNICODE_STRING.
#[no_mangle]
pub unsafe extern "system" fn RtlAppendUnicodeToString(
    destination: PCOGNAC_UNICODE_STRING,
    source: *const WCHAR,
) -> LONG {
    if destination.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if source.is_null() {
        return STATUS_SUCCESS;
    }
    let dst = unsafe { &mut *destination };
    if dst.length & 1 != 0 || dst.length > dst.maximum_length {
        return STATUS_INVALID_PARAMETER;
    }
    let mut count = 0usize;
    while unsafe { source.add(count).read() } != 0 {
        count += 1;
    }
    let bytes = count.saturating_mul(2);
    if bytes > (dst.maximum_length - dst.length) as usize {
        return STATUS_BUFFER_OVERFLOW;
    }
    if count != 0 {
        if dst.buffer.is_null() { return STATUS_INVALID_PARAMETER; }
        unsafe {
            core::ptr::copy(source, dst.buffer.add((dst.length / 2) as usize), count);
        }
    }
    dst.length += bytes as USHORT;
    STATUS_SUCCESS
}

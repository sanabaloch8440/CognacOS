//! Rust port of CognacOS's freestanding NTDLL string helpers.
//!
//! Source correspondence: root/wine/dlls/ntdll/cognac_rtl_string.c.
//! This preserves the current in-tree C subset and its explicitly limited
//! ASCII-only case folding. It is not yet a full Unicode-table implementation.
//! Build integration is deferred while source files are being ported one at a time.

#![allow(non_snake_case)]

pub type UCHAR = u8;
pub type USHORT = u16;
pub type WCHAR = u16;
pub type ULONG = u32;
pub type LONG = i32;
pub type ULONG_PTR = usize;
pub type BOOLEAN = u8;

pub const STATUS_SUCCESS: LONG = 0x0000_0000;
pub const STATUS_INVALID_PARAMETER: LONG = 0xC000_000D_u32 as LONG;
pub const STATUS_BUFFER_TOO_SMALL: LONG = 0xC000_0023_u32 as LONG;

/// ABI-compatible ANSI_STRING layout for the x64 CognacOS target.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct COGNAC_ANSI_STRING {
    pub Length: USHORT,
    pub MaximumLength: USHORT,
    pub Buffer: *mut u8,
}

/// ABI-compatible UNICODE_STRING layout for the x64 CognacOS target.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct COGNAC_UNICODE_STRING {
    pub Length: USHORT,
    pub MaximumLength: USHORT,
    pub Buffer: *mut WCHAR,
}

pub type PCOGNAC_ANSI_STRING = *mut COGNAC_ANSI_STRING;
pub type PCCOGNAC_ANSI_STRING = *const COGNAC_ANSI_STRING;
pub type PCOGNAC_UNICODE_STRING = *mut COGNAC_UNICODE_STRING;
pub type PCCOGNAC_UNICODE_STRING = *const COGNAC_UNICODE_STRING;

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
        descriptor.Length = 0;
        descriptor.MaximumLength = 0;
        descriptor.Buffer = core::ptr::null_mut();
        return;
    }

    let mut length: usize = 0;
    while length < 0xfffe && unsafe { source.add(length).read() } != 0 {
        length += 1;
    }
    descriptor.Length = length as USHORT;
    descriptor.MaximumLength = (length + 1) as USHORT;
    descriptor.Buffer = source.cast_mut();
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
        descriptor.Length = 0;
        descriptor.MaximumLength = 0;
        descriptor.Buffer = core::ptr::null_mut();
        return;
    }

    let mut length: usize = 0;
    while length < 0x7ffe && unsafe { source.add(length).read() } != 0 {
        length += 1;
    }
    descriptor.Length = (length * core::mem::size_of::<WCHAR>()) as USHORT;
    descriptor.MaximumLength =
        descriptor.Length + core::mem::size_of::<WCHAR>() as USHORT;
    descriptor.Buffer = source.cast_mut();
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
    if (first.Length != 0 && first.Buffer.is_null())
        || (second.Length != 0 && second.Buffer.is_null())
    {
        return if first.Length != 0 {
            1
        } else if second.Length != 0 {
            -1
        } else {
            0
        };
    }

    let limit = core::cmp::min(first.Length, second.Length) as usize;
    for index in 0..limit {
        // SAFETY: Each descriptor's buffer must be readable for Length bytes.
        let (a, b) = unsafe {
            (
                fold_ansi(first.Buffer.add(index).read(), ignore_case),
                fold_ansi(second.Buffer.add(index).read(), ignore_case),
            )
        };
        if a != b {
            return a as LONG - b as LONG;
        }
    }
    first.Length as LONG - second.Length as LONG
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
    if unsafe { (*first).Length != (*second).Length } {
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
    if (first.Length & 1) != 0 || (second.Length & 1) != 0 {
        return if (first.Length & 1) != 0 { 1 } else { -1 };
    }
    if (first.Length != 0 && first.Buffer.is_null())
        || (second.Length != 0 && second.Buffer.is_null())
    {
        return if first.Length != 0 {
            1
        } else if second.Length != 0 {
            -1
        } else {
            0
        };
    }

    let first_count = (first.Length / 2) as usize;
    let second_count = (second.Length / 2) as usize;
    let limit = core::cmp::min(first_count, second_count);
    for index in 0..limit {
        // SAFETY: Buffers must be readable for their declared UTF-16 lengths.
        let (a, b) = unsafe {
            (
                fold_unicode(first.Buffer.add(index).read(), ignore_case),
                fold_unicode(second.Buffer.add(index).read(), ignore_case),
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
    if unsafe { (*first).Length != (*second).Length } {
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
    destination.Length = 0;
    if destination.Buffer.is_null() || destination.MaximumLength < 2 {
        return;
    }
    if source.is_null() {
        unsafe { destination.Buffer.write(0) };
        return;
    }
    // SAFETY: Source must point to a readable descriptor.
    let source = unsafe { &*source };
    if source.Buffer.is_null() || source.Length == 0 {
        unsafe { destination.Buffer.write(0) };
        return;
    }

    let mut count = (source.Length / 2) as usize;
    count = core::cmp::min(
        count,
        (destination.MaximumLength as usize / 2).saturating_sub(1),
    );

    let destination_address = destination.Buffer as ULONG_PTR;
    let source_address = source.Buffer as ULONG_PTR;
    if destination_address < source_address {
        for index in 0..count {
            // SAFETY: Caller provides readable source and writable destination ranges.
            unsafe { destination.Buffer.add(index).write(source.Buffer.add(index).read()) };
        }
    } else if destination_address > source_address {
        let mut index = count;
        while index != 0 {
            index -= 1;
            // SAFETY: Reverse copy handles overlapping ranges.
            unsafe { destination.Buffer.add(index).write(source.Buffer.add(index).read()) };
        }
    }
    // SAFETY: MaximumLength >= 2 and count is bounded to leave one WCHAR.
    unsafe { destination.Buffer.add(count).write(0) };
    destination.Length = (count * 2) as USHORT;
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
    if (string.Length & 1) != 0 || (string.Length != 0 && string.Buffer.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }

    let count = (string.Length / 2) as usize;
    let mut index = 0usize;
    let mut value: ULONG = 0;
    let mut sign: i32 = 1;
    let mut found = false;

    while index < count && unsafe { is_ascii_space(string.Buffer.add(index).read()) } {
        index += 1;
    }
    if index < count {
        let ch = unsafe { string.Buffer.add(index).read() };
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
        let first = unsafe { string.Buffer.add(index).read() };
        let second = unsafe { string.Buffer.add(index + 1).read() };
        if first == b'0' as WCHAR && (second == b'x' as WCHAR || second == b'X' as WCHAR) {
            index += 2;
        }
    }
    if !(2..=36).contains(&base) {
        return STATUS_INVALID_PARAMETER;
    }

    while index < count {
        let ch = unsafe { string.Buffer.add(index).read() };
        let digit = unicode_digit(ch);
        if digit < 0 {
            if unsafe { is_ascii_space(ch) } {
                while index < count
                    && unsafe { is_ascii_space(string.Buffer.add(index).read()) }
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
    if destination.Buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    destination.Length = 0;
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

    if (count + 1) * 2 > destination.MaximumLength as usize {
        return STATUS_BUFFER_TOO_SMALL;
    }
    for index in 0..count {
        // SAFETY: Capacity was checked above; buffer must be writable.
        unsafe { destination.Buffer.add(index).write(reversed[count - index - 1]) };
    }
    // SAFETY: Capacity check reserves the terminating WCHAR.
    unsafe { destination.Buffer.add(count).write(0) };
    destination.Length = (count * 2) as USHORT;
    STATUS_SUCCESS
}

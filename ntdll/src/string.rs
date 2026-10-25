//! Source-first Rust port of foundational routines from Wine's ntdll/string.c.
//!
//! This module intentionally does not emit unmangled C symbols yet: the imported
//! Wine C implementation remains in the tree, and native build/export integration
//! must choose one implementation before these routines can replace it.
//!
//! Raw pointers follow the Windows CRT contract: non-null pointers must refer to
//! valid readable/writable buffers of the lengths implied by each call.

#![allow(non_snake_case, non_camel_case_types)]

use core::ffi::c_void;

pub type SIZE_T = usize;
pub type ULONG = u32;
pub type errno_t = i32;

pub const EINVAL: errno_t = 22;
pub const ERANGE: errno_t = 34;
pub const STRUNCATE: errno_t = 80;
pub const TRUNCATE: usize = usize::MAX;

#[inline]
unsafe fn byte(ptr: *const u8, index: usize) -> u8 {
    // SAFETY: caller guarantees the addressed byte is readable.
    unsafe { *ptr.add(index) }
}

/// Returns the first matching byte, or null.
pub unsafe extern "system" fn memchr(ptr: *const c_void, c: i32, n: SIZE_T) -> *mut c_void {
    if n == 0 || ptr.is_null() { return core::ptr::null_mut(); }
    let p = ptr.cast::<u8>();
    for i in 0..n {
        if unsafe { byte(p, i) } == c as u8 {
            return unsafe { p.add(i) as *mut c_void };
        }
    }
    core::ptr::null_mut()
}

/// Compares byte sequences lexicographically and returns -1, 0, or 1.
pub unsafe extern "system" fn memcmp(a: *const c_void, b: *const c_void, n: SIZE_T) -> i32 {
    if n == 0 { return 0; }
    if a.is_null() || b.is_null() { return 0; }
    let (a, b) = (a.cast::<u8>(), b.cast::<u8>());
    for i in 0..n {
        let (x, y) = (unsafe { byte(a, i) }, unsafe { byte(b, i) });
        if x < y { return -1; }
        if x > y { return 1; }
    }
    0
}

/// Copies bytes with memmove semantics (overlapping regions are supported).
pub unsafe extern "system" fn memcpy(dst: *mut c_void, src: *const c_void, n: SIZE_T) -> *mut c_void {
    if n == 0 { return dst; }
    if dst.is_null() || src.is_null() { return dst; }
    // SAFETY: caller supplies valid buffers; copy permits overlap.
    unsafe { core::ptr::copy(src.cast::<u8>(), dst.cast::<u8>(), n); }
    dst
}

pub unsafe extern "system" fn memmove(dst: *mut c_void, src: *const c_void, n: SIZE_T) -> *mut c_void {
    unsafe { memcpy(dst, src, n) }
}

pub unsafe extern "system" fn memset(dst: *mut c_void, c: i32, n: SIZE_T) -> *mut c_void {
    if n == 0 || dst.is_null() { return dst; }
    // SAFETY: caller guarantees dst is writable for n bytes.
    unsafe { core::ptr::write_bytes(dst.cast::<u8>(), c as u8, n); }
    dst
}

pub unsafe extern "system" fn memcpy_s(dst: *mut c_void, len: SIZE_T, src: *const c_void, count: SIZE_T) -> errno_t {
    if count == 0 { return 0; }
    if dst.is_null() { return EINVAL; }
    if src.is_null() {
        unsafe { memset(dst, 0, len); }
        return EINVAL;
    }
    if count > len {
        unsafe { memset(dst, 0, len); }
        return ERANGE;
    }
    unsafe { memmove(dst, src, count); }
    0
}

pub unsafe extern "system" fn memmove_s(dst: *mut c_void, len: SIZE_T, src: *const c_void, count: SIZE_T) -> errno_t {
    if count == 0 { return 0; }
    if dst.is_null() || src.is_null() { return EINVAL; }
    if count > len { return ERANGE; }
    unsafe { memmove(dst, src, count); }
    0
}

pub unsafe extern "system" fn RtlCompareMemoryUlong(src: *const c_void, len: SIZE_T, value: ULONG) -> SIZE_T {
    if src.is_null() { return 0; }
    let words = len / core::mem::size_of::<ULONG>();
    let p = src.cast::<ULONG>();
    let mut i = 0;
    while i < words && unsafe { *p.add(i) } == value { i += 1; }
    i * core::mem::size_of::<ULONG>()
}

pub unsafe extern "system" fn RtlFillMemoryUlong(dst: *mut ULONG, len: ULONG, value: ULONG) {
    if dst.is_null() { return; }
    for i in 0..(len as usize / core::mem::size_of::<ULONG>()) {
        unsafe { dst.add(i).write(value); }
    }
}

unsafe fn c_strlen(s: *const u8) -> usize {
    if s.is_null() { return 0; }
    let mut n = 0;
    while unsafe { byte(s, n) } != 0 { n += 1; }
    n
}

pub unsafe extern "system" fn strlen(s: *const i8) -> SIZE_T {
    unsafe { c_strlen(s.cast()) }
}

pub unsafe extern "system" fn strnlen(s: *const i8, max: SIZE_T) -> SIZE_T {
    if s.is_null() { return 0; }
    let mut n = 0;
    while n < max && unsafe { byte(s.cast(), n) } != 0 { n += 1; }
    n
}

pub unsafe extern "system" fn strcmp(a: *const i8, b: *const i8) -> i32 {
    if a.is_null() || b.is_null() { return 0; }
    let (a, b) = (a.cast::<u8>(), b.cast::<u8>());
    let mut i = 0;
    loop {
        let (x, y) = (unsafe { byte(a, i) }, unsafe { byte(b, i) });
        if x != y { return if x < y { -1 } else { 1 }; }
        if x == 0 { return 0; }
        i += 1;
    }
}

pub unsafe extern "system" fn strncmp(a: *const i8, b: *const i8, n: SIZE_T) -> i32 {
    if n == 0 { return 0; }
    if a.is_null() || b.is_null() { return 0; }
    let (a, b) = (a.cast::<u8>(), b.cast::<u8>());
    for i in 0..n {
        let (x, y) = (unsafe { byte(a, i) }, unsafe { byte(b, i) });
        if x != y { return x as i32 - y as i32; }
        if x == 0 { return 0; }
    }
    0
}

pub unsafe extern "system" fn strcpy(dst: *mut i8, src: *const i8) -> *mut i8 {
    if dst.is_null() || src.is_null() { return dst; }
    let mut i = 0;
    loop {
        let v = unsafe { byte(src.cast(), i) };
        unsafe { dst.cast::<u8>().add(i).write(v); }
        if v == 0 { break; }
        i += 1;
    }
    dst
}

pub unsafe extern "system" fn strncpy(dst: *mut i8, src: *const i8, n: SIZE_T) -> *mut i8 {
    if n == 0 { return dst; }
    if dst.is_null() || src.is_null() { return dst; }
    let mut i = 0;
    while i < n {
        let v = unsafe { byte(src.cast(), i) };
        unsafe { dst.cast::<u8>().add(i).write(v); }
        i += 1;
        if v == 0 {
            while i < n { unsafe { dst.cast::<u8>().add(i).write(0); } i += 1; }
            return dst;
        }
    }
    dst
}

pub unsafe extern "system" fn strcat(dst: *mut i8, src: *const i8) -> *mut i8 {
    if dst.is_null() || src.is_null() { return dst; }
    let mut d = unsafe { c_strlen(dst.cast()) };
    let mut s = 0;
    loop {
        let v = unsafe { byte(src.cast(), s) };
        unsafe { dst.cast::<u8>().add(d).write(v); }
        if v == 0 { break; }
        d += 1; s += 1;
    }
    dst
}

pub unsafe extern "system" fn strncat(dst: *mut i8, src: *const i8, n: SIZE_T) -> *mut i8 {
    if dst.is_null() || src.is_null() { return dst; }
    let mut d = unsafe { c_strlen(dst.cast()) };
    let mut s = 0;
    while s < n {
        let v = unsafe { byte(src.cast(), s) };
        if v == 0 { break; }
        unsafe { dst.cast::<u8>().add(d).write(v); }
        d += 1; s += 1;
    }
    unsafe { dst.cast::<u8>().add(d).write(0); }
    dst
}

pub unsafe extern "system" fn strchr(s: *const i8, c: i32) -> *mut i8 {
    if s.is_null() { return core::ptr::null_mut(); }
    let s = s.cast::<u8>();
    let c = c as u8;
    let mut i = 0;
    loop {
        let v = unsafe { byte(s, i) };
        if v == c { return unsafe { s.add(i) as *mut i8 }; }
        if v == 0 { return core::ptr::null_mut(); }
        i += 1;
    }
}

pub unsafe extern "system" fn strrchr(s: *const i8, c: i32) -> *mut i8 {
    if s.is_null() { return core::ptr::null_mut(); }
    let s = s.cast::<u8>();
    let c = c as u8;
    let mut last = core::ptr::null_mut();
    let mut i = 0;
    loop {
        let v = unsafe { byte(s, i) };
        if v == c { last = unsafe { s.add(i) as *mut i8 }; }
        if v == 0 { return last; }
        i += 1;
    }
}

pub unsafe extern "system" fn strstr(haystack: *const i8, needle: *const i8) -> *mut i8 {
    if haystack.is_null() || needle.is_null() { return core::ptr::null_mut(); }
    let nlen = unsafe { c_strlen(needle.cast()) };
    if nlen == 0 { return haystack as *mut i8; }
    let hlen = unsafe { c_strlen(haystack.cast()) };
    if nlen > hlen { return core::ptr::null_mut(); }
    for i in 0..=(hlen - nlen) {
        let mut matched = true;
        for j in 0..nlen {
            if unsafe { byte(haystack.cast(), i + j) } != unsafe { byte(needle.cast(), j) } {
                matched = false; break;
            }
        }
        if matched { return unsafe { haystack.add(i) as *mut i8 }; }
    }
    core::ptr::null_mut()
}

pub unsafe extern "system" fn tolower(c: i32) -> i32 {
    if (b'A' as i32..=b'Z' as i32).contains(&c) { c + 32 } else { c }
}

pub unsafe extern "system" fn _strnicmp(a: *const i8, b: *const i8, n: SIZE_T) -> i32 {
    if n == 0 { return 0; }
    if a.is_null() || b.is_null() { return 0; }
    for i in 0..n {
        let x = unsafe { byte(a.cast(), i) };
        let y = unsafe { byte(b.cast(), i) };
        let lx = unsafe { tolower(x as i32) } as u8;
        let ly = unsafe { tolower(y as i32) } as u8;
        if lx != ly { return lx as i32 - ly as i32; }
        if lx == 0 { return 0; }
    }
    0
}

pub unsafe extern "system" fn _stricmp(a: *const i8, b: *const i8) -> i32 {
    unsafe { _strnicmp(a, b, usize::MAX) }
}


/// Secure concatenation matching the error/clearing behavior used by Wine's string.c.
pub unsafe extern "system" fn strcat_s(dst: *mut i8, len: SIZE_T, src: *const i8) -> errno_t {
    if dst.is_null() || len == 0 { return EINVAL; }
    if src.is_null() {
        unsafe { dst.write(0); }
        return EINVAL;
    }
    let mut dlen = 0;
    while dlen < len && unsafe { byte(dst.cast(), dlen) } != 0 { dlen += 1; }
    let mut i = 0;
    while dlen + i < len {
        let v = unsafe { byte(src.cast(), i) };
        unsafe { dst.add(dlen + i).write(v as i8); }
        if v == 0 { return 0; }
        i += 1;
    }
    unsafe { dst.write(0); }
    ERANGE
}

/// Secure copy: destination is always terminated/cleared on a range failure.
pub unsafe extern "system" fn strcpy_s(dst: *mut i8, len: SIZE_T, src: *const i8) -> errno_t {
    if dst.is_null() || len == 0 { return EINVAL; }
    if src.is_null() {
        unsafe { dst.write(0); }
        return EINVAL;
    }
    for i in 0..len {
        let v = unsafe { byte(src.cast(), i) };
        unsafe { dst.add(i).write(v as i8); }
        if v == 0 { return 0; }
    }
    unsafe { dst.write(0); }
    ERANGE
}

pub unsafe extern "system" fn strncpy_s(dst: *mut i8, len: SIZE_T, src: *const i8, count: SIZE_T) -> errno_t {
    if dst.is_null() || len == 0 { return EINVAL; }
    if count == 0 {
        unsafe { dst.write(0); }
        return 0;
    }
    if src.is_null() {
        unsafe { dst.write(0); }
        return EINVAL;
    }
    let limit = if count == TRUNCATE { len } else { core::cmp::min(count, len) };
    let mut i = 0;
    while i < limit {
        let v = unsafe { byte(src.cast(), i) };
        if v == 0 {
            unsafe { dst.add(i).write(0); }
            return 0;
        }
        unsafe { dst.add(i).write(v as i8); }
        i += 1;
    }
    if count == TRUNCATE {
        unsafe { dst.add(len - 1).write(0); }
        STRUNCATE
    } else if count >= len {
        unsafe { dst.write(0); }
        ERANGE
    } else {
        unsafe { dst.add(i).write(0); }
        0
    }
}

pub unsafe extern "system" fn strncat_s(dst: *mut i8, len: SIZE_T, src: *const i8, count: SIZE_T) -> errno_t {
    if dst.is_null() || len == 0 { return EINVAL; }
    if count == 0 { return 0; }
    if src.is_null() {
        unsafe { dst.write(0); }
        return EINVAL;
    }
    let mut dlen = 0;
    while dlen < len && unsafe { byte(dst.cast(), dlen) } != 0 { dlen += 1; }
    if dlen == len {
        unsafe { dst.write(0); }
        return ERANGE;
    }
    let limit = if count == TRUNCATE { len - dlen - 1 } else { count };
    let mut i = 0;
    while i < limit && dlen + i + 1 < len {
        let v = unsafe { byte(src.cast(), i) };
        if v == 0 {
            unsafe { dst.add(dlen + i).write(0); }
            return 0;
        }
        unsafe { dst.add(dlen + i).write(v as i8); }
        i += 1;
    }
    unsafe { dst.add(dlen + i).write(0); }
    if count == TRUNCATE {
        if unsafe { byte(src.cast(), i) } != 0 { STRUNCATE } else { 0 }
    } else if i < count && unsafe { byte(src.cast(), i) } != 0 {
        unsafe { dst.write(0); }
        ERANGE
    } else { 0 }
}

pub unsafe extern "system" fn strcspn(s: *const i8, reject: *const i8) -> SIZE_T {
    if s.is_null() || reject.is_null() { return 0; }
    let mut i = 0;
    while unsafe { byte(s.cast(), i) } != 0 {
        if !unsafe { strchr(reject, byte(s.cast(), i) as i32) }.is_null() { break; }
        i += 1;
    }
    i
}

pub unsafe extern "system" fn strspn(s: *const i8, accept: *const i8) -> SIZE_T {
    if s.is_null() || accept.is_null() { return 0; }
    let mut i = 0;
    while unsafe { byte(s.cast(), i) } != 0 {
        if unsafe { strchr(accept, byte(s.cast(), i) as i32) }.is_null() { break; }
        i += 1;
    }
    i
}

pub unsafe extern "system" fn strpbrk(s: *const i8, accept: *const i8) -> *mut i8 {
    if s.is_null() || accept.is_null() { return core::ptr::null_mut(); }
    let mut i = 0;
    while unsafe { byte(s.cast(), i) } != 0 {
        if !unsafe { strchr(accept, byte(s.cast(), i) as i32) }.is_null() {
            return unsafe { s.add(i) as *mut i8 };
        }
        i += 1;
    }
    core::ptr::null_mut()
}

pub unsafe extern "system" fn toupper(c: i32) -> i32 {
    if (b'a' as i32..=b'z' as i32).contains(&c) { c - 32 } else { c }
}

pub unsafe extern "system" fn _strlwr(s: *mut i8) -> *mut i8 {
    if s.is_null() { return s; }
    let mut i = 0;
    loop {
        let v = unsafe { byte(s.cast(), i) };
        if v == 0 { break; }
        unsafe { s.add(i).write(tolower(v as i32) as i8); }
        i += 1;
    }
    s
}

pub unsafe extern "system" fn _strupr(s: *mut i8) -> *mut i8 {
    if s.is_null() { return s; }
    let mut i = 0;
    loop {
        let v = unsafe { byte(s.cast(), i) };
        if v == 0 { break; }
        unsafe { s.add(i).write(toupper(v as i32) as i8); }
        i += 1;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_functions_handle_overlap_and_unsigned_comparison() {
        let mut bytes = *b"abcdef";
        unsafe { memcpy(bytes.as_mut_ptr().add(1).cast(), bytes.as_ptr().cast(), 5); }
        assert_eq!(&bytes, b"aabcde");
        assert_eq!(unsafe { memcmp(b"\xff\0".as_ptr().cast(), b"\x01\0".as_ptr().cast(), 1) }, 1);
        assert_eq!(unsafe { memchr(bytes.as_ptr().cast(), b'c' as i32, bytes.len()) }, unsafe { bytes.as_mut_ptr().add(3).cast() });
    }

    #[test]
    fn c_string_operations_cover_terminators_and_case() {
        let mut dst = [0i8; 16];
        unsafe {
            strcpy(dst.as_mut_ptr(), b"Cog\0".as_ptr().cast());
            strcat(dst.as_mut_ptr(), b"nac\0".as_ptr().cast());
            assert_eq!(strlen(dst.as_ptr()), 7);
            assert_eq!(strcmp(dst.as_ptr(), b"Cognac\0".as_ptr().cast()), 0);
            assert_eq!(_stricmp(dst.as_ptr(), b"COGNAC\0".as_ptr().cast()), 0);
            assert_eq!(strstr(dst.as_ptr(), b"nac\0".as_ptr().cast()), dst.as_mut_ptr().add(3));
            assert_eq!(strchr(dst.as_ptr(), b'n' as i32), dst.as_mut_ptr().add(3));
        }
    }

    #[test]
    fn secure_memory_functions_validate_lengths() {
        let mut dst = [0x55u8; 4];
        unsafe {
            assert_eq!(memcpy_s(dst.as_mut_ptr().cast(), dst.len(), b"12345".as_ptr().cast(), 5), ERANGE);
            assert_eq!(dst, [0; 4]);
            assert_eq!(memmove_s(dst.as_mut_ptr().cast(), dst.len(), b"12".as_ptr().cast(), 2), 0);
        }
        assert_eq!(&dst[..2], b"12");
    }

    #[test]
    fn rtl_ulong_memory_helpers_report_byte_counts() {
        let mut words = [7u32; 4];
        unsafe {
            assert_eq!(RtlCompareMemoryUlong(words.as_ptr().cast(), 16, 7), 16);
            RtlFillMemoryUlong(words.as_mut_ptr(), 8, 0x1234);
            assert_eq!(words, [0x1234, 0x1234, 7, 7]);
        }
    }

    #[test]
    fn secure_string_and_span_helpers() {
        let mut dst = [0i8; 8];
        unsafe {
            assert_eq!(strcpy_s(dst.as_mut_ptr(), dst.len(), b"ab\0".as_ptr().cast()), 0);
            assert_eq!(strcat_s(dst.as_mut_ptr(), dst.len(), b"cd\0".as_ptr().cast()), 0);
            assert_eq!(strcspn(dst.as_ptr(), b"dx\0".as_ptr().cast()), 3);
            assert_eq!(strspn(dst.as_ptr(), b"abcd\0".as_ptr().cast()), 4);
            assert_eq!(strpbrk(dst.as_ptr(), b"dx\0".as_ptr().cast()), dst.as_mut_ptr().add(3));
            assert_eq!(strncpy_s(dst.as_mut_ptr(), dst.len(), b"123456789\0".as_ptr().cast(), TRUNCATE), STRUNCATE);
            assert_eq!(dst[7], 0);
            _strlwr(dst.as_mut_ptr());
            assert_eq!(dst[0], b'1' as i8);
            _strupr(dst.as_mut_ptr());
            assert_eq!(dst[0], b'1' as i8);
        }
    }

    #[test]
    fn secure_concatenation_clears_on_range_error() {
        let mut dst = [0i8; 4];
        unsafe {
            strcpy(dst.as_mut_ptr(), b"ab\0".as_ptr().cast());
            assert_eq!(strcat_s(dst.as_mut_ptr(), dst.len(), b"cdef\0".as_ptr().cast()), ERANGE);
            assert_eq!(dst[0], 0);
        }
    }

}

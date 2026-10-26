//! Windows CRT/compiler helper exports used by the Windows 11 x64 NTDLL surface.
//!
//! Ordinary CRT routines use the Microsoft x64 ABI. Compiler-private unwind
//! and stack helpers have specialized contracts and are called out separately.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::{c_char, c_void};
use core::ptr;

const EINVAL: i32 = 22;
const ERANGE: i32 = 34;

#[no_mangle]
pub static _fltused: i32 = 0x9875;

static mut CRT_ERRNO: i32 = 0;

#[no_mangle]
pub unsafe extern "system" fn _errno() -> *mut i32 {
    // TODO: replace with per-thread CRT state once TEB TLS is wired.
    core::ptr::addr_of_mut!(CRT_ERRNO)
}

#[no_mangle]
pub unsafe extern "system" fn __isascii(c: i32) -> i32 {
    ((c as u32) <= 0x7f) as i32
}

#[no_mangle]
pub unsafe extern "system" fn __iscsym(c: i32) -> i32 {
    let c = c as u32;
    ((c <= 0x7f) && ((c as u8).is_ascii_alphanumeric() || c == b'_' as u32)) as i32
}

#[no_mangle]
pub unsafe extern "system" fn __iscsymf(c: i32) -> i32 {
    let c = c as u32;
    ((c <= 0x7f) && ((c as u8).is_ascii_alphabetic() || c == b'_' as u32)) as i32
}

#[no_mangle]
pub unsafe extern "system" fn __toascii(c: i32) -> i32 {
    c & 0x7f
}

#[no_mangle]
pub unsafe extern "system" fn _memccpy(
    destination: *mut c_void,
    source: *const c_void,
    stop_character: i32,
    count: usize,
) -> *mut c_void {
    if count == 0 || destination.is_null() || source.is_null() { return ptr::null_mut(); }
    let dst = destination.cast::<u8>();
    let src = source.cast::<u8>();
    let stop = stop_character as u8;
    for i in 0..count {
        let byte = unsafe { *src.add(i) };
        unsafe { *dst.add(i) = byte; }
        if byte == stop { return unsafe { dst.add(i + 1).cast::<c_void>() }; }
    }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "system" fn _memicmp(
    left: *const c_void,
    right: *const c_void,
    count: usize,
) -> i32 {
    if count == 0 { return 0; }
    if left.is_null() || right.is_null() { return 0; }
    let a = left.cast::<u8>();
    let b = right.cast::<u8>();
    for i in 0..count {
        let x = unsafe { *a.add(i) }.to_ascii_lowercase();
        let y = unsafe { *b.add(i) }.to_ascii_lowercase();
        if x != y { return x as i32 - y as i32; }
    }
    0
}

#[no_mangle]
pub unsafe extern "system" fn _lfind(
    key: *const c_void,
    base: *const c_void,
    number_of_elements: *mut u32,
    element_width: u32,
    compare: Option<unsafe extern "system" fn(*const c_void, *const c_void) -> i32>,
) -> *mut c_void {
    if key.is_null() || base.is_null() || number_of_elements.is_null() || element_width == 0 {
        return ptr::null_mut();
    }
    let Some(compare) = compare else { return ptr::null_mut(); };
    let count = unsafe { *number_of_elements } as usize;
    let width = element_width as usize;
    let Some(total) = count.checked_mul(width) else { return ptr::null_mut(); };
    let start = base.cast::<u8>();
    for offset in (0..total).step_by(width) {
        let item = unsafe { start.add(offset).cast::<c_void>() };
        if unsafe { compare(key, item) } == 0 { return item.cast_mut(); }
    }
    ptr::null_mut()
}

#[inline]
unsafe fn wide_to_i64(text: *const u16) -> i64 {
    if text.is_null() { return 0; }
    let mut p = text;
    loop {
        let c = unsafe { *p };
        if c == 0x20 || (0x09..=0x0d).contains(&c) { p = unsafe { p.add(1) }; }
        else { break; }
    }
    let mut negative = false;
    match unsafe { *p } {
        0x2b => p = unsafe { p.add(1) },
        0x2d => { negative = true; p = unsafe { p.add(1) }; }
        _ => {}
    }
    let mut value: u64 = 0;
    let mut any = false;
    while (b'0' as u16..=b'9' as u16).contains(&unsafe { *p }) {
        any = true;
        let digit = (unsafe { *p } - b'0' as u16) as u64;
        value = match value.checked_mul(10).and_then(|v| v.checked_add(digit)) {
            Some(v) => v,
            None => return if negative { i64::MIN } else { i64::MAX },
        };
        p = unsafe { p.add(1) };
    }
    if !any { return 0; }
    if negative {
        if value >= (i64::MAX as u64) + 1 { i64::MIN } else { -(value as i64) }
    } else {
        value.min(i64::MAX as u64) as i64
    }
}

#[no_mangle]
pub unsafe extern "system" fn _wtoi(text: *const u16) -> i32 {
    (unsafe { wide_to_i64(text) }).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[no_mangle]
pub unsafe extern "system" fn _wtol(text: *const u16) -> i32 {
    // Windows x64 uses LLP64: C long remains 32 bits.
    unsafe { _wtoi(text) }
}

#[no_mangle]
pub unsafe extern "system" fn _wtoi64(text: *const u16) -> i64 {
    unsafe { wide_to_i64(text) }
}

#[no_mangle]
pub unsafe extern "system" fn _wtoi64_l(text: *const u16, _locale: *mut c_void) -> i64 {
    unsafe { _wtoi64(text) }
}
#[no_mangle]
pub unsafe extern "system" fn _wtoi_l(text: *const u16, _locale: *mut c_void) -> i32 {
    unsafe { _wtoi(text) }
}
#[no_mangle]
pub unsafe extern "system" fn _wtol_l(text: *const u16, _locale: *mut c_void) -> i32 {
    unsafe { _wtol(text) }
}

fn split_path_bytes(path: &[u8]) -> (usize, usize, usize, usize) {
    let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    let path = &path[..len];
    let drive_end = if path.len() >= 2 && path[1] == b':' { 2 } else { 0 };
    let sep = path.iter().rposition(|&c| c == b'/' || c == b'\\');
    let dir_end = sep.map(|i| i + 1).unwrap_or(drive_end).max(drive_end);
    let dot = path[dir_end..].iter().rposition(|&c| c == b'.')
        .filter(|&i| i > 0).map(|i| dir_end + i);
    (drive_end, dir_end, dot.unwrap_or(path.len()), path.len())
}

unsafe fn copy_part_u8(dst: *mut u8, capacity: usize, src: &[u8]) -> i32 {
    if capacity == 0 { return if src.is_empty() { 0 } else { ERANGE }; }
    if dst.is_null() { return EINVAL; }
    if src.len() >= capacity {
        unsafe { *dst = 0; }
        return ERANGE;
    }
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
        *dst.add(src.len()) = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "system" fn _splitpath_s(
    path: *const u8,
    drive: *mut u8, drive_size: usize,
    directory: *mut u8, directory_size: usize,
    filename: *mut u8, filename_size: usize,
    extension: *mut u8, extension_size: usize,
) -> i32 {
    if path.is_null() { return EINVAL; }
    let mut len = 0usize;
    while len < 32768 && unsafe { *path.add(len) } != 0 { len += 1; }
    if len == 32768 { return EINVAL; }
    let bytes = unsafe { core::slice::from_raw_parts(path, len) };
    let (drive_end, dir_end, name_end, total) = split_path_bytes(bytes);
    let parts = [&bytes[..drive_end], &bytes[drive_end..dir_end],
                 &bytes[dir_end..name_end], &bytes[name_end..total]];
    let targets = [(drive, drive_size), (directory, directory_size),
                   (filename, filename_size), (extension, extension_size)];
    for (dst, cap) in targets {
        if !dst.is_null() && cap > 0 { unsafe { *dst = 0; } }
    }
    let mut result = 0;
    for ((dst, cap), part) in targets.into_iter().zip(parts) {
        let status = unsafe { copy_part_u8(dst, cap, part) };
        if status != 0 && result == 0 { result = status; }
    }
    result
}

#[no_mangle]
pub unsafe extern "system" fn _makepath_s(
    path: *mut u8, path_size: usize,
    drive: *const u8, directory: *const u8, filename: *const u8, extension: *const u8,
) -> i32 {
    if path.is_null() || path_size == 0 { return EINVAL; }
    unsafe fn get<'a>(p: *const u8) -> Option<&'a [u8]> {
        if p.is_null() { return None; }
        let mut n = 0usize;
        while n < 32768 && unsafe { *p.add(n) } != 0 { n += 1; }
        if n == 32768 { None } else { Some(unsafe { core::slice::from_raw_parts(p, n) }) }
    }
    let d = unsafe { get(drive) }.unwrap_or(&[]);
    let dir = unsafe { get(directory) }.unwrap_or(&[]);
    let name = unsafe { get(filename) }.unwrap_or(&[]);
    let ext = unsafe { get(extension) }.unwrap_or(&[]);
    let mut out = [0u8; 4096];
    let mut n = 0usize;
    let append = |out: &mut [u8], n: &mut usize, part: &[u8]| -> bool {
        if n.checked_add(part.len()).map_or(true, |v| v >= out.len()) { return false; }
        out[*n..*n + part.len()].copy_from_slice(part);
        *n += part.len();
        true
    };
    if !append(&mut out, &mut n, d) || !append(&mut out, &mut n, dir) {
        unsafe { *path = 0; } return ERANGE;
    }
    if !dir.is_empty() && !dir.last().is_some_and(|&c| c == b'/' || c == b'\\') {
        if !append(&mut out, &mut n, b"\\") { unsafe { *path = 0; } return ERANGE; }
    }
    if !append(&mut out, &mut n, name) { unsafe { *path = 0; } return ERANGE; }
    if !ext.is_empty() {
        if ext[0] != b'.' && !append(&mut out, &mut n, b".") { unsafe { *path = 0; } return ERANGE; }
        if !append(&mut out, &mut n, ext) { unsafe { *path = 0; } return ERANGE; }
    }
    if n >= path_size { unsafe { *path = 0; } return ERANGE; }
    unsafe { ptr::copy_nonoverlapping(out.as_ptr(), path, n); *path.add(n) = 0; }
    0
}

#[no_mangle]
pub unsafe extern "system" fn _splitpath(
    path: *const u8, drive: *mut u8, directory: *mut u8, filename: *mut u8, extension: *mut u8,
) {
    if path.is_null() { return; }
    let mut len = 0usize;
    while len < 32768 && unsafe { *path.add(len) } != 0 { len += 1; }
    if len == 32768 { return; }
    let bytes = unsafe { core::slice::from_raw_parts(path, len) };
    let (drive_end, dir_end, name_end, total) = split_path_bytes(bytes);
    unsafe {
        if !drive.is_null() { ptr::copy_nonoverlapping(bytes.as_ptr(), drive, drive_end); *drive.add(drive_end) = 0; }
        if !directory.is_null() { ptr::copy_nonoverlapping(bytes[drive_end..dir_end].as_ptr(), directory, dir_end-drive_end); *directory.add(dir_end-drive_end) = 0; }
        if !filename.is_null() { ptr::copy_nonoverlapping(bytes[dir_end..name_end].as_ptr(), filename, name_end-dir_end); *filename.add(name_end-dir_end) = 0; }
        if !extension.is_null() { ptr::copy_nonoverlapping(bytes[name_end..total].as_ptr(), extension, total-name_end); *extension.add(total-name_end) = 0; }
    }
}

// The SEH handler's real implementation must consume compiler scope tables and
// dispatcher metadata. Returning ExceptionContinueSearch is the safe fallback.
#[no_mangle]
pub unsafe extern "system" fn __C_specific_handler(
    _exception_record: *mut c_void,
    _establisher_frame: *mut c_void,
    _context_record: *mut c_void,
    _dispatcher_context: *mut c_void,
) -> i32 { 1 }

// These are compiler runtime entry points, not normal C functions. Full setjmp
// state capture and x64 stack probing require target-specific assembly/unwind code.
#[no_mangle]
pub unsafe extern "system" fn _setjmp(_environment: *mut c_void) -> i32 { 0 }
#[no_mangle]
pub unsafe extern "system" fn _setjmpex(_environment: *mut c_void, _frame: *mut c_void) -> i32 { 0 }
#[no_mangle]
pub unsafe extern "system" fn _local_unwind(_frame: *mut c_void, _registration: *mut c_void, _stop: i32) {}


fn split_path_wide(path: &[u16]) -> (usize, usize, usize, usize) {
    let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    let path = &path[..len];
    let drive_end = if path.len() >= 2 && path[1] == b':' as u16 { 2 } else { 0 };
    let sep = path.iter().rposition(|&c| c == b'/' as u16 || c == b'\\' as u16);
    let dir_end = sep.map(|i| i + 1).unwrap_or(drive_end).max(drive_end);
    let dot = path[dir_end..].iter().rposition(|&c| c == b'.' as u16)
        .filter(|&i| i > 0).map(|i| dir_end + i);
    (drive_end, dir_end, dot.unwrap_or(path.len()), path.len())
}

unsafe fn copy_part_u16(dst: *mut u16, capacity: usize, src: &[u16]) -> i32 {
    if capacity == 0 { return if src.is_empty() { 0 } else { ERANGE }; }
    if dst.is_null() { return EINVAL; }
    if src.len() >= capacity {
        unsafe { *dst = 0; }
        return ERANGE;
    }
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
        *dst.add(src.len()) = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "system" fn _wsplitpath_s(
    path: *const u16,
    drive: *mut u16, drive_size: usize,
    directory: *mut u16, directory_size: usize,
    filename: *mut u16, filename_size: usize,
    extension: *mut u16, extension_size: usize,
) -> i32 {
    if path.is_null() { return EINVAL; }
    let mut len = 0usize;
    while len < 32768 && unsafe { *path.add(len) } != 0 { len += 1; }
    if len == 32768 { return EINVAL; }
    let units = unsafe { core::slice::from_raw_parts(path, len) };
    let (drive_end, dir_end, name_end, total) = split_path_wide(units);
    let parts = [&units[..drive_end], &units[drive_end..dir_end],
                 &units[dir_end..name_end], &units[name_end..total]];
    let targets = [(drive, drive_size), (directory, directory_size),
                   (filename, filename_size), (extension, extension_size)];
    for (dst, cap) in targets {
        if !dst.is_null() && cap > 0 { unsafe { *dst = 0; } }
    }
    let mut result = 0;
    for ((dst, cap), part) in targets.into_iter().zip(parts) {
        let status = unsafe { copy_part_u16(dst, cap, part) };
        if status != 0 && result == 0 { result = status; }
    }
    result
}

#[no_mangle]
pub unsafe extern "system" fn _wmakepath_s(
    path: *mut u16, path_size: usize,
    drive: *const u16, directory: *const u16, filename: *const u16, extension: *const u16,
) -> i32 {
    if path.is_null() || path_size == 0 { return EINVAL; }
    unsafe fn get<'a>(p: *const u16) -> Option<&'a [u16]> {
        if p.is_null() { return None; }
        let mut n = 0usize;
        while n < 32768 && unsafe { *p.add(n) } != 0 { n += 1; }
        if n == 32768 { None } else { Some(unsafe { core::slice::from_raw_parts(p, n) }) }
    }
    let d = unsafe { get(drive) }.unwrap_or(&[]);
    let dir = unsafe { get(directory) }.unwrap_or(&[]);
    let name = unsafe { get(filename) }.unwrap_or(&[]);
    let ext = unsafe { get(extension) }.unwrap_or(&[]);
    let mut out = [0u16; 4096];
    let mut n = 0usize;
    let append = |out: &mut [u16], n: &mut usize, part: &[u16]| -> bool {
        if n.checked_add(part.len()).map_or(true, |v| v >= out.len()) { return false; }
        out[*n..*n + part.len()].copy_from_slice(part);
        *n += part.len();
        true
    };
    if !append(&mut out, &mut n, d) || !append(&mut out, &mut n, dir) {
        unsafe { *path = 0; } return ERANGE;
    }
    if !dir.is_empty() && !dir.last().is_some_and(|&c| c == b'/' as u16 || c == b'\\' as u16) {
        if !append(&mut out, &mut n, &[b'\\' as u16]) { unsafe { *path = 0; } return ERANGE; }
    }
    if !append(&mut out, &mut n, name) { unsafe { *path = 0; } return ERANGE; }
    if !ext.is_empty() {
        if ext[0] != b'.' as u16 && !append(&mut out, &mut n, &[b'.' as u16]) {
            unsafe { *path = 0; } return ERANGE;
        }
        if !append(&mut out, &mut n, ext) { unsafe { *path = 0; } return ERANGE; }
    }
    if n >= path_size { unsafe { *path = 0; } return ERANGE; }
    unsafe { ptr::copy_nonoverlapping(out.as_ptr(), path, n); *path.add(n) = 0; }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_helpers_follow_c_character_rules() {
        assert_eq!(unsafe { __isascii(0x7f) }, 1);
        assert_eq!(unsafe { __isascii(0x80) }, 0);
        assert_eq!(unsafe { __iscsym(b'_' as i32) }, 1);
        assert_eq!(unsafe { __iscsymf(b'9' as i32) }, 0);
        assert_eq!(unsafe { __toascii(0xff) }, 0x7f);
    }

    #[test]
    fn memccpy_returns_byte_after_delimiter() {
        let source = *b"ab:c";
        let mut destination = [0u8; 4];
        let result = unsafe { _memccpy(destination.as_mut_ptr().cast(), source.as_ptr().cast(), b':' as i32, source.len()) };
        assert_eq!(result, unsafe { destination.as_mut_ptr().add(3).cast() });
        assert_eq!(&destination[..3], b"ab:");
    }

    #[test]
    fn wide_integer_conversion_handles_sign_and_whitespace() {
        let text = [b' ' as u16, b'-' as u16, b'4' as u16, b'2' as u16, 0];
        assert_eq!(unsafe { _wtoi(text.as_ptr()) }, -42);
        assert_eq!(unsafe { _wtoi64(text.as_ptr()) }, -42);
        let invalid = [b'x' as u16, 0];
        assert_eq!(unsafe { _wtoi(invalid.as_ptr()) }, 0);
    }

    #[test]
    fn splitpath_splits_drive_directory_filename_and_extension() {
        let path = b"C:\\dir\\file.ext\0";
        let mut drive = [0u8; 4];
        let mut directory = [0u8; 32];
        let mut filename = [0u8; 16];
        let mut extension = [0u8; 8];
        assert_eq!(unsafe { _splitpath_s(path.as_ptr(), drive.as_mut_ptr(), drive.len(), directory.as_mut_ptr(), directory.len(), filename.as_mut_ptr(), filename.len(), extension.as_mut_ptr(), extension.len()) }, 0);
        assert_eq!(&drive[..3], b"C:\0");
        assert_eq!(&directory[..6], b"\\dir\\\0");
        assert_eq!(&filename[..5], b"file\0");
        assert_eq!(&extension[..5], b".ext\0");
    }

    #[test]
    fn makepath_s_builds_path_and_rejects_small_output() {
        let drive = b"C:\0";
        let dir = b"dir\0";
        let name = b"file\0";
        let ext = b"txt\0";
        let mut output = [0u8; 32];
        assert_eq!(unsafe { _makepath_s(output.as_mut_ptr(), output.len(), drive.as_ptr(), dir.as_ptr(), name.as_ptr(), ext.as_ptr()) }, 0);
        assert_eq!(&output[..14], b"C:dir\\file.txt");
        let mut small = [0u8; 3];
        assert_eq!(unsafe { _makepath_s(small.as_mut_ptr(), small.len(), drive.as_ptr(), dir.as_ptr(), name.as_ptr(), ext.as_ptr()) }, ERANGE);
        assert_eq!(small[0], 0);
    }
}


// C string, search, sorting and memory routines moved out of misc.rs.
#[no_mangle]
pub unsafe extern "C" fn atoi(s: *const core::ffi::c_char) -> i32 {
    strtol(s, core::ptr::null_mut(), 10) as i32
}

#[no_mangle]
pub unsafe extern "C" fn atol(s: *const core::ffi::c_char) -> i32 {
    strtol(s, core::ptr::null_mut(), 10) as isize
}

#[no_mangle]
pub unsafe extern "C" fn strtol(s: *const core::ffi::c_char, end: *mut *mut core::ffi::c_char, base: i32) -> i32 {
    if end.is_null() == false { *end = s as *mut _; }
    if s.is_null() || !(base == 0 || (2..=36).contains(&base)) { return 0; }
    let mut p = s; while (*p as u8).is_ascii_whitespace() { p = p.add(1); }
    let mut neg = false; if *p == b'-' as i8 { neg = true; p = p.add(1); } else if *p == b'+' as i8 { p = p.add(1); }
    let mut radix = base as u32; if radix == 0 { if *p == b'0' as i8 && (*p.add(1) == b'x' as i8 || *p.add(1) == b'X' as i8) { radix = 16; p = p.add(2); } else if *p == b'0' as i8 { radix = 8; } else { radix = 10; } } else if radix == 16 && *p == b'0' as i8 && (*p.add(1) == b'x' as i8 || *p.add(1) == b'X' as i8) { p = p.add(2); }
    let limit = if neg { (i32::MAX as u64) + 1 } else { i32::MAX as u64 }; let mut value = 0u64; let mut any = false;
    loop { let ch = *p as u8; let d = if ch >= b'0' && ch <= b'9' { (ch-b'0') as u32 } else if ch >= b'a' && ch <= b'z' { (ch-b'a'+10) as u32 } else if ch >= b'A' && ch <= b'Z' { (ch-b'A'+10) as u32 } else { break; }; if d >= radix { break; } any = true; value = match value.checked_mul(radix as u64).and_then(|v| v.checked_add(d as u64)) { Some(v) if v <= limit => v, _ => limit }; p = p.add(1); }
    if !any { if !end.is_null() { *end = s as *mut _; } return 0; } if !end.is_null() { *end = p as *mut _; } if neg { (value.wrapping_neg()) as i32 } else { value as i32 }
}

#[no_mangle]
pub unsafe extern "C" fn wcslen(s: *const u16) -> usize {
    if s.is_null() { return 0; }
    let mut n = 0usize;
    while *s.add(n) != 0 { n = n.saturating_add(1); }
    n
}

#[no_mangle]
pub unsafe extern "C" fn strcmp(a: *const core::ffi::c_char, b: *const core::ffi::c_char) -> i32 {
    if a.is_null() || b.is_null() { return if a.is_null() { if b.is_null() { 0 } else { -1 } } else { 1 }; }
    let (mut i, mut x, mut y) = (0usize, 0u8, 0u8);
    loop { x = *a.add(i) as u8; y = *b.add(i) as u8; if x != y || x == 0 { return x as i32 - y as i32; } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn strncmp(a: *const core::ffi::c_char, b: *const core::ffi::c_char, n: usize) -> i32 {
    if n == 0 { return 0; }
    if a.is_null() || b.is_null() { return if a.is_null() { if b.is_null() { 0 } else { -1 } } else { 1 }; }
    let mut i = 0usize; while i < n { let x = *a.add(i) as u8; let y = *b.add(i) as u8; if x != y || x == 0 { return x as i32 - y as i32; } i += 1; } 0
}

#[no_mangle]
pub unsafe extern "C" fn wcscmp(a: *const u16, b: *const u16) -> i32 {
    if a.is_null() || b.is_null() { return if a.is_null() { if b.is_null() { 0 } else { -1 } } else { 1 }; }
    let mut i = 0usize; loop { let x = *a.add(i); let y = *b.add(i); if x != y || x == 0 { return x as i32 - y as i32; } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn wcsncmp(a: *const u16, b: *const u16, n: usize) -> i32 {
    if n == 0 { return 0; }
    if a.is_null() || b.is_null() { return if a.is_null() { if b.is_null() { 0 } else { -1 } } else { 1 }; }
    let mut i = 0usize; while i < n { let x = *a.add(i); let y = *b.add(i); if x != y || x == 0 { return x as i32 - y as i32; } i += 1; } 0
}

#[no_mangle]
pub unsafe extern "C" fn strchr(s: *const core::ffi::c_char, ch: i32) -> *mut core::ffi::c_char {
    if s.is_null() { return core::ptr::null_mut(); } let target = ch as u8; let mut i = 0usize; loop { let v = *s.add(i) as u8; if v == target { return s.add(i) as *mut _; } if v == 0 { return core::ptr::null_mut(); } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn wcschr(s: *const u16, ch: i32) -> *mut u16 {
    if s.is_null() { return core::ptr::null_mut(); } let target = ch as u16; let mut i = 0usize; loop { let v = *s.add(i); if v == target { return s.add(i) as *mut _; } if v == 0 { return core::ptr::null_mut(); } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn strrchr(s: *const core::ffi::c_char, ch: i32) -> *mut core::ffi::c_char {
    if s.is_null() { return core::ptr::null_mut(); } let target = ch as u8; let mut i = 0usize; let mut found = core::ptr::null_mut(); loop { let v = *s.add(i) as u8; if v == target { found = s.add(i) as *mut _; } if v == 0 { return found; } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn wcsrchr(s: *const u16, ch: i32) -> *mut u16 {
    if s.is_null() { return core::ptr::null_mut(); } let target = ch as u16; let mut i = 0usize; let mut found = core::ptr::null_mut(); loop { let v = *s.add(i); if v == target { found = s.add(i) as *mut _; } if v == 0 { return found; } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn bsearch(key:*const c_void,base:*const c_void,count:usize,width:usize,compare:Option<unsafe extern "C" fn(*const c_void,*const c_void)->i32>)->*mut c_void {
 if key.is_null()||compare.is_none()||width==0||(base.is_null()&&count!=0){return ptr::null_mut();} let cmp=compare.unwrap();let(mut lo,mut hi)=(0usize,count);while lo<hi{let mid=lo+(hi-lo)/2;let item=(base as *const u8).add(mid*width) as *const c_void;let c=cmp(key,item);if c==0{return item as *mut _;}if c<0{hi=mid;}else{lo=mid+1;}}ptr::null_mut()
}
#[no_mangle]
pub unsafe extern "C" fn qsort(base:*mut c_void,count:usize,width:usize,compare:Option<unsafe extern "C" fn(*const c_void,*const c_void)->i32>){
 if base.is_null()||count<2||width==0||compare.is_none(){return;}let cmp=compare.unwrap();let p=base as *mut u8;let swap=|a:usize,b:usize|unsafe{for j in 0..width{ptr::swap(p.add(a*width+j),p.add(b*width+j));}};let sift=|mut root:usize,end:usize|unsafe{loop{let child=root*2+1;if child>=end{break;}let mut largest=child;if child+1<end&&cmp(p.add(child*width) as _,p.add((child+1)*width) as _) <0{largest=child+1;}if cmp(p.add(root*width) as _,p.add(largest*width) as _)>=0{break;}swap(root,largest);root=largest;}};let mut start=count/2;while start>0{start-=1;sift(start,count);}let mut end=count;while end>1{end-=1;swap(0,end);sift(0,end);}
}
#[no_mangle]
pub unsafe extern "C" fn memchr(s:*const c_void,ch:i32,n:usize)->*mut c_void{if s.is_null(){return ptr::null_mut();}let p=s as *const u8;for i in 0..n{if *p.add(i)==ch as u8{return p.add(i) as *mut _;}}ptr::null_mut()}
#[no_mangle]
pub unsafe extern "C" fn strnlen(s:*const c_char,max:usize)->usize{if s.is_null(){return 0;}let mut i=0;while i<max&&*s.add(i)!=0{i+=1;}i}
#[no_mangle]
pub unsafe extern "C" fn strcpy(d:*mut c_char,s:*const c_char)->*mut c_char{if d.is_null()||s.is_null(){return d;}let mut i=0;loop{let c=*s.add(i);*d.add(i)=c;if c==0{break;}i+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn strncpy(d:*mut c_char,s:*const c_char,n:usize)->*mut c_char{if d.is_null()||s.is_null(){return d;}let mut i=0;while i<n&&*s.add(i)!=0{*d.add(i)=*s.add(i);i+=1;}while i<n{*d.add(i)=0;i+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn strcat(d:*mut c_char,s:*const c_char)->*mut c_char{if d.is_null()||s.is_null(){return d;}let mut i=0;while *d.add(i)!=0{i+=1;}let mut j=0;loop{let c=*s.add(j);*d.add(i+j)=c;if c==0{break;}j+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn strncat(d:*mut c_char,s:*const c_char,n:usize)->*mut c_char{if d.is_null()||s.is_null(){return d;}let mut i=0;while *d.add(i)!=0{i+=1;}let mut j=0;while j<n&&*s.add(j)!=0{*d.add(i+j)=*s.add(j);j+=1;}*d.add(i+j)=0;d}
#[no_mangle]
pub unsafe extern "C" fn wcscpy(d:*mut u16,s:*const u16)->*mut u16{if d.is_null()||s.is_null(){return d;}let mut i=0;loop{let c=*s.add(i);*d.add(i)=c;if c==0{break;}i+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn wcsncpy(d:*mut u16,s:*const u16,n:usize)->*mut u16{if d.is_null()||s.is_null(){return d;}let mut i=0;while i<n&&*s.add(i)!=0{*d.add(i)=*s.add(i);i+=1;}while i<n{*d.add(i)=0;i+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn wcscat(d:*mut u16,s:*const u16)->*mut u16{if d.is_null()||s.is_null(){return d;}let mut i=0;while *d.add(i)!=0{i+=1;}let mut j=0;loop{let c=*s.add(j);*d.add(i+j)=c;if c==0{break;}j+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn wcsncat(d:*mut u16,s:*const u16,n:usize)->*mut u16{if d.is_null()||s.is_null(){return d;}let mut i=0;while *d.add(i)!=0{i+=1;}let mut j=0;while j<n&&*s.add(j)!=0{*d.add(i+j)=*s.add(j);j+=1;}*d.add(i+j)=0;d}


/// Microsoft-specific secure search/sort signatures use the context as the first comparator argument.
#[no_mangle]
pub unsafe extern "C" fn bsearch_s(key:*const c_void,base:*const c_void,count:usize,width:usize,compare:Option<unsafe extern "C" fn(*mut c_void,*const c_void,*const c_void)->i32>,context:*mut c_void)->*mut c_void {
 if key.is_null()||compare.is_none()||width==0||(base.is_null()&&count!=0){return ptr::null_mut();}
 let cmp=compare.unwrap();let(mut lo,mut hi)=(0usize,count);
 while lo<hi{let mid=lo+(hi-lo)/2;let item=(base as *const u8).add(mid*width) as *const c_void;let r=cmp(context,key,item);if r==0{return item as *mut c_void;}if r<0{hi=mid;}else{lo=mid+1;}}ptr::null_mut()
}
#[no_mangle]
pub unsafe extern "C" fn qsort_s(base:*mut c_void,count:usize,width:usize,compare:Option<unsafe extern "C" fn(*mut c_void,*const c_void,*const c_void)->i32>,context:*mut c_void){
 if base.is_null()||count<2||width==0||compare.is_none(){return;}let cmp=compare.unwrap();let p=base as *mut u8;let swap=|a:usize,b:usize|unsafe{for j in 0..width{ptr::swap(p.add(a*width+j),p.add(b*width+j));}};let sift=|mut root:usize,end:usize|unsafe{loop{let child=root*2+1;if child>=end{break;}let mut largest=child;if child+1<end&&cmp(context,p.add(child*width) as _,p.add((child+1)*width) as _) <0{largest=child+1;}if cmp(context,p.add(root*width) as _,p.add(largest*width) as _)>=0{break;}swap(root,largest);root=largest;}};let mut start=count/2;while start>0{start-=1;sift(start,count);}let mut end=count;while end>1{end-=1;swap(0,end);sift(0,end);}
}
#[no_mangle]
pub unsafe extern "C" fn memcpy_s(dst:*mut c_void,dst_size:usize,src:*const c_void,count:usize)->i32{if count==0{return 0;}if dst.is_null()||src.is_null()||count>dst_size{if !dst.is_null(){ptr::write_bytes(dst,0,dst_size);}return EINVAL;}ptr::copy_nonoverlapping(src as *const u8,dst as *mut u8,count);0}
#[no_mangle]
pub unsafe extern "C" fn memmove_s(dst:*mut c_void,dst_size:usize,src:*const c_void,count:usize)->i32{if count==0{return 0;}if dst.is_null()||src.is_null()||count>dst_size{if !dst.is_null(){ptr::write_bytes(dst,0,dst_size);}return EINVAL;}ptr::copy(src as *const u8,dst as *mut u8,count);0}
#[no_mangle]
pub unsafe extern "C" fn strtoul(s:*const c_char,end:*mut *mut c_char,base:i32)->u32{if !end.is_null(){*end=s as *mut c_char;}if s.is_null()||!(base==0||(2..=36).contains(&base)){return 0;}let mut p=s;while (*p as u8).is_ascii_whitespace(){p=p.add(1);}let neg=*p==b'-' as i8;if neg||*p==b'+' as i8{p=p.add(1);}let mut radix=base as u32;if radix==0{if *p==b'0' as i8&&(*p.add(1)==b'x' as i8||*p.add(1)==b'X' as i8){radix=16;p=p.add(2);}else if *p==b'0' as i8{radix=8;}else{radix=10;}}else if radix==16&&*p==b'0' as i8&&(*p.add(1)==b'x' as i8||*p.add(1)==b'X' as i8){p=p.add(2);}let mut any=false;let mut v=0u64;loop{let ch=*p as u8;let d=if ch>=b'0'&&ch<=b'9'{(ch-b'0')as u32}else if ch>=b'a'&&ch<=b'z'{(ch-b'a'+10)as u32}else if ch>=b'A'&&ch<=b'Z'{(ch-b'A'+10)as u32}else{break};if d>=radix{break;}any=true;v=v.saturating_mul(radix as u64).saturating_add(d as u64);p=p.add(1);}if !any{return 0;}if !end.is_null(){*end=p as *mut c_char;}if neg{0u64.wrapping_sub(v)}else{v}}
#[no_mangle]
pub unsafe extern "C" fn strspn(s:*const c_char,accept:*const c_char)->usize{if s.is_null()||accept.is_null(){return 0;}let mut i=0;while *s.add(i)!=0{let mut j=0;while *accept.add(j)!=0&&*accept.add(j)!=*s.add(i){j+=1;}if *accept.add(j)==0{break;}i+=1;}i}
#[no_mangle]
pub unsafe extern "C" fn strcspn(s:*const c_char,reject:*const c_char)->usize{if s.is_null()||reject.is_null(){return 0;}let mut i=0;while *s.add(i)!=0{let mut j=0;while *reject.add(j)!=0&&*reject.add(j)!=*s.add(i){j+=1;}if *reject.add(j)!=0{break;}i+=1;}i}
#[no_mangle]
pub unsafe extern "C" fn strpbrk(s:*const c_char,accept:*const c_char)->*mut c_char{if s.is_null()||accept.is_null(){return ptr::null_mut();}let mut i=0;while *s.add(i)!=0{let mut j=0;while *accept.add(j)!=0{if *s.add(i)==*accept.add(j){return s.add(i) as *mut c_char;}j+=1;}i+=1;}ptr::null_mut()}
#[no_mangle]
pub unsafe extern "C" fn strstr(haystack:*const c_char,needle:*const c_char)->*mut c_char{if haystack.is_null()||needle.is_null(){return ptr::null_mut();}if *needle==0{return haystack as *mut c_char;}let mut i=0;while *haystack.add(i)!=0{let mut j=0;while *needle.add(j)!=0&&*haystack.add(i+j)==*needle.add(j){j+=1;}if *needle.add(j)==0{return haystack.add(i) as *mut c_char;}i+=1;}ptr::null_mut()}
#[no_mangle]
pub unsafe extern "C" fn wcsnlen(s:*const u16,max:usize)->usize{if s.is_null(){return 0;}let mut i=0;while i<max&&*s.add(i)!=0{i+=1;}i}
#[no_mangle]
pub unsafe extern "C" fn wcsspn(s:*const u16,accept:*const u16)->usize{if s.is_null()||accept.is_null(){return 0;}let mut i=0;while *s.add(i)!=0{let mut j=0;while *accept.add(j)!=0&&*accept.add(j)!=*s.add(i){j+=1;}if *accept.add(j)==0{break;}i+=1;}i}
#[no_mangle]
pub unsafe extern "C" fn wcscspn(s:*const u16,reject:*const u16)->usize{if s.is_null()||reject.is_null(){return 0;}let mut i=0;while *s.add(i)!=0{let mut j=0;while *reject.add(j)!=0&&*reject.add(j)!=*s.add(i){j+=1;}if *reject.add(j)!=0{break;}i+=1;}i}
#[no_mangle]
pub unsafe extern "C" fn wcspbrk(s:*const u16,accept:*const u16)->*mut u16{if s.is_null()||accept.is_null(){return ptr::null_mut();}let mut i=0;while *s.add(i)!=0{let mut j=0;while *accept.add(j)!=0{if *s.add(i)==*accept.add(j){return s.add(i) as *mut u16;}j+=1;}i+=1;}ptr::null_mut()}
#[no_mangle]
pub unsafe extern "C" fn wcsstr(haystack:*const u16,needle:*const u16)->*mut u16{if haystack.is_null()||needle.is_null(){return ptr::null_mut();}if *needle==0{return haystack as *mut u16;}let mut i=0;while *haystack.add(i)!=0{let mut j=0;while *needle.add(j)!=0&&*haystack.add(i+j)==*needle.add(j){j+=1;}if *needle.add(j)==0{return haystack.add(i) as *mut u16;}i+=1;}ptr::null_mut()}
#[no_mangle]
pub unsafe extern "C" fn wcstol(s:*const u16,end:*mut *mut u16,base:i32)->i32{if !end.is_null(){*end=s as *mut u16;}if s.is_null()||!(base==0||(2..=36).contains(&base)){return 0;}let mut i=0usize;while (*s.add(i))<=0x20&&*s.add(i)!=0{i+=1;}let mut neg=false;if *s.add(i)==45{neg=true;i+=1;}else if *s.add(i)==43{i+=1;}let mut radix=base as u32;if radix==0{if *s.add(i)==48{if *s.add(i+1)==120||*s.add(i+1)==88{radix=16;i+=2;}else{radix=8;}}else{radix=10;}}else if radix==16&&*s.add(i)==48&&(*s.add(i+1)==120||*s.add(i+1)==88){i+=2;}let mut any=false;let mut v=0u64;let mut limit=if neg{(i32::MAX as u64)+1}else{i32::MAX as u64};loop{let ch=*s.add(i) as u32;let d=if ch>=48&&ch<=57{ch-48}else if ch>=97&&ch<=122{ch-97+10}else if ch>=65&&ch<=90{ch-65+10}else{break};if d>=radix{break;}any=true;v=v.saturating_mul(radix as u64).saturating_add(d as u64).min(limit);i+=1;}if !any{return 0;}if !end.is_null(){*end=s.add(i) as *mut u16;}if neg{v.wrapping_neg() as i32}else{v as i32}}
#[no_mangle]
pub unsafe extern "C" fn wcstoul(s:*const u16,end:*mut *mut u16,base:i32)->u32{wcstol(s,end,base) as u32}


// Scalar C runtime math and ASCII classification routines.

#[no_mangle] pub unsafe extern "C" fn atan(x:f64)->f64{x.atan()}
#[no_mangle] pub unsafe extern "C" fn atan2(y:f64,x:f64)->f64{y.atan2(x)}
#[no_mangle] pub unsafe extern "C" fn ceil(x:f64)->f64{x.ceil()}
#[no_mangle] pub unsafe extern "C" fn cos(x:f64)->f64{x.cos()}
#[no_mangle] pub unsafe extern "C" fn fabs(x:f64)->f64{x.abs()}
#[no_mangle] pub unsafe extern "C" fn floor(x:f64)->f64{x.floor()}
#[no_mangle] pub unsafe extern "C" fn log(x:f64)->f64{x.ln()}
#[no_mangle] pub unsafe extern "C" fn pow(x:f64,y:f64)->f64{x.powf(y)}
#[no_mangle] pub unsafe extern "C" fn sin(x:f64)->f64{x.sin()}
#[no_mangle] pub unsafe extern "C" fn sqrt(x:f64)->f64{x.sqrt()}
#[no_mangle] pub unsafe extern "C" fn tan(x:f64)->f64{x.tan()}
#[no_mangle] pub unsafe extern "C" fn isalnum(c:i32)->i32{((0..=127).contains(&c)&&(c as u8).is_ascii_alphanumeric()) as i32}
#[no_mangle] pub unsafe extern "C" fn isalpha(c:i32)->i32{((0..=127).contains(&c)&&(c as u8).is_ascii_alphabetic()) as i32}
#[no_mangle] pub unsafe extern "C" fn iscntrl(c:i32)->i32{((0..=127).contains(&c)&&(c<32||c==127)) as i32}
#[no_mangle] pub unsafe extern "C" fn isdigit(c:i32)->i32{((0..=127).contains(&c)&&(c as u8).is_ascii_digit()) as i32}
#[no_mangle] pub unsafe extern "C" fn isgraph(c:i32)->i32{(33..=126).contains(&c) as i32}
#[no_mangle] pub unsafe extern "C" fn islower(c:i32)->i32{((0..=127).contains(&c)&&(c as u8).is_ascii_lowercase()) as i32}
#[no_mangle] pub unsafe extern "C" fn isprint(c:i32)->i32{(32..=126).contains(&c) as i32}
#[no_mangle] pub unsafe extern "C" fn ispunct(c:i32)->i32{((33..=126).contains(&c)&&!(c as u8).is_ascii_alphanumeric()) as i32}
#[no_mangle] pub unsafe extern "C" fn isspace(c:i32)->i32{matches!(c,9..=13|32) as i32}
#[no_mangle] pub unsafe extern "C" fn isupper(c:i32)->i32{((0..=127).contains(&c)&&(c as u8).is_ascii_uppercase()) as i32}
#[no_mangle] pub unsafe extern "C" fn isxdigit(c:i32)->i32{((0..=127).contains(&c)&&(c as u8).is_ascii_hexdigit()) as i32}
#[no_mangle] pub unsafe extern "C" fn tolower(c:i32)->i32{if (65..=90).contains(&c){c+32}else{c}}
#[no_mangle] pub unsafe extern "C" fn toupper(c:i32)->i32{if (97..=122).contains(&c){c-32}else{c}}

//! Windows CRT/compiler helper exports used by the Windows 11 x64 NTDLL surface.
//!
//! Ordinary CRT routines use the Microsoft x64 ABI. Compiler-private unwind
//! and stack helpers have specialized contracts and are called out separately.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
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
    let get = |p: *const u8| -> Option<&[u8]> {
        if p.is_null() { return None; }
        let mut n = 0usize;
        while n < 32768 && unsafe { *p.add(n) } != 0 { n += 1; }
        if n == 32768 { None } else { Some(unsafe { core::slice::from_raw_parts(p, n) }) }
    };
    let d = get(drive).unwrap_or(&[]);
    let dir = get(directory).unwrap_or(&[]);
    let name = get(filename).unwrap_or(&[]);
    let ext = get(extension).unwrap_or(&[]);
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
        assert_eq!(&output[..13], b"C:dir\\file.txt");
        let mut small = [0u8; 3];
        assert_eq!(unsafe { _makepath_s(small.as_mut_ptr(), small.len(), drive.as_ptr(), dir.as_ptr(), name.as_ptr(), ext.as_ptr()) }, ERANGE);
        assert_eq!(small[0], 0);
    }
}

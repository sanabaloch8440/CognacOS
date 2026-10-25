#![allow(non_snake_case)]

pub type WCHAR = u16;
pub type ULONG = u32;
pub type RTL_PATH_TYPE = u32;

pub const RtlPathTypeUnknown: RTL_PATH_TYPE = 0;
pub const RtlPathTypeUncAbsolute: RTL_PATH_TYPE = 1;
pub const RtlPathTypeDriveAbsolute: RTL_PATH_TYPE = 2;
pub const RtlPathTypeDriveRelative: RTL_PATH_TYPE = 3;
pub const RtlPathTypeRooted: RTL_PATH_TYPE = 4;
pub const RtlPathTypeRelative: RTL_PATH_TYPE = 5;
pub const RtlPathTypeLocalDevice: RTL_PATH_TYPE = 6;
pub const RtlPathTypeRootLocalDevice: RTL_PATH_TYPE = 7;

#[inline]
fn separator(ch: WCHAR) -> bool {
    ch == 92 || ch == 47
}

#[inline]
fn fold(ch: WCHAR) -> WCHAR {
    if (65..=90).contains(&ch) { ch + 32 } else { ch }
}

unsafe fn at(path: *const WCHAR, index: usize) -> WCHAR {
    // SAFETY: The caller supplies a readable NUL-terminated UTF-16 path.
    unsafe { path.add(index).read() }
}

unsafe fn equals_ascii_prefix_ci(path: *const WCHAR, text: &[u8]) -> bool {
    for (i, ch) in text.iter().enumerate() {
        if fold(unsafe { at(path, i) }) != fold(*ch as WCHAR) { return false; }
    }
    true
}

unsafe fn equals_ascii_ci(path: *const WCHAR, text: &[u8]) -> bool {
    unsafe { equals_ascii_prefix_ci(path, text) && at(path, text.len()) == 0 }
}

#[no_mangle]
pub unsafe extern "system" fn RtlDetermineDosPathNameType_U(path: *const WCHAR) -> RTL_PATH_TYPE {
    if path.is_null() { return RtlPathTypeUnknown; }
    let first = unsafe { at(path, 0) };
    if separator(first) {
        let second = unsafe { at(path, 1) };
        if !separator(second) { return RtlPathTypeRooted; }
        let third = unsafe { at(path, 2) };
        if third != 46 && third != 63 { return RtlPathTypeUncAbsolute; }
        if separator(unsafe { at(path, 3) }) { return RtlPathTypeLocalDevice; }
        if unsafe { at(path, 3) } != 0 { return RtlPathTypeUncAbsolute; }
        return RtlPathTypeRootLocalDevice;
    }
    if first == 0 || unsafe { at(path, 1) } != 58 { return RtlPathTypeRelative; }
    if separator(unsafe { at(path, 2) }) { RtlPathTypeDriveAbsolute } else { RtlPathTypeDriveRelative }
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsDosDeviceName_U(path: *const WCHAR) -> ULONG {
    if path.is_null() { return 0; }
    let kind = unsafe { RtlDetermineDosPathNameType_U(path) };
    if kind == RtlPathTypeUnknown || kind == RtlPathTypeUncAbsolute { return 0; }

    if kind == RtlPathTypeLocalDevice {
        if unsafe { equals_ascii_ci(path, b"\\\\.\\CON") } {
            return (8 << 16) | 6;
        }
        return 0;
    }

    let mut start = if kind == RtlPathTypeDriveAbsolute || kind == RtlPathTypeDriveRelative { 2 } else { 0 };
    let mut i = start;
    while unsafe { at(path, i) } != 0 {
        if separator(unsafe { at(path, i) }) { start = i + 1; }
        i += 1;
    }

    let mut end = start;
    while unsafe { at(path, end) } != 0 {
        let ch = unsafe { at(path, end) };
        if ch == 46 || ch == 58 { break; }
        end += 1;
    }
    if end == start { return 0; }
    end -= 1;
    while end >= start && unsafe { at(path, end) } == 32 {
        if end == start { return 0; }
        end -= 1;
    }
    let length = end - start + 1;

    let matched = match length {
        3 => unsafe {
            equals_ascii_prefix_ci(path.add(start), b"AUX")
                || equals_ascii_prefix_ci(path.add(start), b"CON")
                || equals_ascii_prefix_ci(path.add(start), b"NUL")
                || equals_ascii_prefix_ci(path.add(start), b"PRN")
        },
        4 => {
            let a = fold(unsafe { at(path, start) });
            let b = fold(unsafe { at(path, start + 1) });
            let c = fold(unsafe { at(path, start + 2) });
            let digit = unsafe { at(path, end) };
            ((a == 67 && b == 79 && c == 77) || (a == 76 && b == 80 && c == 84))
                && digit > 48 && digit <= 57
        },
        6 => unsafe { equals_ascii_prefix_ci(path.add(start), b"CONIN$") },
        7 => unsafe { equals_ascii_prefix_ci(path.add(start), b"CONOUT$") },
        _ => false,
    };
    if !matched { return 0; }

    let name_bytes = (length * 2) as u32;
    let offset_bytes = (start * 2) as u32;
    (offset_bytes << 16) | name_bytes
}


/// Collapse separators, "." / ".." components, and Windows trailing dots/spaces.
/// The path slice must contain a NUL terminator; `mark` protects its root prefix.
fn collapse_dos_path(path: &mut [WCHAR], mark: usize) {
    let capacity = path.len();
    if capacity == 0 { return; }
    let mut len = path.iter().position(|&c| c == 0).unwrap_or(capacity);
    for ch in &mut path[..len] {
        if *ch == b'/' as WCHAR { *ch = b'\\\\' as WCHAR; }
    }

    // Compact duplicate separators without disturbing the protected prefix.
    let mut write = core::cmp::max(1, mark).min(len);
    for read in write..len {
        if path[read] != b'\\\\' as WCHAR || write == 0 || path[write - 1] != b'\\\\' as WCHAR {
            path[write] = path[read];
            write += 1;
        }
    }
    len = write;
    path[len] = 0;

    let mut p = mark.min(len);
    while p < len {
        if path[p] == b'.' as WCHAR {
            let next = path.get(p + 1).copied().unwrap_or(0);
            if next == b'\\\\' as WCHAR || next == 0 {
                let is_parent = next != 0 && path.get(p + 2).copied() == Some(b'.' as WCHAR)
                    && (path.get(p + 3).copied() == Some(b'\\\\' as WCHAR)
                        || path.get(p + 3).copied().unwrap_or(0) == 0);
                let is_current = next == b'\\\\' as WCHAR;
                if is_current || next == 0 || is_parent {
                    let mut start = p;
                    if is_parent {
                        if start > mark {
                            start -= 1;
                            while start > mark && path[start - 1] != b'\\\\' as WCHAR { start -= 1; }
                        }
                        let mut end = p + 3;
                        if path.get(end).copied() == Some(b'\\\\' as WCHAR) { end += 1; }
                        if path.get(p + 3).copied().unwrap_or(0) == 0 {
                            if start > mark && path[start - 1] == b'\\\\' as WCHAR { start -= 1; }
                            path[start] = 0;
                            len = start;
                            break;
                        }
                        let tail = len - end;
                        path.copy_within(end..end + tail, start);
                        len -= end - start;
                    } else if next == 0 {
                        if start > mark { start -= 1; }
                        path[start] = 0;
                        len = start;
                        break;
                    } else {
                        let mut end = p + 2;
                        while end < len && path[end] == b'\\\\' as WCHAR { end += 1; }
                        let tail = len - end;
                        path.copy_within(end..end + tail, p);
                        len -= end - p;
                    }
                    path[len] = 0;
                    p = start;
                    continue;
                }
            }
        }
        while p < len && path[p] != b'\\\\' as WCHAR { p += 1; }
        if p < len {
            if p > mark && path[p - 1] == b'.' as WCHAR {
                path.copy_within(p..len, p - 1);
                len -= 1;
                path[len] = 0;
            } else { p += 1; }
        }
    }

    while len > mark && (path[len - 1] == b' ' as WCHAR || path[len - 1] == b'.' as WCHAR) {
        len -= 1;
    }
    path[len] = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<WCHAR> {
        s.encode_utf16().chain(core::iter::once(0)).collect()
    }


    #[test]
    fn collapses_dot_components_and_trailing_dots() {
        fn run(input: &str, mark: usize) -> String {
            let mut path: Vec<WCHAR> = input.encode_utf16().chain(core::iter::once(0)).collect();
            collapse_dos_path(&mut path, mark);
            let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
            String::from_utf16(&path[..end]).unwrap()
        }
        assert_eq!(run("C:/one/./two", 3), "C:\\\\one\\\\two");
        assert_eq!(run("C:\\\\one\\\\two\\\\..\\\\three", 3), "C:\\\\one\\\\three");
        assert_eq!(run("C:\\\\folder.\\\\file... ", 3), "C:\\\\folder\\\\file");
    }

    #[test]
    fn classifies_path_forms() {
        for (s, expected) in [
            ("file.txt", RtlPathTypeRelative),
            ("\\file.txt", RtlPathTypeRooted),
            ("C:\\file.txt", RtlPathTypeDriveAbsolute),
            ("C:file.txt", RtlPathTypeDriveRelative),
            ("\\\\server\\share", RtlPathTypeUncAbsolute),
            ("\\\\.\\PhysicalDrive0", RtlPathTypeLocalDevice),
            ("\\\\?\\C:\\file.txt", RtlPathTypeLocalDevice),
            ("\\\\.", RtlPathTypeRootLocalDevice),
        ] {
            let p = wide(s);
            assert_eq!(unsafe { RtlDetermineDosPathNameType_U(p.as_ptr()) }, expected);
        }
    }

    #[test]
    fn finds_device_names_and_offsets_in_bytes() {
        for (s, expected) in [
            ("CON", 0x0000_0006),
            ("c:\\temp\\nul.txt", 0x0010_0006),
            ("COM1.log", 0x0000_0008),
            ("LPT9:", 0x0000_0008),
            ("CONIN$", 0x0000_000c),
            ("CONOUT$", 0x0000_000e),
            ("\\\\.\\con", 0x0008_0006),
        ] {
            let p = wide(s);
            assert_eq!(unsafe { RtlIsDosDeviceName_U(p.as_ptr()) }, expected, "{s}");
        }
    }

    #[test]
    fn rejects_non_devices_and_unc_paths() {
        for s in ["console", "COM0", "COM10", "\\\\server\\CON", "NULx"] {
            let p = wide(s);
            assert_eq!(unsafe { RtlIsDosDeviceName_U(p.as_ptr()) }, 0, "{s}");
        }
    }
}

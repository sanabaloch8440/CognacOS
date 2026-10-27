//! Windows 11 x64 NTDLL path export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

#[no_mangle]
pub unsafe extern "system" fn RtlAppendPathElement() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAreLongPathsEnabled() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConstructCrossVmEventPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConstructCrossVmMutexPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosLongPathNameToNtPathName_U_WithStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosLongPathNameToRelativeNtPathName_U_WithStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosPathNameToNtPathName_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosPathNameToNtPathName_U_WithStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosPathNameToRelativeNtPathName_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosPathNameToRelativeNtPathName_U_WithStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosSearchPath_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosSearchPath_Ustr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFormatCurrentUserKeyPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetAppContainerNamedObjectPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetExePath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFileMUIPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFullPathName_UstrEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetLengthWithoutLastFullDosOrNtPathElement() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetLengthWithoutTrailingPathSeperators() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetLongestNtPathLength() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSearchPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetTokenNamedObjectPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNtPathNameToDosPathName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleasePath(path: *mut u16) {
    if path.is_null() { return; }
    let heap = unsafe { crate::rtl_heap::RtlGetProcessHeap() };
    if !heap.is_null() {
        let _ = unsafe { crate::rtl_heap::RtlFreeHeap(heap, 0, path.cast()) };
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleaseRelativeName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReplaceSystemDirectoryInPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetCurrentDirectory_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetSearchPathMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


// Consolidated DOS path classification, device-name and full-path helpers.
pub mod compat {
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
        if *ch == b'/' as WCHAR { *ch = b'\\' as WCHAR; }
    }

    // Compact duplicate separators without disturbing the protected prefix.
    let mut write = core::cmp::max(1, mark).min(len);
    for read in write..len {
        if path[read] != b'\\' as WCHAR || write == 0 || path[write - 1] != b'\\' as WCHAR {
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
            if next == b'\\' as WCHAR || next == 0 {
                let is_parent = next != 0 && path.get(p + 2).copied() == Some(b'.' as WCHAR)
                    && (path.get(p + 3).copied() == Some(b'\\' as WCHAR)
                        || path.get(p + 3).copied().unwrap_or(0) == 0);
                let is_current = next == b'\\' as WCHAR;
                if is_current || next == 0 || is_parent {
                    let mut start = p;
                    if is_parent {
                        if start > mark {
                            start -= 1;
                            while start > mark && path[start - 1] != b'\\' as WCHAR { start -= 1; }
                        }
                        let mut end = p + 3;
                        if path.get(end).copied() == Some(b'\\' as WCHAR) { end += 1; }
                        if path.get(p + 3).copied().unwrap_or(0) == 0 {
                            if start > mark && path[start - 1] == b'\\' as WCHAR { start -= 1; }
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
                        while end < len && path[end] == b'\\' as WCHAR { end += 1; }
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
        while p < len && path[p] != b'\\' as WCHAR { p += 1; }
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



const MAX_DOS_PATH: usize = 32768;
const PROCESS_PARAMETERS_OFFSET: usize = 0x20;
const CURRENT_DIRECTORY_OFFSET: usize = 0x38;

#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString64 {
    length: u16,
    maximum_length: u16,
    padding: u32,
    buffer: *const WCHAR,
}

extern "system" {
    fn RtlGetCurrentPeb() -> *mut core::ffi::c_void;
}

/// Read RTL_USER_PROCESS_PARAMETERS.CurrentDirectory.DosPath from the current
/// process PEB. This is the same user-visible structure initialized by
/// CognacOS in kernel/src/process.rs.
unsafe fn current_directory(dst: &mut [WCHAR; MAX_DOS_PATH]) -> usize {
    let peb = unsafe { RtlGetCurrentPeb() } as *const u8;
    if peb.is_null() { return 0; }
    let params = unsafe {
        core::ptr::read_unaligned(peb.add(PROCESS_PARAMETERS_OFFSET).cast::<*const u8>())
    };
    if params.is_null() { return 0; }
    let us = unsafe {
        core::ptr::read_unaligned(params.add(CURRENT_DIRECTORY_OFFSET).cast::<UnicodeString64>())
    };
    if us.buffer.is_null() || us.length == 0 || us.length as usize % 2 != 0 {
        return 0;
    }
    let count = core::cmp::min((us.length as usize) / 2, MAX_DOS_PATH - 1);
    for i in 0..count {
        dst[i] = unsafe { us.buffer.add(i).read() };
    }
    dst[count] = 0;
    count
}

fn append_slice(dst: &mut [WCHAR; MAX_DOS_PATH], len: &mut usize, src: &[WCHAR]) -> bool {
    if *len + src.len() >= MAX_DOS_PATH { return false; }
    dst[*len..*len + src.len()].copy_from_slice(src);
    *len += src.len();
    dst[*len] = 0;
    true
}

fn full_path_into(name: &[WCHAR], current: &[WCHAR], out: &mut [WCHAR; MAX_DOS_PATH]) -> Option<(usize, usize)> {
    if name.is_empty() || name.iter().all(|&c| c == 32) { return None; }
    let kind = {
        let mut tmp = [0u16; MAX_DOS_PATH];
        if name.len() >= MAX_DOS_PATH { return None; }
        tmp[..name.len()].copy_from_slice(name);
        unsafe { RtlDetermineDosPathNameType_U(tmp.as_ptr()) }
    };
    let mut len = 0usize;
    let mut mark = 0usize;
    match kind {
        RtlPathTypeUncAbsolute | RtlPathTypeLocalDevice => {
            if !append_slice(out, &mut len, name) { return None; }
            if kind == RtlPathTypeUncAbsolute {
                let mut i = 2;
                while i < len && !separator(out[i]) { i += 1; }
                while i < len && separator(out[i]) { i += 1; }
                while i < len && !separator(out[i]) { i += 1; }
                while i < len && separator(out[i]) { i += 1; }
                mark = i;
            } else { mark = 4.min(len); }
        }
        RtlPathTypeDriveAbsolute => {
            if !append_slice(out, &mut len, name) { return None; }
            mark = 3.min(len);
        }
        RtlPathTypeDriveRelative => {
            let same_drive = current.len() >= 2 && name.len() >= 2
                && fold(current[0]) == fold(name[0]) && current[1] == 58;
            if same_drive {
                if !append_slice(out, &mut len, current) { return None; }
                if len > 0 && !separator(out[len - 1]) && name.len() > 2 {
                    if !append_slice(out, &mut len, &[92]) { return None; }
                }
                if !append_slice(out, &mut len, &name[2..]) { return None; }
            } else {
                if !append_slice(out, &mut len, &name[..2]) || !append_slice(out, &mut len, &[92]) { return None; }
                if !append_slice(out, &mut len, &name[2..]) { return None; }
            }
            mark = 3.min(len);
        }
        RtlPathTypeRooted => {
            if current.len() >= 2 && current[1] == 58 {
                if !append_slice(out, &mut len, &current[..2]) { return None; }
                mark = 3;
            } else if current.len() >= 2 && separator(current[0]) && separator(current[1]) {
                if !append_slice(out, &mut len, current) { return None; }
                let mut i = 2;
                while i < len && !separator(out[i]) { i += 1; }
                while i < len && separator(out[i]) { i += 1; }
                while i < len && !separator(out[i]) { i += 1; }
                while i < len && separator(out[i]) { i += 1; }
                len = i;
                out[len] = 0;
                mark = len;
            } else { return None; }
            if !append_slice(out, &mut len, name) { return None; }
        }
        RtlPathTypeRelative => {
            if !append_slice(out, &mut len, current) { return None; }
            if len > 0 && !separator(out[len - 1]) {
                if !append_slice(out, &mut len, &[92]) { return None; }
            }
            if !append_slice(out, &mut len, name) { return None; }
            if current.len() >= 2 && current[1] == 58 {
                mark = 3.min(len);
            } else {
                let mut i = 2;
                while i < len && !separator(out[i]) { i += 1; }
                while i < len && separator(out[i]) { i += 1; }
                while i < len && !separator(out[i]) { i += 1; }
                while i < len && separator(out[i]) { i += 1; }
                mark = i;
            }
        }
        RtlPathTypeRootLocalDevice => {
            if !append_slice(out, &mut len, &[92, 92, 46, 92]) { return None; }
            if name.len() > 3 && !append_slice(out, &mut len, &name[3..]) { return None; }
            mark = 4;
        }
        _ => return None,
    }
    out[len] = 0;
    collapse_dos_path(&mut out[..len + 1], mark);
    let final_len = out.iter().position(|&c| c == 0).unwrap_or(MAX_DOS_PATH);
    let mut file_part = 0usize;
    let mut i = 0usize;
    while i < final_len {
        if out[i] == 92 { file_part = i + 1; }
        i += 1;
    }
    Some((final_len, file_part))
}

/// Returns the current DOS directory length in bytes, excluding the NUL.
/// On an insufficient buffer, returns the required size including the NUL.
#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentDirectory_U(buffer_length: ULONG, buffer: *mut WCHAR) -> ULONG {
    let mut current = [0u16; MAX_DOS_PATH];
    let len = unsafe { current_directory(&mut current) };
    if len == 0 { return 0; }
    let required = (len + 1) * 2;
    if buffer.is_null() || (buffer_length as usize) < required { return required as ULONG; }
    unsafe { core::ptr::copy_nonoverlapping(current.as_ptr(), buffer, len + 1); }
    (len * 2) as ULONG
}

/// Windows x64 ABI implementation of RtlGetFullPathName_UEx using the current
/// process parameters. This implementation does not perform filesystem checks
/// or per-drive environment-variable lookup.
#[no_mangle]
pub unsafe extern "system" fn RtlGetFullPathName_UEx(
    name: *const WCHAR, size: ULONG, buffer: *mut WCHAR,
    file_part: *mut *mut WCHAR, path_type: *mut RTL_PATH_TYPE,
) -> ULONG {
    if name.is_null() || unsafe { name.read() } == 0 { return 0; }
    if !file_part.is_null() { unsafe { file_part.write(core::ptr::null_mut()); } }
    let kind = unsafe { RtlDetermineDosPathNameType_U(name) };
    if !path_type.is_null() { unsafe { path_type.write(kind); } }

    let dosdev = unsafe { RtlIsDosDeviceName_U(name) };
    if dosdev != 0 {
        let offset = (dosdev >> 16) as usize / 2;
        let chars = (dosdev & 0xffff) as usize / 2;
        let required = 4 + chars + 1;
        if buffer.is_null() || (size as usize) < required * 2 { return ((required - 1) * 2 + 2) as ULONG; }
        let prefix = [92u16, 92, 46, 92];
        unsafe {
            core::ptr::copy_nonoverlapping(prefix.as_ptr(), buffer, 4);
            core::ptr::copy_nonoverlapping(name.add(offset), buffer.add(4), chars);
            buffer.add(4 + chars).write(0);
        }
        return ((required - 1) * 2) as ULONG;
    }

    let mut input = [0u16; MAX_DOS_PATH];
    let mut name_len = 0usize;
    while name_len < MAX_DOS_PATH - 1 {
        let ch = unsafe { name.add(name_len).read() };
        if ch == 0 { break; }
        input[name_len] = ch;
        name_len += 1;
    }
    if name_len == 0 || name_len == MAX_DOS_PATH - 1 { return 0; }
    let mut current = [0u16; MAX_DOS_PATH];
    let current_len = unsafe { current_directory(&mut current) };
    if current_len == 0 { return 0; }
    let mut output = [0u16; MAX_DOS_PATH];
    let Some((len, part)) = full_path_into(&input[..name_len], &current[..current_len], &mut output) else { return 0; };
    let required_bytes = ((len + 1) * 2) as ULONG;
    if buffer.is_null() || size < required_bytes { return required_bytes; }
    unsafe {
        core::ptr::copy_nonoverlapping(output.as_ptr(), buffer, len + 1);
        if !file_part.is_null() && part < len && output[part..len].iter().all(|&c| c != 0) {
            let mut last_sep = None;
            for i in 0..len { if output[i] == 92 { last_sep = Some(i); } }
            if let Some(i) = last_sep { if i + 1 < len { file_part.write(buffer.add(i + 1)); } }
        }
    }
    (len * 2) as ULONG
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFullPathName_U(
    name: *const WCHAR, size: ULONG, buffer: *mut WCHAR, file_part: *mut *mut WCHAR,
) -> ULONG {
    unsafe { RtlGetFullPathName_UEx(name, size, buffer, file_part, core::ptr::null_mut()) }
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
    fn full_path_uses_current_directory_base_and_normalizes() {
        let name: Vec<WCHAR> = "one\\\\..\\\\two.txt".encode_utf16().collect();
        let current: Vec<WCHAR> = "C:\\\\work\\\\app".encode_utf16().collect();
        let mut out = [0u16; MAX_DOS_PATH];
        let (len, _) = full_path_into(&name, &current, &mut out).unwrap();
        assert_eq!(String::from_utf16(&out[..len]).unwrap(), "C:\\\\work\\\\two.txt");

        let name: Vec<WCHAR> = "\\\\logs".encode_utf16().collect();
        let (len, _) = full_path_into(&name, &current, &mut out).unwrap();
        assert_eq!(String::from_utf16(&out[..len]).unwrap(), "C:\\\\logs");

        let name: Vec<WCHAR> = "C:temp.txt".encode_utf16().collect();
        let (len, _) = full_path_into(&name, &current, &mut out).unwrap();
        assert_eq!(String::from_utf16(&out[..len]).unwrap(), "C:\\\\work\\\\app\\\\temp.txt");

        let name: Vec<WCHAR> = "D:temp.txt".encode_utf16().collect();
        let (len, _) = full_path_into(&name, &current, &mut out).unwrap();
        assert_eq!(String::from_utf16(&out[..len]).unwrap(), "D:\\\\temp.txt");
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

}
pub use compat::{RtlDetermineDosPathNameType_U, RtlIsDosDeviceName_U, RtlGetCurrentDirectory_U, RtlGetFullPathName_UEx, RtlGetFullPathName_U};

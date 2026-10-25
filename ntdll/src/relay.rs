//! Partial portable Rust port of Wine's ntdll/relay.c.
//!
//! Implements relay descriptor layouts and include/exclude matching helpers.
//! This is diagnostic compatibility code, not part of the Windows 11 NTDLL ABI.
//! Architecture-specific assembly thunks and executable export-table patching are
//! intentionally kept out of the native runtime until a diagnostic relay is needed.
//! The enclosing Wine tree carries the LGPL-2.1-or-later license.

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use core::ffi::c_void;

pub type ULONG = u32;
pub type DWORD = u32;
pub type ULONG_PTR = usize;
pub type BOOL = i32;
pub type HMODULE = *mut c_void;
pub type FARPROC = *mut c_void;
pub type WCHAR = u16;
pub type PCWSTR = *const WCHAR;

pub const RELAY_DESCR_MAGIC: ULONG_PTR = 0xdeb9_0002;
pub const TRUE: BOOL = 1;
pub const FALSE: BOOL = 0;

#[repr(C)]
pub struct relay_descr {
    pub magic: ULONG_PTR,
    pub relay_call: *mut c_void,
    pub private: *mut c_void,
    pub entry_point_base: *const u8,
    pub entry_point_offsets: *const u32,
    pub args_string: *const u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct relay_descr_rva {
    pub magic: DWORD,
    pub descr: DWORD,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct relay_entry_point {
    pub orig_func: FARPROC,
    pub name: *const u8,
}

/// ASCII/UTF-16 comparison matching relay.c's strcmpAW: exact code-unit
/// comparison, independent of locale and current ANSI code page.
pub unsafe fn strcmp_aw(ascii: *const u8, wide: PCWSTR) -> i32 {
    if ascii.is_null() || wide.is_null() {
        return if ascii.is_null() { if wide.is_null() { 0 } else { -1 } } else { 1 };
    }
    let mut i = 0usize;
    loop {
        let a = unsafe { *ascii.add(i) } as i32;
        let w = unsafe { *wide.add(i) } as i32;
        if a != w { return a - w; }
        if a == 0 { return 0; }
        i += 1;
    }
}

/// Parses a semicolon-delimited UTF-16 function list into owned entries.
pub fn build_list(buffer: &[u16]) -> Vec<Vec<u16>> {
    let end = buffer.iter().position(|&ch| ch == 0).unwrap_or(buffer.len());
    let source = &buffer[..end];
    let mut result = Vec::new();
    let mut start = 0usize;
    for i in 0..=source.len() {
        if i == source.len() || source[i] == b';' as u16 {
            result.push(source[start..i].to_vec());
            start = i + 1;
        }
    }
    result
}

#[inline]
fn ascii_eq_wide(ascii: &[u8], wide: &[u16]) -> bool {
    ascii.len() == wide.len() && ascii.iter().zip(wide).all(|(&a, &w)| a as u16 == w)
}

#[inline]
fn wide_ascii_case_eq(a: &[u16], b: &[u16]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(&x, &y)| {
        let x = if (b'A' as u16..=b'Z' as u16).contains(&x) { x + 32 } else { x };
        let y = if (b'A' as u16..=b'Z' as u16).contains(&y) { y + 32 } else { y };
        x == y
    })
}

fn wide_eq_ascii_ignore_case(wide: &[u16], ascii: &[u8]) -> bool {
    wide.len() == ascii.len() && wide.iter().zip(ascii).all(|(&w, &a)| {
        let w = if (b'A' as u16..=b'Z' as u16).contains(&w) { w + 32 } else { w };
        let a = if a.is_ascii_uppercase() { a + 32 } else { a };
        w == a as u16
    })
}

/// Checks a module/function list. Entries may be module.function, module.*,
/// module.ordinal, or a function name alone.
pub fn check_list(module: &[u16], ordinal: u32, function: Option<&[u8]>, list: &[Vec<u16>]) -> bool {
    let ordinal_ascii = ordinal.to_string();
    let ordinal_bytes = ordinal_ascii.as_bytes();

    for item in list {
        if let Some(dot) = item.iter().rposition(|&c| c == b'.' as u16) {
            if dot == 0 { continue; }
            let module_part = &item[..dot];
            if !wide_eq_ascii_ignore_case(module_part, &module.iter().map(|&c| c as u8).collect::<Vec<_>>()) {
                continue;
            }
            let tail = &item[dot + 1..];
            if tail.len() == 1 && tail[0] == b'*' as u16 { return true; }
            if ascii_eq_wide(ordinal_bytes, tail) { return true; }
            if let Some(name) = function {
                if ascii_eq_wide(name, tail) { return true; }
            }
        } else if let Some(name) = function {
            if ascii_eq_wide(name, item) { return true; }
        }
    }
    false
}

/// Exclusion wins if both lists are provided, matching relay.c.
pub fn check_from_module(
    includelist: Option<&[Vec<u16>]>,
    excludelist: Option<&[Vec<u16>]>,
    module: Option<&[u16]>,
) -> bool {
    let Some(module) = module else { return true; };
    let (list, show_on_match) = if let Some(excludes) = excludelist {
        (excludes, false)
    } else if let Some(includes) = includelist {
        (includes, true)
    } else {
        return true;
    };

    for item in list {
        if wide_ascii_case_eq(item, module) { return show_on_match; }
        if module.len() == item.len() + 4
            && wide_ascii_case_eq(&module[..item.len()], item)
            && wide_ascii_case_eq(&module[item.len()..], &[b'.' as u16, b'd' as u16, b'l' as u16, b'l' as u16])
        {
            return show_on_match;
        }
    }
    !show_on_match
}

#[inline]
pub const fn is_ret_val(value: u8) -> bool {
    value >= b'A' && value <= b'Z'
}

/// Without validated relay metadata, preserve the original procedure address.
pub unsafe extern "system" fn RELAY_GetProcAddress(
    _module: HMODULE, _exports: *const c_void, _export_size: DWORD,
    proc: FARPROC, _ordinal: DWORD, _user: PCWSTR,
) -> FARPROC { proc }

/// Architecture-specific relay thunk installation is not implemented here.
pub unsafe extern "system" fn RELAY_SetupDLL(_module: HMODULE) {}

/// Conservative fallback used on platforms without the i386 snoop thunks.
pub unsafe extern "system" fn SNOOP_GetProcAddress(
    _module: HMODULE, _exports: *const c_void, _export_size: DWORD,
    original: FARPROC, _ordinal: DWORD, _user: PCWSTR,
) -> FARPROC { original }

pub unsafe extern "system" fn SNOOP_SetupDLL(_module: HMODULE) {}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayArgKind { Integer, Int64, Int128, AnsiString, WideString, Float, Double, ThisCall, FastCall }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayReturnKind { Integer, Int64, Float, Double, Pointer, Unknown(u8) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelaySignature {
    pub args: Vec<RelayArgKind>,
    pub return_kind: RelayReturnKind,
    pub uses_thiscall: bool,
    pub uses_fastcall: bool,
}

/// Decode Wine's compact signature format without invoking the target function.
pub fn decode_signature(signature: &[u8]) -> Option<RelaySignature> {
    let mut args = Vec::new();
    let mut uses_thiscall = false;
    let mut uses_fastcall = false;
    let mut return_kind = None;
    for &kind in signature {
        if is_ret_val(kind) {
            return_kind = Some(match kind {
                b'J' => RelayReturnKind::Int64,
                b'F' => RelayReturnKind::Float,
                b'D' => RelayReturnKind::Double,
                b'P' | b'S' | b'W' => RelayReturnKind::Pointer,
                b'I' | b'L' | b'H' | b'V' => RelayReturnKind::Integer,
                other => RelayReturnKind::Unknown(other),
            });
            break;
        }
        let decoded = match kind {
            b'j' => RelayArgKind::Int64,
            b'k' => RelayArgKind::Int128,
            b's' => RelayArgKind::AnsiString,
            b'w' => RelayArgKind::WideString,
            b'f' => RelayArgKind::Float,
            b'd' => RelayArgKind::Double,
            b't' => {
                if args.is_empty() { uses_thiscall = true; }
                else if uses_thiscall && args.len() == 1 { uses_fastcall = true; }
                RelayArgKind::ThisCall
            }
            _ => RelayArgKind::Integer,
        };
        args.push(decoded);
    }
    Some(RelaySignature { args, return_kind: return_kind?, uses_thiscall, uses_fastcall })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayDescriptorLocation { pub descriptor_rva: u32, pub descriptor_size: usize }

/// Bounds-checked lookup for the descriptor marker used by Wine relay-enabled
/// PE images. Input must be a mapped image and export metadata must be validated.
pub fn find_relay_descriptor(
    image: &[u8], export_rva: u32, export_size: u32,
    export_name_rva: u32, descriptor_size: usize,
) -> Option<RelayDescriptorLocation> {
    let name = export_name_rva as usize;
    let export_start = export_rva as usize;
    let export_end = export_start.checked_add(export_size as usize)?;
    if name < 8 || name > image.len() || export_end > image.len() { return None; }
    if name <= export_start || name >= export_end || name % core::mem::size_of::<u32>() != 0 {
        return None;
    }
    let marker = name.checked_sub(8)?;
    let magic = u32::from_le_bytes(image.get(marker..marker + 4)?.try_into().ok()?);
    if magic != RELAY_DESCR_MAGIC as u32 { return None; }
    let descr_rva = u32::from_le_bytes(image.get(marker + 4..marker + 8)?.try_into().ok()?);
    let target = if descr_rva != 0 { descr_rva as usize } else { export_end };
    let end = target.checked_add(descriptor_size)?;
    if descriptor_size < 4 || end > image.len() { return None; }
    let descr_magic = u32::from_le_bytes(image.get(target..target + 4)?.try_into().ok()?);
    if descr_magic != RELAY_DESCR_MAGIC as u32 { return None; }
    Some(RelayDescriptorLocation { descriptor_rva: target as u32, descriptor_size })
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelayValue { Integer(u64), Int64(u64), Int128([u32; 4]), Pointer(usize), FloatBits(u32), DoubleBits(u64) }

/// Decode common 64-bit word values for diagnostic display. Does not dereference
/// string pointers; ABI-specific register and stack reconstruction is separate.
pub fn decode_word_values(signature: &RelaySignature, words: &[u64]) -> Option<Vec<RelayValue>> {
    if words.len() < signature.args.len() { return None; }
    let mut out = Vec::with_capacity(signature.args.len());
    for (i, kind) in signature.args.iter().enumerate() {
        let word = words[i];
        out.push(match kind {
            RelayArgKind::AnsiString | RelayArgKind::WideString | RelayArgKind::ThisCall | RelayArgKind::FastCall => RelayValue::Pointer(word as usize),
            RelayArgKind::Int64 => RelayValue::Int64(word),
            RelayArgKind::Int128 => {
                let next = *words.get(i + 1)?;
                RelayValue::Int128([word as u32, (word >> 32) as u32, next as u32, (next >> 32) as u32])
            }
            RelayArgKind::Float => RelayValue::FloatBits(word as u32),
            RelayArgKind::Double => RelayValue::DoubleBits(word),
            RelayArgKind::Integer => RelayValue::Integer(word),
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> { s.encode_utf16().collect() }

    #[test]
    fn parses_semicolon_separated_lists() {
        assert_eq!(build_list(&w("foo;bar;baz")), vec![w("foo"), w("bar"), w("baz")]);
    }

    #[test]
    fn parses_trailing_empty_item_like_source() {
        assert_eq!(build_list(&w("foo;")), vec![w("foo"), Vec::<u16>::new()]);
    }

    #[test]
    fn return_type_is_uppercase() {
        assert!(is_ret_val(b'I'));
        assert!(!is_ret_val(b'i'));
        assert!(!is_ret_val(0));
    }

    #[test]
    fn include_exclude_policy_prefers_exclude_list() {
        let include = vec![w("app.exe")];
        let exclude = vec![w("kernel32")];
        assert!(!check_from_module(Some(&include), Some(&exclude), Some(&w("kernel32"))));
        assert!(check_from_module(Some(&include), Some(&exclude), Some(&w("app.exe"))));
    }

    #[test]
    fn decodes_signature_arguments_and_return_marker() {
        let sig = decode_signature(b"isdJ").unwrap();
        assert_eq!(sig.args, vec![RelayArgKind::Integer, RelayArgKind::AnsiString, RelayArgKind::Double]);
        assert_eq!(sig.return_kind, RelayReturnKind::Int64);
    }

    #[test]
    fn malformed_signature_without_return_marker_is_rejected() {
        assert!(decode_signature(b"isd").is_none());
    }

    #[test]
    fn descriptor_locator_rejects_bad_magic_and_out_of_bounds() {
        let mut image = vec![0u8; 128];
        image[24..28].copy_from_slice(&(RELAY_DESCR_MAGIC as u32).to_le_bytes());
        image[28..32].copy_from_slice(&64u32.to_le_bytes());
        image[64..68].copy_from_slice(&(RELAY_DESCR_MAGIC as u32).to_le_bytes());
        assert_eq!(find_relay_descriptor(&image, 8, 48, 32, 16), Some(RelayDescriptorLocation { descriptor_rva: 64, descriptor_size: 16 }));
        assert!(find_relay_descriptor(&image, 8, 48, 32, 80).is_none());
        image[24] = 0;
        assert!(find_relay_descriptor(&image, 8, 48, 32, 16).is_none());
    }

    #[test]
    fn selects_relay_thunk_only_when_available_and_requested() {
        assert_eq!(select_proc_address(0x1000, 0x2000, true, true), 0x2000);
        assert_eq!(select_proc_address(0x1000, 0x2000, false, true), 0x1000);
        assert_eq!(select_proc_address(0x1000, 0x2000, true, false), 0x1000);
    }

    #[test]
    fn no_filter_means_relay_is_allowed() {
        assert!(check_from_module(None, None, Some(&w("any.dll"))));
        assert!(check_from_module(None, None, None));
    }
}

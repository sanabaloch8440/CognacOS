//! Windows 11 x64 NTDLL atom routines.
//!
//! Implementations in this file use documented native ABI shapes where available.
//! Atom-table syscalls remain separate until the CognacOS object/atom service exists.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;

type NTSTATUS = i32;
type USHORT = u16;
type ULONG = u32;
type PWSTR = *const u16;
type PULONG = *mut ULONG;
type PVOID = *mut c_void;

const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC0000002u32 as i32;

#[no_mangle]
pub unsafe extern "system" fn RtlGetIntegerAtom(string: PWSTR, atom: *mut USHORT) -> u8 {
    if string.is_null() || atom.is_null() {
        return 0;
    }

    // Integer-atom syntax is "#<decimal>"; bound the scan to prevent unbounded reads.
    if unsafe { *string } != b'#' as u16 {
        return 0;
    }

    let mut value: u32 = 0;
    let mut digits = 0usize;
    let mut index = 1usize;
    loop {
        if index > 5 {
            return 0;
        }
        let ch = unsafe { *string.add(index) };
        if ch == 0 {
            break;
        }
        if !(b'0' as u16..=b'9' as u16).contains(&ch) {
            return 0;
        }
        value = match value.checked_mul(10).and_then(|v| v.checked_add((ch - b'0' as u16) as u32)) {
            Some(v) => v,
            None => return 0,
        };
        digits += 1;
        index += 1;
    }

    if digits == 0 || value == 0 || value > 0xBFFF {
        return 0;
    }

    unsafe { atom.write(value as USHORT) };
    1
}

#[no_mangle]
pub unsafe extern "system" fn NtAddAtom(_name: PWSTR, _length: u32, _atom: *mut USHORT) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAddAtomEx(_name: PWSTR, _length: u32, _atom: *mut USHORT, _flags: ULONG) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDeleteAtom(_atom: USHORT) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFindAtom(_name: PWSTR, _length: u32, _atom: *mut USHORT) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationAtom(
    _atom: USHORT,
    _information_class: ULONG,
    _information: PVOID,
    _information_length: ULONG,
    _return_length: PULONG,
) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

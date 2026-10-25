/*
 * Rust source port of Wine dlls/ntdll/env.c (environment and process-parameter
 * routines). Based on Wine's LGPL-2.1-or-later source.
 *
 * This is a staged source-only port. The functions below implement the
 * environment-block primitives whose ABI can be expressed without assuming a
 * CognacOS-specific TEB/PEB layout. PEB-backed operations and full process
 * parameter allocation remain to be ported before this file is complete.
 */

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use core::ffi::c_void;
use core::ptr::{null, null_mut};

type WCHAR = u16;
type ULONG = u32;
type USHORT = u16;
type SIZE_T = usize;
type NTSTATUS = i32;
type BOOLEAN = u8;
type HANDLE = *mut c_void;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const STATUS_VARIABLE_NOT_FOUND: NTSTATUS = 0xC000_0100u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_INVALID_PARAMETER_1: NTSTATUS = 0xC000_00EFu32 as i32;
const HEAP_ZERO_MEMORY: ULONG = 0x0000_0008;
const PROCESS_PARAMS_FLAG_NORMALIZED: ULONG = 0x0000_0001;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UNICODE_STRING {
    pub Length: USHORT,
    pub MaximumLength: USHORT,
    pub Buffer: *mut WCHAR,
}

#[repr(C)]
pub struct RTL_DRIVE_LETTER_CURDIR {
    pub Flags: USHORT,
    pub Length: USHORT,
    pub TimeStamp: ULONG,
    pub DosPath: UNICODE_STRING,
}

#[repr(C)]
pub struct RTL_USER_PROCESS_PARAMETERS {
    pub MaximumLength: ULONG,
    pub Length: ULONG,
    pub Flags: ULONG,
    pub DebugFlags: ULONG,
    pub ConsoleHandle: HANDLE,
    pub ConsoleFlags: ULONG,
    pub StandardInput: HANDLE,
    pub StandardOutput: HANDLE,
    pub StandardError: HANDLE,
    pub CurrentDirectory: RTL_DRIVE_LETTER_CURDIR,
    pub DllPath: UNICODE_STRING,
    pub ImagePathName: UNICODE_STRING,
    pub CommandLine: UNICODE_STRING,
    pub Environment: *mut WCHAR,
    // The full Windows structure has more fields. These members are not used
    // by the normalize/denormalize helpers below.
}

extern "system" {
    fn GetProcessHeap() -> HANDLE;
    fn RtlAllocateHeap(heap: HANDLE, flags: ULONG, size: SIZE_T) -> *mut c_void;
    fn RtlFreeHeap(heap: HANDLE, flags: ULONG, memory: *mut c_void) -> BOOLEAN;
    fn RtlCompareUnicodeStrings(
        first: *const WCHAR, first_len: SIZE_T,
        second: *const WCHAR, second_len: SIZE_T, case_insensitive: BOOLEAN,
    ) -> i32;
}

#[inline]
unsafe fn alloc(size: usize, zero: bool) -> *mut c_void {
    RtlAllocateHeap(GetProcessHeap(), if zero { HEAP_ZERO_MEMORY } else { 0 }, size)
}

#[inline]
unsafe fn free(ptr: *mut c_void) {
    if !ptr.is_null() {
        let _ = RtlFreeHeap(GetProcessHeap(), 0, ptr);
    }
}

#[inline]
unsafe fn wcslen16(mut p: *const WCHAR) -> usize {
    let start = p;
    while !p.is_null() && *p != 0 { p = p.add(1); }
    p.offset_from(start) as usize
}

/// Return the number of UTF-16 code units in a double-NUL-terminated block,
/// including the final NUL. A null block is treated as empty defensively.
unsafe fn env_length(env: *const WCHAR) -> usize {
    if env.is_null() { return 1; }
    let mut p = env;
    while *p != 0 {
        p = p.add(wcslen16(p) + 1);
    }
    p.offset_from(env) as usize + 1
}

/// Find a variable value in a Windows environment block. Names are compared
/// case-insensitively and embedded '=' characters in names are not accepted,
/// except for a leading '=' used by drive-current-directory entries.
unsafe fn find_variable(env: *const WCHAR, name: *const WCHAR, name_len: usize) -> *const WCHAR {
    if env.is_null() || name.is_null() || name_len == 0 { return null(); }
    let mut entry = env;
    while *entry != 0 {
        let entry_len = wcslen16(entry);
        if entry_len > name_len
            && *entry.add(name_len) == b'=' as WCHAR
            && RtlCompareUnicodeStrings(entry, name_len, name, name_len, 1) == 0
        {
            let mut eq_count = 0usize;
            let mut i = 0usize;
            while i < entry_len {
                if *entry.add(i) == b'=' as WCHAR { eq_count += 1; }
                i += 1;
            }
            // A leading '=' is allowed, but the separator must be the only
            // '=' after the beginning of the variable name.
            if eq_count == 1 || (*entry == b'=' as WCHAR && eq_count == 2) {
                return entry.add(name_len + 1);
            }
        }
        entry = entry.add(entry_len + 1);
    }
    null()
}

/// RtlCreateEnvironment for a fresh, empty environment block. Inheriting the
/// process environment requires CognacOS's PEB/process-parameter integration.
#[no_mangle]
pub unsafe extern "system" fn RtlCreateEnvironment(
    inherit: BOOLEAN,
    environment: *mut *mut WCHAR,
) -> NTSTATUS {
    if environment.is_null() { return STATUS_INVALID_PARAMETER; }
    *environment = null_mut();
    if inherit != 0 {
        // Do not silently fabricate an inherited environment without a native
        // current-process environment accessor.
        return STATUS_INVALID_PARAMETER;
    }
    let p = alloc(core::mem::size_of::<WCHAR>(), true).cast::<WCHAR>();
    if p.is_null() { return STATUS_NO_MEMORY; }
    *environment = p;
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyEnvironment(environment: *mut WCHAR) -> NTSTATUS {
    free(environment.cast::<c_void>());
    STATUS_SUCCESS
}

/// RtlQueryEnvironmentVariable_U. For now, callers must pass an explicit
/// environment block; null (the current process environment) needs PEB support.
#[no_mangle]
pub unsafe extern "system" fn RtlQueryEnvironmentVariable_U(
    environment: *mut WCHAR,
    name: *const UNICODE_STRING,
    value: *mut UNICODE_STRING,
) -> NTSTATUS {
    if name.is_null() || value.is_null() || (*name).Buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    (*value).Length = 0;
    if (*name).Length == 0 { return STATUS_VARIABLE_NOT_FOUND; }
    if environment.is_null() { return STATUS_INVALID_PARAMETER; }

    let name_len = ((*name).Length as usize) / 2;
    let found = find_variable(environment, (*name).Buffer, name_len);
    if found.is_null() { return STATUS_VARIABLE_NOT_FOUND; }

    let len = wcslen16(found);
    let byte_len = len.saturating_mul(2);
    if byte_len > USHORT::MAX as usize { return STATUS_BUFFER_TOO_SMALL; }
    (*value).Length = byte_len as USHORT;

    if byte_len > (*value).MaximumLength as usize {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if (*value).Buffer.is_null() && byte_len != 0 {
        (*value).Length = 0;
        return STATUS_INVALID_PARAMETER;
    }
    let copy_units = core::cmp::min(len + 1, (*value).MaximumLength as usize / 2);
    if copy_units != 0 {
        core::ptr::copy(found, (*value).Buffer, copy_units);
    }
    STATUS_SUCCESS
}

/// RtlQueryEnvironmentVariable. Return length is measured in WCHAR units,
/// matching the native API. Null environment/current-process lookup is deferred.
#[no_mangle]
pub unsafe extern "system" fn RtlQueryEnvironmentVariable(
    environment: *mut WCHAR,
    name: *const WCHAR,
    name_len: SIZE_T,
    value: *mut WCHAR,
    value_length: SIZE_T,
    return_length: *mut SIZE_T,
) -> NTSTATUS {
    if !return_length.is_null() { *return_length = 0; }
    if name.is_null() || name_len == 0 { return STATUS_VARIABLE_NOT_FOUND; }
    if environment.is_null() { return STATUS_INVALID_PARAMETER; }

    let found = find_variable(environment, name, name_len);
    if found.is_null() { return STATUS_VARIABLE_NOT_FOUND; }

    let len = wcslen16(found);
    if !return_length.is_null() { *return_length = len; }
    if len > value_length { 
        if !return_length.is_null() { *return_length = len + 1; }
        return STATUS_BUFFER_TOO_SMALL;
    }
    if value.is_null() && value_length != 0 { return STATUS_INVALID_PARAMETER; }
    let copy_units = core::cmp::min(len + 1, value_length);
    if copy_units != 0 { core::ptr::copy(found, value, copy_units); }
    STATUS_SUCCESS
}

#[inline]
unsafe fn normalize_pointer(base: *mut u8, field: *mut *mut WCHAR) {
    let value = *field;
    if !value.is_null() {
        *field = base.add(value as usize).cast::<WCHAR>();
    }
}

#[inline]
unsafe fn denormalize_pointer(base: *const u8, field: *mut *mut WCHAR) {
    let value = *field;
    if !value.is_null() {
        *field = (value as usize).wrapping_sub(base as usize) as *mut WCHAR;
    }
}

/// Convert the string pointers in the supported process-parameter prefix from
/// offsets to absolute pointers. Full ABI layout integration is still needed.
#[no_mangle]
pub unsafe extern "system" fn RtlNormalizeProcessParams(
    params: *mut RTL_USER_PROCESS_PARAMETERS,
) -> *mut RTL_USER_PROCESS_PARAMETERS {
    if params.is_null() || ((*params).Flags & PROCESS_PARAMS_FLAG_NORMALIZED) != 0 {
        return params;
    }
    let base = params.cast::<u8>();
    normalize_pointer(base, &mut (*params).CurrentDirectory.DosPath.Buffer);
    normalize_pointer(base, &mut (*params).DllPath.Buffer);
    normalize_pointer(base, &mut (*params).ImagePathName.Buffer);
    normalize_pointer(base, &mut (*params).CommandLine.Buffer);
    (*params).Flags |= PROCESS_PARAMS_FLAG_NORMALIZED;
    params
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeNormalizeProcessParams(
    params: *mut RTL_USER_PROCESS_PARAMETERS,
) -> *mut RTL_USER_PROCESS_PARAMETERS {
    if params.is_null() || ((*params).Flags & PROCESS_PARAMS_FLAG_NORMALIZED) == 0 {
        return params;
    }
    let base = params.cast::<u8>();
    denormalize_pointer(base, &mut (*params).CurrentDirectory.DosPath.Buffer);
    denormalize_pointer(base, &mut (*params).DllPath.Buffer);
    denormalize_pointer(base, &mut (*params).ImagePathName.Buffer);
    denormalize_pointer(base, &mut (*params).CommandLine.Buffer);
    (*params).Flags &= !PROCESS_PARAMS_FLAG_NORMALIZED;
    params
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyProcessParameters(
    params: *mut RTL_USER_PROCESS_PARAMETERS,
) {
    free(params.cast::<c_void>());
}

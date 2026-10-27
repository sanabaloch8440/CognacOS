/*
 * Rust source port of Wine dlls/ntdll/env.c process-parameter routines. Based on Wine's LGPL-2.1-or-later source.
 *
 * This module contains process-parameter and current-process environment query routines; environment-block creation and mutation live in env.rs.
 * Operations needing the live CognacOS TEB/PEB are kept explicit/deferred rather
 * than guessing its layout. Build integration and ABI verification remain pending.
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
    pub StartingX: ULONG,
    pub StartingY: ULONG,
    pub CountX: ULONG,
    pub CountY: ULONG,
    pub CountCharsX: ULONG,
    pub CountCharsY: ULONG,
    pub FillAttribute: ULONG,
    pub WindowFlags: ULONG,
    pub ShowWindowFlags: ULONG,
    pub WindowTitle: UNICODE_STRING,
    pub Desktop: UNICODE_STRING,
    pub ShellInfo: UNICODE_STRING,
    pub RuntimeInfo: UNICODE_STRING,
    pub CurrentDirectories: [RTL_DRIVE_LETTER_CURDIR; 32],
    pub EnvironmentSize: SIZE_T,
    pub EnvironmentVersion: SIZE_T,
    pub PackageDependencyData: *mut c_void,
    pub ProcessGroupId: ULONG,
    pub LoaderThreads: ULONG,
}

extern "system" {
    pub(super) fn RtlGetCurrentPeb() -> *mut c_void;
    pub(super) fn RtlAcquirePebLock();
    pub(super) fn RtlReleasePebLock();
        fn RtlAllocateHeap(heap: HANDLE, flags: ULONG, size: SIZE_T) -> *mut c_void;
    fn RtlFreeHeap(heap: HANDLE, flags: ULONG, memory: *mut c_void) -> BOOLEAN;
    fn RtlCompareUnicodeStrings(
        first: *const WCHAR, first_len: SIZE_T,
        second: *const WCHAR, second_len: SIZE_T, case_insensitive: BOOLEAN,
    ) -> i32;
}



/// Locate PEB.ProcessParameters.Environment using documented x64 ABI offsets.
/// Concurrent access to the returned slot requires PEB.FastPebLock.
#[inline]
pub(super) unsafe fn current_environment_slot() -> *mut *mut WCHAR {
    let peb = RtlGetCurrentPeb().cast::<u8>();
    if peb.is_null() { return null_mut(); }
    let params = core::ptr::read_unaligned(peb.add(0x20).cast::<*mut u8>());
    if params.is_null() { return null_mut(); }
    params.add(0x80).cast::<*mut WCHAR>()
}

#[inline]
unsafe fn alloc(size: usize, zero: bool) -> *mut c_void {
    RtlAllocateHeap(crate::rtl_heap::RtlGetProcessHeap(), if zero { HEAP_ZERO_MEMORY } else { 0 }, size)
}

#[inline]
unsafe fn free(ptr: *mut c_void) {
    if !ptr.is_null() {
        let _ = RtlFreeHeap(crate::rtl_heap::RtlGetProcessHeap(), 0, ptr);
    }
}

#[inline]
pub(super) unsafe fn wcslen16(mut p: *const WCHAR) -> usize {
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
pub(super) unsafe fn find_variable(env: *const WCHAR, name: *const WCHAR, name_len: usize) -> *const WCHAR {
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
        RtlAcquirePebLock();
        let slot = current_environment_slot();
        if slot.is_null() || (*slot).is_null() {
            RtlReleasePebLock();
            return STATUS_INVALID_PARAMETER;
        }
        let source = *slot;
        let units = env_length(source);
        let copy = alloc(units.saturating_mul(core::mem::size_of::<WCHAR>()), false).cast::<WCHAR>();
        if copy.is_null() {
            RtlReleasePebLock();
            return STATUS_NO_MEMORY;
        }
        core::ptr::copy_nonoverlapping(source, copy, units);
        RtlReleasePebLock();
        *environment = copy;
        return STATUS_SUCCESS;
    }
    let p = alloc(core::mem::size_of::<WCHAR>(), true).cast::<WCHAR>();
    if p.is_null() { return STATUS_NO_MEMORY; }
    *environment = p;
    STATUS_SUCCESS
}

// Compatibility helper for freeing a process-parameter environment block. Canonical environment-block exports live in env.rs.
pub unsafe extern "system" fn RtlDestroyEnvironment(environment: *mut WCHAR) -> NTSTATUS {
    free(environment.cast::<c_void>());
    STATUS_SUCCESS
}



/// Replace the current process environment while holding PEB.FastPebLock.
/// When old_environment is null, the previous block is destroyed after swap.
#[no_mangle]
pub unsafe extern "system" fn RtlSetCurrentEnvironment(
    new_environment: *mut WCHAR,
    old_environment: *mut *mut WCHAR,
) {
    RtlAcquirePebLock();
    let slot = current_environment_slot();
    if slot.is_null() {
        RtlReleasePebLock();
        return;
    }
    let previous = *slot;
    *slot = new_environment;
    let peb = RtlGetCurrentPeb().cast::<u8>();
    let params = core::ptr::read_unaligned(peb.add(0x20).cast::<*mut u8>());
    if !params.is_null() {
        // RTL_USER_PROCESS_PARAMETERS.EnvironmentSize is at x64 offset 0x3f0.
        let size_slot = params.add(0x3f0).cast::<usize>();
        if new_environment.is_null() {
            *size_slot = 0;
        } else {
            *size_slot = env_length(new_environment).saturating_mul(core::mem::size_of::<WCHAR>());
        }
    }
    RtlReleasePebLock();
    if !old_environment.is_null() {
        *old_environment = previous;
    } else if !previous.is_null() && previous != new_environment {
        let _ = RtlDestroyEnvironment(previous);
    }
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
    normalize_pointer(base, &mut (*params).WindowTitle.Buffer);
    normalize_pointer(base, &mut (*params).Desktop.Buffer);
    normalize_pointer(base, &mut (*params).ShellInfo.Buffer);
    normalize_pointer(base, &mut (*params).RuntimeInfo.Buffer);
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
    denormalize_pointer(base, &mut (*params).WindowTitle.Buffer);
    denormalize_pointer(base, &mut (*params).Desktop.Buffer);
    denormalize_pointer(base, &mut (*params).ShellInfo.Buffer);
    denormalize_pointer(base, &mut (*params).RuntimeInfo.Buffer);
    (*params).Flags &= !PROCESS_PARAMS_FLAG_NORMALIZED;
    params
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyProcessParameters(
    params: *mut RTL_USER_PROCESS_PARAMETERS,
) {
    free(params.cast::<c_void>());
}


/// Set or remove a variable in an explicit block, or the current PEB environment when environment is null.
unsafe fn set_environment_variable_impl(
    environment: *mut *mut WCHAR,
    name: *const UNICODE_STRING,
    value: *const UNICODE_STRING,
) -> NTSTATUS {
    if name.is_null() || (*name).Buffer.is_null() || (*name).Length == 0
        || ((*name).Length as usize) % 2 != 0
    {
        return STATUS_INVALID_PARAMETER_1;
    }
    if environment.is_null() || (*environment).is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let name_len = (*name).Length as usize / 2;
    let name_ptr = (*name).Buffer;
    let mut i = 1usize;
    while i < name_len {
        if *name_ptr.add(i) == b'=' as WCHAR { return STATUS_INVALID_PARAMETER; }
        i += 1;
    }
    if !value.is_null() && ((*value).Length > 0 && (*value).Buffer.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }

    let env = *environment;
    let old_units = env_length(env);
    let mut cursor = env;
    let mut found = null_mut::<WCHAR>();
    while !cursor.is_null() && *cursor != 0 {
        let entry_len = wcslen16(cursor);
        if entry_len > name_len && *cursor.add(name_len) == b'=' as WCHAR
            && RtlCompareUnicodeStrings(cursor, name_len, name_ptr, name_len, 1) == 0
        {
            found = cursor;
            break;
        }
        cursor = cursor.add(entry_len + 1);
    }

    if value.is_null() && found.is_null() { return STATUS_SUCCESS; }
    let prefix_units = if found.is_null() { old_units - 1 } else { found.offset_from(env) as usize };
    let removed_units = if found.is_null() { 0 } else { wcslen16(found) + 1 };
    let suffix_start = if found.is_null() { env.add(old_units - 1) } else { found.add(removed_units) };
    let suffix_units = old_units.saturating_sub(suffix_start.offset_from(env) as usize);
    let new_entry_units = if value.is_null() { 0 } else { name_len + 1 + ((*value).Length as usize / 2) + 1 };
    let new_units = prefix_units + new_entry_units + suffix_units;
    let old_bytes = old_units.saturating_mul(2);
    let new_bytes = new_units.saturating_mul(2);
    let heap = crate::rtl_heap::RtlGetProcessHeap();
    let replacement = alloc(new_bytes.max(2), false).cast::<WCHAR>();
    if replacement.is_null() { return STATUS_NO_MEMORY; }

    core::ptr::copy_nonoverlapping(env, replacement, prefix_units);
    let mut out = replacement.add(prefix_units);
    if !value.is_null() {
        core::ptr::copy_nonoverlapping(name_ptr, out, name_len);
        out = out.add(name_len);
        *out = b'=' as WCHAR;
        out = out.add(1);
        let value_len = (*value).Length as usize / 2;
        if value_len != 0 { core::ptr::copy_nonoverlapping((*value).Buffer, out, value_len); }
        out = out.add(value_len);
        *out = 0;
    }
    core::ptr::copy_nonoverlapping(suffix_start, replacement.add(prefix_units + new_entry_units), suffix_units);
    let _ = old_bytes;
    free(env.cast::<c_void>());
    *environment = replacement;
    let _ = heap;
    STATUS_SUCCESS
}


/// Set or remove a variable in an explicit environment block, or in the current process environment when environment is null.
#[no_mangle]
pub unsafe extern "system" fn RtlSetEnvironmentVariable(
    environment: *mut *mut WCHAR,
    name: *const UNICODE_STRING,
    value: *const UNICODE_STRING,
) -> NTSTATUS {
    if !environment.is_null() {
        return set_environment_variable_impl(environment, name, value);
    }
    RtlAcquirePebLock();
    let slot = current_environment_slot();
    let status = if slot.is_null() {
        STATUS_INVALID_PARAMETER
    } else {
        set_environment_variable_impl(slot, name, value)
    };
    RtlReleasePebLock();
    status
}

/// Set the process-parameter environment pointer and optionally return the old
/// block. This helper operates on an explicitly supplied parameter structure.
pub unsafe extern "system" fn CognacRtlSetProcessParametersEnvironment(
    params: *mut RTL_USER_PROCESS_PARAMETERS,
    new_environment: *mut WCHAR,
    old_environment: *mut *mut WCHAR,
) -> NTSTATUS {
    if params.is_null() { return STATUS_INVALID_PARAMETER; }
    let previous = (*params).Environment;
    (*params).Environment = new_environment;
    (*params).EnvironmentSize = if new_environment.is_null() { 0 } else { env_length(new_environment) * 2 };
    if !old_environment.is_null() { *old_environment = previous; }
    else { let _ = RtlDestroyEnvironment(previous); }
    STATUS_SUCCESS
}

#[inline]
unsafe fn rounded(size: usize, align: usize) -> usize {
    size.saturating_add(align - 1) & !(align - 1)
}

unsafe fn append_us(data: &mut *mut u8, source: *const UNICODE_STRING, target: *mut UNICODE_STRING, align: usize) {
    if source.is_null() {
        (*target).Length = 0; (*target).MaximumLength = 0; (*target).Buffer = null_mut(); return;
    }
    (*target).Length = (*source).Length;
    (*target).MaximumLength = (*source).MaximumLength;
    if (*source).MaximumLength == 0 { (*target).Buffer = null_mut(); return; }
    (*target).Buffer = *data as *mut WCHAR;
    if (*source).Length != 0 && !(*source).Buffer.is_null() {
        core::ptr::copy_nonoverlapping((*source).Buffer, (*target).Buffer, (*source).Length as usize / 2);
    }
    *data = (*data).add(rounded((*source).MaximumLength as usize, align));
}

/// Build an in-allocation, self-contained process-parameter block from the
/// supplied strings. This variant intentionally requires explicit inputs;
/// inheriting current PEB values is left for CognacOS PEB integration.
#[no_mangle]
pub unsafe extern "system" fn RtlCreateProcessParametersEx(
    result: *mut *mut RTL_USER_PROCESS_PARAMETERS,
    image: *const UNICODE_STRING,
    dll_path: *const UNICODE_STRING,
    current_directory: *const UNICODE_STRING,
    command_line: *const UNICODE_STRING,
    environment: *mut WCHAR,
    window_title: *const UNICODE_STRING,
    desktop: *const UNICODE_STRING,
    shell_info: *const UNICODE_STRING,
    runtime_info: *const UNICODE_STRING,
    flags: ULONG,
) -> NTSTATUS {
    if result.is_null() || image.is_null() || (*image).Buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    *result = null_mut();
    let empty = UNICODE_STRING { Length: 0, MaximumLength: 0, Buffer: null_mut() };
    let strings = [image, if dll_path.is_null() { &empty } else { &*dll_path },
        if current_directory.is_null() { &empty } else { &*current_directory },
        if command_line.is_null() { image } else { &*command_line },
        if window_title.is_null() { &empty } else { &*window_title },
        if desktop.is_null() { &empty } else { &*desktop },
        if shell_info.is_null() { &empty } else { &*shell_info },
        if runtime_info.is_null() { &empty } else { &*runtime_info }];
    let align = core::mem::size_of::<usize>();
    let header = core::mem::size_of::<RTL_USER_PROCESS_PARAMETERS>();
    let mut size = header;
    for item in strings { size = size.saturating_add(rounded(unsafe { (*item).MaximumLength } as usize, align)); }
    let env_units = if environment.is_null() { 0 } else { env_length(environment) };
    size = size.saturating_add(rounded(env_units * 2, align));
    let params = alloc(size, true).cast::<RTL_USER_PROCESS_PARAMETERS>();
    if params.is_null() { return STATUS_NO_MEMORY; }
    (*params).MaximumLength = size.min(ULONG::MAX as usize) as ULONG;
    (*params).Length = header.min(ULONG::MAX as usize) as ULONG;
    (*params).Flags = PROCESS_PARAMS_FLAG_NORMALIZED;
    (*params).EnvironmentSize = env_units * 2;
    let mut data = params.add(1).cast::<u8>();
    append_us(&mut data, strings[2], &mut (*params).CurrentDirectory.DosPath, align);
    append_us(&mut data, strings[1], &mut (*params).DllPath, align);
    append_us(&mut data, strings[0], &mut (*params).ImagePathName, align);
    append_us(&mut data, strings[3], &mut (*params).CommandLine, align);
    append_us(&mut data, strings[4], &mut (*params).WindowTitle, align);
    append_us(&mut data, strings[5], &mut (*params).Desktop, align);
    append_us(&mut data, strings[6], &mut (*params).ShellInfo, align);
    append_us(&mut data, strings[7], &mut (*params).RuntimeInfo, align);
    if env_units != 0 {
        (*params).Environment = data.cast::<WCHAR>();
        core::ptr::copy_nonoverlapping(environment, (*params).Environment, env_units);
    }
    if flags & PROCESS_PARAMS_FLAG_NORMALIZED == 0 { let _ = RtlDeNormalizeProcessParams(params); }
    *result = params;
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateProcessParameters(
    result: *mut *mut RTL_USER_PROCESS_PARAMETERS,
    image: *const UNICODE_STRING,
    dll_path: *const UNICODE_STRING,
    current_directory: *const UNICODE_STRING,
    command_line: *const UNICODE_STRING,
    environment: *mut WCHAR,
    window_title: *const UNICODE_STRING,
    desktop: *const UNICODE_STRING,
    shell_info: *const UNICODE_STRING,
    runtime_info: *const UNICODE_STRING,
) -> NTSTATUS {
    RtlCreateProcessParametersEx(result, image, dll_path, current_directory, command_line,
        environment, window_title, desktop, shell_info, runtime_info, 0)
}

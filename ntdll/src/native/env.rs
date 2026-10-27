//! Windows 11 x64 NTDLL environment-block exports.
//!
//! Environment blocks are UTF-16 multistrings: NAME=VALUE\0...\0. These are
//! user-mode RTL routines; they do not belong in ntoskrnl. The allocator below
//! owns blocks created by this module so they can be safely replaced/freed.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::alloc::Layout;
use core::ffi::c_void;
use core::ptr::{self, null, null_mut};
use alloc::{alloc::{alloc, dealloc}, vec::Vec};
use alloc::string::ToString;

type NTSTATUS = i32;
type ULONG = u32;
type WCHAR = u16;
type USHORT = u16;
type SIZE_T = usize;
type PWSTR = *mut u16;
type PCWSTR = *const u16;
type PVOID = *mut c_void;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const STATUS_VARIABLE_NOT_FOUND: NTSTATUS = 0xC000_0100u32 as i32;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
const RTL_CREATE_ENVIRONMENT_TRANSLATE: ULONG = 0x1;
const ENV_MAGIC: u64 = 0x434F_474E_454E_5631;
const MAX_ENV_UNITS: usize = 1 << 20;

#[repr(C)]
struct EnvHeader {
    magic: u64,
    capacity: usize,
    length: usize,
}

#[inline]
fn allocation_layout(capacity: usize) -> Option<Layout> {
    let bytes = core::mem::size_of::<EnvHeader>()
        .checked_add(capacity.checked_mul(core::mem::size_of::<u16>())?)?;
    Layout::from_size_align(bytes, core::mem::align_of::<EnvHeader>()).ok()
}

unsafe fn allocate_environment(capacity: usize) -> Result<PWSTR, NTSTATUS> {
    let capacity = capacity.max(2);
    let layout = allocation_layout(capacity).ok_or(STATUS_NO_MEMORY)?;
    let base = unsafe { alloc(layout) } as *mut EnvHeader;
    if base.is_null() { return Err(STATUS_NO_MEMORY); }
    unsafe {
        ptr::write(base, EnvHeader { magic: ENV_MAGIC, capacity, length: 2 });
        let data = base.add(1) as PWSTR;
        *data = 0;
        *data.add(1) = 0;
        Ok(data)
    }
}

unsafe fn owned_header(environment: PWSTR) -> Result<*mut EnvHeader, NTSTATUS> {
    if environment.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let header = unsafe { (environment as *mut EnvHeader).sub(1) };
    if unsafe { (*header).magic } != ENV_MAGIC { return Err(STATUS_INVALID_PARAMETER); }
    Ok(header)
}

unsafe fn free_owned(environment: PWSTR) -> NTSTATUS {
    let header = match unsafe { owned_header(environment) } {
        Ok(h) => h,
        Err(status) => return status,
    };
    let Some(layout) = allocation_layout(unsafe { (*header).capacity }) else {
        return STATUS_INVALID_PARAMETER;
    };
    unsafe {
        (*header).magic = 0;
        dealloc(header as *mut u8, layout);
    }
    STATUS_SUCCESS
}

unsafe fn environment_length(environment: PCWSTR) -> Result<usize, NTSTATUS> {
    if environment.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    for i in 0..MAX_ENV_UNITS {
        if unsafe { *environment.add(i) } == 0 {
            if i == 0 {
                if unsafe { *environment.add(1) } == 0 { return Ok(2); }
            } else if unsafe { *environment.add(i - 1) } == 0 {
                return Ok(i + 1);
            }
        }
    }
    Err(STATUS_INVALID_PARAMETER)
}

#[inline]
fn eq_name_ci(a: &[u16], b: &[u16]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(&x, &y)| {
        // Windows environment variable names are case-insensitive. Restrict
        // folding to Unicode's simple lowercase mapping, as available in Rust.
        char::from_u32(x as u32).unwrap_or('\u{FFFD}').to_lowercase().to_string()
            == char::from_u32(y as u32).unwrap_or('\u{FFFD}').to_lowercase().to_string()
    })
}

fn entries(block: &[u16]) -> Vec<(usize, usize, usize)> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < block.len() && block[start] != 0 {
        let mut end = start;
        while end < block.len() && block[end] != 0 { end += 1; }
        if end == block.len() { break; }
        // Drive-current-directory pseudo-variables start with '=' (e.g. =C:).
        let search_from = if block[start] == b'=' as u16 { start + 1 } else { start };
        let split = (search_from..end).find(|&i| block[i] == b'=' as u16);
        if let Some(split) = split { result.push((start, split, end)); }
        start = end + 1;
    }
    result
}

/// Creates an environment block. A null source creates an empty block; a
/// non-null source is cloned. RTL_CREATE_ENVIRONMENT_TRANSLATE is accepted as
/// a compatibility flag, but ANSI/OEM translation is not performed here.
#[no_mangle]
pub unsafe extern "system" fn RtlCreateEnvironmentEx(
    source_environment: PVOID,
    environment: *mut PVOID,
    flags: ULONG,
) -> NTSTATUS {
    if environment.is_null() || (flags & !RTL_CREATE_ENVIRONMENT_TRANSLATE) != 0 {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { *environment = ptr::null_mut(); }
    let source = source_environment as PCWSTR;
    let units = if source.is_null() {
        2
    } else {
        match unsafe { environment_length(source) } {
            Ok(n) => n,
            Err(status) => return status,
        }
    };
    let out = match unsafe { allocate_environment(units.max(2)) } {
        Ok(p) => p,
        Err(status) => return status,
    };
    if !source.is_null() {
        unsafe { ptr::copy_nonoverlapping(source, out, units); }
        if let Ok(header) = unsafe { owned_header(out) } {
            unsafe { (*header).length = units; }
        }
    }
    unsafe { *environment = out as PVOID; }
    STATUS_SUCCESS
}

/// Sets, replaces, or deletes one variable in an owned environment block.
/// NameLength and ValueLength are byte lengths, matching the native SIZE_T ABI.
#[no_mangle]
pub unsafe extern "system" fn RtlSetEnvironmentVar(
    environment: *mut PVOID,
    name: PWSTR,
    name_length: SIZE_T,
    value: PWSTR,
    value_length: SIZE_T,
) -> NTSTATUS {
    if environment.is_null() || name.is_null() || name_length == 0 || name_length % 2 != 0
        || value_length % 2 != 0 || (value.is_null() && value_length != 0)
    {
        return STATUS_INVALID_PARAMETER;
    }
    let current = unsafe { *environment as PWSTR };
    if current.is_null() { return STATUS_INVALID_PARAMETER; }
    let header = match unsafe { owned_header(current) } {
        Ok(h) => h,
        Err(status) => return status,
    };
    let name_units = name_length / 2;
    let value_units = value_length / 2;
    let name_slice = unsafe { core::slice::from_raw_parts(name, name_units) };
    if name_slice.contains(&(0u16)) || name_slice.contains(&(b'=' as u16)) {
        return STATUS_INVALID_PARAMETER;
    }
    if !value.is_null() && value_units > MAX_ENV_UNITS {
        return STATUS_INVALID_PARAMETER;
    }
    let old_len = unsafe { (*header).length };
    let old_block = unsafe { core::slice::from_raw_parts(current, old_len) };
    let mut rebuilt: Vec<u16> = Vec::with_capacity(old_len.saturating_add(name_units + value_units + 2));
    let mut replaced = false;
    for (start, split, end) in entries(old_block) {
        if eq_name_ci(&old_block[start..split], name_slice) {
            if !value.is_null() {
                rebuilt.extend_from_slice(name_slice);
                rebuilt.push(b'=' as u16);
                if value_units != 0 {
                    rebuilt.extend_from_slice(unsafe { core::slice::from_raw_parts(value, value_units) });
                }
                rebuilt.push(0);
            }
            replaced = true;
        } else {
            rebuilt.extend_from_slice(&old_block[start..=end]);
        }
    }
    if !replaced && !value.is_null() {
        rebuilt.extend_from_slice(name_slice);
        rebuilt.push(b'=' as u16);
        if value_units != 0 {
            rebuilt.extend_from_slice(unsafe { core::slice::from_raw_parts(value, value_units) });
        }
        rebuilt.push(0);
    }
    rebuilt.push(0);
    if rebuilt.len() == 1 { rebuilt.push(0); }
    if rebuilt.len() > MAX_ENV_UNITS { return STATUS_NO_MEMORY; }

    let new_env = match unsafe { allocate_environment(rebuilt.len()) } {
        Ok(p) => p,
        Err(status) => return status,
    };
    unsafe {
        ptr::copy_nonoverlapping(rebuilt.as_ptr(), new_env, rebuilt.len());
        (*owned_header(new_env).unwrap()).length = rebuilt.len();
    }
    unsafe { *environment = new_env as PVOID; }
    let _ = unsafe { free_owned(current) };
    STATUS_SUCCESS
}

/// Frees an environment block created by RtlCreateEnvironmentEx or this module's
/// RtlSetEnvironmentVar. Foreign process-owned blocks are rejected.
#[no_mangle]
pub unsafe extern "system" fn RtlDestroyEnvironment(environment: PVOID) -> NTSTATUS {
    if environment.is_null() { return STATUS_SUCCESS; }
    unsafe { free_owned(environment as PWSTR) }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn as_units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn create_clone_set_replace_delete_and_destroy_environment() {
        unsafe {
            let source = as_units("Path=C:\\Windows\0TEMP=C:\\Temp\0\0");
            let mut env: PVOID = ptr::null_mut();
            assert_eq!(RtlCreateEnvironmentEx(source.as_ptr() as PVOID, &mut env, 0), STATUS_SUCCESS);
            let name = as_units("TEMP");
            let value = as_units("C:\\NewTemp");
            assert_eq!(RtlSetEnvironmentVar(&mut env, name.as_ptr() as PWSTR, name.len()*2, value.as_ptr() as PWSTR, value.len()*2), STATUS_SUCCESS);
            let out = env as PWSTR;
            let n = environment_length(out).unwrap();
            let block = core::slice::from_raw_parts(out, n);
            let pairs = entries(block);
            assert!(pairs.iter().any(|&(s, split, e)| block[s..split] == as_units("TEMP") && block[split+1..e] == value));
            let empty: Vec<u16> = Vec::new();
            assert_eq!(RtlSetEnvironmentVar(&mut env, name.as_ptr() as PWSTR, name.len()*2, empty.as_ptr() as PWSTR, 0), STATUS_SUCCESS);
            assert_eq!(RtlDestroyEnvironment(env), STATUS_SUCCESS);
        }
    }

    #[test]
    fn null_source_creates_double_terminated_empty_block() {
        unsafe {
            let mut env: PVOID = ptr::null_mut();
            assert_eq!(RtlCreateEnvironmentEx(ptr::null_mut(), &mut env, 0), STATUS_SUCCESS);
            let p = env as PWSTR;
            assert_eq!(*p, 0);
            assert_eq!(*p.add(1), 0);
            assert_eq!(RtlDestroyEnvironment(env), STATUS_SUCCESS);
        }
    }
}


// Environment query/expansion uses the shared process environment lookup and
// PEB locking helpers. These routines do not allocate or free environment
// blocks, so ownership remains with the creator of the supplied block.
use super::process_parameters::{
    current_environment_slot, find_variable, wcslen16, RtlAcquirePebLock,
    RtlGetCurrentPeb, RtlReleasePebLock, UNICODE_STRING,
};

/// RtlQueryEnvironmentVariable_U. A null environment selects the current PEB environment under FastPebLock.
unsafe fn query_environment_variable_u_explicit(
    environment: *mut WCHAR,
    name: *const UNICODE_STRING,
    value: *mut UNICODE_STRING,
) -> NTSTATUS {
    if name.is_null() || value.is_null() || (*name).buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    (*value).length = 0;
    if (*name).length == 0 { return STATUS_VARIABLE_NOT_FOUND; }
    if environment.is_null() { return STATUS_INVALID_PARAMETER; }

    let name_len = ((*name).length as usize) / 2;
    let found = find_variable(environment, (*name).buffer, name_len);
    if found.is_null() { return STATUS_VARIABLE_NOT_FOUND; }

    let len = wcslen16(found);
    let byte_len = len.saturating_mul(2);
    if byte_len > USHORT::MAX as usize { return STATUS_BUFFER_TOO_SMALL; }
    (*value).length = byte_len as USHORT;

    if byte_len > (*value).maximum_length as usize {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if (*value).buffer.is_null() && byte_len != 0 {
        (*value).length = 0;
        return STATUS_INVALID_PARAMETER;
    }
    let copy_units = core::cmp::min(len + 1, (*value).maximum_length as usize / 2);
    if copy_units != 0 {
        core::ptr::copy(found, (*value).buffer, copy_units);
    }
    STATUS_SUCCESS
}


/// Query a named variable from an explicit environment block or the current PEB environment.
pub unsafe extern "system" fn RtlQueryEnvironmentVariable_U(
    environment: *mut WCHAR,
    name: *const UNICODE_STRING,
    value: *mut UNICODE_STRING,
) -> NTSTATUS {
    if !environment.is_null() {
        return query_environment_variable_u_explicit(environment, name, value);
    }
    RtlAcquirePebLock();
    let slot = current_environment_slot();
    let current = if slot.is_null() { null_mut() } else { *slot };
    let status = if current.is_null() {
        STATUS_INVALID_PARAMETER
    } else {
        query_environment_variable_u_explicit(current, name, value)
    };
    RtlReleasePebLock();
    status
}

/// RtlQueryEnvironmentVariable. Return length is measured in WCHAR units,
/// matching the native API. Null environment/current-process lookup is deferred.
unsafe fn query_environment_variable_explicit(
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


/// Query a variable from an explicit environment block or the current process environment.
pub unsafe extern "system" fn RtlQueryEnvironmentVariable(
    environment: *mut WCHAR,
    name: *const WCHAR,
    name_len: SIZE_T,
    value: *mut WCHAR,
    value_length: SIZE_T,
    return_length: *mut SIZE_T,
) -> NTSTATUS {
    if !environment.is_null() {
        return query_environment_variable_explicit(environment, name, name_len, value, value_length, return_length);
    }
    RtlAcquirePebLock();
    let slot = current_environment_slot();
    let current = if slot.is_null() { null_mut() } else { *slot };
    let status = if current.is_null() {
        STATUS_INVALID_PARAMETER
    } else {
        query_environment_variable_explicit(current, name, name_len, value, value_length, return_length)
    };
    RtlReleasePebLock();
    status
}

/// Expand %NAME% references. The return length includes the trailing NUL and
/// is measured in UTF-16 code units, as in Wine's ntdll implementation.
unsafe fn expand_environment_strings_explicit(
    environment: *const WCHAR,
    source: *const WCHAR,
    source_len: SIZE_T,
    destination: *mut WCHAR,
    destination_count: SIZE_T,
    required_length: *mut SIZE_T,
) -> NTSTATUS {
    if source.is_null() || (destination_count != 0 && destination.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }
    if environment.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut src = source;
    let mut remaining = source_len;
    let mut dst_pos = 0usize;
    let mut required = 1usize;
    while remaining != 0 {
        let token: *const WCHAR;
        let token_len: usize;
        let copy_len: usize;
        if *src != b'%' as WCHAR {
            let mut n = 0;
            while n < remaining && *src.add(n) != b'%' as WCHAR { n += 1; }
            token = src; token_len = n; copy_len = n;
            src = src.add(n); remaining -= n;
        } else {
            let mut end = 1usize;
            while end < remaining && *src.add(end) != b'%' as WCHAR { end += 1; }
            if end < remaining {
                let found = find_variable(environment, src.add(1), end - 1);
                if !found.is_null() {
                    token = found; token_len = wcslen16(found);
                    src = src.add(end + 1); remaining -= end + 1;
                    if token_len < destination_count.saturating_sub(dst_pos) { copy_len = token_len; }
                    else { copy_len = 0; }
                } else {
                    token = src; token_len = end + 1; copy_len = end + 1;
                    src = src.add(end + 1); remaining -= end + 1;
                }
            } else {
                token = src; token_len = remaining; copy_len = remaining;
                src = src.add(remaining); remaining = 0;
            }
        }
        required = required.saturating_add(token_len);
        if destination_count > dst_pos {
            let available = destination_count - dst_pos;
            let n = core::cmp::min(copy_len, available.saturating_sub(1));
            if n != 0 { core::ptr::copy(token, destination.add(dst_pos), n); }
            dst_pos += n;
        }
    }
    if !required_length.is_null() { *required_length = required; }
    if destination_count != 0 && dst_pos < destination_count { *destination.add(dst_pos) = 0; }
    if destination_count >= required { STATUS_SUCCESS } else { STATUS_BUFFER_TOO_SMALL }
}


/// Expand environment references using an explicit block or the current PEB environment.
pub unsafe extern "system" fn RtlExpandEnvironmentStrings(
    environment: *const WCHAR,
    source: *const WCHAR,
    source_len: SIZE_T,
    destination: *mut WCHAR,
    destination_count: SIZE_T,
    required_length: *mut SIZE_T,
) -> NTSTATUS {
    if !environment.is_null() {
        return expand_environment_strings_explicit(environment, source, source_len, destination, destination_count, required_length);
    }
    RtlAcquirePebLock();
    let slot = current_environment_slot();
    let current = if slot.is_null() { null_mut() } else { *slot };
    let status = if current.is_null() {
        STATUS_INVALID_PARAMETER
    } else {
        expand_environment_strings_explicit(current, source, source_len, destination, destination_count, required_length)
    };
    RtlReleasePebLock();
    status
}

pub unsafe extern "system" fn RtlExpandEnvironmentStrings_U(
    environment: *const WCHAR,
    source: *const UNICODE_STRING,
    destination: *mut UNICODE_STRING,
    required_bytes: *mut ULONG,
) -> NTSTATUS {
    if source.is_null() || destination.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut required_units = 0usize;
    let status = RtlExpandEnvironmentStrings(
        environment, (*source).buffer, (*source).length as usize / 2,
        (*destination).buffer, (*destination).maximum_length as usize / 2,
        &mut required_units,
    );
    if !required_bytes.is_null() {
        *required_bytes = required_units.saturating_mul(2).min(ULONG::MAX as usize) as ULONG;
    }
    if required_units.saturating_mul(2) > USHORT::MAX as usize { return STATUS_BUFFER_TOO_SMALL; }
    if status == STATUS_SUCCESS {
        (*destination).length = required_units.saturating_sub(1).saturating_mul(2) as USHORT;
    }
    status
}

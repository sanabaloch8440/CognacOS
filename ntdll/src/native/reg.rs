//! Partial Rust port of Wine's root/wine/dlls/ntdll/reg.c.
//!
//! Covers the low-level key wrappers and registry-root path mapping used by
//! RtlCheckRegistryKey, RtlCreateRegistryKey, RtlDeleteRegistryValue and
//! RtlWriteRegistryValue. RtlQueryRegistryValues callback-table processing,
//! token/SID formatting and heap-backed variable-length queries are not yet ported.
//! Keep the source's LGPL-2.1-or-later attribution in the surrounding Wine tree.

#![allow(non_camel_case_types, non_snake_case)]

use core::ffi::c_void;
use core::ptr::{null, null_mut};

pub type NTSTATUS = i32;
pub type ULONG = u32;
pub type USHORT = u16;
pub type ACCESS_MASK = u32;
pub type HANDLE = *mut c_void;
pub type PVOID = *mut c_void;
pub type PCWSTR = *const u16;
pub type PWSTR = *mut u16;
pub type PHANDLE = *mut HANDLE;

pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000D_u32 as i32;
pub const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017_u32 as i32;
pub const STATUS_OBJECT_NAME_NOT_FOUND: NTSTATUS = 0xC000_0034_u32 as i32;
pub const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008_u32 as i32;
pub const STATUS_OBJECT_PATH_SYNTAX_BAD: NTSTATUS = 0xC000_003B_u32 as i32;

pub const OBJ_PERMANENT: ULONG = 0x10;
pub const OBJ_EXCLUSIVE: ULONG = 0x20;
pub const OBJ_CASE_INSENSITIVE: ULONG = 0x40;
pub const KEY_ALL_ACCESS: ACCESS_MASK = 0x000F_003F;
pub const REG_OPTION_NON_VOLATILE: ULONG = 0;
pub const RTL_REGISTRY_ABSOLUTE: ULONG = 0;
pub const RTL_REGISTRY_SERVICES: ULONG = 1;
pub const RTL_REGISTRY_CONTROL: ULONG = 2;
pub const RTL_REGISTRY_WINDOWS_NT: ULONG = 3;
pub const RTL_REGISTRY_DEVICEMAP: ULONG = 4;
pub const RTL_REGISTRY_USER: ULONG = 5;
pub const RTL_REGISTRY_HANDLE: ULONG = 0x4000_0000;
pub const RTL_REGISTRY_OPTIONAL: ULONG = 0x8000_0000;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UNICODE_STRING {
    pub Length: USHORT,
    pub MaximumLength: USHORT,
    pub Buffer: PWSTR,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OBJECT_ATTRIBUTES {
    pub Length: ULONG,
    pub RootDirectory: HANDLE,
    pub ObjectName: *mut UNICODE_STRING,
    pub Attributes: ULONG,
    pub SecurityDescriptor: PVOID,
    pub SecurityQualityOfService: PVOID,
}

extern "system" {
    fn NtCreateKey(
        key: PHANDLE, access: ACCESS_MASK, attributes: *const OBJECT_ATTRIBUTES,
        title_index: ULONG, class: *const UNICODE_STRING, options: ULONG,
        disposition: *mut ULONG,
    ) -> NTSTATUS;
    fn NtOpenKey(key: PHANDLE, access: ACCESS_MASK, attributes: *const OBJECT_ATTRIBUTES) -> NTSTATUS;
    fn NtDeleteKey(key: HANDLE) -> NTSTATUS;
    fn NtDeleteValueKey(key: HANDLE, value_name: *const UNICODE_STRING) -> NTSTATUS;
    fn NtSetValueKey(
        key: HANDLE, value_name: *const UNICODE_STRING, title_index: ULONG,
        value_type: ULONG, data: PVOID, data_size: ULONG,
    ) -> NTSTATUS;
    fn NtClose(handle: HANDLE) -> NTSTATUS;
    fn RtlInitUnicodeString(destination: *mut UNICODE_STRING, source: PCWSTR);
}

#[inline]
fn init_object_attributes(
    attributes: &mut OBJECT_ATTRIBUTES,
    name: *mut UNICODE_STRING,
    flags: ULONG,
) {
    *attributes = OBJECT_ATTRIBUTES {
        Length: core::mem::size_of::<OBJECT_ATTRIBUTES>() as ULONG,
        RootDirectory: null_mut(),
        ObjectName: name,
        Attributes: flags,
        SecurityDescriptor: null_mut(),
        SecurityQualityOfService: null_mut(),
    };
}

/// Mirrors RtlpNtCreateKey: strips permanent/exclusive object attributes and
/// intentionally ignores class/title/options in favor of native key creation defaults.
#[no_mangle]
pub unsafe extern "system" fn RtlpNtCreateKey(
    retkey: PHANDLE,
    access: ACCESS_MASK,
    attributes: *const OBJECT_ATTRIBUTES,
    _title_index: ULONG,
    _class: *const UNICODE_STRING,
    _options: ULONG,
    disposition: *mut ULONG,
) -> NTSTATUS {
    let mut sanitized = OBJECT_ATTRIBUTES {
        Length: core::mem::size_of::<OBJECT_ATTRIBUTES>() as ULONG,
        RootDirectory: null_mut(),
        ObjectName: null_mut(),
        Attributes: 0,
        SecurityDescriptor: null_mut(),
        SecurityQualityOfService: null_mut(),
    };
    let attr_ptr = if attributes.is_null() {
        null()
    } else {
        sanitized = unsafe { attributes.read() };
        sanitized.Attributes &= !(OBJ_PERMANENT | OBJ_EXCLUSIVE);
        &sanitized
    };
    unsafe { NtCreateKey(retkey, access, attr_ptr, 0, null(), 0, disposition) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlpNtOpenKey(
    retkey: PHANDLE,
    access: ACCESS_MASK,
    attributes: *mut OBJECT_ATTRIBUTES,
) -> NTSTATUS {
    if !attributes.is_null() {
        unsafe { (*attributes).Attributes &= !(OBJ_PERMANENT | OBJ_EXCLUSIVE) };
    }
    unsafe { NtOpenKey(retkey, access, attributes) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlpNtMakeTemporaryKey(key: HANDLE) -> NTSTATUS {
    unsafe { NtDeleteKey(key) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlpNtSetValueKey(
    key: HANDLE, value_type: ULONG, data: PVOID, data_size: ULONG,
) -> NTSTATUS {
    let mut empty_name = UNICODE_STRING { Length: 0, MaximumLength: 0, Buffer: null_mut() };
    unsafe { NtSetValueKey(key, &mut empty_name, 0, value_type, data, data_size) }
}

fn root_prefix(relative_to: ULONG) -> Option<&'static [u16]> {
    // UTF-16 constants without trailing NUL; the caller appends the requested path.
    match relative_to & 0xff {
        RTL_REGISTRY_ABSOLUTE => Some(&[]),
        RTL_REGISTRY_CONTROL => Some(&[
            92,82,101,103,105,115,116,114,121,92,77,97,99,104,105,110,101,92,
            83,121,115,116,101,109,92,67,117,114,114,101,110,116,67,111,110,116,114,111,108,83,101,116,92,67,111,110,116,114,111,108,92
        ]),
        RTL_REGISTRY_DEVICEMAP => Some(&[
            92,82,101,103,105,115,116,114,121,92,77,97,99,104,105,110,101,92,72,97,114,100,119,97,114,101,92,68,101,118,105,99,101,77,97,112,92
        ]),
        RTL_REGISTRY_SERVICES => Some(&[
            92,82,101,103,105,115,116,114,121,92,77,97,99,104,105,110,101,92,83,121,115,116,101,109,92,67,117,114,114,101,110,116,67,111,110,116,114,111,108,83,101,116,92,83,101,114,118,105,99,101,115,92
        ]),
        RTL_REGISTRY_USER => Some(&[
            92,82,101,103,105,115,116,114,121,92,85,115,101,114,92,67,117,114,114,101,110,116,85,115,101,114,92
        ]),
        RTL_REGISTRY_WINDOWS_NT => Some(&[
            92,82,101,103,105,115,116,114,121,92,77,97,99,104,105,110,101,92,83,111,102,116,119,97,114,101,92,77,105,99,114,111,115,111,102,116,92,87,105,110,100,111,119,115,32,78,84,92,67,117,114,114,101,110,116,86,101,114,115,105,111,110,92
        ]),
        _ => None,
    }
}

/// Builds a native registry object name in caller-owned storage.
/// Returns the number of UTF-16 code units, or an NTSTATUS error.
unsafe fn make_registry_name(
    relative_to: ULONG, path: PCWSTR, storage: &mut [u16],
    string: &mut UNICODE_STRING, attributes: &mut OBJECT_ATTRIBUTES,
) -> Result<(), NTSTATUS> {
    let prefix = root_prefix(relative_to).ok_or(STATUS_INVALID_PARAMETER)?;
    if path.is_null() && !prefix.is_empty() { return Err(STATUS_INVALID_PARAMETER); }
    let mut path_len = 0usize;
    if !path.is_null() {
        while path_len < 32760 && unsafe { path.add(path_len).read() } != 0 { path_len += 1; }
        if path_len == 32760 { return Err(STATUS_INVALID_PARAMETER); }
    }
    let total = prefix.len().checked_add(path_len).ok_or(STATUS_INVALID_PARAMETER)?;
    if total >= storage.len() || total.checked_add(1).and_then(|n| n.checked_mul(2)).unwrap_or(usize::MAX) > u16::MAX as usize {
        return Err(STATUS_INVALID_PARAMETER);
    }
    storage[..prefix.len()].copy_from_slice(prefix);
    for i in 0..path_len { storage[prefix.len() + i] = unsafe { path.add(i).read() }; }
    storage[total] = 0;
    *string = UNICODE_STRING {
        Length: (total * 2) as USHORT,
        MaximumLength: ((total + 1) * 2) as USHORT,
        Buffer: storage.as_mut_ptr(),
    };
    init_object_attributes(attributes, string, OBJ_CASE_INSENSITIVE);
    Ok(())
}

unsafe fn open_registry_key(relative_to: ULONG, path: PCWSTR, key: *mut HANDLE) -> NTSTATUS {
    if relative_to & RTL_REGISTRY_HANDLE != 0 {
        if path.is_null() || key.is_null() { return STATUS_INVALID_PARAMETER; }
        unsafe { key.write(path.cast_mut().cast()) };
        return STATUS_SUCCESS;
    }
    if key.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut storage = [0u16; 1024];
    let mut name = UNICODE_STRING { Length: 0, MaximumLength: 0, Buffer: null_mut() };
    let mut attributes = OBJECT_ATTRIBUTES {
        Length: 0, RootDirectory: null_mut(), ObjectName: null_mut(), Attributes: 0,
        SecurityDescriptor: null_mut(), SecurityQualityOfService: null_mut(),
    };
    if let Err(status) = unsafe {
        make_registry_name(relative_to, path, &mut storage, &mut name, &mut attributes)
    } { return status; }
    unsafe { NtOpenKey(key, KEY_ALL_ACCESS, &attributes) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckRegistryKey(relative_to: ULONG, path: PWSTR) -> NTSTATUS {
    if relative_to == RTL_REGISTRY_ABSOLUTE && (path.is_null() || unsafe { path.read() } == 0) {
        return STATUS_OBJECT_PATH_SYNTAX_BAD;
    }
    if relative_to & RTL_REGISTRY_HANDLE != 0 { return STATUS_SUCCESS; }
    if relative_to <= RTL_REGISTRY_USER && (path.is_null() || unsafe { path.read() } == 0) {
        return STATUS_SUCCESS;
    }
    let mut key = null_mut();
    let status = unsafe { open_registry_key(relative_to, path, &mut key) };
    if status == STATUS_SUCCESS { unsafe { NtClose(key) }; }
    if status == STATUS_INVALID_HANDLE { STATUS_OBJECT_NAME_NOT_FOUND } else { status }
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateRegistryKey(relative_to: ULONG, path: PWSTR) -> NTSTATUS {
    let relative_to = relative_to & !RTL_REGISTRY_OPTIONAL;
    if relative_to == RTL_REGISTRY_ABSOLUTE && (path.is_null() || unsafe { path.read() } == 0) {
        return STATUS_OBJECT_PATH_SYNTAX_BAD;
    }
    if relative_to <= RTL_REGISTRY_USER && (path.is_null() || unsafe { path.read() } == 0) {
        return STATUS_SUCCESS;
    }
    let mut storage = [0u16; 1024];
    let mut name = UNICODE_STRING { Length: 0, MaximumLength: 0, Buffer: null_mut() };
    let mut attributes = OBJECT_ATTRIBUTES {
        Length: 0, RootDirectory: null_mut(), ObjectName: null_mut(), Attributes: 0,
        SecurityDescriptor: null_mut(), SecurityQualityOfService: null_mut(),
    };
    if let Err(status) = unsafe {
        make_registry_name(relative_to, path, &mut storage, &mut name, &mut attributes)
    } { return status; }
    let mut key = null_mut();
    let status = unsafe {
        NtCreateKey(&mut key, KEY_ALL_ACCESS, &attributes, 0, null(), REG_OPTION_NON_VOLATILE, null_mut())
    };
    if !key.is_null() { unsafe { NtClose(key) }; }
    status
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteRegistryValue(
    relative_to: ULONG, path: PCWSTR, value_name: PCWSTR,
) -> NTSTATUS {
    if value_name.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut value = UNICODE_STRING { Length: 0, MaximumLength: 0, Buffer: null_mut() };
    unsafe { RtlInitUnicodeString(&mut value, value_name) };
    let mut key = null_mut();
    if relative_to == RTL_REGISTRY_HANDLE {
        key = path.cast_mut().cast();
    } else {
        let status = unsafe { open_registry_key(relative_to, path, &mut key) };
        if status != STATUS_SUCCESS { return status; }
    }
    let status = unsafe { NtDeleteValueKey(key, &value) };
    if relative_to != RTL_REGISTRY_HANDLE { unsafe { NtClose(key) }; }
    status
}

#[no_mangle]
pub unsafe extern "system" fn RtlWriteRegistryValue(
    relative_to: ULONG, path: PCWSTR, value_name: PCWSTR,
    value_type: ULONG, data: PVOID, data_size: ULONG,
) -> NTSTATUS {
    if value_name.is_null() || (data.is_null() && data_size != 0) {
        return STATUS_INVALID_PARAMETER;
    }
    let mut value = UNICODE_STRING { Length: 0, MaximumLength: 0, Buffer: null_mut() };
    unsafe { RtlInitUnicodeString(&mut value, value_name) };
    if relative_to == RTL_REGISTRY_HANDLE {
        return unsafe { NtSetValueKey(path.cast_mut().cast(), &value, 0, value_type, data, data_size) };
    }
    let mut key = null_mut();
    let status = unsafe { open_registry_key(relative_to, path, &mut key) };
    if status != STATUS_SUCCESS { return status; }
    let status = unsafe { NtSetValueKey(key, &value, 0, value_type, data, data_size) };
    unsafe { NtClose(key) };
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_prefixes_are_nt_registry_paths() {
        assert_eq!(root_prefix(RTL_REGISTRY_ABSOLUTE), Some(&[][..]));
        assert_eq!(root_prefix(RTL_REGISTRY_CONTROL).unwrap()[..10], [92,82,101,103,105,115,116,114,121,92]);
        assert_eq!(root_prefix(RTL_REGISTRY_SERVICES).unwrap().last(), Some(&92));
        assert_eq!(root_prefix(RTL_REGISTRY_WINDOWS_NT).unwrap().last(), Some(&92));
    }

    #[test]
    fn unsupported_registry_root_is_rejected() {
        assert!(root_prefix(6).is_none());
    }

    #[test]
    fn unicode_string_layout_matches_x64() {
        assert_eq!(core::mem::size_of::<UNICODE_STRING>(), 16);
        assert_eq!(core::mem::size_of::<OBJECT_ATTRIBUTES>(), 48);
    }
}

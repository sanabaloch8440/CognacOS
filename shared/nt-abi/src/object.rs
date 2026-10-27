//! Shared Object Manager and counted UTF-16 string layouts.

use crate::types::{HANDLE, ULONG, USHORT, WCHAR};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UNICODE_STRING {
    pub length: USHORT,
    pub maximum_length: USHORT,
    pub buffer: *mut WCHAR,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OBJECT_ATTRIBUTES {
    pub length: ULONG,
    pub root_directory: HANDLE,
    pub object_name: *mut UNICODE_STRING,
    pub attributes: ULONG,
    pub security_descriptor: *mut core::ffi::c_void,
    pub security_quality_of_service: *mut core::ffi::c_void,
}

pub const OBJ_INHERIT: ULONG = 0x0000_0002;
pub const OBJ_CASE_INSENSITIVE: ULONG = 0x0000_0040;
pub const OBJ_OPENIF: ULONG = 0x0000_0080;
pub const OBJ_KERNEL_HANDLE: ULONG = 0x0000_0200;

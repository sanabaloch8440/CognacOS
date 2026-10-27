//! Shared Object Manager and counted UTF-16 string layouts.

use crate::types::{HANDLE, ULONG, USHORT, WCHAR};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UNICODE_STRING {
    /// Length in bytes, excluding any trailing NUL.
    pub length: USHORT,
    /// Buffer capacity in bytes.
    pub maximum_length: USHORT,
    pub buffer: *mut WCHAR,
}

pub type PUNICODE_STRING = *mut UNICODE_STRING;

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

pub type POBJECT_ATTRIBUTES = *mut OBJECT_ATTRIBUTES;

pub const OBJ_INHERIT: ULONG = 0x0000_0002;
pub const OBJ_CASE_INSENSITIVE: ULONG = 0x0000_0040;
pub const OBJ_OPENIF: ULONG = 0x0000_0080;
pub const OBJ_KERNEL_HANDLE: ULONG = 0x0000_0200;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn unicode_string_layout_matches_windows_pointer_width() {
        assert_eq!(offset_of!(UNICODE_STRING, length), 0);
        assert_eq!(offset_of!(UNICODE_STRING, maximum_length), 2);
        if cfg!(target_pointer_width = "64") {
            assert_eq!(size_of::<UNICODE_STRING>(), 16);
            assert_eq!(align_of::<UNICODE_STRING>(), 8);
            assert_eq!(offset_of!(UNICODE_STRING, buffer), 8);
        } else {
            assert_eq!(size_of::<UNICODE_STRING>(), 8);
            assert_eq!(align_of::<UNICODE_STRING>(), 4);
            assert_eq!(offset_of!(UNICODE_STRING, buffer), 4);
        }
    }

    #[test]
    fn object_attributes_layout_matches_windows_pointer_width() {
        assert_eq!(offset_of!(OBJECT_ATTRIBUTES, length), 0);
        if cfg!(target_pointer_width = "64") {
            assert_eq!(size_of::<OBJECT_ATTRIBUTES>(), 48);
            assert_eq!(align_of::<OBJECT_ATTRIBUTES>(), 8);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, root_directory), 8);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, object_name), 16);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, attributes), 24);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, security_descriptor), 32);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, security_quality_of_service), 40);
        } else {
            assert_eq!(size_of::<OBJECT_ATTRIBUTES>(), 24);
            assert_eq!(align_of::<OBJECT_ATTRIBUTES>(), 4);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, root_directory), 4);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, object_name), 8);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, attributes), 12);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, security_descriptor), 16);
            assert_eq!(offset_of!(OBJECT_ATTRIBUTES, security_quality_of_service), 20);
        }
    }
}

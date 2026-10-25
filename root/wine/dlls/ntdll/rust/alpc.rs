//! Source-first Rust port of Wine's ntdll/alpc.c attribute helpers.
//!
//! Ports AlpcGetHeaderSize, AlpcGetMessageAttribute, and
//! AlpcInitializeMessageAttribute. Layouts follow CognacOS's vendored
//! winternl.h declarations for x64. This module is not linked into the native
//! PE32+ build yet.
//!
//! # Safety
//! These APIs mirror the Windows raw-pointer ABI. Non-null pointers supplied
//! by callers must be valid for the sizes implied by their arguments.

#![allow(non_camel_case_types, non_snake_case)]

use core::{ffi::c_void, mem::size_of};

pub type ULONG = u32;
pub type SIZE_T = usize;
pub type NTSTATUS = i32;
pub type PVOID = *mut c_void;

pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023_u32 as NTSTATUS;

pub const ALPC_MESSAGE_SECURITY_ATTRIBUTE: ULONG = 0x8000_0000;
pub const ALPC_MESSAGE_VIEW_ATTRIBUTE: ULONG = 0x4000_0000;
pub const ALPC_MESSAGE_CONTEXT_ATTRIBUTE: ULONG = 0x2000_0000;
pub const ALPC_MESSAGE_HANDLE_ATTRIBUTE: ULONG = 0x1000_0000;
pub const ALPC_MESSAGE_TOKEN_ATTRIBUTE: ULONG = 0x0800_0000;
pub const ALPC_MESSAGE_DIRECT_ATTRIBUTE: ULONG = 0x0400_0000;
pub const ALPC_MESSAGE_WORK_ON_BEHALF_ATTRIBUTE: ULONG = 0x0200_0000;
pub const ALPC_MESSAGE_ATTRIBUTE_ALL: ULONG = 0xFE00_0000;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ALPC_MESSAGE_ATTRIBUTES {
    pub AllocatedAttributes: ULONG,
    pub ValidAttributes: ULONG,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_SECURITY_ATTR {
    pub Flags: ULONG,
    pub QoS: *mut c_void,
    pub ContextHandle: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_VIEW_ATTR {
    pub Flags: ULONG,
    pub SectionHandle: *mut c_void,
    pub ViewBase: *mut c_void,
    pub ViewSize: SIZE_T,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_CONTEXT_ATTR {
    pub PortContext: *mut c_void,
    pub MessageContext: *mut c_void,
    pub Sequence: ULONG,
    pub MessageId: ULONG,
    pub CallbackId: ULONG,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_HANDLE_ATTR {
    pub Flags: ULONG,
    pub Handle: *mut c_void,
    pub ObjectType: ULONG,
    pub DesiredAccess: ULONG,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_TOKEN_ATTR {
    pub TokenId: u64,
    pub AuthenticationId: u64,
    pub ModifiedId: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_DIRECT_ATTR {
    pub Event: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_WORK_ON_BEHALF_ATTR {
    pub Ticket: u64,
}

const ATTRIBUTE_SIZES: [(ULONG, usize); 7] = [
    (ALPC_MESSAGE_SECURITY_ATTRIBUTE, size_of::<ALPC_SECURITY_ATTR>()),
    (ALPC_MESSAGE_VIEW_ATTRIBUTE, size_of::<ALPC_VIEW_ATTR>()),
    (ALPC_MESSAGE_CONTEXT_ATTRIBUTE, size_of::<ALPC_CONTEXT_ATTR>()),
    (ALPC_MESSAGE_HANDLE_ATTRIBUTE, size_of::<ALPC_HANDLE_ATTR>()),
    (ALPC_MESSAGE_TOKEN_ATTRIBUTE, size_of::<ALPC_TOKEN_ATTR>()),
    (ALPC_MESSAGE_DIRECT_ATTRIBUTE, size_of::<ALPC_DIRECT_ATTR>()),
    (ALPC_MESSAGE_WORK_ON_BEHALF_ATTRIBUTE, size_of::<ALPC_WORK_ON_BEHALF_ATTR>()),
];

/// Returns the header plus storage required for each recognized attribute.
#[no_mangle]
pub extern "system" fn AlpcGetHeaderSize(attribute_flags: ULONG) -> SIZE_T {
    ATTRIBUTE_SIZES.iter().fold(
        size_of::<ALPC_MESSAGE_ATTRIBUTES>(),
        |total, (flag, size)| {
            if attribute_flags & flag != 0 { total + size } else { total }
        },
    )
}

/// Returns the pointer to one allocated attribute, or null when unavailable.
///
/// # Safety
/// attributes must point to a readable header and the allocation must contain
/// all attribute storage described by AllocatedAttributes.
#[no_mangle]
pub unsafe extern "system" fn AlpcGetMessageAttribute(
    attributes: *mut ALPC_MESSAGE_ATTRIBUTES,
    attribute_flag: ULONG,
) -> PVOID {
    if attribute_flag == 0 || attribute_flag & (attribute_flag - 1) != 0 {
        return core::ptr::null_mut();
    }
    if attributes.is_null() {
        return core::ptr::null_mut();
    }

    // SAFETY: Required by this function's pointer contract.
    let allocated = unsafe { (*attributes).AllocatedAttributes };
    if (attribute_flag & allocated & ALPC_MESSAGE_ATTRIBUTE_ALL) != attribute_flag {
        return core::ptr::null_mut();
    }

    let higher_attributes = allocated & !(attribute_flag | (attribute_flag - 1));
    // SAFETY: The caller guarantees the allocation covers the declared attributes.
    unsafe {
        attributes.cast::<u8>().add(AlpcGetHeaderSize(higher_attributes)).cast::<c_void>()
    }
}

/// Initializes the attribute header and reports the required allocation size.
///
/// The caller must provide a writable required_buffer_size pointer. A null
/// buffer is accepted when only querying the required size.
///
/// # Safety
/// required_buffer_size must be writable; a non-null buffer must provide at
/// least buffer_size writable bytes.
#[no_mangle]
pub unsafe extern "system" fn AlpcInitializeMessageAttribute(
    attribute_flags: ULONG,
    buffer: *mut ALPC_MESSAGE_ATTRIBUTES,
    buffer_size: SIZE_T,
    required_buffer_size: *mut SIZE_T,
) -> NTSTATUS {
    if required_buffer_size.is_null() {
        return STATUS_BUFFER_TOO_SMALL;
    }

    let required = AlpcGetHeaderSize(attribute_flags);
    // SAFETY: Required by this function's pointer contract.
    unsafe { required_buffer_size.write(required) };

    if buffer_size < required {
        return STATUS_BUFFER_TOO_SMALL;
    }

    if !buffer.is_null() {
        // SAFETY: The caller guarantees writability and the size check covers the header.
        unsafe {
            (*buffer).AllocatedAttributes = attribute_flags;
            (*buffer).ValidAttributes = 0;
        }
    }

    STATUS_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_includes_only_recognized_attributes() {
        assert_eq!(AlpcGetHeaderSize(0), size_of::<ALPC_MESSAGE_ATTRIBUTES>());
        assert_eq!(
            AlpcGetHeaderSize(ALPC_MESSAGE_SECURITY_ATTRIBUTE),
            size_of::<ALPC_MESSAGE_ATTRIBUTES>() + size_of::<ALPC_SECURITY_ATTR>()
        );
        assert_eq!(
            AlpcGetHeaderSize(ALPC_MESSAGE_ATTRIBUTE_ALL | 1),
            ATTRIBUTE_SIZES.iter().map(|(_, size)| size).sum::<usize>()
                + size_of::<ALPC_MESSAGE_ATTRIBUTES>()
        );
    }

    #[test]
    fn initialize_reports_required_size_and_checks_capacity() {
        let flags = ALPC_MESSAGE_VIEW_ATTRIBUTE | ALPC_MESSAGE_TOKEN_ATTRIBUTE;
        let required = AlpcGetHeaderSize(flags);
        let mut reported = 0usize;
        let mut header = ALPC_MESSAGE_ATTRIBUTES {
            AllocatedAttributes: 0,
            ValidAttributes: u32::MAX,
        };

        // SAFETY: Both output pointers are valid for their declared types.
        let status = unsafe {
            AlpcInitializeMessageAttribute(flags, &mut header, required - 1, &mut reported)
        };
        assert_eq!(status, STATUS_BUFFER_TOO_SMALL);
        assert_eq!(reported, required);
        assert_eq!(header.AllocatedAttributes, 0);

        // SAFETY: Both pointers are valid and the advertised capacity is sufficient.
        let status = unsafe {
            AlpcInitializeMessageAttribute(flags, &mut header, required, &mut reported)
        };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(header.AllocatedAttributes, flags);
        assert_eq!(header.ValidAttributes, 0);
    }

    #[test]
    fn attribute_pointer_is_ordered_by_descending_flag() {
        let flags = ALPC_MESSAGE_SECURITY_ATTRIBUTE
            | ALPC_MESSAGE_CONTEXT_ATTRIBUTE
            | ALPC_MESSAGE_TOKEN_ATTRIBUTE;
        let total = AlpcGetHeaderSize(flags);
        let mut storage = vec![0u64; (total + 7) / 8];
        let header = storage.as_mut_ptr().cast::<ALPC_MESSAGE_ATTRIBUTES>();

        // SAFETY: Storage is sufficiently sized and suitably aligned for the header.
        unsafe {
            (*header).AllocatedAttributes = flags;
            let security = AlpcGetMessageAttribute(header, ALPC_MESSAGE_SECURITY_ATTRIBUTE);
            let context = AlpcGetMessageAttribute(header, ALPC_MESSAGE_CONTEXT_ATTRIBUTE);
            let token = AlpcGetMessageAttribute(header, ALPC_MESSAGE_TOKEN_ATTRIBUTE);

            assert_eq!(security as usize - header as usize, size_of::<ALPC_MESSAGE_ATTRIBUTES>());
            assert_eq!(
                context as usize - header as usize,
                size_of::<ALPC_MESSAGE_ATTRIBUTES>() + size_of::<ALPC_SECURITY_ATTR>()
            );
            assert_eq!(
                token as usize - header as usize,
                size_of::<ALPC_MESSAGE_ATTRIBUTES>()
                    + size_of::<ALPC_SECURITY_ATTR>()
                    + size_of::<ALPC_CONTEXT_ATTR>()
            );
            assert!(AlpcGetMessageAttribute(header, 0).is_null());
            assert!(AlpcGetMessageAttribute(
                header,
                ALPC_MESSAGE_TOKEN_ATTRIBUTE | ALPC_MESSAGE_VIEW_ATTRIBUTE
            ).is_null());
            assert!(AlpcGetMessageAttribute(header, ALPC_MESSAGE_VIEW_ATTRIBUTE).is_null());
        }
    }

    #[test]
    fn x64_attribute_layout_sizes_match_winternl() {
        assert_eq!(size_of::<ALPC_MESSAGE_ATTRIBUTES>(), 8);
        assert_eq!(size_of::<ALPC_SECURITY_ATTR>(), 24);
        assert_eq!(size_of::<ALPC_VIEW_ATTR>(), 32);
        assert_eq!(size_of::<ALPC_CONTEXT_ATTR>(), 32);
        assert_eq!(size_of::<ALPC_HANDLE_ATTR>(), 24);
        assert_eq!(size_of::<ALPC_TOKEN_ATTR>(), 24);
        assert_eq!(size_of::<ALPC_DIRECT_ATTR>(), 8);
        assert_eq!(size_of::<ALPC_WORK_ON_BEHALF_ATTR>(), 8);
    }
}

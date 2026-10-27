//! Windows x64 NTDLL ALPC entry points.
//!
//! These declarations use the public PHNT/WDK-compatible x64 argument layouts.
//! CognacOS enters its own kernel through vector 0x83 and private service IDs;
//! these are NOT Windows build-specific syscall ordinals. A signature-correct
//! entry point does not imply that every kernel operation is implemented.
#![allow(non_snake_case, non_camel_case_types, clippy::missing_safety_doc)]

use core::ffi::c_void;

pub type NTSTATUS = i32;
pub type HANDLE = *mut c_void;
pub type PHANDLE = *mut HANDLE;
pub type PVOID = *mut c_void;
pub type ULONG = u32;
pub type PULONG = *mut ULONG;
pub type SIZE_T = usize;
pub type PSIZE_T = *mut SIZE_T;
pub type ACCESS_MASK = u32;
pub type BOOLEAN = u8;
pub type PALPC_MESSAGE_ATTRIBUTES = *mut ALPC_MESSAGE_ATTRIBUTES;
pub type ALPC_HANDLE = HANDLE;
pub type PALPC_HANDLE = *mut ALPC_HANDLE;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UNICODE_STRING { pub Length: u16, pub MaximumLength: u16, pub Buffer: *mut u16 }
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
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PORT_MESSAGE {
    pub DataLength: u16, pub TotalLength: u16,
    pub Type: u16, pub DataInfoOffset: u16,
    pub ClientId: [usize; 2], pub MessageId: ULONG, pub ClientViewSize: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_PORT_ATTRIBUTES {
    pub Flags: ULONG, pub SecurityQos: [u8; 12],
    pub MaxMessageLength: SIZE_T, pub MemoryBandwidth: SIZE_T,
    pub MaxPoolUsage: SIZE_T, pub MaxSectionSize: SIZE_T,
    pub MaxViewSize: SIZE_T, pub MaxTotalSectionSize: SIZE_T,
    pub DupObjectTypes: ULONG, pub Reserved: ULONG,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_MESSAGE_ATTRIBUTES { pub AllocatedAttributes: ULONG, pub ValidAttributes: ULONG }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_DATA_VIEW_ATTR {
    pub Flags: ULONG, pub _padding: ULONG,
    pub SectionHandle: ALPC_HANDLE, pub ViewBase: PVOID, pub ViewSize: SIZE_T,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_SECURITY_ATTR {
    pub Flags: ULONG, pub QoS: PVOID, pub ContextHandle: ALPC_HANDLE,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_PORT_ASSOCIATE_COMPLETION_PORT {
    pub CompletionKey: PVOID, pub CompletionPort: HANDLE,
}

core::arch::global_asm!(r#"
    .text
    .p2align 4
    .macro ALPC_STUB name, id
    .global \name
\name:
    mov r10, rcx
    mov eax, \id
    int 0x83
    ret
    .endm
    ALPC_STUB NtAlpcCreatePort,                         0x11b
    ALPC_STUB NtAlpcDisconnectPort,                     0x11c
    ALPC_STUB NtAlpcConnectPort,                        0x11d
    ALPC_STUB NtAlpcConnectPortEx,                      0x131
    ALPC_STUB NtAlpcAcceptConnectPort,                  0x11e
    ALPC_STUB NtAlpcSendWaitReceivePort,                0x11f
    ALPC_STUB NtAlpcCreatePortSection,                  0x120
    ALPC_STUB NtAlpcDeletePortSection,                  0x121
    ALPC_STUB NtAlpcCreateSectionView,                  0x122
    ALPC_STUB NtAlpcDeleteSectionView,                  0x123
    ALPC_STUB NtAlpcCreateSecurityContext,              0x124
    ALPC_STUB NtAlpcDeleteSecurityContext,              0x125
    ALPC_STUB NtAlpcRevokeSecurityContext,              0x126
    ALPC_STUB NtAlpcCreateResourceReserve,              0x127
    ALPC_STUB NtAlpcDeleteResourceReserve,              0x128
    ALPC_STUB NtAlpcQueryInformation,                   0x129
    ALPC_STUB NtAlpcSetInformation,                     0x12a
    ALPC_STUB NtAlpcQueryInformationMessage,            0x12b
    ALPC_STUB NtAlpcCancelMessage,                      0x12c
    ALPC_STUB NtAlpcOpenSenderProcess,                  0x12d
    ALPC_STUB NtAlpcOpenSenderThread,                   0x12e
    ALPC_STUB NtAlpcImpersonateClientOfPort,             0x12f
    ALPC_STUB NtAlpcImpersonateClientContainerOfPort,     0x130
"#);

unsafe extern "system" {
    pub fn NtAlpcCreatePort(PortHandle: PHANDLE, ObjectAttributes: *const OBJECT_ATTRIBUTES, PortAttributes: *const ALPC_PORT_ATTRIBUTES) -> NTSTATUS;
    pub fn NtAlpcDisconnectPort(PortHandle: HANDLE, Flags: ULONG) -> NTSTATUS;
    pub fn NtAlpcConnectPort(PortHandle: PHANDLE, PortName: *const UNICODE_STRING, ObjectAttributes: *const OBJECT_ATTRIBUTES, PortAttributes: *const ALPC_PORT_ATTRIBUTES, Flags: ULONG, RequiredServerSid: PVOID, ConnectionMessage: PVOID, BufferLength: PSIZE_T, OutMessageAttributes: *mut ALPC_MESSAGE_ATTRIBUTES, InMessageAttributes: *mut ALPC_MESSAGE_ATTRIBUTES, Timeout: PVOID) -> NTSTATUS;
    pub fn NtAlpcConnectPortEx(PortHandle: PHANDLE, ConnectionPortObjectAttributes: *const OBJECT_ATTRIBUTES, ClientPortObjectAttributes: *const OBJECT_ATTRIBUTES, PortAttributes: *const ALPC_PORT_ATTRIBUTES, Flags: ULONG, ServerSecurityRequirements: PVOID, ConnectionMessage: PVOID, BufferLength: PSIZE_T, OutMessageAttributes: *mut ALPC_MESSAGE_ATTRIBUTES, InMessageAttributes: *mut ALPC_MESSAGE_ATTRIBUTES, Timeout: PVOID) -> NTSTATUS;
    pub fn NtAlpcAcceptConnectPort(PortHandle: PHANDLE, ConnectionPortHandle: HANDLE, Flags: ULONG, ObjectAttributes: *const OBJECT_ATTRIBUTES, PortAttributes: *const ALPC_PORT_ATTRIBUTES, PortContext: PVOID, ConnectionRequest: *const PORT_MESSAGE, ConnectionMessageAttributes: *mut ALPC_MESSAGE_ATTRIBUTES, AcceptConnection: u8) -> NTSTATUS;
    pub fn NtAlpcSendWaitReceivePort(PortHandle: HANDLE, Flags: ULONG, SendMessage: *const PORT_MESSAGE, SendMessageAttributes: *const ALPC_MESSAGE_ATTRIBUTES, ReceiveMessage: *mut PORT_MESSAGE, BufferLength: PSIZE_T, ReceiveMessageAttributes: *mut ALPC_MESSAGE_ATTRIBUTES, Timeout: PVOID) -> NTSTATUS;
    pub fn NtAlpcCreatePortSection(PortHandle: HANDLE, Flags: ULONG, SectionHandle: HANDLE, SectionSize: SIZE_T, AlpcSectionHandle: PALPC_HANDLE, ActualSectionSize: PSIZE_T) -> NTSTATUS;
    pub fn NtAlpcDeletePortSection(PortHandle: HANDLE, Flags: ULONG, SectionHandle: ALPC_HANDLE) -> NTSTATUS;
    pub fn NtAlpcCreateSectionView(PortHandle: HANDLE, Flags: ULONG, ViewAttributes: *mut ALPC_DATA_VIEW_ATTR) -> NTSTATUS;
    pub fn NtAlpcDeleteSectionView(PortHandle: HANDLE, Flags: ULONG, ViewBase: PVOID) -> NTSTATUS;
    pub fn NtAlpcCreateSecurityContext(PortHandle: HANDLE, Flags: ULONG, SecurityAttribute: *mut ALPC_SECURITY_ATTR) -> NTSTATUS;
    pub fn NtAlpcDeleteSecurityContext(PortHandle: HANDLE, Flags: ULONG, ContextHandle: ALPC_HANDLE) -> NTSTATUS;
    pub fn NtAlpcRevokeSecurityContext(PortHandle: HANDLE, Flags: ULONG, ContextHandle: ULONG) -> NTSTATUS;
    pub fn NtAlpcCreateResourceReserve(PortHandle: HANDLE, Flags: ULONG, MessageSize: SIZE_T, ResourceId: PULONG) -> NTSTATUS;
    pub fn NtAlpcDeleteResourceReserve(PortHandle: HANDLE, Flags: ULONG, ResourceId: ULONG) -> NTSTATUS;
    pub fn NtAlpcQueryInformation(PortHandle: HANDLE, PortInformationClass: ULONG, PortInformation: PVOID, Length: ULONG, ReturnLength: PULONG) -> NTSTATUS;
    pub fn NtAlpcSetInformation(PortHandle: HANDLE, PortInformationClass: ULONG, PortInformation: PVOID, Length: ULONG) -> NTSTATUS;
    pub fn NtAlpcQueryInformationMessage(PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, MessageInformationClass: ULONG, MessageInformation: PVOID, Length: ULONG, ReturnLength: PULONG) -> NTSTATUS;
    pub fn NtAlpcCancelMessage(PortHandle: HANDLE, Flags: ULONG, MessageContext: PVOID) -> NTSTATUS;
    pub fn NtAlpcOpenSenderProcess(ProcessHandle: PHANDLE, PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, Flags: ULONG, DesiredAccess: ACCESS_MASK, ObjectAttributes: *const OBJECT_ATTRIBUTES) -> NTSTATUS;
    pub fn NtAlpcOpenSenderThread(ThreadHandle: PHANDLE, PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, Flags: ULONG, DesiredAccess: ACCESS_MASK, ObjectAttributes: *const OBJECT_ATTRIBUTES) -> NTSTATUS;
    pub fn NtAlpcImpersonateClientOfPort(PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, Flags: PVOID) -> NTSTATUS;
    pub fn NtAlpcImpersonateClientContainerOfPort(PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, Flags: ULONG) -> NTSTATUS;
}


const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const ALPC_MESSAGE_SECURITY_ATTRIBUTE: ULONG = 0x8000_0000;
const ALPC_MESSAGE_VIEW_ATTRIBUTE: ULONG = 0x4000_0000;
const ALPC_MESSAGE_CONTEXT_ATTRIBUTE: ULONG = 0x2000_0000;
const ALPC_MESSAGE_HANDLE_ATTRIBUTE: ULONG = 0x1000_0000;

#[repr(C)]
struct ALPC_CONTEXT_ATTR_NATIVE { PortContext: PVOID, MessageContext: PVOID, Sequence: ULONG, MessageId: ULONG, CallbackId: ULONG }
#[repr(C)]
struct ALPC_HANDLE_ATTR_NATIVE { Flags: ULONG, _padding: ULONG, Handle: HANDLE, ObjectType: ULONG, DesiredAccess: ACCESS_MASK }
#[repr(C)]
struct ALPC_PORT_COMPLETION_LIST_INFORMATION { Buffer: PVOID, Size: SIZE_T, ConcurrencyCount: ULONG, AttributeFlags: ULONG }

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value.checked_add(alignment - 1).map(|v| v & !(alignment - 1))
}
fn attribute_size(flag: ULONG) -> Option<usize> {
    match flag {
        ALPC_MESSAGE_SECURITY_ATTRIBUTE => Some(core::mem::size_of::<ALPC_SECURITY_ATTR>()),
        ALPC_MESSAGE_VIEW_ATTRIBUTE => Some(core::mem::size_of::<ALPC_DATA_VIEW_ATTR>()),
        ALPC_MESSAGE_CONTEXT_ATTRIBUTE => Some(core::mem::size_of::<ALPC_CONTEXT_ATTR_NATIVE>()),
        ALPC_MESSAGE_HANDLE_ATTRIBUTE => Some(core::mem::size_of::<ALPC_HANDLE_ATTR_NATIVE>()),
        _ => None,
    }
}

#[no_mangle]
pub extern "system" fn AlpcMaxAllowedMessageLength() -> ULONG { 512 }

#[no_mangle]
pub unsafe extern "system" fn AlpcRegisterCompletionList(
    PortHandle: HANDLE, Buffer: PVOID, Size: ULONG, ConcurrencyCount: ULONG, AttributeFlags: ULONG,
) -> NTSTATUS {
    if Buffer.is_null() || Size == 0 || ConcurrencyCount == 0 { return STATUS_INVALID_PARAMETER; }
    let info = ALPC_PORT_COMPLETION_LIST_INFORMATION { Buffer, Size: Size as SIZE_T, ConcurrencyCount, AttributeFlags };
    unsafe { NtAlpcSetInformation(PortHandle, 6, (&info as *const ALPC_PORT_COMPLETION_LIST_INFORMATION).cast_mut().cast(), core::mem::size_of_val(&info) as ULONG) }
}
#[no_mangle]
pub unsafe extern "system" fn AlpcUnregisterCompletionList(PortHandle: HANDLE) -> NTSTATUS {
    unsafe { NtAlpcSetInformation(PortHandle, 7, core::ptr::null_mut(), 0) }
}
#[no_mangle]
pub unsafe extern "system" fn AlpcRundownCompletionList(PortHandle: HANDLE) -> NTSTATUS {
    unsafe { NtAlpcSetInformation(PortHandle, 10, core::ptr::null_mut(), 0) }
}
#[no_mangle]
pub unsafe extern "system" fn AlpcAdjustCompletionListConcurrencyCount(PortHandle: HANDLE, ConcurrencyCount: ULONG) -> NTSTATUS {
    unsafe { NtAlpcSetInformation(PortHandle, 8, (&ConcurrencyCount as *const ULONG).cast_mut().cast(), core::mem::size_of::<ULONG>() as ULONG) }
}
#[no_mangle]
pub unsafe extern "system" fn AlpcRegisterCompletionListWorkerThread(_CompletionList: PVOID) -> BOOLEAN { 0 }
#[no_mangle]
pub unsafe extern "system" fn AlpcUnregisterCompletionListWorkerThread(_CompletionList: PVOID) -> BOOLEAN { 0 }
#[no_mangle]
pub unsafe extern "system" fn AlpcGetCompletionListLastMessageInformation(_CompletionList: PVOID, LastMessageId: PULONG, LastCallbackId: PULONG) {
    if !LastMessageId.is_null() { unsafe { LastMessageId.write(0); } }
    if !LastCallbackId.is_null() { unsafe { LastCallbackId.write(0); } }
}
#[no_mangle]
pub unsafe extern "system" fn AlpcGetOutstandingCompletionListMessageCount(_CompletionList: PVOID) -> ULONG { 0 }
#[no_mangle]
pub unsafe extern "system" fn AlpcGetMessageFromCompletionList(_CompletionList: PVOID, MessageAttributes: *mut PALPC_MESSAGE_ATTRIBUTES) -> *mut PORT_MESSAGE {
    if !MessageAttributes.is_null() { unsafe { MessageAttributes.write(core::ptr::null_mut()); } }
    core::ptr::null_mut()
}
#[no_mangle]
pub unsafe extern "system" fn AlpcFreeCompletionListMessage(_CompletionList: PVOID, _Message: *mut PORT_MESSAGE) {}
#[no_mangle]
pub unsafe extern "system" fn AlpcGetCompletionListMessageAttributes(_CompletionList: PVOID, _Message: *mut PORT_MESSAGE) -> PALPC_MESSAGE_ATTRIBUTES {
    core::ptr::null_mut()
}


// Wine-derived ALPC message-attribute helpers are consolidated here. The
// shared message header and security attribute use the canonical native ABI
// types above; legacy-only attribute layouts stay scoped to this helper module.
mod attribute_compat {
#![allow(non_camel_case_types, non_snake_case)]
use super::{NTSTATUS, PVOID, ULONG, SIZE_T, ALPC_MESSAGE_ATTRIBUTES, ALPC_SECURITY_ATTR};
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
use core::{ffi::c_void, mem::size_of};

const ALPC_MESSAGE_SECURITY_ATTRIBUTE: ULONG = super::ALPC_MESSAGE_SECURITY_ATTRIBUTE;
const ALPC_MESSAGE_VIEW_ATTRIBUTE: ULONG = super::ALPC_MESSAGE_VIEW_ATTRIBUTE;
const ALPC_MESSAGE_CONTEXT_ATTRIBUTE: ULONG = super::ALPC_MESSAGE_CONTEXT_ATTRIBUTE;
const ALPC_MESSAGE_HANDLE_ATTRIBUTE: ULONG = super::ALPC_MESSAGE_HANDLE_ATTRIBUTE;
pub const ALPC_MESSAGE_TOKEN_ATTRIBUTE: ULONG = 0x0800_0000;
pub const ALPC_MESSAGE_DIRECT_ATTRIBUTE: ULONG = 0x0400_0000;
pub const ALPC_MESSAGE_WORK_ON_BEHALF_ATTRIBUTE: ULONG = 0x0200_0000;
pub const ALPC_MESSAGE_ATTRIBUTE_ALL: ULONG = 0xFE00_0000;

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
pub struct ALPC_HANDLE_ATTR32 {
    pub Flags: ULONG,
    pub Reserved0: ULONG,
    pub SameAccess: ULONG,
    pub SameAttributes: ULONG,
    pub Indirect: ULONG,
    pub Inherit: ULONG,
    pub Reserved1: ULONG,
    pub Handle: ULONG,
    pub ObjectType: ULONG,
    pub DesiredAccess: ULONG,
    pub GrantedAccess: ULONG,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_HANDLE_ATTR {
    pub Flags: ULONG,
    pub Reserved0: ULONG,
    pub SameAccess: ULONG,
    pub SameAttributes: ULONG,
    pub Indirect: ULONG,
    pub Inherit: ULONG,
    pub Reserved1: ULONG,
    pub Handle: *mut c_void,
    pub HandleAttrArray: *mut ALPC_HANDLE_ATTR32,
    pub ObjectType: ULONG,
    pub HandleCount: ULONG,
    pub DesiredAccess: ULONG,
    pub GrantedAccess: ULONG,
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
pub extern "system" fn AlpcGetHeaderSize(attribute_flags: ULONG) -> ULONG {
    ATTRIBUTE_SIZES.iter().fold(
        size_of::<ALPC_MESSAGE_ATTRIBUTES>(),
        |total, (flag, size)| {
            if attribute_flags & *flag != 0 { total + *size } else { total }
        },
    ) as ULONG
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
        attributes.cast::<u8>().add(AlpcGetHeaderSize(higher_attributes) as usize).cast::<c_void>()
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
    buffer_size: ULONG,
    required_buffer_size: *mut ULONG,
) -> NTSTATUS {
    if required_buffer_size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }

    let required = AlpcGetHeaderSize(attribute_flags) as usize;
    // SAFETY: Required by this function's pointer contract.
    unsafe { required_buffer_size.write(required as ULONG) };

    if (buffer_size as usize) < required {
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
        assert_eq!(AlpcGetHeaderSize(0), size_of::<ALPC_MESSAGE_ATTRIBUTES>() as ULONG);
        assert_eq!(
            AlpcGetHeaderSize(ALPC_MESSAGE_SECURITY_ATTRIBUTE),
            (size_of::<ALPC_MESSAGE_ATTRIBUTES>() + size_of::<ALPC_SECURITY_ATTR>()) as ULONG
        );
        assert_eq!(
            AlpcGetHeaderSize(ALPC_MESSAGE_ATTRIBUTE_ALL | 1),
            (ATTRIBUTE_SIZES.iter().map(|(_, size)| *size).sum::<usize>()
                + size_of::<ALPC_MESSAGE_ATTRIBUTES>()) as ULONG
        );
    }

    #[test]
    fn initialize_reports_required_size_and_checks_capacity() {
        let flags = ALPC_MESSAGE_VIEW_ATTRIBUTE | ALPC_MESSAGE_TOKEN_ATTRIBUTE;
        let required = AlpcGetHeaderSize(flags);
        let mut reported = 0u32;
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
        let mut storage = vec![0u64; (total as usize + 7) / 8];
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
                (size_of::<ALPC_MESSAGE_ATTRIBUTES>() + size_of::<ALPC_SECURITY_ATTR>())
            );
            assert_eq!(
                token as usize - header as usize,
                (size_of::<ALPC_MESSAGE_ATTRIBUTES>()
                    + size_of::<ALPC_SECURITY_ATTR>()
                    + size_of::<ALPC_CONTEXT_ATTR>())
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
        assert_eq!(size_of::<ALPC_HANDLE_ATTR32>(), 44);
        assert_eq!(size_of::<ALPC_HANDLE_ATTR>(), 64);
        assert_eq!(size_of::<ALPC_TOKEN_ATTR>(), 24);
        assert_eq!(size_of::<ALPC_DIRECT_ATTR>(), 8);
        assert_eq!(size_of::<ALPC_WORK_ON_BEHALF_ATTR>(), 8);
    }
}

}

pub use attribute_compat::{
    AlpcGetHeaderSize, AlpcGetMessageAttribute, AlpcInitializeMessageAttribute,
    ALPC_CONTEXT_ATTR, ALPC_HANDLE_ATTR, ALPC_HANDLE_ATTR32, ALPC_TOKEN_ATTR,
    ALPC_DIRECT_ATTR, ALPC_WORK_ON_BEHALF_ATTR, ALPC_VIEW_ATTR,
    ALPC_MESSAGE_TOKEN_ATTRIBUTE, ALPC_MESSAGE_DIRECT_ATTRIBUTE,
    ALPC_MESSAGE_WORK_ON_BEHALF_ATTRIBUTE, ALPC_MESSAGE_ATTRIBUTE_ALL,
};

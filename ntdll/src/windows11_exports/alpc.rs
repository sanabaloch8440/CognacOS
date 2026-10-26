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
    pub SectionHandle: HANDLE, pub ViewBase: PVOID, pub ViewSize: SIZE_T,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ALPC_SECURITY_ATTR {
    pub Flags: ULONG, pub QoS: PVOID, pub ContextHandle: ULONG,
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
    .global \\name
\\name:
    mov r10, rcx
    mov eax, \\id
    int 0x83
    ret
    .endm
    ALPC_STUB NtAlpcCreatePort,                         0x11b
    ALPC_STUB NtAlpcDisconnectPort,                     0x11c
    ALPC_STUB NtAlpcConnectPort,                        0x11d
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
    pub fn NtAlpcAcceptConnectPort(PortHandle: PHANDLE, ConnectionPortHandle: HANDLE, Flags: ULONG, ObjectAttributes: *const OBJECT_ATTRIBUTES, PortAttributes: *const ALPC_PORT_ATTRIBUTES, PortContext: PVOID, ConnectionRequest: *const PORT_MESSAGE, ConnectionMessageAttributes: *mut ALPC_MESSAGE_ATTRIBUTES, AcceptConnection: u8) -> NTSTATUS;
    pub fn NtAlpcSendWaitReceivePort(PortHandle: HANDLE, Flags: ULONG, SendMessage: *const PORT_MESSAGE, SendMessageAttributes: *const ALPC_MESSAGE_ATTRIBUTES, ReceiveMessage: *mut PORT_MESSAGE, BufferLength: PSIZE_T, ReceiveMessageAttributes: *mut ALPC_MESSAGE_ATTRIBUTES, Timeout: PVOID) -> NTSTATUS;
    pub fn NtAlpcCreatePortSection(PortHandle: HANDLE, Flags: ULONG, SectionHandle: HANDLE, SectionSize: SIZE_T, AlpcSectionHandle: *mut ULONG, ActualSectionSize: PSIZE_T) -> NTSTATUS;
    pub fn NtAlpcDeletePortSection(PortHandle: HANDLE, Flags: ULONG, SectionHandle: ULONG) -> NTSTATUS;
    pub fn NtAlpcCreateSectionView(PortHandle: HANDLE, Flags: ULONG, ViewAttributes: *mut ALPC_DATA_VIEW_ATTR) -> NTSTATUS;
    pub fn NtAlpcDeleteSectionView(PortHandle: HANDLE, Flags: ULONG, ViewBase: PVOID) -> NTSTATUS;
    pub fn NtAlpcCreateSecurityContext(PortHandle: HANDLE, Flags: ULONG, SecurityAttribute: *mut ALPC_SECURITY_ATTR) -> NTSTATUS;
    pub fn NtAlpcDeleteSecurityContext(PortHandle: HANDLE, Flags: ULONG, ContextHandle: ULONG) -> NTSTATUS;
    pub fn NtAlpcRevokeSecurityContext(PortHandle: HANDLE, Flags: ULONG, ContextHandle: ULONG) -> NTSTATUS;
    pub fn NtAlpcCreateResourceReserve(PortHandle: HANDLE, Flags: ULONG, MessageSize: SIZE_T, ResourceId: *mut ULONG) -> NTSTATUS;
    pub fn NtAlpcDeleteResourceReserve(PortHandle: HANDLE, Flags: ULONG, ResourceId: ULONG) -> NTSTATUS;
    pub fn NtAlpcQueryInformation(PortHandle: HANDLE, PortInformationClass: ULONG, PortInformation: PVOID, Length: ULONG, ReturnLength: PULONG) -> NTSTATUS;
    pub fn NtAlpcSetInformation(PortHandle: HANDLE, PortInformationClass: ULONG, PortInformation: PVOID, Length: ULONG) -> NTSTATUS;
    pub fn NtAlpcQueryInformationMessage(PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, MessageInformationClass: ULONG, MessageInformation: PVOID, Length: ULONG, ReturnLength: PULONG) -> NTSTATUS;
    pub fn NtAlpcCancelMessage(PortHandle: HANDLE, Flags: ULONG, MessageContext: PVOID) -> NTSTATUS;
    pub fn NtAlpcOpenSenderProcess(ProcessHandle: PHANDLE, PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, Flags: ULONG, DesiredAccess: ACCESS_MASK, ObjectAttributes: *const OBJECT_ATTRIBUTES) -> NTSTATUS;
    pub fn NtAlpcOpenSenderThread(ThreadHandle: PHANDLE, PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, Flags: ULONG, DesiredAccess: ACCESS_MASK, ObjectAttributes: *const OBJECT_ATTRIBUTES) -> NTSTATUS;
    pub fn NtAlpcImpersonateClientOfPort(PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, Flags: ULONG) -> NTSTATUS;
    pub fn NtAlpcImpersonateClientContainerOfPort(PortHandle: HANDLE, PortMessage: *const PORT_MESSAGE, Flags: ULONG) -> NTSTATUS;
}

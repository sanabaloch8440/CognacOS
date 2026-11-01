#![no_std]
//! Shared Windows NT-style ABI declarations for CognacOS.
//!
//! This crate is the Rust equivalent of a small, layered set of WDK headers:
//! `windows` for common scalar/Unicode definitions, `ntddk` for executive
//! and kernel types, `wdm` for driver/I/O contracts, and `ntifs` for
//! filesystem-facing declarations. These modules describe ABI only; they do
//! not imply that every declared operation is implemented by the kernel.
//!
//! Keep wire/layout definitions here instead of redefining them in NTDLL,
//! ntoskrnl, HAL, and drivers. OS-specific behavior belongs in those crates.

pub mod ntstatus;
pub mod types;
pub mod object;
pub mod io;
pub mod mm;
pub mod security;

/// Common definitions analogous to the shared subset of Windows.h.
pub mod windows {
    pub use crate::ntstatus::{
        NTSTATUS, nt_success, STATUS_ACCESS_DENIED, STATUS_ACCESS_VIOLATION, STATUS_BUFFER_OVERFLOW, STATUS_DLL_NOT_FOUND,
        STATUS_BUFFER_TOO_SMALL, STATUS_INVALID_HANDLE, STATUS_INVALID_IMAGE_FORMAT, STATUS_INVALID_INFO_CLASS,
        STATUS_INVALID_PARAMETER, STATUS_INVALID_DEVICE_REQUEST, STATUS_NOT_IMPLEMENTED, STATUS_NO_MEMORY, STATUS_NOT_OWNER, STATUS_PROCEDURE_NOT_FOUND,
        STATUS_DELETE_PENDING, STATUS_DEVICE_BUSY, STATUS_INSUFFICIENT_RESOURCES, STATUS_OBJECT_NAME_COLLISION, STATUS_OBJECT_NAME_NOT_FOUND, STATUS_PENDING,
        STATUS_SUCCESS, STATUS_TIMEOUT, STATUS_UNSUCCESSFUL,
    };
    pub use crate::types::{
        BOOLEAN, GUID, HANDLE, KAFFINITY, LARGE_INTEGER, LIST_ENTRY, LONG, LONG64, NTSTATUS_TYPE, UCHAR,
        PCVOID, PVOID, PRTL_SPLAY_LINKS, RTL_SPLAY_LINKS, SIZE_T, ULONG, ULONG64, ULONG_PTR, USHORT, WCHAR,
    };
    pub use crate::object::{ANSI_STRING, PANSI_STRING, POBJECT_ATTRIBUTES, PUNICODE_STRING, UNICODE_STRING, OBJECT_ATTRIBUTES};
}

/// Kernel and executive declarations analogous to ntddk.h.
pub mod ntddk {
    pub use crate::io::{DEVICE_OBJECT, DRIVER_OBJECT, IO_STATUS_BLOCK, IRP, PDRIVER_DISPATCH};
    pub use crate::mm::{
        MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE,
        PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_GUARD, PAGE_NOACCESS,
        PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY,
    };
    pub use crate::object::{ANSI_STRING, PANSI_STRING, POBJECT_ATTRIBUTES, PUNICODE_STRING, OBJECT_ATTRIBUTES, UNICODE_STRING};
    pub use crate::types::{
        BOOLEAN, GUID, HANDLE, KAFFINITY, LARGE_INTEGER, LIST_ENTRY, LONG, LONG64,
        NTSTATUS_TYPE, PCVOID, PVOID, PRTL_SPLAY_LINKS, RTL_SPLAY_LINKS, SIZE_T, UCHAR, ULONG, ULONG64, ULONG_PTR, USHORT, WCHAR,
    };
    pub use crate::ntstatus::{
        NTSTATUS, STATUS_ACCESS_DENIED, STATUS_ACCESS_VIOLATION, STATUS_BUFFER_OVERFLOW,
        STATUS_BUFFER_TOO_SMALL, STATUS_DLL_NOT_FOUND, STATUS_INVALID_HANDLE, STATUS_INVALID_IMAGE_FORMAT, STATUS_INVALID_INFO_CLASS,
        STATUS_INVALID_PARAMETER, STATUS_INVALID_DEVICE_REQUEST, STATUS_NOT_IMPLEMENTED, STATUS_NO_MEMORY, STATUS_NOT_OWNER, STATUS_PROCEDURE_NOT_FOUND,
        STATUS_DELETE_PENDING, STATUS_DEVICE_BUSY, STATUS_INSUFFICIENT_RESOURCES, STATUS_OBJECT_NAME_COLLISION, STATUS_OBJECT_NAME_NOT_FOUND, STATUS_PENDING,
        STATUS_SUCCESS, STATUS_TIMEOUT, STATUS_UNSUCCESSFUL,
    };
    pub use crate::security::{ACCESS_MASK, GENERIC_MAPPING};
}

/// WDM driver-facing declaration surface.
pub mod wdm {
    pub use crate::io::{DEVICE_OBJECT, DRIVER_OBJECT, IO_STATUS_BLOCK, IRP, PDRIVER_DISPATCH};
    pub use crate::object::{ANSI_STRING, PANSI_STRING, OBJECT_ATTRIBUTES, POBJECT_ATTRIBUTES, PUNICODE_STRING, UNICODE_STRING};
    pub use crate::types::{
        BOOLEAN, HANDLE, LARGE_INTEGER, LIST_ENTRY, LONG, NTSTATUS_TYPE, PRTL_SPLAY_LINKS, PVOID, RTL_SPLAY_LINKS, SIZE_T, UCHAR,
        ULONG, ULONG_PTR, USHORT, WCHAR,
    };
    pub use crate::ntstatus::{
        NTSTATUS, STATUS_ACCESS_DENIED, STATUS_ACCESS_VIOLATION, STATUS_BUFFER_OVERFLOW,
        STATUS_BUFFER_TOO_SMALL, STATUS_DLL_NOT_FOUND, STATUS_INVALID_HANDLE,
        STATUS_INVALID_IMAGE_FORMAT, STATUS_INVALID_INFO_CLASS, STATUS_INVALID_PARAMETER,
        STATUS_INVALID_DEVICE_REQUEST, STATUS_NOT_IMPLEMENTED, STATUS_NO_MEMORY, STATUS_NOT_OWNER,
        STATUS_PROCEDURE_NOT_FOUND, STATUS_DELETE_PENDING, STATUS_DEVICE_BUSY,
        STATUS_INSUFFICIENT_RESOURCES, STATUS_OBJECT_NAME_COLLISION,
        STATUS_OBJECT_NAME_NOT_FOUND, STATUS_PENDING, STATUS_SUCCESS, STATUS_TIMEOUT,
        STATUS_UNSUCCESSFUL,
    };
}

/// Filesystem/IFS-facing declaration surface analogous to ntifs.h.
pub mod ntifs {
    pub use crate::io::{DEVICE_OBJECT, DRIVER_OBJECT, IO_STATUS_BLOCK, IRP, PDRIVER_DISPATCH};
    pub use crate::object::{ANSI_STRING, PANSI_STRING, OBJECT_ATTRIBUTES, POBJECT_ATTRIBUTES, PUNICODE_STRING, UNICODE_STRING};
    pub use crate::types::{
        BOOLEAN, HANDLE, LARGE_INTEGER, LIST_ENTRY, LONG, NTSTATUS_TYPE, PRTL_SPLAY_LINKS, PVOID, RTL_SPLAY_LINKS, SIZE_T, UCHAR,
        ULONG, ULONG_PTR, USHORT, WCHAR,
    };
    pub use crate::ntstatus::{
        NTSTATUS, STATUS_ACCESS_DENIED, STATUS_ACCESS_VIOLATION, STATUS_BUFFER_OVERFLOW,
        STATUS_BUFFER_TOO_SMALL, STATUS_DLL_NOT_FOUND, STATUS_INVALID_HANDLE,
        STATUS_INVALID_IMAGE_FORMAT, STATUS_INVALID_INFO_CLASS, STATUS_INVALID_PARAMETER,
        STATUS_INVALID_DEVICE_REQUEST, STATUS_NOT_IMPLEMENTED, STATUS_NO_MEMORY, STATUS_NOT_OWNER,
        STATUS_PROCEDURE_NOT_FOUND, STATUS_DELETE_PENDING, STATUS_DEVICE_BUSY,
        STATUS_INSUFFICIENT_RESOURCES, STATUS_OBJECT_NAME_COLLISION,
        STATUS_OBJECT_NAME_NOT_FOUND, STATUS_PENDING, STATUS_SUCCESS, STATUS_TIMEOUT,
        STATUS_UNSUCCESSFUL,
    };
    pub use crate::security::{ACCESS_MASK, GENERIC_MAPPING};
}


/// Versioned interfaces exchanged between bus/firmware drivers and ntoskrnl.
///
/// These are deliberately narrow, pointer-based WDM-style contracts: the
/// kernel owns PnP device nodes and resource arbitration, while bus drivers
/// own access to their firmware or bus configuration mechanisms. A consumer
/// must validate both `size` and `version` before calling any function.
pub mod bus {
    use crate::types::PVOID;
    use crate::ntstatus::NTSTATUS;

    pub const ACPI_FIRMWARE_INTERFACE_VERSION: u32 = 1;
    pub const PCI_BUS_INTERFACE_VERSION: u32 = 1;

    /// Stable interface identifiers used by IRP_MN_QUERY_INTERFACE.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct GUID {
        pub data1: u32,
        pub data2: u16,
        pub data3: u16,
        pub data4: [u8; 8],
    }
    pub const GUID_ACPI_FIRMWARE_INTERFACE: GUID = GUID {
        data1: 0x7b7a_2a01, data2: 0x5d31, data3: 0x4f18,
        data4: [0x91, 0x62, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43],
    };
    pub const GUID_PCI_BUS_INTERFACE: GUID = GUID {
        data1: 0x7b7a_2a02, data2: 0x5d31, data3: 0x4f18,
        data4: [0x91, 0x62, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43],
    };

    /// Common prefix for all published bus interfaces.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct INTERFACE_HEADER {
        pub size: u16,
        pub version: u16,
        pub context: PVOID,
        pub interface_reference: Option<unsafe extern "win64" fn(PVOID)>,
        pub interface_dereference: Option<unsafe extern "win64" fn(PVOID)>,
    }

    /// ACPI.sys firmware-table access. The provider copies the requested
    /// table into the caller's buffer and reports its required size.
    pub type PACPI_QUERY_TABLE = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            signature: *const u8,
            instance: u32,
            buffer: PVOID,
            buffer_length: u32,
            required_length: *mut u32,
        ) -> NTSTATUS,
    >;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ACPI_FIRMWARE_INTERFACE {
        pub header: INTERFACE_HEADER,
        pub query_table: PACPI_QUERY_TABLE,
    }

    /// PCI configuration-space access provided by pci.sys. Device and
    /// function are conventional PCI coordinates; segment and bus are
    /// explicit so callers cannot accidentally alias nonzero segments.
    pub type PPCI_READ_CONFIG = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            segment: u16,
            bus: u8,
            device: u8,
            function: u8,
            offset: u16,
            width: u8,
            value: *mut u32,
        ) -> NTSTATUS,
    >;
    pub type PPCI_WRITE_CONFIG = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            segment: u16,
            bus: u8,
            device: u8,
            function: u8,
            offset: u16,
            width: u8,
            value: u32,
        ) -> NTSTATUS,
    >;

    /// Stable bus-neutral PCI function record returned by pci.sys inventory.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct PCI_DEVICE_INFORMATION {
        pub segment: u16,
        pub bus: u8,
        pub device: u8,
        pub function: u8,
        pub header_type: u8,
        pub revision_id: u8,
        pub programming_interface: u8,
        pub subclass: u8,
        pub class_code: u8,
        pub reserved: u16,
        pub vendor_id: u16,
        pub device_id: u16,
        pub command: u16,
    }

    pub type PPCI_QUERY_COUNTS = Option<
        unsafe extern "win64" fn(context: PVOID, functions: *mut u32, bridges: *mut u32, xhci: *mut u32) -> NTSTATUS,
    >;
    pub type PPCI_QUERY_FUNCTION = Option<
        unsafe extern "win64" fn(context: PVOID, index: u32, output: *mut PCI_DEVICE_INFORMATION) -> NTSTATUS,
    >;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct PCI_BUS_INTERFACE {
        pub header: INTERFACE_HEADER,
        pub read_config: PPCI_READ_CONFIG,
        pub write_config: PPCI_WRITE_CONFIG,
        pub query_counts: PPCI_QUERY_COUNTS,
        pub query_function: PPCI_QUERY_FUNCTION,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn interface_layout_is_stable_on_x64() {
            if cfg!(target_pointer_width = "64") {
                assert_eq!(core::mem::size_of::<GUID>(), 0x10);
                assert_ne!(GUID_ACPI_FIRMWARE_INTERFACE, GUID_PCI_BUS_INTERFACE);
                assert_eq!(core::mem::size_of::<INTERFACE_HEADER>(), 0x20);
                assert_eq!(core::mem::offset_of!(INTERFACE_HEADER, context), 0x08);
                assert_eq!(core::mem::size_of::<ACPI_FIRMWARE_INTERFACE>(), 0x28);
                assert_eq!(core::mem::size_of::<PCI_BUS_INTERFACE>(), 0x40);
                assert_eq!(core::mem::size_of::<PCI_DEVICE_INFORMATION>(), 0x12);
            }
        }
    }
}

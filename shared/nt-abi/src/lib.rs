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
    pub use crate::io::{DEVICE_OBJECT, DRIVER_OBJECT, IO_STATUS_BLOCK, IRP, VPB, PDRIVER_DISPATCH, IO_TYPE_VPB, VPB_MOUNTED, VPB_LOCKED, VPB_PERSISTENT, VPB_REMOVE_PENDING, VPB_RAW_MOUNT, VPB_DIRECT_WRITES_ALLOWED, MAXIMUM_VOLUME_LABEL_LENGTH};
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
    pub use crate::io::{DEVICE_OBJECT, DRIVER_OBJECT, IO_STATUS_BLOCK, IRP, VPB, PDRIVER_DISPATCH, IO_TYPE_VPB, VPB_MOUNTED, VPB_LOCKED, VPB_PERSISTENT, VPB_REMOVE_PENDING, VPB_RAW_MOUNT, VPB_DIRECT_WRITES_ALLOWED, MAXIMUM_VOLUME_LABEL_LENGTH};
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
    /// Version 2 uses a distinct interface GUID, as required by the WDM
    /// driver-defined-interface versioning guidance. Version 1 and its layout
    /// remain binary compatible and are never enlarged in place.
    pub const ACPI_FIRMWARE_INTERFACE_VERSION_2: u32 = 2;
    pub const ACPI_FIRMWARE_INTERFACE_VERSION_3: u32 = 3;
    pub const ACPI_FIRMWARE_INTERFACE_VERSION_4: u32 = 4;
    /// Kernel-private PnP bus directory identity. These keys are not
    /// Windows PE exports or part of the public WDM export table.
    pub const ACPI_BUS_ENUMERATOR_ID: u32 = u32::from_le_bytes(*b"ACPI");
    pub const ACPI_BUS_IDENTITY_HASH: u64 = 0x4143_5049_4255_5301;
    pub const PCI_BUS_INTERFACE_VERSION: u32 = 3;

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
    /// ACPI firmware interface v2. A new GUID is used instead of changing
    /// the size/version contract behind GUID_ACPI_FIRMWARE_INTERFACE.
    pub const GUID_ACPI_FIRMWARE_INTERFACE_V2: GUID = GUID {
        data1: 0x7b7a_2a04, data2: 0x5d31, data3: 0x4f18,
        data4: [0x91, 0x62, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43],
    };
    pub const GUID_ACPI_FIRMWARE_INTERFACE_V3: GUID = GUID {
        data1: 0x7b7a_2a06, data2: 0x5d31, data3: 0x4f18,
        data4: [0x91, 0x62, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43],
    };
    pub const GUID_ACPI_FIRMWARE_INTERFACE_V4: GUID = GUID {
        data1: 0x7b7a_2a08, data2: 0x5d31, data3: 0x4f18,
        data4: [0x91, 0x62, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43],
    };
    pub const GUID_PCI_BUS_INTERFACE: GUID = GUID {
        data1: 0x7b7a_2a02, data2: 0x5d31, data3: 0x4f18,
        data4: [0x91, 0x62, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43],
    };

    /// Per-PCI-PDO identity interface. Unlike the bus-wide configuration
    /// interface, this is queried on an individual child PDO and reports only
    /// the immutable identity captured during enumeration.
    pub const PCI_DEVICE_INTERFACE_VERSION: u32 = 1;
    pub const GUID_PCI_DEVICE_INTERFACE: GUID = GUID {
        data1: 0x7b7a_2a03, data2: 0x5d31, data3: 0x4f18,
        data4: [0x91, 0x62, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43],
    };

    /// Per-PCI-PDO interrupt lifecycle interface. PCI configures its ACPI
    /// route masked during START_DEVICE; the function driver registers its
    /// ISR/DPC and explicitly enables delivery only after device initialization.
    pub const PCI_INTERRUPT_INTERFACE_VERSION: u32 = 1;
    pub const GUID_PCI_INTERRUPT_INTERFACE: GUID = GUID {
        data1: 0x7b7a_2a05, data2: 0x5d31, data3: 0x4f18,
        data4: [0x91, 0x62, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43],
    };
    pub type PCI_INTERRUPT_ISR = extern "C" fn(context: u64) -> bool;
    pub type PCI_INTERRUPT_DPC = extern "C" fn(context: u64);
    pub type PPCI_REGISTER_INTERRUPT = Option<unsafe extern "win64" fn(
        context: PVOID,
        callback_context: u64,
        isr: PCI_INTERRUPT_ISR,
        dpc: PCI_INTERRUPT_DPC,
    ) -> NTSTATUS>;
    pub type PPCI_SET_INTERRUPT_ENABLED = Option<unsafe extern "win64" fn(context: PVOID) -> NTSTATUS>;
    pub type PPCI_UNREGISTER_INTERRUPT = Option<unsafe extern "win64" fn(context: PVOID) -> NTSTATUS>;
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct PCI_INTERRUPT_INTERFACE {
        pub header: INTERFACE_HEADER,
        pub register: PPCI_REGISTER_INTERRUPT,
        pub enable: PPCI_SET_INTERRUPT_ENABLED,
        pub disable: PPCI_SET_INTERRUPT_ENABLED,
        pub unregister: PPCI_UNREGISTER_INTERRUPT,
    }
    const _: () = assert!(core::mem::size_of::<PCI_INTERRUPT_INTERFACE>() == 0x40);

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

    /// Microsoft WDM BUS_INTERFACE_STANDARD (wdm.h), queried with
    /// GUID_BUS_INTERFACE_STANDARD through IRP_MN_QUERY_INTERFACE.
    pub const BUS_INTERFACE_STANDARD_VERSION: u32 = 1;
    pub const PCI_WHICHSPACE_CONFIG: u32 = 0;
    pub const GUID_BUS_INTERFACE_STANDARD: GUID = GUID {
        data1: 0x496b_8280, data2: 0x6f25, data3: 0x11d0,
        data4: [0xbe, 0xaf, 0x08, 0x00, 0x2b, 0xe2, 0x09, 0x2f],
    };
    pub type PINTERFACE_REFERENCE = Option<unsafe extern "win64" fn(context: PVOID)>;
    pub type PINTERFACE_DEREFERENCE = Option<unsafe extern "win64" fn(context: PVOID)>;
    pub type PTRANSLATE_BUS_ADDRESS = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            bus_address: i64,
            address_space: u32,
            translated_address_space: *mut u32,
            translated_address: *mut i64,
        ) -> u8,
    >;
    pub type PGET_DMA_ADAPTER = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            device_description: PVOID,
            number_of_map_registers: *mut u32,
        ) -> PVOID,
    >;
    pub type PGET_SET_DEVICE_DATA = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            data_type: u32,
            buffer: PVOID,
            offset: u32,
            length: u32,
        ) -> u32,
    >;
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct BUS_INTERFACE_STANDARD {
        pub size: u16,
        pub version: u16,
        pub context: PVOID,
        pub interface_reference: PINTERFACE_REFERENCE,
        pub interface_dereference: PINTERFACE_DEREFERENCE,
        pub translate_bus_address: PTRANSLATE_BUS_ADDRESS,
        pub get_dma_adapter: PGET_DMA_ADAPTER,
        pub set_bus_data: PGET_SET_DEVICE_DATA,
        pub get_bus_data: PGET_SET_DEVICE_DATA,
    }

    const _: () = assert!(core::mem::size_of::<BUS_INTERFACE_STANDARD>() == 0x40);
    const _: () = assert!(core::mem::offset_of!(BUS_INTERFACE_STANDARD, get_bus_data) == 0x38);

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

    /// PCI coordinates used by the ACPI _PRT routing service. Pin is the
    /// PCI Interrupt Pin register value (1..=4), not the zero-based _PRT pin.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ACPI_PCI_INTERRUPT_ROUTE_REQUEST {
        pub segment: u16,
        pub bus: u8,
        pub device: u8,
        pub function: u8,
        pub pin: u8,
        pub reserved: u16,
    }

    /// One validated Type-1 bridge on the endpoint's root-to-leaf PCI path.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ACPI_PCI_BRIDGE_PATH_ENTRY {
        pub segment: u16,
        pub primary_bus: u8,
        pub secondary_bus: u8,
        pub subordinate_bus: u8,
        pub device: u8,
        pub function: u8,
        pub reserved: u8,
    }

    /// Extended route request. The original v2 request remains unchanged.
    /// Bridges are ordered from the root bus toward the endpoint bus.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V2 {
        pub segment: u16,
        pub bus: u8,
        pub device: u8,
        pub function: u8,
        pub pin: u8,
        pub bridge_count: u8,
        pub root_bus: u8,
        pub bridges: [ACPI_PCI_BRIDGE_PATH_ENTRY; 8],
    }
    impl Default for ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V2 {
        fn default() -> Self {
            Self { segment: 0, bus: 0, device: 0, function: 0, pin: 0,
                bridge_count: 0, root_bus: 0,
                bridges: [ACPI_PCI_BRIDGE_PATH_ENTRY::default(); 8] }
        }
    }

    /// Version-4 route request supports the complete 8-bit PCI bus-number
    /// hierarchy: at most 255 bridges between a root bus and an endpoint.
    /// The v1/v2/v3 request layouts remain unchanged.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3 {
        pub segment: u16,
        pub bus: u8,
        pub device: u8,
        pub function: u8,
        pub pin: u8,
        pub bridge_count: u8,
        pub root_bus: u8,
        pub bridges: [ACPI_PCI_BRIDGE_PATH_ENTRY; 255],
    }
    impl Default for ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3 {
        fn default() -> Self {
            Self { segment: 0, bus: 0, device: 0, function: 0, pin: 0,
                bridge_count: 0, root_bus: 0,
                bridges: [ACPI_PCI_BRIDGE_PATH_ENTRY::default(); 255] }
        }
    }

    /// Resolved electrical route. Boolean values are UCHAR-sized for a stable
    /// C ABI; reserved bytes must be zero on output.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ACPI_PCI_INTERRUPT_ROUTE {
        pub gsi: u32,
        pub active_low: u8,
        pub level_triggered: u8,
        pub reserved: [u8; 2],
        /// I/O-APIC physical base selected from the ACPI MADT for this GSI.
        pub io_apic_base: u64,
        /// First GSI owned by the selected I/O-APIC.
        pub gsi_base: u32,
        pub reserved2: u32,
    }

    pub type PACPI_QUERY_PCI_INTERRUPT_ROUTE = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            request: *const ACPI_PCI_INTERRUPT_ROUTE_REQUEST,
            route: *mut ACPI_PCI_INTERRUPT_ROUTE,
        ) -> NTSTATUS,
    >;

    /// ACPI firmware interface v2. The common WDM INTERFACE header remains
    /// first, and v1's query_table callback is preserved byte-for-byte before
    /// appending the PCI _PRT route query.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ACPI_FIRMWARE_INTERFACE_V2 {
        pub header: INTERFACE_HEADER,
        pub query_table: PACPI_QUERY_TABLE,
        pub query_pci_interrupt_route: PACPI_QUERY_PCI_INTERRUPT_ROUTE,
    }

    const _: () = assert!(core::mem::size_of::<ACPI_FIRMWARE_INTERFACE>() == 0x28);
    const _: () = assert!(core::mem::size_of::<ACPI_FIRMWARE_INTERFACE_V2>() == 0x30);
    pub type PACPI_QUERY_PCI_INTERRUPT_ROUTE_V2 = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            request: *const ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V2,
            route: *mut ACPI_PCI_INTERRUPT_ROUTE,
        ) -> NTSTATUS,
    >;

    /// Version 3 adds a bridge-path-aware route query without changing v1/v2 layouts.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ACPI_FIRMWARE_INTERFACE_V3 {
        pub header: INTERFACE_HEADER,
        pub query_table: PACPI_QUERY_TABLE,
        pub query_pci_interrupt_route: PACPI_QUERY_PCI_INTERRUPT_ROUTE,
        pub query_pci_interrupt_route_v2: PACPI_QUERY_PCI_INTERRUPT_ROUTE_V2,
    }

    const _: () = assert!(core::mem::offset_of!(ACPI_FIRMWARE_INTERFACE_V2, query_table) == 0x20);
    const _: () = assert!(core::mem::offset_of!(ACPI_FIRMWARE_INTERFACE_V2, query_pci_interrupt_route) == 0x28);
    const _: () = assert!(core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE_REQUEST>() == 8);
    const _: () = assert!(core::mem::size_of::<ACPI_PCI_BRIDGE_PATH_ENTRY>() == 8);
    const _: () = assert!(core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V2>() == 72);
    const _: () = assert!(core::mem::size_of::<ACPI_FIRMWARE_INTERFACE_V3>() == 0x38);
    const _: () = assert!(core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE>() == 24);
    pub type PACPI_QUERY_PCI_INTERRUPT_ROUTE_V3 = Option<
        unsafe extern "win64" fn(
            context: PVOID,
            request: *const ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3,
            route: *mut ACPI_PCI_INTERRUPT_ROUTE,
        ) -> NTSTATUS,
    >;

    /// Version 4 adds the full-depth bridge path callback under a new GUID.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ACPI_FIRMWARE_INTERFACE_V4 {
        pub header: INTERFACE_HEADER,
        pub query_table: PACPI_QUERY_TABLE,
        pub query_pci_interrupt_route: PACPI_QUERY_PCI_INTERRUPT_ROUTE,
        pub query_pci_interrupt_route_v2: PACPI_QUERY_PCI_INTERRUPT_ROUTE_V2,
        pub query_pci_interrupt_route_v3: PACPI_QUERY_PCI_INTERRUPT_ROUTE_V3,
    }

    const _: () = assert!(core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3>() == 2048);
    const _: () = assert!(core::mem::size_of::<ACPI_FIRMWARE_INTERFACE_V4>() == 0x40);
    const _: () = assert!(core::mem::offset_of!(ACPI_FIRMWARE_INTERFACE_V4, query_pci_interrupt_route_v3) == 0x38);


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

    pub type PPCI_QUERY_DEVICE_INFORMATION = Option<
        unsafe extern "win64" fn(context: PVOID, information: *mut PCI_DEVICE_INFORMATION) -> NTSTATUS,
    >;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct PCI_DEVICE_INTERFACE {
        pub header: INTERFACE_HEADER,
        pub query_information: PPCI_QUERY_DEVICE_INFORMATION,
    }

    /// Per-IDE-controller resources negotiated by pci.sys and exposed to
    /// pciide.sys through IRP_MN_QUERY_INTERFACE after START_DEVICE.
    pub const PCI_IDE_RESOURCE_INTERFACE_VERSION:u32=1;
    pub const GUID_PCI_IDE_RESOURCE_INTERFACE:GUID=GUID{
        data1:0x6e2a_1b71,data2:0x8d44,data3:0x4c21,
        data4:[0xa2,0x11,0x50,0x43,0x49,0x44,0x45,0x31],
    };
    #[repr(C)]
    #[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
    pub struct PCI_IDE_CHANNEL_RESOURCES{
        pub command_base:u16,
        pub control_base:u16,
        /// Bit 0 indicates native mode; bit 1 indicates the channel is switchable.
        pub flags:u32,
    }
    #[repr(C)]
    #[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
    pub struct PCI_IDE_RESOURCES{
        pub primary:PCI_IDE_CHANNEL_RESOURCES,
        pub secondary:PCI_IDE_CHANNEL_RESOURCES,
        pub bus_master_base:u16,
        /// Bit 0: bus-master capability; bit 1: assigned BAR4 is present.
        pub flags:u16,
    }
    pub type PPCI_QUERY_IDE_RESOURCES=Option<
        unsafe extern "win64" fn(context:PVOID,output:*mut PCI_IDE_RESOURCES)->NTSTATUS
    >;
    #[repr(C)]
    #[derive(Clone,Copy)]
    pub struct PCI_IDE_RESOURCE_INTERFACE{
        pub header:INTERFACE_HEADER,
        pub query_resources:PPCI_QUERY_IDE_RESOURCES,
    }

    /// MCFG-derived ECAM aperture mapped by NTOSKRNL and consumed by pci.sys.
    /// The driver uses the mapped virtual address directly; no kernel or HAL
    /// callback pointers are embedded in this handoff.
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct PCI_ECAM_WINDOW {
        pub virtual_base: u64,
        pub physical_base: u64,
        pub segment: u16,
        pub bus_start: u8,
        pub bus_end: u8,
    }

    pub const PCI_DRIVER_HANDOFF_MAGIC: u64 = 0x434f_474e_4143_5043;
    pub const PCI_DRIVER_HANDOFF_VERSION: u32 = 5;
    pub const PCI_DRIVER_HANDOFF_MAX_WINDOWS: usize = 16;

    /// Data-only initialization contract between NTOSKRNL and pci.sys.
    /// Device-object lifecycle is handled through the documented NTOSKRNL
    /// IoCreateDevice/IoDeleteDevice imports, not private callback pointers.
    #[repr(C)]
    pub struct PCI_DRIVER_HANDOFF {
        pub magic: u64,
        pub size: u32,
        pub version: u32,
        pub window_count: u32,
        pub reserved: u32,
        pub windows: [PCI_ECAM_WINDOW; PCI_DRIVER_HANDOFF_MAX_WINDOWS],
    }

    pub type PPCI_QUERY_COUNTS = Option<
        unsafe extern "win64" fn(context: PVOID, functions: *mut u32, bridges: *mut u32, xhci: *mut u32) -> NTSTATUS,
    >;
    /// Bridge topology entry published by pci.sys. The bridge's BDF is on
    /// primary_bus; child buses are in the inclusive secondary..subordinate range.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct PCI_BRIDGE_INFORMATION {
        pub segment: u16,
        pub primary_bus: u8,
        pub secondary_bus: u8,
        pub subordinate_bus: u8,
        pub device: u8,
        pub function: u8,
        pub reserved: u8,
    }

    pub type PPCI_QUERY_FUNCTION = Option<
        unsafe extern "win64" fn(context: PVOID, index: u32, output: *mut PCI_DEVICE_INFORMATION) -> NTSTATUS,
    >;
    /// xHCI endpoint information discovered by pci.sys. BAR sizing and
    /// mapping remain kernel resource-arbitration responsibilities.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct PCI_XHCI_INFORMATION {
        pub segment: u16,
        pub root_bus: u8,
        pub bus: u8,
        pub device: u8,
        pub function: u8,
        pub vendor_id: u16,
        pub device_id: u16,
        pub command: u16,
        pub interrupt_line: u8,
        pub interrupt_pin: u8,
        pub bar_is_64bit: u8,
        pub bar_prefetchable: u8,
        pub reserved: u16,
        pub bar_physical: u64,
    }

    pub type PPCI_QUERY_XHCI = Option<
        unsafe extern "win64" fn(context: PVOID, index: u32, output: *mut PCI_XHCI_INFORMATION) -> NTSTATUS,
    >;

    pub type PPCI_QUERY_BRIDGE = Option<
        unsafe extern "win64" fn(context: PVOID, index: u32, output: *mut PCI_BRIDGE_INFORMATION) -> NTSTATUS,
    >;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct PCI_BUS_INTERFACE {
        pub header: INTERFACE_HEADER,
        pub read_config: PPCI_READ_CONFIG,
        pub write_config: PPCI_WRITE_CONFIG,
        pub query_counts: PPCI_QUERY_COUNTS,
        pub query_function: PPCI_QUERY_FUNCTION,
        pub query_bridge: PPCI_QUERY_BRIDGE,
        pub query_xhci: PPCI_QUERY_XHCI,
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
                assert_eq!(core::mem::size_of::<ACPI_FIRMWARE_INTERFACE_V2>(), 0x30);
                assert_eq!(core::mem::size_of::<ACPI_FIRMWARE_INTERFACE_V3>(), 0x38);
                assert_eq!(core::mem::size_of::<ACPI_FIRMWARE_INTERFACE_V4>(), 0x40);
                assert_eq!(core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3>(), 2048);
                assert_ne!(GUID_ACPI_FIRMWARE_INTERFACE_V3, GUID_ACPI_FIRMWARE_INTERFACE_V4);
                assert_eq!(core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V2>(), 72);
                assert_eq!(core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE_REQUEST>(), 8);
                assert_eq!(core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE>(), 24);
                assert_eq!(core::mem::size_of::<PCI_INTERRUPT_INTERFACE>(), 0x40);
                assert_ne!(GUID_PCI_INTERRUPT_INTERFACE, GUID_PCI_DEVICE_INTERFACE);
                assert_ne!(GUID_ACPI_FIRMWARE_INTERFACE, GUID_ACPI_FIRMWARE_INTERFACE_V2);
                assert_eq!(core::mem::size_of::<PCI_BRIDGE_INFORMATION>(), 0x08);
                assert_eq!(core::mem::size_of::<PCI_BUS_INTERFACE>(), 0x50);
                assert_eq!(core::mem::size_of::<PCI_DEVICE_INTERFACE>(), 0x28);
                assert_eq!(core::mem::size_of::<PCI_IDE_CHANNEL_RESOURCES>(), 0x08);
                assert_eq!(core::mem::size_of::<PCI_IDE_RESOURCES>(), 0x14);
                assert_eq!(core::mem::size_of::<PCI_IDE_RESOURCE_INTERFACE>(), 0x28);
                assert_ne!(GUID_PCI_DEVICE_INTERFACE, GUID_PCI_BUS_INTERFACE);
                assert_eq!(core::mem::size_of::<PCI_XHCI_INFORMATION>(), 0x20);
                assert_eq!(core::mem::size_of::<PCI_DEVICE_INFORMATION>(), 0x12);
                assert_eq!(core::mem::size_of::<PCI_ECAM_WINDOW>(), 0x18);
                assert_eq!(core::mem::size_of::<PCI_DRIVER_HANDOFF>(), 0x198);
            }
        }
    }
}

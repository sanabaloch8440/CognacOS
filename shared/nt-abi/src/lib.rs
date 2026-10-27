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
    pub use crate::ntstatus::{NTSTATUS, STATUS_INVALID_PARAMETER, STATUS_NOT_IMPLEMENTED, STATUS_SUCCESS};
    pub use crate::types::{BOOLEAN, HANDLE, LARGE_INTEGER, NTSTATUS_TYPE, SIZE_T, ULONG, ULONG_PTR, USHORT, WCHAR};
}

/// Kernel and executive declarations analogous to ntddk.h.
pub mod ntddk {
    pub use crate::io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STATUS_BLOCK};
    pub use crate::object::{OBJECT_ATTRIBUTES, UNICODE_STRING};
    pub use crate::types::{BOOLEAN, HANDLE, LARGE_INTEGER, NTSTATUS_TYPE, SIZE_T, ULONG, ULONG_PTR, USHORT, WCHAR};
    pub use crate::ntstatus::{NTSTATUS, STATUS_INVALID_PARAMETER, STATUS_NOT_IMPLEMENTED, STATUS_SUCCESS};
}

/// WDM driver-facing declaration surface.
pub mod wdm {
    pub use crate::io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STATUS_BLOCK};
    pub use crate::object::{OBJECT_ATTRIBUTES, UNICODE_STRING};
    pub use crate::types::{BOOLEAN, HANDLE, LARGE_INTEGER, NTSTATUS_TYPE, SIZE_T, ULONG, ULONG_PTR, USHORT, WCHAR};
    pub use crate::ntstatus::NTSTATUS;
}

/// Filesystem/IFS-facing declaration surface analogous to ntifs.h.
pub mod ntifs {
    pub use crate::io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STATUS_BLOCK};
    pub use crate::object::{OBJECT_ATTRIBUTES, UNICODE_STRING};
    pub use crate::types::{BOOLEAN, HANDLE, LARGE_INTEGER, NTSTATUS_TYPE, SIZE_T, ULONG, ULONG_PTR, USHORT, WCHAR};
    pub use crate::ntstatus::NTSTATUS;
    pub use crate::security::{ACCESS_MASK, GENERIC_MAPPING};
}

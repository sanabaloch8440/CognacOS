//! Shared WDM I/O ABI layouts. Runtime ownership and synchronization stay in ntoskrnl.
//!
//! The IRP and device/driver structures below are initial compatibility layouts,
//! not yet validated against a pinned Windows 11 WDK. Do not expose them to
//! third-party binary drivers until layout tests and field-by-field auditing pass.

use crate::ntstatus::NTSTATUS;
use crate::object::UNICODE_STRING;
use crate::types::{HANDLE, PVOID, ULONG, ULONG_PTR};

#[repr(C)]
#[derive(Clone, Copy)]
pub union IO_STATUS_BLOCK_STATUS {
    pub status: NTSTATUS,
    pub pointer: PVOID,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IO_STATUS_BLOCK {
    pub status: IO_STATUS_BLOCK_STATUS,
    pub information: ULONG_PTR,
}

impl Default for IO_STATUS_BLOCK {
    fn default() -> Self {
        Self {
            status: IO_STATUS_BLOCK_STATUS { status: 0 },
            information: 0,
        }
    }
}

#[repr(C)]
pub struct DRIVER_OBJECT {
    pub type_: i16,
    pub size: i16,
    pub device_object: *mut DEVICE_OBJECT,
    pub flags: ULONG,
    pub driver_start: PVOID,
    pub driver_size: ULONG,
    pub driver_section: PVOID,
    pub driver_extension: PVOID,
    pub driver_name: UNICODE_STRING,
    pub hardware_database: *mut UNICODE_STRING,
    pub fast_io_dispatch: PVOID,
    pub driver_init: PVOID,
    pub driver_start_io: PVOID,
    pub driver_unload: PVOID,
    pub major_function: [PVOID; 28],
}

#[repr(C)]
pub struct DEVICE_OBJECT {
    pub type_: i16,
    pub size: u16,
    pub reference_count: i32,
    pub driver_object: *mut DRIVER_OBJECT,
    pub next_device: *mut DEVICE_OBJECT,
    pub attached_device: *mut DEVICE_OBJECT,
    pub current_irp: *mut IRP,
    pub flags: ULONG,
    pub characteristics: ULONG,
    pub reserved: ULONG,
}

#[repr(C)]
pub struct IRP {
    pub type_: i16,
    pub size: u16,
    pub mdl_address: PVOID,
    pub flags: ULONG,
    pub associated_irp: [usize; 2],
    pub thread_list_entry: [usize; 2],
    pub io_status: IO_STATUS_BLOCK,
    pub requestor_mode: i8,
    pub pending_returned: u8,
    pub stack_count: i8,
    pub current_location: i8,
    pub cancel: u8,
    pub cancel_irql: u8,
    pub apc_environment: i8,
    pub allocation_flags: u8,
    pub user_iosb: *mut IO_STATUS_BLOCK,
    pub user_event: HANDLE,
    pub overlay: [usize; 2],
    pub cancel_routine: PVOID,
    pub user_buffer: PVOID,
    pub tail: [usize; 8],
}

pub type PDRIVER_DISPATCH = Option<unsafe extern "system" fn(*mut DEVICE_OBJECT, *mut IRP) -> NTSTATUS>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_status_block_matches_x64_size_and_alignment() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(core::mem::size_of::<IO_STATUS_BLOCK>(), 16);
            assert_eq!(core::mem::align_of::<IO_STATUS_BLOCK>(), 8);
        }
    }
}

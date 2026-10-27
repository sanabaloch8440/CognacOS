//! Shared WDM I/O ABI layouts. Runtime ownership and synchronization stay in ntoskrnl.

use crate::ntstatus::NTSTATUS;
use crate::object::UNICODE_STRING;
use crate::types::{HANDLE, ULONG, ULONG_PTR};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct IO_STATUS_BLOCK {
    pub status_or_pointer: ULONG_PTR,
    pub information: ULONG_PTR,
}

#[repr(C)]
pub struct DRIVER_OBJECT {
    pub type_: i16,
    pub size: i16,
    pub device_object: *mut DEVICE_OBJECT,
    pub flags: ULONG,
    pub driver_start: *mut core::ffi::c_void,
    pub driver_size: ULONG,
    pub driver_section: *mut core::ffi::c_void,
    pub driver_extension: *mut core::ffi::c_void,
    pub driver_name: UNICODE_STRING,
    pub hardware_database: *mut UNICODE_STRING,
    pub fast_io_dispatch: *mut core::ffi::c_void,
    pub driver_init: *mut core::ffi::c_void,
    pub driver_start_io: *mut core::ffi::c_void,
    pub driver_unload: *mut core::ffi::c_void,
    pub major_function: [*mut core::ffi::c_void; 28],
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
    pub mdl_address: *mut core::ffi::c_void,
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
    pub cancel_routine: *mut core::ffi::c_void,
    pub user_buffer: *mut core::ffi::c_void,
    pub tail: [usize; 8],
}

pub type PDRIVER_DISPATCH = Option<unsafe extern "system" fn(*mut DEVICE_OBJECT, *mut IRP) -> NTSTATUS>;

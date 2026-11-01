//! Shared WDM I/O ABI layouts. Runtime ownership and synchronization stay in ntoskrnl.
//!
//! The x64 structures below follow the public WDK field layout for the fields
//! represented here; compile-time tests pin key offsets. This does not imply
//! complete WDM behavioral compatibility or validate every union member.

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

/// Windows x64 SECTION_OBJECT_POINTERS used to connect a stream's file,
/// image, and shared-cache section state between the Cache Manager and MM.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SECTION_OBJECT_POINTERS {
    pub data_section_object: PVOID,
    pub shared_cache_map: PVOID,
    pub image_section_object: PVOID,
}

/// Windows WDM volume parameter block. This is the public x64 VPB layout
/// from wdm.h; the I/O manager owns allocation and the RealDevice/Type/Size
/// members, while the mounted file system owns its volume object and label.
pub const IO_TYPE_VPB: i16 = 0x0b;
pub const VPB_MOUNTED: u16 = 0x0001;
pub const VPB_LOCKED: u16 = 0x0002;
pub const VPB_PERSISTENT: u16 = 0x0004;
pub const VPB_REMOVE_PENDING: u16 = 0x0008;
pub const VPB_RAW_MOUNT: u16 = 0x0010;
pub const VPB_DIRECT_WRITES_ALLOWED: u16 = 0x0020;
pub const MAXIMUM_VOLUME_LABEL_LENGTH: usize = 32 * core::mem::size_of::<u16>();

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VPB {
    pub type_: i16,
    pub size: i16,
    pub flags: u16,
    pub volume_label_length: u16,
    pub device_object: *mut DEVICE_OBJECT,
    pub real_device: *mut DEVICE_OBJECT,
    pub serial_number: ULONG,
    pub reference_count: ULONG,
    pub volume_label: [u16; 32],
}

impl VPB {
    pub const fn empty() -> Self {
        Self {
            type_: IO_TYPE_VPB,
            size: core::mem::size_of::<Self>() as i16,
            flags: 0,
            volume_label_length: 0,
            device_object: core::ptr::null_mut(),
            real_device: core::ptr::null_mut(),
            serial_number: 0,
            reference_count: 0,
            volume_label: [0; 32],
        }
    }
}

pub type PDRIVER_DISPATCH = Option<unsafe extern "win64" fn(*mut DEVICE_OBJECT, *mut IRP) -> NTSTATUS>;
pub type PDRIVER_UNLOAD = Option<unsafe extern "win64" fn(*mut DRIVER_OBJECT)>;
pub type PIO_COMPLETION_ROUTINE = Option<unsafe extern "win64" fn(*mut DEVICE_OBJECT, *mut IRP, PVOID) -> NTSTATUS>;

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
    pub driver_unload: PDRIVER_UNLOAD,
    pub major_function: [PDRIVER_DISPATCH; 28],
}

#[repr(C, align(16))]
pub struct DEVICE_OBJECT {
    pub type_: i16,
    pub size: u16,
    pub reference_count: i32,
    pub driver_object: *mut DRIVER_OBJECT,
    pub next_device: *mut DEVICE_OBJECT,
    pub attached_device: *mut DEVICE_OBJECT,
    pub current_irp: *mut IRP,
    pub timer: PVOID,
    pub flags: ULONG,
    pub characteristics: ULONG,
    pub vpb: PVOID,
    pub device_extension: PVOID,
    pub device_type: ULONG,
    pub stack_size: i8,
    pub queue_padding: [u8; 3],
    pub queue: [u8; 0x48],
    pub alignment_requirement: ULONG,
    pub device_queue_padding: [u8; 4],
    pub device_queue: [u8; 0x28],
    pub dpc: [u8; 0x40],
    pub active_thread_count: ULONG,
    pub security_descriptor: PVOID,
    pub device_lock: [u8; 0x18],
    pub sector_size: u16,
    pub spare1: u16,
    pub device_object_extension: PVOID,
    pub reserved: PVOID,
}

#[repr(C)]
pub struct IRP {
    pub type_: i16,
    pub size: u16,
    pub mdl_address: PVOID,
    pub flags: ULONG,
    /// The WDK union at 0x18 is pointer-sized on x64.
    pub associated_irp: [usize; 1],
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
    /// Tail begins at 0x78 and extends to the fixed x64 IRP size (0xD0).
    pub tail: [usize; 11],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IO_STACK_LOCATION {
    pub major_function: u8,
    pub minor_function: u8,
    pub flags: u8,
    pub control: u8,
    /// Raw bytes preserve the WDK Parameters union's x64 field alignment,
    /// including POINTER_ALIGNMENT members in Read/Write/DeviceIoControl.
    pub parameters: [u8; 0x20],
    pub device_object: *mut DEVICE_OBJECT,
    pub file_object: PVOID,
    pub completion_routine: PIO_COMPLETION_ROUTINE,
    pub context: PVOID,
}

pub const FO_SYNCHRONOUS_IO: u32 = 0x0000_0002;
pub const FO_SEQUENTIAL_ONLY: u32 = 0x0000_0004;
pub const FO_NO_INTERMEDIATE_BUFFERING: u32 = 0x0000_0008;
pub const FO_CACHE_SUPPORTED: u32 = 0x0000_0040;
pub const FO_STREAM_FILE: u32 = 0x0000_0100;
pub const FO_DIRECT_DEVICE_OPEN: u32 = 0x0000_0200;
pub const FO_FILE_MODIFIED: u32 = 0x0000_0400;
pub const FO_FILE_SIZE_CHANGED: u32 = 0x0000_0800;
pub const FO_DELETE_ON_CLOSE: u32 = 0x0000_1000;
pub const FO_CLEANUP_COMPLETE: u32 = 0x0000_8000;

/// Windows x64 FILE_OBJECT layout. Event/list storage remains opaque to drivers.
#[repr(C)]
pub struct FILE_OBJECT {
    pub type_: i16, pub size: i16,
    pub device_object: *mut DEVICE_OBJECT, pub vpb: *mut VPB,
    pub fs_context: PVOID, pub fs_context2: PVOID,
    pub section_object_pointer: PVOID, pub private_cache_map: PVOID,
    pub final_status: NTSTATUS, pub related_file_object: *mut FILE_OBJECT,
    pub lock_operation: u8, pub delete_pending: u8, pub read_access: u8,
    pub write_access: u8, pub delete_access: u8, pub shared_read: u8,
    pub shared_write: u8, pub shared_delete: u8, pub flags: ULONG,
    pub file_name: UNICODE_STRING, pub current_byte_offset: i64,
    pub waiters: ULONG, pub busy: ULONG, pub last_lock: PVOID,
    pub lock: [u8; 0x18], pub event: [u8; 0x18],
    pub completion_context: PVOID, pub irp_list_lock: usize,
    pub irp_list: [usize; 2], pub file_object_extension: PVOID,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_x64_file_object_layout_matches_wdk() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(core::mem::size_of::<FILE_OBJECT>(), 0xd8);
            assert_eq!(core::mem::align_of::<FILE_OBJECT>(), 8);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, device_object), 0x08);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, vpb), 0x10);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, fs_context), 0x18);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, final_status), 0x38);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, related_file_object), 0x40);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, flags), 0x50);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, file_name), 0x58);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, current_byte_offset), 0x68);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, lock), 0x80);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, event), 0x98);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, completion_context), 0xb0);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, irp_list), 0xc0);
            assert_eq!(core::mem::offset_of!(FILE_OBJECT, file_object_extension), 0xd0);
        }
    }

    #[test]
    fn section_object_pointers_match_windows_x64_layout() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(core::mem::size_of::<SECTION_OBJECT_POINTERS>(), 0x18);
            assert_eq!(core::mem::offset_of!(SECTION_OBJECT_POINTERS, data_section_object), 0x00);
            assert_eq!(core::mem::offset_of!(SECTION_OBJECT_POINTERS, shared_cache_map), 0x08);
            assert_eq!(core::mem::offset_of!(SECTION_OBJECT_POINTERS, image_section_object), 0x10);
        }
    }

    #[test]
    fn windows_x64_vpb_layout_matches_wdk() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(core::mem::size_of::<VPB>(), 0x60);
            assert_eq!(core::mem::align_of::<VPB>(), 8);
            assert_eq!(core::mem::offset_of!(VPB, type_), 0x00);
            assert_eq!(core::mem::offset_of!(VPB, size), 0x02);
            assert_eq!(core::mem::offset_of!(VPB, flags), 0x04);
            assert_eq!(core::mem::offset_of!(VPB, volume_label_length), 0x06);
            assert_eq!(core::mem::offset_of!(VPB, device_object), 0x08);
            assert_eq!(core::mem::offset_of!(VPB, real_device), 0x10);
            assert_eq!(core::mem::offset_of!(VPB, serial_number), 0x18);
            assert_eq!(core::mem::offset_of!(VPB, reference_count), 0x1c);
            assert_eq!(core::mem::offset_of!(VPB, volume_label), 0x20);
        }
    }

    #[test]
    fn io_status_block_matches_x64_size_and_alignment() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(core::mem::size_of::<IO_STATUS_BLOCK_STATUS>(), 8);
            assert_eq!(core::mem::align_of::<IO_STATUS_BLOCK_STATUS>(), 8);
            assert_eq!(core::mem::size_of::<IO_STATUS_BLOCK>(), 16);
            assert_eq!(core::mem::align_of::<IO_STATUS_BLOCK>(), 8);
            assert_eq!(core::mem::offset_of!(IO_STATUS_BLOCK, status), 0);
            assert_eq!(core::mem::offset_of!(IO_STATUS_BLOCK, information), 8);
        }
    }

    #[test]
    fn windows_x64_irp_and_stack_location_offsets_match_wdk_layout() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(core::mem::size_of::<IRP>(), 0xD0);
            assert_eq!(core::mem::offset_of!(IRP, mdl_address), 0x08);
            assert_eq!(core::mem::offset_of!(IRP, flags), 0x10);
            assert_eq!(core::mem::offset_of!(IRP, associated_irp), 0x18);
            assert_eq!(core::mem::offset_of!(IRP, thread_list_entry), 0x20);
            assert_eq!(core::mem::offset_of!(IRP, io_status), 0x30);
            assert_eq!(core::mem::offset_of!(IRP, requestor_mode), 0x40);
            assert_eq!(core::mem::offset_of!(IRP, user_iosb), 0x48);
            assert_eq!(core::mem::offset_of!(IRP, user_event), 0x50);
            assert_eq!(core::mem::offset_of!(IRP, cancel_routine), 0x68);
            assert_eq!(core::mem::offset_of!(IRP, user_buffer), 0x70);
            assert_eq!(core::mem::offset_of!(IRP, tail), 0x78);
            assert_eq!(core::mem::size_of::<IO_STACK_LOCATION>(), 0x48);
            assert_eq!(core::mem::offset_of!(IO_STACK_LOCATION, parameters), 0x04);
            assert_eq!(core::mem::offset_of!(IO_STACK_LOCATION, device_object), 0x28);
            assert_eq!(core::mem::offset_of!(IO_STACK_LOCATION, file_object), 0x30);
            assert_eq!(core::mem::offset_of!(IO_STACK_LOCATION, completion_routine), 0x38);
            assert_eq!(core::mem::offset_of!(IO_STACK_LOCATION, context), 0x40);
        }
    }

    #[test]
    fn windows_x64_driver_and_device_object_offsets_match_wdk_layout() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(core::mem::size_of::<DRIVER_OBJECT>(), 0x150);
            assert_eq!(core::mem::offset_of!(DRIVER_OBJECT, device_object), 0x08);
            assert_eq!(core::mem::offset_of!(DRIVER_OBJECT, driver_start), 0x18);
            assert_eq!(core::mem::offset_of!(DRIVER_OBJECT, driver_name), 0x38);
            assert_eq!(core::mem::offset_of!(DRIVER_OBJECT, driver_unload), 0x68);
            assert_eq!(core::mem::offset_of!(DRIVER_OBJECT, major_function), 0x70);
            assert_eq!(core::mem::size_of::<DEVICE_OBJECT>(), 0x150);
            assert_eq!(core::mem::align_of::<DEVICE_OBJECT>(), 16);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, driver_object), 0x08);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, current_irp), 0x20);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, flags), 0x30);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, device_extension), 0x40);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, stack_size), 0x4c);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, queue), 0x50);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, device_queue), 0xa0);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, dpc), 0xc8);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, device_object_extension), 0x138);
            assert_eq!(core::mem::offset_of!(DEVICE_OBJECT, reserved), 0x140);
        }
    }
}

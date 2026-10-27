//! Shared memory-manager constants and layouts.

use crate::types::{ULONG, ULONG_PTR, SIZE_T};

pub const PAGE_SIZE: SIZE_T = 4096;
pub const PAGE_SHIFT: u32 = 12;
pub const PAGE_NOACCESS: ULONG = 0x01;
pub const PAGE_READONLY: ULONG = 0x02;
pub const PAGE_READWRITE: ULONG = 0x04;
pub const PAGE_WRITECOPY: ULONG = 0x08;
pub const PAGE_EXECUTE: ULONG = 0x10;
pub const PAGE_EXECUTE_READ: ULONG = 0x20;
pub const PAGE_EXECUTE_READWRITE: ULONG = 0x40;
pub const PAGE_GUARD: ULONG = 0x100;
pub const MEM_COMMIT: ULONG = 0x1000;
pub const MEM_RESERVE: ULONG = 0x2000;
pub const MEM_DECOMMIT: ULONG = 0x4000;
pub const MEM_RELEASE: ULONG = 0x8000;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MEMORY_BASIC_INFORMATION {
    pub base_address: *mut core::ffi::c_void,
    pub allocation_base: *mut core::ffi::c_void,
    pub allocation_protect: ULONG,
    pub partition_id: u16,
    pub region_size: SIZE_T,
    pub state: ULONG,
    pub protect: ULONG,
    pub type_: ULONG,
}

pub const fn align_down_page(address: ULONG_PTR) -> ULONG_PTR {
    address & !(PAGE_SIZE - 1)
}

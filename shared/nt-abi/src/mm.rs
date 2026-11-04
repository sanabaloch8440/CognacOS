//! Shared memory-manager constants and layouts.

use crate::types::{ULONG, ULONG_PTR, SIZE_T};

pub const PAGE_SIZE: SIZE_T = 4096;

/// Versioned kernel-to-bus-driver snapshot envelope. The entries are physical
/// RAM ranges from the authoritative Memory Manager map, not allocator free
/// pages. Bus drivers must reserve every listed byte from BAR placement.
pub const MM_PROTECTED_MEMORY_SNAPSHOT_VERSION: u32 = 1;
pub const MM_PROTECTED_MEMORY_SNAPSHOT_MAX_RANGES: usize = 256;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PHYSICAL_MEMORY_RANGE {
    pub base_address: i64,
    pub number_of_bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MM_PROTECTED_MEMORY_SNAPSHOT {
    pub version: u32,
    pub size: u32,
    pub count: u32,
    pub reserved: u32,
    pub ranges: [PHYSICAL_MEMORY_RANGE; MM_PROTECTED_MEMORY_SNAPSHOT_MAX_RANGES],
}
impl MM_PROTECTED_MEMORY_SNAPSHOT {
    pub const fn empty() -> Self {
        Self {
            version: MM_PROTECTED_MEMORY_SNAPSHOT_VERSION,
            size: core::mem::size_of::<Self>() as u32,
            count: 0,
            reserved: 0,
            ranges: [PHYSICAL_MEMORY_RANGE { base_address: 0, number_of_bytes: 0 }; MM_PROTECTED_MEMORY_SNAPSHOT_MAX_RANGES],
        }
    }
}

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
#[derive(Clone, Copy, Debug)]
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

impl Default for MEMORY_BASIC_INFORMATION {
    fn default() -> Self {
        Self {
            base_address: core::ptr::null_mut(),
            allocation_base: core::ptr::null_mut(),
            allocation_protect: 0,
            partition_id: 0,
            region_size: 0,
            state: 0,
            protect: 0,
            type_: 0,
        }
    }
}

pub const fn align_down_page(address: ULONG_PTR) -> ULONG_PTR {
    address & !(PAGE_SIZE - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_memory_snapshot_has_stable_x64_layout() {
        assert_eq!(core::mem::size_of::<PHYSICAL_MEMORY_RANGE>(), 16);
        assert_eq!(core::mem::size_of::<MM_PROTECTED_MEMORY_SNAPSHOT>(), 16 + 256 * 16);
        assert_eq!(core::mem::offset_of!(MM_PROTECTED_MEMORY_SNAPSHOT, ranges), 16);
        let snapshot = MM_PROTECTED_MEMORY_SNAPSHOT::empty();
        assert_eq!(snapshot.version, MM_PROTECTED_MEMORY_SNAPSHOT_VERSION);
        assert_eq!(snapshot.size as usize, core::mem::size_of_val(&snapshot));
        assert_eq!(snapshot.count, 0);
    }

    #[test]
    fn memory_basic_information_matches_x64_size() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(core::mem::size_of::<MEMORY_BASIC_INFORMATION>(), 48);
            assert_eq!(core::mem::align_of::<MEMORY_BASIC_INFORMATION>(), 8);
        }
    }

    #[test]
    fn page_alignment_rounds_down_without_changing_aligned_values() {
        assert_eq!(align_down_page(0x1234), 0x1000);
        assert_eq!(align_down_page(0x2000), 0x2000);
    }
}

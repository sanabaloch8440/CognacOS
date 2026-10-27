//! Canonical NT scalar and pointer-sized types.
//! Use fixed-width aliases so ABI widths do not depend on host assumptions.

pub type BOOLEAN = u8;
pub type WCHAR = u16;
pub type USHORT = u16;
pub type ULONG = u32;
pub type ULONG64 = u64;
pub type LONG = i32;
pub type LONG64 = i64;
pub type NTSTATUS_TYPE = i32;
pub type HANDLE = *mut core::ffi::c_void;
pub type PVOID = *mut core::ffi::c_void;
pub type PCVOID = *const core::ffi::c_void;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;
pub type KAFFINITY = usize;

/// Intrusive Windows RTL splay-tree links, shared by native RTL and kernel users.
/// The root node's Parent points to itself; child links are null when absent.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(non_camel_case_types)]
pub struct RTL_SPLAY_LINKS {
    pub Parent: *mut RTL_SPLAY_LINKS,
    pub LeftChild: *mut RTL_SPLAY_LINKS,
    pub RightChild: *mut RTL_SPLAY_LINKS,
}

pub type PRTL_SPLAY_LINKS = *mut RTL_SPLAY_LINKS;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LARGE_INTEGER {
    pub quad_part: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LIST_ENTRY {
    pub flink: *mut LIST_ENTRY,
    pub blink: *mut LIST_ENTRY,
}

impl Default for LIST_ENTRY {
    fn default() -> Self {
        Self {
            flink: core::ptr::null_mut(),
            blink: core::ptr::null_mut(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[cfg(test)]
mod abi_layout_tests {
    use super::*;

    #[test]
    fn rtl_splay_links_match_three_pointer_layout() {
        assert_eq!(core::mem::size_of::<RTL_SPLAY_LINKS>(), 3 * core::mem::size_of::<usize>());
        assert_eq!(core::mem::align_of::<RTL_SPLAY_LINKS>(), core::mem::align_of::<usize>());
    }
}

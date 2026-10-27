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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LARGE_INTEGER {
    pub quad_part: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LIST_ENTRY {
    pub flink: *mut LIST_ENTRY,
    pub blink: *mut LIST_ENTRY,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

//! Modular NTOSKRNL RTL runtime; submodules are grouped by functionality.
pub(crate) mod nls;
mod bitmap;
mod memory;
mod strings;
mod security;
mod misc;
mod conversion;
mod strings_extra;
mod sid;
mod conversion_extra;
mod misc_extra;
mod bitmap_extra;
mod network;
mod bitmap_ex;
mod system_helpers;
mod acl;
mod acl_extra;
mod acl_more;
mod strings_abi;
mod network_extra;
mod network_ipv6;
mod run_once;
mod misc_api;
pub mod bitmap_abi;
mod sid_abi;
mod abi_extra;
mod sd_convert;
mod strings_numeric;
mod strings_alloc;
mod pool;
#[path = "pool_page.rs"] mod pool_page;
mod timezone;


pub use bitmap::*;
pub use memory::*;
pub use strings::*;
pub use security::*;
pub use misc::*;
pub use conversion::*;
pub use strings_extra::*;
pub use sid::*;
pub use conversion_extra::*;
pub use misc_extra::*;
pub use bitmap_extra::*;
pub use network::*;
pub use bitmap_ex::*;
pub use system_helpers::*;
pub use acl::*;
pub use acl_extra::*;
pub use acl_more::*;
pub use strings_abi::*;
pub use network_extra::*;
pub use network_ipv6::*;
pub use run_once::*;
pub use misc_api::*;
pub use bitmap_abi::*;
pub use sid_abi::*;
pub use abi_extra::*;
pub use sd_convert::*;
pub use strings_numeric::*;
pub use strings_alloc::*;
pub use timezone::set_time_zone_bias_minutes;



/// Kernel-internal tagged nonpaged allocation interface. This currently uses
/// the bounded RTL arena; callers must handle allocation failure and must not
/// assume that the pool can satisfy arbitrary-size requests.
pub(crate) fn pool_allocate_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 {
    pool::allocate_tagged(size, tag, zeroed)
}

/// Kernel-internal demand-zero pageable pool interface. Fault-in is supported;
/// page-out/backing-store I/O remains unavailable until a block/pagefile driver exists.
pub(crate) fn pool_allocate_paged_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 {
    pool::allocate_paged_tagged(size, tag, zeroed)
}

/// Release a pool block, optionally checking its allocation tag.
pub(crate) fn pool_free_tagged(ptr: *mut u8, expected_tag: Option<u32>) -> bool {
    pool::deallocate_tagged(ptr, expected_tag)
}

/// Inspect metadata only for an exact live pool allocation pointer.
pub(crate) fn pool_allocation_info(ptr: *const u8) -> Option<(usize, u32)> {
    pool::allocation_info(ptr)
}

#[repr(C)]
pub struct RtlBitmap { pub size_of_bitmap: u32, pub buffer: *mut u32 }
#[repr(C)]
pub struct AnsiString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u8 }
#[repr(C)]
pub struct UnicodeString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u16 }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Luid { pub low_part: u32, pub high_part: i32 }

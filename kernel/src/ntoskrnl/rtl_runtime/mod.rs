//! Modular NTOSKRNL RTL runtime; submodules are grouped by functionality.
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
mod run_once;
mod misc_api;
mod bitmap_abi;
mod sid_abi;
mod abi_extra;
mod sd_convert;


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
pub use run_once::*;
pub use misc_api::*;
pub use bitmap_abi::*;
pub use sid_abi::*;
pub use abi_extra::*;
pub use sd_convert::*;



#[repr(C)]
pub struct RtlBitmap { pub size_of_bitmap: u32, pub buffer: *mut u32 }
#[repr(C)]
pub struct AnsiString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u8 }
#[repr(C)]
pub struct UnicodeString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u16 }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Luid { pub low_part: u32, pub high_part: i32 }


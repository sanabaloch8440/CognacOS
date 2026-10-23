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

pub use bitmap::*;
pub use memory::*;
pub use strings::*;
pub use security::*;
pub use misc::*;
pub use conversion::*;
pub use strings_extra::*;
pub use sid::*;
pub use conversion_extra::*;


#[repr(C)]
pub struct RtlBitmap { pub size_of_bitmap: u32, pub buffer: *mut u32 }
#[repr(C)]
pub struct AnsiString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u8 }
#[repr(C)]
pub struct UnicodeString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u16 }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Luid { pub low_part: u32, pub high_part: i32 }


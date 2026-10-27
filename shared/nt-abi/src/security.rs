//! Shared access-mask definitions; authorization policy belongs to ntoskrnl/se.

use crate::types::ULONG;

pub type ACCESS_MASK = ULONG;

pub const DELETE: ACCESS_MASK = 0x0001_0000;
pub const READ_CONTROL: ACCESS_MASK = 0x0002_0000;
pub const WRITE_DAC: ACCESS_MASK = 0x0004_0000;
pub const WRITE_OWNER: ACCESS_MASK = 0x0008_0000;
pub const SYNCHRONIZE: ACCESS_MASK = 0x0010_0000;
pub const STANDARD_RIGHTS_REQUIRED: ACCESS_MASK = 0x000F_0000;
pub const GENERIC_ALL: ACCESS_MASK = 0x1000_0000;
pub const GENERIC_EXECUTE: ACCESS_MASK = 0x2000_0000;
pub const GENERIC_WRITE: ACCESS_MASK = 0x4000_0000;
pub const GENERIC_READ: ACCESS_MASK = 0x8000_0000;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GENERIC_MAPPING {
    pub generic_read: ACCESS_MASK,
    pub generic_write: ACCESS_MASK,
    pub generic_execute: ACCESS_MASK,
    pub generic_all: ACCESS_MASK,
}

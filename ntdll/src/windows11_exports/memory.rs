//! Compatibility facade for NTDLL memory-related exports.
//! Implementations are separated by subsystem under `memory/`.
#![allow(non_snake_case, clippy::missing_safety_doc)]

mod virtual_memory;
mod zones;
mod streams;
mod nonvolatile;
mod resources;

pub use virtual_memory::*;
pub use zones::*;
pub use streams::*;
pub use nonvolatile::*;
pub use resources::*;

// Internal items used by the module-level ABI tests.
pub(super) use virtual_memory::{STATUS_INVALID_PARAMETER, STATUS_NOT_IMPLEMENTED, STATUS_SUCCESS};
pub(super) use streams::{S_OK, STREAM_SEEK_SET};
pub(super) use resources::STATUS_UNSUCCESSFUL;

/// Shared native bridge used by the owning security module.
pub(crate) unsafe fn native_check_token_membership(token: HANDLE, sid: PVOID, member: *mut u8, flags: ULONG) -> NTSTATUS {
    unsafe { virtual_memory::cognacos_rtl_check_token_membership(token, sid, member, flags) }
}

#[cfg(test)]
mod tests;

//! Compatibility facade for NTDLL memory-related exports.
//! Implementations are separated by subsystem under `memory/`.
#![allow(non_snake_case, clippy::missing_safety_doc)]

mod virtual_memory;
mod zones;
mod streams;
mod nonvolatile;
mod resources;
mod security;

pub use virtual_memory::*;
pub use zones::*;
pub use streams::*;
pub use nonvolatile::*;
pub use resources::*;
pub use security::*;

// Internal items used by the module-level ABI tests.
pub(super) use virtual_memory::{STATUS_INVALID_PARAMETER, STATUS_NOT_IMPLEMENTED, STATUS_SUCCESS};
pub(super) use streams::{S_OK, STREAM_SEEK_SET};
pub(super) use resources::STATUS_UNSUCCESSFUL;

#[cfg(test)]
mod tests;

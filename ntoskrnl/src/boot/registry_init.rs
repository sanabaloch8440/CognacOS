//! Early Configuration Manager bring-up for the native NT boot path.
//!
//! Boot hives are staged and mounted by the UEFI handoff layer. This module
//! validates the mounted SYSTEM hive and resolves SYSTEM\\Select before any
//! registry-configured boot driver is allowed to enter DriverEntry.
use crate::ntoskrnl::cm::{
    driver_config::{self, ControlSetSelection},
    hive::Hive,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegistryInitError {
    MissingSystemHive,
    InvalidSystemHive,
    InvalidControlSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemRegistryState {
    pub control_sets: ControlSetSelection,
}

/// Validate the mounted SYSTEM hive and its active control-set selection.
///
/// The hive remains owned by Configuration Manager; the returned structure is
/// a small immutable snapshot for boot policy, not a second registry store.
pub(crate) fn initialize_system_registry(
) -> Result<SystemRegistryState, RegistryInitError> {
    let bytes = crate::boot_handoff::boot_hive_bytes("SYSTEM")
        .ok_or(RegistryInitError::MissingSystemHive)?;
    let hive = Hive::open(bytes).map_err(|_| RegistryInitError::InvalidSystemHive)?;
    let control_sets = driver_config::query_control_set_selection(&hive)
        .map_err(|_| RegistryInitError::InvalidControlSet)?;

    Ok(SystemRegistryState { control_sets })
}

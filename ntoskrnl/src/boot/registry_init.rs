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
    pub boot_driver_count: usize,
    pub boot_driver_services: &'static [[u16; 128]],
}

// Boot runs this before secondary processors and the general scheduler are
// online. The fixed storage therefore remains stable for the next boot phase.
static mut BOOT_DRIVER_SERVICE_NAMES: [[u16; 128]; 64] = [[0; 128]; 64];


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
    let names = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(BOOT_DRIVER_SERVICE_NAMES).cast::<[u16; 128]>(),
            64,
        )
    };
    let boot_driver_count =
        crate::ntoskrnl::cm::service_config::enumerate_boot_driver_services(&hive, names)
            .map_err(|_| RegistryInitError::InvalidSystemHive)?;
    // Publish only the populated prefix as an immutable boot snapshot. This
    // storage is never modified after Configuration Manager initialization.
    let boot_driver_services = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(BOOT_DRIVER_SERVICE_NAMES).cast::<[u16; 128]>(),
            boot_driver_count,
        )
    };

    Ok(SystemRegistryState {
        control_sets,
        boot_driver_count,
        boot_driver_services,
    })
}

//! Windows-style boot orchestration primitives.
//!
//! This module deliberately separates kernel boot milestones from user-mode
//! service startup. A phase is published only after its owning subsystem has
//! completed successfully; merely loading an image or discovering a device is
//! not treated as a started service/device.

use core::sync::atomic::{AtomicU8, Ordering};

use crate::ntoskrnl::{
    cm::driver_config::ControlSetSelection,
    pnp::{self, PnpDeviceSnapshot, MAX_PNP_DEVICES, PNP_DEVICE_REMOVED, PNP_NO_PARENT},
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootStage {
    Firmware = 0,
    Registry = 1,
    BootDrivers = 2,
    DeviceTree = 3,
    SystemDrivers = 4,
    SessionManager = 5,
    ServiceControlManager = 6,
    Running = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageError {
    OutOfOrder { current: BootStage, requested: BootStage },
    AlreadyAdvanced,
}

impl BootStage {
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Firmware,
            1 => Self::Registry,
            2 => Self::BootDrivers,
            3 => Self::DeviceTree,
            4 => Self::SystemDrivers,
            5 => Self::SessionManager,
            6 => Self::ServiceControlManager,
            _ => Self::Running,
        }
    }
}

/// Per-boot monotonic milestone tracker. It is intentionally not a readiness
/// substitute for any individual driver or process; callers advance it only
/// after the real operation has returned success.
pub struct BootCoordinator {
    stage: AtomicU8,
}

impl BootCoordinator {
    pub const fn new() -> Self {
        Self { stage: AtomicU8::new(BootStage::Firmware as u8) }
    }

    pub fn stage(&self) -> BootStage {
        BootStage::from_raw(self.stage.load(Ordering::Acquire))
    }

    pub fn advance(&self, requested: BootStage) -> Result<(), StageError> {
        let current = self.stage();
        if requested as u8 != current as u8 + 1 {
            return Err(StageError::OutOfOrder { current, requested });
        }
        self.stage.compare_exchange(
            current as u8,
            requested as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ).map(|_| ()).map_err(|_| StageError::AlreadyAdvanced)
    }
}

impl Default for BootCoordinator {
    fn default() -> Self { Self::new() }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlSetAttempt {
    Current(u32),
    LastKnownGood(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryError {
    MissingLastKnownGood,
    InvalidSelection,
    RecoveryAlreadyAttempted,
    NonCriticalFailure,
}

/// Select the normal boot control set unless SYSTEM\\Select\\Failed says
/// that this exact Current set failed on the preceding boot. In that case use
/// the validated LastKnownGood set once, rather than silently substituting
/// Default.
pub fn initial_control_set(
    selection: ControlSetSelection,
) -> Result<ControlSetAttempt, RecoveryError> {
    if selection.current == 0 || selection.current > 999 {
        return Err(RecoveryError::InvalidSelection);
    }
    if selection.failed == Some(selection.current) {
        let lkg = selection.last_known_good.ok_or(RecoveryError::MissingLastKnownGood)?;
        if lkg == 0 || lkg > 999 || lkg == selection.current {
            return Err(RecoveryError::InvalidSelection);
        }
        return Ok(ControlSetAttempt::LastKnownGood(lkg));
    }
    Ok(ControlSetAttempt::Current(selection.current))
}

/// A severe/critical boot-driver failure may retry the validated LastKnownGood
/// control set exactly once. Ignore/normal error-control failures do not trigger
/// rollback. The caller must persist the failed-set/rollback result transactionally
/// before rebooting; this pure policy function never mutates a SYSTEM hive.
pub fn retry_last_known_good(
    selection: ControlSetSelection,
    attempted: ControlSetAttempt,
    error_control: u32,
    recovery_already_attempted: bool,
) -> Result<ControlSetAttempt, RecoveryError> {
    if error_control < 2 { return Err(RecoveryError::NonCriticalFailure); }
    if recovery_already_attempted || matches!(attempted, ControlSetAttempt::LastKnownGood(_)) {
        return Err(RecoveryError::RecoveryAlreadyAttempted);
    }
    let lkg = selection.last_known_good.ok_or(RecoveryError::MissingLastKnownGood)?;
    if lkg == 0 || lkg > 999 || lkg == selection.current {
        return Err(RecoveryError::InvalidSelection);
    }
    Ok(ControlSetAttempt::LastKnownGood(lkg))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceTreeEntry {
    pub device: PnpDeviceSnapshot,
}

/// Copy the current kernel PnP devnode table into a caller-owned immutable
/// snapshot. Parent references are checked in a second pass so enumeration
/// order does not affect validation. Removed and empty slots are omitted.
/// Validate the live devnode hierarchy without allocating or changing it.
/// The bounded ancestor walk rejects missing parents and cycles.
pub fn validate_device_tree() -> Result<usize, DeviceTreeError> {
    let mut count = 0usize;
    for index in 0..MAX_PNP_DEVICES {
        let Some(device) = pnp::query_pnp_device(index as u32) else { continue };
        if device.state == PNP_DEVICE_REMOVED { continue; }
        count += 1;
        let mut parent = device.parent;
        let mut depth = 0usize;
        while parent != PNP_NO_PARENT {
            if depth >= MAX_PNP_DEVICES { return Err(DeviceTreeError::ParentCycle); }
            let Some(ancestor) = pnp::query_pnp_device(parent) else {
                return Err(DeviceTreeError::MissingParent);
            };
            if ancestor.state == PNP_DEVICE_REMOVED { return Err(DeviceTreeError::MissingParent); }
            if ancestor.index == device.index { return Err(DeviceTreeError::ParentCycle); }
            parent = ancestor.parent;
            depth += 1;
        }
    }
    Ok(count)
}

pub fn build_device_tree(output: &mut [DeviceTreeEntry]) -> Result<usize, DeviceTreeError> {
    let mut count = 0usize;
    for index in 0..MAX_PNP_DEVICES {
        let Some(device) = pnp::query_pnp_device(index as u32) else { continue };
        if device.state == PNP_DEVICE_REMOVED { continue; }
        if count == output.len() { return Err(DeviceTreeError::BufferTooSmall); }
        output[count] = DeviceTreeEntry { device };
        count += 1;
    }
    for entry in output.iter().take(count) {
        let parent = entry.device.parent;
        if parent == PNP_NO_PARENT { continue; }
        if !output.iter().take(count).any(|candidate| candidate.device.index == parent) {
            return Err(DeviceTreeError::MissingParent);
        }
    }
    Ok(count)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceTreeError {
    BufferTooSmall,
    MissingParent,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection() -> ControlSetSelection {
        ControlSetSelection {
            current: 1,
            default: Some(1),
            last_known_good: Some(2),
            failed: None,
        }
    }

    #[test]
    fn boot_stages_are_strictly_ordered() {
        let coordinator = BootCoordinator::new();
        assert_eq!(coordinator.stage(), BootStage::Firmware);
        assert_eq!(coordinator.advance(BootStage::BootDrivers),
            Err(StageError::OutOfOrder { current: BootStage::Firmware, requested: BootStage::BootDrivers }));
        for stage in [
            BootStage::Registry, BootStage::BootDrivers, BootStage::DeviceTree,
            BootStage::SystemDrivers, BootStage::SessionManager,
            BootStage::ServiceControlManager, BootStage::Running,
        ] {
            assert_eq!(coordinator.advance(stage), Ok(()));
        }
        assert_eq!(coordinator.stage(), BootStage::Running);
    }

    #[test]
    fn previous_failure_selects_lkg_not_default() {
        let mut state = selection();
        state.failed = Some(1);
        assert_eq!(initial_control_set(state), Ok(ControlSetAttempt::LastKnownGood(2)));
    }

    #[test]
    fn normal_boot_uses_current_not_default() {
        let mut state = selection();
        state.current = 3;
        state.default = Some(4);
        assert_eq!(initial_control_set(state), Ok(ControlSetAttempt::Current(3)));
    }

    #[test]
    fn only_severe_or_critical_driver_failure_can_trigger_one_lkg_retry() {
        let state = selection();
        assert_eq!(retry_last_known_good(state, ControlSetAttempt::Current(1), 1, false),
            Err(RecoveryError::NonCriticalFailure));
        assert_eq!(retry_last_known_good(state, ControlSetAttempt::Current(1), 2, false),
            Ok(ControlSetAttempt::LastKnownGood(2)));
        assert_eq!(retry_last_known_good(state, ControlSetAttempt::Current(1), 3, true),
            Err(RecoveryError::RecoveryAlreadyAttempted));
        assert_eq!(retry_last_known_good(state, ControlSetAttempt::LastKnownGood(2), 3, false),
            Err(RecoveryError::RecoveryAlreadyAttempted));
    }

    #[test]
    fn invalid_or_missing_lkg_is_never_guessed() {
        let mut state = selection();
        state.last_known_good = None;
        assert_eq!(retry_last_known_good(state, ControlSetAttempt::Current(1), 3, false),
            Err(RecoveryError::MissingLastKnownGood));
        state.last_known_good = Some(1000);
        assert_eq!(retry_last_known_good(state, ControlSetAttempt::Current(1), 3, false),
            Err(RecoveryError::InvalidSelection));
    }
}

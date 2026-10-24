//! NT power-manager runtime and ABI entry points.
//!
//! Power-specific ABI and system power state live here. Device D-state metadata
//! remains owned by the I/O manager's device-object lifetime records.
use core::sync::atomic::{AtomicU32, Ordering};
use crate::io::{self, WdmIrp};
use crate::io::DeviceObject;

#[repr(C)]
#[derive(Clone, Copy)]
pub union PowerState {
    pub system_state: u32,
    pub device_state: u32,
}

static SYSTEM_POWER_STATE: AtomicU32 = AtomicU32::new(1);

/// Report a committed system/device power state and return the previous state.
/// This bookkeeping entry point does not perform a hardware transition or
/// replace IRP_MJ_POWER dispatch, PnP sequencing, or asynchronous completion.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoSetPowerState(
    device: *mut DeviceObject,
    state_type: u32,
    new_state: PowerState,
) -> PowerState {
    match state_type {
        0 => {
            let next = unsafe { new_state.system_state };
            // SYSTEM_POWER_STATE values S0..S5 plus the internal shutdown slot.
            if next > 6 {
                return PowerState { system_state: 0 };
            }
            let old = SYSTEM_POWER_STATE.swap(next, Ordering::AcqRel);
            PowerState { system_state: old }
        }
        1 => {
            if device.is_null() {
                return PowerState { device_state: 0 };
            }
            // WDM DEVICE_POWER_STATE values are Unspecified=0, D0=1..D3=4.
            let raw = unsafe { new_state.device_state };
            if !(1..=4).contains(&raw) {
                return PowerState { device_state: 0 };
            }
            match io::io_set_device_power_state(device, (raw - 1) as u8) {
                Some(previous) => PowerState { device_state: previous as u32 + 1 },
                None => PowerState { device_state: 0 },
            }
        }
        _ => PowerState { device_state: 0 },
    }
}

/// Kept for legacy WDM drivers; modern power IRPs still require real dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoStartNextPowerIrp(_irp: *mut WdmIrp) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::{io_create_device, io_delete_device, DriverObject};
    use core::ptr;

    #[test]
    fn power_state_reports_previous_device_and_system_states() {
        let mut driver: DriverObject = unsafe { core::mem::zeroed() };
        let mut device: *mut DeviceObject = ptr::null_mut();
        assert_eq!(unsafe { io_create_device(&mut driver, 0, ptr::null_mut(), 7, 0, 0, &mut device) }, 0);
        let old = unsafe { PoSetPowerState(device, 1, PowerState { device_state: 4 }) };
        assert_eq!(unsafe { old.device_state }, 1);
        assert_eq!(crate::io::device_power_state(device), 3);
        let previous = unsafe { PoSetPowerState(ptr::null_mut(), 0, PowerState { system_state: 3 }) };
        assert_eq!(unsafe { previous.system_state }, 1);
        unsafe { io_delete_device(device); }
    }

    #[test]
    fn invalid_power_state_does_not_modify_device() {
        let mut driver: DriverObject = unsafe { core::mem::zeroed() };
        let mut device: *mut DeviceObject = ptr::null_mut();
        assert_eq!(unsafe { io_create_device(&mut driver, 0, ptr::null_mut(), 7, 0, 0, &mut device) }, 0);
        let result = unsafe { PoSetPowerState(device, 1, PowerState { device_state: 5 }) };
        assert_eq!(unsafe { result.device_state }, 0);
        assert_eq!(crate::io::device_power_state(device), 0);
        unsafe { io_delete_device(device); }
    }
}

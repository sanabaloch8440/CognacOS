//! Central, dependency-aware initialization of registry-selected boot drivers.
//!
//! The caller supplies a bounded candidate snapshot decoded from the active
//! SYSTEM ControlSet. This manager owns the ordering-to-loading transition,
//! validates each candidate again against the hive immediately before loading,
//! preserves DRIVER_OBJECT/name storage, and applies ErrorControl policy.
//! Automatic/Demand services remain the responsibility of the later PnP lifecycle.

use core::ffi::c_void;
use crate::ntoskrnl::driver_loader::{self, DriverObject, LoadedDriver, RegistryDriverLoadError};
use crate::ntoskrnl::driver_order::{self, GroupTagOrder, ScheduleError, ServiceNode};
use crate::ntoskrnl::cm::hive::Hive;

#[derive(Clone, Copy, Debug)]
pub enum DriverInitStatus {
    NotRun,
    Loaded(LoadedDriver),
    Failed(RegistryDriverLoadError),
}
#[derive(Clone, Copy, Debug)]
pub struct DriverInitRecord {
    pub service_index: usize,
    pub error_control: u32,
    pub status: DriverInitStatus,
}
impl DriverInitRecord {
    const EMPTY: Self = Self { service_index: usize::MAX, error_control: 0, status: DriverInitStatus::NotRun };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverInitError { BufferTooSmall, InvalidServiceName, Schedule(ScheduleError) }
#[derive(Clone, Copy, Debug, Default)]
pub struct BootInitReport {
    pub scheduled: usize,
    pub attempted: usize,
    pub loaded: usize,
    pub failed: usize,
    pub fatal_service: Option<usize>,
}

fn abort_boot(error_control: u32) -> bool {
    // Severe/critical failures require LastKnownGood rollback, not yet implemented.
    error_control >= 2
}

fn name_eq(a: &[u16], b: &[u16]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| {
        let fold = |c: u16| if (b'A' as u16..=b'Z' as u16).contains(&c) { c + 32 } else { c };
        fold(*x) == fold(*y)
    })
}

fn dependencies_initialized(
    index: usize,
    nodes: &[ServiceNode<'_>],
    records: &[DriverInitRecord],
) -> bool {
    let node = &nodes[index];
    for dependency in node.service_dependencies {
        let Some(dep_index) = nodes.iter().position(|candidate| name_eq(candidate.name, dependency)) else {
            return false;
        };
        if !matches!(records[dep_index].status, DriverInitStatus::Loaded(_)) {
            return false;
        }
    }
    for dependency in node.group_dependencies {
        if !nodes.iter().enumerate().any(|(candidate_index, candidate)| {
            name_eq(candidate.group, dependency)
                && matches!(records[candidate_index].status, DriverInitStatus::Loaded(_))
        }) {
            return false;
        }
    }
    true
}

/// Schedule and initialize Boot/System-start candidates in one allocation-free
/// pass. The caller supplies a bounded candidate snapshot decoded from SYSTEM.
/// Driver objects and UTF-16 names are retained in caller-owned arrays and must
/// outlive every loaded driver.
pub unsafe fn initialize_boot_drivers<F>(
    hive: &Hive<'_>,
    nodes: &[ServiceNode<'_>],
    group_order: &[&[u16]],
    tag_orders: &[GroupTagOrder<'_>],
    schedule: &mut [usize],
    scheduler_state: &mut [u8],
    driver_objects: &mut [DriverObject],
    service_names: &mut [[u16; 128]],
    records: &mut [DriverInitRecord],
    path_buffer: &mut [u16],
    image_buffer: &mut [u8],
    mut resolve_image: F,
) -> Result<BootInitReport, DriverInitError>
where F: FnMut(&[u16], &mut [u8]) -> Option<usize> {
    let count = nodes.len();
    if schedule.len() < count || scheduler_state.len() < count
        || driver_objects.len() < count || service_names.len() < count || records.len() < count {
        return Err(DriverInitError::BufferTooSmall);
    }
    for (index, node) in nodes.iter().enumerate() {
        if node.name.is_empty() || node.name.len() > service_names[index].len() {
            return Err(DriverInitError::InvalidServiceName);
        }
    }
    records[..count].fill(DriverInitRecord::EMPTY);
    let scheduled = driver_order::schedule_boot_drivers(
        nodes, group_order, tag_orders, schedule, scheduler_state,
    ).map_err(DriverInitError::Schedule)?;
    let mut report = BootInitReport { scheduled, ..BootInitReport::default() };

    for &service_index in &schedule[..scheduled] {
        let node = &nodes[service_index];
        // The order pass establishes a topological sequence, but initialization
        // can still fail. Never start a dependent after its service/group
        // prerequisite failed at runtime.
        if !dependencies_initialized(service_index, nodes, records) {
            let failure = RegistryDriverLoadError::DependencyFailed;
            records[service_index] = DriverInitRecord {
                service_index, error_control: 1, status: DriverInitStatus::Failed(failure),
            };
            report.failed += 1;
            continue;
        }
        let error_control = match crate::ntoskrnl::cm::service_config::query_service_details(
            hive, node.name, path_buffer,
        ) {
            Ok(details) => details.base.error_control,
            Err(error) => {
                let failure = RegistryDriverLoadError::ServiceMetadata(error);
                records[service_index] = DriverInitRecord {
                    service_index, error_control: 3, status: DriverInitStatus::Failed(failure),
                };
                report.failed += 1;
                report.attempted += 1;
                report.fatal_service = Some(service_index);
                break;
            }
        };

        let name = &mut service_names[service_index];
        name.fill(0);
        name[..node.name.len()].copy_from_slice(node.name);
        let name_length = (node.name.len() * 2) as u16;
        let driver_name = crate::nt_abi::object::UNICODE_STRING {
            length: name_length,
            maximum_length: (name.len() * 2) as u16,
            buffer: name.as_mut_ptr(),
        };
        driver_objects[service_index] = DriverObject {
            object_type: 4,
            size: core::mem::size_of::<DriverObject>() as i16,
            device_object: core::ptr::null_mut(),
            flags: 0,
            driver_start: core::ptr::null_mut(),
            driver_size: 0,
            driver_section: core::ptr::null_mut(),
            driver_extension: core::ptr::null_mut(),
            driver_name,
            hardware_database: core::ptr::null_mut(),
            fast_io_dispatch: core::ptr::null_mut(),
            driver_init: None,
            driver_start_io: None,
            driver_unload: None,
            major_function: [None; 28],
        };

        let driver_ptr = &mut driver_objects[service_index] as *mut DriverObject;
        let result = unsafe {
            driver_loader::load_registered_service_with_resolver(
                hive, node.name, path_buffer, image_buffer, driver_ptr,
                core::ptr::null_mut::<c_void>(), &mut resolve_image, crate::ntoskrnl::resolve,
            )
        };
        report.attempted += 1;
        match result {
            Ok(loaded) => {
                report.loaded += 1;
                records[service_index] = DriverInitRecord {
                    service_index, error_control, status: DriverInitStatus::Loaded(loaded),
                };
            }
            Err(error) => {
                report.failed += 1;
                records[service_index] = DriverInitRecord {
                    service_index, error_control, status: DriverInitStatus::Failed(error),
                };
                if abort_boot(error_control) {
                    report.fatal_service = Some(service_index);
                    break;
                }
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn severe_and_critical_failures_stop_boot_without_last_known_good() {
        assert!(!abort_boot(0));
        assert!(!abort_boot(1));
        assert!(abort_boot(2));
        assert!(abort_boot(3));
    }
    #[test]
    fn boot_report_starts_empty() {
        let report = BootInitReport::default();
        assert_eq!(report.scheduled, 0);
        assert_eq!(report.attempted, 0);
        assert_eq!(report.loaded, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(report.fatal_service, None);
    }
}

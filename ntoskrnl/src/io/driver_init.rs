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
    AlreadyInitialized,
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
pub enum DriverInitError { BufferTooSmall, InvalidServiceName, Schedule(ScheduleError), FatalService(usize) }
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
    preinitialized_services: &[&[u16]],
) -> bool {
    let node = &nodes[index];
    for dependency in node.service_dependencies {
        let Some(dep_index) = nodes.iter().position(|candidate| name_eq(candidate.name, dependency)) else {
            if preinitialized_services.iter().any(|name| name_eq(name, dependency)) { continue; }
            return false;
        };
        if !matches!(records[dep_index].status, DriverInitStatus::Loaded(_) | DriverInitStatus::AlreadyInitialized) {
            return false;
        }
    }
    for dependency in node.group_dependencies {
        if !nodes.iter().enumerate().any(|(candidate_index, candidate)| {
            name_eq(candidate.group, dependency)
                && matches!(records[candidate_index].status, DriverInitStatus::Loaded(_) | DriverInitStatus::AlreadyInitialized)
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
    preinitialized_services: &[&[u16]],
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
    for (index, node) in nodes.iter().enumerate() {
        if preinitialized_services.iter().any(|name| name_eq(node.name, name)) {
            records[index] = DriverInitRecord {
                service_index: index,
                error_control: 0,
                status: DriverInitStatus::AlreadyInitialized,
            };
        }
    }
    let scheduled = driver_order::schedule_boot_drivers_with_initialized(
        nodes, group_order, tag_orders, preinitialized_services, schedule, scheduler_state,
    ).map_err(DriverInitError::Schedule)?;
    let mut report = BootInitReport { scheduled, ..BootInitReport::default() };

    for &service_index in &schedule[..scheduled] {
        let node = &nodes[service_index];
        // The order pass establishes a topological sequence, but initialization
        // can still fail. Never start a dependent after its service/group
        // prerequisite failed at runtime.
        if !dependencies_initialized(service_index, nodes, records, preinitialized_services) {
            let failure = RegistryDriverLoadError::DependencyFailed;
            records[service_index] = DriverInitRecord {
                service_index, error_control: 1, status: DriverInitStatus::Failed(failure),
            };
            report.failed += 1;
            // A dependency that was scheduled but did not initialize is a
            // boot-phase failure. Continuing would allow later drivers to run
            // against an incomplete kernel/device prerequisite graph.
            report.fatal_service = Some(service_index);
            break;
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
    // WDM reinitialization callbacks are deferred until all eligible boot and
    // system-start drivers have completed their first DriverEntry pass.
    let _ = driver_loader::run_driver_reinitialization();
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

    #[test]
    fn failed_service_dependency_blocks_dependent_driver() {
        let dependency = [b'D' as u16, b'e' as u16, b'p' as u16];
        let dependent = [b'C' as u16, b'h' as u16, b'i' as u16, b'l' as u16, b'd' as u16];
        let dependencies: [&[u16]; 1] = [&dependency];
        let nodes = [
            ServiceNode { name: &dependency, group: &[], tag: None, start_type: 0,
                service_dependencies: &[], group_dependencies: &[] },
            ServiceNode { name: &dependent, group: &[], tag: None, start_type: 1,
                service_dependencies: &dependencies, group_dependencies: &[] },
        ];
        let records = [
            DriverInitRecord { service_index: 0, error_control: 1,
                status: DriverInitStatus::Failed(RegistryDriverLoadError::DependencyFailed) },
            DriverInitRecord::EMPTY,
        ];
        assert!(!dependencies_initialized(1, &nodes, &records, &[]));
    }

    #[test]
    fn initialized_service_dependency_is_case_insensitive() {
        let dependency = [b'D' as u16, b'E' as u16, b'P' as u16];
        let dependent = [b'C' as u16, b'H' as u16, b'I' as u16, b'L' as u16, b'D' as u16];
        let dependency_reference = [b'd' as u16, b'e' as u16, b'p' as u16];
        let dependencies: [&[u16]; 1] = [&dependency_reference];
        let nodes = [
            ServiceNode { name: &dependency, group: &[], tag: None, start_type: 0,
                service_dependencies: &[], group_dependencies: &[] },
            ServiceNode { name: &dependent, group: &[], tag: None, start_type: 1,
                service_dependencies: &dependencies, group_dependencies: &[] },
        ];
        let loaded = LoadedDriver { image_base: 0xffff_8000_0000_0000, image_size: 4096,
            entry_point: 0xffff_8000_0000_1000, status: 0 };
        let records = [
            DriverInitRecord { service_index: 0, error_control: 1,
                status: DriverInitStatus::Loaded(loaded) },
            DriverInitRecord::EMPTY,
        ];
        assert!(dependencies_initialized(1, &nodes, &records));
    }
}

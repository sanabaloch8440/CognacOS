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
pub(crate) struct BootServiceMetadata {
    pub tag: Option<u32>,
    pub group: [u16; 128],
    pub group_len: usize,
    pub service_dependencies: [u16; 512],
    pub service_dependencies_len: usize,
    pub group_dependencies: [u16; 512],
    pub group_dependencies_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemRegistryState {
    pub control_sets: ControlSetSelection,
    pub boot_driver_count: usize,
    pub boot_driver_services: &'static [[u16; 128]],
    pub boot_driver_metadata: &'static [BootServiceMetadata],
    pub service_group_count: usize,
    pub service_groups: &'static [[u16; 128]],
    pub group_tag_counts: &'static [usize],
    pub group_tag_values: &'static [[u32; 128]],
}

// Boot runs this before secondary processors and the general scheduler are
// online. The fixed storage therefore remains stable for the next boot phase.
const MAX_BOOT_DRIVER_SERVICES: usize = 128;
static mut BOOT_DRIVER_SERVICE_NAMES: [[u16; 128]; MAX_BOOT_DRIVER_SERVICES] =
    [[0; 128]; MAX_BOOT_DRIVER_SERVICES];
static mut BOOT_DRIVER_METADATA: [BootServiceMetadata; MAX_BOOT_DRIVER_SERVICES] =
    [BootServiceMetadata {
        tag: None, group: [0; 128], group_len: 0,
        service_dependencies: [0; 512], service_dependencies_len: 0,
        group_dependencies: [0; 512], group_dependencies_len: 0,
    }; MAX_BOOT_DRIVER_SERVICES];
const MAX_SERVICE_GROUPS: usize = 64;
static mut SERVICE_GROUP_NAMES: [[u16; 128]; MAX_SERVICE_GROUPS] =
    [[0; 128]; MAX_SERVICE_GROUPS];
static mut SERVICE_GROUP_ORDER_RAW: [u16; 1024] = [0; 1024];
static mut SERVICE_GROUP_TAG_COUNTS: [usize; MAX_SERVICE_GROUPS] = [0; MAX_SERVICE_GROUPS];
static mut SERVICE_GROUP_TAG_VALUES: [[u32; 128]; MAX_SERVICE_GROUPS] =
    [[0; 128]; MAX_SERVICE_GROUPS];


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
            MAX_BOOT_DRIVER_SERVICES,
        )
    };
    let boot_driver_count =
        crate::ntoskrnl::cm::service_config::enumerate_boot_driver_services(&hive, names)
            .map_err(|_| RegistryInitError::InvalidSystemHive)?;
    // Copy every dependency-bearing field out of the hive-backed parser result
    // into bounded kernel-owned storage before the boot loader can retain it.
    let metadata = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(BOOT_DRIVER_METADATA).cast::<BootServiceMetadata>(),
            MAX_BOOT_DRIVER_SERVICES,
        )
    };
    metadata.fill(BootServiceMetadata {
        tag: None, group: [0; 128], group_len: 0,
        service_dependencies: [0; 512], service_dependencies_len: 0,
        group_dependencies: [0; 512], group_dependencies_len: 0,
    });
    let mut image_path = [0u16; 1024];
    for index in 0..boot_driver_count {
        let name_len = names[index].iter().position(|unit| *unit == 0).unwrap_or(128);
        let details = crate::ntoskrnl::cm::service_config::query_service_details(
            &hive, &names[index][..name_len], &mut image_path,
        ).map_err(|_| RegistryInitError::InvalidSystemHive)?;
        let target = &mut metadata[index];
        target.tag = details.tag;
        target.group_len = details.group_len;
        target.group[..details.group_len].copy_from_slice(&details.group[..details.group_len]);
        target.service_dependencies_len = details.depend_on_service_len;
        target.service_dependencies[..details.depend_on_service_len]
            .copy_from_slice(&details.depend_on_service[..details.depend_on_service_len]);
        target.group_dependencies_len = details.depend_on_group_len;
        target.group_dependencies[..details.depend_on_group_len]
            .copy_from_slice(&details.depend_on_group[..details.depend_on_group_len]);
    }
    // Snapshot the registry's ServiceGroupOrder into kernel-owned fixed storage.
    // The raw MULTI_SZ is parsed before publishing so no hive-backed slices escape.
    let raw = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(SERVICE_GROUP_ORDER_RAW).cast::<u16>(), 1024,
        )
    };
    let raw_len = crate::ntoskrnl::cm::service_config::query_service_group_order(&hive, raw)
        .map_err(|_| RegistryInitError::InvalidSystemHive)?;
    let groups = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(SERVICE_GROUP_NAMES).cast::<[u16; 128]>(),
            MAX_SERVICE_GROUPS,
        )
    };
    groups.fill([0; 128]);
    let mut group_count = 0usize;
    let mut offset = 0usize;
    while offset < raw_len && raw[offset] != 0 {
        let Some(rel_end) = raw[offset..raw_len].iter().position(|unit| *unit == 0) else {
            return Err(RegistryInitError::InvalidSystemHive);
        };
        if rel_end == 0 || rel_end > 128 || group_count == MAX_SERVICE_GROUPS {
            return Err(RegistryInitError::InvalidSystemHive);
        }
        groups[group_count][..rel_end].copy_from_slice(&raw[offset..offset + rel_end]);
        group_count += 1;
        offset += rel_end + 1;
    }
    // Parse every applicable GroupOrderList entry into fixed storage. A missing
    // per-group value is valid and means no explicit tag preference.
    let tag_counts = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(SERVICE_GROUP_TAG_COUNTS).cast::<usize>(),
            MAX_SERVICE_GROUPS,
        )
    };
    let tag_values = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(SERVICE_GROUP_TAG_VALUES).cast::<[u32; 128]>(),
            MAX_SERVICE_GROUPS,
        )
    };
    tag_counts.fill(0);
    tag_values.fill([0; 128]);
    for index in 0..group_count {
        tag_counts[index] = crate::ntoskrnl::cm::service_config::query_group_tag_order(
            &hive, &groups[index][..groups[index].iter().position(|unit| *unit == 0).unwrap_or(128)],
            &mut tag_values[index],
        ).map_err(|_| RegistryInitError::InvalidSystemHive)?;
    }
    // Publish only the populated prefixes as immutable boot snapshots. These
    // buffers are kernel-owned and are never mutated after registry init.
    let boot_driver_services = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(BOOT_DRIVER_SERVICE_NAMES).cast::<[u16; 128]>(),
            boot_driver_count,
        )
    };

    let service_groups = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(SERVICE_GROUP_NAMES).cast::<[u16; 128]>(), group_count,
        )
    };
    let group_tag_counts = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(SERVICE_GROUP_TAG_COUNTS).cast::<usize>(), group_count,
        )
    };
    let group_tag_values = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(SERVICE_GROUP_TAG_VALUES).cast::<[u32; 128]>(), group_count,
        )
    };
    let boot_driver_metadata = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(BOOT_DRIVER_METADATA).cast::<BootServiceMetadata>(),
            boot_driver_count,
        )
    };
    Ok(SystemRegistryState {
        control_sets,
        boot_driver_count,
        boot_driver_services,
        boot_driver_metadata,
        service_group_count: group_count,
        service_groups,
        group_tag_counts,
        group_tag_values,
    })
}

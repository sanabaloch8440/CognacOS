//! Immutable kernel-provided callbacks used by the independently linked HAL.
//!
//! The HAL never imports NTOSKRNL. The executive installs this table once,
//! before publishing services that require memory-manager, DMA, or boot-policy
//! state. The table and its context must remain valid for the kernel lifetime.

use core::sync::atomic::{AtomicPtr, Ordering};
use crate::{HalKernelServices, HalStatus, IommuBackend};

static SERVICES: AtomicPtr<HalKernelServices> = AtomicPtr::new(core::ptr::null_mut());

/// Install the executive's permanent platform-service table.
///
/// # Safety
/// `services` and its context must remain valid and immutable for the
/// lifetime of the kernel. Installation is one-shot; replacement is rejected.
pub unsafe fn install(services: &'static HalKernelServices) -> Result<(), HalStatus> {
    if !services.compatible() {
        return Err(HalStatus::INVALID_PARAMETER);
    }
    let pointer = services as *const HalKernelServices as *mut HalKernelServices;
    match SERVICES.compare_exchange(
        core::ptr::null_mut(),
        pointer,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => Ok(()),
        Err(existing) if existing == pointer => Ok(()),
        Err(_) => Err(HalStatus::DEVICE_ERROR),
    }
}

/// Return the immutable table after successful one-time installation.
pub fn get() -> Option<&'static HalKernelServices> {
    let pointer = SERVICES.load(Ordering::Acquire);
    if pointer.is_null() {
        None
    } else {
        Some(unsafe { &*pointer })
    }
}

pub fn memory_manager_ready() -> bool {
    get().and_then(|s| s.memory_manager_ready)
        .is_some_and(|f| f(s.context))
}

pub fn map_io_space(physical: u64, length: u64) -> Result<u64, HalStatus> {
    let services = get().ok_or(HalStatus::NOT_SUPPORTED)?;
    let callback = services.map_io_space.ok_or(HalStatus::NOT_SUPPORTED)?;
    let mut virtual_address = 0;
    let status = unsafe {
        callback(services.context, physical, length, &mut virtual_address)
    };
    if status.code == HalStatus::SUCCESS.code && virtual_address != 0 {
        Ok(virtual_address)
    } else {
        Err(if status.code == HalStatus::SUCCESS.code {
            HalStatus::DEVICE_ERROR
        } else {
            status
        })
    }
}

pub fn unmap_io_space(virtual_address: u64, length: u64) -> Result<(), HalStatus> {
    let services = get().ok_or(HalStatus::NOT_SUPPORTED)?;
    let callback = services.unmap_io_space.ok_or(HalStatus::NOT_SUPPORTED)?;
    let status = unsafe { callback(services.context, virtual_address, length) };
    if status.code == HalStatus::SUCCESS.code { Ok(()) } else { Err(status) }
}

pub fn poll_tlb(logical_processor: usize) {
    if let Some(services) = get() {
        if let Some(callback) = services.poll_tlb {
            callback(services.context, logical_processor);
        }
    }
}

pub fn dma_available() -> bool {
    get().and_then(|s| s.dma_available)
        .is_some_and(|f| f(get().map_or(core::ptr::null_mut(), |s| s.context)))
}

pub fn create_dma_adapter(
    device_object: *mut core::ffi::c_void,
    flags: u32,
    maximum_address: u64,
) -> *mut core::ffi::c_void {
    let Some(services) = get() else { return core::ptr::null_mut(); };
    let Some(callback) = services.create_dma_adapter else { return core::ptr::null_mut(); };
    unsafe { callback(services.context, device_object, flags, maximum_address) }
}

pub fn allocate_common_buffer(
    adapter: *mut core::ffi::c_void,
    size: usize,
    maximum_address: u64,
) -> Result<(*mut u8, u64), i32> {
    let services = get().ok_or(HalStatus::NOT_SUPPORTED.code as i32)?;
    let callback = services.allocate_common_buffer
        .ok_or(HalStatus::NOT_SUPPORTED.code as i32)?;
    let (mut cpu, mut device) = (0u64, 0u64);
    let status = unsafe {
        callback(services.context, adapter, size, maximum_address, &mut cpu, &mut device)
    };
    if status == 0 && cpu != 0 && device != 0 {
        Ok((cpu as *mut u8, device))
    } else {
        Err(if status == 0 { HalStatus::DEVICE_ERROR.code as i32 } else { status })
    }
}

pub fn free_common_buffer(
    adapter: *mut core::ffi::c_void,
    cpu: *mut u8,
    device: u64,
    size: usize,
) -> Result<(), i32> {
    let services = get().ok_or(HalStatus::NOT_SUPPORTED.code as i32)?;
    let callback = services.free_common_buffer
        .ok_or(HalStatus::NOT_SUPPORTED.code as i32)?;
    let status = unsafe { callback(services.context, adapter, cpu as u64, device, size) };
    if status == 0 { Ok(()) } else { Err(status) }
}

pub fn validate_common_buffer(device_object: usize, cpu: u64, device: u64, size: u64) -> bool {
    let Some(services) = get() else { return false; };
    let Some(callback) = services.validate_common_buffer else { return false; };
    callback(services.context, device_object, cpu, device, size)
}

pub unsafe fn register_iommu_backend(backend: *mut IommuBackend) -> i32 {
    let Some(services) = get() else { return HalStatus::NOT_SUPPORTED.code as i32; };
    let Some(callback) = services.register_iommu_backend else {
        return HalStatus::NOT_SUPPORTED.code as i32;
    };
    unsafe { callback(services.context, backend) }
}

pub fn requester_identity(device_object: usize) -> Option<(u16, u8, u8)> {
    let services = get()?;
    let callback = services.requester_identity?;
    let (mut segment, mut bus, mut device_function) = (0u16, 0u8, 0u8);
    if callback(services.context, device_object, &mut segment, &mut bus, &mut device_function) {
        Some((segment, bus, device_function))
    } else {
        None
    }
}

pub unsafe fn get_platform_quirk_policy(
    policy: *mut *const core::ffi::c_void,
    size: *mut u32,
    version: *mut u32,
) -> u32 {
    let Some(services) = get() else { return HalStatus::NOT_SUPPORTED.code; };
    let Some(callback) = services.get_platform_quirk_policy else {
        return HalStatus::NOT_SUPPORTED.code;
    };
    unsafe { callback(policy, size, version) }
}

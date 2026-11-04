//! NT executive adapter for the independent HAL kernel-service contract.
//!
//! This is the only place where the kernel binds private MM, DMA, and boot
//! policy routines to HAL callbacks. The HAL crate itself has no dependency on
//! these executive implementation modules.

use cognacos_hal_contract::{HalKernelServices, HalStatus, IommuBackend};

unsafe extern "C" fn map_io_space(
    _context: *mut core::ffi::c_void,
    physical: u64,
    length: u64,
    output: *mut u64,
) -> HalStatus {
    if output.is_null() || length == 0 {
        return HalStatus::INVALID_PARAMETER;
    }
    match crate::kernel_vm::map_io_space(physical, length) {
        Ok(address) if address != 0 => {
            unsafe { output.write(address); }
            HalStatus::SUCCESS
        }
        _ => HalStatus::DEVICE_ERROR,
    }
}

unsafe extern "C" fn unmap_io_space(
    _context: *mut core::ffi::c_void,
    address: u64,
    length: u64,
) -> HalStatus {
    if address == 0 || length == 0 {
        return HalStatus::INVALID_PARAMETER;
    }
    match crate::kernel_vm::unmap_io_space(address, length) {
        Ok(()) => HalStatus::SUCCESS,
        Err(_) => HalStatus::DEVICE_ERROR,
    }
}

extern "C" fn memory_manager_ready(_context: *mut core::ffi::c_void) -> bool {
    crate::kernel_vm::is_initialized()
}

extern "C" fn poll_tlb(_context: *mut core::ffi::c_void, processor: usize) {
    crate::kernel_vm::smp_tlb_poll(processor);
}

extern "C" fn dma_available(_context: *mut core::ffi::c_void) -> bool {
    crate::dma::iommu::available()
}

unsafe extern "C" fn create_dma_adapter(
    _context: *mut core::ffi::c_void,
    device: *mut core::ffi::c_void,
    version: u32,
    maximum_address: u64,
) -> *mut core::ffi::c_void {
    crate::dma::create_device_adapter(device, version, maximum_address)
}

unsafe extern "C" fn allocate_common_buffer(
    _context: *mut core::ffi::c_void,
    adapter: *mut core::ffi::c_void,
    size: usize,
    maximum_address: u64,
    cpu_output: *mut u64,
    device_output: *mut u64,
) -> i32 {
    if cpu_output.is_null() || device_output.is_null() {
        return HalStatus::INVALID_PARAMETER.code as i32;
    }
    match unsafe { crate::dma::iommu_allocate_common(adapter, size, maximum_address) } {
        Ok((cpu, device)) if !cpu.is_null() && device != 0 => {
            unsafe { cpu_output.write(cpu as u64); device_output.write(device); }
            HalStatus::SUCCESS.code as i32
        }
        Ok(_) => HalStatus::DEVICE_ERROR.code as i32,
        Err(_) => HalStatus::NOT_SUPPORTED.code as i32,
    }
}

unsafe extern "C" fn free_common_buffer(
    _context: *mut core::ffi::c_void,
    adapter: *mut core::ffi::c_void,
    cpu: u64,
    device: u64,
    size: usize,
) -> i32 {
    if cpu == 0 || device == 0 || size == 0 {
        return HalStatus::INVALID_PARAMETER.code as i32;
    }
    match unsafe {
        crate::dma::free_common_buffer_for_adapter(adapter, cpu as *mut u8, device, size)
    } {
        Ok(()) => HalStatus::SUCCESS.code as i32,
        Err(_) => HalStatus::DEVICE_ERROR.code as i32,
    }
}

extern "C" fn validate_common_buffer(
    _context: *mut core::ffi::c_void,
    device: usize,
    cpu: u64,
    logical: u64,
    size: u64,
) -> bool {
    crate::dma::iommu::common_dma_mapping_valid(device, cpu, logical, size)
}

unsafe extern "C" fn register_iommu_backend(
    _context: *mut core::ffi::c_void,
    backend: *mut IommuBackend,
) -> i32 {
    unsafe { crate::dma::iommu::RdotDmaRegisterIommuBackend(backend) }
}

extern "C" fn requester_identity(
    _context: *mut core::ffi::c_void,
    device: usize,
    segment: *mut u16,
    bus: *mut u8,
    device_function: *mut u8,
) -> bool {
    if segment.is_null() || bus.is_null() || device_function.is_null() {
        return false;
    }
    let Some((s, b, df)) = crate::dma::iommu::requester_identity(device) else {
        return false;
    };
    unsafe {
        segment.write(s);
        bus.write(b);
        device_function.write(df);
    }
    true
}

unsafe extern "C" fn get_platform_quirk_policy(
    policy: *mut *const core::ffi::c_void,
    size: *mut u32,
    version: *mut u32,
) -> u32 {
    if policy.is_null() || size.is_null() || version.is_null() {
        return HalStatus::INVALID_PARAMETER.code;
    }
    let (pointer, bytes) = crate::boot_handoff::acpi_quirk_policy_ptr();
    unsafe {
        policy.write(pointer);
        size.write(bytes);
        version.write(cognacos_hal_contract::ACPI_PLATFORM_QUIRK_POLICY_VERSION);
    }
    HalStatus::SUCCESS.code
}

static KERNEL_SERVICES: HalKernelServices = HalKernelServices {
    size: core::mem::size_of::<HalKernelServices>() as u32,
    version: cognacos_hal_contract::HAL_KERNEL_SERVICES_VERSION,
    context: core::ptr::null_mut(),
    map_io_space: Some(map_io_space),
    unmap_io_space: Some(unmap_io_space),
    memory_manager_ready: Some(memory_manager_ready),
    poll_tlb: Some(poll_tlb),
    dma_available: Some(dma_available),
    create_dma_adapter: Some(create_dma_adapter),
    allocate_common_buffer: Some(allocate_common_buffer),
    free_common_buffer: Some(free_common_buffer),
    validate_common_buffer: Some(validate_common_buffer),
    register_iommu_backend: Some(register_iommu_backend),
    requester_identity: Some(requester_identity),
    get_platform_quirk_policy: Some(get_platform_quirk_policy),
};

/// Install the immutable kernel-owned callback table before any HAL service
/// can request MM, DMA, or boot-policy functionality.
pub(crate) unsafe fn initialize() -> bool {
    cognacos_hal_contract::kernel_services::install(&KERNEL_SERVICES).is_ok()
}

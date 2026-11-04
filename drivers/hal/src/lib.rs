#![no_std]

// The implementation crate depends on the shared contract; the kernel and
// drivers depend on that contract directly and never link this crate.
pub use cognacos_hal_contract::*;
pub use cognacos_acpi as acpi;

pub mod kernel_services;

/// Return the immutable, boot-selected ACPI quirk policy published by the
/// executive through the versioned HAL service contract. The HAL does not
/// reach into ntoskrnl globals or maintain a second policy singleton.
pub fn acpi_quirk_policy() -> Option<acpi::quirks::QuirkPolicy> {
    let mut policy = core::ptr::null();
    let mut size = 0u32;
    let mut version = 0u32;
    let status = unsafe {
        kernel_services::get_platform_quirk_policy(
            &mut policy,
            &mut size,
            &mut version,
        )
    };
    if status != HalStatus::SUCCESS.code
        || policy.is_null()
        || version != ACPI_PLATFORM_QUIRK_POLICY_VERSION
        || (size as usize) < core::mem::size_of::<acpi::quirks::QuirkPolicy>()
        || (policy as usize) % core::mem::align_of::<acpi::quirks::QuirkPolicy>() != 0
    {
        return None;
    }

    // The callback contract guarantees a kernel-lifetime immutable object
    // whenever it returns success with the current version and sufficient size.
    Some(unsafe { policy.cast::<acpi::quirks::QuirkPolicy>().read() })
}

/// Standalone x64 HAL implementation, built only into Hal.dll.
#[path = "../../../Hal/mod.rs"]
pub mod hal;

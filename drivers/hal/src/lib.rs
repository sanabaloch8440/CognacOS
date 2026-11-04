#![no_std]

// The implementation crate depends on the shared contract; the kernel and
// drivers depend on that contract directly and never link this crate.
pub use cognacos_hal_contract::*;

pub mod kernel_services;

/// Standalone x64 HAL implementation, built only into Hal.dll.
#[path = "../../../Hal/mod.rs"]
pub mod hal;

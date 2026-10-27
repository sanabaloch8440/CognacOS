//! Generic processor-class driver policy and HAL capability validation.
#![no_std]

/// Canonical Windows NT ABI declarations shared with ntoskrnl and NTDLL.
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)]
extern crate std;
use cognacos_hal_contract::{HalServiceTable, HAL_CONTRACT_VERSION};

pub const PROCESSOR_DRIVER_VERSION: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProcessorCapabilities {
    pub logical_processor_count: u32,
    pub invariant_tsc: bool,
    pub x2apic: bool,
    pub package_power_management: bool,
    pub performance_control: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessorStatus { Success, InvalidHalContract, InvalidProcessorCount, UnsupportedOperation }

/// Convert processor-driver results to the shared NTSTATUS ABI.
pub const fn status_to_ntstatus(status: ProcessorStatus) -> nt_abi::ntstatus::NTSTATUS {
    use nt_abi::ntstatus::*;
    match status {
        ProcessorStatus::Success => STATUS_SUCCESS,
        ProcessorStatus::InvalidProcessorCount => STATUS_INVALID_PARAMETER,
        ProcessorStatus::InvalidHalContract | ProcessorStatus::UnsupportedOperation => STATUS_INVALID_DEVICE_REQUEST,
    }
}

pub struct ProcessorDriver { initialized: bool, capabilities: ProcessorCapabilities }
impl ProcessorDriver {
    pub const fn new() -> Self {
        Self { initialized: false, capabilities: ProcessorCapabilities {
            logical_processor_count: 0, invariant_tsc: false, x2apic: false,
            package_power_management: false, performance_control: false,
        }}
    }
    pub fn initialize(&mut self, hal: &HalServiceTable, capabilities: ProcessorCapabilities) -> ProcessorStatus {
        if !hal.compatible() || hal.version != HAL_CONTRACT_VERSION { return ProcessorStatus::InvalidHalContract; }
        if capabilities.logical_processor_count == 0 { return ProcessorStatus::InvalidProcessorCount; }
        self.capabilities = capabilities;
        self.initialized = true;
        ProcessorStatus::Success
    }
    pub const fn is_initialized(&self) -> bool { self.initialized }
    pub const fn capabilities(&self) -> ProcessorCapabilities { self.capabilities }
    /// Vendor-specific MSR writes and power policy remain deliberately unsupported.
    pub const fn request_performance_state(&self, _state: u32) -> ProcessorStatus {
        ProcessorStatus::UnsupportedOperation
    }
}
impl Default for ProcessorDriver { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use cognacos_hal_contract::{HalServiceTable, HAL_CONTRACT_VERSION};
    fn hal() -> HalServiceTable {
        HalServiceTable { size: core::mem::size_of::<HalServiceTable>() as u32,
            version: HAL_CONTRACT_VERSION, read_port_u8: None, write_port_u8: None,
            read_mmio_u32: None, write_mmio_u32: None, query_counter: None,
            send_ipi: None, set_irql: None }
    }
    #[test]
    fn initialization_checks_hal_and_processor_count() {
        let mut d = ProcessorDriver::new();
        assert_eq!(d.initialize(&hal(), ProcessorCapabilities::default()), ProcessorStatus::InvalidProcessorCount);
        assert!(!d.is_initialized());
        let c = ProcessorCapabilities { logical_processor_count: 4, ..ProcessorCapabilities::default() };
        assert_eq!(d.initialize(&hal(), c), ProcessorStatus::Success);
        assert_eq!(d.capabilities().logical_processor_count, 4);
    }
}

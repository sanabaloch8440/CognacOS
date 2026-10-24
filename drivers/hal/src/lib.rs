//! Versioned platform-neutral service contract between the NT executive and HAL.
#![no_std]
#[cfg(test)]
extern crate std;

pub const HAL_CONTRACT_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HalStatus { pub code: u32 }
impl HalStatus {
    pub const SUCCESS: Self = Self { code: 0 };
    pub const NOT_SUPPORTED: Self = Self { code: 1 };
    pub const INVALID_PARAMETER: Self = Self { code: 2 };
    pub const DEVICE_ERROR: Self = Self { code: 3 };
}
pub type ReadPortU8 = unsafe extern "C" fn(port: u16, value: *mut u8) -> HalStatus;
pub type WritePortU8 = unsafe extern "C" fn(port: u16, value: u8) -> HalStatus;
pub type ReadMmioU32 = unsafe extern "C" fn(address: u64, value: *mut u32) -> HalStatus;
pub type WriteMmioU32 = unsafe extern "C" fn(address: u64, value: u32) -> HalStatus;
pub type QueryCounter = extern "C" fn(value: *mut u64, frequency_hz: *mut u64) -> HalStatus;
pub type SendIpi = unsafe extern "C" fn(apic_id: u32, vector: u8) -> HalStatus;
pub type SetIrql = unsafe extern "C" fn(new_irql: u8, old_irql: *mut u8) -> HalStatus;

/// Versioned function table passed to the kernel and drivers. Callers validate
/// size and version before accessing any entry.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HalServiceTable {
    pub size: u32,
    pub version: u32,
    pub read_port_u8: Option<ReadPortU8>,
    pub write_port_u8: Option<WritePortU8>,
    pub read_mmio_u32: Option<ReadMmioU32>,
    pub write_mmio_u32: Option<WriteMmioU32>,
    pub query_counter: Option<QueryCounter>,
    pub send_ipi: Option<SendIpi>,
    pub set_irql: Option<SetIrql>,
}
impl HalServiceTable {
    pub const fn compatible(&self) -> bool {
        self.size as usize >= core::mem::size_of::<Self>() && self.version == HAL_CONTRACT_VERSION
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_size_and_version() {
        let mut t = HalServiceTable { size: core::mem::size_of::<HalServiceTable>() as u32,
            version: HAL_CONTRACT_VERSION, read_port_u8: None, write_port_u8: None,
            read_mmio_u32: None, write_mmio_u32: None, query_counter: None,
            send_ipi: None, set_irql: None };
        assert!(t.compatible());
        t.version = 0;
        assert!(!t.compatible());
        t.version = HAL_CONTRACT_VERSION;
        t.size = 0;
        assert!(!t.compatible());
    }
}

//! Versioned platform-neutral service contract between the NT executive and HAL.
#![no_std]

/// Canonical Windows NT ABI declarations shared with ntoskrnl and NTDLL.
pub use cognacos_nt_abi as nt_abi;
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


/// Separate versioned device-service contract for DMA and interrupt-backed drivers.
/// These are declarations only: the kernel/HAL runtime must supply implementations
/// that honor IOMMU isolation, cache coherency, IRQL rules, and device teardown.
pub const DEVICE_SERVICES_VERSION:u32=1;
pub type AllocateDma=unsafe extern "C" fn(size:u64,alignment:u64,maximum_address:u64,flags:u32,cpu_address:*mut u64,device_address:*mut u64)->HalStatus;
pub type FreeDma=unsafe extern "C" fn(cpu_address:u64,device_address:u64,size:u64)->HalStatus;
pub type InterruptIsr=extern "C" fn(context:u64)->bool;
pub type InterruptDpc=extern "C" fn(context:u64);
pub type RegisterInterrupt=unsafe extern "C" fn(vector:u32,context:u64,isr:Option<InterruptIsr>,dpc:Option<InterruptDpc>)->HalStatus;
pub type UnregisterInterrupt=unsafe extern "C" fn(vector:u32,context:u64)->HalStatus;
pub type SetInterruptEnabled=unsafe extern "C" fn(vector:u32,enabled:bool)->HalStatus;
#[repr(C)]
#[derive(Clone,Copy)]
pub struct DeviceServiceTable{
 pub size:u32,pub version:u32,
 pub allocate_dma:Option<AllocateDma>,pub free_dma:Option<FreeDma>,
 pub register_interrupt:Option<RegisterInterrupt>,pub unregister_interrupt:Option<UnregisterInterrupt>,
 pub set_interrupt_enabled:Option<SetInterruptEnabled>,
}
impl DeviceServiceTable{
 pub const fn compatible(&self)->bool{self.size as usize>=core::mem::size_of::<Self>()&&self.version==DEVICE_SERVICES_VERSION}
 pub const fn has_dma(&self)->bool{self.allocate_dma.is_some()&&self.free_dma.is_some()}
 pub const fn has_interrupts(&self)->bool{self.register_interrupt.is_some()&&self.unregister_interrupt.is_some()&&self.set_interrupt_enabled.is_some()}
}
#[cfg(test)]
mod device_service_tests{
 use super::*;
 #[test]fn device_services_require_version_and_complete_function_pairs(){
  let mut t=DeviceServiceTable{size:core::mem::size_of::<DeviceServiceTable>() as u32,version:DEVICE_SERVICES_VERSION,allocate_dma:None,free_dma:None,register_interrupt:None,unregister_interrupt:None,set_interrupt_enabled:None};
  assert!(t.compatible());assert!(!t.has_dma());assert!(!t.has_interrupts());
  t.version=0;assert!(!t.compatible());
 }
}


/// Explicit memory attributes for device mappings. Exactly one cache policy is
/// required; executable mappings are intentionally not part of the device-MMIO API.
pub const HAL_MEMORY_DEVICE: u32 = 1 << 0;
pub const HAL_MEMORY_UNCACHED: u32 = 1 << 1;
pub const HAL_MEMORY_WRITE_COMBINED: u32 = 1 << 2;
pub const HAL_MEMORY_READ_ONLY: u32 = 1 << 3;
pub const HAL_MEMORY_READ_WRITE: u32 = 1 << 4;
pub const HAL_DMA_CONTIGUOUS: u32 = 1 << 0;
pub const HAL_DMA_COHERENT: u32 = 1 << 1;
pub const HAL_DMA_ZEROED: u32 = 1 << 2;

/// Validates the requested x64 device mapping policy before invoking a mapper.
/// MMIO defaults are never inferred: the caller must explicitly select uncached
/// or write-combined attributes and read-only or read-write access.
pub const fn validate_device_mapping_flags(flags: u32) -> bool {
    let cache = flags & (HAL_MEMORY_UNCACHED | HAL_MEMORY_WRITE_COMBINED);
    let access = flags & (HAL_MEMORY_READ_ONLY | HAL_MEMORY_READ_WRITE);
    flags & HAL_MEMORY_DEVICE != 0
        && (cache == HAL_MEMORY_UNCACHED || cache == HAL_MEMORY_WRITE_COMBINED)
        && (access == HAL_MEMORY_READ_ONLY || access == HAL_MEMORY_READ_WRITE)
        && flags & !(HAL_MEMORY_DEVICE | HAL_MEMORY_UNCACHED | HAL_MEMORY_WRITE_COMBINED
            | HAL_MEMORY_READ_ONLY | HAL_MEMORY_READ_WRITE) == 0
}
pub const fn validate_dma_flags(flags: u32) -> bool {
    flags & (HAL_DMA_CONTIGUOUS | HAL_DMA_COHERENT) == (HAL_DMA_CONTIGUOUS | HAL_DMA_COHERENT)
        && flags & !(HAL_DMA_CONTIGUOUS | HAL_DMA_COHERENT | HAL_DMA_ZEROED) == 0
}

/// Extended service ABI: separate from version 1 so existing consumers do not
/// misinterpret a larger function table. Implementations must honor mapping
/// attributes, device address limits, DMA lifetime, and interrupt synchronization.
pub const HAL_EXTENDED_SERVICES_VERSION: u32 = 2;
pub type MapDeviceIoSpace = unsafe extern "C" fn(physical_address: u64, length: u64, flags: u32, virtual_address: *mut u64) -> HalStatus;
pub type UnmapDeviceIoSpace = unsafe extern "C" fn(virtual_address: u64, length: u64) -> HalStatus;
pub type SynchronizeInterrupt = unsafe extern "C" fn(vector: u32, context: u64, routine: Option<extern "C" fn(u64) -> bool>) -> HalStatus;
pub type MemoryBarrier = extern "C" fn();
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HalExtendedServiceTable {
    pub size: u32,
    pub version: u32,
    pub map_device_io_space: Option<MapDeviceIoSpace>,
    pub unmap_device_io_space: Option<UnmapDeviceIoSpace>,
    pub synchronize_interrupt: Option<SynchronizeInterrupt>,
    pub memory_barrier: Option<MemoryBarrier>,
}
impl HalExtendedServiceTable {
    pub const fn compatible(&self) -> bool {
        self.size as usize >= core::mem::size_of::<Self>()
            && self.version == HAL_EXTENDED_SERVICES_VERSION
    }
    pub const fn has_mmio_mapping(&self) -> bool {
        self.map_device_io_space.is_some() && self.unmap_device_io_space.is_some()
    }
}
#[cfg(test)]
mod hardware_contract_tests {
    use super::*;
    #[test]
    fn mapping_policy_requires_explicit_nonconflicting_attributes() {
        assert!(validate_device_mapping_flags(HAL_MEMORY_DEVICE | HAL_MEMORY_UNCACHED | HAL_MEMORY_READ_WRITE));
        assert!(validate_device_mapping_flags(HAL_MEMORY_DEVICE | HAL_MEMORY_WRITE_COMBINED | HAL_MEMORY_READ_ONLY));
        assert!(!validate_device_mapping_flags(HAL_MEMORY_DEVICE | HAL_MEMORY_READ_WRITE));
        assert!(!validate_device_mapping_flags(HAL_MEMORY_DEVICE | HAL_MEMORY_UNCACHED | HAL_MEMORY_WRITE_COMBINED | HAL_MEMORY_READ_WRITE));
        assert!(!validate_device_mapping_flags(HAL_MEMORY_UNCACHED | HAL_MEMORY_READ_WRITE));
    }
    #[test]
    fn dma_contract_requires_contiguous_coherent_mapping() {
        assert!(validate_dma_flags(HAL_DMA_CONTIGUOUS | HAL_DMA_COHERENT | HAL_DMA_ZEROED));
        assert!(!validate_dma_flags(HAL_DMA_CONTIGUOUS));
        assert!(!validate_dma_flags(HAL_DMA_COHERENT | HAL_DMA_ZEROED));
    }
    #[test]
    fn extended_table_is_versioned_independently() {
        let t = HalExtendedServiceTable { size: core::mem::size_of::<HalExtendedServiceTable>() as u32,
            version: HAL_EXTENDED_SERVICES_VERSION, map_device_io_space: None,
            unmap_device_io_space: None, synchronize_interrupt: None, memory_barrier: None };
        assert!(t.compatible());
        assert!(!t.has_mmio_mapping());
    }
}

//! Versioned platform-neutral service contract between the NT executive and HAL.
#![no_std]

/// HAL implementation consumes executive callbacks through this module.
pub mod kernel_services;

/// Canonical Windows NT ABI declarations shared with ntoskrnl and NTDLL.
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)]
extern crate std;

pub const HAL_CONTRACT_VERSION: u32 = 1;

/// ACPI-to-PCI host-bridge resource handoff. Versioned independently from
/// the HAL service table so ACPI and PCI can evolve this payload explicitly.
pub const PCI_HOST_BRIDGE_RESOURCES_VERSION: u32 = 1;
pub const PCI_HOST_BRIDGE_MAX_WINDOWS: usize = 32;
pub const PCI_HOST_BRIDGE_WINDOW_IO: u8 = 1;
pub const PCI_HOST_BRIDGE_WINDOW_MEMORY: u8 = 2;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PciHostBridgeResourceWindow {
    pub space: u8,
    /// Bit 0: writable; bit 1: cacheable. Reserved bits must be zero.
    pub attributes: u8,
    /// ACPI address descriptor width: 0=word, 1=dword, 2=qword.
    pub address_width: u8,
    pub reserved: u8,
    /// Lowest/highest permitted base address, inclusive.
    pub minimum: u64,
    pub maximum: u64,
    pub alignment: u64,
    /// ACPI bus-to-primary address translation offset; nonzero offsets must be handled explicitly.
    pub translation_offset: u64,
    /// Size of the resource window at each permitted base.
    pub length: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PciHostBridgeResourceSet {
    pub size: u32,
    pub version: u32,
    pub segment: u16,
    pub bus_start: u8,
    pub bus_end: u8,
    pub count: u32,
    pub windows: [PciHostBridgeResourceWindow; PCI_HOST_BRIDGE_MAX_WINDOWS],
}
impl PciHostBridgeResourceSet {
    pub const fn empty(segment: u16, bus_start: u8, bus_end: u8) -> Self {
        Self {
            size: core::mem::size_of::<Self>() as u32,
            version: PCI_HOST_BRIDGE_RESOURCES_VERSION,
            segment, bus_start, bus_end, count: 0,
            windows: [PciHostBridgeResourceWindow {
                space: 0, attributes: 0, address_width: 0, reserved: 0,
                minimum: 0, maximum: 0, alignment: 0, translation_offset: 0, length: 0,
            }; PCI_HOST_BRIDGE_MAX_WINDOWS],
        }
    }
    pub const fn compatible(&self) -> bool {
        self.size as usize >= core::mem::size_of::<Self>()
            && self.version == PCI_HOST_BRIDGE_RESOURCES_VERSION
            && self.count as usize <= PCI_HOST_BRIDGE_MAX_WINDOWS
            && self.bus_start <= self.bus_end
    }
}

/// Small NT I/O-manager service surface passed to native WDM drivers. Keeping
/// completion in a versioned kernel table avoids static imports from a driver
/// image while preserving the IoCompleteRequest calling convention.
pub const KERNEL_IO_SERVICES_VERSION: u32 = 8;
pub type CompleteIrp = unsafe extern "win64" fn(
    irp: *mut cognacos_nt_abi::io::IRP,
    priority_boost: i8,
);
pub type CreateDevice = unsafe extern "win64" fn(
    driver: *mut core::ffi::c_void, extension_size: u32,
    name: *mut cognacos_nt_abi::object::UNICODE_STRING,
    device_type: u32, characteristics: u32, exclusive: u8,
    output: *mut *mut core::ffi::c_void,
) -> i32;
pub type DeleteDevice = unsafe extern "win64" fn(device: *mut core::ffi::c_void);
pub type CallDriver = unsafe extern "win64" fn(device: *mut core::ffi::c_void, irp: *mut core::ffi::c_void) -> i32;
pub type AllocateIrp = unsafe extern "win64" fn(stack_size: i8, charge_quota: u8) -> *mut core::ffi::c_void;
pub type GetIrpStackLocation = unsafe extern "win64" fn(irp: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
pub type CompleteIrpStatus = unsafe extern "win64" fn(irp: *mut core::ffi::c_void, status: i32, information: usize, priority_boost: i8);
pub type PinBuffer = unsafe extern "win64" fn(buffer:*mut core::ffi::c_void,length:u32,write_access:u8,mdl_out:*mut *mut core::ffi::c_void,system_address_out:*mut *mut core::ffi::c_void)->i32;
pub type UnpinBuffer = unsafe extern "win64" fn(mdl:*mut core::ffi::c_void,system_address:*mut core::ffi::c_void);
pub type AttachDevice = unsafe extern "win64" fn(source:*mut core::ffi::c_void,target:*mut core::ffi::c_void)->*mut core::ffi::c_void;
pub type DetachDevice = unsafe extern "win64" fn(target:*mut core::ffi::c_void);
pub type PublishBusRelations = unsafe extern "win64" fn(bus:*mut core::ffi::c_void,children:*const *mut core::ffi::c_void,count:u32)->i32;
pub type IrpOperation = unsafe extern "win64" fn(irp:*mut core::ffi::c_void);
pub type MarkBusPdo = unsafe extern "win64" fn(device:*mut core::ffi::c_void)->i32;
pub type IoCompletionRoutine = unsafe extern "win64" fn(device:*mut core::ffi::c_void,irp:*mut core::ffi::c_void,context:*mut core::ffi::c_void)->i32;
pub type SetCompletionRoutine = unsafe extern "win64" fn(irp:*mut core::ffi::c_void,routine:Option<IoCompletionRoutine>,context:*mut core::ffi::c_void,success:u8,error:u8,cancel:u8);
pub type IoCancelRoutine = unsafe extern "win64" fn(device:*mut core::ffi::c_void,irp:*mut core::ffi::c_void);
pub type SetCancelRoutine = unsafe extern "win64" fn(irp:*mut core::ffi::c_void,routine:Option<IoCancelRoutine>)->Option<IoCancelRoutine>;
pub type CancelIrp = unsafe extern "win64" fn(irp:*mut core::ffi::c_void)->u8;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelIoServiceTable {
    pub size: u32,
    pub version: u32,
    pub complete_irp: Option<CompleteIrp>,
    pub create_device: Option<CreateDevice>,
    pub delete_device: Option<DeleteDevice>,
    pub call_driver: Option<CallDriver>,
    pub allocate_irp: Option<AllocateIrp>,
    pub current_stack_location: Option<GetIrpStackLocation>,
    pub next_stack_location: Option<GetIrpStackLocation>,
    pub complete_irp_status: Option<CompleteIrpStatus>,
    pub pin_buffer:Option<PinBuffer>,
    pub unpin_buffer:Option<UnpinBuffer>,
    pub attach_device:Option<AttachDevice>,
    pub detach_device:Option<DetachDevice>,
    pub publish_bus_relations:Option<PublishBusRelations>,
    pub skip_current_stack:Option<IrpOperation>,
    pub free_irp:Option<IrpOperation>,
    pub mark_bus_enumerated_pdo:Option<MarkBusPdo>,
    pub set_completion_routine:Option<SetCompletionRoutine>,
    pub set_cancel_routine:Option<SetCancelRoutine>,
    pub cancel_irp:Option<CancelIrp>,
}
impl KernelIoServiceTable {
    pub const fn compatible(&self) -> bool {
        self.size as usize >= core::mem::size_of::<Self>()
            && self.version == KERNEL_IO_SERVICES_VERSION
            && self.complete_irp.is_some()
            && self.create_device.is_some()
            && self.delete_device.is_some()
            && self.call_driver.is_some()
            && self.allocate_irp.is_some()
            && self.current_stack_location.is_some()
            && self.next_stack_location.is_some()
            && self.complete_irp_status.is_some()
            && self.pin_buffer.is_some()
            && self.unpin_buffer.is_some()
            && self.attach_device.is_some()
            && self.detach_device.is_some()
            && self.publish_bus_relations.is_some()
            && self.skip_current_stack.is_some()
            && self.free_irp.is_some()
            && self.mark_bus_enumerated_pdo.is_some()
            && self.set_completion_routine.is_some()
            && self.set_cancel_routine.is_some()
            && self.cancel_irp.is_some()
    }
}


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
/// Version 4 adds an exact live-mapping validation callback. DMA callbacks carry
/// the owning device object so the kernel selects its requester-specific IOMMU domain.
pub const DEVICE_SERVICES_VERSION:u32=4;
pub type AllocateDma=unsafe extern "C" fn(device_object:u64,size:u64,alignment:u64,maximum_address:u64,flags:u32,cpu_address:*mut u64,device_address:*mut u64)->HalStatus;
pub type FreeDma=unsafe extern "C" fn(device_object:u64,cpu_address:u64,device_address:u64,size:u64)->HalStatus;
pub type ValidateDma=unsafe extern "C" fn(device_object:u64,cpu_address:u64,device_address:u64,size:u64)->HalStatus;
pub type InterruptIsr=extern "C" fn(context:u64)->bool;
pub type InterruptDpc=extern "C" fn(context:u64);
pub type RegisterInterrupt=unsafe extern "C" fn(vector:u32,context:u64,isr:Option<InterruptIsr>,dpc:Option<InterruptDpc>)->HalStatus;
pub type UnregisterInterrupt=unsafe extern "C" fn(vector:u32,context:u64)->HalStatus;
pub type SetInterruptEnabled=unsafe extern "C" fn(vector:u32,enabled:bool)->HalStatus;
pub type SetBusMaster=unsafe extern "C" fn(device_object:u64,enabled:bool)->HalStatus;

/// Dedicated, versioned PCI interrupt lifecycle. This is a private HAL service
/// ABI, not a Windows HAL export. The ISR/DPC callback ABI intentionally
/// matches the existing device-interrupt dispatcher callback convention.
pub const PCI_INTERRUPT_SERVICES_VERSION: u32 = 1;
pub type PciInterruptIsr = extern "C" fn(context: u64) -> bool;
pub type PciInterruptDpc = extern "C" fn(context: u64);
pub type PciInterruptAllocateVector =
    unsafe extern "C" fn(vector_out: *mut u8) -> u32;
pub type PciInterruptConfigureMasked = unsafe extern "C" fn(
    vector: u8,
    io_apic_base: u64,
    gsi_base: u32,
    gsi: u32,
    active_low: bool,
    level_triggered: bool,
) -> u32;
pub type PciInterruptRegisterIsr = unsafe extern "C" fn(
    vector: u8,
    context: u64,
    isr: Option<PciInterruptIsr>,
    dpc: Option<PciInterruptDpc>,
) -> u32;
pub type PciInterruptSetEnabled = unsafe extern "C" fn(vector: u8) -> u32;
pub type PciInterruptUnregister = unsafe extern "C" fn(vector: u8, context: u64) -> u32;
pub type PciInterruptReleaseVector = unsafe extern "C" fn(vector: u8) -> u32;

/// PCI INTx/MSI routing lifecycle. A route is allocated and configured masked;
/// the ISR/DPC pair is then registered; only a subsequent enable call may
/// unmask delivery. Teardown disables, unregisters/drains, then releases the
/// vector. Each operation is separate so callers can unwind partial startup.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PciInterruptHalServices {
    pub size: u32,
    pub version: u32,
    pub allocate_vector: Option<PciInterruptAllocateVector>,
    pub configure_masked: Option<PciInterruptConfigureMasked>,
    pub register_isr: Option<PciInterruptRegisterIsr>,
    pub enable: Option<PciInterruptSetEnabled>,
    pub disable: Option<PciInterruptSetEnabled>,
    pub unregister: Option<PciInterruptUnregister>,
    pub release_vector: Option<PciInterruptReleaseVector>,
}
impl PciInterruptHalServices {
    pub const fn compatible(&self) -> bool {
        self.size as usize >= core::mem::size_of::<Self>()
            && self.version == PCI_INTERRUPT_SERVICES_VERSION
            && self.allocate_vector.is_some()
            && self.configure_masked.is_some()
            && self.register_isr.is_some()
            && self.enable.is_some()
            && self.disable.is_some()
            && self.unregister.is_some()
            && self.release_vector.is_some()
    }
}

#[cfg(test)]
mod pci_interrupt_contract_tests {
    use super::*;

    fn empty_table() -> PciInterruptHalServices {
        PciInterruptHalServices {
            size: core::mem::size_of::<PciInterruptHalServices>() as u32,
            version: PCI_INTERRUPT_SERVICES_VERSION,
            allocate_vector: None,
            configure_masked: None,
            register_isr: None,
            enable: None,
            disable: None,
            unregister: None,
            release_vector: None,
        }
    }

    #[test]
    fn pci_interrupt_contract_rejects_missing_callbacks() {
        let table = empty_table();
        assert!(!table.compatible());
    }

    #[test]
    fn pci_interrupt_contract_rejects_wrong_version_and_truncated_table() {
        let mut table = empty_table();
        table.version = 0;
        assert!(!table.compatible());
        table.version = PCI_INTERRUPT_SERVICES_VERSION;
        table.size = 0;
        assert!(!table.compatible());
    }
}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct DeviceServiceTable{
 pub size:u32,pub version:u32,
 pub allocate_dma:Option<AllocateDma>,pub free_dma:Option<FreeDma>,pub validate_dma:Option<ValidateDma>,
 pub register_interrupt:Option<RegisterInterrupt>,pub unregister_interrupt:Option<UnregisterInterrupt>,
 pub set_interrupt_enabled:Option<SetInterruptEnabled>,pub set_bus_master:Option<SetBusMaster>,
}
impl DeviceServiceTable{
 pub const fn compatible(&self)->bool{self.size as usize>=core::mem::size_of::<Self>()&&self.version==DEVICE_SERVICES_VERSION}
 pub const fn has_dma(&self)->bool{self.allocate_dma.is_some()&&self.free_dma.is_some()&&self.validate_dma.is_some()}
 pub const fn has_dma_validation(&self)->bool{self.validate_dma.is_some()}
 pub const fn has_interrupts(&self)->bool{self.register_interrupt.is_some()&&self.unregister_interrupt.is_some()&&self.set_interrupt_enabled.is_some()}
 pub const fn has_bus_master(&self)->bool{self.set_bus_master.is_some()}
}
#[cfg(test)]
mod device_service_tests{
 use super::*;
 #[test]fn device_services_require_version_and_complete_function_pairs(){
  let mut t=DeviceServiceTable{size:core::mem::size_of::<DeviceServiceTable>() as u32,version:DEVICE_SERVICES_VERSION,allocate_dma:None,free_dma:None,validate_dma:None,register_interrupt:None,unregister_interrupt:None,set_interrupt_enabled:None,set_bus_master:None};
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


/// Private, size/version-tagged ACPI SCI backend contract shared by HAL and ACPI.sys.
/// This is an internal CognacOS interface, not an additional Windows HAL export.
pub const ACPI_SCI_SERVICES_VERSION: u32 = 2;
pub const ACPI_PLATFORM_QUIRK_POLICY_VERSION: u32 = 1;
pub type AcpiGetPlatformQuirkPolicy = unsafe extern "C" fn(
    policy: *mut *const core::ffi::c_void, size: *mut u32, version: *mut u32,
) -> u32;
pub type AcpiSciIsr = extern "C" fn(context: u64) -> bool;
pub type AcpiSciDpc = extern "C" fn(context: u64);
pub type AcpiSciAllocateVector = unsafe extern "C" fn(vector: *mut u8) -> u32;
pub type AcpiSciConfigureMasked = unsafe extern "C" fn(
    vector: u8, io_apic: u64, gsi_base: u32, gsi: u32,
    active_low: bool, level_triggered: bool,
) -> u32;
pub type AcpiSciRegister = unsafe extern "C" fn(
    vector: u8, context: u64, isr: Option<AcpiSciIsr>, dpc: Option<AcpiSciDpc>,
) -> u32;
pub type AcpiSciSetEnabled = unsafe extern "C" fn(vector: u8, enabled: bool) -> u32;
pub type AcpiSciUnregister = unsafe extern "C" fn(vector: u8, context: u64) -> u32;
pub type AcpiSciReleaseVector = unsafe extern "C" fn(vector: u8) -> u32;
pub type AcpiQueuePassiveWorker = extern "C" fn(context: u64, routine: AcpiSciDpc) -> bool;
pub type AcpiRundownPassiveWorker = extern "C" fn(context: u64) -> bool;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AcpiSciHalServices {
    pub size: u32,
    pub version: u32,
    pub allocate_vector: Option<AcpiSciAllocateVector>,
    pub configure_masked: Option<AcpiSciConfigureMasked>,
    pub register: Option<AcpiSciRegister>,
    pub set_enabled: Option<AcpiSciSetEnabled>,
    pub unregister: Option<AcpiSciUnregister>,
    pub release_vector: Option<AcpiSciReleaseVector>,
    pub queue_passive_worker: Option<AcpiQueuePassiveWorker>,
    pub rundown_passive_worker: Option<AcpiRundownPassiveWorker>,
    /// Returns a read-only, immutable boot-selected policy owned by the kernel.
    pub get_platform_quirk_policy: Option<AcpiGetPlatformQuirkPolicy>,
}
impl AcpiSciHalServices {
    pub const fn compatible(&self) -> bool {
        self.size as usize >= core::mem::size_of::<Self>()
            && self.version == ACPI_SCI_SERVICES_VERSION
            && self.allocate_vector.is_some()
            && self.configure_masked.is_some()
            && self.register.is_some()
            && self.set_enabled.is_some()
            && self.unregister.is_some()
            && self.release_vector.is_some()
            && self.queue_passive_worker.is_some()
            && self.rundown_passive_worker.is_some()
            && self.get_platform_quirk_policy.is_some()
    }
}


// ---------------------------------------------------------------------------
// Kernel-owned platform services consumed by the independently linked HAL.
//
// This is a private CognacOS boot/runtime ABI, not a Windows HAL export. The
// table is installed by the kernel before HAL services that require MM, DMA,
// or boot-policy state are enabled. HAL must not link against NTOSKRNL to
// obtain these services.
pub const HAL_KERNEL_SERVICES_VERSION: u32 = 1;

pub type HalKernelMapIoSpace = unsafe extern "C" fn(
    context: *mut core::ffi::c_void,
    physical_address: u64,
    length: u64,
    virtual_address: *mut u64,
) -> HalStatus;
pub type HalKernelUnmapIoSpace = unsafe extern "C" fn(
    context: *mut core::ffi::c_void,
    virtual_address: u64,
    length: u64,
) -> HalStatus;
pub type HalKernelMemoryManagerReady =
    extern "C" fn(context: *mut core::ffi::c_void) -> bool;
pub type HalKernelPollTlb =
    extern "C" fn(context: *mut core::ffi::c_void, logical_processor: usize);
pub type HalKernelDmaAvailable = extern "C" fn(context: *mut core::ffi::c_void) -> bool;
pub type HalKernelCreateDmaAdapter = unsafe extern "C" fn(
    context: *mut core::ffi::c_void,
    device_object: *mut core::ffi::c_void,
    flags: u32,
    maximum_address: u64,
) -> *mut core::ffi::c_void;
pub type HalKernelAllocateCommonBuffer = unsafe extern "C" fn(
    context: *mut core::ffi::c_void,
    adapter: *mut core::ffi::c_void,
    size: usize,
    maximum_address: u64,
    cpu_address: *mut u64,
    device_address: *mut u64,
) -> i32;
pub type HalKernelFreeCommonBuffer = unsafe extern "C" fn(
    context: *mut core::ffi::c_void,
    adapter: *mut core::ffi::c_void,
    cpu_address: u64,
    device_address: u64,
    size: usize,
) -> i32;
pub type HalKernelValidateCommonBuffer = extern "C" fn(
    context: *mut core::ffi::c_void,
    device_object: usize,
    cpu_address: u64,
    device_address: u64,
    size: u64,
) -> bool;
pub type HalKernelRegisterIommuBackend = unsafe extern "C" fn(
    context: *mut core::ffi::c_void,
    backend: *mut IommuBackend,
) -> i32;
pub type HalKernelRequesterIdentity = extern "C" fn(
    context: *mut core::ffi::c_void,
    device_object: usize,
    segment: *mut u16,
    bus: *mut u8,
    device_function: *mut u8,
) -> bool;

/// Canonical kernel/HAL DMA mapper registration ABI. The kernel owns backend
/// registration and DMA mapping lifetime; the HAL supplies hardware mapping
/// callbacks without importing the kernel's I/O-manager implementation.
pub type IommuMap = unsafe extern "system" fn(
    context: *mut core::ffi::c_void,
    device: usize,
    physical: u64,
    length: u32,
    direction: u8,
    logical_out: *mut u64,
    cookie_out: *mut u64,
) -> i32;
pub type IommuUnmap = unsafe extern "system" fn(
    context: *mut core::ffi::c_void,
    device: usize,
    logical: u64,
    cookie: u64,
) -> i32;
pub type IommuSync = unsafe extern "system" fn(
    context: *mut core::ffi::c_void,
    device: usize,
    logical: u64,
    length: u32,
    direction: u8,
) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IommuBackend {
    pub size: u32,
    pub context: *mut core::ffi::c_void,
    pub map: Option<IommuMap>,
    pub unmap: Option<IommuUnmap>,
    pub sync: Option<IommuSync>,
}
unsafe impl Send for IommuBackend {}
unsafe impl Sync for IommuBackend {}

#[repr(C)]
#[derive(Clone, Copy)]
// The raw context is an opaque, kernel-lifetime callback context. Its
// synchronization and immutability are guaranteed by the one-shot installer.
unsafe impl Send for HalKernelServices {}
unsafe impl Sync for HalKernelServices {}

pub struct HalKernelServices {
    pub size: u32,
    pub version: u32,
    pub context: *mut core::ffi::c_void,
    pub map_io_space: Option<HalKernelMapIoSpace>,
    pub unmap_io_space: Option<HalKernelUnmapIoSpace>,
    pub memory_manager_ready: Option<HalKernelMemoryManagerReady>,
    pub poll_tlb: Option<HalKernelPollTlb>,
    pub dma_available: Option<HalKernelDmaAvailable>,
    pub create_dma_adapter: Option<HalKernelCreateDmaAdapter>,
    pub allocate_common_buffer: Option<HalKernelAllocateCommonBuffer>,
    pub free_common_buffer: Option<HalKernelFreeCommonBuffer>,
    pub validate_common_buffer: Option<HalKernelValidateCommonBuffer>,
    pub register_iommu_backend: Option<HalKernelRegisterIommuBackend>,
    pub requester_identity: Option<HalKernelRequesterIdentity>,
    pub get_platform_quirk_policy: Option<AcpiGetPlatformQuirkPolicy>,
}
impl HalKernelServices {
    pub const fn compatible(&self) -> bool {
        self.size as usize >= core::mem::size_of::<Self>()
            && self.version == HAL_KERNEL_SERVICES_VERSION
            && self.map_io_space.is_some()
            && self.unmap_io_space.is_some()
            && self.memory_manager_ready.is_some()
            && self.poll_tlb.is_some()
    }
    pub const fn has_common_dma_services(&self) -> bool {
        self.dma_available.is_some()
            && self.create_dma_adapter.is_some()
            && self.allocate_common_buffer.is_some()
            && self.free_common_buffer.is_some()
            && self.validate_common_buffer.is_some()
    }
    pub const fn has_iommu_registration_services(&self) -> bool {
        self.register_iommu_backend.is_some() && self.requester_identity.is_some()
    }
}

#[cfg(test)]
mod kernel_hal_boundary_tests {
    use super::*;

    unsafe extern "C" fn map(_: *mut core::ffi::c_void, _: u64, _: u64, _: *mut u64) -> HalStatus {
        HalStatus::SUCCESS
    }
    unsafe extern "C" fn unmap(_: *mut core::ffi::c_void, _: u64, _: u64) -> HalStatus {
        HalStatus::SUCCESS
    }
    extern "C" fn ready(_: *mut core::ffi::c_void) -> bool { true }
    extern "C" fn poll(_: *mut core::ffi::c_void, _: usize) {}

    fn base_services() -> HalKernelServices {
        HalKernelServices {
            size: core::mem::size_of::<HalKernelServices>() as u32,
            version: HAL_KERNEL_SERVICES_VERSION,
            context: core::ptr::null_mut(),
            map_io_space: Some(map),
            unmap_io_space: Some(unmap),
            memory_manager_ready: Some(ready),
            poll_tlb: Some(poll),
            dma_available: None,
            create_dma_adapter: None,
            allocate_common_buffer: None,
            free_common_buffer: None,
            validate_common_buffer: None,
            register_iommu_backend: None,
            requester_identity: None,
            get_platform_quirk_policy: None,
        }
    }

    #[test]
    fn kernel_hal_services_require_version_size_and_memory_boundary() {
        let mut table = base_services();
        assert!(table.compatible());
        table.version = 0;
        assert!(!table.compatible());
        table.version = HAL_KERNEL_SERVICES_VERSION;
        table.size = 0;
        assert!(!table.compatible());
        table.size = core::mem::size_of::<HalKernelServices>() as u32;
        table.map_io_space = None;
        assert!(!table.compatible());
    }

    #[test]
    fn dma_services_are_advertised_only_as_a_complete_set() {
        let table = base_services();
        assert!(!table.has_common_dma_services());
        assert!(!table.has_iommu_registration_services());
    }

    #[test]
    fn iommu_backend_layout_is_shared_and_version_independent() {
        assert_eq!(core::mem::offset_of!(IommuBackend, size), 0);
        assert_eq!(core::mem::offset_of!(IommuBackend, context), 8);
        assert_eq!(core::mem::size_of::<IommuBackend>(), 40);
    }
}

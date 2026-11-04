#![no_std]
#![no_main]

use core::{cell::UnsafeCell, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};
use cognacos_pci as pci;
use cognacos_pci::EcamWindowExt;
use cognacos_hal_contract::{PciInterruptHalServices, PciHostBridgeResourceSet};
use cognacos_nt_abi::{bus::{
    BUS_INTERFACE_STANDARD, BUS_INTERFACE_STANDARD_VERSION, GUID_BUS_INTERFACE_STANDARD,
    PCI_WHICHSPACE_CONFIG, PCI_BUS_INTERFACE, INTERFACE_HEADER, PCI_BUS_INTERFACE_VERSION,
    PCI_DRIVER_HANDOFF, PCI_DRIVER_HANDOFF_MAGIC, PCI_DRIVER_HANDOFF_VERSION,
    PCI_DRIVER_HANDOFF_MAX_WINDOWS, PCI_DEVICE_INFORMATION, PCI_XHCI_INFORMATION,
    ACPI_FIRMWARE_INTERFACE_V4, ACPI_PCI_INTERRUPT_ROUTE, ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3,
    ACPI_PCI_HOST_BRIDGE_RESOURCES_INTERFACE,
    PCI_INTERRUPT_INTERFACE, PCI_INTERRUPT_INTERFACE_VERSION, PCI_INTERRUPT_ISR,
    PCI_INTERRUPT_DPC,
}, io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STACK_LOCATION}, ntstatus::*, types::PVOID};
use cognacos_nt_abi::object::UNICODE_STRING;
use cognacos_nt_abi::mm::{MM_PROTECTED_MEMORY_SNAPSHOT, MM_PROTECTED_MEMORY_SNAPSHOT_VERSION, MM_PROTECTED_MEMORY_SNAPSHOT_MAX_RANGES, PHYSICAL_MEMORY_RANGE};

unsafe extern "win64" {
    fn IoCreateDevice(
        driver: *mut DRIVER_OBJECT, extension_size: u32, device_name: *mut UNICODE_STRING,
        device_type: u32, characteristics: u32, exclusive: u8,
        device_object: *mut *mut DEVICE_OBJECT,
    ) -> i32;
    fn IoDeleteDevice(device: *mut DEVICE_OBJECT);
    fn ExAllocatePoolWithTag(pool_type: u32, number_of_bytes: usize, tag: u32) -> PVOID;
    fn ExFreePoolWithTag(pointer: PVOID, tag: u32);
    fn MmGetPhysicalMemoryRanges() -> PVOID;
    fn ObfReferenceObject(object: PVOID);
    fn ObDereferenceObject(object: PVOID);
    fn IoAllocateIrp(stack_size: i8, charge_quota: u8) -> *mut IRP;
    fn IoCallDriver(device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32;
    fn IoFreeIrp(irp: *mut IRP);
    fn IoCompleteRequest(irp: *mut IRP, priority_boost: i8);
    fn IoPnPReferenceBusFdo(enumerator_id: u32, identity_hash: u64, output: *mut *mut DEVICE_OBJECT) -> i32;
    fn IoPnPDereferenceBusFdo(device: *mut DEVICE_OBJECT);
    fn HalPciGetInterruptServices(output: *mut PciInterruptHalServices) -> i32;
}

// These are ordinary PE import thunks. The PE conversion step turns the
// __imp_* slots into a standard IMAGE_IMPORT_DESCRIPTOR/IAT for ntoskrnl.exe.
core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.global IoCreateDevice
.type IoCreateDevice,@function
IoCreateDevice:
    jmp QWORD PTR [rip + __imp_ntoskrnl_IoCreateDevice]
.size IoCreateDevice, .-IoCreateDevice
.global IoDeleteDevice
.type IoDeleteDevice,@function
IoDeleteDevice:
    jmp QWORD PTR [rip + __imp_ntoskrnl_IoDeleteDevice]
.size IoDeleteDevice, .-IoDeleteDevice
.global ExAllocatePoolWithTag
.type ExAllocatePoolWithTag,@function
ExAllocatePoolWithTag:
    jmp QWORD PTR [rip + __imp_ntoskrnl_ExAllocatePoolWithTag]
.size ExAllocatePoolWithTag, .-ExAllocatePoolWithTag
.global ExFreePoolWithTag
.type ExFreePoolWithTag,@function
ExFreePoolWithTag:
    jmp QWORD PTR [rip + __imp_ntoskrnl_ExFreePoolWithTag]
.size ExFreePoolWithTag, .-ExFreePoolWithTag
.global MmGetPhysicalMemoryRanges
.type MmGetPhysicalMemoryRanges,@function
MmGetPhysicalMemoryRanges:
    jmp QWORD PTR [rip + __imp_ntoskrnl_MmGetPhysicalMemoryRanges]
.size MmGetPhysicalMemoryRanges, .-MmGetPhysicalMemoryRanges
.global ObfReferenceObject
.type ObfReferenceObject,@function
ObfReferenceObject:
    jmp QWORD PTR [rip + __imp_ntoskrnl_ObfReferenceObject]
.size ObfReferenceObject, .-ObfReferenceObject
.global ObDereferenceObject
.type ObDereferenceObject,@function
ObDereferenceObject:
    jmp QWORD PTR [rip + __imp_ntoskrnl_ObDereferenceObject]
.size ObDereferenceObject, .-ObDereferenceObject

.section .data.imports.ntoskrnl,"aw",@progbits
.p2align 3
.global __imp_ntoskrnl_IoCreateDevice
.type __imp_ntoskrnl_IoCreateDevice,@object
__imp_ntoskrnl_IoCreateDevice: .quad 0
.size __imp_ntoskrnl_IoCreateDevice,8
.global __imp_ntoskrnl_IoDeleteDevice
.type __imp_ntoskrnl_IoDeleteDevice,@object
__imp_ntoskrnl_IoDeleteDevice: .quad 0
.size __imp_ntoskrnl_IoDeleteDevice,8
.global __imp_ntoskrnl_ExAllocatePoolWithTag
.type __imp_ntoskrnl_ExAllocatePoolWithTag,@object
__imp_ntoskrnl_ExAllocatePoolWithTag: .quad 0
.size __imp_ntoskrnl_ExAllocatePoolWithTag,8
.global __imp_ntoskrnl_ExFreePoolWithTag
.type __imp_ntoskrnl_ExFreePoolWithTag,@object
__imp_ntoskrnl_ExFreePoolWithTag: .quad 0
.size __imp_ntoskrnl_ExFreePoolWithTag,8
.global __imp_ntoskrnl_MmGetPhysicalMemoryRanges
.type __imp_ntoskrnl_MmGetPhysicalMemoryRanges,@object
__imp_ntoskrnl_MmGetPhysicalMemoryRanges: .quad 0
.size __imp_ntoskrnl_MmGetPhysicalMemoryRanges,8
.global __imp_ntoskrnl_ObfReferenceObject
.type __imp_ntoskrnl_ObfReferenceObject,@object
__imp_ntoskrnl_ObfReferenceObject: .quad 0
.size __imp_ntoskrnl_ObfReferenceObject,8
.global __imp_ntoskrnl_ObDereferenceObject
.type __imp_ntoskrnl_ObDereferenceObject,@object
__imp_ntoskrnl_ObDereferenceObject: .quad 0
.size __imp_ntoskrnl_ObDereferenceObject,8

// Keep executable import thunks in .text; the IAT slots below must remain
// one contiguous, ordered block for the native PE import-table converter.
.section .text.import_thunks,"ax",@progbits
.global IoAllocateIrp
.type IoAllocateIrp,@function
IoAllocateIrp:
    jmp QWORD PTR [rip + __imp_ntoskrnl_IoAllocateIrp]
.size IoAllocateIrp, .-IoAllocateIrp
.global IoCallDriver
.type IoCallDriver,@function
IoCallDriver:
    jmp QWORD PTR [rip + __imp_ntoskrnl_IoCallDriver]
.size IoCallDriver, .-IoCallDriver
.global IoFreeIrp
.type IoFreeIrp,@function
IoFreeIrp:
    jmp QWORD PTR [rip + __imp_ntoskrnl_IoFreeIrp]
.size IoFreeIrp, .-IoFreeIrp
.global IoCompleteRequest
.type IoCompleteRequest,@function
IoCompleteRequest:
    jmp QWORD PTR [rip + __imp_ntoskrnl_IoCompleteRequest]
.size IoCompleteRequest, .-IoCompleteRequest
.global IoPnPReferenceBusFdo
.type IoPnPReferenceBusFdo,@function
IoPnPReferenceBusFdo:
    jmp QWORD PTR [rip + __imp_ntoskrnl_IoPnPReferenceBusFdo]
.size IoPnPReferenceBusFdo, .-IoPnPReferenceBusFdo
.global IoPnPDereferenceBusFdo
.type IoPnPDereferenceBusFdo,@function
IoPnPDereferenceBusFdo:
    jmp QWORD PTR [rip + __imp_ntoskrnl_IoPnPDereferenceBusFdo]
.size IoPnPDereferenceBusFdo, .-IoPnPDereferenceBusFdo

.global HalPciGetInterruptServices
.type HalPciGetInterruptServices,@function
HalPciGetInterruptServices:
    jmp QWORD PTR [rip + __imp_hal_HalPciGetInterruptServices]
.size HalPciGetInterruptServices, .-HalPciGetInterruptServices

.section .data.imports.ntoskrnl,"aw",@progbits
.p2align 3
.global __imp_ntoskrnl_IoAllocateIrp
.type __imp_ntoskrnl_IoAllocateIrp,@object
__imp_ntoskrnl_IoAllocateIrp: .quad 0
.size __imp_ntoskrnl_IoAllocateIrp,8
.global __imp_ntoskrnl_IoCallDriver
.type __imp_ntoskrnl_IoCallDriver,@object
__imp_ntoskrnl_IoCallDriver: .quad 0
.size __imp_ntoskrnl_IoCallDriver,8
.global __imp_ntoskrnl_IoFreeIrp
.type __imp_ntoskrnl_IoFreeIrp,@object
__imp_ntoskrnl_IoFreeIrp: .quad 0
.size __imp_ntoskrnl_IoFreeIrp,8
.global __imp_ntoskrnl_IoCompleteRequest
.type __imp_ntoskrnl_IoCompleteRequest,@object
__imp_ntoskrnl_IoCompleteRequest: .quad 0
.size __imp_ntoskrnl_IoCompleteRequest,8
.global __imp_ntoskrnl_IoPnPReferenceBusFdo
.type __imp_ntoskrnl_IoPnPReferenceBusFdo,@object
__imp_ntoskrnl_IoPnPReferenceBusFdo: .quad 0
.size __imp_ntoskrnl_IoPnPReferenceBusFdo,8
.global __imp_ntoskrnl_IoPnPDereferenceBusFdo
.type __imp_ntoskrnl_IoPnPDereferenceBusFdo,@object
__imp_ntoskrnl_IoPnPDereferenceBusFdo: .quad 0
.size __imp_ntoskrnl_IoPnPDereferenceBusFdo,8

.section .data.imports.hal,"aw",@progbits
.p2align 3
.global __imp_hal_HalPciGetInterruptServices
.type __imp_hal_HalPciGetInterruptServices,@object
__imp_hal_HalPciGetInterruptServices: .quad 0
.size __imp_hal_HalPciGetInterruptServices,8
"#);

const STATUS_INVALID_DEVICE_STATE: i32 = 0xC000_0184_u32 as i32;
const PCI_PDO_EXTENSION_MAGIC: u64 = 0x434f_474e_5044_4f31;
#[repr(C)]
struct PciPdoExtension {
    magic: u64,
    version: u32,
    function_index: u32,
    device_object: *mut DEVICE_OBJECT,
    started: u8,
    raw_resources: PVOID,
    raw_resources_size: usize,
    translated_resources: PVOID,
    translated_resources_size: usize,
    information: PCI_DEVICE_INFORMATION,
    ide_ports: pci::ide::IdeControllerPorts,
    ide_ports_valid: u8,
    interrupt_vector: u8,
    interrupt_allocated: u8,
    interrupt_registered: u8,
    interrupt_reserved: u8,
    interrupt_context: u64,
    interrupt_hal: PciInterruptHalServices,
}

unsafe fn delete_pdo(pdo: *mut DEVICE_OBJECT) {
    if pdo.is_null() { return; }
    let extension = unsafe { (*pdo).device_extension.cast::<PciPdoExtension>() };
    if !extension.is_null() && unsafe { (*extension).magic == PCI_PDO_EXTENSION_MAGIC } {
        if unsafe { release_pci_interrupt(extension) } != STATUS_SUCCESS {
            // Never free a PDO extension while a registered ISR/DPC can still
            // reference it. PnP STOP/REMOVE reports the failure to the manager;
            // this fallback keeps unload from creating a use-after-free.
            return;
        }
        unsafe {
            if !(*extension).raw_resources.is_null() { ExFreePoolWithTag((*extension).raw_resources, PCI_REQUIREMENTS_POOL_TAG); }
            if !(*extension).translated_resources.is_null() { ExFreePoolWithTag((*extension).translated_resources, PCI_REQUIREMENTS_POOL_TAG); }
            (*extension).raw_resources = core::ptr::null_mut();
            (*extension).translated_resources = core::ptr::null_mut();
            (*extension).raw_resources_size = 0;
            (*extension).translated_resources_size = 0;
            (*extension).magic = 0;
        }
    }
    unsafe { IoDeleteDevice(pdo); }
}


const HANDOFF_MAGIC: u64 = PCI_DRIVER_HANDOFF_MAGIC;
const HANDOFF_VERSION: u32 = PCI_DRIVER_HANDOFF_VERSION;
const MAX_PCI_PDOS: usize = 32;
const BUS_RELATIONS: u32 = 0;
const MAX_WINDOWS: usize = PCI_DRIVER_HANDOFF_MAX_WINDOWS;

type PciDriverHandoff = PCI_DRIVER_HANDOFF;

struct PciState {
    ready: AtomicBool,
    inventory_ready: AtomicBool,
    bridge_count: AtomicUsize,
    xhci_count: AtomicUsize,
    function_count: AtomicUsize,
    functions: UnsafeCell<[[Option<pci::PciFunction>; pci::MAX_PCI_FUNCTIONS]; MAX_WINDOWS]>,
    function_counts: UnsafeCell<[usize; MAX_WINDOWS]>,
    bridges: UnsafeCell<[[Option<pci::PciBridgeRange>; pci::MAX_PCI_BRIDGES]; MAX_WINDOWS]>,
    bridge_counts: UnsafeCell<[usize; MAX_WINDOWS]>,
    bridge_resource_windows: UnsafeCell<[[Option<pci::bridge_resources::BridgeWindows>; pci::MAX_PCI_BRIDGES]; MAX_WINDOWS]>,
    xhci: UnsafeCell<[[Option<pci::XhciPciFunction>; pci::MAX_XHCI_FUNCTIONS]; MAX_WINDOWS]>,
    xhci_counts: UnsafeCell<[usize; MAX_WINDOWS]>,
    pdos: UnsafeCell<[*mut DEVICE_OBJECT; MAX_PCI_PDOS]>,
    pdo_count: AtomicUsize,
    host_bridge_resources: UnsafeCell<[PciHostBridgeResourceSet; MAX_WINDOWS]>,
    host_bridge_resource_count: AtomicUsize,
    protected_memory: UnsafeCell<MM_PROTECTED_MEMORY_SNAPSHOT>,
    clipped_host_apertures: UnsafeCell<[pci::host_bridge::PreparedHostBridgeApertures; MAX_WINDOWS]>,
}
unsafe impl Sync for PciState {}
static HANDOFF: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static INVENTORY_BUSY: AtomicBool = AtomicBool::new(false);
static STATE: PciState = PciState {
    ready: AtomicBool::new(false),
    inventory_ready: AtomicBool::new(false),
    bridge_count: AtomicUsize::new(0),
    xhci_count: AtomicUsize::new(0),
    function_count: AtomicUsize::new(0),
    functions: UnsafeCell::new([[None; pci::MAX_PCI_FUNCTIONS]; MAX_WINDOWS]),
    function_counts: UnsafeCell::new([0; MAX_WINDOWS]),
    bridges: UnsafeCell::new([[None; pci::MAX_PCI_BRIDGES]; MAX_WINDOWS]),
    bridge_counts: UnsafeCell::new([0; MAX_WINDOWS]),
    bridge_resource_windows: UnsafeCell::new([[None; pci::MAX_PCI_BRIDGES]; MAX_WINDOWS]),
    xhci: UnsafeCell::new([[None; pci::MAX_XHCI_FUNCTIONS]; MAX_WINDOWS]),
    xhci_counts: UnsafeCell::new([0; MAX_WINDOWS]),
    pdos: UnsafeCell::new([core::ptr::null_mut(); MAX_PCI_PDOS]),
    pdo_count: AtomicUsize::new(0),
    host_bridge_resources: UnsafeCell::new([const { PciHostBridgeResourceSet::empty(0, 0, 0) }; MAX_WINDOWS]),
    host_bridge_resource_count: AtomicUsize::new(0),
    protected_memory: UnsafeCell::new(MM_PROTECTED_MEMORY_SNAPSHOT::empty()),
    clipped_host_apertures: UnsafeCell::new([pci::host_bridge::PreparedHostBridgeApertures::EMPTY; MAX_WINDOWS]),
};
static DRIVER_OBJECT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static PDO_BUSY: AtomicBool = AtomicBool::new(false);
static HOST_BRIDGE_BUSY: AtomicBool = AtomicBool::new(false);

#[repr(C)]
struct PciDeviceRelations { count: u32, objects: [*mut DEVICE_OBJECT; MAX_PCI_PDOS] }
struct RelationsStorage(UnsafeCell<PciDeviceRelations>);
unsafe impl Sync for RelationsStorage {}
static RELATIONS: RelationsStorage = RelationsStorage(UnsafeCell::new(PciDeviceRelations {
    count: 0, objects: [core::ptr::null_mut(); MAX_PCI_PDOS],
}));

const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_QUERY_DEVICE_RELATIONS: u8 = 0x07;
const IRP_MN_QUERY_INTERFACE: u8 = 0x08;
const IRP_MN_QUERY_RESOURCE_REQUIREMENTS: u8 = 0x0b;
const IO_RESOURCE_REQUIREMENTS_MAX_SIZE: usize = 32 + 8 + 6 * pci::resource_manager::IO_RESOURCE_DESCRIPTOR_SIZE;
const PCI_REQUIREMENTS_POOL_TAG: u32 = u32::from_le_bytes(*b"PciR");

/// Enumerate the PCI configuration inventory at the bus PnP discovery
/// point, not during DriverEntry. PDO creation is deferred until BusRelations
/// so the PnP manager sees only persistent, fully initialized child objects.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciEnumerateInventory() -> i32 {
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if STATE.inventory_ready.load(Ordering::Acquire) { return STATUS_SUCCESS; }
    if INVENTORY_BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        return STATUS_DEVICE_BUSY;
    }
    let result = (|| {
        let address = HANDOFF.load(Ordering::Acquire);
        if address == 0 { return STATUS_DEVICE_NOT_READY; }
        let handoff = unsafe { &*(address as *const PciDriverHandoff) };
        let mut total_bridges = 0usize;
        let mut total_xhci = 0usize;
        let mut total_functions = 0usize;
        for i in 0..handoff.window_count as usize {
            let window = &handoff.windows[i];
            let (bridges, bridge_count) = match unsafe { pci::scan_bridge_ranges(window) } {
                Ok(v) => v, Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
            };
            let mut bridge_resources = [None; pci::MAX_PCI_BRIDGES];
            for (bi, bridge) in bridges.iter().take(bridge_count).flatten().enumerate() {
                let mut config = pci::EcamFunctionConfig {
                    window, segment: bridge.segment, bus: bridge.primary_bus,
                    device: bridge.device, function: bridge.function,
                };
                bridge_resources[bi] = match pci::bridge_resources::decode_bridge_windows(&mut config) {
                    Ok(windows) => Some(windows),
                    Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
                };
            }
            let (xhci, xhci_count) = match unsafe { pci::scan_xhci(window) } {
                Ok(v) => v, Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
            };
            let function_count = match unsafe { pci::scan_functions(window, &mut (*STATE.functions.get())[i]) } {
                Ok(v) => v, Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
            };
            unsafe {
                (*STATE.function_counts.get())[i] = function_count;
                (*STATE.bridges.get())[i] = bridges;
                (*STATE.bridge_counts.get())[i] = bridge_count;
                (*STATE.bridge_resource_windows.get())[i] = bridge_resources;
                (*STATE.xhci.get())[i] = xhci;
                (*STATE.xhci_counts.get())[i] = xhci_count;
            }
            total_bridges = match total_bridges.checked_add(bridge_count) { Some(v) => v, None => return STATUS_INSUFFICIENT_RESOURCES };
            total_xhci = match total_xhci.checked_add(xhci_count) { Some(v) => v, None => return STATUS_INSUFFICIENT_RESOURCES };
            total_functions = match total_functions.checked_add(function_count) { Some(v) => v, None => return STATUS_INSUFFICIENT_RESOURCES };
        }
        STATE.bridge_count.store(total_bridges, Ordering::Release);
        STATE.xhci_count.store(total_xhci, Ordering::Release);
        STATE.function_count.store(total_functions, Ordering::Release);
        STATE.inventory_ready.store(true, Ordering::Release);
        STATUS_SUCCESS
    })();
    INVENTORY_BUSY.store(false, Ordering::Release);
    result
}

/// Standard WDM interface reference callbacks. The context is the PDO
/// itself, so outstanding interface references keep the device object alive.
unsafe extern "win64" fn pci_bus_interface_reference(context:PVOID) {
    if !context.is_null() { unsafe { ObfReferenceObject(context); } }
}
unsafe extern "win64" fn pci_bus_interface_dereference(context:PVOID) {
    if !context.is_null() { unsafe { ObDereferenceObject(context); } }
}

/// BUS_INTERFACE_STANDARD GetBusData callback. The PDO's immutable BDF is
/// resolved against the firmware-provided ECAM windows; no project-private
/// PCI information interface is exposed to function drivers.
unsafe extern "win64" fn pci_bus_get_data(
    context:PVOID, data_type:u32, buffer:PVOID, offset:u32, length:u32,
)->u32 {
    if context.is_null() || buffer.is_null() || data_type != PCI_WHICHSPACE_CONFIG
        || length == 0 || offset >= 4096 || offset.checked_add(length).is_none()
        || offset + length > 4096 {
        return 0;
    }
    let device=context.cast::<DEVICE_OBJECT>();
    let extension=unsafe{(*device).device_extension.cast::<PciPdoExtension>()};
    if extension.is_null() || unsafe{(*extension).magic!=PCI_PDO_EXTENSION_MAGIC || (*extension).version!=1} {
        return 0;
    }
    let information=unsafe{(*extension).information};
    let address=HANDOFF.load(Ordering::Acquire);
    if address==0{return 0}
    let handoff=unsafe{&*(address as *const PciDriverHandoff)};
    if handoff.magic!=HANDOFF_MAGIC || handoff.version!=HANDOFF_VERSION
        || handoff.window_count as usize>MAX_WINDOWS { return 0; }
    for window in handoff.windows.iter().take(handoff.window_count as usize) {
        if window.segment!=information.segment || information.bus<window.bus_start || information.bus>window.bus_end {
            continue;
        }
        let mut copied=0u32;
        while copied<length {
            let current=offset+copied;
            let Ok(value) = (unsafe{window.read_u8(
                information.segment,information.bus,information.device,information.function,current as u16
            )}) else { break };
            unsafe{buffer.cast::<u8>().add(copied as usize).write(value);}
            copied+=1;
        }
        return copied;
    }
    0
}

/// Keep configuration writes fail-closed for standard header and capability
/// registers. Vendor-specific extended configuration space may be written
/// through the parent bus only when the caller requests PCI config space.
unsafe extern "win64" fn pci_bus_set_data(
    context:PVOID,data_type:u32,buffer:PVOID,offset:u32,length:u32,
)->u32 {
    if context.is_null() || buffer.is_null() || data_type!=PCI_WHICHSPACE_CONFIG
        || length==0 || offset<0x100 || offset>=4096 || offset.checked_add(length).is_none()
        || offset+length>4096 { return 0; }
    let device=context.cast::<DEVICE_OBJECT>();
    let extension=unsafe{(*device).device_extension.cast::<PciPdoExtension>()};
    if extension.is_null() || unsafe{(*extension).magic!=PCI_PDO_EXTENSION_MAGIC || (*extension).version!=1} { return 0; }
    let information=unsafe{(*extension).information};
    let address=HANDOFF.load(Ordering::Acquire);
    if address==0{return 0}
    let handoff=unsafe{&*(address as *const PciDriverHandoff)};
    if handoff.magic!=HANDOFF_MAGIC || handoff.version!=HANDOFF_VERSION
        || handoff.window_count as usize>MAX_WINDOWS { return 0; }
    for window in handoff.windows.iter().take(handoff.window_count as usize) {
        if window.segment!=information.segment || information.bus<window.bus_start || information.bus>window.bus_end { continue; }
        let mut written=0u32;
        while written<length {
            let current=offset+written;
            let value=unsafe{buffer.cast::<u8>().add(written as usize).read()};
            if unsafe{window.write_u8(information.segment,information.bus,information.device,information.function,current as u16,value)}.is_err(){break}
            written+=1;
        }
        return written;
    }
    0
}

/// The current PCI resource path publishes translated CM resources to
/// function drivers. It does not expose a second bus-address translation path.
unsafe extern "win64" fn pci_bus_translate_address(
    _context:PVOID,_bus_address:i64,_address_space:u32,
    _translated_address_space:*mut u32,_translated_address:*mut i64,
)->u8 { 0 }

/// The current storage stack is PIO-only and does not advertise DMA support.
/// Returning NULL is the standard no-adapter result; drivers must not assume
/// that every BUS_INTERFACE_STANDARD provider has a DMA adapter.
unsafe extern "win64" fn pci_bus_get_dma_adapter(
    _context:PVOID,_device_description:PVOID,_number_of_map_registers:*mut u32,
)->PVOID { core::ptr::null_mut() }

/// Publish the Microsoft WDM BUS_INTERFACE_STANDARD to a function PDO.
unsafe fn query_standard_bus_interface(device:*mut DEVICE_OBJECT,version:u16,size:u16,output:PVOID)->i32 {
    if device.is_null() || output.is_null() || version as u32!=BUS_INTERFACE_STANDARD_VERSION
        || (size as usize)<core::mem::size_of::<BUS_INTERFACE_STANDARD>() {
        return STATUS_NOT_SUPPORTED;
    }
    let extension=unsafe{(*device).device_extension.cast::<PciPdoExtension>()};
    if extension.is_null() || unsafe{(*extension).magic!=PCI_PDO_EXTENSION_MAGIC || (*extension).version!=1} {
        return STATUS_NOT_SUPPORTED;
    }
    let interface=BUS_INTERFACE_STANDARD {
        size:core::mem::size_of::<BUS_INTERFACE_STANDARD>() as u16,
        version:BUS_INTERFACE_STANDARD_VERSION as u16,
        context:device.cast(),
        interface_reference:Some(pci_bus_interface_reference),
        interface_dereference:Some(pci_bus_interface_dereference),
        translate_bus_address:Some(pci_bus_translate_address),
        get_dma_adapter:Some(pci_bus_get_dma_adapter),
        set_bus_data:Some(pci_bus_set_data),
        get_bus_data:Some(pci_bus_get_data),
    };
    // The bus driver owns one interface reference for every successful
    // IRP_MN_QUERY_INTERFACE response; the consumer releases it through
    // InterfaceDereference when it has finished using the returned table.
    unsafe { pci_bus_interface_reference(device.cast()); }
    unsafe{core::ptr::write_unaligned(output.cast::<BUS_INTERFACE_STANDARD>(),interface);}
    STATUS_SUCCESS
}

unsafe extern "win64" fn query_pdo_information(
    context: PVOID,
    output: *mut PCI_DEVICE_INFORMATION,
) -> i32 {
    if context.is_null() || output.is_null() { return STATUS_INVALID_PARAMETER; }
    let extension = context.cast::<PciPdoExtension>();
    if unsafe { (*extension).magic != PCI_PDO_EXTENSION_MAGIC || (*extension).version != 1 } {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { output.write((*extension).information); }
    STATUS_SUCCESS
}


/// Retrieve stable PDO-owned copies of resources accepted at START_DEVICE.
/// Pointers remain valid until STOP/REMOVE; callers must not free them.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciGetAssignedResources(context:PVOID,raw_out:*mut PVOID,raw_size_out:*mut usize,translated_out:*mut PVOID,translated_size_out:*mut usize)->i32{
    if context.is_null()||raw_out.is_null()||raw_size_out.is_null()||translated_out.is_null()||translated_size_out.is_null(){return STATUS_INVALID_PARAMETER}
    let ext=context.cast::<PciPdoExtension>();
    if unsafe{(*ext).magic!=PCI_PDO_EXTENSION_MAGIC||(*ext).version!=1}{return STATUS_INVALID_PARAMETER}
    if unsafe{(*ext).started==0||(*ext).raw_resources.is_null()||(*ext).translated_resources.is_null()}{return STATUS_DEVICE_NOT_READY}
    unsafe{raw_out.write((*ext).raw_resources);raw_size_out.write((*ext).raw_resources_size);translated_out.write((*ext).translated_resources);translated_size_out.write((*ext).translated_resources_size);}
    STATUS_SUCCESS
}

/// Return the validated I/O-port mapping for a started PCI IDE PDO.
/// This is the handoff consumed by pciide.sys/ataport.sys; it is populated only
/// from PnP-assigned BARs and is unavailable before START_DEVICE succeeds.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciGetIdePorts(
    context: PVOID,
    output: *mut pci::ide::IdeControllerPorts,
) -> i32 {
    if context.is_null() || output.is_null() { return STATUS_INVALID_PARAMETER; }
    let extension = context.cast::<PciPdoExtension>();
    if unsafe { (*extension).magic != PCI_PDO_EXTENSION_MAGIC || (*extension).version != 1 } {
        return STATUS_INVALID_PARAMETER;
    }
    if unsafe { (*extension).started == 0 || (*extension).ide_ports_valid == 0 } {
        return STATUS_DEVICE_NOT_READY;
    }
    unsafe { output.write((*extension).ide_ports); }
    STATUS_SUCCESS
}

unsafe fn query_acpi_pci_interrupt_route(
    request: &ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3,
    route: &mut ACPI_PCI_INTERRUPT_ROUTE,
) -> i32 {
    let mut acpi_bus = core::ptr::null_mut::<DEVICE_OBJECT>();
    let lookup = unsafe {
        IoPnPReferenceBusFdo(
            cognacos_nt_abi::bus::ACPI_BUS_ENUMERATOR_ID,
            cognacos_nt_abi::bus::ACPI_BUS_IDENTITY_HASH,
            &mut acpi_bus,
        )
    };
    if lookup != STATUS_SUCCESS { return lookup; }
    if acpi_bus.is_null() { return STATUS_NOT_FOUND; }

    let mut interface = core::mem::MaybeUninit::<ACPI_FIRMWARE_INTERFACE_V4>::zeroed();
    let irp = unsafe { IoAllocateIrp((*acpi_bus).stack_size as i8, 0) };
    if irp.is_null() {
        unsafe { IoPnPDereferenceBusFdo(acpi_bus); }
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    let status = (|| {
        let current = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
        if current.is_null() { return STATUS_INVALID_PARAMETER; }
        // IoGetNextIrpStackLocation is the WDM macro equivalent of
        // CurrentStackLocation - 1. IoCallDriver advances the IRP to this slot.
        let next = unsafe { current.sub(1) };
        unsafe {
            (*next).major_function = IRP_MJ_PNP as u8;
            (*next).minor_function = IRP_MN_QUERY_INTERFACE;
            let parameters = (*next).parameters.as_mut_ptr();
            core::ptr::write_unaligned(
                parameters.cast::<*const cognacos_nt_abi::bus::GUID>(),
                &cognacos_nt_abi::bus::GUID_ACPI_FIRMWARE_INTERFACE_V4,
            );
            core::ptr::write_unaligned(
                parameters.add(8).cast::<u16>(),
                core::mem::size_of::<ACPI_FIRMWARE_INTERFACE_V4>() as u16,
            );
            core::ptr::write_unaligned(
                parameters.add(10).cast::<u16>(),
                cognacos_nt_abi::bus::ACPI_FIRMWARE_INTERFACE_VERSION_4 as u16,
            );
            core::ptr::write_unaligned(parameters.add(16).cast::<PVOID>(), interface.as_mut_ptr().cast());
            (*irp).io_status.status.status = STATUS_NOT_SUPPORTED;
            (*irp).io_status.information = 0;
        }
        let call_status = unsafe { IoCallDriver(acpi_bus, irp) };
        // The ACPI query-interface dispatch is synchronous. The current
        // executive's IoCallDriver returns the dispatch status but leaves
        // completion to the PnP request owner, so this stack-owned helper
        // completes its own IRP before freeing it.
        if call_status == STATUS_PENDING {
            unsafe { (*irp).io_status.status.status = STATUS_INVALID_DEVICE_STATE; }
        }
        let io_status = unsafe { (*irp).io_status.status.status };
        let status = if call_status == STATUS_SUCCESS { io_status }
            else if call_status == STATUS_PENDING { STATUS_INVALID_DEVICE_STATE }
            else { call_status };
        unsafe { IoCompleteRequest(irp, 0); }
        if status != STATUS_SUCCESS { return status; }

        let interface = unsafe { interface.assume_init_ref() };
        if interface.header.version as u32 != cognacos_nt_abi::bus::ACPI_FIRMWARE_INTERFACE_VERSION_4
            || (interface.header.size as usize) < core::mem::size_of::<ACPI_FIRMWARE_INTERFACE_V4>()
            || interface.query_pci_interrupt_route_v3.is_none()
            || interface.header.interface_dereference.is_none() {
            if let Some(deref) = interface.header.interface_dereference {
                unsafe { deref(interface.header.context); }
            }
            return STATUS_NOT_SUPPORTED;
        }
        let query = interface.query_pci_interrupt_route_v3.unwrap();
        let dereference = interface.header.interface_dereference.unwrap();
        let route_status = unsafe { query(interface.header.context, request, route) };
        unsafe { dereference(interface.header.context); }
        route_status
    })();

    unsafe {
        IoFreeIrp(irp);
        IoPnPDereferenceBusFdo(acpi_bus);
    }
    status
}

#[repr(C)]
struct AcpiDeviceRelations { count: u32, objects: [*mut DEVICE_OBJECT; 64] }

/// Query one ACPI root PDO through its WDM IRP_MN_QUERY_INTERFACE contract.
/// The root PDO, not the ACPI bus FDO, is the provider for _CRS windows.
unsafe fn query_acpi_root_pdo_resources(
    pdo: *mut DEVICE_OBJECT,
    output: &mut PciHostBridgeResourceSet,
) -> i32 {
    if pdo.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut interface = core::mem::MaybeUninit::<ACPI_PCI_HOST_BRIDGE_RESOURCES_INTERFACE>::zeroed();
    let irp = unsafe { IoAllocateIrp((*pdo).stack_size as i8, 0) };
    if irp.is_null() { return STATUS_INSUFFICIENT_RESOURCES; }
    let status = (|| {
        let current = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
        if current.is_null() { return STATUS_INVALID_PARAMETER; }
        let next = unsafe { current.sub(1) };
        unsafe {
            (*next).major_function = IRP_MJ_PNP as u8;
            (*next).minor_function = IRP_MN_QUERY_INTERFACE;
            let parameters = (*next).parameters.as_mut_ptr();
            core::ptr::write_unaligned(parameters.cast::<*const cognacos_nt_abi::bus::GUID>(),
                &cognacos_nt_abi::bus::GUID_ACPI_PCI_HOST_BRIDGE_RESOURCES);
            core::ptr::write_unaligned(parameters.add(8).cast::<u16>(),
                core::mem::size_of::<ACPI_PCI_HOST_BRIDGE_RESOURCES_INTERFACE>() as u16);
            core::ptr::write_unaligned(parameters.add(10).cast::<u16>(),
                cognacos_nt_abi::bus::ACPI_PCI_HOST_BRIDGE_RESOURCES_VERSION as u16);
            core::ptr::write_unaligned(parameters.add(16).cast::<PVOID>(), interface.as_mut_ptr().cast());
            (*irp).io_status.status.status = STATUS_NOT_SUPPORTED;
            (*irp).io_status.information = 0;
        }
        let call_status = unsafe { IoCallDriver(pdo, irp) };
        let io_status = unsafe { (*irp).io_status.status.status };
        let result = if call_status == STATUS_SUCCESS { io_status }
            else if call_status == STATUS_PENDING { STATUS_INVALID_DEVICE_STATE }
            else { call_status };
        unsafe { IoCompleteRequest(irp, 0); }
        if result != STATUS_SUCCESS { return result; }

        let table = unsafe { interface.assume_init_ref() };
        if table.header.version as u32 != cognacos_nt_abi::bus::ACPI_PCI_HOST_BRIDGE_RESOURCES_VERSION
            || (table.header.size as usize) < core::mem::size_of::<ACPI_PCI_HOST_BRIDGE_RESOURCES_INTERFACE>()
            || table.query_resources.is_none()
            || table.header.interface_dereference.is_none() {
            if let Some(deref) = table.header.interface_dereference {
                unsafe { deref(table.header.context); }
            }
            return STATUS_DEVICE_CONFIGURATION_ERROR;
        }
        let query = table.query_resources.unwrap();
        let dereference = table.header.interface_dereference.unwrap();
        let query_status = unsafe { query(table.header.context, pdo.cast(), (output as *mut PciHostBridgeResourceSet).cast()) };
        unsafe { dereference(table.header.context); }
        query_status
    })();
    unsafe { IoFreeIrp(irp); }
    status
}

/// Resolve exactly one ACPI root PDO whose _SEG/_BBN/_CRS covers this MCFG
/// bus window. Ambiguous roots or incomplete resource publication are fatal.
unsafe fn query_acpi_host_bridge_for_window(
    window: &pci::EcamWindow,
    output: &mut PciHostBridgeResourceSet,
) -> i32 {
    let mut acpi_bus = core::ptr::null_mut::<DEVICE_OBJECT>();
    let lookup = unsafe { IoPnPReferenceBusFdo(
        cognacos_nt_abi::bus::ACPI_BUS_ENUMERATOR_ID,
        cognacos_nt_abi::bus::ACPI_BUS_IDENTITY_HASH,
        &mut acpi_bus,
    ) };
    if lookup != STATUS_SUCCESS { return lookup; }
    if acpi_bus.is_null() { return STATUS_NOT_FOUND; }

    let irp = unsafe { IoAllocateIrp((*acpi_bus).stack_size as i8, 0) };
    if irp.is_null() {
        unsafe { IoPnPDereferenceBusFdo(acpi_bus); }
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    let relations_status = (|| {
        let current = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
        if current.is_null() { return STATUS_INVALID_PARAMETER; }
        let next = unsafe { current.sub(1) };
        unsafe {
            (*next).major_function = IRP_MJ_PNP as u8;
            (*next).minor_function = IRP_MN_QUERY_DEVICE_RELATIONS;
            core::ptr::write_unaligned((*next).parameters.as_mut_ptr().cast::<u32>(), BUS_RELATIONS);
            (*irp).io_status.status.status = STATUS_NOT_SUPPORTED;
            (*irp).io_status.information = 0;
        }
        let call_status = unsafe { IoCallDriver(acpi_bus, irp) };
        let io_status = unsafe { (*irp).io_status.status.status };
        let status = if call_status == STATUS_SUCCESS { io_status }
            else if call_status == STATUS_PENDING { STATUS_INVALID_DEVICE_STATE }
            else { call_status };
        if status != STATUS_SUCCESS { return status; }
        let relations = unsafe { (*irp).io_status.information as *const AcpiDeviceRelations };
        if relations.is_null() { return STATUS_DEVICE_CONFIGURATION_ERROR; }
        let count = unsafe { (*relations).count as usize };
        if count > 64 { return STATUS_DEVICE_CONFIGURATION_ERROR; }
        let mut matches = 0usize;
        let mut selected = PciHostBridgeResourceSet::empty(window.segment, window.bus_start, window.bus_end);
        for index in 0..count {
            let pdo = unsafe { (*relations).objects[index] };
            if pdo.is_null() { return STATUS_DEVICE_CONFIGURATION_ERROR; }
            let mut candidate = PciHostBridgeResourceSet::empty(window.segment, window.bus_start, window.bus_end);
            let query = unsafe { query_acpi_root_pdo_resources(pdo, &mut candidate) };
            if query == STATUS_NOT_SUPPORTED || query == STATUS_INVALID_PARAMETER { continue; }
            if query != STATUS_SUCCESS { return query; }
            if !candidate.compatible() { return STATUS_DEVICE_CONFIGURATION_ERROR; }
            if candidate.segment == window.segment
                && candidate.bus_start <= window.bus_start
                && candidate.bus_end >= window.bus_end {
                matches += 1;
                selected = candidate;
            }
        }
        if matches != 1 { return if matches == 0 { STATUS_NOT_FOUND } else { STATUS_DEVICE_CONFIGURATION_ERROR }; }
        *output = selected;
        STATUS_SUCCESS
    })();
    unsafe {
        IoCompleteRequest(irp, 0);
        IoFreeIrp(irp);
        IoPnPDereferenceBusFdo(acpi_bus);
    }
    relations_status
}

/// PCI root-FDO initialization consumes ACPI root-PDO resource interfaces
/// before it publishes PCI children. MCFG is only config-space reachability;
/// the ACPI _CRS resource set is retained separately for aperture arbitration.
const MEMORY_RANGES_POOL_TAG: u32 = u32::from_le_bytes(*b"RMMM");

/// Take a bounded, versioned copy of the kernel's authoritative RAM map.
/// MmGetPhysicalMemoryRanges returns a terminated array owned by the caller;
/// the PCI driver validates every element before publishing its immutable
/// snapshot. A missing terminator or truncated/oversized map fails closed.
unsafe fn acquire_protected_memory_snapshot(
    output: &mut MM_PROTECTED_MEMORY_SNAPSHOT,
) -> i32 {
    let source = unsafe { MmGetPhysicalMemoryRanges() };
    if source.is_null() { return STATUS_DEVICE_CONFIGURATION_ERROR; }
    let mut staged = MM_PROTECTED_MEMORY_SNAPSHOT::empty();
    let ranges = source.cast::<PHYSICAL_MEMORY_RANGE>();
    let mut terminated = false;
    for index in 0..=MM_PROTECTED_MEMORY_SNAPSHOT_MAX_RANGES {
        let item = unsafe { ranges.add(index).read_unaligned() };
        if item.base_address == 0 && item.number_of_bytes == 0 {
            terminated = true;
            break;
        }
        if index == MM_PROTECTED_MEMORY_SNAPSHOT_MAX_RANGES
            || item.base_address < 0 || item.number_of_bytes == 0
            || (item.base_address as u64) & 0xfff != 0
            || item.number_of_bytes & 0xfff != 0
            || (item.base_address as u64).checked_add(item.number_of_bytes).is_none() {
            unsafe { ExFreePoolWithTag(source, MEMORY_RANGES_POOL_TAG); }
            return STATUS_DEVICE_CONFIGURATION_ERROR;
        }
        let start = item.base_address as u64;
        if index != 0 {
            let previous = staged.ranges[index - 1];
            let previous_end = (previous.base_address as u64)
                .checked_add(previous.number_of_bytes);
            if previous_end.is_none_or(|end| start <= end) {
                unsafe { ExFreePoolWithTag(source, MEMORY_RANGES_POOL_TAG); }
                return STATUS_DEVICE_CONFIGURATION_ERROR;
            }
        }
        staged.ranges[index] = item;
        staged.count += 1;
    }
    unsafe { ExFreePoolWithTag(source, MEMORY_RANGES_POOL_TAG); }
    if !terminated || staged.count == 0 || staged.version != MM_PROTECTED_MEMORY_SNAPSHOT_VERSION {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    *output = staged;
    STATUS_SUCCESS
}

fn acquire_acpi_host_bridge_resources() -> i32 {
    let address = HANDOFF.load(Ordering::Acquire);
    if address == 0 { return STATUS_DEVICE_NOT_READY; }
    let handoff = unsafe { &*(address as *const PciDriverHandoff) };
    let mut staged = [const { PciHostBridgeResourceSet::empty(0, 0, 0) }; MAX_WINDOWS];
    for index in 0..handoff.window_count as usize {
        let status = unsafe { query_acpi_host_bridge_for_window(&handoff.windows[index], &mut staged[index]) };
        if status != STATUS_SUCCESS { return status; }
    }

    let mut protected = MM_PROTECTED_MEMORY_SNAPSHOT::empty();
    let status = unsafe { acquire_protected_memory_snapshot(&mut protected) };
    if status != STATUS_SUCCESS { return status; }
    let mut protected_ranges = [pci::e820::E820Range { start: 0, length: 0 }; MM_PROTECTED_MEMORY_SNAPSHOT_MAX_RANGES];
    for (index, range) in protected.ranges.iter().take(protected.count as usize).enumerate() {
        protected_ranges[index] = pci::e820::E820Range {
            start: range.base_address as u64,
            length: range.number_of_bytes,
        };
    }

    let mut clipped = [pci::host_bridge::PreparedHostBridgeApertures::EMPTY; MAX_WINDOWS];
    for index in 0..handoff.window_count as usize {
        clipped[index] = match pci::host_bridge::prepare_host_bridge_apertures(
            &staged[index], &protected_ranges[..protected.count as usize],
        ) {
            Ok(value) => value,
            Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
        };
        if clipped[index].segment != handoff.windows[index].segment
            || clipped[index].bus_start > handoff.windows[index].bus_start
            || clipped[index].bus_end < handoff.windows[index].bus_end {
            return STATUS_DEVICE_CONFIGURATION_ERROR;
        }
    }

    unsafe {
        (*STATE.host_bridge_resources.get()) = staged;
        (*STATE.protected_memory.get()) = protected;
        (*STATE.clipped_host_apertures.get()) = clipped;
    }
    STATE.host_bridge_resource_count.store(handoff.window_count as usize, Ordering::Release);
    STATUS_SUCCESS
}

/// Read the root-PDO resource set staged by the PCI root-FDO. The clipped
/// aperture cache is published atomically with this descriptor cache.
fn get_host_bridge_resources(
    index: usize,
) -> Result<PciHostBridgeResourceSet, i32> {
    let count = STATE.host_bridge_resource_count.load(Ordering::Acquire);
    if index >= count { return Err(STATUS_NOT_FOUND); }
    Ok(unsafe { (*STATE.host_bridge_resources.get())[index] })
}

fn hal_status_to_ntstatus(status: u32) -> i32 {
    if status == 0 { STATUS_SUCCESS } else { STATUS_DEVICE_CONFIGURATION_ERROR }
}

unsafe fn configure_pci_intx_masked(
    extension: *mut PciPdoExtension,
    function: pci::PciFunction,
    window: &pci::EcamWindow,
) -> i32 {
    let pin = match unsafe {
        window.read_u8(function.segment, function.bus, function.device, function.function, 0x3d)
    } {
        Ok(value) => value,
        Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
    };
    // PCI Interrupt Line (offset 0x3c) is legacy BIOS state, not a portable
    // GSI in APIC mode. Route INTx through ACPI _PRT and bridge swizzling.
    if pin == 0 { return STATUS_SUCCESS; }
    if pin > 4 { return STATUS_DEVICE_CONFIGURATION_ERROR; }
    if !STATE.inventory_ready.load(Ordering::Acquire) {
        return STATUS_DEVICE_NOT_READY;
    }

    let request_bytes = core::mem::size_of::<ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3>();
    let bridge_capacity = pci::MAX_PCI_BRIDGES.checked_mul(MAX_WINDOWS)
        .ok_or(STATUS_INSUFFICIENT_RESOURCES);
    let bridge_capacity = match bridge_capacity { Ok(capacity) => capacity, Err(status) => return status };
    let bridge_bytes = core::mem::size_of::<pci::PciBridgeRange>()
        .checked_mul(bridge_capacity).ok_or(STATUS_INSUFFICIENT_RESOURCES);
    let bridge_bytes = match bridge_bytes { Ok(bytes) => bytes, Err(status) => return status };
    // The expanded v4 request and full-segment bridge inventory are pool-backed:
    // neither multi-kilobyte buffer is placed on the kernel stack.
    let request_allocation = unsafe {
        ExAllocatePoolWithTag(1, request_bytes, PCI_REQUIREMENTS_POOL_TAG)
    };
    if request_allocation.is_null() { return STATUS_INSUFFICIENT_RESOURCES; }
    let bridge_allocation = unsafe {
        ExAllocatePoolWithTag(1, bridge_bytes, PCI_REQUIREMENTS_POOL_TAG)
    };
    if bridge_allocation.is_null() {
        unsafe { ExFreePoolWithTag(request_allocation, PCI_REQUIREMENTS_POOL_TAG); }
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    let result = (|| {
        unsafe {
            core::ptr::write_bytes(request_allocation.cast::<u8>(), 0, request_bytes);
            core::ptr::write_bytes(bridge_allocation.cast::<u8>(), 0, bridge_bytes);
        }
        let request = unsafe { &mut *request_allocation.cast::<ACPI_PCI_INTERRUPT_ROUTE_REQUEST_V3>() };
        request.segment = function.segment;
        request.bus = function.bus;
        request.device = function.device;
        request.function = function.function;
        request.pin = pin;

        let bridge_ranges = unsafe {
            core::slice::from_raw_parts_mut(
                bridge_allocation.cast::<pci::PciBridgeRange>(),
                bridge_capacity,
            )
        };
        let mut bridge_range_count = 0usize;
        for window_index in 0..MAX_WINDOWS {
            let count = unsafe { (*STATE.bridge_counts.get())[window_index] };
            let entries = unsafe { &(*STATE.bridges.get())[window_index] };
            for bridge in entries.iter().take(count).flatten() {
                if bridge.segment == function.segment {
                    if bridge_range_count == bridge_ranges.len() {
                        return STATUS_INSUFFICIENT_RESOURCES;
                    }
                    bridge_ranges[bridge_range_count] = *bridge;
                    bridge_range_count += 1;
                }
            }
        }
        let requester = match pci::resolve_requester_path(
            function.segment, window.bus_start, function.bus,
            function.device, function.function, &bridge_ranges[..bridge_range_count],
        ) {
            Ok(path) => path,
            Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
        };
        request.root_bus = requester.root_bus;
        request.bridge_count = requester.path_count.saturating_sub(1) as u8;
        let mut current_bus = requester.root_bus;
        for path_index in 0..request.bridge_count as usize {
            let path_element = requester.path[path_index];
            let Some(bridge) = bridge_ranges[..bridge_range_count].iter().find(|bridge|
                bridge.primary_bus == current_bus
                    && bridge.device == path_element.device
                    && bridge.function == path_element.function
            ) else { return STATUS_DEVICE_CONFIGURATION_ERROR; };
            request.bridges[path_index] = cognacos_nt_abi::bus::ACPI_PCI_BRIDGE_PATH_ENTRY {
                segment: bridge.segment,
                primary_bus: bridge.primary_bus,
                secondary_bus: bridge.secondary_bus,
                subordinate_bus: bridge.subordinate_bus,
                device: bridge.device,
                function: bridge.function,
                reserved: 0,
            };
            current_bus = bridge.secondary_bus;
        }
        if current_bus != function.bus { return STATUS_DEVICE_CONFIGURATION_ERROR; }
        let mut route = ACPI_PCI_INTERRUPT_ROUTE::default();
        let status = unsafe { query_acpi_pci_interrupt_route(request, &mut route) };
        if status != STATUS_SUCCESS { return status; }
        if route.reserved != [0; 2] || route.reserved2 != 0
            || route.active_low > 1 || route.level_triggered > 1
            || route.io_apic_base == 0 || route.io_apic_base & 0xfff != 0
            || route.gsi < route.gsi_base || route.gsi - route.gsi_base >= 120 {
            return STATUS_DEVICE_CONFIGURATION_ERROR;
        }

        let mut hal = unsafe { core::mem::zeroed::<PciInterruptHalServices>() };
        let status = unsafe { HalPciGetInterruptServices(&mut hal) };
        if status != STATUS_SUCCESS || !hal.compatible() { return STATUS_NOT_SUPPORTED; }
        let Some(allocate) = hal.allocate_vector else { return STATUS_NOT_SUPPORTED; };
        let Some(configure) = hal.configure_masked else { return STATUS_NOT_SUPPORTED; };
        let Some(release) = hal.release_vector else { return STATUS_NOT_SUPPORTED; };
        let mut vector = 0u8;
        let hal_status = unsafe { allocate(&mut vector) };
        if hal_status != 0 { return hal_status_to_ntstatus(hal_status); }
        let hal_status = unsafe {
            configure(
                vector, route.io_apic_base, route.gsi_base, route.gsi,
                route.active_low != 0, route.level_triggered != 0,
            )
        };
        if hal_status != 0 {
            let _ = unsafe { release(vector) };
            return hal_status_to_ntstatus(hal_status);
        }
        unsafe {
            (*extension).interrupt_vector = vector;
            (*extension).interrupt_allocated = 1;
            (*extension).interrupt_registered = 0;
            (*extension).interrupt_context = 0;
            (*extension).interrupt_hal = hal;
        }
        STATUS_SUCCESS
    })();

    unsafe {
        ExFreePoolWithTag(bridge_allocation, PCI_REQUIREMENTS_POOL_TAG);
        ExFreePoolWithTag(request_allocation, PCI_REQUIREMENTS_POOL_TAG);
    }
    result
}

unsafe fn pdo_interrupt_extension(context: PVOID) -> Result<*mut PciPdoExtension, i32> {
    if context.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let device = context.cast::<DEVICE_OBJECT>();
    let extension = unsafe { (*device).device_extension.cast::<PciPdoExtension>() };
    if extension.is_null()
        || unsafe { (*extension).magic != PCI_PDO_EXTENSION_MAGIC || (*extension).version != 1 } {
        return Err(STATUS_INVALID_PARAMETER);
    }
    Ok(extension)
}

unsafe extern "win64" fn pci_pdo_interrupt_register(
    context: PVOID,
    callback_context: u64,
    isr: PCI_INTERRUPT_ISR,
    dpc: PCI_INTERRUPT_DPC,
) -> i32 {
    let extension = match unsafe { pdo_interrupt_extension(context) } {
        Ok(extension) => extension, Err(status) => return status,
    };
    if unsafe { (*extension).started == 0 || (*extension).interrupt_allocated == 0
        || (*extension).interrupt_registered != 0 || callback_context == 0 } {
        return STATUS_DEVICE_NOT_READY;
    }
    let Some(register) = (unsafe { (*extension).interrupt_hal.register_isr }) else {
        return STATUS_NOT_SUPPORTED;
    };
    let status = unsafe {
        register((*extension).interrupt_vector, callback_context, Some(isr), Some(dpc))
    };
    if status == 0 {
        unsafe {
            (*extension).interrupt_context = callback_context;
            (*extension).interrupt_registered = 1;
        }
        STATUS_SUCCESS
    } else {
        hal_status_to_ntstatus(status)
    }
}

unsafe extern "win64" fn pci_pdo_interrupt_enable(context: PVOID) -> i32 {
    let extension = match unsafe { pdo_interrupt_extension(context) } {
        Ok(extension) => extension, Err(status) => return status,
    };
    if unsafe { (*extension).started == 0 || (*extension).interrupt_allocated == 0
        || (*extension).interrupt_registered == 0 } {
        return STATUS_DEVICE_NOT_READY;
    }
    let Some(enable) = (unsafe { (*extension).interrupt_hal.enable }) else {
        return STATUS_NOT_SUPPORTED;
    };
    hal_status_to_ntstatus(unsafe { enable((*extension).interrupt_vector) })
}

unsafe extern "win64" fn pci_pdo_interrupt_disable(context: PVOID) -> i32 {
    let extension = match unsafe { pdo_interrupt_extension(context) } {
        Ok(extension) => extension, Err(status) => return status,
    };
    if unsafe { (*extension).interrupt_allocated == 0 || (*extension).interrupt_registered == 0 } {
        return STATUS_SUCCESS;
    }
    let Some(disable) = (unsafe { (*extension).interrupt_hal.disable }) else {
        return STATUS_NOT_SUPPORTED;
    };
    hal_status_to_ntstatus(unsafe { disable((*extension).interrupt_vector) })
}

unsafe extern "win64" fn pci_pdo_interrupt_unregister(context: PVOID) -> i32 {
    let extension = match unsafe { pdo_interrupt_extension(context) } {
        Ok(extension) => extension, Err(status) => return status,
    };
    if unsafe { (*extension).interrupt_registered == 0 } { return STATUS_SUCCESS; }
    let Some(disable) = (unsafe { (*extension).interrupt_hal.disable }) else {
        return STATUS_NOT_SUPPORTED;
    };
    let Some(unregister) = (unsafe { (*extension).interrupt_hal.unregister }) else {
        return STATUS_NOT_SUPPORTED;
    };
    let vector = unsafe { (*extension).interrupt_vector };
    let callback_context = unsafe { (*extension).interrupt_context };
    let status = unsafe { disable(vector) };
    if status != 0 { return hal_status_to_ntstatus(status); }
    let status = unsafe { unregister(vector, callback_context) };
    if status == 0 {
        unsafe {
            (*extension).interrupt_registered = 0;
            (*extension).interrupt_context = 0;
        }
        STATUS_SUCCESS
    } else {
        hal_status_to_ntstatus(status)
    }
}

unsafe fn release_pci_interrupt(extension: *mut PciPdoExtension) -> i32 {
    if extension.is_null() || unsafe { (*extension).interrupt_allocated == 0 } {
        return STATUS_SUCCESS;
    }
    if unsafe { (*extension).interrupt_registered != 0 } {
        let device = unsafe { (*extension).device_object };
        if device.is_null() { return STATUS_INVALID_PARAMETER; }
        let status = unsafe { pci_pdo_interrupt_unregister(device.cast()) };
        if status != STATUS_SUCCESS { return status; }
    }
    let Some(release) = (unsafe { (*extension).interrupt_hal.release_vector }) else {
        return STATUS_NOT_SUPPORTED;
    };
    let status = unsafe { release((*extension).interrupt_vector) };
    if status == 0 {
        unsafe {
            (*extension).interrupt_allocated = 0;
            (*extension).interrupt_registered = 0;
            (*extension).interrupt_context = 0;
            (*extension).interrupt_vector = 0;
            (*extension).interrupt_hal = core::mem::zeroed();
        }
        STATUS_SUCCESS
    } else {
        hal_status_to_ntstatus(status)
    }
}

/// Consume the PnP manager's assigned raw/translated resource lists. BARs are
/// programmed from the bus-relative (raw) addresses; translated resources are
/// retained only for consumers above the bus driver. Unsupported resource
/// layouts fail closed rather than guessing an address.
fn assigned_range_fits_clipped_host_aperture(
    window_index: usize,
    kind: pci::resources::BarKind,
    prefetchable: bool,
    start: u64,
    length: u64,
) -> bool {
    if length == 0 || start.checked_add(length).is_none() { return false; }
    let count = STATE.host_bridge_resource_count.load(Ordering::Acquire);
    if window_index >= count || window_index >= MAX_WINDOWS { return false; }
    let prepared = unsafe { (*STATE.clipped_host_apertures.get())[window_index] };
    let end = start + length;
    prepared.as_slice().iter().any(|aperture| {
        aperture.kind == kind
            && (kind == pci::resources::BarKind::Io || aperture.prefetchable == prefetchable)
            && aperture.start <= start
            && aperture.start.checked_add(aperture.length).is_some_and(|limit| end <= limit)
    })
}

fn start_pci_device(
    device: *mut DEVICE_OBJECT,
    irp: *mut cognacos_nt_abi::io::IRP,
    stack: *mut cognacos_nt_abi::io::IO_STACK_LOCATION,
) -> i32 {
    if device.is_null() || irp.is_null() || stack.is_null()
        || !STATE.ready.load(Ordering::Acquire) {
        return STATUS_DEVICE_NOT_READY;
    }
    let extension = unsafe { (*device).device_extension.cast::<PciPdoExtension>() };
    if extension.is_null()
        || unsafe { (*extension).magic != PCI_PDO_EXTENSION_MAGIC || (*extension).version != 1 } {
        return STATUS_INVALID_PARAMETER;
    }
    if unsafe { (*extension).started != 0 } { return STATUS_SUCCESS; }
    // START_DEVICE's first two pointer-sized fields are AllocatedResources and
    // AllocatedResourcesTranslated in the x64 PnP stack parameter union.
    let parameters = unsafe { (*stack).parameters.as_ptr() };
    let raw = unsafe { core::ptr::read_unaligned(parameters.cast::<*const u8>()) };
    let translated = unsafe { core::ptr::read_unaligned(parameters.add(8).cast::<*const u8>()) };
    if raw.is_null() || translated.is_null() { return STATUS_DEVICE_CONFIGURATION_ERROR; }

    let address = HANDOFF.load(Ordering::Acquire);
    if address == 0 { return STATUS_DEVICE_NOT_READY; }
    let handoff = unsafe { &*(address as *const PciDriverHandoff) };
    let mut remaining = unsafe { (*extension).function_index as usize };
    let mut function = None;
    for wi in 0..handoff.window_count as usize {
        let n = unsafe { (*STATE.function_counts.get())[wi] };
        if remaining >= n { remaining -= n; continue; }
        function = unsafe { (*STATE.functions.get())[wi][remaining] };
        break;
    }
    let Some(function) = function else { return STATUS_NOT_FOUND; };
    let Some((window_index, window)) = handoff.windows[..handoff.window_count as usize].iter().enumerate().find(|(_, w)|
        w.segment == function.segment && function.bus >= w.bus_start && function.bus <= w.bus_end
    ) else { return STATUS_DEVICE_CONFIGURATION_ERROR; };
    // The PCI root FDO has already queried the matching ACPI root PDO. Refuse
    // child START_DEVICE if the root bridge's _CRS identity/range does not
    // cover this function; the PnP arbiter remains responsible for assignment.
    let host_resources = match get_host_bridge_resources(window_index) {
        Ok(value) => value, Err(status) => return status,
    };
    if !host_resources.compatible() || host_resources.segment != function.segment
        || function.bus < host_resources.bus_start || function.bus > host_resources.bus_end {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    let mut config = pci::EcamFunctionConfig {
        window, segment: function.segment, bus: function.bus,
        device: function.device, function: function.function,
    };
    let mut requirements: [Option<pci::resources::BarRequirement>; 6] = [None; 6];
    let count = match pci::size_bars_exclusive(&mut config, function.header.header_type, &mut requirements) {
        Ok(n) => n, Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
    };

    // CM_RESOURCE_LIST: count, then one CM_FULL_RESOURCE_DESCRIPTOR followed
    // by a CM_PARTIAL_RESOURCE_LIST and 16-byte CM_PARTIAL_RESOURCE_DESCRIPTORs.
    let full_count = unsafe { core::ptr::read_unaligned(raw.cast::<u32>()) };
    if full_count != 1 { return STATUS_NOT_SUPPORTED; }
    let raw_interface = unsafe { core::ptr::read_unaligned(raw.add(4).cast::<u32>()) };
    let raw_bus = unsafe { core::ptr::read_unaligned(raw.add(8).cast::<u32>()) };
    let raw_version = unsafe { core::ptr::read_unaligned(raw.add(12).cast::<u16>()) };
    let raw_revision = unsafe { core::ptr::read_unaligned(raw.add(14).cast::<u16>()) };
    let raw_n = unsafe { core::ptr::read_unaligned(raw.add(16).cast::<u32>()) } as usize;
    if raw_interface != 5 || raw_bus != function.bus as u32 || raw_version != 1 || raw_revision != 1 || raw_n > 64 {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    let raw_desc = unsafe { raw.add(20) };
    let trans_count = unsafe { core::ptr::read_unaligned(translated.cast::<u32>()) };
    if trans_count != 1 { return STATUS_NOT_SUPPORTED; }
    let trans_interface = unsafe { core::ptr::read_unaligned(translated.add(4).cast::<u32>()) };
    let trans_bus = unsafe { core::ptr::read_unaligned(translated.add(8).cast::<u32>()) };
    let trans_version = unsafe { core::ptr::read_unaligned(translated.add(12).cast::<u16>()) };
    let trans_revision = unsafe { core::ptr::read_unaligned(translated.add(14).cast::<u16>()) };
    if trans_interface != raw_interface || trans_bus != raw_bus || trans_version != 1 || trans_revision != 1 {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    let trans_n = unsafe { core::ptr::read_unaligned(translated.add(16).cast::<u32>()) } as usize;
    if trans_n != raw_n { return STATUS_DEVICE_CONFIGURATION_ERROR; }
    let trans_desc = unsafe { translated.add(20) };

    let mut plan = pci::resource_manager::AllocationPlan::empty();
    let mut translated_plan = pci::resource_manager::AllocationPlan::empty();
    let mut resource_index = 0usize;
    for req in requirements.iter().take(count).flatten() {
        let mut found = None;
        let mut translated_base = None;
        while resource_index < raw_n {
            let descriptor_index = resource_index;
            let p = unsafe { raw_desc.add(descriptor_index * pci::resource_manager::CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE) };
            let kind = unsafe { *p };
            let start = unsafe { core::ptr::read_unaligned(p.add(4).cast::<u64>()) };
            let length = unsafe { core::ptr::read_unaligned(p.add(12).cast::<u32>()) } as u64;
            resource_index += 1;
            let flags = unsafe { core::ptr::read_unaligned(p.add(2).cast::<u16>()) };
            let is_io = req.kind == pci::resources::BarKind::Io;
            let resource_kind_matches = (is_io && kind == 1) || (!is_io && kind == 3);
            let prefetch_matches = is_io || ((flags & 0x0004 != 0) == req.prefetchable);
            if resource_kind_matches && prefetch_matches {
                let end = match start.checked_add(length) { Some(v) => v, None => return STATUS_DEVICE_CONFIGURATION_ERROR };
                if length < req.length || req.alignment == 0 || start % req.alignment != 0
                    || (is_io && end > 0x1_0000)
                    || (req.kind == pci::resources::BarKind::Memory32 && end > (1u64 << 32))
                    || !assigned_range_fits_clipped_host_aperture(
                        window_index, req.kind, req.prefetchable, start, length,
                    ) {
                    return STATUS_DEVICE_CONFIGURATION_ERROR;
                }
                let translated_p = unsafe { trans_desc.add(descriptor_index * pci::resource_manager::CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE) };
                let translated_start = unsafe { core::ptr::read_unaligned(translated_p.add(4).cast::<u64>()) };
                let translated_length = unsafe { core::ptr::read_unaligned(translated_p.add(12).cast::<u32>()) } as u64;
                let translated_end = match translated_start.checked_add(translated_length) { Some(v) => v, None => return STATUS_DEVICE_CONFIGURATION_ERROR };
                if translated_length < req.length || translated_start % req.alignment != 0
                    || (is_io && translated_end > 0x1_0000)
                    || (req.kind == pci::resources::BarKind::Memory32 && translated_end > (1u64 << 32)) {
                    return STATUS_DEVICE_CONFIGURATION_ERROR;
                }
                found = Some(start);
                translated_base = Some(translated_start);
                break;
            }
        }
        let Some(base) = found else { return STATUS_DEVICE_CONFIGURATION_ERROR; };
        let Some(translated_base) = translated_base else { return STATUS_DEVICE_CONFIGURATION_ERROR; };
        plan.assignments[plan.count] = Some(pci::resource_manager::BarAssignment {
            index: req.index, kind: req.kind, base, length: req.length, prefetchable: req.prefetchable,
        });
        translated_plan.assignments[translated_plan.count] = Some(pci::resource_manager::BarAssignment {
            index: req.index, kind: req.kind, base: translated_base,
            length: req.length, prefetchable: req.prefetchable,
        });
        plan.count += 1;
        translated_plan.count += 1;
    }
    // Decode IDE command/control and bus-master BARs from the exact resources
    // accepted by PnP. Native-mode channels never fall back to legacy ports.
    let ide_ports = if function.header.is_ide_controller() {
        match pci::ide::resolve_ide_ports(function.header.programming_interface, &translated_plan) {
            Ok(ports) => Some(ports),
            Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
        }
    } else { None };
    // Require translated descriptors to be structurally present before touching
    // hardware. The OS has already performed address translation/arbitration.
    for i in 0..trans_n {
        let offset=i*pci::resource_manager::CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE;
        let raw_p=unsafe{raw_desc.add(offset)};
        let trans_p=unsafe{trans_desc.add(offset)};
        let raw_kind=unsafe{*raw_p};
        let trans_kind=unsafe{*trans_p};
        // Windows' raw and translated resource lists preserve descriptor order
        // and type. Reject mismatched pairs rather than pairing a BAR with an
        // unrelated translated address.
        let raw_flags=unsafe{core::ptr::read_unaligned(raw_p.add(2).cast::<u16>())};
        let trans_flags=unsafe{core::ptr::read_unaligned(trans_p.add(2).cast::<u16>())};
        if raw_kind!=trans_kind||raw_flags!=trans_flags{return STATUS_DEVICE_CONFIGURATION_ERROR;}
        if raw_kind==1||raw_kind==3 {
            let raw_start=unsafe{core::ptr::read_unaligned(raw_p.add(4).cast::<u64>())};
            let trans_start=unsafe{core::ptr::read_unaligned(trans_p.add(4).cast::<u64>())};
            let raw_len=unsafe{core::ptr::read_unaligned(raw_p.add(12).cast::<u32>())};
            let trans_len=unsafe{core::ptr::read_unaligned(trans_p.add(12).cast::<u32>())};
            if raw_len==0||raw_len!=trans_len
                ||raw_start.checked_add(raw_len as u64).is_none()
                ||trans_start.checked_add(trans_len as u64).is_none(){
                return STATUS_DEVICE_CONFIGURATION_ERROR;
            }
        }
    }
    let list_size = 20usize.checked_add(raw_n.checked_mul(pci::resource_manager::CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE).unwrap_or(usize::MAX)).unwrap_or(usize::MAX);
    if list_size < 20 || list_size > 20 + 64 * pci::resource_manager::CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    let raw_copy = unsafe { ExAllocatePoolWithTag(1, list_size, PCI_REQUIREMENTS_POOL_TAG) };
    let translated_copy = unsafe { ExAllocatePoolWithTag(1, list_size, PCI_REQUIREMENTS_POOL_TAG) };
    if raw_copy.is_null() || translated_copy.is_null() {
        if !raw_copy.is_null() { unsafe { ExFreePoolWithTag(raw_copy, PCI_REQUIREMENTS_POOL_TAG); } }
        if !translated_copy.is_null() { unsafe { ExFreePoolWithTag(translated_copy, PCI_REQUIREMENTS_POOL_TAG); } }
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(raw, raw_copy.cast::<u8>(), list_size);
        core::ptr::copy_nonoverlapping(translated, translated_copy.cast::<u8>(), list_size);
    }
    if pci::program_assignments_exclusive(&mut config, function.header.header_type, &plan).is_err() {
        unsafe {
            ExFreePoolWithTag(raw_copy, PCI_REQUIREMENTS_POOL_TAG);
            ExFreePoolWithTag(translated_copy, PCI_REQUIREMENTS_POOL_TAG);
        }
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    let interrupt_status = unsafe { configure_pci_intx_masked(extension, function, window) };
    if interrupt_status != STATUS_SUCCESS {
        let _ = unsafe { release_pci_interrupt(extension) };
        unsafe {
            ExFreePoolWithTag(raw_copy, PCI_REQUIREMENTS_POOL_TAG);
            ExFreePoolWithTag(translated_copy, PCI_REQUIREMENTS_POOL_TAG);
        }
        return interrupt_status;
    }
    unsafe {
        if !(*extension).raw_resources.is_null() { ExFreePoolWithTag((*extension).raw_resources, PCI_REQUIREMENTS_POOL_TAG); }
        if !(*extension).translated_resources.is_null() { ExFreePoolWithTag((*extension).translated_resources, PCI_REQUIREMENTS_POOL_TAG); }
        (*extension).raw_resources = raw_copy;
        (*extension).raw_resources_size = list_size;
        (*extension).translated_resources = translated_copy;
        (*extension).translated_resources_size = list_size;
        (*extension).started = 1;
        if let Some(ports) = ide_ports {
            (*extension).ide_ports = ports;
            (*extension).ide_ports_valid = 1;
        }
    }
    STATUS_SUCCESS
}

unsafe extern "win64" fn query_pdo_ide_resources(
    context:PVOID,
    output:*mut cognacos_nt_abi::bus::PCI_IDE_RESOURCES,
)->i32{
    if context.is_null()||output.is_null(){return STATUS_INVALID_PARAMETER}
    let ext=context.cast::<PciPdoExtension>();
    if unsafe{(*ext).magic!=PCI_PDO_EXTENSION_MAGIC||(*ext).version!=1}{return STATUS_INVALID_PARAMETER}
    if unsafe{(*ext).started==0||(*ext).ide_ports_valid==0}{return STATUS_DEVICE_NOT_READY}
    let ports=unsafe{(*ext).ide_ports};
    let result=cognacos_nt_abi::bus::PCI_IDE_RESOURCES{
        primary:cognacos_nt_abi::bus::PCI_IDE_CHANNEL_RESOURCES{
            command_base:ports.primary.command_base,control_base:ports.primary.control_base,
            flags:(ports.primary.native as u32)|((ports.primary.switchable as u32)<<1),
        },
        secondary:cognacos_nt_abi::bus::PCI_IDE_CHANNEL_RESOURCES{
            command_base:ports.secondary.command_base,control_base:ports.secondary.control_base,
            flags:(ports.secondary.native as u32)|((ports.secondary.switchable as u32)<<1),
        },
        bus_master_base:ports.bus_master_base.unwrap_or(0),
        flags:(ports.bus_master_capable as u16)|((ports.bus_master_base.is_some() as u16)<<1),
    };
    unsafe{output.write(result)}
    STATUS_SUCCESS
}

unsafe extern "win64" fn pnp_dispatch(device: *mut cognacos_nt_abi::io::DEVICE_OBJECT, irp: *mut cognacos_nt_abi::io::IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut cognacos_nt_abi::io::IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { (*irp).io_status.information = 0; }
    let status = match unsafe { (*stack).minor_function } {
                IRP_MN_STOP_DEVICE | IRP_MN_REMOVE_DEVICE => {
            if device.is_null() { STATUS_INVALID_PARAMETER } else {
                let ext = unsafe { (*device).device_extension.cast::<PciPdoExtension>() };
                if ext.is_null() || unsafe { (*ext).magic != PCI_PDO_EXTENSION_MAGIC } {
                    STATUS_INVALID_PARAMETER
                } else {
                    let teardown = unsafe { release_pci_interrupt(ext) };
                    if teardown != STATUS_SUCCESS { teardown } else {
                        unsafe {
                            if !(*ext).raw_resources.is_null() { ExFreePoolWithTag((*ext).raw_resources, PCI_REQUIREMENTS_POOL_TAG); }
                            if !(*ext).translated_resources.is_null() { ExFreePoolWithTag((*ext).translated_resources, PCI_REQUIREMENTS_POOL_TAG); }
                            (*ext).raw_resources = core::ptr::null_mut();
                            (*ext).translated_resources = core::ptr::null_mut();
                            (*ext).raw_resources_size = 0;
                            (*ext).translated_resources_size = 0;
                            (*ext).started = 0;
                        }
                        STATUS_SUCCESS
                    }
                }
            }
        },
        IRP_MN_QUERY_DEVICE_RELATIONS => {
            let relation_type = unsafe {
                core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())
            };
            if relation_type != BUS_RELATIONS {
                STATUS_NOT_SUPPORTED
            } else {
                match unsafe { query_bus_relations() } {
                    Ok(relations) => {
                        unsafe { (*irp).io_status.information = relations as usize; }
                        STATUS_SUCCESS
                    }
                    Err(status) => status,
                }
            }
        },
        IRP_MN_QUERY_RESOURCE_REQUIREMENTS => {
            if device.is_null() {
                STATUS_INVALID_PARAMETER
            } else {
                let extension = unsafe { (*device).device_extension.cast::<PciPdoExtension>() };
                if extension.is_null()
                    || unsafe { (*extension).magic != PCI_PDO_EXTENSION_MAGIC || (*extension).version != 1 } {
                    STATUS_INVALID_PARAMETER
                } else {
                    let bytes = unsafe { ExAllocatePoolWithTag(1, IO_RESOURCE_REQUIREMENTS_MAX_SIZE, PCI_REQUIREMENTS_POOL_TAG) };
                    if bytes.is_null() {
                        STATUS_INSUFFICIENT_RESOURCES
                    } else {
                        let status = unsafe {
                            CognacPciQueryResourceRequirements(
                                HANDOFF.load(Ordering::Acquire) as PVOID,
                                (*extension).function_index,
                                bytes.cast::<u8>(),
                                IO_RESOURCE_REQUIREMENTS_MAX_SIZE,
                                &mut (*irp).io_status.information,
                            )
                        };
                        if status == STATUS_SUCCESS {
                            STATUS_SUCCESS
                        } else {
                            // Pool ownership remains local unless a complete
                            // requirements list is returned to the PnP manager.
                            unsafe { ExFreePoolWithTag(bytes, PCI_REQUIREMENTS_POOL_TAG); }
                            status
                        }
                    }
                }
            }
        },
        IRP_MN_QUERY_INTERFACE => {
            let parameters = unsafe { &(*stack).parameters };
            let guid = unsafe { core::ptr::read_unaligned(parameters.as_ptr().cast::<*const cognacos_nt_abi::bus::GUID>()) };
            let size = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(8).cast::<u16>()) };
            let version = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(10).cast::<u16>()) };
            let output = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(16).cast::<*mut PCI_BUS_INTERFACE>()) };
            if guid.is_null() || output.is_null() {
                STATUS_INVALID_PARAMETER
            } else if unsafe { *guid } == GUID_BUS_INTERFACE_STANDARD {
                unsafe { query_standard_bus_interface(device,version,size,output.cast()) }
            } else if unsafe { *guid } == cognacos_nt_abi::bus::GUID_PCI_IDE_RESOURCE_INTERFACE {
                if version as u32 != cognacos_nt_abi::bus::PCI_IDE_RESOURCE_INTERFACE_VERSION
                    || (size as usize) < core::mem::size_of::<cognacos_nt_abi::bus::PCI_IDE_RESOURCE_INTERFACE>()
                    || device.is_null() {
                    STATUS_NOT_SUPPORTED
                } else {
                    let extension=unsafe{(*device).device_extension.cast::<PciPdoExtension>()};
                    if extension.is_null()||unsafe{(*extension).magic!=PCI_PDO_EXTENSION_MAGIC||(*extension).version!=1} {
                        STATUS_NOT_SUPPORTED
                    } else {
                        let interface=cognacos_nt_abi::bus::PCI_IDE_RESOURCE_INTERFACE{
                            header:cognacos_nt_abi::bus::INTERFACE_HEADER{
                                size:core::mem::size_of::<cognacos_nt_abi::bus::PCI_IDE_RESOURCE_INTERFACE>() as u16,
                                version:cognacos_nt_abi::bus::PCI_IDE_RESOURCE_INTERFACE_VERSION as u16,
                                context:extension.cast(),interface_reference:None,interface_dereference:None,
                            },
                            query_resources:Some(query_pdo_ide_resources),
                        };
                        unsafe{core::ptr::write_unaligned(output.cast::<cognacos_nt_abi::bus::PCI_IDE_RESOURCE_INTERFACE>(),interface);}
                        STATUS_SUCCESS
                    }
                }
            } else if unsafe { *guid } == cognacos_nt_abi::bus::GUID_PCI_INTERRUPT_INTERFACE {
                if version as u32 != PCI_INTERRUPT_INTERFACE_VERSION
                    || (size as usize) < core::mem::size_of::<PCI_INTERRUPT_INTERFACE>()
                    || device.is_null() {
                    STATUS_NOT_SUPPORTED
                } else {
                    let extension = unsafe { (*device).device_extension.cast::<PciPdoExtension>() };
                    if extension.is_null()
                        || unsafe {
                            (*extension).magic != PCI_PDO_EXTENSION_MAGIC
                                || (*extension).version != 1
                                || (*extension).started == 0
                                || (*extension).interrupt_allocated == 0
                        } {
                        STATUS_DEVICE_NOT_READY
                    } else {
                        let interface = PCI_INTERRUPT_INTERFACE {
                            header: INTERFACE_HEADER {
                                size: core::mem::size_of::<PCI_INTERRUPT_INTERFACE>() as u16,
                                version: PCI_INTERRUPT_INTERFACE_VERSION as u16,
                                context: device.cast(),
                                interface_reference: Some(pci_bus_interface_reference),
                                interface_dereference: Some(pci_bus_interface_dereference),
                            },
                            register: Some(pci_pdo_interrupt_register),
                            enable: Some(pci_pdo_interrupt_enable),
                            disable: Some(pci_pdo_interrupt_disable),
                            unregister: Some(pci_pdo_interrupt_unregister),
                        };
                        unsafe {
                            pci_bus_interface_reference(device.cast());
                            core::ptr::write_unaligned(output.cast::<PCI_INTERRUPT_INTERFACE>(), interface);
                        }
                        STATUS_SUCCESS
                    }
                }
            } else if unsafe { *guid } == cognacos_nt_abi::bus::GUID_PCI_DEVICE_INTERFACE {
                if version as u32 != cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION
                    || (size as usize) < core::mem::size_of::<cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE>()
                    || device.is_null() {
                    STATUS_NOT_SUPPORTED
                } else {
                    let extension = unsafe { (*device).device_extension.cast::<PciPdoExtension>() };
                    if extension.is_null() || unsafe { (*extension).magic != PCI_PDO_EXTENSION_MAGIC || (*extension).version != 1 } {
                        STATUS_NOT_SUPPORTED
                    } else {
                        let interface = cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE {
                            header: cognacos_nt_abi::bus::INTERFACE_HEADER {
                                size: core::mem::size_of::<cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE>() as u16,
                                version: cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION as u16,
                                context: extension.cast(),
                                interface_reference: None,
                                interface_dereference: None,
                            },
                            query_information: Some(query_pdo_information),
                        };
                        unsafe { core::ptr::write_unaligned(output.cast::<cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE>(), interface); }
                        STATUS_SUCCESS
                    }
                }
            } else if unsafe { *guid } != cognacos_nt_abi::bus::GUID_PCI_BUS_INTERFACE
                || version as u32 != PCI_BUS_INTERFACE_VERSION
                || (size as usize) < core::mem::size_of::<PCI_BUS_INTERFACE>() {
                STATUS_NOT_SUPPORTED
            } else {
                unsafe { CognacPciGetBusInterface(output) }
            }
        },
        IRP_MN_START_DEVICE => unsafe { start_pci_device(device, irp, stack) },
        _ => STATUS_INVALID_DEVICE_REQUEST,
    };
    unsafe { (*irp).io_status.status.status = status; }
    status
}
/// Build a stable BusRelations result. PDOs are materialized through the
/// documented IoCreateDevice export and deleted through IoDeleteDevice.
/// A failed creation rolls back only PDOs created by this invocation.
unsafe fn query_bus_relations() -> Result<*const PciDeviceRelations, i32> {
    if !STATE.ready.load(Ordering::Acquire) { return Err(STATUS_DEVICE_NOT_READY); }
    if STATE.host_bridge_resource_count.load(Ordering::Acquire) == 0 {
        if HOST_BRIDGE_BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            return Err(STATUS_DEVICE_BUSY);
        }
        let status = if STATE.host_bridge_resource_count.load(Ordering::Acquire) == 0 {
            unsafe { acquire_acpi_host_bridge_resources() }
        } else { STATUS_SUCCESS };
        HOST_BRIDGE_BUSY.store(false, Ordering::Release);
        if status != STATUS_SUCCESS { return Err(status); }
    }
    if unsafe { CognacPciEnumerateInventory() } != STATUS_SUCCESS {
        return Err(STATUS_DEVICE_CONFIGURATION_ERROR);
    }
    if PDO_BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        return Err(STATUS_DEVICE_BUSY);
    }
    let result = (|| {
        let address = HANDOFF.load(Ordering::Acquire);
        if address == 0 { return Err(STATUS_DEVICE_NOT_READY); }
        let handoff = unsafe { &*(address as *const PciDriverHandoff) };
        let driver = DRIVER_OBJECT.load(Ordering::Acquire) as *mut DRIVER_OBJECT;
        if driver.is_null() { return Err(STATUS_DEVICE_NOT_READY); }
        let total = STATE.function_count.load(Ordering::Acquire);
        if total > MAX_PCI_PDOS { return Err(STATUS_INSUFFICIENT_RESOURCES); }

        let mut created = [core::ptr::null_mut::<DEVICE_OBJECT>(); MAX_PCI_PDOS];
        let mut created_count = 0usize;
        for index in 0..total {
            if !unsafe { (*STATE.pdos.get())[index] }.is_null() { continue; }
            let mut information = PCI_DEVICE_INFORMATION::default();
            let status = unsafe {
                query_inventory_function(handoff as *const _ as PVOID, index as u32, &mut information)
            };
            if status != STATUS_SUCCESS {
                for pdo in created[..created_count].iter().copied() {
                    if !pdo.is_null() { unsafe { delete_pdo(pdo); } }
                }
                return Err(status);
            }
            let mut pdo = core::ptr::null_mut();
            let status = unsafe {
                IoCreateDevice(
                    driver,
                    core::mem::size_of::<PciPdoExtension>() as u32,
                    core::ptr::null_mut(),
                    0x22,
                    0,
                    0,
                    &mut pdo,
                )
            };
            if status != STATUS_SUCCESS || pdo.is_null() {
                if !pdo.is_null() { unsafe { delete_pdo(pdo); } }
                for created_pdo in created[..created_count].iter().copied() {
                    if !created_pdo.is_null() { unsafe { delete_pdo(created_pdo); } }
                }
                return Err(if status == STATUS_SUCCESS { STATUS_DEVICE_CONFIGURATION_ERROR } else { status });
            }
            let extension = unsafe { (*pdo).device_extension.cast::<PciPdoExtension>() };
            if extension.is_null() {
                unsafe { delete_pdo(pdo); }
                for created_pdo in created[..created_count].iter().copied() {
                    if !created_pdo.is_null() { unsafe { delete_pdo(created_pdo); } }
                }
                return Err(STATUS_INSUFFICIENT_RESOURCES);
            }
            unsafe {
                extension.write(PciPdoExtension {
                    magic: PCI_PDO_EXTENSION_MAGIC,
                    version: 1,
                    function_index: index as u32,
                    device_object: pdo,
                    started: 0,
                    raw_resources: core::ptr::null_mut(),
                    raw_resources_size: 0,
                    translated_resources: core::ptr::null_mut(),
                    translated_resources_size: 0,
                    information,
                    ide_ports: pci::ide::IdeControllerPorts::default(),
                    ide_ports_valid: 0,
                    interrupt_vector: 0,
                    interrupt_allocated: 0,
                    interrupt_registered: 0,
                    interrupt_reserved: 0,
                    interrupt_context: 0,
                    interrupt_hal: core::mem::zeroed(),
                });
                // DO_BUS_ENUMERATED_DEVICE: this is a PDO created by its bus.
                (*pdo).flags |= 0x0000_0200;
            }
            created[created_count] = pdo;
            created_count += 1;
        }

        let mut next = 0usize;
        for index in 0..total {
            if !unsafe { (*STATE.pdos.get())[index] }.is_null() { continue; }
            if next >= created_count { return Err(STATUS_DEVICE_CONFIGURATION_ERROR); }
            unsafe { (*STATE.pdos.get())[index] = created[next]; }
            next += 1;
        }
        STATE.pdo_count.store(total, Ordering::Release);
        let relations = unsafe { &mut *RELATIONS.0.get() };
        relations.objects.fill(core::ptr::null_mut());
        for index in 0..total {
            relations.objects[index] = unsafe { (*STATE.pdos.get())[index] };
            if relations.objects[index].is_null() { return Err(STATUS_DEVICE_CONFIGURATION_ERROR); }
        }
        relations.count = total as u32;
        Ok(relations as *const PciDeviceRelations)
    })();
    PDO_BUSY.store(false, Ordering::Release);
    result
}

unsafe extern "win64" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    STATE.ready.store(false, Ordering::Release);
    let count = STATE.pdo_count.swap(0, Ordering::AcqRel).min(MAX_PCI_PDOS);
    for index in 0..count {
        let pdo = unsafe { (*STATE.pdos.get())[index] };
        if !pdo.is_null() {
            unsafe { delete_pdo(pdo); }
            unsafe { (*STATE.pdos.get())[index] = core::ptr::null_mut(); }
        }
    }
    STATE.host_bridge_resource_count.store(0, Ordering::Release);
    unsafe {
        (*STATE.protected_memory.get()) = MM_PROTECTED_MEMORY_SNAPSHOT::empty();
        (*STATE.clipped_host_apertures.get()) = [pci::host_bridge::PreparedHostBridgeApertures::EMPTY; MAX_WINDOWS];
        (*STATE.host_bridge_resources.get()) = [const { PciHostBridgeResourceSet::empty(0, 0, 0) }; MAX_WINDOWS];
    }
    HANDOFF.store(0, Ordering::Release);
    DRIVER_OBJECT.store(0, Ordering::Release);
}

/// Read-only PCI bus discovery. BAR programming, bus mastering, MMIO mapping,
/// resource arbitration, and child PDO publication are intentionally deferred
/// to the PnP manager after it has assigned resources.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    let object = unsafe { &mut *(driver as *mut DRIVER_OBJECT) };
    if object.driver_extension.is_null() { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(object.driver_extension as *const PciDriverHandoff) };
    if handoff.magic != HANDOFF_MAGIC || handoff.version != HANDOFF_VERSION
        || (handoff.size as usize) < core::mem::size_of::<PciDriverHandoff>()
        || handoff.window_count == 0 || handoff.window_count as usize > MAX_WINDOWS {
        return STATUS_INVALID_PARAMETER;
    }

    for i in 0..handoff.window_count as usize {
        let window = &handoff.windows[i];
        if window.validate().is_err() { return STATUS_INVALID_PARAMETER; }
        for prior in &handoff.windows[..i] {
            if prior.segment == window.segment
                && prior.bus_start <= window.bus_end && window.bus_start <= prior.bus_end {
                return STATUS_INVALID_PARAMETER;
            }
        }
    }
    HANDOFF.store(handoff as *const _ as u64, Ordering::Release);
    DRIVER_OBJECT.store(driver as u64, Ordering::Release);
    object.major_function[IRP_MJ_PNP] = Some(pnp_dispatch);
    object.driver_unload = Some(driver_unload);
    STATE.ready.store(true, Ordering::Release);
    STATUS_SUCCESS
}

unsafe extern "win64" fn read_config(
    context: PVOID, segment: u16, bus: u8, device: u8, function: u8,
    offset: u16, width: u8, value: *mut u32,
) -> i32 {
    if context.is_null() || value.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if context as u64 != HANDOFF.load(Ordering::Acquire) { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(context as *const PciDriverHandoff) };
    let Some(window) = handoff.windows[..handoff.window_count as usize]
        .iter().find(|w| w.segment == segment && bus >= w.bus_start && bus <= w.bus_end) else {
        return STATUS_INVALID_PARAMETER;
    };
    let result = match width {
        1 => unsafe { window.read_u8(segment, bus, device, function, offset) }.map(u32::from),
        2 => unsafe { window.read_u16(segment, bus, device, function, offset) }.map(u32::from),
        4 => unsafe { window.read_u32(segment, bus, device, function, offset) },
        _ => return STATUS_INVALID_PARAMETER,
    };
    match result {
        Ok(v) => { unsafe { value.write(v); } STATUS_SUCCESS }
        Err(error) => pci::error_to_ntstatus(error),
    }
}

unsafe extern "win64" fn write_config(
    context: PVOID, segment: u16, bus: u8, device: u8, function: u8,
    offset: u16, width: u8, value: u32,
) -> i32 {
    if context.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if context as u64 != HANDOFF.load(Ordering::Acquire) { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(context as *const PciDriverHandoff) };
    let Some(window) = handoff.windows[..handoff.window_count as usize]
        .iter().find(|w| w.segment == segment && bus >= w.bus_start && bus <= w.bus_end) else {
        return STATUS_INVALID_PARAMETER;
    };
    let result = match width {
        1 => unsafe { window.write_u8(segment, bus, device, function, offset, value as u8) },
        2 => unsafe { window.write_u16(segment, bus, device, function, offset, value as u16) },
        4 => unsafe { window.write_u32(segment, bus, device, function, offset, value) },
        _ => Err(pci::PciError::InvalidOffset),
    };
    match result {
        Ok(()) => STATUS_SUCCESS,
        Err(error) => pci::error_to_ntstatus(error),
    }
}

unsafe extern "win64" fn query_inventory_counts(
    context: PVOID, functions: *mut u32, bridges: *mut u32, xhci: *mut u32,
) -> i32 {
    if context.is_null() || functions.is_null() || bridges.is_null() || xhci.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if context as u64 != HANDOFF.load(Ordering::Acquire) { return STATUS_INVALID_PARAMETER; }
    // The first PnP inventory request is the discovery trigger. DriverEntry
    // validates the firmware-provided ECAM windows only; PCI enumeration is
    // deliberately deferred until the PnP manager asks for bus inventory.
    if !STATE.inventory_ready.load(Ordering::Acquire) {
        let status = unsafe { CognacPciEnumerateInventory() };
        if status != STATUS_SUCCESS { return status; }
    }
    let Ok(function_count) = u32::try_from(STATE.function_count.load(Ordering::Acquire)) else {
        return STATUS_INSUFFICIENT_RESOURCES;
    };
    let Ok(bridge_count) = u32::try_from(STATE.bridge_count.load(Ordering::Acquire)) else {
        return STATUS_INSUFFICIENT_RESOURCES;
    };
    let Ok(xhci_count) = u32::try_from(STATE.xhci_count.load(Ordering::Acquire)) else {
        return STATUS_INSUFFICIENT_RESOURCES;
    };
    unsafe { functions.write(function_count); bridges.write(bridge_count); xhci.write(xhci_count); }
    STATUS_SUCCESS
}


/// Query the resource requirements for one enumerated PCI function into
/// caller-owned storage. This is the allocation-safe primitive used by the
/// PnP IRP adapter: it returns the exact encoded byte count and never returns
/// a pointer to static storage. Call only before the function has been started.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciQueryResourceRequirements(
    context: PVOID,
    index: u32,
    output: *mut u8,
    output_capacity: usize,
    bytes_written: *mut usize,
) -> i32 {
    if context.is_null() || output.is_null() || bytes_written.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { bytes_written.write(0); }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if context as u64 != HANDOFF.load(Ordering::Acquire) { return STATUS_INVALID_PARAMETER; }
    if !STATE.inventory_ready.load(Ordering::Acquire) {
        let status = unsafe { CognacPciEnumerateInventory() };
        if status != STATUS_SUCCESS { return status; }
    }
    let handoff = unsafe { &*(context as *const PciDriverHandoff) };
    let mut remaining = index as usize;
    let mut selected = None;
    for window_index in 0..handoff.window_count as usize {
        let count = unsafe { (*STATE.function_counts.get())[window_index] };
        if remaining >= count { remaining -= count; continue; }
        selected = unsafe { (*STATE.functions.get())[window_index][remaining] };
        break;
    }
    let Some(function) = selected else { return STATUS_NOT_FOUND; };
    let Some((window_index, window)) = handoff.windows[..handoff.window_count as usize]
        .iter().enumerate().find(|(_, w)| w.segment == function.segment
            && function.bus >= w.bus_start && function.bus <= w.bus_end) else {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    };
    let host_count = STATE.host_bridge_resource_count.load(Ordering::Acquire);
    if window_index >= host_count || window_index >= MAX_WINDOWS {
        return STATUS_DEVICE_NOT_READY;
    }
    let clipped = unsafe { (*STATE.clipped_host_apertures.get())[window_index] };
    if clipped.segment != function.segment || function.bus < clipped.bus_start
        || function.bus > clipped.bus_end {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    let mut config = pci::EcamFunctionConfig {
        window,
        segment: function.segment,
        bus: function.bus,
        device: function.device,
        function: function.function,
    };
    let result = unsafe {
        pci::resource_manager::probe_and_encode_requirements_in_apertures(
            &mut config,
            function.header.header_type,
            function.bus as u32,
            clipped.as_slice(),
            core::slice::from_raw_parts_mut(output, output_capacity),
        )
    };
    match result {
        Ok(size) => {
            unsafe { bytes_written.write(size); }
            STATUS_SUCCESS
        }
        Err(pci::resources::ResourceError::Capacity) => STATUS_INSUFFICIENT_RESOURCES,
        Err(pci::resources::ResourceError::Config) => STATUS_DEVICE_CONFIGURATION_ERROR,
        Err(_) => STATUS_NOT_SUPPORTED,
    }
}

unsafe extern "win64" fn query_inventory_function(
    context: PVOID, index: u32, output: *mut cognacos_nt_abi::bus::PCI_DEVICE_INFORMATION,
) -> i32 {
    if context.is_null() || output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.inventory_ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if context as u64 != HANDOFF.load(Ordering::Acquire) { return STATUS_INVALID_PARAMETER; }
    let mut remaining = index as usize;
    let handoff = unsafe { &*(context as *const PciDriverHandoff) };
    for window in 0..handoff.window_count as usize {
        let count = unsafe { (*STATE.function_counts.get())[window] };
        if remaining >= count { remaining -= count; continue; }
        let Some(function) = (unsafe { (*STATE.functions.get())[window][remaining] }) else {
            return STATUS_DEVICE_CONFIGURATION_ERROR;
        };
        let h = function.header;
        unsafe {
            output.write(cognacos_nt_abi::bus::PCI_DEVICE_INFORMATION {
                segment: function.segment, bus: function.bus, device: function.device,
                function: function.function, header_type: h.header_type,
                revision_id: h.revision_id, programming_interface: h.programming_interface,
                subclass: h.subclass, class_code: h.class_code, reserved: 0,
                vendor_id: h.vendor_id, device_id: h.device_id, command: h.command,
            });
        }
        return STATUS_SUCCESS;
    }
    STATUS_NOT_FOUND
}

/// Return PCI.sys' versioned configuration-space interface. The interface
/// exposes only checked ECAM access; resource assignment remains a PnP duty.
/// Return the firmware-programmed I/O, non-prefetchable and prefetchable
/// windows for an enumerated bridge. No window is synthesized from ECAM.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciQueryBridgeWindows(index:u32,output:*mut pci::bridge_resources::BridgeWindows)->i32{
    if output.is_null(){return STATUS_INVALID_PARAMETER}
    if !STATE.inventory_ready.load(Ordering::Acquire){return STATUS_DEVICE_NOT_READY}
    let handoff=unsafe{&*(HANDOFF.load(Ordering::Acquire) as *const PciDriverHandoff)};
    let mut remaining=index as usize;
    for wi in 0..handoff.window_count as usize{
        let n=unsafe{(*STATE.bridge_counts.get())[wi]};
        if remaining>=n{remaining-=n;continue}
        let Some(w)= (unsafe{(*STATE.bridge_resource_windows.get())[wi][remaining]}) else{return STATUS_DEVICE_CONFIGURATION_ERROR};
        unsafe{output.write(w)}
        return STATUS_SUCCESS
    }
    STATUS_NOT_FOUND
}

unsafe extern "win64" fn query_inventory_bridge(
    context: PVOID, index: u32, output: *mut cognacos_nt_abi::bus::PCI_BRIDGE_INFORMATION,
) -> i32 {
    if context.is_null() || output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.inventory_ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if context as u64 != HANDOFF.load(Ordering::Acquire) { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(context as *const PciDriverHandoff) };
    let mut remaining = index as usize;
    for window in 0..handoff.window_count as usize {
        let count = unsafe { (*STATE.bridge_counts.get())[window] };
        if remaining >= count { remaining -= count; continue; }
        let Some(bridge) = (unsafe { (*STATE.bridges.get())[window][remaining] }) else {
            return STATUS_DEVICE_CONFIGURATION_ERROR;
        };
        unsafe {
            output.write(cognacos_nt_abi::bus::PCI_BRIDGE_INFORMATION {
                segment: bridge.segment, primary_bus: bridge.primary_bus,
                secondary_bus: bridge.secondary_bus, subordinate_bus: bridge.subordinate_bus,
                device: bridge.device, function: bridge.function, reserved: 0,
            });
        }
        return STATUS_SUCCESS;
    }
    STATUS_NOT_FOUND
}


/// Return the read-only xHCI inventory entry, including the firmware-assigned
/// BAR and legacy interrupt identity. BAR sizing, decode changes and MMIO
/// mapping remain the kernel's resource-arbitration responsibilities.
unsafe extern "win64" fn query_inventory_xhci(
    context: PVOID, index: u32, output: *mut PCI_XHCI_INFORMATION,
) -> i32 {
    if context.is_null() || output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.inventory_ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if context as u64 != HANDOFF.load(Ordering::Acquire) { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(context as *const PciDriverHandoff) };
    let mut remaining = index as usize;
    for window_index in 0..handoff.window_count as usize {
        let count = unsafe { (*STATE.xhci_counts.get())[window_index] };
        if remaining >= count { remaining -= count; continue; }
        let Some(function) = (unsafe { (*STATE.xhci.get())[window_index][remaining] }) else {
            return STATUS_DEVICE_CONFIGURATION_ERROR;
        };
        let window = &handoff.windows[window_index];
        let interrupt_line = match unsafe {
            window.read_u8(function.segment, function.bus, function.device, function.function, 0x3c)
        } { Ok(v) => v, Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR };
        let interrupt_pin = match unsafe {
            window.read_u8(function.segment, function.bus, function.device, function.function, 0x3d)
        } { Ok(v) => v, Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR };
        unsafe {
            output.write(PCI_XHCI_INFORMATION {
                segment: function.segment,
                root_bus: window.bus_start,
                bus: function.bus,
                device: function.device,
                function: function.function,
                vendor_id: function.vendor_id,
                device_id: function.device_id,
                command: function.command,
                interrupt_line,
                interrupt_pin,
                bar_is_64bit: function.bar.is_64_bit as u8,
                bar_prefetchable: function.bar.prefetchable as u8,
                reserved: 0,
                bar_physical: function.bar.base,
            });
        }
        return STATUS_SUCCESS;
    }
    STATUS_NOT_FOUND
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciGetBusInterface(
    output: *mut PCI_BUS_INTERFACE,
) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    let context = HANDOFF.load(Ordering::Acquire) as PVOID;
    if context.is_null() { return STATUS_DEVICE_NOT_READY; }
    unsafe {
        output.write(PCI_BUS_INTERFACE {
            header: INTERFACE_HEADER {
                size: core::mem::size_of::<PCI_BUS_INTERFACE>() as u16,
                version: PCI_BUS_INTERFACE_VERSION as u16,
                context,
                interface_reference: None,
                interface_dereference: None,
            },
            read_config: Some(read_config),
            write_config: Some(write_config),
            query_counts: Some(query_inventory_counts),
            query_function: Some(query_inventory_function),
            query_bridge: Some(query_inventory_bridge),
            query_xhci: Some(query_inventory_xhci),
        });
    }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub extern "win64" fn CognacPciBridgeCount() -> usize {
    if STATE.inventory_ready.load(Ordering::Acquire) { STATE.bridge_count.load(Ordering::Acquire) } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "win64" fn CognacPciXhciCount() -> usize {
    if STATE.inventory_ready.load(Ordering::Acquire) { STATE.xhci_count.load(Ordering::Acquire) } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "win64" fn CognacPciFunctionCount() -> usize {
    if STATE.inventory_ready.load(Ordering::Acquire) { STATE.function_count.load(Ordering::Acquire) } else { 0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciFindAhci(output: *mut pci::PciFunction) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.inventory_ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    for window in 0..MAX_WINDOWS {
        let count = unsafe { (*STATE.function_counts.get())[window] };
        let functions = unsafe { &(*STATE.functions.get())[window] };
        for function in functions.iter().take(count).flatten() {
            if function.header.is_ahci() { unsafe { output.write(*function); } return STATUS_SUCCESS; }
        }
    }
    STATUS_NOT_FOUND
}
/// Locate the first PCI IDE-class controller (base class 01h, subclass
/// 01h). This is inventory only: it does not size BARs, change decode bits,
/// enable bus mastering, or claim resources. The PnP start-device resource
/// transaction remains the only authority for activating the controller.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciFindIde(output: *mut cognacos_nt_abi::bus::PCI_DEVICE_INFORMATION) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.inventory_ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    for window in 0..MAX_WINDOWS {
        let count = unsafe { (*STATE.function_counts.get())[window] };
        let functions = unsafe { &(*STATE.functions.get())[window] };
        for function in functions.iter().take(count).flatten() {
            let h = function.header;
            if h.is_ide_controller() {
                unsafe { output.write(cognacos_nt_abi::bus::PCI_DEVICE_INFORMATION {
                    segment: function.segment, bus: function.bus, device: function.device,
                    function: function.function, header_type: h.header_type,
                    revision_id: h.revision_id, programming_interface: h.programming_interface,
                    subclass: h.subclass, class_code: h.class_code, reserved: 0,
                    vendor_id: h.vendor_id, device_id: h.device_id, command: h.command,
                }); }
                return STATUS_SUCCESS;
            }
        }
    }
    STATUS_NOT_FOUND
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciFindNvme(output: *mut pci::PciFunction) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.inventory_ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    for window in 0..MAX_WINDOWS {
        let count = unsafe { (*STATE.function_counts.get())[window] };
        let functions = unsafe { &(*STATE.functions.get())[window] };
        for function in functions.iter().take(count).flatten() {
            if function.header.is_nvme() { unsafe { output.write(*function); } return STATUS_SUCCESS; }
        }
    }
    STATUS_NOT_FOUND
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"PCI_____";
#[used]
#[unsafe(link_section = ".data")]
static DRIVER_COOKIE: u64 = 0x5244_4F54_4F53_0001;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

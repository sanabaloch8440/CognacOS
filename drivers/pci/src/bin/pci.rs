#![no_std]
#![no_main]

use core::{cell::UnsafeCell, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};
use cognacos_pci as pci;
use cognacos_nt_abi::{bus::{PCI_BUS_INTERFACE, INTERFACE_HEADER, PCI_BUS_INTERFACE_VERSION, PPCI_CREATE_PDO, PPCI_DELETE_PDO, PCI_DEVICE_INFORMATION}, io::{DEVICE_OBJECT, DRIVER_OBJECT}, ntstatus::*, types::PVOID};

const HANDOFF_MAGIC: u64 = 0x434f_474e_4143_5043; // COGNACPC
const HANDOFF_VERSION: u32 = 2;
const MAX_PCI_PDOS: usize = 32;
const BUS_RELATIONS: u32 = 0;
const MAX_WINDOWS: usize = 16;

#[repr(C)]
pub struct PciDriverHandoff {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    pub window_count: u32,
    pub reserved: u32,
    /// Kernel callbacks for creating and deleting bus-enumerated PDOs.
    pub pdo_context: PVOID,
    pub create_pdo: PPCI_CREATE_PDO,
    pub delete_pdo: PPCI_DELETE_PDO,
    /// MCFG-derived ECAM windows already mapped with uncached device attributes.
    pub windows: [pci::EcamWindow; MAX_WINDOWS],
}

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
    xhci: UnsafeCell<[[Option<pci::XhciPciFunction>; pci::MAX_XHCI_FUNCTIONS]; MAX_WINDOWS]>,
    xhci_counts: UnsafeCell<[usize; MAX_WINDOWS]>,
    pdos: UnsafeCell<[*mut DEVICE_OBJECT; MAX_PCI_PDOS]>,
    pdo_count: AtomicUsize,
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
    xhci: UnsafeCell::new([[None; pci::MAX_XHCI_FUNCTIONS]; MAX_WINDOWS]),
    xhci_counts: UnsafeCell::new([0; MAX_WINDOWS]),
    pdos: UnsafeCell::new([core::ptr::null_mut(); MAX_PCI_PDOS]),
    pdo_count: AtomicUsize::new(0),
};
static DRIVER_OBJECT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static PDO_BUSY: AtomicBool = AtomicBool::new(false);

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

unsafe extern "win64" fn pnp_dispatch(_device: *mut cognacos_nt_abi::io::DEVICE_OBJECT, irp: *mut cognacos_nt_abi::io::IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut cognacos_nt_abi::io::IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { (*irp).io_status.information = 0; }
    let status = match unsafe { (*stack).minor_function } {
        IRP_MN_START_DEVICE if STATE.ready.load(Ordering::Acquire) => STATUS_SUCCESS,
        IRP_MN_STOP_DEVICE | IRP_MN_REMOVE_DEVICE => STATUS_SUCCESS,
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
        IRP_MN_QUERY_INTERFACE => {
            let parameters = unsafe { &(*stack).parameters };
            let guid = unsafe { core::ptr::read_unaligned(parameters.as_ptr().cast::<*const cognacos_nt_abi::bus::GUID>()) };
            let size = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(8).cast::<u16>()) };
            let version = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(10).cast::<u16>()) };
            let output = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(16).cast::<*mut PCI_BUS_INTERFACE>()) };
            if guid.is_null() || output.is_null() {
                STATUS_INVALID_PARAMETER
            } else if unsafe { *guid } != cognacos_nt_abi::bus::GUID_PCI_BUS_INTERFACE
                || version as u32 != PCI_BUS_INTERFACE_VERSION
                || (size as usize) < core::mem::size_of::<PCI_BUS_INTERFACE>() {
                STATUS_NOT_SUPPORTED
            } else {
                unsafe { CognacPciGetBusInterface(output) }
            }
        },
        IRP_MN_START_DEVICE => STATUS_DEVICE_NOT_READY,
        _ => STATUS_INVALID_DEVICE_REQUEST,
    };
    unsafe { (*irp).io_status.status.status = status; }
    status
}
/// Build a stable BusRelations result. Each PDO is created exactly once
/// through the kernel callback and cached for the lifetime of this bus driver.
/// A failed creation rolls back only PDOs created by this invocation.
unsafe fn query_bus_relations() -> Result<*const PciDeviceRelations, i32> {
    if !STATE.ready.load(Ordering::Acquire) { return Err(STATUS_DEVICE_NOT_READY); }
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
        let create_pdo = handoff.create_pdo.ok_or(STATUS_DEVICE_NOT_READY)?;
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
                if let Some(delete_pdo) = handoff.delete_pdo {
                    for pdo in created[..created_count].iter().copied() { unsafe { delete_pdo(handoff.pdo_context, pdo); } }
                }
                return Err(status);
            }
            let mut pdo = core::ptr::null_mut();
            let status = unsafe { create_pdo(handoff.pdo_context, driver, &information, &mut pdo) };
            if status != STATUS_SUCCESS || pdo.is_null() {
                if let Some(delete_pdo) = handoff.delete_pdo {
                    for pdo in created[..created_count].iter().copied() { unsafe { delete_pdo(handoff.pdo_context, pdo); } }
                }
                return Err(if status == STATUS_SUCCESS { STATUS_DEVICE_CONFIGURATION_ERROR } else { status });
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
    let handoff_address = HANDOFF.load(Ordering::Acquire);
    if handoff_address != 0 {
        let handoff = unsafe { &*(handoff_address as *const PciDriverHandoff) };
        if let Some(delete_pdo) = handoff.delete_pdo {
            let count = STATE.pdo_count.swap(0, Ordering::AcqRel).min(MAX_PCI_PDOS);
            for index in 0..count {
                let pdo = unsafe { (*STATE.pdos.get())[index] };
                if !pdo.is_null() {
                    unsafe { delete_pdo(handoff.pdo_context, pdo); }
                    unsafe { (*STATE.pdos.get())[index] = core::ptr::null_mut(); }
                }
            }
        }
    }
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
    if handoff.pdo_context.is_null() || handoff.create_pdo.is_none() || handoff.delete_pdo.is_none() {
        return STATUS_INVALID_PARAMETER;
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
    let result = (|| {
        let address = window.register_address(segment, bus, device, function, offset)?;
        match width {
            1 => unsafe { core::ptr::write_volatile(address, value as u8); },
            2 if offset & 1 == 0 => unsafe { core::ptr::write_volatile(address.cast::<u16>(), value as u16); },
            4 if offset & 3 == 0 => unsafe { core::ptr::write_volatile(address.cast::<u32>(), value); },
            _ => return Err(pci::PciError::InvalidOffset),
        }
        core::sync::atomic::fence(Ordering::SeqCst);
        Ok(())
    })();
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

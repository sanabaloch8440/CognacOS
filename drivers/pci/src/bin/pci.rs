#![no_std]
#![no_main]

use core::{cell::UnsafeCell, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};
use cognacos_pci as pci;
use cognacos_nt_abi::{io::DRIVER_OBJECT, ntstatus::*, types::PVOID};

const HANDOFF_MAGIC: u64 = 0x434f_474e_4143_5043; // COGNACPC
const HANDOFF_VERSION: u32 = 1;
const MAX_WINDOWS: usize = 16;

#[repr(C)]
pub struct PciDriverHandoff {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    pub window_count: u32,
    pub reserved: u32,
    /// MCFG-derived ECAM windows already mapped with uncached device attributes.
    pub windows: [pci::EcamWindow; MAX_WINDOWS],
}

struct PciState {
    ready: AtomicBool,
    bridge_count: AtomicUsize,
    xhci_count: AtomicUsize,
    function_count: AtomicUsize,
    functions: UnsafeCell<[[Option<pci::PciFunction>; pci::MAX_PCI_FUNCTIONS]; MAX_WINDOWS]>,
    function_counts: UnsafeCell<[usize; MAX_WINDOWS]>,
    bridges: UnsafeCell<[[Option<pci::PciBridgeRange>; pci::MAX_PCI_BRIDGES]; MAX_WINDOWS]>,
    bridge_counts: UnsafeCell<[usize; MAX_WINDOWS]>,
    xhci: UnsafeCell<[[Option<pci::XhciPciFunction>; pci::MAX_XHCI_FUNCTIONS]; MAX_WINDOWS]>,
    xhci_counts: UnsafeCell<[usize; MAX_WINDOWS]>,
}
unsafe impl Sync for PciState {}
static STATE: PciState = PciState {
    ready: AtomicBool::new(false),
    bridge_count: AtomicUsize::new(0),
    xhci_count: AtomicUsize::new(0),
    function_count: AtomicUsize::new(0),
    functions: UnsafeCell::new([[None; pci::MAX_PCI_FUNCTIONS]; MAX_WINDOWS]),
    function_counts: UnsafeCell::new([0; MAX_WINDOWS]),
    bridges: UnsafeCell::new([[None; pci::MAX_PCI_BRIDGES]; MAX_WINDOWS]),
    bridge_counts: UnsafeCell::new([0; MAX_WINDOWS]),
    xhci: UnsafeCell::new([[None; pci::MAX_XHCI_FUNCTIONS]; MAX_WINDOWS]),
    xhci_counts: UnsafeCell::new([0; MAX_WINDOWS]),
};

const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_QUERY_DEVICE_RELATIONS: u8 = 0x07;

unsafe extern "system" fn pnp_dispatch(_device: *mut cognacos_nt_abi::io::DEVICE_OBJECT, irp: *mut cognacos_nt_abi::io::IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut cognacos_nt_abi::io::IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let status = match unsafe { (*stack).minor_function } {
        IRP_MN_START_DEVICE if STATE.ready.load(Ordering::Acquire) => STATUS_SUCCESS,
        IRP_MN_STOP_DEVICE | IRP_MN_REMOVE_DEVICE => STATUS_SUCCESS,
        // PCI function inventory is read-only until bus-resource arbitration,
        // PDO creation, and BusRelations publication are connected to PnP.
        IRP_MN_QUERY_DEVICE_RELATIONS => STATUS_NOT_SUPPORTED,
        IRP_MN_START_DEVICE => STATUS_DEVICE_NOT_READY,
        _ => STATUS_INVALID_DEVICE_REQUEST,
    };
    unsafe { (*irp).io_status.status.status = status; (*irp).io_status.information = 0; }
    status
}
unsafe extern "system" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    STATE.ready.store(false, Ordering::Release);
}

/// Read-only PCI bus discovery. BAR programming, bus mastering, MMIO mapping,
/// resource arbitration, and child PDO publication are intentionally deferred
/// to the PnP manager after it has assigned resources.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    let object = unsafe { &*(driver as *const DRIVER_OBJECT) };
    if object.driver_extension.is_null() { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(object.driver_extension as *const PciDriverHandoff) };
    if handoff.magic != HANDOFF_MAGIC || handoff.version != HANDOFF_VERSION
        || (handoff.size as usize) < core::mem::size_of::<PciDriverHandoff>()
        || handoff.window_count == 0 || handoff.window_count as usize > MAX_WINDOWS {
        return STATUS_INVALID_PARAMETER;
    }

    let mut total_bridges = 0usize;
    let mut total_xhci = 0usize;
    let mut total_functions = 0usize;

    for i in 0..handoff.window_count as usize {
        let window = &handoff.windows[i];
        if window.validate().is_err() { return STATUS_INVALID_PARAMETER; }
        for prior in &handoff.windows[..i] {
            if prior.segment == window.segment
                && prior.bus_start <= window.bus_end && window.bus_start <= prior.bus_end {
                return STATUS_INVALID_PARAMETER;
            }
        }
        let (bridges, bridge_count) = match unsafe { pci::scan_bridge_ranges(window) } {
            Ok(v) => v,
            Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
        };
        let (xhci, xhci_count) = match unsafe { pci::scan_xhci(window) } {
            Ok(v) => v,
            Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
        };
        let function_count = match unsafe { pci::scan_functions(window, &mut (*STATE.functions.get())[i]) } {
            Ok(v) => v,
            Err(_) => return STATUS_DEVICE_CONFIGURATION_ERROR,
        };
        unsafe {
            (*STATE.function_counts.get())[i] = function_count;
            (*STATE.bridges.get())[i] = bridges;
            (*STATE.bridge_counts.get())[i] = bridge_count;
            (*STATE.xhci.get())[i] = xhci;
            (*STATE.xhci_counts.get())[i] = xhci_count;
        }
        total_bridges = match total_bridges.checked_add(bridge_count) {
            Some(v) => v, None => return STATUS_INSUFFICIENT_RESOURCES,
        };
        total_xhci = match total_xhci.checked_add(xhci_count) {
            Some(v) => v, None => return STATUS_INSUFFICIENT_RESOURCES,
        };
        total_functions = match total_functions.checked_add(function_count) {
            Some(v) => v, None => return STATUS_INSUFFICIENT_RESOURCES,
        };
    }

    STATE.bridge_count.store(total_bridges, Ordering::Release);
    STATE.xhci_count.store(total_xhci, Ordering::Release);
    STATE.function_count.store(total_functions, Ordering::Release);
    STATE.ready.store(true, Ordering::Release);
    object.major_function[IRP_MJ_PNP] = Some(pnp_dispatch);
    object.driver_unload = Some(driver_unload);
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub extern "win64" fn CognacPciBridgeCount() -> usize {
    if STATE.ready.load(Ordering::Acquire) { STATE.bridge_count.load(Ordering::Acquire) } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "win64" fn CognacPciXhciCount() -> usize {
    if STATE.ready.load(Ordering::Acquire) { STATE.xhci_count.load(Ordering::Acquire) } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "win64" fn CognacPciFunctionCount() -> usize {
    if STATE.ready.load(Ordering::Acquire) { STATE.function_count.load(Ordering::Acquire) } else { 0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacPciFindAhci(output: *mut pci::PciFunction) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
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
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
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

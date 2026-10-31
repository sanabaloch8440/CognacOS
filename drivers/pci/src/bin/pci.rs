#![no_std]
#![no_main]

use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};
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
    bridges: UnsafeCell<MaybeUninit<[[Option<pci::PciBridgeRange>; pci::MAX_PCI_BRIDGES]; MAX_WINDOWS]>>,
    bridge_counts: UnsafeCell<MaybeUninit<[usize; MAX_WINDOWS]>>,
    xhci: UnsafeCell<MaybeUninit<[[Option<pci::XhciPciFunction>; pci::MAX_XHCI_FUNCTIONS]; MAX_WINDOWS]>>,
    xhci_counts: UnsafeCell<MaybeUninit<[usize; MAX_WINDOWS]>>,
}
unsafe impl Sync for PciState {}
static STATE: PciState = PciState {
    ready: AtomicBool::new(false),
    bridge_count: AtomicUsize::new(0),
    xhci_count: AtomicUsize::new(0),
    bridges: UnsafeCell::new(MaybeUninit::uninit()),
    bridge_counts: UnsafeCell::new(MaybeUninit::uninit()),
    xhci: UnsafeCell::new(MaybeUninit::uninit()),
    xhci_counts: UnsafeCell::new(MaybeUninit::uninit()),
};

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

    let mut bridge_storage = [[None; pci::MAX_PCI_BRIDGES]; MAX_WINDOWS];
    let mut bridge_counts = [0usize; MAX_WINDOWS];
    let mut xhci_storage = [[None; pci::MAX_XHCI_FUNCTIONS]; MAX_WINDOWS];
    let mut xhci_counts = [0usize; MAX_WINDOWS];
    let mut total_bridges = 0usize;
    let mut total_xhci = 0usize;

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
        bridge_storage[i] = bridges;
        bridge_counts[i] = bridge_count;
        xhci_storage[i] = xhci;
        xhci_counts[i] = xhci_count;
        total_bridges = match total_bridges.checked_add(bridge_count) {
            Some(v) => v, None => return STATUS_INSUFFICIENT_RESOURCES,
        };
        total_xhci = match total_xhci.checked_add(xhci_count) {
            Some(v) => v, None => return STATUS_INSUFFICIENT_RESOURCES,
        };
    }

    unsafe {
        (*STATE.bridges.get()).write(bridge_storage);
        (*STATE.bridge_counts.get()).write(bridge_counts);
        (*STATE.xhci.get()).write(xhci_storage);
        (*STATE.xhci_counts.get()).write(xhci_counts);
    }
    STATE.bridge_count.store(total_bridges, Ordering::Release);
    STATE.xhci_count.store(total_xhci, Ordering::Release);
    STATE.ready.store(true, Ordering::Release);
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

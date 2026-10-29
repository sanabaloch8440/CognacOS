//! Kernel-owned PCI/PnP resource handoff for the first xHCI controller.
//! This is deliberately not a general PCI bus driver yet. The record is stable
//! for the lifetime of the kernel and is published only after BAR sizing and
//! uncached MMIO mapping have succeeded. Bus mastering stays disabled until an
//! IOMMU requester domain and interrupt path are ready.
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

pub const XHCI_PNP_MAGIC: u64 = 0x434f_474e_4143_504e;
pub const XHCI_PNP_VERSION: u32 = 1;

#[repr(C, align(64))]
pub struct PciDeviceObject {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    pub ready: AtomicBool,
    pub segment: AtomicU32,
    pub bus: AtomicU32,
    pub device: AtomicU32,
    pub function: AtomicU32,
    pub vendor_id: AtomicU32,
    pub device_id: AtomicU32,
    pub requester_id: AtomicU32,
    pub interrupt_line: AtomicU32,
    pub interrupt_pin: AtomicU32,
    pub bar_physical: AtomicU64,
    pub bar_length: AtomicU64,
    pub bar_virtual: AtomicU64,
    pub state: AtomicU32,
}
impl PciDeviceObject {
    const fn new() -> Self {
        Self { magic:XHCI_PNP_MAGIC, size:core::mem::size_of::<Self>() as u32,
            version:XHCI_PNP_VERSION, ready:AtomicBool::new(false),
            segment:AtomicU32::new(0), bus:AtomicU32::new(0), device:AtomicU32::new(0),
            function:AtomicU32::new(0), vendor_id:AtomicU32::new(0), device_id:AtomicU32::new(0),
            requester_id:AtomicU32::new(0), interrupt_line:AtomicU32::new(0),
            interrupt_pin:AtomicU32::new(0), bar_physical:AtomicU64::new(0),
            bar_length:AtomicU64::new(0), bar_virtual:AtomicU64::new(0), state:AtomicU32::new(0) }
    }
}
pub static XHCI_PCI_DEVICE: PciDeviceObject = PciDeviceObject::new();
static PUBLISH_LOCK: AtomicBool = AtomicBool::new(false);

pub const STATE_DISCOVERED:u32=1;
pub const STATE_MMIO_MAPPED:u32=2;
pub const STATE_DMA_READY:u32=3;
pub const STATE_INTERRUPT_READY:u32=4;
pub const STATE_DRIVER_BOUND:u32=5;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct XhciPciResources {
    pub device_object:u64,
    pub segment:u16,
    pub bus:u8,
    pub device:u8,
    pub function:u8,
    pub vendor_id:u16,
    pub device_id:u16,
    pub requester_id:u16,
    pub interrupt_line:u8,
    pub interrupt_pin:u8,
    pub bar_physical:u64,
    pub bar_length:u64,
    pub bar_virtual:u64,
    pub state:u32,
}

/// Publish a single validated controller resource record. This does not enable
/// PCI bus mastering and does not imply DMA/interrupt readiness.
pub fn publish_xhci(
    bus:u8,device:u8,function:u8,vendor_id:u16,device_id:u16,
    interrupt_line:u8,interrupt_pin:u8,bar_physical:u64,bar_length:u64,bar_virtual:u64,
)->Result<u64,()> {
    if device>=32||function>=8||vendor_id==0||vendor_id==0xffff||bar_physical==0
        ||bar_length<0x20||bar_virtual==0||bar_physical.checked_add(bar_length).is_none()
        ||bar_virtual.checked_add(bar_length).is_none() { return Err(()); }
    if PUBLISH_LOCK.compare_exchange(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){return Err(());}
    if XHCI_PCI_DEVICE.ready.load(Ordering::Acquire) {
        PUBLISH_LOCK.store(false,Ordering::Release);
        return Err(());
    }
    XHCI_PCI_DEVICE.bus.store(bus as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.device.store(device as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.function.store(function as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.vendor_id.store(vendor_id as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.device_id.store(device_id as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.requester_id.store(((bus as u16 as u32)<<8)|((device as u32)<<3)|function as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.interrupt_line.store(interrupt_line as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.interrupt_pin.store(interrupt_pin as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_physical.store(bar_physical,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_length.store(bar_length,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_virtual.store(bar_virtual,Ordering::Relaxed);
    XHCI_PCI_DEVICE.state.store(STATE_MMIO_MAPPED,Ordering::Relaxed);
    // Bind the stable PnP device-object address to its real PCI requester ID
    // before exposing the object to driver/DMA-adapter consumers.
    let requester_status = unsafe {
        crate::dma::iommu::RdotDmaRegisterDeviceRequester(
            core::ptr::addr_of!(XHCI_PCI_DEVICE) as *mut core::ffi::c_void,
            0, bus, device, function,
        )
    };
    if requester_status != 0 {
        XHCI_PCI_DEVICE.state.store(0, Ordering::Relaxed);
        PUBLISH_LOCK.store(false, Ordering::Release);
        return Err(());
    }
    XHCI_PCI_DEVICE.ready.store(true,Ordering::Release);
    PUBLISH_LOCK.store(false,Ordering::Release);
    Ok(core::ptr::addr_of!(XHCI_PCI_DEVICE) as u64)
}

pub fn xhci_resources()->Option<XhciPciResources> {
    let d=&XHCI_PCI_DEVICE;
    if !d.ready.load(Ordering::Acquire)||d.magic!=XHCI_PNP_MAGIC||d.version!=XHCI_PNP_VERSION{return None}
    Some(XhciPciResources{
        device_object:core::ptr::addr_of!(XHCI_PCI_DEVICE) as u64,
        segment:d.segment.load(Ordering::Relaxed) as u16,
        bus:d.bus.load(Ordering::Relaxed) as u8,device:d.device.load(Ordering::Relaxed) as u8,
        function:d.function.load(Ordering::Relaxed) as u8,vendor_id:d.vendor_id.load(Ordering::Relaxed) as u16,
        device_id:d.device_id.load(Ordering::Relaxed) as u16,requester_id:d.requester_id.load(Ordering::Relaxed) as u16,
        interrupt_line:d.interrupt_line.load(Ordering::Relaxed) as u8,interrupt_pin:d.interrupt_pin.load(Ordering::Relaxed) as u8,
        bar_physical:d.bar_physical.load(Ordering::Relaxed),bar_length:d.bar_length.load(Ordering::Relaxed),
        bar_virtual:d.bar_virtual.load(Ordering::Relaxed),state:d.state.load(Ordering::Relaxed)})
}

pub fn advance_xhci_state(expected:u32,next:u32)->Result<(),()> {
    if !XHCI_PCI_DEVICE.ready.load(Ordering::Acquire)||next!=expected+1||next>STATE_DRIVER_BOUND{return Err(())}
    // A published MMIO BAR is not proof that DMA is safe. Do not let the
    // controller advance to DMA_READY until a real IOMMU backend is registered.
    if next==STATE_DMA_READY && !crate::dma::iommu::available(){return Err(())}
    XHCI_PCI_DEVICE.state.compare_exchange(expected,next,Ordering::AcqRel,Ordering::Acquire).map(|_|()).map_err(|_|())
}

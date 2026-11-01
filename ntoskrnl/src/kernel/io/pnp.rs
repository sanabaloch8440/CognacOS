//! Kernel-owned PCI/PnP resource handoff for the first xHCI controller.
//! This is deliberately not a general PCI bus driver yet. The record is stable
//! for the lifetime of the kernel and is published after BAR sizing and real
//! PCI PDO binding. usbxhci.sys maps the BAR through MmMapIoSpace; bus mastering
//! stays disabled until its IOMMU-backed DMA allocations are ready.
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

pub const XHCI_PNP_MAGIC: u64 = 0x434f_474e_4143_504e;
pub const XHCI_PNP_VERSION: u32 = 1;

#[repr(C, align(64))]
pub struct PciDeviceObject {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    pub ready: AtomicBool,
    pub pdo_device_object: AtomicUsize,
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
    pub root_bus: AtomicU32,
    pub path_count: AtomicU32,
    pub path_devfn: [AtomicU32; 8],
}
impl PciDeviceObject {
    const fn new() -> Self {
        Self { magic:XHCI_PNP_MAGIC, size:core::mem::size_of::<Self>() as u32,
            version:XHCI_PNP_VERSION, ready:AtomicBool::new(false),
            pdo_device_object:AtomicUsize::new(0), segment:AtomicU32::new(0), bus:AtomicU32::new(0), device:AtomicU32::new(0),
            function:AtomicU32::new(0), vendor_id:AtomicU32::new(0), device_id:AtomicU32::new(0),
            requester_id:AtomicU32::new(0), interrupt_line:AtomicU32::new(0),
            interrupt_pin:AtomicU32::new(0), bar_physical:AtomicU64::new(0),
            bar_length:AtomicU64::new(0), bar_virtual:AtomicU64::new(0), state:AtomicU32::new(0),
            root_bus:AtomicU32::new(0), path_count:AtomicU32::new(0),
            path_devfn:[const { AtomicU32::new(0) };8] }
    }
}
pub static XHCI_PCI_DEVICE: PciDeviceObject = PciDeviceObject::new();
static PUBLISH_LOCK: AtomicBool = AtomicBool::new(false);

pub const STATE_DISCOVERED:u32=1;
pub const STATE_MMIO_MAPPED:u32=2;
pub const STATE_DMA_READY:u32=3;
pub const STATE_INTERRUPT_READY:u32=4;
pub const STATE_DRIVER_BOUND:u32=5;
pub const STATE_BAR_ASSIGNED:u32=STATE_DISCOVERED;

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
    pub interrupt_vector:u8,
    pub requester_path:cognacos_pci::PciRequesterPath,
}

/// Publish a single validated controller resource record. This does not enable
/// PCI bus mastering and does not imply DMA/interrupt readiness.
pub fn publish_xhci(
    path:&cognacos_pci::PciRequesterPath,vendor_id:u16,device_id:u16,
    interrupt_line:u8,interrupt_pin:u8,pdo_device_object:*mut core::ffi::c_void,
    bar_physical:u64,bar_length:u64,bar_virtual:u64,
)->Result<u64,()> {
    let count=path.path_count as usize;
    if count==0||count>8||path.endpoint_device>=32||path.endpoint_function>=8
        ||path.path[count-1].device!=path.endpoint_device
        ||path.path[count-1].function!=path.endpoint_function
        ||path.path.iter().take(count).any(|p|p.device>=32||p.function>=8)
        ||pdo_device_object.is_null()||vendor_id==0||vendor_id==0xffff||bar_physical==0
        ||bar_length<0x20||bar_physical.checked_add(bar_length).is_none()
        ||(bar_virtual!=0&&bar_virtual.checked_add(bar_length).is_none()) { return Err(()); }
    if PUBLISH_LOCK.compare_exchange(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){return Err(());}
    if XHCI_PCI_DEVICE.ready.load(Ordering::Acquire) {
        PUBLISH_LOCK.store(false,Ordering::Release);
        return Err(());
    }
    XHCI_PCI_DEVICE.pdo_device_object.store(pdo_device_object as usize,Ordering::Relaxed);
    XHCI_PCI_DEVICE.segment.store(path.segment as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bus.store(path.endpoint_bus as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.device.store(path.endpoint_device as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.function.store(path.endpoint_function as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.root_bus.store(path.root_bus as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.path_count.store(count as u32,Ordering::Relaxed);
    for i in 0..8 {
        let packed=if i<count {((path.path[i].device as u32)<<3)|path.path[i].function as u32}else{0};
        XHCI_PCI_DEVICE.path_devfn[i].store(packed,Ordering::Relaxed);
    }
    XHCI_PCI_DEVICE.vendor_id.store(vendor_id as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.device_id.store(device_id as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.requester_id.store(((path.endpoint_bus as u16 as u32)<<8)|((path.endpoint_device as u32)<<3)|path.endpoint_function as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.interrupt_line.store(interrupt_line as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.interrupt_pin.store(interrupt_pin as u32,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_physical.store(bar_physical,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_length.store(bar_length,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_virtual.store(bar_virtual,Ordering::Relaxed);
    XHCI_PCI_DEVICE.state.store(STATE_BAR_ASSIGNED,Ordering::Relaxed);
    // Bind the real PCI PDO address to its PCI requester ID
    // before exposing the object to driver/DMA-adapter consumers.
    let requester_status = unsafe {
        crate::dma::iommu::RdotDmaRegisterDeviceRequester(
            pdo_device_object,
            path.segment, path.endpoint_bus, path.endpoint_device, path.endpoint_function,
        )
    };
    if requester_status != 0 {
        XHCI_PCI_DEVICE.state.store(0, Ordering::Relaxed);
        PUBLISH_LOCK.store(false, Ordering::Release);
        return Err(());
    }
    XHCI_PCI_DEVICE.ready.store(true,Ordering::Release);
    PUBLISH_LOCK.store(false,Ordering::Release);
    Ok(pdo_device_object as u64)
}

pub fn xhci_resources()->Option<XhciPciResources> {
    let d=&XHCI_PCI_DEVICE;
    if !d.ready.load(Ordering::Acquire)||d.magic!=XHCI_PNP_MAGIC||d.version!=XHCI_PNP_VERSION{return None}
    Some(XhciPciResources{
        device_object:d.pdo_device_object.load(Ordering::Relaxed) as u64,
        segment:d.segment.load(Ordering::Relaxed) as u16,
        bus:d.bus.load(Ordering::Relaxed) as u8,device:d.device.load(Ordering::Relaxed) as u8,
        function:d.function.load(Ordering::Relaxed) as u8,vendor_id:d.vendor_id.load(Ordering::Relaxed) as u16,
        device_id:d.device_id.load(Ordering::Relaxed) as u16,requester_id:d.requester_id.load(Ordering::Relaxed) as u16,
        interrupt_line:d.interrupt_line.load(Ordering::Relaxed) as u8,interrupt_pin:d.interrupt_pin.load(Ordering::Relaxed) as u8,
        bar_physical:d.bar_physical.load(Ordering::Relaxed),bar_length:d.bar_length.load(Ordering::Relaxed),
        bar_virtual:d.bar_virtual.load(Ordering::Relaxed),state:d.state.load(Ordering::Relaxed),
        interrupt_vector:XHCI_INTERRUPT_VECTOR.load(Ordering::Acquire) as u8,
        requester_path:cognacos_pci::PciRequesterPath {
            segment:d.segment.load(Ordering::Relaxed) as u16,
            root_bus:d.root_bus.load(Ordering::Relaxed) as u8,
            endpoint_bus:d.bus.load(Ordering::Relaxed) as u8,
            endpoint_device:d.device.load(Ordering::Relaxed) as u8,
            endpoint_function:d.function.load(Ordering::Relaxed) as u8,
            path:{let mut p=[cognacos_pci::PciPathElement::default();8];
                for i in 0..8 {let v=d.path_devfn[i].load(Ordering::Relaxed);p[i]=cognacos_pci::PciPathElement{device:(v>>3) as u8,function:(v&7) as u8};}p},
            path_count:d.path_count.load(Ordering::Relaxed) as u8,
        }})
}

/// Advance controller startup only when each prerequisite is independently met.
/// The interrupt-ready transition requires a real HAL callback registration;
/// it deliberately does not infer readiness from PCI's legacy IRQ line number.
pub fn advance_xhci_state(expected:u32,next:u32)->Result<(),()> {
    if !XHCI_PCI_DEVICE.ready.load(Ordering::Acquire)||next!=expected+1||next>STATE_DRIVER_BOUND{return Err(())}
    if next==STATE_DMA_READY && !crate::dma::iommu::available(){return Err(())}
    if next==STATE_INTERRUPT_READY {
        // A vector must be bound to a handler before the device can be treated
        // as interrupt-ready. Route unmasking remains a separate HAL operation.
        let vector = XHCI_INTERRUPT_VECTOR.load(Ordering::Acquire);
        if vector == 0 || !crate::hal::device_interrupt_handler_registered(vector as u8) { return Err(()); }
    }
    if next==STATE_DRIVER_BOUND && XHCI_PCI_DEVICE.state.load(Ordering::Acquire)!=STATE_INTERRUPT_READY{return Err(())}
    XHCI_PCI_DEVICE.state.compare_exchange(expected,next,Ordering::AcqRel,Ordering::Acquire).map(|_|()).map_err(|_|())
}

static XHCI_INTERRUPT_VECTOR: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Reserve and configure the controller's PCI INTx route while keeping it
/// masked. The IRQ line must already have been resolved through MADT overrides
/// and matched to the I/O-APIC that owns its GSI range.
pub fn prepare_xhci_interrupt_route(io_base:u64,gsi_base:u32,gsi:u32,bsp_apic_id:u8,active_low:bool,level_triggered:bool)->Result<u8,()>{
    if !XHCI_PCI_DEVICE.ready.load(Ordering::Acquire)||XHCI_INTERRUPT_VECTOR.load(Ordering::Acquire)!=0{return Err(())}
    let vector=crate::hal::allocate_device_interrupt_vector().map_err(|_|())?;
    if !crate::ntoskrnl::ke::arch::device_interrupt_vector_ready(vector){
        let _=crate::hal::release_device_interrupt_vector(vector);return Err(())
    }
    if unsafe{crate::hal::configure_device_interrupt_route_masked(io_base,gsi_base,gsi,vector,bsp_apic_id,active_low,level_triggered)}.is_err(){
        let _=crate::hal::release_device_interrupt_vector(vector);return Err(())
    }
    if crate::hal::bind_device_interrupt_route(vector,io_base,gsi_base,gsi).is_err(){
        let _=crate::hal::release_device_interrupt_vector(vector);return Err(())
    }
    XHCI_INTERRUPT_VECTOR.store(vector as u32,Ordering::Release);
    Ok(vector)
}


/// Record the vector reserved and registered for the xHCI interrupt callback.
/// The handler must already be installed; a different vector cannot replace it.
pub fn register_xhci_interrupt_vector(vector:u8)->Result<(),()> {
    if !crate::hal::device_interrupt_handler_registered(vector) { return Err(()); }
    match XHCI_INTERRUPT_VECTOR.compare_exchange(0,vector as u32,Ordering::AcqRel,Ordering::Acquire) {
        Ok(_) => Ok(()),
        Err(existing) if existing==vector as u32 => Ok(()),
        Err(_) => Err(()),
    }
}


/// Kernel-owned device-node table used by bus enumerators. The table is
/// deliberately fixed-size for early boot (no heap dependency); it records
/// discovery and lifecycle state separately from any driver-specific resource
/// handoff such as the xHCI record above.
pub const MAX_PNP_DEVICES: usize = 128;
pub const PNP_NO_PARENT: u32 = u32::MAX;
pub const PNP_DEVICE_EMPTY: u32 = 0;
pub const PNP_DEVICE_ENUMERATED: u32 = 1;
pub const PNP_DEVICE_RESOURCES_ASSIGNED: u32 = 2;
pub const PNP_DEVICE_STARTED: u32 = 3;
pub const PNP_DEVICE_FAILED: u32 = 4;
pub const PNP_DEVICE_REMOVED: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PnpError {
    InvalidFunction,
    InvalidParent,
    TableFull,
    DuplicateDevice,
    InvalidState,
    InvalidTransition,
    Registry(crate::ntoskrnl::cm::service_config::ExtendedConfigError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PnpDeviceSnapshot {
    pub index: u32,
    pub state: u32,
    pub parent: u32,
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub programming_interface: u8,
    pub last_status: i32,
    pub start_attempts: u32,
    pub is_root: bool,
    pub device_object: usize,
}

struct PnpDeviceNode {
    state: AtomicU32,
    parent: AtomicU32,
    segment: AtomicU32,
    bus: AtomicU32,
    device: AtomicU32,
    function: AtomicU32,
    vendor_id: AtomicU32,
    device_id: AtomicU32,
    class_code: AtomicU32,
    subclass: AtomicU32,
    programming_interface: AtomicU32,
    last_status: core::sync::atomic::AtomicI32,
    start_attempts: AtomicU32,
    is_root: AtomicBool,
    device_object: AtomicUsize,
    service_name: UnsafeCell<[u16;128]>,
    service_name_len: AtomicUsize,
}
impl PnpDeviceNode {
    const fn new() -> Self {
        Self {
            state: AtomicU32::new(PNP_DEVICE_EMPTY),
            parent: AtomicU32::new(PNP_NO_PARENT),
            segment: AtomicU32::new(0), bus: AtomicU32::new(0),
            device: AtomicU32::new(0), function: AtomicU32::new(0),
            vendor_id: AtomicU32::new(0), device_id: AtomicU32::new(0),
            class_code: AtomicU32::new(0), subclass: AtomicU32::new(0),
            programming_interface: AtomicU32::new(0),
            last_status: core::sync::atomic::AtomicI32::new(0),
            start_attempts: AtomicU32::new(0),
            is_root: AtomicBool::new(false),
            device_object: AtomicUsize::new(0),
            service_name: UnsafeCell::new([0;128]),
            service_name_len: AtomicUsize::new(0),
        }
    }
}
unsafe impl Sync for PnpDeviceNode {}
/// Match every discovered PCI devnode against the installed Windows Enum\\PCI
/// tree. The Enum instance's Service value selects a configured service; the
/// Configuration Manager then validates that service's Start/Type/ImagePath.
pub fn match_registry_services(hive:&crate::ntoskrnl::cm::hive::Hive<'_>)->Result<usize,PnpError>{
    let mut matched=0usize;
    for index in 0..MAX_PNP_DEVICES{
        let Some(snapshot)=query_pnp_device(index as u32) else{continue};
        if snapshot.state!=PNP_DEVICE_ENUMERATED{continue}
        let mut name=[0u16;128];
        let Some(length)=crate::ntoskrnl::cm::service_config::find_pci_device_service(
            hive,snapshot.vendor_id,snapshot.device_id,&mut name,
        ).map_err(PnpError::Registry)? else{continue};
        if length==0||length>name.len(){return Err(PnpError::InvalidState)}
        let _guard=lock_pnp_table();
        let node=&PNP_DEVICES[index];
        if node.state.load(Ordering::Acquire)!=PNP_DEVICE_ENUMERATED{continue}
        unsafe{(*node.service_name.get()).fill(0);(*node.service_name.get())[..length].copy_from_slice(&name[..length]);}
        node.service_name_len.store(length,Ordering::Release);
        matched+=1;
    }
    Ok(matched)
}

/// Bind a devnode to its live PDO/FDO and retain an Object/I/O-manager
/// reference until the devnode transitions to REMOVED. The reference is
/// acquired before taking the devnode lock to preserve WDM lock ordering.
pub fn bind_pnp_device_object(
    index: u32,
    device: *mut crate::io::DeviceObject,
) -> Result<(), PnpError> {
    if device.is_null() { return Err(PnpError::InvalidFunction); }
    let referenced = unsafe { crate::io::io_reference_device_object(device) };
    if referenced.is_null() { return Err(PnpError::InvalidState); }
    let result = {
        let _guard = lock_pnp_table();
        match PNP_DEVICES.get(index as usize) {
            None => Err(PnpError::InvalidState),
            Some(node) if matches!(node.state.load(Ordering::Acquire), PNP_DEVICE_EMPTY | PNP_DEVICE_REMOVED) => {
                Err(PnpError::InvalidState)
            }
            Some(node) if node.device_object.compare_exchange(
                0, device as usize, Ordering::AcqRel, Ordering::Acquire,
            ).is_err() => Err(PnpError::DuplicateDevice),
            Some(_) => Ok(()),
        }
    };
    if result.is_err() { unsafe { crate::io::io_dereference_device_object(referenced); } }
    result
}

/// Retire the devnode associated with a PDO/FDO after successful PnP removal.
/// The retained I/O-manager reference is released by transition_pnp_device.
pub fn remove_pnp_device_object(device: *mut crate::io::DeviceObject, status: i32) {
    if device.is_null() || status != crate::nt_abi::ntstatus::STATUS_SUCCESS { return; }
    let index = {
        let _guard = lock_pnp_table();
        PNP_DEVICES.iter().enumerate().find_map(|(index, node)| {
            (node.device_object.load(Ordering::Acquire) == device as usize
                && node.state.load(Ordering::Acquire) != PNP_DEVICE_REMOVED)
                .then_some(index as u32)
        })
    };
    if let Some(index) = index {
        let _ = transition_pnp_device(index, PNP_DEVICE_REMOVED, status);
    }
}

/// Copy the registry-selected service name for one devnode. A zero-length
/// result means no installed Enum instance matched this hardware ID.
pub fn query_pnp_device_service(index:u32,out:&mut[u16])->Option<usize>{
    let _guard=lock_pnp_table();
    let node=PNP_DEVICES.get(index as usize)?;
    let length=node.service_name_len.load(Ordering::Acquire);
    if length==0||length>out.len(){return None}
    unsafe{out[..length].copy_from_slice(&(*node.service_name.get())[..length]);}
    Some(length)
}

static PNP_DEVICES: [PnpDeviceNode; MAX_PNP_DEVICES] =
    [const { PnpDeviceNode::new() }; MAX_PNP_DEVICES];
static PNP_TABLE_LOCK: AtomicBool = AtomicBool::new(false);
struct PnpTableGuard;
impl Drop for PnpTableGuard {
    fn drop(&mut self) { PNP_TABLE_LOCK.store(false, Ordering::Release); }
}
fn lock_pnp_table() -> PnpTableGuard {
    while PNP_TABLE_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    PnpTableGuard
}

fn valid_pnp_transition(from: u32, to: u32) -> bool {
    match from {
        PNP_DEVICE_ENUMERATED => to == PNP_DEVICE_RESOURCES_ASSIGNED || to == PNP_DEVICE_FAILED || to == PNP_DEVICE_REMOVED,
        PNP_DEVICE_RESOURCES_ASSIGNED => to == PNP_DEVICE_STARTED || to == PNP_DEVICE_FAILED || to == PNP_DEVICE_REMOVED,
        PNP_DEVICE_STARTED => to == PNP_DEVICE_REMOVED || to == PNP_DEVICE_FAILED,
        PNP_DEVICE_FAILED => to == PNP_DEVICE_REMOVED,
        _ => false,
    }
}

/// Create the synthetic root-bus devnode. It has no fabricated PCI BDF.
pub fn register_root_device() -> Result<u32, PnpError> {
    let _guard = lock_pnp_table();
    if let Some((index, _)) = PNP_DEVICES.iter().enumerate().find(|(_, node)|
        node.is_root.load(Ordering::Acquire)
            && node.state.load(Ordering::Acquire) != PNP_DEVICE_EMPTY
            && node.state.load(Ordering::Relaxed) != PNP_DEVICE_REMOVED) {
        return Ok(index as u32);
    }
    let (index, node) = PNP_DEVICES.iter().enumerate()
        .find(|(_, node)| matches!(node.state.load(Ordering::Acquire), PNP_DEVICE_EMPTY | PNP_DEVICE_REMOVED))
        .ok_or(PnpError::TableFull)?;
    node.parent.store(PNP_NO_PARENT, Ordering::Relaxed);
    node.segment.store(0, Ordering::Relaxed); node.bus.store(0, Ordering::Relaxed);
    node.device.store(0, Ordering::Relaxed); node.function.store(0, Ordering::Relaxed);
    node.vendor_id.store(0, Ordering::Relaxed); node.device_id.store(0, Ordering::Relaxed);
    node.class_code.store(0, Ordering::Relaxed); node.subclass.store(0, Ordering::Relaxed);
    node.programming_interface.store(0, Ordering::Relaxed);
    node.last_status.store(0, Ordering::Relaxed); node.start_attempts.store(0, Ordering::Relaxed);
    node.service_name_len.store(0, Ordering::Relaxed);
    node.is_root.store(true, Ordering::Relaxed);
    node.device_object.store(0, Ordering::Relaxed);
    node.state.store(PNP_DEVICE_ENUMERATED, Ordering::Release);
    Ok(index as u32)
}

/// Return HardwareIDs and CompatibleIDs as UTF-16 MULTI_SZ values.
/// Returned lengths include the double-NUL terminator. Pass None to query
/// required length. IDs use Windows PCI matching syntax.
pub fn query_pnp_device_ids(
    index: u32,
    hardware_ids: Option<&mut [u16]>,
    compatible_ids: Option<&mut [u16]>,
) -> Result<(usize, usize), PnpError> {
    let snapshot = query_pnp_device(index).ok_or(PnpError::InvalidState)?;
    fn ascii(out: &mut [u16], at: &mut usize, s: &[u8]) -> Result<(), PnpError> {
        let end = at.checked_add(s.len()).ok_or(PnpError::InvalidState)?;
        if end >= out.len() { return Err(PnpError::TableFull); }
        for (dst, src) in out[*at..end].iter_mut().zip(s) { *dst = *src as u16; }
        *at = end; Ok(())
    }
    fn hex(out: &mut [u16], at: &mut usize, value: u32, digits: usize) -> Result<(), PnpError> {
        const H: &[u8;16] = b"0123456789ABCDEF";
        let end = at.checked_add(digits).ok_or(PnpError::InvalidState)?;
        if end >= out.len() { return Err(PnpError::TableFull); }
        for i in 0..digits { out[*at+i] = H[((value >> ((digits-1-i)*4)) & 15) as usize] as u16; }
        *at = end; Ok(())
    }
    fn finish(out: &mut [u16], at: usize) -> Result<usize, PnpError> {
        if at + 1 >= out.len() { return Err(PnpError::TableFull); }
        out[at] = 0; out[at+1] = 0; Ok(at+2)
    }
    let mut hw_len = 0usize;
    let mut compat_len = 0usize;
    if let Some(out) = hardware_ids {
        out.fill(0);
        if snapshot.is_root { ascii(out, &mut hw_len, b"ROOT\\COGNACOS")?; }
        else {
            ascii(out, &mut hw_len, b"PCI\\VEN_")?; hex(out, &mut hw_len, snapshot.vendor_id as u32, 4)?;
            ascii(out, &mut hw_len, b"&DEV_")?; hex(out, &mut hw_len, snapshot.device_id as u32, 4)?;
        }
        hw_len = finish(out, hw_len)?;
    } else { hw_len = if snapshot.is_root { 15 } else { 23 }; }
    if let Some(out) = compatible_ids {
        out.fill(0);
        if snapshot.is_root { ascii(out, &mut compat_len, b"ROOT\\PNP0A08")?; }
        else {
            ascii(out, &mut compat_len, b"PCI\\CC_")?;
            hex(out, &mut compat_len, ((snapshot.class_code as u32)<<16)
                | ((snapshot.subclass as u32)<<8) | snapshot.programming_interface as u32, 6)?;
            compat_len = finish(out, compat_len)?;
            ascii(out, &mut compat_len, b"PCI\\CC_")?;
            hex(out, &mut compat_len, ((snapshot.class_code as u32)<<8) | snapshot.subclass as u32, 4)?;
        }
        compat_len = finish(out, compat_len)?;
    } else { compat_len = if snapshot.is_root { 14 } else { 28 }; }
    Ok((hw_len, compat_len))
}

/// Add a PCI function to the kernel PnP device tree. Enumeration is read-only:
/// this function does not size BARs, enable decode/bus mastering, or start hardware.
/// Parent indices identify the bus/bridge node when the enumerator has resolved
/// the topology; root-bus functions use None.
pub fn register_pci_function(
    function: &cognacos_pci::PciFunction,
    parent: Option<u32>,
) -> Result<u32, PnpError> {
    let h = function.header;
    register_pci_information(&crate::nt_abi::bus::PCI_DEVICE_INFORMATION {
        segment: function.segment, bus: function.bus, device: function.device,
        function: function.function, header_type: h.header_type, revision_id: h.revision_id,
        programming_interface: h.programming_interface, subclass: h.subclass,
        class_code: h.class_code, reserved: 0, vendor_id: h.vendor_id,
        device_id: h.device_id, command: h.command,
    }, parent)
}

/// Register a bus-neutral PCI inventory record supplied by pci.sys. This is
/// the kernel PnP boundary: the bus driver owns configuration access and
/// enumeration; NTOSKRNL owns devnode identity and lifecycle.
pub fn register_pci_information(
    function: &crate::nt_abi::bus::PCI_DEVICE_INFORMATION,
    parent: Option<u32>,
) -> Result<u32, PnpError> {
    if function.device >= 32 || function.function >= 8
        || function.vendor_id == 0 || function.vendor_id == 0xffff {
        return Err(PnpError::InvalidFunction);
    }
    let _guard = lock_pnp_table();
    let parent_index = parent.unwrap_or(PNP_NO_PARENT);
    if parent_index != PNP_NO_PARENT {
        let p = PNP_DEVICES.get(parent_index as usize).ok_or(PnpError::InvalidParent)?;
        if p.state.load(Ordering::Acquire) == PNP_DEVICE_EMPTY || p.state.load(Ordering::Relaxed) == PNP_DEVICE_REMOVED {
            return Err(PnpError::InvalidParent);
        }
    }
    for (i, node) in PNP_DEVICES.iter().enumerate() {
        let state = node.state.load(Ordering::Acquire);
        if state == PNP_DEVICE_EMPTY || state == PNP_DEVICE_REMOVED { continue; }
        if node.segment.load(Ordering::Relaxed) == function.segment as u32
            && node.bus.load(Ordering::Relaxed) == function.bus as u32
            && node.device.load(Ordering::Relaxed) == function.device as u32
            && node.function.load(Ordering::Relaxed) == function.function as u32 {
            return Err(PnpError::DuplicateDevice);
        }
        let _ = i;
    }
    let (index, node) = PNP_DEVICES.iter().enumerate()
        .find(|(_, n)| {
            let s = n.state.load(Ordering::Acquire);
            s == PNP_DEVICE_EMPTY || s == PNP_DEVICE_REMOVED
        }).ok_or(PnpError::TableFull)?;
    node.parent.store(parent_index, Ordering::Relaxed);
    node.segment.store(function.segment as u32, Ordering::Relaxed);
    node.bus.store(function.bus as u32, Ordering::Relaxed);
    node.device.store(function.device as u32, Ordering::Relaxed);
    node.function.store(function.function as u32, Ordering::Relaxed);
    node.vendor_id.store(function.vendor_id as u32, Ordering::Relaxed);
    node.device_id.store(function.device_id as u32, Ordering::Relaxed);
    node.class_code.store(function.class_code as u32, Ordering::Relaxed);
    node.subclass.store(function.subclass as u32, Ordering::Relaxed);
    node.programming_interface.store(function.programming_interface as u32, Ordering::Relaxed);
    node.last_status.store(0, Ordering::Relaxed);
    node.start_attempts.store(0, Ordering::Relaxed);
    node.is_root.store(false, Ordering::Relaxed);
    node.device_object.store(0, Ordering::Relaxed);
    node.service_name_len.store(0, Ordering::Relaxed);
    node.state.store(PNP_DEVICE_ENUMERATED, Ordering::Release);
    Ok(index as u32)
}

/// Import the bounded inventory published by pci.sys through its versioned
/// bus interface. Each returned function is validated again before a devnode
/// is published. Parent/bridge relationships are added by the topology pass.
pub unsafe fn import_pci_inventory(
    interface: &crate::nt_abi::bus::PCI_BUS_INTERFACE,
) -> Result<usize, PnpError> {
    use crate::nt_abi::ntstatus::{STATUS_SUCCESS, STATUS_NOT_FOUND};
    use crate::nt_abi::bus::{PCI_BRIDGE_INFORMATION, PCI_DEVICE_INFORMATION};
    let header = &interface.header;
    if (header.size as usize) < core::mem::size_of::<crate::nt_abi::bus::PCI_BUS_INTERFACE>()
        || header.version as u32 != crate::nt_abi::bus::PCI_BUS_INTERFACE_VERSION
        || header.context.is_null() {
        return Err(PnpError::InvalidFunction);
    }
    let query_counts = interface.query_counts.ok_or(PnpError::InvalidFunction)?;
    let query_function = interface.query_function.ok_or(PnpError::InvalidFunction)?;
    let query_bridge = interface.query_bridge.ok_or(PnpError::InvalidFunction)?;
    let mut functions = 0u32;
    let mut bridges = 0u32;
    let mut xhci = 0u32;
    let status = unsafe { query_counts(header.context, &mut functions, &mut bridges, &mut xhci) };
    if status != STATUS_SUCCESS { return Err(PnpError::InvalidState); }
    if functions as usize > MAX_PNP_DEVICES || bridges as usize > MAX_PNP_DEVICES {
        return Err(PnpError::TableFull);
    }

    // Fetch topology from pci.sys before publishing any devnodes. This keeps
    // bridge discovery in the bus driver and lets the kernel PnP manager build
    // the parent tree without touching PCI configuration space.
    let mut topology = [PCI_BRIDGE_INFORMATION::default(); MAX_PNP_DEVICES];
    for index in 0..bridges {
        let status = unsafe { query_bridge(header.context, index, &mut topology[index as usize]) };
        if status == STATUS_NOT_FOUND { return Err(PnpError::InvalidState); }
        if status != STATUS_SUCCESS { return Err(PnpError::InvalidState); }
        let bridge = topology[index as usize];
        if bridge.device >= 32 || bridge.function >= 8
            || bridge.secondary_bus <= bridge.primary_bus
            || bridge.subordinate_bus < bridge.secondary_bus {
            return Err(PnpError::InvalidFunction);
        }
    }

    let root_index = register_root_device()?;
    let mut imported = 0usize;
    for index in 0..functions {
        let mut information = PCI_DEVICE_INFORMATION::default();
        let status = unsafe { query_function(header.context, index, &mut information) };
        if status == STATUS_NOT_FOUND { break; }
        if status != STATUS_SUCCESS { return Err(PnpError::InvalidState); }

        // Select the narrowest containing bridge range. Equal-width matches
        // are ambiguous and rejected rather than creating a false parent.
        let mut best: Option<PCI_BRIDGE_INFORMATION> = None;
        let mut ambiguous = false;
        for bridge in topology[..bridges as usize].iter().copied().filter(|bridge|
            bridge.segment == information.segment
                && bridge.secondary_bus <= information.bus
                && information.bus <= bridge.subordinate_bus) {
            match best {
                None => { best = Some(bridge); ambiguous = false; }
                Some(old) => {
                    let old_width = old.subordinate_bus as u16 - old.secondary_bus as u16;
                    let new_width = bridge.subordinate_bus as u16 - bridge.secondary_bus as u16;
                    if new_width < old_width { best = Some(bridge); ambiguous = false; }
                    else if new_width == old_width { ambiguous = true; }
                }
            }
        }
        if ambiguous { return Err(PnpError::InvalidFunction); }
        let parent = if let Some(bridge) = best {
            let parent = find_pnp_pci_function(
                bridge.segment, bridge.primary_bus, bridge.device, bridge.function,
            ).ok_or(PnpError::InvalidState)?;
            Some(parent)
        } else {
            Some(root_index)
        };
        match register_pci_information(&information, parent) {
            Ok(_) => imported += 1,
            Err(PnpError::DuplicateDevice) => {},
            Err(error) => return Err(error),
        }
    }
    Ok(imported)
}

/// Complete the bus-to-PnP discovery handoff and resolve registry-installed
/// services for all newly visible PCI devnodes. Loading/start is a separate
/// operation and is never implied by a successful hardware-ID match.
pub unsafe fn import_pci_inventory_and_match(
    interface:&crate::nt_abi::bus::PCI_BUS_INTERFACE,
    hive:&crate::ntoskrnl::cm::hive::Hive<'_>,
)->Result<(usize,usize),PnpError>{
    let imported=unsafe{import_pci_inventory(interface)}?;
    let matched=match_registry_services(hive)?;
    Ok((imported,matched))
}

/// Apply the PnP lifecycle transition only after the relevant operation has
/// completed. A failed start is recorded as FAILED; it can never be reported as
/// STARTED. The caller is responsible for unwinding hardware resources before
/// removing a failed device, matching the Windows PnP start/remove contract.
pub fn transition_pnp_device(index: u32, next: u32, status: i32) -> Result<(), PnpError> {
    let _guard = lock_pnp_table();
    let node = PNP_DEVICES.get(index as usize).ok_or(PnpError::InvalidState)?;
    let current = node.state.load(Ordering::Acquire);
    if !valid_pnp_transition(current, next) { return Err(PnpError::InvalidTransition); }
    if current == PNP_DEVICE_RESOURCES_ASSIGNED && next == PNP_DEVICE_STARTED && status < 0 {
        node.last_status.store(status, Ordering::Relaxed);
        node.start_attempts.fetch_add(1, Ordering::Relaxed);
        node.state.store(PNP_DEVICE_FAILED, Ordering::Release);
        return Ok(());
    }
    if current == PNP_DEVICE_RESOURCES_ASSIGNED && next == PNP_DEVICE_STARTED {
        node.start_attempts.fetch_add(1, Ordering::Relaxed);
    }
    node.last_status.store(status, Ordering::Relaxed);
    let release_device = if next == PNP_DEVICE_REMOVED {
        node.device_object.swap(0, Ordering::AcqRel)
    } else { 0 };
    node.state.store(next, Ordering::Release);
    drop(_guard);
    if release_device != 0 {
        unsafe { crate::io::io_dereference_device_object(release_device as *mut crate::io::DeviceObject); }
    }
    Ok(())
}

pub fn query_pnp_device(index: u32) -> Option<PnpDeviceSnapshot> {
    let _guard = lock_pnp_table();
    let node = PNP_DEVICES.get(index as usize)?;
    let state = node.state.load(Ordering::Acquire);
    if state == PNP_DEVICE_EMPTY { return None; }
    Some(PnpDeviceSnapshot {
        index, state, parent: node.parent.load(Ordering::Relaxed),
        segment: node.segment.load(Ordering::Relaxed) as u16,
        bus: node.bus.load(Ordering::Relaxed) as u8,
        device: node.device.load(Ordering::Relaxed) as u8,
        function: node.function.load(Ordering::Relaxed) as u8,
        vendor_id: node.vendor_id.load(Ordering::Relaxed) as u16,
        device_id: node.device_id.load(Ordering::Relaxed) as u16,
        class_code: node.class_code.load(Ordering::Relaxed) as u8,
        subclass: node.subclass.load(Ordering::Relaxed) as u8,
        programming_interface: node.programming_interface.load(Ordering::Relaxed) as u8,
        last_status: node.last_status.load(Ordering::Relaxed),
        start_attempts: node.start_attempts.load(Ordering::Relaxed),
        is_root: node.is_root.load(Ordering::Relaxed),
        device_object: node.device_object.load(Ordering::Relaxed),
    })
}


#[cfg(test)]
mod pnp_tree_tests {
    use super::*;

    fn sample_function() -> cognacos_pci::PciFunction {
        cognacos_pci::PciFunction {
            segment: 2, bus: 7, device: 5, function: 0,
            header: cognacos_pci::PciFunctionHeader {
                vendor_id: 0x8086, device_id: 0x1234, command: 0,
                revision_id: 1, programming_interface: 0x30,
                subclass: 0x03, class_code: 0x0c, header_type: 0,
            },
        }
    }

    #[test]
    fn pnp_start_failure_is_never_published_as_started() {
        let index = register_pci_function(&sample_function(), None).unwrap();
        assert_eq!(query_pnp_device(index).unwrap().state, PNP_DEVICE_ENUMERATED);
        assert_eq!(transition_pnp_device(index, PNP_DEVICE_STARTED, 0), Err(PnpError::InvalidTransition));
        transition_pnp_device(index, PNP_DEVICE_RESOURCES_ASSIGNED, 0).unwrap();
        transition_pnp_device(index, PNP_DEVICE_STARTED, 0xC000_0001u32 as i32).unwrap();
        let failed = query_pnp_device(index).unwrap();
        assert_eq!(failed.state, PNP_DEVICE_FAILED);
        assert_eq!(failed.last_status, 0xC000_0001u32 as i32);
        assert_eq!(failed.start_attempts, 1);
        transition_pnp_device(index, PNP_DEVICE_REMOVED, 0).unwrap();
    }

    #[test]
    fn root_devnode_and_pci_ids_are_windows_multisz() {
        let root = register_root_device().unwrap();
        let snapshot = query_pnp_device(root).unwrap();
        assert!(snapshot.is_root);
        let mut hardware = [0u16; 64];
        let mut compatible = [0u16; 64];
        let lengths = query_pnp_device_ids(root, Some(&mut hardware), Some(&mut compatible)).unwrap();
        assert_eq!(lengths, (15, 14));
        let expected_root_hw = [82u16, 79, 79, 84, 92, 67, 79, 71, 78, 65, 67, 79, 83];
        assert_eq!(&hardware[..13], &expected_root_hw);
        assert_eq!(hardware[13..15], [0, 0]);
        let expected_root_compatible = [82u16, 79, 79, 84, 92, 80, 78, 80, 48, 65, 48, 56];
        assert_eq!(&compatible[..12], &expected_root_compatible);
        assert_eq!(compatible[12..14], [0, 0]);

        let mut function = sample_function();
        function.device = 21;
        let child = register_pci_function(&function, Some(root)).unwrap();
        let mut hw = [0u16; 64];
        let mut compat = [0u16; 64];
        let (hw_len, compat_len) = query_pnp_device_ids(child, Some(&mut hw), Some(&mut compat)).unwrap();
        assert_eq!(hw_len, 23);
        assert_eq!(compat_len, 28);
        let expected_hardware = [80u16, 67, 73, 92, 86, 69, 78, 95, 56, 48, 56, 54, 38, 68, 69, 86, 95, 49, 50, 51, 52];
        assert_eq!(&hw[..21], &expected_hardware);
        let expected_compatible = [80u16, 67, 73, 92, 67, 67, 95, 48, 67, 48, 51, 51, 48];
        assert_eq!(&compat[..13], &expected_compatible);
        let expected_class = [80u16, 67, 73, 92, 67, 67, 95, 48, 67, 48, 51];
        assert_eq!(&compat[15..26], &expected_class);
    }

    #[test]
    fn devnode_holds_and_releases_its_pdo_reference() {
        let mut driver: crate::io::DriverObject = unsafe { core::mem::zeroed() };
        let mut device: *mut crate::io::DeviceObject = core::ptr::null_mut();
        assert_eq!(unsafe { crate::io::io_create_device(
            &mut driver, 0, core::ptr::null_mut(), 0x22, 0, 0, &mut device,
        ) }, crate::nt_abi::ntstatus::STATUS_SUCCESS);
        let root = register_root_device().unwrap();
        assert_eq!(bind_pnp_device_object(root, device), Ok(()));
        assert_eq!(query_pnp_device(root).unwrap().device_object, device as usize);
        assert_eq!(transition_pnp_device(root, PNP_DEVICE_REMOVED, 0), Ok(()));
        assert_eq!(query_pnp_device(root).unwrap().device_object, 0);
        unsafe { crate::io::io_delete_device(device); }
    }

    #[test]
    fn pnp_identity_includes_pci_segment_and_duplicate_bdfs_are_rejected() {
        let mut first = sample_function();
        first.device = 6;
        let index = register_pci_function(&first, None).unwrap();
        assert_eq!(register_pci_function(&first, None), Err(PnpError::DuplicateDevice));
        let mut other_segment = first;
        other_segment.segment = 3;
        let other_index = register_pci_function(&other_segment, None).unwrap();
        assert_ne!(index, other_index);
        transition_pnp_device(index, PNP_DEVICE_REMOVED, 0).unwrap();
        transition_pnp_device(other_index, PNP_DEVICE_REMOVED, 0).unwrap();
    }
}

/// Find an already enumerated PCI devnode by its complete segment/BDF identity.
/// Removed nodes are excluded so a restarted device cannot accidentally bind to
/// a stale lifecycle record.
pub fn find_pnp_pci_function(segment:u16,bus:u8,device:u8,function:u8)->Option<u32>{
    let _guard=lock_pnp_table();
    PNP_DEVICES.iter().enumerate().find_map(|(i,node)|{
        let state=node.state.load(Ordering::Acquire);
        if state!=PNP_DEVICE_EMPTY && state!=PNP_DEVICE_REMOVED
            && node.segment.load(Ordering::Relaxed)==segment as u32
            && node.bus.load(Ordering::Relaxed)==bus as u32
            && node.device.load(Ordering::Relaxed)==device as u32
            && node.function.load(Ordering::Relaxed)==function as u32 {
            Some(i as u32)
        } else { None }
    })
}

/// Withdraw a failed xHCI resource publication after the caller has disabled
/// DMA and unmapped its MMIO range. The static object can then be republished
/// only after a fresh, validated resource-discovery pass.
pub fn withdraw_xhci() {
    XHCI_PCI_DEVICE.ready.store(false,Ordering::Release);
    XHCI_PCI_DEVICE.pdo_device_object.store(0,Ordering::Relaxed);
    XHCI_PCI_DEVICE.state.store(0,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_virtual.store(0,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_length.store(0,Ordering::Relaxed);
    XHCI_PCI_DEVICE.bar_physical.store(0,Ordering::Relaxed);
}

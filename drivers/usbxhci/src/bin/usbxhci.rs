#![no_std]
#![no_main]

use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
use cognacos_nt_abi::{types::PVOID, io::{DRIVER_OBJECT, DEVICE_OBJECT, IRP}};
use cognacos_nt_abi::object::UNICODE_STRING;
use cognacos_usbxhci::{self as xhci, Capabilities, DmaAllocation, XhciPlatform, XhciRegisterIo, XhciRuntimeResources};

type InterruptServiceRoutine = unsafe extern "win64" fn(*mut c_void,*mut c_void)->u8;
type DpcRoutine = unsafe extern "win64" fn(*mut c_void,*mut c_void,*mut c_void,*mut c_void);
type CreateDevice = unsafe extern "win64" fn(*mut DRIVER_OBJECT,u32,*mut UNICODE_STRING,u32,u32,u8,*mut *mut DEVICE_OBJECT)->i32;
type DeleteDevice = unsafe extern "win64" fn(*mut DEVICE_OBJECT);
type GetDmaAdapter = unsafe extern "win64" fn(*mut DEVICE_OBJECT,*const c_void,*mut u32)->*mut c_void;
type MapIoSpace = unsafe extern "win64" fn(i64,u64,i32)->*mut c_void;
type UnmapIoSpace = unsafe extern "win64" fn(*mut c_void,u64);
type AllocateCommonBuffer = unsafe extern "win64" fn(*mut c_void,u32,*mut i64,u8)->*mut c_void;
type FreeCommonBuffer = unsafe extern "win64" fn(*mut c_void,u32,i64,*mut c_void,u8);
type GetBusData = unsafe extern "win64" fn(u32,u32,u32,*mut u8,u32,u32)->u32;
type SetBusData = unsafe extern "win64" fn(u32,u32,u32,*const u8,u32,u32)->u32;
type ConnectInterrupt = unsafe extern "win64" fn(*mut *mut c_void,Option<InterruptServiceRoutine>,*mut c_void,*mut c_void,u32,u8,u8,u32,u8,u64,u8)->i32;
type DisconnectInterrupt = unsafe extern "win64" fn(*mut c_void);
type InitializeDpc = unsafe extern "win64" fn(*mut c_void,Option<DpcRoutine>,*mut c_void);
type InsertQueueDpc = unsafe extern "win64" fn(*mut c_void,*mut c_void,*mut c_void)->u8;
type GetCurrentIrpStack = unsafe extern "win64" fn(*mut IRP)->*mut cognacos_nt_abi::io::IO_STACK_LOCATION;
type CompleteRequest = unsafe extern "win64" fn(*mut IRP,i8);

unsafe extern "win64" {
    fn IoCreateDevice(driver:*mut DRIVER_OBJECT,extension_size:u32,name:*mut UNICODE_STRING,device_type:u32,characteristics:u32,exclusive:u8,output:*mut *mut DEVICE_OBJECT)->i32;
    fn IoDeleteDevice(device:*mut DEVICE_OBJECT);
    fn IoGetDmaAdapter(device:*mut DEVICE_OBJECT,description:*const c_void,map_registers:*mut u32)->*mut c_void;
    fn MmMapIoSpace(physical_address:i64,length:u64,cache_type:i32)->*mut c_void;
    fn MmUnmapIoSpace(base:*mut c_void,length:u64);
    fn HalAllocateCommonBuffer(adapter:*mut c_void,length:u32,logical:*mut i64,cache_enabled:u8)->*mut c_void;
    fn HalFreeCommonBuffer(adapter:*mut c_void,length:u32,logical:i64,virtual_address:*mut c_void,cache_enabled:u8);
    fn HalGetBusDataByOffset(bus_type:u32,bus:u32,slot:u32,buffer:*mut u8,offset:u32,length:u32)->u32;
    fn HalSetBusDataByOffset(bus_type:u32,bus:u32,slot:u32,buffer:*const u8,offset:u32,length:u32)->u32;
    fn IoConnectInterrupt(interrupt:*mut *mut c_void,routine:Option<InterruptServiceRoutine>,context:*mut c_void,spin_lock:*mut c_void,vector:u32,irql:u8,synchronize_irql:u8,mode:u32,share:u8,affinity:u64,floating_save:u8)->i32;
    fn IoDisconnectInterrupt(interrupt:*mut c_void);
    fn KeInitializeDpc(dpc:*mut c_void,routine:Option<DpcRoutine>,context:*mut c_void);
    fn KeInsertQueueDpc(dpc:*mut c_void,arg1:*mut c_void,arg2:*mut c_void)->u8;
    fn IoGetCurrentIrpStackLocation(irp:*mut IRP)->*mut cognacos_nt_abi::io::IO_STACK_LOCATION;
    fn IoCompleteRequest(irp:*mut IRP,priority_boost:i8);
}

core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.macro thunk name, slot
.global \name
.type \name,@function
\name:
    jmp QWORD PTR [rip + \slot]
.size \name, .-\name
.endm
thunk IoCreateDevice, __imp_ntoskrnl_IoCreateDevice
thunk IoDeleteDevice, __imp_ntoskrnl_IoDeleteDevice
thunk IoGetDmaAdapter, __imp_ntoskrnl_IoGetDmaAdapter
thunk MmMapIoSpace, __imp_ntoskrnl_MmMapIoSpace
thunk MmUnmapIoSpace, __imp_ntoskrnl_MmUnmapIoSpace
thunk IoConnectInterrupt, __imp_ntoskrnl_IoConnectInterrupt
thunk IoDisconnectInterrupt, __imp_ntoskrnl_IoDisconnectInterrupt
thunk KeInitializeDpc, __imp_ntoskrnl_KeInitializeDpc
thunk KeInsertQueueDpc, __imp_ntoskrnl_KeInsertQueueDpc
thunk IoGetCurrentIrpStackLocation, __imp_ntoskrnl_IoGetCurrentIrpStackLocation
thunk IoCompleteRequest, __imp_ntoskrnl_IoCompleteRequest
thunk HalAllocateCommonBuffer, __imp_hal_HalAllocateCommonBuffer
thunk HalFreeCommonBuffer, __imp_hal_HalFreeCommonBuffer
thunk HalGetBusDataByOffset, __imp_hal_HalGetBusDataByOffset
thunk HalSetBusDataByOffset, __imp_hal_HalSetBusDataByOffset
.section .data.imports.ntoskrnl,"aw",@progbits
.p2align 3
.macro iat name
.global __imp_ntoskrnl_\name
.type __imp_ntoskrnl_\name,@object
__imp_ntoskrnl_\name: .quad 0
.size __imp_ntoskrnl_\name,8
.endm
iat IoCreateDevice
iat IoDeleteDevice
iat IoGetDmaAdapter
iat MmMapIoSpace
iat MmUnmapIoSpace
iat IoConnectInterrupt
iat IoDisconnectInterrupt
iat KeInitializeDpc
iat KeInsertQueueDpc
iat IoGetCurrentIrpStackLocation
iat IoCompleteRequest
.section .data.imports.hal,"aw",@progbits
.p2align 3
.macro hiat name
.global __imp_hal_\name
.type __imp_hal_\name,@object
__imp_hal_\name: .quad 0
.size __imp_hal_\name,8
.endm
hiat HalAllocateCommonBuffer
hiat HalFreeCommonBuffer
hiat HalGetBusDataByOffset
hiat HalSetBusDataByOffset
"#);

const EXTENSION_MAGIC: u64 = 0x434f_474e_4143_5848; // "COGNACXH"
const DRIVER_FAILED: i32 = 0xC0000001u32 as i32;
const DRIVER_INVALID_PARAMETER: i32 = 0xC000000Du32 as i32;
const DRIVER_NOT_SUPPORTED: i32 = 0xC00000BBu32 as i32;
const DRIVER_INSUFFICIENT_RESOURCES: i32 = 0xC000009Au32 as i32;
const DRIVER_DEVICE_NOT_READY: i32 = 0xC00000A3u32 as i32;
const DRIVER_IO_TIMEOUT: i32 = 0xC00000B5u32 as i32;
const POLL_BUDGET: u32 = 2_000_000;
const MMIO_LIMIT: u64 = 0x10000;
const USBSTS: u32 = 0x44;
const USBSTS_INTERRUPT: u32 = 1 << 3;

/// Kernel-populated, versioned xHCI handoff block stored in DriverObject.DriverExtension.
/// The kernel must only publish this after PCI BAR mapping and safe DMA/IRQ services exist.
/// The MMIO base is a mapped virtual address, never a PCI physical BAR address.
#[repr(C)]
pub struct XhciDriverExtension {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    pub mmio_base: u64,
    pub mmio_length: u64,
    /// Stable PnP device-object address registered with the IOMMU requester table.
    pub device_object: u64,
    pub interrupt_vector: u32,
    pub max_slots: u32,
    pub hal: *const HalServiceTable,
    pub device: *const DeviceServiceTable,
    pub kernel_io: *const KernelIoServiceTable,
    pub fdo_out:*mut u64,
    pub ready: AtomicBool,
    pub interrupt_requested: AtomicBool,
    pub event_count: AtomicU64,
    pub port_change_count: AtomicU64,
    pub command_completion_count: AtomicU64,
    pub transfer_completion_count: AtomicU64,
    runtime_lock: AtomicBool,
    event_sink:core::sync::atomic::AtomicU64,
    event_context:core::sync::atomic::AtomicU64,
    operational_offset:core::sync::atomic::AtomicU32,
    port_count:core::sync::atomic::AtomicU32,
    runtime: UnsafeCell<MaybeUninit<XhciRuntimeResources>>,
}
unsafe impl Sync for XhciDriverExtension {}

type DriverDispatch = unsafe extern "win64" fn(*mut c_void,*mut c_void)->i32;
#[repr(C)]
struct DriverObject {
    object_type:i16,size:i16,device_object:*mut c_void,flags:u32,
    driver_start:*mut c_void,driver_size:u32,driver_section:*mut c_void,
    driver_extension:*mut XhciDriverExtension,
    driver_name:cognacos_nt_abi::object::UNICODE_STRING,
    hardware_database:*mut cognacos_nt_abi::object::UNICODE_STRING,
    fast_io_dispatch:*mut c_void,
    driver_init:*mut c_void,driver_start_io:*mut c_void,
    driver_unload:Option<unsafe extern "win64" fn(*mut c_void)>,
    major_function:[Option<DriverDispatch>;28],
}

struct Platform<'a> { ext: &'a XhciDriverExtension, hal: &'a HalServiceTable, dev: &'a DeviceServiceTable }
impl XhciRegisterIo for Platform<'_> {
    fn read32(&mut self, offset: u32) -> Result<u32, ()> {
        let end = (offset as u64).checked_add(4).ok_or(())?;
        if end > self.ext.mmio_length || end > MMIO_LIMIT { return Err(()); }
        let read = self.hal.read_mmio_u32.ok_or(())?;
        let mut value = 0u32;
        let status = unsafe { read(self.ext.mmio_base.checked_add(offset as u64).ok_or(())?, &mut value) };
        if status.code == 0 { Ok(value) } else { Err(()) }
    }
    fn write32(&mut self, offset: u32, value: u32) -> Result<(), ()> {
        let end = (offset as u64).checked_add(4).ok_or(())?;
        if end > self.ext.mmio_length || end > MMIO_LIMIT { return Err(()); }
        let write = self.hal.write_mmio_u32.ok_or(())?;
        let status = unsafe { write(self.ext.mmio_base.checked_add(offset as u64).ok_or(())?, value) };
        if status.code == 0 { Ok(()) } else { Err(()) }
    }
    fn set_bus_master(&mut self, enabled: bool) -> Result<(), ()> {
        let set = self.dev.set_bus_master.ok_or(())?;
        let status = unsafe { set(self.ext.device_object, enabled) };
        if status.code == 0 { Ok(()) } else { Err(()) }
    }
}
impl XhciPlatform for Platform<'_> {
    fn dma_mapping_ready(&self, allocation: DmaAllocation) -> bool {
        // Validate the exact live mapping against this controller's requester
        // domain. A successful allocation alone does not prove that the IOVA
        // is currently mapped and pinned for this device.
        let Some(validate) = self.dev.validate_dma else { return false; };
        unsafe {
            validate(
                self.ext.device_object,
                allocation.cpu_address,
                allocation.device_address,
                allocation.size,
            ).code == 0
        }
    }

    fn allocate_dma(&mut self, size: u64, alignment: u64, maximum: u64) -> Result<DmaAllocation, ()> {
        let allocate = self.dev.allocate_dma.ok_or(())?;
        let mut cpu = 0u64;
        let mut device = 0u64;
        let status = unsafe { allocate(self.ext.device_object, size, alignment, maximum, 7, &mut cpu, &mut device) };
        if status.code != 0 || cpu == 0 || device == 0 { return Err(()); }
        Ok(DmaAllocation { cpu_address: cpu, device_address: device, size })
    }
    fn free_dma(&mut self, allocation: DmaAllocation) -> Result<(), ()> {
        // A missing release service or failed HAL release means the allocation
        // may still be mapped/reachable by the controller. Propagate failure so
        // the xHCI core can fail closed instead of recycling potentially live DMA.
        let free = self.dev.free_dma.ok_or(())?;
        let status = unsafe {
            free(
                self.ext.device_object,
                allocation.cpu_address,
                allocation.device_address,
                allocation.size,
            )
        };
        if status.code == 0 { Ok(()) } else { Err(()) }
    }
    fn register_interrupt(&mut self) -> Result<(), ()> {
        let register = self.dev.register_interrupt.ok_or(())?;
        let status = unsafe { register(self.ext.interrupt_vector, self.ext as *const _ as u64,
            Some(xhci_isr), Some(xhci_dpc)) };
        if status.code == 0 { Ok(()) } else { Err(()) }
    }
    fn unregister_interrupt(&mut self) {
        if let Some(unregister) = self.dev.unregister_interrupt {
            let _ = unsafe { unregister(self.ext.interrupt_vector, self.ext as *const _ as u64) };
        }
    }
    fn set_interrupt_enabled(&mut self, enabled: bool) -> Result<(), ()> {
        if enabled && !self.ext.ready.load(Ordering::Acquire) {
            self.ext.interrupt_requested.store(true, Ordering::Release);
            return Ok(());
        }
        let set = self.dev.set_interrupt_enabled.ok_or(())?;
        let status = unsafe { set(self.ext.interrupt_vector, enabled) };
        if status.code == 0 { Ok(()) } else { Err(()) }
    }

}

static XHCI_FDO:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static XHCI_EXTENSION:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
unsafe extern "win64" fn xhci_dispatch(_device:*mut c_void,irp:*mut c_void)->i32{
    const STATUS_SUCCESS:i32=0;
    const STATUS_INVALID_PARAMETER:i32=0xC000000D_u32 as i32;
    const STATUS_INVALID_DEVICE_REQUEST:i32=0xC0000010_u32 as i32;
    if irp.is_null(){return STATUS_INVALID_PARAMETER}
    let ext_ptr=XHCI_EXTENSION.load(Ordering::Acquire) as *const XhciDriverExtension;
    if ext_ptr.is_null(){return STATUS_INVALID_DEVICE_REQUEST}
    let ext=unsafe{&*ext_ptr};
    let Some(io)= (unsafe{ext.kernel_io.as_ref()}) else{return STATUS_INVALID_DEVICE_REQUEST};
    let Some(get_stack)=io.current_stack_location else{return STATUS_INVALID_DEVICE_REQUEST};
    let stack=unsafe{get_stack(irp)};
    if stack.is_null(){return STATUS_INVALID_PARAMETER}
    let major=unsafe{core::ptr::read(stack.cast::<u8>())};
    let minor=unsafe{core::ptr::read(stack.cast::<u8>().add(1))};
    let status=match major{
        0x1b=>match minor{0x00|0x03|0x04|0x05|0x06|0x17|0x02=>STATUS_SUCCESS,_=>STATUS_INVALID_DEVICE_REQUEST},
        0x16=>STATUS_SUCCESS,
        _=>STATUS_INVALID_DEVICE_REQUEST,
    };
    if let Some(complete)=io.complete_irp_status{unsafe{complete(irp,status,0,0);}}
    status
}

extern "C" fn xhci_isr(context: u64) -> bool {
    if context == 0 { return false; }
    let ext = unsafe { &*(context as *const XhciDriverExtension) };
    if !ext.ready.load(Ordering::Acquire) { return false; }
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return false; };
    let Some(read) = hal.read_mmio_u32 else { return false; };
    let Some(write) = hal.write_mmio_u32 else { return false; };
    let status_address = match ext.mmio_base.checked_add(USBSTS as u64) { Some(v) => v, None => return false };
    let mut status = 0u32;
    if unsafe { read(status_address, &mut status) }.code != 0 || status == u32::MAX { return false; }
    if status & USBSTS_INTERRUPT == 0 { return false; }
    // USBSTS is write-one-to-clear. DPC drains the DMA event ring after the ISR acknowledges it.
    let _ = unsafe { write(status_address, status & 0x3f) };
    true
}
extern "C" fn xhci_dpc(context: u64) {
    if context == 0 { return; }
    let ext = unsafe { &*(context as *const XhciDriverExtension) };
    if !ext.ready.load(Ordering::Acquire) || ext.runtime_lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { return; }
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else { ext.runtime_lock.store(false, Ordering::Release); return; };
    let Some(dev) = (unsafe { ext.device.as_ref() }) else { ext.runtime_lock.store(false, Ordering::Release); return; };
    let runtime = unsafe { &mut *ext.runtime.get() };
    let resources = unsafe { runtime.assume_init_mut() };
    let mut platform = Platform { ext, hal, dev };
    let caps = match read_capabilities(&mut platform, ext.mmio_length) {
        Some(c) => c,
        None => { ext.runtime_lock.store(false, Ordering::Release); return; }
    };
    let _ = xhci::drain_event_ring(&mut platform, &caps, resources, 128, |event| {
        ext.event_count.fetch_add(1, Ordering::Relaxed);
        match event.trb_type() as u32 {
            xhci::TRB_TYPE_PORT_STATUS_CHANGE => { ext.port_change_count.fetch_add(1, Ordering::Relaxed); }
            xhci::TRB_TYPE_COMMAND_COMPLETION => { ext.command_completion_count.fetch_add(1, Ordering::Relaxed); }
            xhci::TRB_TYPE_TRANSFER_EVENT => { ext.transfer_completion_count.fetch_add(1, Ordering::Relaxed); }
            _ => {}
        }
        let sink=ext.event_sink.load(Ordering::Acquire);
        if sink!=0 {
            let context=ext.event_context.load(Ordering::Acquire);
            let callback:unsafe extern "win64" fn(u64,*const xhci::Trb)=unsafe{core::mem::transmute(sink as usize)};
            unsafe{callback(context,&event);}
        }
    });
    ext.runtime_lock.store(false, Ordering::Release);
}

fn read_capabilities(io: &mut impl XhciRegisterIo, length: u64) -> Option<Capabilities> {
    if length < 0x20 { return None; }
    let mut snapshot = [0u8; 0x40];
    for offset in (0..0x20u32).step_by(4) {
        let value = io.read32(offset).ok()?;
        snapshot[offset as usize..offset as usize + 4].copy_from_slice(&value.to_le_bytes());
    }
    // PAGESIZE is an operational register at CAPLENGTH + 0x08; parser intentionally leaves it zero.
    let mut caps = xhci::parse_capabilities(&snapshot).ok()?;
    let pagesize = io.read32(caps.cap_length as u32 + 8).ok()?;
    if pagesize & 1 == 0 { return None; }
    caps.page_size_mask = pagesize;
    Some(caps)
}

/// Windows-native DriverEntry: validate the kernel handoff, parse real controller
/// capability registers, allocate IOMMU-backed DMA structures, reset/start xHCI,
/// and register an ISR/DPC event-ring path. Missing services fail closed.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return DRIVER_INVALID_PARAMETER; }
    let driver_object = unsafe { &mut *(driver as *mut DriverObject) };
    let Some(ext) = (unsafe { driver_object.driver_extension.as_ref() }) else { return DRIVER_NOT_SUPPORTED; };
    XHCI_EXTENSION.store(ext as *const _ as u64,Ordering::Release);
    if ext.magic != EXTENSION_MAGIC || ext.version != 1
        || (ext.size as usize) < core::mem::size_of::<XhciDriverExtension>()
        || ext.mmio_base == 0 || ext.mmio_base & 3 != 0
        || ext.mmio_length < 0x1000 || ext.mmio_length > MMIO_LIMIT
        || ext.device_object == 0 || ext.interrupt_vector == 0 || ext.max_slots == 0 || ext.max_slots > 64
        || ext.hal.is_null() || ext.device.is_null() || ext.kernel_io.is_null() || ext.fdo_out.is_null() { return DRIVER_INVALID_PARAMETER; }
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return DRIVER_INVALID_PARAMETER; };
    let Some(dev) = (unsafe { ext.device.as_ref() }) else { return DRIVER_INVALID_PARAMETER; };
    let Some(kernel_io) = (unsafe { ext.kernel_io.as_ref() }) else { return DRIVER_INVALID_PARAMETER; };
    if !hal.compatible() || hal.version != HAL_CONTRACT_VERSION
        || !dev.compatible() || dev.version != DEVICE_SERVICES_VERSION
        || hal.read_mmio_u32.is_none() || hal.write_mmio_u32.is_none()
        || !dev.has_dma() || !dev.has_interrupts() || !dev.has_bus_master()
        || !kernel_io.compatible() { return DRIVER_NOT_SUPPORTED; }
    let mut platform = Platform { ext, hal, dev };
    let Some(caps) = read_capabilities(&mut platform, ext.mmio_length) else { return DRIVER_NOT_SUPPORTED; };
    ext.operational_offset.store(caps.cap_length as u32,Ordering::Release);
    ext.port_count.store(caps.max_ports as u32,Ordering::Release);
    let slots = (ext.max_slots as u8).min(caps.max_slots);
    let resources = match xhci::start_controller(&mut platform, &caps, slots, POLL_BUDGET) {
        Ok(resources) => resources,
        Err(xhci::RuntimeStartError::AllocationFailed) => return DRIVER_INSUFFICIENT_RESOURCES,
        Err(xhci::RuntimeStartError::InvalidAllocation) => return DRIVER_INVALID_PARAMETER,
        // start_controller returns this before allocating DMA resources. Do not
        // continue controller initialization when bus mastering cannot be cleared.
        Err(xhci::RuntimeStartError::BusMasterDisableFailed) => return DRIVER_DEVICE_NOT_READY,
        Err(xhci::RuntimeStartError::InterruptRegistrationFailed) => return DRIVER_NOT_SUPPORTED,
        Err(xhci::RuntimeStartError::InterruptEnableFailed) => return DRIVER_DEVICE_NOT_READY,
        Err(xhci::RuntimeStartError::Controller(error)) => return match error {
            xhci::ControllerInitError::ResetTimeout
            | xhci::ControllerInitError::ControllerDidNotHalt
            | xhci::ControllerInitError::ControllerDidNotStart => DRIVER_IO_TIMEOUT,
            xhci::ControllerInitError::BusMasterEnableFailed
            | xhci::ControllerInitError::RegisterIoFailure => DRIVER_DEVICE_NOT_READY,
            xhci::ControllerInitError::UnsupportedPageSize
            | xhci::ControllerInitError::InterruptNotRegistered => DRIVER_NOT_SUPPORTED,
            xhci::ControllerInitError::InvalidCapabilities
            | xhci::ControllerInitError::InvalidDmaConfiguration => DRIVER_INVALID_PARAMETER,
        },
    };
    // Bind a real kernel DEVICE_OBJECT to this DriverObject and publish
    // the dispatch table consumed by IoCallDriver. The DMA requester remains
    // the PCI PDO identity supplied separately in ext.device_object.
    static DEVICE_NAME:[u16;19]=[
        92,68,101,118,105,99,101,92,67,111,103,110,97,99,88,104,99,105,0
    ];
    let mut counted_name=cognacos_nt_abi::object::UNICODE_STRING{
        length:36,maximum_length:38,buffer:DEVICE_NAME.as_ptr() as *mut u16
    };
    let mut fdo=core::ptr::null_mut::<c_void>();
    for entry in driver_object.major_function.iter_mut(){*entry=Some(xhci_dispatch);}
    let Some(create_device)=kernel_io.create_device else{return DRIVER_NOT_SUPPORTED};
    let create_status=unsafe{create_device(
        driver,0,&mut counted_name,4,0,0,&mut fdo
    )};
    if create_status<0||fdo.is_null(){return create_status};
    XHCI_FDO.store(fdo as u64,Ordering::Release);
    unsafe{ext.fdo_out.write(fdo as u64);}
    unsafe { (*ext.runtime.get()).write(resources); }
    ext.ready.store(true, Ordering::Release);
    if ext.interrupt_requested.swap(false, Ordering::AcqRel) {
        if platform.set_interrupt_enabled(true).is_err() {
            ext.ready.store(false, Ordering::Release);
            let runtime = unsafe { (*ext.runtime.get()).assume_init_read() };
            let _ = xhci::stop_controller(&mut platform, &caps, runtime, POLL_BUDGET);
            return DRIVER_FAILED;
        }
    }
    0
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"USBXHCI_";

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

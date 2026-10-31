#![no_std]
#![no_main]

use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
use cognacos_hal_contract::{DeviceServiceTable, HalServiceTable};
use cognacos_nt_abi::{io::DRIVER_OBJECT, ntstatus::*, types::PVOID};
use cognacos_storage::{self as storage, ahci::{ControllerInfo, RegisterIo}};

const HANDOFF_MAGIC: u64 = 0x434f_474e_4143_4148; // COGNACAH
const HANDOFF_VERSION: u32 = 1;

#[repr(C)]
pub struct AhciDriverHandoff {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    /// Uncached, non-executable virtual mapping of the assigned AHCI BAR.
    pub mmio_base: u64,
    /// Physical address from the translated CM_RESOURCE_LIST memory descriptor.
    pub mmio_physical: u64,
    pub mmio_length: u64,
    /// Stable PCI device-object key for requester-specific DMA and bus-master policy.
    pub device_object: u64,
    pub hal: *const HalServiceTable,
    pub device: *const DeviceServiceTable,
}
struct SharedInfo(UnsafeCell<MaybeUninit<ControllerInfo>>);
unsafe impl Sync for SharedInfo {}
static HANDOFF: AtomicU64 = AtomicU64::new(0);
static PREPARED: AtomicBool = AtomicBool::new(false);
static START_RESOURCES_VALID: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);
static INFO: SharedInfo = SharedInfo(UnsafeCell::new(MaybeUninit::uninit()));

fn handoff() -> Option<&'static AhciDriverHandoff> {
    let address = HANDOFF.load(Ordering::Acquire);
    if address == 0 { return None; }
    Some(unsafe { &*(address as *const AhciDriverHandoff) })
}
struct Mmio<'a> { base: u64, length: u64, hal: &'a HalServiceTable }
impl RegisterIo for Mmio<'_> {
    fn read32(&mut self, offset: usize) -> Result<u32, storage::StorageError> {
        let end = (offset as u64).checked_add(4).ok_or(storage::StorageError::Overflow)?;
        if end > self.length || offset & 3 != 0 { return Err(storage::StorageError::OutOfRange); }
        let address = self.base.checked_add(offset as u64).ok_or(storage::StorageError::Overflow)?;
        let read = self.hal.read_mmio_u32.ok_or(storage::StorageError::Unsupported)?;
        let mut value = 0u32;
        if unsafe { read(address, &mut value) }.code != 0 { return Err(storage::StorageError::DeviceError); }
        Ok(value)
    }
    fn write32(&mut self, offset: usize, value: u32) -> Result<(), storage::StorageError> {
        let end = (offset as u64).checked_add(4).ok_or(storage::StorageError::Overflow)?;
        if end > self.length || offset & 3 != 0 { return Err(storage::StorageError::OutOfRange); }
        let address = self.base.checked_add(offset as u64).ok_or(storage::StorageError::Overflow)?;
        let write = self.hal.write_mmio_u32.ok_or(storage::StorageError::Unsupported)?;
        if unsafe { write(address, value) }.code != 0 { return Err(storage::StorageError::DeviceError); }
        Ok(())
    }
}

/// DriverEntry validates driver-wide service contracts only. Hardware ownership,
/// BAR use and controller reset occur later from the PnP start path.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    let object = unsafe { &mut *(driver as *mut DRIVER_OBJECT) };
    if object.driver_extension.is_null() { return STATUS_INVALID_PARAMETER; }
    let ext = unsafe { &*(object.driver_extension as *const AhciDriverHandoff) };
    if ext.magic != HANDOFF_MAGIC || ext.version != HANDOFF_VERSION
        || (ext.size as usize) < core::mem::size_of::<AhciDriverHandoff>()
        || ext.mmio_base == 0 || ext.mmio_base & 3 != 0 || ext.mmio_physical == 0
        || ext.mmio_physical & 0xfff != 0 || ext.mmio_length < 0x1100
        || ext.mmio_length > 0x10000 || ext.device_object == 0
        || ext.hal.is_null() || ext.device.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    let Some(device) = (unsafe { ext.device.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    if !hal.compatible() || hal.read_mmio_u32.is_none() || hal.write_mmio_u32.is_none()
        || !device.compatible() || !device.has_dma() || !device.has_interrupts()
        || !device.has_bus_master() {
        return STATUS_NOT_SUPPORTED;
    }
    HANDOFF.store(ext as *const _ as u64, Ordering::Release);
    object.major_function[IRP_MJ_PNP] = Some(pnp_dispatch);
    object.driver_unload = Some(driver_unload);
    STATUS_SUCCESS
}

/// Safe first stage of IRP_MN_START_DEVICE: revoke PCI bus mastering, perform
/// BIOS ownership handoff and AHCI HBA reset, and leave every interrupt masked.
/// Command/FIS DMA allocation and port activation are deliberately a separate
/// step; this routine never enables bus mastering or starts a port engine.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacStorAhciPrepareController() -> i32 {
    if !START_RESOURCES_VALID.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        return STATUS_DEVICE_BUSY;
    }
    let result = (|| {
        let Some(ext) = handoff() else { return STATUS_DEVICE_NOT_READY; };
        let Some(device) = (unsafe { ext.device.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
        let Some(disable_bus_master) = device.set_bus_master else { return STATUS_NOT_SUPPORTED; };
        if unsafe { disable_bus_master(ext.device_object, false) }.code != 0 {
            return STATUS_DEVICE_NOT_READY;
        }
        let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
        let mut io = Mmio { base: ext.mmio_base, length: ext.mmio_length, hal };
        let info = match storage::ahci::initialize_controller(&mut io, ext.mmio_length, 2_000_000) {
            Ok(value) => value,
            Err(storage::StorageError::Timeout) => return STATUS_IO_TIMEOUT,
            Err(storage::StorageError::OutOfRange | storage::StorageError::InvalidArgument) => return STATUS_INVALID_PARAMETER,
            Err(_) => return STATUS_DEVICE_NOT_READY,
        };
        if info.ports_implemented == 0 { return STATUS_DEVICE_NOT_READY; }
        unsafe { (*INFO.0.get()).write(info); }
        PREPARED.store(true, Ordering::Release);
        STATUS_SUCCESS
    })();
    BUSY.store(false, Ordering::Release);
    result
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacStorAhciQueryControllerInfo(output: *mut ControllerInfo) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !PREPARED.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    unsafe { output.write((*INFO.0.get()).assume_init_read()); }
    STATUS_SUCCESS
}

const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_QUERY_REMOVE_DEVICE: u8 = 0x01;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_CANCEL_REMOVE_DEVICE: u8 = 0x03;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_QUERY_STOP_DEVICE: u8 = 0x05;
const IRP_MN_CANCEL_STOP_DEVICE: u8 = 0x06;
const IRP_MN_SURPRISE_REMOVAL: u8 = 0x17;

unsafe fn raw_ahci_memory_resource_present(resources: *const u8, minimum_length: u64) -> bool {
    if resources.is_null() { return false; }
    let full_count = unsafe { core::ptr::read_unaligned(resources.cast::<u32>()) };
    if full_count == 0 || full_count > 8 { return false; }
    let mut cursor = 4usize;
    for _ in 0..full_count {
        let partial_count = unsafe { core::ptr::read_unaligned(resources.add(cursor + 12).cast::<u32>()) };
        if partial_count > 64 { return false; }
        let mut descriptor = cursor + 16;
        for _ in 0..partial_count {
            let kind = unsafe { *resources.add(descriptor) };
            if kind == 3 {
                let length = unsafe { core::ptr::read_unaligned(resources.add(descriptor + 12).cast::<u32>()) } as u64;
                if length >= minimum_length { return true; }
            }
            descriptor += 16;
        }
        cursor = descriptor;
        if cursor > 4096 { return false; }
    }
    false
}

unsafe fn translated_ahci_bar_matches(resources: *const u8, physical: u64, minimum_length: u64) -> bool {
    if resources.is_null() { return false; }
    let full_count = unsafe { core::ptr::read_unaligned(resources.cast::<u32>()) };
    if full_count == 0 || full_count > 8 { return false; }
    let mut cursor = 4usize;
    for _ in 0..full_count {
        let partial_count = unsafe { core::ptr::read_unaligned(resources.add(cursor + 12).cast::<u32>()) };
        if partial_count > 64 { return false; }
        let mut descriptor = cursor + 16;
        for _ in 0..partial_count {
            let kind = unsafe { *resources.add(descriptor) };
            if kind == 3 {
                let start = unsafe { core::ptr::read_unaligned(resources.add(descriptor + 4).cast::<u64>()) };
                let length = unsafe { core::ptr::read_unaligned(resources.add(descriptor + 12).cast::<u32>()) } as u64;
                if start == physical && length >= minimum_length { return true; }
            }
            descriptor += 16;
        }
        cursor = descriptor;
        if cursor > 4096 { return false; }
    }
    false
}

unsafe extern "win64" fn pnp_dispatch(_device: *mut cognacos_nt_abi::io::DEVICE_OBJECT, irp: *mut cognacos_nt_abi::io::IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut cognacos_nt_abi::io::IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let status = match unsafe { (*stack).minor_function } {
        IRP_MN_START_DEVICE => {
            let Some(ext) = handoff() else { return STATUS_DEVICE_NOT_READY; };
            let raw = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<*const u8>()) };
            let translated = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<*const u8>()) };
            if unsafe { raw_ahci_memory_resource_present(raw, ext.mmio_length)
                && translated_ahci_bar_matches(translated, ext.mmio_physical, ext.mmio_length) } {
                START_RESOURCES_VALID.store(true, Ordering::Release);
                let result = unsafe { CognacStorAhciPrepareController() };
                if result != STATUS_SUCCESS { START_RESOURCES_VALID.store(false, Ordering::Release); }
                result
            } else { STATUS_INVALID_PARAMETER }
        }
        IRP_MN_STOP_DEVICE | IRP_MN_SURPRISE_REMOVAL | IRP_MN_REMOVE_DEVICE => {
            START_RESOURCES_VALID.store(false, Ordering::Release);
            PREPARED.store(false, Ordering::Release);
            if let Some(ext) = handoff() {
                if let Some(device) = unsafe { ext.device.as_ref() } {
                    if let Some(set) = device.set_bus_master {
                        if unsafe { set(ext.device_object, false) }.code != 0 { STATUS_DEVICE_NOT_READY } else { STATUS_SUCCESS }
                    } else { STATUS_NOT_SUPPORTED }
                } else { STATUS_INVALID_PARAMETER }
            } else { STATUS_DEVICE_NOT_READY }
        }
        IRP_MN_QUERY_REMOVE_DEVICE | IRP_MN_CANCEL_REMOVE_DEVICE
        | IRP_MN_QUERY_STOP_DEVICE | IRP_MN_CANCEL_STOP_DEVICE => STATUS_SUCCESS,
        _ => STATUS_INVALID_DEVICE_REQUEST,
    };
    unsafe { (*irp).io_status.status.status = status; (*irp).io_status.information = 0; }
    status
}
unsafe extern "win64" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    START_RESOURCES_VALID.store(false, Ordering::Release);
    PREPARED.store(false, Ordering::Release);
    if let Some(ext) = handoff() {
        if let Some(device) = unsafe { ext.device.as_ref() } {
            if let Some(set) = device.set_bus_master { let _ = unsafe { set(ext.device_object, false) }; }
        }
    }
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"AHCI____";
#[used]
#[unsafe(link_section = ".data")]
static DRIVER_COOKIE: u64 = 0x5244_4F54_4F53_0001;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

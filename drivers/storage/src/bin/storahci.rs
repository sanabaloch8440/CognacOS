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


// Single-command, polled AHCI DMA path. All DMA allocations are device-scoped,
// validated before bus mastering is enabled, and retained until the engine is
// stopped. Requests are serialized; interrupt-driven queueing is a later step.
const AHCI_SECTOR_BYTES: usize = 512;
const AHCI_MAX_TRANSFER: usize = 1024 * 1024;
const PORT_TFD: usize = 0x20;
const PORT_SIG: usize = 0x24;
const PORT_SSTS: usize = 0x28;
const PORT_SERR: usize = 0x30;
const PORT_CI: usize = 0x38;
const PORT_IS: usize = 0x10;
const PORT_IE: usize = 0x14;
const PORT_CMD: usize = 0x18;
const PORT_CLB: usize = 0x00;
const PORT_FB: usize = 0x08;
const CMD_ST: u32 = 1;
const CMD_FRE: u32 = 1 << 4;
const CMD_FR: u32 = 1 << 14;
const CMD_CR: u32 = 1 << 15;
const TFD_BSY: u32 = 1 << 7;
const TFD_DRQ: u32 = 1 << 3;
const TFD_ERR: u32 = 1;
const IS_TFES: u32 = 1 << 30;
const DMA_FLAG: u32 = 0;
const AHCI_POLL_BUDGET: u32 = 2_000_000;

struct AhciRuntime {
    port: u8,
    sectors: u64,
    clb_cpu: u64, clb_dma: u64,
    fb_cpu: u64, fb_dma: u64,
    table_cpu: u64, table_dma: u64,
    bounce_cpu: u64, bounce_dma: u64,
}
struct RuntimeCell(core::cell::UnsafeCell<MaybeUninit<AhciRuntime>>);
unsafe impl Sync for RuntimeCell {}
static AHCI_RUNTIME: RuntimeCell = RuntimeCell(core::cell::UnsafeCell::new(MaybeUninit::uninit()));
static AHCI_ONLINE: AtomicBool = AtomicBool::new(false);
static AHCI_IO_BUSY: AtomicBool = AtomicBool::new(false);

fn allocate_dma(ext: &AhciDriverHandoff, size: u64, alignment: u64, maximum: u64) -> Result<(u64, u64), i32> {
    let device = unsafe { ext.device.as_ref() }.ok_or(STATUS_INVALID_PARAMETER)?;
    let allocate = device.allocate_dma.ok_or(STATUS_NOT_SUPPORTED)?;
    let mut cpu = 0u64;
    let mut dma = 0u64;
    if unsafe { allocate(ext.device_object, size, alignment, maximum, DMA_FLAG, &mut cpu, &mut dma) }.code != 0
        || cpu == 0 || dma == 0 || dma % alignment != 0 {
        return Err(STATUS_INSUFFICIENT_RESOURCES);
    }
    let validate = device.validate_dma.ok_or(STATUS_NOT_SUPPORTED)?;
    if unsafe { validate(ext.device_object, cpu, dma, size) }.code != 0 {
        if let Some(free) = device.free_dma { let _ = unsafe { free(ext.device_object, cpu, dma, size) }; }
        return Err(STATUS_INVALID_PARAMETER);
    }
    Ok((cpu, dma))
}

fn ahci_port_base(port: u8) -> usize {
    storage::ahci::PORT_BASE + (port as usize) * storage::ahci::PORT_STRIDE
}
fn ahci_poll_clear(io: &mut Mmio<'_>, offset: usize, mask: u32) -> Result<(), i32> {
    for _ in 0..AHCI_POLL_BUDGET {
        let value = io.read32(offset).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        if value & mask == 0 { return Ok(()); }
        core::hint::spin_loop();
    }
    Err(STATUS_IO_TIMEOUT)
}
fn ahci_wait_ready(io: &mut Mmio<'_>, base: usize) -> Result<(), i32> {
    for _ in 0..AHCI_POLL_BUDGET {
        let status = io.read32(base + PORT_TFD).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        if status & TFD_ERR != 0 { return Err(STATUS_IO_DEVICE_ERROR); }
        if status & (TFD_BSY | TFD_DRQ) == 0 { return Ok(()); }
        core::hint::spin_loop();
    }
    Err(STATUS_IO_TIMEOUT)
}

// Slot zero only. Header is 32 bytes, command table is 256-byte aligned,
// command FIS occupies bytes 0..20, and the single PRDT entry starts at 128.
unsafe fn ahci_issue(
    io: &mut Mmio<'_>, runtime: &AhciRuntime, command: u8,
    lba: u64, sectors: u16, bytes: usize, write: bool, lba48: bool,
) -> Result<(), i32> {
    if bytes == 0 || bytes > AHCI_MAX_TRANSFER || bytes % AHCI_SECTOR_BYTES != 0 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let base = ahci_port_base(runtime.port);
    ahci_wait_ready(io, base)?;
    let table = runtime.table_cpu as *mut u8;
    unsafe { core::ptr::write_bytes(table, 0, 256); }
    let fis = table;
    unsafe {
        fis.write(0x27);
        fis.add(1).write(0x80);
        fis.add(2).write(command);
        if lba48 {
            fis.add(4).write(lba as u8);
            fis.add(5).write((lba >> 8) as u8);
            fis.add(6).write((lba >> 16) as u8);
            fis.add(7).write(1 << 6);
            fis.add(8).write((lba >> 24) as u8);
            fis.add(9).write((lba >> 32) as u8);
            fis.add(10).write((lba >> 40) as u8);
            fis.add(12).write(sectors as u8);
            fis.add(13).write((sectors >> 8) as u8);
        }
        // PRDT[0]: 64-bit DMA address, byte count encoded as N-1.
        let prdt = table.add(128) as *mut u32;
        prdt.write(runtime.bounce_dma as u32);
        prdt.add(1).write((runtime.bounce_dma >> 32) as u32);
        prdt.add(2).write(0);
        prdt.add(3).write((bytes as u32 - 1) | (1 << 31));
        // Command header: CFL=5 dwords, W for device-bound writes, PRDTL=1.
        let header = runtime.clb_cpu as *mut u32;
        header.write(5 | if write { 1 << 6 } else { 0 });
        header.add(1).write(0);
        header.add(2).write(runtime.table_dma as u32);
        header.add(3).write((runtime.table_dma >> 32) as u32);
        header.add(4).write(0);
        header.add(5).write(0);
        header.add(6).write(0);
        header.add(7).write(0);
    }
    let is = io.read32(base + PORT_IS).map_err(|_| STATUS_DEVICE_NOT_READY)?;
    if is != 0 && is != u32::MAX { io.write32(base + PORT_IS, is).map_err(|_| STATUS_DEVICE_NOT_READY)?; }
    io.write32(base + PORT_CI, 1).map_err(|_| STATUS_DEVICE_NOT_READY)?;
    for _ in 0..AHCI_POLL_BUDGET {
        let status = io.read32(base + PORT_TFD).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        let interrupt = io.read32(base + PORT_IS).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        if status & TFD_ERR != 0 || interrupt & IS_TFES != 0 {
            return Err(STATUS_IO_DEVICE_ERROR);
        }
        let ci = io.read32(base + PORT_CI).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        if ci & 1 == 0 { return Ok(()); }
        core::hint::spin_loop();
    }
    Err(STATUS_IO_TIMEOUT)
}

/// Activate the first implemented, connected SATA ATA port and establish a
/// validated command-list/FIS/table/bounce-buffer DMA working set.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacStorAhciStartDataPath() -> i32 {
    if !PREPARED.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if AHCI_ONLINE.load(Ordering::Acquire) { return STATUS_SUCCESS; }
    if BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        return STATUS_DEVICE_BUSY;
    }
    let result = (|| {
        let Some(ext) = handoff() else { return STATUS_DEVICE_NOT_READY; };
        if !PREPARED.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
        let info = unsafe { (*INFO.0.get()).assume_init_ref() };
        let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
        let mut io = Mmio { base: ext.mmio_base, length: ext.mmio_length, hal };
        let maximum = if info.supports_64bit_dma { u64::MAX } else { u32::MAX as u64 };
        let mut selected = None;
        for port in 0u8..32 {
            if info.ports_implemented & (1u32 << port) == 0 { continue; }
            let base = ahci_port_base(port);
            let ssts = io.read32(base + PORT_SSTS).map_err(|_| STATUS_DEVICE_NOT_READY)?;
            let sig = io.read32(base + PORT_SIG).map_err(|_| STATUS_DEVICE_NOT_READY)?;
            if storage::ahci::classify_port(ssts, sig) == storage::ahci::PortDevice::Ata {
                selected = Some(port);
                break;
            }
        }
        let port = selected.ok_or(STATUS_DEVICE_NOT_READY)?;
        let (clb_cpu, clb_dma) = allocate_dma(ext, 1024, 1024, maximum)?;
        let (fb_cpu, fb_dma) = allocate_dma(ext, 256, 256, maximum)?;
        let (table_cpu, table_dma) = allocate_dma(ext, 256, 128, maximum)?;
        let (bounce_cpu, bounce_dma) = allocate_dma(ext, AHCI_MAX_TRANSFER as u64, 4096, maximum)?;
        unsafe {
            core::ptr::write_bytes(clb_cpu as *mut u8, 0, 1024);
            core::ptr::write_bytes(fb_cpu as *mut u8, 0, 256);
            core::ptr::write_bytes(table_cpu as *mut u8, 0, 256);
            core::ptr::write_bytes(bounce_cpu as *mut u8, 0, AHCI_MAX_TRANSFER);
        }
        let runtime = AhciRuntime { port, sectors: 0, clb_cpu, clb_dma, fb_cpu, fb_dma, table_cpu, table_dma, bounce_cpu, bounce_dma };
        let base = ahci_port_base(port);
        io.write32(base + PORT_CLB, clb_dma as u32).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        io.write32(base + PORT_CLB + 4, (clb_dma >> 32) as u32).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        io.write32(base + PORT_FB, fb_dma as u32).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        io.write32(base + PORT_FB + 4, (fb_dma >> 32) as u32).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        io.write32(base + PORT_IE, 0).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        let cmd = io.read32(base + PORT_CMD).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        io.write32(base + PORT_CMD, cmd | CMD_FRE).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        io.write32(base + PORT_CMD, cmd | CMD_FRE | CMD_ST).map_err(|_| STATUS_DEVICE_NOT_READY)?;
        ahci_wait_ready(&mut io, base)?;
        // Bus mastering is enabled only after all DMA regions are allocated,
        // initialized, and validated for this PCI requester.
        let Some(device) = (unsafe { ext.device.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
        let Some(set_bus_master) = device.set_bus_master else { return STATUS_NOT_SUPPORTED; };
        if unsafe { set_bus_master(ext.device_object, true) }.code != 0 { return STATUS_DEVICE_NOT_READY; }
        unsafe { ahci_issue(&mut io, &runtime, 0xec, 0, 1, 512, false, false)?; }
        let words = unsafe { core::slice::from_raw_parts(bounce_cpu as *const u16, 256) };
        let sectors = if words[83] & (1 << 10) != 0 {
            (words[100] as u64) | ((words[101] as u64) << 16) | ((words[102] as u64) << 32) | ((words[103] as u64) << 48)
        } else { words[60] as u64 | ((words[61] as u64) << 16) };
        if sectors == 0 { return STATUS_DEVICE_NOT_READY; }
        let runtime = AhciRuntime { sectors, ..runtime };
        unsafe { (*AHCI_RUNTIME.0.get()).write(runtime); }
        AHCI_ONLINE.store(true, Ordering::Release);
        STATUS_SUCCESS
    })();
    BUSY.store(false, Ordering::Release);
    result
}

/// Synchronous DMA block transfer API. Caller supplies a resident kernel buffer;
/// all device DMA targets the validated nonpaged bounce buffer, never a user VA.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacStorAhciReadWrite(
    lba: u64, sectors: u32, buffer: *mut u8, write: bool,
) -> i32 {
    if !AHCI_ONLINE.load(Ordering::Acquire) || buffer.is_null() || sectors == 0 {
        return STATUS_DEVICE_NOT_READY;
    }
    let Some(bytes) = (sectors as usize).checked_mul(AHCI_SECTOR_BYTES) else { return STATUS_INVALID_PARAMETER; };
    if bytes > AHCI_MAX_TRANSFER || lba.checked_add(sectors as u64).map_or(true, |end| end > unsafe { (*(*AHCI_RUNTIME.0.get()).assume_init_ref()).sectors }) {
        return STATUS_INVALID_PARAMETER;
    }
    if AHCI_IO_BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { return STATUS_DEVICE_BUSY; }
    let result = (|| {
        let Some(ext) = handoff() else { return STATUS_DEVICE_NOT_READY; };
        let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
        let runtime = unsafe { (*AHCI_RUNTIME.0.get()).assume_init_ref() };
        if unsafe { ext.device.as_ref().and_then(|d| d.validate_dma).map(|f| f(ext.device_object, runtime.bounce_cpu, runtime.bounce_dma, AHCI_MAX_TRANSFER as u64).code == 0) } != Some(true) {
            return STATUS_INVALID_PARAMETER;
        }
        if write { unsafe { core::ptr::copy_nonoverlapping(buffer, runtime.bounce_cpu as *mut u8, bytes); } }
        let mut io = Mmio { base: ext.mmio_base, length: ext.mmio_length, hal };
        let opcode = if write { 0x35 } else { 0x25 };
        let status = unsafe { ahci_issue(&mut io, runtime, opcode, lba, sectors as u16, bytes, write, true) };
        if let Err(status) = status { return status; }
        if !write { unsafe { core::ptr::copy_nonoverlapping(runtime.bounce_cpu as *const u8, buffer, bytes); } }
        STATUS_SUCCESS
    })();
    AHCI_IO_BUSY.store(false, Ordering::Release);
    result
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

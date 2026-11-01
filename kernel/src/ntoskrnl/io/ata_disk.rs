//! Bounded ATA PIO disk transport and end-to-end GPT/ext4 read/write validation.
//! Targets QEMU's primary IDE master; each ATA command transfers one sector.
use cognacos_ext4 as extfs;
use extfs::{ReadOnlyBlockDevice, BlockDevice};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use crate::wdm::{self, DriverObject, DeviceObject, Irp, IoStackLocation};
use crate::hal::IrqSpinLock;

const DATA: u16 = 0x1f0;
const ERROR: u16 = 0x1f1;
const SECTOR_COUNT: u16 = 0x1f2;
const LBA_LOW: u16 = 0x1f3;
const LBA_MID: u16 = 0x1f4;
const LBA_HIGH: u16 = 0x1f5;
const DRIVE: u16 = 0x1f6;
const STATUS: u16 = 0x1f7;
const ALT_STATUS: u16 = 0x3f6;
const COMMAND: u16 = 0x1f7;
const CMD_IDENTIFY: u8 = 0xec;
const CMD_READ_SECTORS: u8 = 0x20;
const CMD_WRITE_SECTORS: u8 = 0x30;
const CMD_FLUSH_CACHE: u8 = 0xe7;
const BSY: u8 = 0x80;
const DRQ: u8 = 0x08;
const ERR: u8 = 0x01;
const SECTOR_BYTES: usize = 512;
const MAX_LBA28_SECTORS: u64 = 1 << 28;
const EXPECTED: &[u8] = b"Readme, I have a secret to kernel stability";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtaError { Offline, Timeout, Device, Bounds, Unsupported }

#[inline]
unsafe fn out8(port: u16, value: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}
#[inline]
unsafe fn in8(port: u16) -> u8 {
    let value: u8;
    unsafe { core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags)); }
    value
}
#[inline]
unsafe fn in16(port: u16) -> u16 {
    let value: u16;
    unsafe { core::arch::asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags)); }
    value
}
fn delay_400ns() {
    unsafe { let _ = in8(ALT_STATUS); let _ = in8(ALT_STATUS); let _ = in8(ALT_STATUS); let _ = in8(ALT_STATUS); }
}
fn wait_not_busy() -> Result<u8, AtaError> {
    for _ in 0..2_000_000usize {
        let s = unsafe { in8(STATUS) };
        if s == 0xff { return Err(AtaError::Offline); }
        if s & BSY == 0 {
            if s & ERR != 0 { let _ = unsafe { in8(ERROR) }; return Err(AtaError::Device); }
            return Ok(s);
        }
        core::hint::spin_loop();
    }
    Err(AtaError::Timeout)
}
fn wait_drq() -> Result<(), AtaError> {
    for _ in 0..2_000_000usize {
        let s = unsafe { in8(STATUS) };
        if s == 0xff { return Err(AtaError::Offline); }
        if s & BSY == 0 {
            if s & ERR != 0 { let _ = unsafe { in8(ERROR) }; return Err(AtaError::Device); }
            if s & DRQ != 0 { return Ok(()); }
        }
        core::hint::spin_loop();
    }
    Err(AtaError::Timeout)
}

/// ATA LBA28 primary-master device. Each command transfers exactly one sector,
/// avoiding unbounded ATA multi-sector commands and making bounds explicit.
#[derive(Clone, Copy)]
struct AtaPrimaryMaster { sectors: u64 }

static STORAGE_DISK: IrqSpinLock<Option<AtaPrimaryMaster>> = IrqSpinLock::new(None);
static DISK_CAPACITY: AtomicU64 = AtomicU64::new(0);
static DISK_DEVICE: AtomicUsize = AtomicUsize::new(0);
static PARTITION_DEVICE: AtomicUsize = AtomicUsize::new(0);
static PARTITION_START: AtomicU64 = AtomicU64::new(0);
static PARTITION_CAPACITY: AtomicU64 = AtomicU64::new(0);
static TRACE_PARTITION_START: AtomicU64 = AtomicU64::new(0);
static TRACE_PARTITION_CAPACITY: AtomicU64 = AtomicU64::new(0);
impl AtaPrimaryMaster {
    fn initialize() -> Result<Self, AtaError> {
        unsafe { out8(DRIVE, 0xa0); }
        delay_400ns();
        unsafe {
            out8(SECTOR_COUNT, 0);
            out8(LBA_LOW, 0);
            out8(LBA_MID, 0);
            out8(LBA_HIGH, 0);
            out8(COMMAND, CMD_IDENTIFY);
        }
        let s = unsafe { in8(STATUS) };
        if s == 0 || s == 0xff { return Err(AtaError::Offline); }
        wait_not_busy()?;
        wait_drq()?;
        let mut words = [0u16; 256];
        for w in words.iter_mut() { *w = unsafe { in16(DATA) }; }
        if words[49] & (1 << 9) == 0 { return Err(AtaError::Unsupported); }
        let sectors = words[60] as u64 | ((words[61] as u64) << 16);
        if sectors == 0 { return Err(AtaError::Offline); }
        Ok(Self { sectors: sectors.min(MAX_LBA28_SECTORS) })
    }
    fn write_sector(&mut self, lba: u64, data: &[u8; SECTOR_BYTES]) -> Result<(), AtaError> {
        if lba >= self.sectors || lba >= MAX_LBA28_SECTORS { return Err(AtaError::Bounds); }
        unsafe {
            out8(DRIVE, 0xe0 | ((lba >> 24) as u8 & 0x0f));
            delay_400ns();
            out8(SECTOR_COUNT, 1);
            out8(LBA_LOW, lba as u8);
            out8(LBA_MID, (lba >> 8) as u8);
            out8(LBA_HIGH, (lba >> 16) as u8);
            out8(COMMAND, CMD_WRITE_SECTORS);
        }
        delay_400ns();
        wait_drq()?;
        for chunk in data.chunks_exact(2) {
            let word = u16::from_le_bytes([chunk[0], chunk[1]]);
            unsafe { core::arch::asm!("out dx, ax", in("dx") DATA, in("ax") word, options(nomem, nostack, preserves_flags)); }
        }
        let _ = wait_not_busy()?;
        Ok(())
    }
    fn flush_cache(&mut self) -> Result<(), AtaError> {
        let _ = wait_not_busy()?;
        unsafe { out8(DRIVE, 0xe0); delay_400ns(); out8(COMMAND, CMD_FLUSH_CACHE); }
        let _ = wait_not_busy()?;
        Ok(())
    }

    fn read_sector(&mut self, lba: u64, out: &mut [u8; SECTOR_BYTES]) -> Result<(), AtaError> {
        if lba >= self.sectors || lba >= MAX_LBA28_SECTORS { return Err(AtaError::Bounds); }
        unsafe {
            out8(DRIVE, 0xe0 | ((lba >> 24) as u8 & 0x0f));
            delay_400ns();
            out8(SECTOR_COUNT, 1);
            out8(LBA_LOW, lba as u8);
            out8(LBA_MID, (lba >> 8) as u8);
            out8(LBA_HIGH, (lba >> 16) as u8);
            out8(COMMAND, CMD_READ_SECTORS);
        }
        delay_400ns();
        wait_drq()?;
        for i in 0..256 {
            let w = unsafe { in16(DATA) }.to_le_bytes();
            out[i * 2] = w[0];
            out[i * 2 + 1] = w[1];
        }
        let _ = wait_not_busy()?;
        Ok(())
    }
}
impl ReadOnlyBlockDevice for AtaPrimaryMaster {
    type DeviceError = AtaError;
    fn capacity_bytes(&self) -> u64 { self.sectors.saturating_mul(SECTOR_BYTES as u64) }
    fn read_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), Self::DeviceError> {
        let end = offset.checked_add(out.len() as u64).ok_or(AtaError::Bounds)?;
        if end > self.capacity_bytes() { return Err(AtaError::Bounds); }
        let mut done = 0usize;
        let mut sector = [0u8; SECTOR_BYTES];
        while done < out.len() {
            let pos = offset.checked_add(done as u64).ok_or(AtaError::Bounds)?;
            let lba = pos / SECTOR_BYTES as u64;
            let within = (pos % SECTOR_BYTES as u64) as usize;
            self.read_sector(lba, &mut sector)?;
            let count = core::cmp::min(SECTOR_BYTES - within, out.len() - done);
            out[done..done + count].copy_from_slice(&sector[within..within + count]);
            done += count;
        }
        Ok(())
    }
}
impl BlockDevice for AtaPrimaryMaster {
    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<(), Self::DeviceError> {
        let end = offset.checked_add(data.len() as u64).ok_or(AtaError::Bounds)?;
        if end > self.capacity_bytes() { return Err(AtaError::Bounds); }
        let mut done = 0usize;
        let mut sector = [0u8; SECTOR_BYTES];
        while done < data.len() {
            let pos = offset.checked_add(done as u64).ok_or(AtaError::Bounds)?;
            let lba = pos / SECTOR_BYTES as u64;
            let within = (pos % SECTOR_BYTES as u64) as usize;
            let count = core::cmp::min(SECTOR_BYTES - within, data.len() - done);
            if within != 0 || count != SECTOR_BYTES {
                self.read_sector(lba, &mut sector)?;
            }
            sector[within..within + count].copy_from_slice(&data[done..done + count]);
            self.write_sector(lba, &sector)?;
            done += count;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), Self::DeviceError> { self.flush_cache() }
}

pub enum StorageError { Offline, InvalidRequest, Io }

const fn make_storage_driver()->DriverObject {
 let mut major:[Option<wdm::DriverDispatch>;28]=[None;28];
 major[IRP_MJ_CREATE as usize]=Some(storage_dispatch);
 major[IRP_MJ_CLOSE as usize]=Some(storage_dispatch);
 major[IRP_MJ_READ as usize]=Some(storage_dispatch);
 major[IRP_MJ_WRITE as usize]=Some(storage_dispatch);
 DriverObject {
  object_type:4,size:core::mem::size_of::<DriverObject>() as i16,
  device_object:core::ptr::null_mut(),flags:0,driver_start:core::ptr::null_mut(),
  driver_size:0,driver_section:core::ptr::null_mut(),driver_extension:core::ptr::null_mut(),
  driver_name:crate::nt_abi::object::UNICODE_STRING{length:0,maximum_length:0,buffer:core::ptr::null_mut()},
  hardware_database:core::ptr::null_mut(),fast_io_dispatch:core::ptr::null_mut(),
  driver_init:core::ptr::null_mut(),driver_start_io:core::ptr::null_mut(),driver_unload:None,
  major_function:major,
 }
}
static mut STORAGE_DRIVER:DriverObject=make_storage_driver();

unsafe extern "win64" fn storage_dispatch(device:*mut DeviceObject,irp:*mut Irp)->i32 {
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let stack=unsafe{wdm::io_get_current_irp_stack_location(irp)};
 if stack.is_null(){unsafe{wdm::io_complete_irp_status(irp,STATUS_INVALID_PARAMETER,0,0);}return STATUS_INVALID_PARAMETER}
 let major=unsafe{(*stack).major_function};
 let mut status=0i32;
 let mut information=0usize;
 if major==IRP_MJ_READ||major==IRP_MJ_WRITE {
  let params=unsafe{(*stack).parameters.as_ptr()};
  let length=unsafe{core::ptr::read_unaligned(params.cast::<u32>()) as usize};
  let offset=unsafe{core::ptr::read_unaligned(params.add(8).cast::<i64>())};
  let buffer=unsafe{core::ptr::read_unaligned((*irp).bytes.as_ptr().add(IRP_USER_BUFFER_OFFSET).cast::<*mut u8>())};
  let is_partition=device as usize==PARTITION_DEVICE.load(Ordering::Acquire);
  let capacity=if is_partition{PARTITION_CAPACITY.load(Ordering::Acquire)}else{DISK_CAPACITY.load(Ordering::Acquire)};
  let end=if offset<0{None}else{(offset as u64).checked_add(length as u64)};
  if buffer.is_null()&&length!=0||end.is_none_or(|value|value>capacity)||length>64*1024 {
   status=STATUS_INVALID_PARAMETER;
  } else {
   let absolute=if is_partition {
    PARTITION_START.load(Ordering::Acquire).checked_add(offset as u64)
   } else {Some(offset as u64)};
   match absolute {
    Some(byte_offset)=>{
     let mut guard=STORAGE_DISK.lock();
     match guard.as_mut() {
      Some(disk)=>{
       let bytes=unsafe{core::slice::from_raw_parts_mut(buffer,length)};
       let result=if major==IRP_MJ_READ{disk.read_at(byte_offset,bytes)}else{disk.write_at(byte_offset,bytes)};
       match result {Ok(())=>information=length,Err(_)=>status=STATUS_IO_DEVICE_ERROR}
      }
      None=>status=STATUS_IO_DEVICE_ERROR,
     }
    }
    None=>status=STATUS_INVALID_PARAMETER,
   }
  }
 } else if major!=IRP_MJ_CREATE&&major!=IRP_MJ_CLOSE {
  status=STATUS_INVALID_PARAMETER;
 }
 unsafe{wdm::io_complete_irp_status(irp,status,information,0);}
 status
}

fn create_storage_device(name:&[u8],partition:bool)->Result<*mut DeviceObject,StorageError>{
 if name.is_empty()||name.len()>128||name.len()%2!=0{return Err(StorageError::InvalidRequest)}
 let mut name_units=[0u16;128];
 for (index,pair) in name.chunks_exact(2).enumerate(){name_units[index]=u16::from_le_bytes([pair[0],pair[1]]);}
 let mut unicode=crate::nt_abi::object::UNICODE_STRING{
  length:name.len() as u16,maximum_length:name.len() as u16,buffer:name_units.as_mut_ptr()
 };
 let driver=core::ptr::addr_of_mut!(STORAGE_DRIVER);
 let mut device=core::ptr::null_mut();
 let status=unsafe{wdm::io_create_device(driver,0,&mut unicode,0x0000_0007,0,0,&mut device)};
 if status<0||device.is_null(){return Err(StorageError::Offline)}
 unsafe{(*device).sector_size=SECTOR_BYTES as u16;}
 if partition{PARTITION_DEVICE.store(device as usize,Ordering::Release);}
 else{DISK_DEVICE.store(device as usize,Ordering::Release);}
 Ok(device)
}

/// Synchronous WDM request helper used by the ext4 block-device adapter.
/// The IRP is kernel-created and its UserBuffer is a caller-owned kernel buffer.
fn storage_transfer(device:*mut DeviceObject,major:u8,offset:u64,buffer:*mut u8,length:usize)->Result<usize,StorageError>{
 if device.is_null()||buffer.is_null()&&length!=0||length>64*1024{return Err(StorageError::InvalidRequest)}
 let stack_size=unsafe{(*device).stack_size};
 if stack_size<=0{return Err(StorageError::Offline)}
 let irp=unsafe{wdm::io_allocate_irp(stack_size,0)};
 if irp.is_null(){return Err(StorageError::Offline)}
 let stack=unsafe{wdm::io_get_next_irp_stack_location(irp)};
 if stack.is_null(){unsafe{wdm::io_free_irp(irp);}return Err(StorageError::Offline)}
 unsafe{
  (*stack).major_function=major;
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<u32>(),length as u32);
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<i64>(),offset as i64);
  core::ptr::write_unaligned((*irp).bytes.as_mut_ptr().add(IRP_USER_BUFFER_OFFSET).cast::<*mut u8>(),buffer);
 }
 let dispatch=unsafe{wdm::io_call_driver(device,irp)};
 let status=unsafe{core::ptr::read_unaligned((*irp).bytes.as_ptr().add(IRP_STATUS_OFFSET).cast::<i32>())};
 let information=unsafe{core::ptr::read_unaligned((*irp).bytes.as_ptr().add(IRP_INFORMATION_OFFSET).cast::<usize>())};
 unsafe{wdm::io_free_irp(irp);}
 if dispatch<0||status<0{return Err(StorageError::Io)}
 Ok(information)
}

pub struct IoManagerBlockDevice { device:*mut DeviceObject,capacity:u64 }
impl ReadOnlyBlockDevice for IoManagerBlockDevice {
 type DeviceError=StorageError;
 fn capacity_bytes(&self)->u64{self.capacity}
 fn read_at(&mut self,offset:u64,out:&mut[u8])->Result<(),Self::DeviceError>{
  let count=storage_transfer(self.device,IRP_MJ_READ,offset,out.as_mut_ptr(),out.len())?;
  if count==out.len(){Ok(())}else{Err(StorageError::Io)}
 }
}
impl BlockDevice for IoManagerBlockDevice {
 fn write_at(&mut self,offset:u64,data:&[u8])->Result<(),Self::DeviceError>{
  let count=storage_transfer(self.device,IRP_MJ_WRITE,offset,data.as_ptr() as *mut u8,data.len())?;
  if count==data.len(){Ok(())}else{Err(StorageError::Io)}
 }
 fn flush(&mut self)->Result<(),Self::DeviceError>{
  STORAGE_DISK.lock().as_mut().ok_or(StorageError::Offline)?.flush().map_err(|_|StorageError::Io)
 }
}

/// Create a partition-relative block-device adapter. All filesystem data I/O
/// performed through this adapter is sent down the registered WDM partition stack.
pub fn ext4_block_device() -> Result<IoManagerBlockDevice, extfs::Error> {
    let device = PARTITION_DEVICE.load(Ordering::Acquire) as *mut DeviceObject;
    if device.is_null() { return Err(extfs::Error::Io); }
    Ok(IoManagerBlockDevice {
        device,
        capacity: PARTITION_CAPACITY.load(Ordering::Acquire),
    })
}


pub fn is_ext4_partition(device:*mut DeviceObject)->bool{
 if device.is_null()||device as usize!=PARTITION_DEVICE.load(Ordering::Acquire){return false}
 let mut volume=IoManagerBlockDevice{device,capacity:PARTITION_CAPACITY.load(Ordering::Acquire)};
 extfs::read_superblock(&mut volume).is_ok()
}

/// Optional reserved GPT partition for crash-trace persistence. The partition
/// is discovered from GPT entry index 2; no arbitrary sectors are overwritten.
pub fn debug_trace_store_read(offset: u64, output: &mut [u8]) -> bool {
    trace_store_transfer(offset, output)
}
pub fn debug_trace_store_write(offset: u64, input: &[u8]) -> bool {
    let mut copy = [0u8; 512];
    let mut done = 0usize;
    let capacity = TRACE_PARTITION_CAPACITY.load(Ordering::Acquire);
    if capacity == 0 || offset.checked_add(input.len() as u64).map_or(true, |end| end > capacity) { return false; }
    while done < input.len() {
        let absolute = match offset.checked_add(done as u64) { Some(v) => v, None => return false };
        let lba = TRACE_PARTITION_START.load(Ordering::Acquire).saturating_add(absolute / 512);
        let within = (absolute % 512) as usize;
        let amount = (512 - within).min(input.len() - done);
        let mut guard = match STORAGE_DISK.try_lock() { Some(guard) => guard, None => return false };
        let Some(disk) = guard.as_mut() else { return false; };
        if within != 0 || amount != 512 {
            if disk.read_sector(lba, &mut copy).is_err() { return false; }
        } else { copy.fill(0); }
        copy[within..within + amount].copy_from_slice(&input[done..done + amount]);
        if disk.write_sector(lba, &copy).is_err() || disk.flush_cache().is_err() { return false; }
        done += amount;
    }
    true
}
fn trace_store_transfer(offset: u64, output: &mut [u8]) -> bool {
    let capacity = TRACE_PARTITION_CAPACITY.load(Ordering::Acquire);
    if capacity == 0 || offset.checked_add(output.len() as u64).map_or(true, |end| end > capacity) { return false; }
    let mut done = 0usize;
    while done < output.len() {
        let absolute = match offset.checked_add(done as u64) { Some(v) => v, None => return false };
        let lba = TRACE_PARTITION_START.load(Ordering::Acquire).saturating_add(absolute / 512);
        let within = (absolute % 512) as usize;
        let amount = (512 - within).min(output.len() - done);
        let mut sector = [0u8; 512];
        let mut guard = match STORAGE_DISK.try_lock() { Some(guard) => guard, None => return false };
        let Some(disk) = guard.as_mut() else { return false; };
        if disk.read_sector(lba, &mut sector).is_err() { return false; }
        output[done..done + amount].copy_from_slice(&sector[within..within + amount]);
        done += amount;
    }
    true
}

#[cfg(debug_assertions)]
fn ext4_error_debug_name(error: extfs::Error) -> &'static [u8] {
    match error {
        extfs::Error::Truncated => b"Truncated",
        extfs::Error::BadMagic => b"BadMagic",
        extfs::Error::InvalidBlockSize => b"InvalidBlockSize",
        extfs::Error::InvalidInodeSize => b"InvalidInodeSize",
        extfs::Error::InvalidGeometry => b"InvalidGeometry",
        extfs::Error::UnsupportedIncompatFeature => b"UnsupportedIncompatFeature",
        extfs::Error::OutOfRange => b"OutOfRange",
        extfs::Error::InvalidDirectoryEntry => b"InvalidDirectoryEntry",
        extfs::Error::UnsupportedFormat => b"UnsupportedFormat",
        extfs::Error::UnsupportedRoCompatFeature => b"UnsupportedRoCompatFeature",
        extfs::Error::UnsupportedCompatFeature => b"UnsupportedCompatFeature",
        extfs::Error::InvalidExtentTree => b"InvalidExtentTree",
        extfs::Error::InvalidChecksum => b"InvalidChecksum",
        extfs::Error::InvalidPartitionTable => b"InvalidPartitionTable",
        extfs::Error::UnsupportedPartitionTable => b"UnsupportedPartitionTable",
        extfs::Error::TransactionTooLarge => b"TransactionTooLarge",
        extfs::Error::Io => b"Io",
    }
}
#[cfg(debug_assertions)]
fn ext4_write_error_debug_name(error: &extfs::WriteError) -> &'static [u8] {
    match error {
        extfs::WriteError::Filesystem(fs) => ext4_error_debug_name(*fs),
        extfs::WriteError::ReadOnly => b"ReadOnly",
        extfs::WriteError::OutOfRange => b"OutOfRange",
        extfs::WriteError::Io => b"Io",
        extfs::WriteError::PartialBlock => b"PartialBlock",
    }
}

/// End-to-end guest check. The physical port driver backs a registered disk
/// object; GPT and ext4 then read only through disk/partition READ IRPs.
pub fn run_ext4_readme_test() -> bool {
    let disk = match AtaPrimaryMaster::initialize() { Ok(d) => d, Err(_) => return false };
    DISK_CAPACITY.store(disk.sectors.saturating_mul(SECTOR_BYTES as u64), Ordering::Release);
    *STORAGE_DISK.lock() = Some(disk);
    let disk_device=match create_storage_device(&[0x5c,0,0x44,0,0x65,0,0x76,0,0x69,0,0x63,0,0x65,0,0x5c,0,0x48,0,0x61,0,0x72,0,0x64,0,0x64,0,0x69,0,0x73,0,0x6b,0,0x30,0],false){Ok(device)=>device,Err(_)=>return false};
    unsafe { crate::serial_write(b"Storage port: QEMU IDE primary-master IDENTIFY and bounded LBA28 reads initialized.\r\n"); }
    let mut whole_disk = IoManagerBlockDevice { device: disk_device, capacity: DISK_CAPACITY.load(Ordering::Acquire) };
    // A third, dedicated GPT partition is optional. Trace writes are disabled
    // unless the image explicitly reserves it; ext4 and GPT metadata are never used as scratch space.
    if let Ok(trace_partition) = extfs::read_gpt_partition(&mut whole_disk, 2, SECTOR_BYTES as u32) {
        if let (Ok(start), Ok(capacity)) = (trace_partition.start_byte(), trace_partition.capacity_bytes()) {
            TRACE_PARTITION_START.store(start / SECTOR_BYTES as u64, Ordering::Release);
            TRACE_PARTITION_CAPACITY.store(capacity, Ordering::Release);
            unsafe { crate::serial_write(b"Debug trace persistence: dedicated GPT partition discovered.\r\n"); }
        }
    }
    let partition = match extfs::read_gpt_partition(&mut whole_disk, 1, SECTOR_BYTES as u32) {
        Ok(p) => p, Err(_) => return false,
    };
    let start = match partition.start_byte() { Ok(v) => v, Err(_) => return false };
    let capacity = match partition.capacity_bytes() { Ok(v) => v, Err(_) => return false };
    PARTITION_START.store(start, Ordering::Release);
    PARTITION_CAPACITY.store(capacity, Ordering::Release);
    let partition_device=match create_storage_device(&[0x5c,0,0x44,0,0x65,0,0x76,0,0x69,0,0x63,0,0x65,0,0x5c,0,0x48,0,0x61,0,0x72,0,0x64,0,0x64,0,0x69,0,0x73,0,0x6b,0,0x30,0,0x5c,0,0x50,0,0x61,0,0x72,0,0x74,0,0x69,0,0x74,0,0x69,0,0x6f,0,0x6e,0,0x31,0],true){Ok(device)=>device,Err(_)=>return false};
    unsafe { crate::serial_write(b"Disk class: Harddisk0 and Partition1 registered in the kernel I/O manager; GPT data partition discovered by READ IRPs.\r\n"); }
    let mut volume = IoManagerBlockDevice { device: partition_device, capacity };
    let mut actual = [0u8; 128];
    let (file_size, length) = match extfs::read_root_file_range(
        &mut volume, b"\\README.txt", 0, &mut actual,
    ) {
        Ok(result) => result, Err(_) => return false,
    };
    if file_size != EXPECTED.len() as u64 || length != EXPECTED.len()
        || &actual[..length] != EXPECTED {
        return false;
    }
    unsafe { crate::serial_write(b"ext4: partition superblock and README data read through the registered partition READ interface.\r\n"); }
    unsafe { crate::serial_write(b"Guest ext4 README.txt bytes match expected content exactly.\r\n"); }

    let mut changed = [0u8; 128];
    changed[..EXPECTED.len()].copy_from_slice(EXPECTED);
    changed[0] = b'X';
    match extfs::write_root_file_range(
        volume, b"\\README.txt", 0, &changed[..EXPECTED.len()],
    ) {
        Ok(n) if n == EXPECTED.len() => {},
        Err(error) => {
            #[cfg(debug_assertions)] unsafe {
                crate::serial_write(b"EXT4 DEBUG: checked data write failed: ");
                crate::serial_write(ext4_error_debug_name(error));
                crate::serial_write(b"\r\n");
            }
            return false;
        },
        _ => return false,
    }
    let mut changed_volume = IoManagerBlockDevice { device: partition_device, capacity };
    let mut changed_readback = [0u8; 128];
    let (changed_size, changed_len) = match extfs::read_root_file_range(
        &mut changed_volume, b"\\README.txt", 0, &mut changed_readback,
    ) {
        Ok(result) => result, Err(_) => return false,
    };
    if changed_size != EXPECTED.len() as u64 || changed_len != EXPECTED.len()
        || &changed_readback[..changed_len] != &changed[..EXPECTED.len()] {
        return false;
    }
    unsafe { crate::serial_write(b"Guest ext4 WRITE IRP modified existing README data and read it back exactly.\r\n"); }

    match extfs::write_root_file_range(
        IoManagerBlockDevice { device: partition_device, capacity },
        b"\\README.txt", 0, EXPECTED,
    ) {
        Ok(n) if n == EXPECTED.len() => {},
        Err(error) => {
            #[cfg(debug_assertions)] unsafe {
                crate::serial_write(b"EXT4 DEBUG: restore data write failed: ");
                crate::serial_write(ext4_error_debug_name(error));
                crate::serial_write(b"\r\n");
            }
            return false;
        },
        _ => return false,
    }
    let mut restored_volume = IoManagerBlockDevice { device: partition_device, capacity };
    let mut restored = [0u8; 128];
    let (restored_size, restored_len) = match extfs::read_root_file_range(
        &mut restored_volume, b"\\README.txt", 0, &mut restored,
    ) {
        Ok(result) => result, Err(_) => return false,
    };
    if restored_size != EXPECTED.len() as u64 || restored_len != EXPECTED.len()
        || &restored[..restored_len] != EXPECTED {
        return false;
    }
    unsafe { crate::serial_write(b"Guest ext4 README.txt restored and verified byte-for-byte after write test.\r\n"); }
    if crate::ntoskrnl::debug::restore_from_storage() {
        unsafe { crate::serial_write(b"NTOSKRNL persistent trace snapshot restored from dedicated GPT partition.\r\n"); }
    } else {
        unsafe { crate::serial_write(b"NTOSKRNL persistent trace snapshot unavailable; starting with volatile trace only.\r\n"); }
    }
    if crate::ntoskrnl::debug::persist_to_storage() {
        unsafe { crate::serial_write(b"NTOSKRNL persistent trace snapshot write/flush passed.\r\n"); }
    } else {
        unsafe { crate::serial_write(b"NTOSKRNL persistent trace snapshot write skipped or failed.\r\n"); }
    }
    if crate::file_services::mount_ext4_volume(partition_device)<0{return false;}

    // Verify that the mounted ext4 volume is reachable through the public
    // FILE_OBJECT/IRP file-service path, not only through the block parser.
    let handle=match crate::file_services::open_for_process(0,b"\\README.txt"){
        Ok(handle)=>handle,
        Err(_)=>return false,
    };
    let mut service_read=[0u8;128];
    let read_result=crate::file_services::read_at_for_process(
        0,handle,0,&mut service_read[..EXPECTED.len()]
    );
    if !matches!(read_result,Ok(n) if n==EXPECTED.len())
        || &service_read[..EXPECTED.len()]!=EXPECTED {
        let _=crate::file_services::close_for_process(0,handle);
        return false;
    }
    let mut changed=[0u8;128];
    changed[..EXPECTED.len()].copy_from_slice(EXPECTED);
    changed[0]=b'W';
    let write_result=crate::file_services::write_at_for_process(
        0,handle,0,&changed[..EXPECTED.len()]
    );
    if !matches!(write_result,Ok(n) if n==EXPECTED.len()) {
        let _=crate::file_services::close_for_process(0,handle);
        return false;
    }
    let mut changed_read=[0u8;128];
    let changed_result=crate::file_services::read_at_for_process(
        0,handle,0,&mut changed_read[..EXPECTED.len()]
    );
    if !matches!(changed_result,Ok(n) if n==EXPECTED.len())
        || &changed_read[..EXPECTED.len()]!=&changed[..EXPECTED.len()] {
        let _=crate::file_services::write_at_for_process(0,handle,0,EXPECTED);
        let _=crate::file_services::close_for_process(0,handle);
        return false;
    }
    if !matches!(crate::file_services::write_at_for_process(0,handle,0,EXPECTED),Ok(n) if n==EXPECTED.len()) {
        let _=crate::file_services::close_for_process(0,handle);
        return false;
    }
    let mut restored=[0u8;128];
    let restored_result=crate::file_services::read_at_for_process(
        0,handle,0,&mut restored[..EXPECTED.len()]
    );
    let close_result=crate::file_services::close_for_process(0,handle);
    if !matches!(restored_result,Ok(n) if n==EXPECTED.len())
        || &restored[..EXPECTED.len()]!=EXPECTED
        || close_result.is_err()
    {return false;}
    unsafe { crate::serial_write(b"Guest ext4 WDM FILE_OBJECT read/write/cache/MDL/cleanup/close path passed.\r\n"); }
    true
}

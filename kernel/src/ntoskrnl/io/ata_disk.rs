//! Early boot ATA PIO disk transport and live GPT/ext4 readback test.
//! This path is deliberately read-only and bounded. It targets QEMU's primary
//! IDE master (the boot image); the primary slave remains reserved for paging.
use cognacos_ext4 as extfs;
use extfs::ReadOnlyBlockDevice;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
static DISK_DEVICE_ID: AtomicU32 = AtomicU32::new(0);
static PARTITION_DEVICE_ID: AtomicU32 = AtomicU32::new(0);
static PARTITION_START: AtomicU64 = AtomicU64::new(0);
static PARTITION_CAPACITY: AtomicU64 = AtomicU64::new(0);
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

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}

/// Read an inode from the validated group descriptor/inode table. This early
/// smoke supports the 256-byte inode format created by the CI ext4 image.
fn read_inode<D: ReadOnlyBlockDevice>(
    dev: &mut D, sb: &extfs::Superblock, ino: u32, out: &mut [u8; 256],
) -> Result<extfs::Inode, extfs::Error> {
    if sb.inode_size > 256 { return Err(extfs::Error::UnsupportedFormat); }
    let (group, within) = sb.inode_location(ino)?;
    let gdt = (sb.first_data_block as u64 + 1)
        .checked_mul(sb.block_size as u64).ok_or(extfs::Error::OutOfRange)?;
    let gd_off = gdt.checked_add((group as u64).checked_mul(sb.descriptor_size as u64).ok_or(extfs::Error::OutOfRange)?)
        .ok_or(extfs::Error::OutOfRange)?;
    let mut gd_bytes = [0u8; 64];
    dev.read_at(gd_off, &mut gd_bytes[..sb.descriptor_size as usize]).map_err(|_| extfs::Error::Io)?;
    let gd = extfs::GroupDescriptor::parse(&gd_bytes, sb)?;
    let off = gd.inode_table.checked_mul(sb.block_size as u64)
        .and_then(|v| v.checked_add(within as u64)).ok_or(extfs::Error::OutOfRange)?;
    out.fill(0);
    dev.read_at(off, &mut out[..sb.inode_size as usize]).map_err(|_| extfs::Error::Io)?;
    extfs::Inode::parse(out, sb.inode_size, sb.revision, sb.feature_ro_compat)
}

/// Resolve a simple filename in the ext root directory and read its extents.
/// It rejects unsupported directory formats instead of guessing at entries.
fn read_root_file<D: ReadOnlyBlockDevice>(
    dev: &mut D, sb: &extfs::Superblock, name: &[u8], out: &mut [u8],
) -> Result<usize, extfs::Error> {
    if name.is_empty() || name.contains(&b'/') || !sb.has_extents() {
        return Err(extfs::Error::UnsupportedFormat);
    }
    let mut inode_raw = [0u8; 256];
    let root = read_inode(dev, sb, 2, &mut inode_raw)?;
    if !root.is_directory || root.size > (sb.block_size as u64) * 64 { return Err(extfs::Error::UnsupportedFormat); }
    let mut block = [0u8; 4096];
    let mut found = None;
    let count = root.size.div_ceil(sb.block_size as u64);
    for logical in 0..count {
        let physical = extfs::map_extent_block(dev, sb, &inode_raw[40..100], logical as u32, &mut block[..sb.block_size as usize])?;
        if let Some(p) = physical {
            extfs::read_block(dev, sb, p, &mut block[..sb.block_size as usize])?;
        } else {
            block[..sb.block_size as usize].fill(0);
        }
        let mut off = 0usize;
        while off < sb.block_size as usize {
            if sb.block_size as usize - off < 8 { return Err(extfs::Error::InvalidDirectoryEntry); }
            let ino = u32_at(&block, off).ok_or(extfs::Error::InvalidDirectoryEntry)?;
            let rec = u16_at(&block, off + 4).ok_or(extfs::Error::InvalidDirectoryEntry)? as usize;
            let nlen = block[off + 6] as usize;
            if rec < 8 || rec & 3 != 0 || rec > sb.block_size as usize - off || nlen > rec - 8 {
                return Err(extfs::Error::InvalidDirectoryEntry);
            }
            if ino != 0 && nlen == name.len() && &block[off + 8..off + 8 + nlen] == name {
                found = Some(ino);
                break;
            }
            off += rec;
        }
        if found.is_some() { break; }
    }
    let file_ino = found.ok_or(extfs::Error::OutOfRange)?;
    let file = read_inode(dev, sb, file_ino, &mut inode_raw)?;
    if !file.is_regular_file || file.size > out.len() as u64 { return Err(extfs::Error::OutOfRange); }
    let length = file.size as usize;
    let mut done = 0usize;
    let mut scratch = [0u8; 4096];
    let count = file.size.div_ceil(sb.block_size as u64);
    for logical in 0..count {
        let n = core::cmp::min(sb.block_size as usize, length - done);
        let physical = extfs::map_extent_block(dev, sb, &inode_raw[40..100], logical as u32, &mut scratch[..sb.block_size as usize])?;
        if let Some(p) = physical {
            extfs::read_block(dev, sb, p, &mut scratch[..sb.block_size as usize])?;
            out[done..done + n].copy_from_slice(&scratch[..n]);
        } else {
            out[done..done + n].fill(0);
        }
        done += n;
    }
    Ok(length)
}

fn success_dispatch(irp: &mut crate::io::Irp) -> crate::io::IoStatus {
    irp.information = 0;
    crate::io::IoStatus::Success
}

fn valid_kernel_buffer(irp: &crate::io::Irp) -> bool {
    irp.user_buffer >= 0xffff_8000_0000_0000
        && irp.length <= crate::io::MAX_IRP_BYTES
        && irp.user_buffer.checked_add(irp.length as u64).is_some()
}

/// Disk class READ dispatch. The current I/O manager passes a kernel-owned
/// system buffer here; user-mode buffers must be captured/validated before IRP
/// dispatch and are never dereferenced directly by this callback.
fn disk_read_dispatch(irp: &mut crate::io::Irp) -> crate::io::IoStatus {
    if !valid_kernel_buffer(irp) { return crate::io::IoStatus::DeviceError; }
    let mut guard = STORAGE_DISK.lock();
    let disk = match guard.as_mut() { Some(d) => d, None => return crate::io::IoStatus::NoSuchDevice };
    let buffer = unsafe { core::slice::from_raw_parts_mut(irp.user_buffer as *mut u8, irp.length) };
    match disk.read_at(irp.byte_offset, buffer) {
        Ok(()) => { irp.information = irp.length; crate::io::IoStatus::Success }
        Err(_) => crate::io::IoStatus::DeviceError,
    }
}

/// Partition class READ dispatch: validate partition-relative bounds, translate
/// to disk byte offsets, and forward through the registered disk object's IRP.
fn partition_read_dispatch(irp: &mut crate::io::Irp) -> crate::io::IoStatus {
    if !valid_kernel_buffer(irp) { return crate::io::IoStatus::DeviceError; }
    let end = match irp.byte_offset.checked_add(irp.length as u64) {
        Some(v) => v, None => return crate::io::IoStatus::DeviceError,
    };
    if end > PARTITION_CAPACITY.load(Ordering::Acquire) { return crate::io::IoStatus::DeviceError; }
    let absolute = match PARTITION_START.load(Ordering::Acquire).checked_add(irp.byte_offset) {
        Some(v) => v, None => return crate::io::IoStatus::DeviceError,
    };
    let disk_id = DISK_DEVICE_ID.load(Ordering::Acquire);
    if disk_id == 0 { return crate::io::IoStatus::NoSuchDevice; }
    let child = match crate::io::submit_read_at(disk_id, absolute, irp.user_buffer as *mut u8, irp.length) {
        Ok(id) => id, Err(_) => return crate::io::IoStatus::DeviceError,
    };
    let dispatched = crate::io::dispatch_one(child);
    let result = crate::io::query_irp(child);
    let _ = crate::io::release_irp(child);
    match (dispatched, result) {
        (Ok(crate::io::IoStatus::Success), Some(r)) if r.status == crate::io::IoStatus::Success && r.information == irp.length => {
            irp.information = irp.length;
            crate::io::IoStatus::Success
        }
        _ => crate::io::IoStatus::DeviceError,
    }
}

fn register_storage_device(name: &[u8], read: crate::io::DispatchFn) -> Result<u32, crate::io::IoError> {
    let mut dispatch = [None; 16];
    dispatch[crate::io::IRP_MJ_CREATE as usize] = Some(success_dispatch);
    dispatch[crate::io::IRP_MJ_CLOSE as usize] = Some(success_dispatch);
    dispatch[crate::io::IRP_MJ_READ as usize] = Some(read);
    crate::io::register_device(name, dispatch)
}

/// Adapter which makes the registered I/O-manager device the sole block-device
/// interface used by GPT and ext4. Reads are synchronous bounded READ IRPs.
struct IoManagerBlockDevice { device_id: u32, capacity: u64 }
impl ReadOnlyBlockDevice for IoManagerBlockDevice {
    type DeviceError = crate::io::IoError;
    fn capacity_bytes(&self) -> u64 { self.capacity }
    fn read_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), Self::DeviceError> {
        if out.len() > crate::io::MAX_IRP_BYTES { return Err(crate::io::IoError::BufferTooLarge); }
        let id = crate::io::submit_read_at(self.device_id, offset, out.as_mut_ptr(), out.len())?;
        let dispatched = crate::io::dispatch_one(id);
        let result = crate::io::query_irp(id);
        let release = crate::io::release_irp(id);
        match (dispatched, result, release) {
            (Ok(crate::io::IoStatus::Success), Some(r), Ok(()))
                if r.status == crate::io::IoStatus::Success && r.information == out.len() => Ok(()),
            (Err(e), _, _) => Err(e),
            (_, _, Err(e)) => Err(e),
            _ => Err(crate::io::IoError::UnknownIrp),
        }
    }
}

/// End-to-end guest check. The physical port driver backs a registered disk
/// object; GPT and ext4 then read only through disk/partition READ IRPs.
pub fn run_ext4_readme_test() -> bool {
    let disk = match AtaPrimaryMaster::initialize() { Ok(d) => d, Err(_) => return false };
    DISK_CAPACITY.store(disk.sectors.saturating_mul(SECTOR_BYTES as u64), Ordering::Release);
    *STORAGE_DISK.lock() = Some(disk);
    let disk_id = match register_storage_device(b"\\Device\\Harddisk0", disk_read_dispatch) {
        Ok(id) => id, Err(_) => return false,
    };
    DISK_DEVICE_ID.store(disk_id, Ordering::Release);
    unsafe { crate::serial_write(b"Storage port: QEMU IDE primary-master IDENTIFY and bounded LBA28 reads initialized.\r\n"); }
    let mut whole_disk = IoManagerBlockDevice { device_id: disk_id, capacity: DISK_CAPACITY.load(Ordering::Acquire) };
    let partition = match extfs::read_gpt_partition(&mut whole_disk, 1, SECTOR_BYTES as u32) {
        Ok(p) => p, Err(_) => return false,
    };
    let start = match partition.start_byte() { Ok(v) => v, Err(_) => return false };
    let capacity = match partition.capacity_bytes() { Ok(v) => v, Err(_) => return false };
    PARTITION_START.store(start, Ordering::Release);
    PARTITION_CAPACITY.store(capacity, Ordering::Release);
    let partition_id = match register_storage_device(b"\\Device\\Harddisk0\\Partition1", partition_read_dispatch) {
        Ok(id) => id, Err(_) => return false,
    };
    PARTITION_DEVICE_ID.store(partition_id, Ordering::Release);
    unsafe { crate::serial_write(b"Disk class: Harddisk0 and Partition1 registered in the kernel I/O manager; GPT data partition discovered by READ IRPs.\r\n"); }
    let mut volume = IoManagerBlockDevice { device_id: partition_id, capacity };
    let sb = match extfs::read_superblock(&mut volume) { Ok(s) => s, Err(_) => return false };
    unsafe { crate::serial_write(b"ext4: partition superblock read through the registered partition READ interface.\r\n"); }
    let mut actual = [0u8; 128];
    let length = match read_root_file(&mut volume, &sb, b"README.txt", &mut actual) {
        Ok(n) => n, Err(_) => return false,
    };
    if length != EXPECTED.len() || &actual[..length] != EXPECTED { return false; }
    unsafe { crate::serial_write(b"Guest ext4 README.txt bytes match expected content exactly.\r\n"); }
    true
}

//! Fixed-size ATA PIO pagefile device for the early x86-64 kernel.
//!
//! The pagefile is a dedicated raw primary-channel slave disk (LBA28), not a
//! filesystem file. Its sectors must never be shared with a mounted volume.
//! This keeps paging I/O independent of filesystem locks and avoids recursive
//! faults while servicing a page fault.
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const SECTOR_SIZE: usize = 512;
const SECTORS_PER_PAGE: u32 = 8;
const PAGE_SIZE: usize = SECTOR_SIZE * SECTORS_PER_PAGE as usize;
const MAX_SLOTS: usize = 32768; // 128 MiB
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
const CMD_READ: u8 = 0x20;
const CMD_WRITE: u8 = 0x30;
const BSY: u8 = 0x80;
const DRQ: u8 = 0x08;
const ERR: u8 = 0x01;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static DEVICE_SECTORS: AtomicU32 = AtomicU32::new(0);
static SLOT_COUNT: AtomicU32 = AtomicU32::new(0);
static SLOTS: [AtomicBool; MAX_SLOTS] = [const { AtomicBool::new(false) }; MAX_SLOTS];
static IO_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Offline, OutOfSlots, InvalidSlot, DeviceTimeout, DeviceError, InvalidBuffer }

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
#[inline]
unsafe fn out16(port: u16, value: u16) {
    unsafe { core::arch::asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags)); }
}
fn delay_400ns() { unsafe { let _ = in8(ALT_STATUS); let _ = in8(ALT_STATUS); let _ = in8(ALT_STATUS); let _ = in8(ALT_STATUS); } }

fn wait_not_busy() -> Result<u8, Error> {
    for _ in 0..10_000_000usize {
        let status = unsafe { in8(STATUS) };
        if status == 0xff { return Err(Error::Offline); }
        if status & BSY == 0 {
            if status & ERR != 0 { return Err(Error::DeviceError); }
            return Ok(status);
        }
        core::hint::spin_loop();
    }
    Err(Error::DeviceTimeout)
}
fn wait_drq() -> Result<(), Error> {
    for _ in 0..10_000_000usize {
        let status = unsafe { in8(STATUS) };
        if status == 0xff { return Err(Error::Offline); }
        if status & BSY == 0 {
            if status & ERR != 0 { let _ = unsafe { in8(ERROR) }; return Err(Error::DeviceError); }
            if status & DRQ != 0 { return Ok(()); }
        }
        core::hint::spin_loop();
    }
    Err(Error::DeviceTimeout)
}

fn identify_pagefile_disk() -> Result<u32, Error> {
    unsafe {
        out8(DRIVE, 0xf0); // primary-channel slave, LBA mode
        delay_400ns();
        out8(SECTOR_COUNT, 0);
        out8(LBA_LOW, 0);
        out8(LBA_MID, 0);
        out8(LBA_HIGH, 0);
        out8(COMMAND, CMD_IDENTIFY);
    }
    let status = unsafe { in8(STATUS) };
    if status == 0 || status == 0xff { return Err(Error::Offline); }
    wait_not_busy()?;
    wait_drq()?;
    let mut words = [0u16; 256];
    for word in words.iter_mut() { *word = unsafe { in16(DATA) }; }
    let lba_supported = words[49] & (1 << 9) != 0;
    let sectors = (words[60] as u32) | ((words[61] as u32) << 16);
    if !lba_supported || sectors < SECTORS_PER_PAGE { return Err(Error::Offline); }
    Ok(sectors)
}

/// Probe the dedicated raw pagefile disk. Returns false when no safe backing
/// device is attached; in that case page-out is disabled instead of touching
/// the boot disk or guessing at unallocated sectors.
pub fn initialize() -> bool {
    if ACTIVE.load(Ordering::Acquire) { return true; }
    let _guard = IO_LOCK.lock();
    let sectors = match identify_pagefile_disk() { Ok(n) => n, Err(_) => return false };
    let slots = core::cmp::min((sectors / SECTORS_PER_PAGE) as usize, MAX_SLOTS);
    if slots == 0 { return false; }
    for slot in SLOTS.iter() { slot.store(false, Ordering::Relaxed); }
    DEVICE_SECTORS.store(sectors, Ordering::Release);
    SLOT_COUNT.store(slots as u32, Ordering::Release);
    ACTIVE.store(true, Ordering::Release);
    true
}

#[inline]
pub fn is_active() -> bool { ACTIVE.load(Ordering::Acquire) }
#[inline]
pub fn capacity_slots() -> u32 { SLOT_COUNT.load(Ordering::Acquire) }

pub fn allocate_slot() -> Result<u32, Error> {
    if !is_active() { return Err(Error::Offline); }
    let count = SLOT_COUNT.load(Ordering::Acquire) as usize;
    for (index, slot) in SLOTS.iter().enumerate().take(count) {
        if slot.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
            return Ok(index as u32);
        }
    }
    Err(Error::OutOfSlots)
}

pub fn free_slot(slot: u32) -> Result<(), Error> {
    if slot as usize >= SLOT_COUNT.load(Ordering::Acquire) as usize { return Err(Error::InvalidSlot); }
    if !SLOTS[slot as usize].swap(false, Ordering::AcqRel) { return Err(Error::InvalidSlot); }
    Ok(())
}

fn transfer(slot: u32, buffer: *mut u8, write: bool) -> Result<(), Error> {
    if buffer.is_null() { return Err(Error::InvalidBuffer); }
    if !is_active() { return Err(Error::Offline); }
    if slot as usize >= SLOT_COUNT.load(Ordering::Acquire) as usize || !SLOTS[slot as usize].load(Ordering::Acquire) {
        return Err(Error::InvalidSlot);
    }
    let lba = slot.checked_mul(SECTORS_PER_PAGE).ok_or(Error::InvalidSlot)?;
    if lba.checked_add(SECTORS_PER_PAGE).filter(|end| *end <= DEVICE_SECTORS.load(Ordering::Acquire)).is_none() {
        return Err(Error::InvalidSlot);
    }
    let _guard = IO_LOCK.lock();
    unsafe {
        out8(DRIVE, 0xf0 | ((lba >> 24) as u8 & 0x0f));
        delay_400ns();
        out8(SECTOR_COUNT, SECTORS_PER_PAGE as u8);
        out8(LBA_LOW, lba as u8);
        out8(LBA_MID, (lba >> 8) as u8);
        out8(LBA_HIGH, (lba >> 16) as u8);
        out8(COMMAND, if write { CMD_WRITE } else { CMD_READ });
    }
    delay_400ns();
    for sector in 0..SECTORS_PER_PAGE as usize {
        wait_drq()?;
        let words = unsafe { core::slice::from_raw_parts_mut(buffer.add(sector * SECTOR_SIZE).cast::<u16>(), SECTOR_SIZE / 2) };
        if write {
            for word in words.iter() { unsafe { out16(DATA, *word); } }
        } else {
            for word in words.iter_mut() { *word = unsafe { in16(DATA) }; }
        }
    }
    let _ = wait_not_busy()?;
    if write {
        unsafe { out8(COMMAND, 0xe7); } // FLUSH CACHE
        let _ = wait_not_busy()?;
    }
    Ok(())
}

/// Read one allocated pagefile slot into a physical page or other writable buffer.
pub unsafe fn read_page(slot: u32, destination: *mut u8) -> Result<(), Error> {
    transfer(slot, destination, false)
}

/// Write one allocated pagefile slot from a physical page or other readable buffer.
pub unsafe fn write_page(slot: u32, source: *const u8) -> Result<(), Error> {
    transfer(slot, source as *mut u8, true)
}

/// Exercise a real pagefile sector round-trip before enabling eviction.
pub fn self_test() -> bool {
    if !is_active() { return false; }
    let slot = match allocate_slot() { Ok(v) => v, Err(_) => return false };
    let page = match unsafe { crate::memory::allocate_page() } {
        Ok(p) => p,
        Err(_) => { let _ = free_slot(slot); return false; }
    };
    unsafe {
        for i in 0..PAGE_SIZE { (page as *mut u8).add(i).write((i as u8).wrapping_mul(31).wrapping_add(7)); }
    }
    let written = unsafe { write_page(slot, page as *const u8) }.is_ok();
    unsafe { core::ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE); }
    let read = written && unsafe { read_page(slot, page as *mut u8) }.is_ok();
    let valid = read && (0..PAGE_SIZE).all(|i| unsafe { (page as *const u8).add(i).read() } == (i as u8).wrapping_mul(31).wrapping_add(7));
    let freed_page = unsafe { crate::memory::free_page(page) }.is_ok();
    let freed_slot = free_slot(slot).is_ok();
    valid && freed_page && freed_slot
}

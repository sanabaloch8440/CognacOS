//! Memory Manager pagefile slot accounting.
//!
//! Physical page I/O is delegated to the I/O manager's registered paging
//! backend. This module contains no ATA register access, port I/O, filesystem
//! access, or assumptions about a particular controller/device. Page-out stays
//! disabled until a storage stack registers a resident, non-recursive backend.

use core::sync::atomic::{AtomicBool, Ordering};

const PAGE_SIZE: usize = crate::ntoskrnl::io::paging::PAGE_SIZE;
const MAX_SLOTS: usize = 32768;

static SLOTS: [AtomicBool; MAX_SLOTS] =
    [const { AtomicBool::new(false) }; MAX_SLOTS];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Offline,
    OutOfSlots,
    InvalidSlot,
    DeviceFailure,
    InvalidBuffer,
}

#[inline]
pub fn initialize() -> bool {
    is_active()
}

#[inline]
pub fn is_active() -> bool {
    crate::ntoskrnl::io::paging::pagefile_capacity_slots() != 0
}

#[inline]
pub fn capacity_slots() -> u32 {
    crate::ntoskrnl::io::paging::pagefile_capacity_slots()
}

pub fn allocate_slot() -> Result<u32, Error> {
    let count = capacity_slots() as usize;
    if count == 0 { return Err(Error::Offline); }
    for (index, slot) in SLOTS.iter().enumerate().take(count) {
        if slot.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
            return Ok(index as u32);
        }
    }
    Err(Error::OutOfSlots)
}

pub fn free_slot(slot: u32) -> Result<(), Error> {
    if slot >= capacity_slots() { return Err(Error::InvalidSlot); }
    if !SLOTS[slot as usize].swap(false, Ordering::AcqRel) {
        return Err(Error::InvalidSlot);
    }
    Ok(())
}

fn validate_slot(slot: u32, buffer: *const u8) -> Result<(), Error> {
    if buffer.is_null() || buffer as usize & (PAGE_SIZE - 1) != 0 {
        return Err(Error::InvalidBuffer);
    }
    if slot >= capacity_slots() || !SLOTS[slot as usize].load(Ordering::Acquire) {
        return Err(Error::InvalidSlot);
    }
    Ok(())
}

/// Read one allocated paging slot into a resident, page-aligned physical page.
///
/// # Safety
/// The destination must remain mapped and exclusively writable for the complete
/// synchronous paging operation. The backend must not recursively fault.
pub unsafe fn read_page(slot: u32, destination: *mut u8) -> Result<(), Error> {
    validate_slot(slot, destination)?;
    unsafe { crate::ntoskrnl::io::paging::read_pagefile_page(slot, destination) }
        .map_err(|_| Error::DeviceFailure)
}

/// Write one allocated paging slot from a resident, page-aligned physical page.
///
/// # Safety
/// The source must remain mapped and stable for the complete synchronous
/// operation. The backend must not recursively fault.
pub unsafe fn write_page(slot: u32, source: *const u8) -> Result<(), Error> {
    validate_slot(slot, source)?;
    unsafe { crate::ntoskrnl::io::paging::write_pagefile_page(slot, source) }
        .map_err(|_| Error::DeviceFailure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paging_is_offline_without_registered_storage_backend() {
        // The test harness does not register a storage device implicitly.
        assert_eq!(capacity_slots(), 0);
        assert!(!initialize());
        assert_eq!(allocate_slot(), Err(Error::Offline));
    }

    #[test]
    fn slot_release_rejects_unallocated_and_out_of_range_slots() {
        assert_eq!(free_slot(u32::MAX), Err(Error::InvalidSlot));
    }

    #[test]
    fn page_buffers_must_be_aligned_and_non_null() {
        assert_eq!(validate_slot(0, core::ptr::null()), Err(Error::InvalidBuffer));
        assert_eq!(validate_slot(0, 1usize as *const u8), Err(Error::InvalidBuffer));
    }
}

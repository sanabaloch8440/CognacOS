//! Non-recursive paging I/O registration point owned by the I/O manager.
//!
//! A backend is supplied by a storage stack only after its paging path is ready.
//! Callbacks must be resident, nonblocking at the caller's execution level, and
//! must use locked physical buffers without issuing filesystem/cache-manager I/O.
//! The registration is immutable for this boot: teardown is deliberately absent
//! until the storage stack has device-removal and outstanding-I/O rundown support.

use core::sync::atomic::{AtomicBool, Ordering};

pub const PAGE_SIZE: usize = 4096;
pub type ReadPage = unsafe fn(context: usize, slot: u32, destination: *mut u8) -> i32;
pub type WritePage = unsafe fn(context: usize, slot: u32, source: *const u8) -> i32;

#[derive(Clone, Copy)]
pub struct PagingBackend {
    pub context: usize,
    pub slot_count: u32,
    pub read_page: ReadPage,
    pub write_page: WritePage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagingIoError {
    InvalidBackend,
    AlreadyRegistered,
    Offline,
    InvalidSlot,
    InvalidBuffer,
    DeviceFailure,
}

static REGISTERED: AtomicBool = AtomicBool::new(false);
static BACKEND: crate::hal::IrqSpinLock<Option<PagingBackend>> =
    crate::hal::IrqSpinLock::new(None);

impl PagingBackend {
    pub fn validate(self) -> Result<Self, PagingIoError> {
        if self.slot_count == 0 || self.slot_count > 32768 {
            return Err(PagingIoError::InvalidBackend);
        }
        Ok(self)
    }
}

/// Register a resident page-I/O backend. The provider must guarantee that its
/// callback path does not fault, allocate pageable memory, or recurse into Mm/Cc.
pub fn register_pagefile_backend(backend: PagingBackend) -> Result<(), PagingIoError> {
    let backend = backend.validate()?;
    let mut current = BACKEND.lock();
    if REGISTERED.load(Ordering::Acquire) || current.is_some() {
        return Err(PagingIoError::AlreadyRegistered);
    }
    *current = Some(backend);
    REGISTERED.store(true, Ordering::Release);
    Ok(())
}

fn backend() -> Result<PagingBackend, PagingIoError> {
    if !REGISTERED.load(Ordering::Acquire) {
        return Err(PagingIoError::Offline);
    }
    BACKEND.lock().as_ref().copied().ok_or(PagingIoError::Offline)
}

pub fn pagefile_capacity_slots() -> u32 {
    backend().map(|value| value.slot_count).unwrap_or(0)
}

pub unsafe fn read_pagefile_page(slot: u32, destination: *mut u8) -> Result<(), PagingIoError> {
    if destination.is_null() || destination as usize & (PAGE_SIZE - 1) != 0 {
        return Err(PagingIoError::InvalidBuffer);
    }
    let provider = backend()?;
    if slot >= provider.slot_count { return Err(PagingIoError::InvalidSlot); }
    let status = unsafe { (provider.read_page)(provider.context, slot, destination) };
    if status < 0 { return Err(PagingIoError::DeviceFailure); }
    Ok(())
}

pub unsafe fn write_pagefile_page(slot: u32, source: *const u8) -> Result<(), PagingIoError> {
    if source.is_null() || source as usize & (PAGE_SIZE - 1) != 0 {
        return Err(PagingIoError::InvalidBuffer);
    }
    let provider = backend()?;
    if slot >= provider.slot_count { return Err(PagingIoError::InvalidSlot); }
    let status = unsafe { (provider.write_page)(provider.context, slot, source) };
    if status < 0 { return Err(PagingIoError::DeviceFailure); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn unused_read(_: usize, _: u32, _: *mut u8) -> i32 { 0 }
    unsafe fn unused_write(_: usize, _: u32, _: *const u8) -> i32 { 0 }

    #[test]
    fn backend_capacity_is_bounded_and_nonzero() {
        let valid = PagingBackend { context: 0, slot_count: 1, read_page: unused_read, write_page: unused_write };
        assert!(valid.validate().is_ok());
        assert_eq!(
            PagingBackend { slot_count: 0, ..valid }.validate(),
            Err(PagingIoError::InvalidBackend)
        );
        assert_eq!(
            PagingBackend { slot_count: 32769, ..valid }.validate(),
            Err(PagingIoError::InvalidBackend)
        );
    }
}

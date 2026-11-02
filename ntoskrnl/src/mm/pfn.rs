//! PFN ownership database for frames allocated by the physical-page allocator.
//!
//! Each tracked frame starts with one allocation-owner reference. Mapping and
//! MDL pin references are independent; the owner may release its reference only
//! after all mappings and pins have been dropped. This database deliberately
//! fails closed on exhaustion, duplicate registration, underflow, and stale PFNs.
//!
//! The fixed-capacity table is bootstrap metadata, not a claim that every
//! firmware-described page has a resident PFN record. The allocator registers
//! every frame returned by its single-page allocation entry points; contiguous
//! and node/range allocators must use the same registration hooks before they
//! are considered fully covered by this milestone.

#[cfg(target_os = "none")]
use crate::hal::{IrqSpinLock, IrqSpinLockGuard};
#[cfg(not(target_os = "none"))]
use core::sync::atomic::{AtomicBool, Ordering};
use core::ops::{Deref, DerefMut};

const CAPACITY: usize = 65_536;
const EMPTY: u8 = 0;
const LIVE: u8 = 1;
const TOMBSTONE: u8 = 2;

#[derive(Clone, Copy)]
struct Entry {
    page: u64,
    references: u32,
    mappings: u32,
    pins: u32,
    state: u8,
}
impl Entry {
    const EMPTY: Self = Self { page: 0, references: 0, mappings: 0, pins: 0, state: EMPTY };
}

struct Database {
    #[cfg(target_os = "none")]
    lock: IrqSpinLock<()>,
    #[cfg(not(target_os = "none"))]
    lock: AtomicBool,
    entries: core::cell::UnsafeCell<[Entry; CAPACITY]>,
}
unsafe impl Sync for Database {}
static DATABASE: Database = Database {
    #[cfg(target_os = "none")]
    lock: IrqSpinLock::new(()),
    #[cfg(not(target_os = "none"))]
    lock: AtomicBool::new(false),
    entries: core::cell::UnsafeCell::new([Entry::EMPTY; CAPACITY]),
};

struct DatabaseGuard<'a> {
    database: &'a Database,
    #[cfg(target_os = "none")]
    _irq: IrqSpinLockGuard<'a, ()>,
}
impl Deref for DatabaseGuard<'_> {
    type Target = [Entry; CAPACITY];
    fn deref(&self) -> &Self::Target { unsafe { &*self.database.entries.get() } }
}
impl DerefMut for DatabaseGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target { unsafe { &mut *self.database.entries.get() } }
}
impl Drop for DatabaseGuard<'_> {
    fn drop(&mut self) {
        #[cfg(not(target_os = "none"))]
        self.database.lock.store(false, Ordering::Release);
    }
}
impl Database {
    fn lock(&self) -> DatabaseGuard<'_> {
        #[cfg(target_os = "none")]
        {
            let irq = self.lock.lock();
            DatabaseGuard { database: self, _irq: irq }
        }
        #[cfg(not(target_os = "none"))]
        {
            while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
                core::hint::spin_loop();
            }
            DatabaseGuard { database: self }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PfnError {
    InvalidPage,
    AlreadyRegistered,
    NotTracked,
    DatabaseFull,
    ReferenceOverflow,
    ReferenceUnderflow,
    StillReferenced,
    MappingUnderflow,
    PinUnderflow,
}

#[inline]
fn hash(page: u64) -> usize {
    let pfn = page >> 12;
    ((pfn ^ (pfn >> 17) ^ (pfn >> 31)).wrapping_mul(0x9e37_79b9_7f4a_7c15) as usize) & (CAPACITY - 1)
}

fn find(entries: &[Entry; CAPACITY], page: u64) -> Option<usize> {
    let start = hash(page);
    for offset in 0..CAPACITY {
        let index = (start + offset) & (CAPACITY - 1);
        let entry = entries[index];
        if entry.state == EMPTY { return None; }
        if entry.state == LIVE && entry.page == page { return Some(index); }
    }
    None
}

/// Register a freshly allocated physical page with one owner reference.
/// Called before the allocator publishes the page to its caller.
pub fn register_allocation(page: u64) -> Result<(), PfnError> {
    if page == 0 || page & 0xfff != 0 { return Err(PfnError::InvalidPage); }
    let mut entries = DATABASE.lock();
    let start = hash(page);
    let mut tombstone = None;
    for offset in 0..CAPACITY {
        let index = (start + offset) & (CAPACITY - 1);
        let entry = entries[index];
        if entry.state == LIVE && entry.page == page { return Err(PfnError::AlreadyRegistered); }
        if entry.state == TOMBSTONE && tombstone.is_none() { tombstone = Some(index); }
        if entry.state == EMPTY {
            let target = tombstone.unwrap_or(index);
            entries[target] = Entry { page, references: 1, mappings: 0, pins: 0, state: LIVE };
            return Ok(());
        }
    }
    if let Some(index) = tombstone {
        entries[index] = Entry { page, references: 1, mappings: 0, pins: 0, state: LIVE };
        Ok(())
    } else {
        Err(PfnError::DatabaseFull)
    }
}

/// Register every frame in a contiguous allocation as one transaction.
pub fn register_allocation_range(base: u64, pages: usize) -> Result<(), PfnError> {
    if base == 0 || base & 0xfff != 0 || pages == 0 {
        return Err(PfnError::InvalidPage);
    }
    let bytes = (pages as u64).checked_mul(4096).ok_or(PfnError::InvalidPage)?;
    base.checked_add(bytes).ok_or(PfnError::InvalidPage)?;
    let mut entries = DATABASE.lock();
    let mut reusable = 0usize;
    let mut live = 0usize;
    for entry in entries.iter() {
        match entry.state {
            LIVE => live += 1,
            EMPTY | TOMBSTONE => reusable += 1,
            _ => {},
        }
    }
    if reusable < pages || live.checked_add(pages).is_none_or(|count| count > CAPACITY) {
        return Err(PfnError::DatabaseFull);
    }
    for offset in 0..pages {
        let page = base + offset as u64 * 4096;
        if find(&entries, page).is_some() { return Err(PfnError::AlreadyRegistered); }
    }
    for offset in 0..pages {
        let page = base + offset as u64 * 4096;
        let start = hash(page);
        let mut tombstone = None;
        for probe in 0..CAPACITY {
            let index = (start + probe) & (CAPACITY - 1);
            if entries[index].state == TOMBSTONE && tombstone.is_none() {
                tombstone = Some(index);
            }
            if entries[index].state == EMPTY {
                let target = tombstone.unwrap_or(index);
                entries[target] = Entry { page, references: 1, mappings: 0, pins: 0, state: LIVE };
                tombstone = None;
                break;
            }
        }
        if let Some(index) = tombstone {
            entries[index] = Entry { page, references: 1, mappings: 0, pins: 0, state: LIVE };
        }
    }
    Ok(())
}

/// Release a contiguous allocation only if every frame has its sole owner reference.
pub fn release_allocation_range(base: u64, pages: usize) -> Result<(), PfnError> {
    if base == 0 || base & 0xfff != 0 || pages == 0 {
        return Err(PfnError::InvalidPage);
    }
    let bytes = (pages as u64).checked_mul(4096).ok_or(PfnError::InvalidPage)?;
    base.checked_add(bytes).ok_or(PfnError::InvalidPage)?;
    let mut entries = DATABASE.lock();
    for offset in 0..pages {
        let page = base + offset as u64 * 4096;
        let index = find(&entries, page).ok_or(PfnError::NotTracked)?;
        let entry = entries[index];
        if entry.references != 1 || entry.mappings != 0 || entry.pins != 0 {
            return Err(PfnError::StillReferenced);
        }
    }
    for offset in 0..pages {
        let page = base + offset as u64 * 4096;
        let index = find(&entries, page).ok_or(PfnError::NotTracked)?;
        entries[index] = Entry { state: TOMBSTONE, ..Entry::EMPTY };
    }
    Ok(())
}

/// Register an arbitrary batch of freshly allocated frames atomically.
/// Duplicate input PFNs, pre-existing live records, and capacity exhaustion
/// reject the whole batch without publishing a partial owner set.
pub fn register_allocations(pages: &[u64]) -> Result<(), PfnError> {
    if pages.is_empty() { return Err(PfnError::InvalidPage); }
    for (index, page) in pages.iter().copied().enumerate() {
        if page == 0 || page & 0xfff != 0 { return Err(PfnError::InvalidPage); }
        if pages[..index].contains(&page) { return Err(PfnError::AlreadyRegistered); }
    }
    let mut entries = DATABASE.lock();
    let mut reusable = 0usize;
    let mut live = 0usize;
    for entry in entries.iter() {
        match entry.state {
            LIVE => live += 1,
            EMPTY | TOMBSTONE => reusable += 1,
            _ => {},
        }
    }
    if reusable < pages.len() || live.checked_add(pages.len()).is_none_or(|count| count > CAPACITY) {
        return Err(PfnError::DatabaseFull);
    }
    for page in pages.iter().copied() {
        if find(&entries, page).is_some() { return Err(PfnError::AlreadyRegistered); }
    }
    for page in pages.iter().copied() {
        let start = hash(page);
        let mut tombstone = None;
        for probe in 0..CAPACITY {
            let index = (start + probe) & (CAPACITY - 1);
            if entries[index].state == TOMBSTONE && tombstone.is_none() {
                tombstone = Some(index);
            }
            if entries[index].state == EMPTY {
                let target = tombstone.unwrap_or(index);
                entries[target] = Entry { page, references: 1, mappings: 0, pins: 0, state: LIVE };
                tombstone = None;
                break;
            }
        }
        if let Some(index) = tombstone {
            entries[index] = Entry { page, references: 1, mappings: 0, pins: 0, state: LIVE };
        }
    }
    Ok(())
}

/// Acquire a reference owned by a valid hardware mapping.
pub fn acquire_mapping(page: u64) -> Result<(), PfnError> {
    let mut entries = DATABASE.lock();
    let index = find(&entries, page).ok_or(PfnError::NotTracked)?;
    let entry = &mut entries[index];
    entry.references = entry.references.checked_add(1).ok_or(PfnError::ReferenceOverflow)?;
    entry.mappings = match entry.mappings.checked_add(1) {
        Some(value) => value,
        None => { entry.references -= 1; return Err(PfnError::ReferenceOverflow); }
    };
    Ok(())
}

/// Drop a mapping reference only after its PTE has been invalidated and TLBs flushed.
pub fn release_mapping(page: u64) -> Result<(), PfnError> {
    let mut entries = DATABASE.lock();
    let index = find(&entries, page).ok_or(PfnError::NotTracked)?;
    let entry = &mut entries[index];
    if entry.mappings == 0 { return Err(PfnError::MappingUnderflow); }
    if entry.references <= 1 { return Err(PfnError::ReferenceUnderflow); }
    entry.mappings -= 1;
    entry.references -= 1;
    Ok(())
}

/// Acquire an MDL/I/O pin reference. Pins prevent final owner release.
pub fn pin_page(page: u64) -> Result<(), PfnError> {
    let mut entries = DATABASE.lock();
    let index = find(&entries, page).ok_or(PfnError::NotTracked)?;
    let entry = &mut entries[index];
    entry.references = entry.references.checked_add(1).ok_or(PfnError::ReferenceOverflow)?;
    entry.pins = match entry.pins.checked_add(1) {
        Some(value) => value,
        None => { entry.references -= 1; return Err(PfnError::ReferenceOverflow); }
    };
    Ok(())
}

pub fn unpin_page(page: u64) -> Result<(), PfnError> {
    let mut entries = DATABASE.lock();
    let index = find(&entries, page).ok_or(PfnError::NotTracked)?;
    let entry = &mut entries[index];
    if entry.pins == 0 { return Err(PfnError::PinUnderflow); }
    if entry.references <= 1 { return Err(PfnError::ReferenceUnderflow); }
    entry.pins -= 1;
    entry.references -= 1;
    Ok(())
}

/// Release the original allocation-owner reference. A live mapping or MDL pin
/// makes this fail; the allocator must not put the frame back on a free list.
pub fn release_allocation(page: u64) -> Result<(), PfnError> {
    let mut entries = DATABASE.lock();
    let index = find(&entries, page).ok_or(PfnError::NotTracked)?;
    let entry = &mut entries[index];
    if entry.references != 1 || entry.mappings != 0 || entry.pins != 0 {
        return Err(PfnError::StillReferenced);
    }
    *entry = Entry { state: TOMBSTONE, ..Entry::EMPTY };
    Ok(())
}

pub fn snapshot(page: u64) -> Option<(u32, u32, u32)> {
    let entries = DATABASE.lock();
    find(&entries, page).map(|index| {
        let entry = entries[index];
        (entry.references, entry.mappings, entry.pins)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings_and_pins_block_owner_release_until_dropped() {
        let page = 0x1234_5000;
        register_allocation(page).unwrap();
        acquire_mapping(page).unwrap();
        pin_page(page).unwrap();
        assert_eq!(snapshot(page), Some((3, 1, 1)));
        assert_eq!(release_allocation(page), Err(PfnError::StillReferenced));
        release_mapping(page).unwrap();
        assert_eq!(release_allocation(page), Err(PfnError::StillReferenced));
        unpin_page(page).unwrap();
        assert_eq!(snapshot(page), Some((1, 0, 0)));
        release_allocation(page).unwrap();
        assert_eq!(snapshot(page), None);
    }

    #[test]
    fn arbitrary_batch_registration_is_atomic_and_tracks_every_frame() {
        let first = 0x3234_5000;
        let second = 0x3234_6000;
        assert_eq!(register_allocations(&[first, second]), Ok(()));
        assert_eq!(snapshot(first), Some((1, 0, 0)));
        assert_eq!(snapshot(second), Some((1, 0, 0)));
        release_allocation(first).unwrap();
        release_allocation(second).unwrap();

        let third = 0x3234_7000;
        assert_eq!(register_allocations(&[third, third]), Err(PfnError::AlreadyRegistered));
        assert_eq!(snapshot(third), None);
        assert_eq!(register_allocations(&[third, third + 1]), Err(PfnError::InvalidPage));
        assert_eq!(snapshot(third), None);
    }

    #[test]
    fn duplicate_registration_and_reference_underflow_are_rejected() {
        let page = 0x2234_5000;
        register_allocation(page).unwrap();
        assert_eq!(register_allocation(page), Err(PfnError::AlreadyRegistered));
        assert_eq!(release_mapping(page), Err(PfnError::MappingUnderflow));
        assert_eq!(unpin_page(page), Err(PfnError::PinUnderflow));
        release_allocation(page).unwrap();
    }
}

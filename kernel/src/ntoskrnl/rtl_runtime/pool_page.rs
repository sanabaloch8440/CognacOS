//! Virtual-memory-backed NT pool backend.
//!
//! Small blocks share zeroed physical pages through power-of-two slabs; large
//! allocations use contiguous page extents. A locked ownership table validates
//! exact returned pointers before metadata is touched. The bootstrap still only
//! identity maps physical memory below 4 GiB and this early allocator is bounded.
use core::cell::UnsafeCell;
#[cfg(not(target_os = "none"))]
use core::sync::atomic::{AtomicBool, Ordering};

const PAGE_SIZE: usize = 4096;
const SMALL_MAX: usize = 2048;
const MIN_BLOCK: usize = 16;
const MAX_ALLOCATIONS: usize = 4096;
const MAX_SLABS: usize = 1024;
const MAX_PAGES_PER_ALLOCATION: usize = 65536;
const NO_SLAB: u16 = u16::MAX;
const BITMAP_WORDS: usize = PAGE_SIZE / MIN_BLOCK / 64;

#[derive(Clone, Copy)]
struct Allocation {
    base: u64,
    backing_base: u64,
    pages: usize,
    requested: usize,
    tag: u32,
    slab_index: u16,
    slab_slot: u16,
    live: bool,
}
impl Allocation {
    const EMPTY: Self = Self {
        base: 0, backing_base: 0, pages: 0, requested: 0, tag: 0,
        slab_index: NO_SLAB, slab_slot: 0, live: false,
    };
}

#[derive(Clone, Copy)]
struct Slab {
    base: u64,
    block_size: usize,
    slots: usize,
    pageable: bool,
    used: [u64; BITMAP_WORDS],
    live: bool,
}
impl Slab {
    const EMPTY: Self = Self { base: 0, block_size: 0, slots: 0, pageable: false, used: [0; BITMAP_WORDS], live: false };
    fn find_free_slot(&self) -> Option<usize> {
        (0..self.slots).find(|&slot| self.used[slot / 64] & (1u64 << (slot % 64)) == 0)
    }
    fn mark_used(&mut self, slot: usize) { self.used[slot / 64] |= 1u64 << (slot % 64); }
    fn mark_free(&mut self, slot: usize) { self.used[slot / 64] &= !(1u64 << (slot % 64)); }
    fn is_empty_except(&self, slot: usize) -> bool {
        self.used.iter().enumerate().all(|(word, bits)| {
            let mask = if word == slot / 64 { !(1u64 << (slot % 64)) } else { u64::MAX };
            bits & mask == 0
        })
    }
}

struct AllocationTable(UnsafeCell<[Allocation; MAX_ALLOCATIONS]>);
unsafe impl Sync for AllocationTable {}
static ALLOCATIONS: AllocationTable = AllocationTable(UnsafeCell::new([Allocation::EMPTY; MAX_ALLOCATIONS]));
struct SlabTable(UnsafeCell<[Slab; MAX_SLABS]>);
unsafe impl Sync for SlabTable {}
static SLABS: SlabTable = SlabTable(UnsafeCell::new([Slab::EMPTY; MAX_SLABS]));

#[cfg(target_os = "none")]
static LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
#[cfg(not(target_os = "none"))]
static LOCK: AtomicBool = AtomicBool::new(false);

struct Guard {
    #[cfg(target_os = "none")]
    _guard: crate::hal::IrqSpinLockGuard<'static, ()>,
}
impl Guard {
    fn acquire() -> Self {
        #[cfg(target_os = "none")]
        { Self { _guard: LOCK.lock() } }
        #[cfg(not(target_os = "none"))]
        {
            while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
                core::hint::spin_loop();
            }
            Self {}
        }
    }
}
#[cfg(not(target_os = "none"))]
impl Drop for Guard {
    fn drop(&mut self) { LOCK.store(false, Ordering::Release); }
}

fn small_class(size: usize) -> Option<usize> {
    if size == 0 || size > SMALL_MAX { return None; }
    size.max(MIN_BLOCK).checked_next_power_of_two().filter(|n| *n <= SMALL_MAX)
}

/// Try to allocate only from an already-created nonpaged slab. This path never
/// creates backing pages and never waits for the pool lock, so it is suitable
/// for the bounded resource acquisition phase of MM_ALLOCATE_NO_WAIT.
fn try_allocate_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 {
    let Some(class_size) = small_class(size) else { return core::ptr::null_mut(); };
    #[cfg(target_os = "none")]
    let _guard = match LOCK.try_lock() {
        Some(guard) => Guard { _guard: guard },
        None => return core::ptr::null_mut(),
    };
    #[cfg(not(target_os = "none"))]
    {
        if LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            return core::ptr::null_mut();
        }
    }
    #[cfg(not(target_os = "none"))]
    let _guard = Guard {};
    let table = unsafe { &mut *ALLOCATIONS.0.get() };
    let Some(allocation_index) = table.iter().position(|a| !a.live) else {
        return core::ptr::null_mut();
    };
    let slabs = unsafe { &mut *SLABS.0.get() };
    let Some(slab_index) = slabs.iter().position(|s|
        s.live && !s.pageable && s.block_size == class_size && s.find_free_slot().is_some()
    ) else {
        return core::ptr::null_mut();
    };
    let slab = &mut slabs[slab_index];
    let Some(slot) = slab.find_free_slot() else { return core::ptr::null_mut(); };
    slab.mark_used(slot);
    let base = slab.base + (slot * class_size) as u64;
    table[allocation_index] = Allocation {
        base, backing_base: slab.base, pages: 1, requested: size, tag,
        slab_index: slab_index as u16, slab_slot: slot as u16, live: true,
    };
    if zeroed { unsafe { core::ptr::write_bytes(base as *mut u8, 0, size); } }
    base as *mut u8
}


/// Allocate nonpaged storage. Small allocations share slab pages; allocations
/// larger than 2 KiB receive their own contiguous page extent. Physical pages are
/// always zeroed when first acquired, and the requested bytes are zeroed when the
/// caller requests zero-initialized semantics.
pub(super) fn allocate_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 {
    allocate_tagged_mode(size, tag, zeroed, false)
}

pub(super) fn allocate_paged_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 {
    allocate_tagged_mode(size, tag, zeroed, true)
}

fn allocate_tagged_mode(size: usize, tag: u32, zeroed: bool, pageable: bool) -> *mut u8 {
    if size == 0 { return core::ptr::null_mut(); }
    let _guard = Guard::acquire();
    let table = unsafe { &mut *ALLOCATIONS.0.get() };
    let Some(allocation_index) = table.iter().position(|a| !a.live) else { return core::ptr::null_mut(); };

    if let Some(class_size) = small_class(size) {
        let slabs = unsafe { &mut *SLABS.0.get() };
        let mut slab_index = slabs.iter().position(|s| s.live && s.block_size == class_size && s.pageable == pageable && s.find_free_slot().is_some());
        if slab_index.is_none() {
            let Some(empty_index) = slabs.iter().position(|s| !s.live) else { return core::ptr::null_mut(); };
            let page = match unsafe { (if pageable { crate::kernel_vm::allocate_pageable_pages(1) } else { crate::kernel_vm::allocate_pages(1) }).map(|p| p as u64) } {
                Ok(base) => base,
                Err(_) => return core::ptr::null_mut(),
            };
            slabs[empty_index] = Slab {
                base: page, block_size: class_size, slots: PAGE_SIZE / class_size, pageable,
                used: [0; BITMAP_WORDS], live: true,
            };
            slab_index = Some(empty_index);
        }
        let slab_index = slab_index.unwrap();
        let slab = &mut slabs[slab_index];
        let Some(slot) = slab.find_free_slot() else { return core::ptr::null_mut(); };
        slab.mark_used(slot);
        let base = slab.base + (slot * class_size) as u64;
        table[allocation_index] = Allocation {
            base, backing_base: slab.base, pages: 1, requested: size, tag,
            slab_index: slab_index as u16, slab_slot: slot as u16, live: true,
        };
        if zeroed { unsafe { core::ptr::write_bytes(base as *mut u8, 0, size); } }
        return base as *mut u8;
    }

    let Some(pages) = size.checked_add(PAGE_SIZE - 1).map(|n| n / PAGE_SIZE) else {
        return core::ptr::null_mut();
    };
    if pages == 0 || pages > MAX_PAGES_PER_ALLOCATION { return core::ptr::null_mut(); }
    let base = match unsafe { (if pageable { crate::kernel_vm::allocate_pageable_pages(pages) } else { crate::kernel_vm::allocate_pages(pages) }).map(|p| p as u64) } {
        Ok(base) => base,
        Err(_) => return core::ptr::null_mut(),
    };
    table[allocation_index] = Allocation {
        base, backing_base: base, pages, requested: size, tag,
        slab_index: NO_SLAB, slab_slot: 0, live: true,
    };
    if zeroed { unsafe { core::ptr::write_bytes(base as *mut u8, 0, size); } }
    base as *mut u8
}

pub(super) fn allocate(size: usize) -> *mut u8 { allocate_tagged(size, 0, false) }

pub(super) fn allocation_info(ptr: *const u8) -> Option<(usize, u32)> {
    if ptr.is_null() { return None; }
    let _guard = Guard::acquire();
    let table = unsafe { &*ALLOCATIONS.0.get() };
    table.iter().find(|a| a.live && a.base == ptr as u64).map(|a| (a.requested, a.tag))
}

pub(super) fn deallocate(ptr: *mut u8) -> bool { deallocate_tagged(ptr, None) }

pub(super) fn deallocate_tagged(ptr: *mut u8, expected_tag: Option<u32>) -> bool {
    if ptr.is_null() { return true; }
    let _guard = Guard::acquire();
    let table = unsafe { &mut *ALLOCATIONS.0.get() };
    let Some(index) = table.iter().position(|a| a.live && a.base == ptr as u64) else { return false; };
    let allocation = table[index];
    if expected_tag.is_some_and(|tag| allocation.tag != tag) { return false; }

    if allocation.slab_index != NO_SLAB {
        let slabs = unsafe { &mut *SLABS.0.get() };
        let slab_index = allocation.slab_index as usize;
        let slot = allocation.slab_slot as usize;
        let Some(slab) = slabs.get(slab_index).filter(|s| s.live && s.base == allocation.backing_base) else { return false; };
        if slab.is_empty_except(slot) {
            // Do not discard the ownership record unless the backing page is
            // actually returned; a full physical free list can be retried later.
            if unsafe { crate::kernel_vm::free_pages(allocation.backing_base as *mut u8, 1) }.is_err() {
                return false;
            }
            slabs[slab_index] = Slab::EMPTY;
        } else {
            slabs[slab_index].mark_free(slot);
        }
    } else if unsafe { crate::kernel_vm::free_pages(allocation.backing_base as *mut u8, allocation.pages) }.is_err() {
        // Keep the record live if the physical allocator cannot accept the run.
        return false;
    }
    table[index] = Allocation::EMPTY;
    true
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_classes_are_aligned_and_bounded() {
        assert_eq!(small_class(0), None);
        assert_eq!(small_class(1), Some(16));
        assert_eq!(small_class(16), Some(16));
        assert_eq!(small_class(17), Some(32));
        assert_eq!(small_class(1025), Some(2048));
        assert_eq!(small_class(2048), Some(2048));
        assert_eq!(small_class(2049), None);
    }

    #[test]
    fn slab_bitmap_reuses_slots_and_detects_last_live_block() {
        let mut slab = Slab {
            base: 0x200000, block_size: 16, slots: PAGE_SIZE / 16, pageable: false,
            used: [0; BITMAP_WORDS], live: true,
        };
        assert_eq!(slab.find_free_slot(), Some(0));
        slab.mark_used(0);
        assert_eq!(slab.find_free_slot(), Some(1));
        assert!(slab.is_empty_except(0));
        slab.mark_used(1);
        assert!(!slab.is_empty_except(0));
        slab.mark_free(0);
        assert_eq!(slab.find_free_slot(), Some(0));
        assert!(slab.is_empty_except(1));
        slab.mark_free(1);
        assert!(slab.used.iter().all(|word| *word == 0));
    }
}

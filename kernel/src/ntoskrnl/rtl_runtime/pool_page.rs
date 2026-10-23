//! Page-backed NT nonpaged pool backend.
//!
//! Allocations are page-aligned physical extents from the kernel page allocator.
//! A locked ownership table validates exact pointers before any metadata is read,
//! so foreign/interior/double frees never cause an arbitrary header dereference.
//! The current bootstrap maps physical memory identity-wise only below 4 GiB.
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

const PAGE_SIZE: usize = 4096;
const MAX_ALLOCATIONS: usize = 4096;
const MAX_PAGES_PER_ALLOCATION: usize = 65536;

#[derive(Clone, Copy)]
struct Allocation {
    base: u64,
    pages: usize,
    requested: usize,
    tag: u32,
    live: bool,
}
impl Allocation {
    const EMPTY: Self = Self { base: 0, pages: 0, requested: 0, tag: 0, live: false };
}

struct AllocationTable(UnsafeCell<[Allocation; MAX_ALLOCATIONS]>);
unsafe impl Sync for AllocationTable {}
static ALLOCATIONS: AllocationTable = AllocationTable(UnsafeCell::new([Allocation::EMPTY; MAX_ALLOCATIONS]));

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

/// Allocate page-backed nonpaged pool storage. Physical allocation is serialized
/// independently; lock ordering is always pool-table lock -> physical-page lock.
pub(super) fn allocate_tagged(size: usize, tag: u32, _zeroed: bool) -> *mut u8 {
    if size == 0 { return core::ptr::null_mut(); }
    let Some(pages) = size.checked_add(PAGE_SIZE - 1).map(|n| n / PAGE_SIZE) else {
        return core::ptr::null_mut();
    };
    if pages == 0 || pages > MAX_PAGES_PER_ALLOCATION { return core::ptr::null_mut(); }

    let _guard = Guard::acquire();
    let table = unsafe { &mut *ALLOCATIONS.0.get() };
    let Some(slot) = table.iter_mut().find(|a| !a.live) else { return core::ptr::null_mut(); };
    let base = match unsafe { crate::memory::allocate_contiguous_pages(pages) } {
        Ok(base) => base,
        Err(_) => return core::ptr::null_mut(),
    };
    *slot = Allocation { base, pages, requested: size, tag, live: true };
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
    let Some(slot) = table.iter_mut().find(|a| a.live && a.base == ptr as u64) else { return false; };
    if expected_tag.is_some_and(|tag| slot.tag != tag) { return false; }

    // Return the entire run atomically. If the physical free stack is full,
    // retain the ownership record so the caller can retry rather than leak pages.
    if unsafe { crate::memory::free_contiguous_pages(slot.base, slot.pages) }.is_err() {
        return false;
    }
    *slot = Allocation::EMPTY;
    true
}

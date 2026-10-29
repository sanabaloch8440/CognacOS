#[cfg(not(target_os = "none"))]
mod bounded {
//! Small, bounded non-paged RTL string pool.
//! Independent of Rust's global allocator; blocks and returned payloads are 16-byte aligned,
//! locked, split on allocation, and coalesced on free. Intended for RTL-owned string buffers.
#[cfg(not(target_os = "none"))]
use core::sync::atomic::{AtomicBool, Ordering};
const ARENA_SIZE: usize = 256 * 1024;
const ALIGN: usize = 16;
const NONE: usize = usize::MAX;
const HEADER_SIZE: usize = 48;

#[repr(C, align(16))]
struct Block { size: usize, next: usize, free: usize, tag: u32, _padding: u32, requested: usize }
#[repr(C, align(16))]
struct Arena([u8; ARENA_SIZE]);

// On the real kernel target, disable local interrupts while holding the pool lock.
// Otherwise an interrupt that calls an RTL allocation/free routine could re-enter
// this allocator on the same CPU and spin forever. Host-side tests use the atomic
// fallback because privileged CLI/STI instructions cannot run in user mode.
#[cfg(target_os = "none")]
static IRQ_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
#[cfg(not(target_os = "none"))]
static LOCK: AtomicBool = AtomicBool::new(false);

// Align the arena itself, not just its headers: payload alignment depends on the base address.
static mut ARENA: Arena = Arena([0; ARENA_SIZE]);
static mut INITIALIZED: bool = false;

#[cfg(target_os = "none")]
struct Guard(crate::hal::IrqSpinLockGuard<'static, ()>);
#[cfg(target_os = "none")]
impl Guard {
    fn acquire() -> Self { Self(IRQ_LOCK.lock()) }
}

#[cfg(not(target_os = "none"))]
struct Guard;
#[cfg(not(target_os = "none"))]
impl Guard {
    fn acquire() -> Self {
        while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
#[cfg(not(target_os = "none"))]
impl Drop for Guard { fn drop(&mut self) { LOCK.store(false, Ordering::Release); } }

#[inline]
fn base() -> *mut u8 { core::ptr::addr_of_mut!(ARENA).cast::<u8>() }

unsafe fn init_locked() {
    if unsafe { INITIALIZED } { return; }
    let h = base().cast::<Block>();
    unsafe {
        h.write(Block { size: ARENA_SIZE - HEADER_SIZE, next: NONE, free: 1, tag: 0, _padding: 0, requested: 0 });
        INITIALIZED = true;
    }
}

/// Allocate an uninitialized buffer from the bounded RTL pool.
/// Returns null for zero-sized requests, overflow, or exhaustion.
/// Allocate from the bounded nonpaged arena while recording the caller's tag and
/// requested byte count. The arena is currently fixed-size; this metadata is the
/// compatibility foundation for tagged ExAllocatePool/ExFreePool services.
pub(super) fn allocate_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 {
    if size == 0 || size > ARENA_SIZE - HEADER_SIZE { return core::ptr::null_mut(); }
    let Some(wanted) = size.checked_add(ALIGN - 1).map(|n| n & !(ALIGN - 1)) else {
        return core::ptr::null_mut();
    };
    let _guard = Guard::acquire();
    unsafe { init_locked(); }
    let mut off = 0usize;
    while off < ARENA_SIZE {
        let h = unsafe { &mut *base().add(off).cast::<Block>() };
        if h.free != 0 && h.size >= wanted {
            let remain = h.size - wanted;
            if remain >= HEADER_SIZE + ALIGN {
                let next_off = off + HEADER_SIZE + wanted;
                let next = unsafe { &mut *base().add(next_off).cast::<Block>() };
                *next = Block { size: remain - HEADER_SIZE, next: h.next, free: 1, tag: 0, _padding: 0, requested: 0 };
                h.next = next_off;
                h.size = wanted;
            }
            h.free = 0;
            h.tag = tag;
            h.requested = size;
            let payload = unsafe { base().add(off + HEADER_SIZE) };
            debug_assert_eq!((payload as usize) % ALIGN, 0);
            if zeroed { unsafe { core::ptr::write_bytes(payload, 0, size); } }
            return payload;
        }
        if h.next == NONE { break; }
        off = h.next;
    }
    core::ptr::null_mut()
}

/// Legacy RTL string allocator entry point; strings do not use a pool tag.
pub(super) fn allocate(size: usize) -> *mut u8 { allocate_tagged(size, 0, false) }

/// Release only live pointers owned by this pool; foreign and duplicate frees are rejected.
pub(super) fn deallocate(ptr: *mut u8) -> bool { deallocate_tagged(ptr, None) }

/// Return requested byte count and tag for an exact live allocation pointer.
pub(super) fn allocation_info(ptr: *const u8) -> Option<(usize, u32)> {
    if ptr.is_null() { return None; }
    let start = base() as usize;
    let p = ptr as usize;
    if p < start + HEADER_SIZE || p >= start + ARENA_SIZE || (p - start - HEADER_SIZE) % ALIGN != 0 {
        return None;
    }
    let _guard = Guard::acquire();
    unsafe { init_locked(); }
    let target = p - start - HEADER_SIZE;
    let mut off = 0usize;
    while off < ARENA_SIZE {
        let h = unsafe { &*base().add(off).cast::<Block>() };
        if off == target {
            return if h.free == 0 { Some((h.requested, h.tag)) } else { None };
        }
        if h.next == NONE { break; }
        off = h.next;
    }
    None
}

/// Free an owned block, optionally requiring its allocation tag to match.
pub(super) fn deallocate_tagged(ptr: *mut u8, expected_tag: Option<u32>) -> bool {
    if ptr.is_null() { return true; }
    let start = base() as usize;
    let p = ptr as usize;
    if p < start + HEADER_SIZE || p >= start + ARENA_SIZE || (p - start - HEADER_SIZE) % ALIGN != 0 {
        return false;
    }
    let _guard = Guard::acquire();
    unsafe { init_locked(); }
    let target = p - start - HEADER_SIZE;
    let mut off = 0usize;
    while off < ARENA_SIZE {
        let h = unsafe { &mut *base().add(off).cast::<Block>() };
        if off == target {
            if h.free != 0 || expected_tag.is_some_and(|tag| h.tag != tag) { return false; }
            h.free = 1;
            h.tag = 0;
            h.requested = 0;
            // Coalesce adjacent free nodes; the list is kept in address order.
            let mut cur = 0usize;
            while cur < ARENA_SIZE {
                let c = unsafe { &mut *base().add(cur).cast::<Block>() };
                if c.next == NONE { break; }
                let n_off = c.next;
                let n = unsafe { &mut *base().add(n_off).cast::<Block>() };
                if c.free != 0 && n.free != 0 && cur + HEADER_SIZE + c.size == n_off {
                    c.size = c.size.saturating_add(HEADER_SIZE).saturating_add(n.size);
                    c.next = n.next;
                } else {
                    cur = n_off;
                }
            }
            return true;
        }
        if h.next == NONE { break; }
        off = h.next;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_splits_reuses_and_returns_aligned_payloads() {
        let a = allocate(64);
        let b = allocate(128);
        assert!(!a.is_null() && !b.is_null() && a != b);
        assert_eq!((a as usize) % ALIGN, 0);
        assert_eq!((b as usize) % ALIGN, 0);
        unsafe { core::ptr::write_bytes(a, 0xA5, 64); }
        assert!(deallocate(a));
        assert!(!deallocate(a), "double free must be rejected");
        assert!(deallocate(b));
        let c = allocate(192);
        assert!(!c.is_null());
        assert_eq!((c as usize) % ALIGN, 0);
        assert!(deallocate(c));
    }

    #[test]
    fn tagged_pool_tracks_requested_size_and_rejects_wrong_tag_free() {
        let p = allocate_tagged(37, 0x52544c31, true);
        assert!(!p.is_null());
        assert_eq!(allocation_info(p), Some((37, 0x52544c31)));
        for i in 0..37 { assert_eq!(unsafe { *p.add(i) }, 0); }
        assert!(!deallocate_tagged(p, Some(0x42414421)));
        assert_eq!(allocation_info(p), Some((37, 0x52544c31)));
        assert!(deallocate_tagged(p, Some(0x52544c31)));
        assert_eq!(allocation_info(p), None);
    }

    #[test]
    fn pool_rejects_foreign_and_zero_size_requests() {
        let mut foreign = [0u8; 32];
        assert!(allocate(0).is_null());
        assert!(!deallocate(foreign.as_mut_ptr()));
        assert!(allocate(ARENA_SIZE).is_null());
    }
}

}

#[cfg(not(target_os = "none"))]
pub(super) fn try_deallocate_tagged(_ptr: *mut u8, _expected_tag: Option<u32>) -> bool { false }
#[cfg(target_os = "none")]
pub(super) fn try_deallocate_tagged(ptr: *mut u8, expected_tag: Option<u32>) -> bool { super::pool_page::try_deallocate_tagged(ptr, expected_tag) }

#[cfg(not(target_os = "none"))]
pub(super) fn allocate_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 { bounded::allocate_tagged(size, tag, zeroed) }
#[cfg(target_os = "none")]
pub(super) fn allocate_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 { super::pool_page::allocate_tagged(size, tag, zeroed) }
#[cfg(not(target_os = "none"))]
pub(super) fn try_allocate_tagged(_size: usize, _tag: u32, _zeroed: bool) -> *mut u8 { core::ptr::null_mut() }
#[cfg(target_os = "none")]
pub(super) fn try_allocate_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 { super::pool_page::try_allocate_tagged(size, tag, zeroed) }

#[cfg(not(target_os = "none"))]
pub(super) fn allocate_paged_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 { bounded::allocate_tagged(size, tag, zeroed) }
#[cfg(target_os = "none")]
pub(super) fn allocate_paged_tagged(size: usize, tag: u32, zeroed: bool) -> *mut u8 { super::pool_page::allocate_paged_tagged(size, tag, zeroed) }

#[cfg(not(target_os = "none"))]
pub(super) fn allocate(size: usize) -> *mut u8 { bounded::allocate(size) }
#[cfg(target_os = "none")]
pub(super) fn allocate(size: usize) -> *mut u8 { super::pool_page::allocate(size) }

#[cfg(not(target_os = "none"))]
pub(super) fn deallocate(ptr: *mut u8) -> bool { bounded::deallocate(ptr) }
#[cfg(target_os = "none")]
pub(super) fn deallocate(ptr: *mut u8) -> bool { super::pool_page::deallocate(ptr) }

#[cfg(not(target_os = "none"))]
pub(super) fn allocation_info(ptr: *const u8) -> Option<(usize, u32)> { bounded::allocation_info(ptr) }
#[cfg(target_os = "none")]
pub(super) fn allocation_info(ptr: *const u8) -> Option<(usize, u32)> { super::pool_page::allocation_info(ptr) }

#[cfg(not(target_os = "none"))]
pub(super) fn deallocate_tagged(ptr: *mut u8, expected_tag: Option<u32>) -> bool { bounded::deallocate_tagged(ptr, expected_tag) }
#[cfg(target_os = "none")]
pub(super) fn deallocate_tagged(ptr: *mut u8, expected_tag: Option<u32>) -> bool { super::pool_page::deallocate_tagged(ptr, expected_tag) }

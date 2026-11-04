//! Pool-backed growable storage primitives for AML runtime objects.
//!
//! These containers deliberately use the versioned executive-pool callbacks;
//! the ACPI crate has no global allocator. All sizes and growth arithmetic are
//! checked, and a failed growth leaves the old allocation intact.
use core::{marker::PhantomData, mem::{align_of, size_of}, ops::{Deref, DerefMut}, ptr, slice};
use crate::aml_storage::{AmlPoolError, AmlPoolServices};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmlHeapError {
    Pool(AmlPoolError),
    CapacityOverflow,
    InvalidAlignment,
}
impl From<AmlPoolError> for AmlHeapError {
    fn from(value: AmlPoolError) -> Self { Self::Pool(value) }
}

/// A uniquely-owned, pool-backed vector of Copy values.
///
/// The executive pool guarantees at least 16-byte alignment. This type rejects
/// element types with stronger alignment requirements. Growth allocates,
/// copies, then releases transactionally; pointers into the vector must not be
/// retained across a mutating operation.
///
/// The service provider must outlive this vector. There are intentionally no
/// Clone/Copy implementations.
pub struct AmlPoolVec<T: Copy> {
    services: AmlPoolServices,
    allocation: *mut T,
    length: usize,
    capacity: usize,
    _element: PhantomData<T>,
}
impl<T: Copy> AmlPoolVec<T> {
    pub const INITIAL_CAPACITY: usize = 8;

    pub fn new(services: AmlPoolServices) -> Result<Self, AmlHeapError> {
        services.validate()?;
        if align_of::<T>() > 16 { return Err(AmlHeapError::InvalidAlignment); }
        Ok(Self { services, allocation: ptr::null_mut(), length: 0, capacity: 0, _element: PhantomData })
    }
    pub fn len(&self) -> usize { self.length }
    pub fn capacity(&self) -> usize { self.capacity }
    pub fn is_empty(&self) -> bool { self.length == 0 }
    pub fn as_slice(&self) -> &[T] {
        if self.length == 0 { return &[]; }
        // SAFETY: allocation contains length initialized T values.
        unsafe { slice::from_raw_parts(self.allocation, self.length) }
    }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        if self.length == 0 { return &mut []; }
        // SAFETY: this vector uniquely owns its initialized elements.
        unsafe { slice::from_raw_parts_mut(self.allocation, self.length) }
    }
    pub fn get(&self, index: usize) -> Option<&T> { self.as_slice().get(index) }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> { self.as_mut_slice().get_mut(index) }

    pub fn try_reserve(&mut self, additional: usize) -> Result<(), AmlHeapError> {
        let required = self.length.checked_add(additional).ok_or(AmlHeapError::CapacityOverflow)?;
        if required <= self.capacity { return Ok(()); }
        let mut next = self.capacity.max(Self::INITIAL_CAPACITY);
        while next < required {
            next = next.checked_mul(2).ok_or(AmlHeapError::CapacityOverflow)?;
        }
        let bytes = next.checked_mul(size_of::<T>()).ok_or(AmlHeapError::CapacityOverflow)?;
        if bytes == 0 { return Err(AmlHeapError::CapacityOverflow); }
        // SAFETY: validated service table; returned storage is pool-aligned.
        let replacement = unsafe { self.services.allocate_zeroed(bytes)? }.cast::<T>();
        if !self.allocation.is_null() && self.length != 0 {
            // SAFETY: allocations are distinct and both contain length T slots.
            unsafe { ptr::copy_nonoverlapping(self.allocation, replacement, self.length); }
        }
        if !self.allocation.is_null() {
            // Do not discard the old allocation if its release is rejected.
            if let Err(error) = unsafe { self.services.release(self.allocation.cast::<u8>()) } {
                let _ = unsafe { self.services.release(replacement.cast::<u8>()) };
                return Err(AmlHeapError::Pool(error));
            }
        }
        self.allocation = replacement;
        self.capacity = next;
        Ok(())
    }

    pub fn try_push(&mut self, value: T) -> Result<usize, AmlHeapError> {
        if self.length == self.capacity { self.try_reserve(1)?; }
        let index = self.length;
        // SAFETY: capacity was reserved and this slot is not yet initialized.
        unsafe { self.allocation.add(index).write(value); }
        self.length += 1;
        Ok(index)
    }

    /// Capture the current logical length before a multi-step AML decode.
    pub fn checkpoint(&self) -> usize { self.length }

    /// Roll back values appended after a checkpoint. Capacity is retained for
    /// reuse, while the logical contents become indistinguishable from the
    /// pre-transaction state.
    pub fn rollback(&mut self, checkpoint: usize) -> Result<(), AmlHeapError> {
        if checkpoint > self.length { return Err(AmlHeapError::CapacityOverflow); }
        self.length = checkpoint;
        Ok(())
    }

    pub fn truncate(&mut self, new_length: usize) {
        if new_length < self.length { self.length = new_length; }
    }
    pub fn clear(&mut self) { self.length = 0; }
}
impl<T: Copy> Deref for AmlPoolVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] { self.as_slice() }
}
impl<T: Copy> DerefMut for AmlPoolVec<T> {
    fn deref_mut(&mut self) -> &mut [T] { self.as_mut_slice() }
}
impl<T: Copy> Drop for AmlPoolVec<T> {
    fn drop(&mut self) {
        if !self.allocation.is_null() {
            // Pool release is expected to be infallible for a valid live block.
            let _ = unsafe { self.services.release(self.allocation.cast::<u8>()) };
            self.allocation = ptr::null_mut();
            self.length = 0;
            self.capacity = 0;
        }
    }
}


/// Explicit resource accounting for one AML execution or loaded definition
/// block. Budgets are caller policy, not AML language/package-size ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmlPoolBudget {
    pub byte_limit: usize,
    pub allocation_limit: usize,
    pub bytes_live: usize,
    pub allocations_live: usize,
    pub peak_bytes: usize,
    pub peak_allocations: usize,
}
impl AmlPoolBudget {
    pub const fn new(byte_limit: usize, allocation_limit: usize) -> Self {
        Self { byte_limit, allocation_limit, bytes_live: 0, allocations_live: 0, peak_bytes: 0, peak_allocations: 0 }
    }
    pub fn charge(&mut self, bytes: usize) -> Result<(), AmlHeapError> {
        if bytes == 0 { return Err(AmlHeapError::CapacityOverflow); }
        let next_bytes = self.bytes_live.checked_add(bytes).ok_or(AmlHeapError::CapacityOverflow)?;
        let next_allocations = self.allocations_live.checked_add(1).ok_or(AmlHeapError::CapacityOverflow)?;
        if next_bytes > self.byte_limit || next_allocations > self.allocation_limit {
            return Err(AmlHeapError::Pool(AmlPoolError::InsufficientResources));
        }
        self.bytes_live = next_bytes;
        self.allocations_live = next_allocations;
        self.peak_bytes = self.peak_bytes.max(next_bytes);
        self.peak_allocations = self.peak_allocations.max(next_allocations);
        Ok(())
    }
    pub fn release(&mut self, bytes: usize) -> Result<(), AmlHeapError> {
        if bytes == 0 || bytes > self.bytes_live || self.allocations_live == 0 {
            return Err(AmlHeapError::CapacityOverflow);
        }
        self.bytes_live -= bytes;
        self.allocations_live -= 1;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.bytes_live = 0;
        self.allocations_live = 0;
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use core::{ffi::c_void, sync::atomic::{AtomicUsize, Ordering}};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static mut HEAP: [u128; 8192] = [0; 8192];

    unsafe extern "win64" fn test_allocate(_: *mut c_void, size: usize, tag: u32, zeroed: u8) -> *mut c_void {
        if tag != crate::aml_storage::AML_POOL_TAG || size == 0 { return ptr::null_mut(); }
        let aligned = match size.checked_add(15) { Some(v) => v & !15, None => return ptr::null_mut() };
        let start = match NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| old.checked_add(aligned).filter(|end| *end <= 65536)) {
            Ok(v) => v, Err(_) => return ptr::null_mut(),
        };
        let base = ptr::addr_of_mut!(HEAP).cast::<u8>();
        let allocation = unsafe { base.add(start) };
        if zeroed != 0 { unsafe { ptr::write_bytes(allocation, 0, size); } }
        allocation.cast()
    }
    unsafe extern "win64" fn test_release(_: *mut c_void, allocation: *mut c_void, tag: u32) -> u8 {
        if allocation.is_null() || tag != crate::aml_storage::AML_POOL_TAG { 0 } else { 1 }
    }
    fn services() -> AmlPoolServices {
        NEXT.store(0, Ordering::Relaxed);
        AmlPoolServices::new(ptr::null_mut(), Some(test_allocate), Some(test_release))
    }

    #[test]
    fn vector_grows_past_old_arena_value_ceiling() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut values = AmlPoolVec::<u64>::new(services()).unwrap();
        for value in 0..1000u64 { values.try_push(value).unwrap(); }
        assert_eq!(values.len(), 1000);
        assert!(values.capacity() >= 1000);
        assert_eq!(values[0], 0);
        assert_eq!(values[767], 767);
        assert_eq!(values[999], 999);
    }

    #[test]
    fn vector_checkpoint_rolls_back_nested_decode_appends() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut values = AmlPoolVec::<u16>::new(services()).unwrap();
        values.try_push(10).unwrap();
        let checkpoint = values.checkpoint();
        values.try_push(20).unwrap();
        values.try_push(30).unwrap();
        values.rollback(checkpoint).unwrap();
        assert_eq!(values.as_slice(), &[10]);
        assert_eq!(values.rollback(2), Err(AmlHeapError::CapacityOverflow));
    }

    #[test]
    fn budget_tracks_live_and_peak_usage_without_language_limits() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut budget = AmlPoolBudget::new(1 << 20, 4096);
        for _ in 0..1000 { budget.charge(64).unwrap(); }
        assert_eq!(budget.allocations_live, 1000);
        assert_eq!(budget.bytes_live, 64000);
        assert_eq!(budget.peak_allocations, 1000);
        budget.release(64).unwrap();
        assert_eq!(budget.allocations_live, 999);
        assert_eq!(budget.bytes_live, 63936);
    }

    #[test]
    fn budget_exhaustion_is_checked_and_does_not_mutate_accounting() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut budget = AmlPoolBudget::new(128, 2);
        budget.charge(64).unwrap();
        budget.charge(64).unwrap();
        assert!(budget.charge(1).is_err());
        assert_eq!(budget.bytes_live, 128);
        assert_eq!(budget.allocations_live, 2);
    }
}


/// Variable-length AML String/Buffer payload storage. The byte vector is
/// independently owned and can grow beyond the former inline 256-byte field.
pub struct AmlPoolBytes {
    bytes: AmlPoolVec<u8>,
}
impl AmlPoolBytes {
    pub fn new(services: AmlPoolServices) -> Result<Self, AmlHeapError> {
        Ok(Self { bytes: AmlPoolVec::new(services)? })
    }
    pub fn len(&self) -> usize { self.bytes.len() }
    pub fn is_empty(&self) -> bool { self.bytes.is_empty() }
    pub fn as_bytes(&self) -> &[u8] { self.bytes.as_slice() }
    pub fn as_mut_bytes(&mut self) -> &mut [u8] { self.bytes.as_mut_slice() }
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), AmlHeapError> {
        self.bytes.try_reserve(additional)
    }
    pub fn try_push(&mut self, byte: u8) -> Result<usize, AmlHeapError> {
        self.bytes.try_push(byte)
    }
    pub fn try_extend_from_slice(&mut self, source: &[u8]) -> Result<(), AmlHeapError> {
        self.bytes.try_reserve(source.len())?;
        for byte in source { self.bytes.try_push(*byte)?; }
        Ok(())
    }
    pub fn checkpoint(&self) -> usize { self.bytes.checkpoint() }
    pub fn rollback(&mut self, checkpoint: usize) -> Result<(), AmlHeapError> {
        self.bytes.rollback(checkpoint)
    }
}

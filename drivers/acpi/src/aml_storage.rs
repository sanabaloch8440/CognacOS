//! Versioned kernel-pool contract for AML-owned variable-sized storage.
//!
//! ACPI is built as a no_std driver crate and must not assume a Rust global
//! allocator. The kernel supplies these callbacks from its executive pool
//! implementation. Allocations are tagged, nonpaged, and released through the
//! same provider. This contract is an internal kernel/ACPI boundary, not a PE export.
use core::ffi::c_void;

pub const AML_POOL_SERVICES_VERSION: u32 = 1;
pub const AML_POOL_TAG: u32 = u32::from_le_bytes(*b"AcPm");

pub type AmlPoolAllocate = Option<
    unsafe extern "win64" fn(context: *mut c_void, size: usize, tag: u32, zeroed: u8) -> *mut c_void,
>;
pub type AmlPoolRelease = Option<
    unsafe extern "win64" fn(context: *mut c_void, allocation: *mut c_void, tag: u32) -> u8,
>;

/// Kernel-provided nonpaged pool operations used by AML runtime storage.
/// The provider must keep callbacks valid until all allocations are released.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AmlPoolServices {
    pub size: u32,
    pub version: u32,
    pub context: *mut c_void,
    pub allocate: AmlPoolAllocate,
    pub release: AmlPoolRelease,
}

unsafe impl Send for AmlPoolServices {}
unsafe impl Sync for AmlPoolServices {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmlPoolError {
    InvalidServices,
    ZeroSize,
    InsufficientResources,
    ReleaseRejected,
}

impl AmlPoolServices {
    pub const fn new(context: *mut c_void, allocate: AmlPoolAllocate, release: AmlPoolRelease) -> Self {
        Self { size: core::mem::size_of::<Self>() as u32, version: AML_POOL_SERVICES_VERSION, context, allocate, release }
    }

    pub fn validate(&self) -> Result<(), AmlPoolError> {
        if self.size < core::mem::size_of::<Self>() as u32
            || self.version != AML_POOL_SERVICES_VERSION
            || self.allocate.is_none()
            || self.release.is_none()
        { return Err(AmlPoolError::InvalidServices); }
        Ok(())
    }

    /// Allocate zeroed nonpaged storage using the stable AML pool tag.
    /// The caller must release the returned pointer exactly once.
    pub unsafe fn allocate_zeroed(&self, size: usize) -> Result<*mut u8, AmlPoolError> {
        self.validate()?;
        if size == 0 { return Err(AmlPoolError::ZeroSize); }
        let allocate = self.allocate.ok_or(AmlPoolError::InvalidServices)?;
        let ptr = unsafe { allocate(self.context, size, AML_POOL_TAG, 1) };
        if ptr.is_null() { return Err(AmlPoolError::InsufficientResources); }
        Ok(ptr.cast::<u8>())
    }

    /// Release a live allocation returned by this exact provider.
    /// # Safety
    /// The pointer must not be accessed after a successful release.
    pub unsafe fn release(&self, allocation: *mut u8) -> Result<(), AmlPoolError> {
        self.validate()?;
        if allocation.is_null() { return Ok(()); }
        let release = self.release.ok_or(AmlPoolError::InvalidServices)?;
        if unsafe { release(self.context, allocation.cast::<c_void>(), AML_POOL_TAG) } == 0 {
            return Err(AmlPoolError::ReleaseRejected);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
    static RELEASES: AtomicUsize = AtomicUsize::new(0);
    static mut TEST_BLOCK: [u8; 128] = [0; 128];

    unsafe extern "win64" fn allocate(_context: *mut c_void, size: usize, tag: u32, zeroed: u8) -> *mut c_void {
        if tag != AML_POOL_TAG || size > 128 { return core::ptr::null_mut(); }
        let ptr = core::ptr::addr_of_mut!(TEST_BLOCK).cast::<u8>();
        if zeroed != 0 { unsafe { core::ptr::write_bytes(ptr, 0, size) }; }
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ptr.cast()
    }

    unsafe extern "win64" fn release(_context: *mut c_void, allocation: *mut c_void, tag: u32) -> u8 {
        if tag != AML_POOL_TAG || allocation != core::ptr::addr_of_mut!(TEST_BLOCK).cast::<u8>().cast() { return 0; }
        RELEASES.fetch_add(1, Ordering::Relaxed);
        1
    }

    #[test]
    fn validates_version_and_required_callbacks() {
        let mut services = AmlPoolServices::new(core::ptr::null_mut(), Some(allocate), Some(release));
        assert_eq!(services.validate(), Ok(()));
        services.version += 1;
        assert_eq!(services.validate(), Err(AmlPoolError::InvalidServices));
    }

    #[test]
    fn allocates_zeroed_tagged_storage_and_releases_it() {
        ALLOCATIONS.store(0, Ordering::Relaxed);
        RELEASES.store(0, Ordering::Relaxed);
        let services = AmlPoolServices::new(core::ptr::null_mut(), Some(allocate), Some(release));
        let block = unsafe { services.allocate_zeroed(64) }.unwrap();
        for index in 0..64 { assert_eq!(unsafe { *block.add(index) }, 0); }
        unsafe { services.release(block) }.unwrap();
        assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 1);
        assert_eq!(RELEASES.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn rejects_zero_size_and_propagates_exhaustion() {
        let services = AmlPoolServices::new(core::ptr::null_mut(), Some(allocate), Some(release));
        assert_eq!(unsafe { services.allocate_zeroed(0) }, Err(AmlPoolError::ZeroSize));
        assert_eq!(unsafe { services.allocate_zeroed(129) }, Err(AmlPoolError::InsufficientResources));
    }
}

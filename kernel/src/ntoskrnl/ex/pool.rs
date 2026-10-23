//! Implemented NT executive pool entry points.
//!
//! These functions currently implement the nonpaged pool subset. Paged pool,
//! quota charging, executable-pool policy, and Windows special-pool flags are
//! rejected until the corresponding VM and executive facilities exist.

#[inline]
pub(crate) extern "system" fn ex_allocate_pool_with_tag(pool_type: u32, size: usize, tag: u32) -> *mut u8 {
    // POOL_TYPE values: NonPagedPool=0, NonPagedPoolNx=512.
    if pool_type != 0 && pool_type != 512 { return core::ptr::null_mut(); }
    crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(size, tag, false)
}

/// ExAllocatePool2 subset: nonpaged allocations only, zeroed unless explicitly opted out.
pub(crate) extern "system" fn ex_allocate_pool2(flags: u64, size: usize, tag: u32) -> *mut u8 {
    const POOL_FLAG_NON_PAGED: u64 = 0x40;
    const POOL_FLAG_ZERO_ALLOCATION: u64 = 0x1000;
    const POOL_FLAG_UNINITIALIZED: u64 = 0x1_0000;
    const SUPPORTED: u64 = POOL_FLAG_NON_PAGED | POOL_FLAG_ZERO_ALLOCATION | POOL_FLAG_UNINITIALIZED;
    if flags & !SUPPORTED != 0 { return core::ptr::null_mut(); }
    let zeroed = flags & POOL_FLAG_UNINITIALIZED == 0 || flags & POOL_FLAG_ZERO_ALLOCATION != 0;
    crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(size, tag, zeroed)
}

/// Release a block only when its original pool tag matches.
pub(crate) extern "system" fn ex_free_pool_with_tag(ptr: *mut u8, tag: u32) {
    if ptr.is_null() { return; }
    let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(ptr, Some(tag));
}

/// Free a live allocation without requiring its tag.
pub(crate) extern "system" fn ex_free_pool(ptr: *mut u8) {
    if ptr.is_null() { return; }
    let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(ptr, None);
}

/// Return requested size for an exact live allocation; unknown pointers return zero.
pub(crate) extern "system" fn ex_query_pool_block_size(ptr: *const u8) -> usize {
    crate::ntoskrnl::rtl_runtime::pool_allocation_info(ptr).map(|(size, _)| size).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_api_checks_tag_ownership_and_zeroes_by_default() {
        assert!(ex_allocate_pool_with_tag(1, 64, 0x54455354).is_null());
        assert!(ex_allocate_pool2(0x100, 64, 0x54455354).is_null());

        let p = ex_allocate_pool2(0x40, 32, 0x5a45524f);
        assert!(!p.is_null());
        for i in 0..32 { assert_eq!(unsafe { *p.add(i) }, 0); }
        assert_eq!(ex_query_pool_block_size(p), 32);

        ex_free_pool_with_tag(p, 0x42414421);
        assert_eq!(ex_query_pool_block_size(p), 32);
        ex_free_pool_with_tag(p, 0x5a45524f);
        assert_eq!(ex_query_pool_block_size(p), 0);
    }

    #[test]
    fn pool_api_handles_null_and_zero_size() {
        assert!(ex_allocate_pool2(0x40, 0, 0x54455354).is_null());
        ex_free_pool(core::ptr::null_mut());
        ex_free_pool_with_tag(core::ptr::null_mut(), 0);
    }
}

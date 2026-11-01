//! NT executive pool entry points with a deliberately explicit supported subset.
//!
//! Pageable allocations now reserve virtual extents and fault pages in on demand.
//! This remains a Windows-compatibility subset: eviction/pagefile backing, session
//! pool, quotas, extended parameters, special-pool policy and failure exceptions
//! still require the storage and memory-pressure subsystems.
//! Unsupported modes fail closed rather than silently returning the wrong pool.
//!
//! Windows drivers use the Microsoft x64 ABI regardless of the kernel's own
//! target triple, so exported routines use `extern "win64"` explicitly.

#[inline]
pub(crate) extern "win64" fn ex_allocate_pool_with_tag(pool_type: u32, size: usize, tag: u32) -> *mut u8 {
    // POOL_TYPE: NonPagedPool=0, PagedPool=1, NonPagedPoolNx=512.
    match pool_type {
        0 | 512 => crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(size, tag, false),
        1 => crate::ntoskrnl::rtl_runtime::pool_allocate_paged_tagged(size, tag, false),
        _ => core::ptr::null_mut(),
    }
}

/// ExAllocatePool2 supports nonpaged allocations and demand-zero pageable
/// allocations. Exactly one pool-kind flag is required; unsupported flags fail closed.
pub(crate) extern "win64" fn ex_allocate_pool2(flags: u64, size: usize, tag: u32) -> *mut u8 {
    const POOL_FLAG_PAGED: u64 = 0x1;
    const POOL_FLAG_NON_PAGED: u64 = 0x40;
    const POOL_FLAG_ZERO_ALLOCATION: u64 = 0x1000;
    const POOL_FLAG_UNINITIALIZED: u64 = 0x1_0000;
    const SUPPORTED: u64 = POOL_FLAG_PAGED | POOL_FLAG_NON_PAGED | POOL_FLAG_ZERO_ALLOCATION | POOL_FLAG_UNINITIALIZED;
    let kind = flags & (POOL_FLAG_PAGED | POOL_FLAG_NON_PAGED);
    if flags & !SUPPORTED != 0 || (kind != POOL_FLAG_PAGED && kind != POOL_FLAG_NON_PAGED) { return core::ptr::null_mut(); }
    let zeroed = flags & POOL_FLAG_UNINITIALIZED == 0 || flags & POOL_FLAG_ZERO_ALLOCATION != 0;
    if kind == POOL_FLAG_PAGED {
        crate::ntoskrnl::rtl_runtime::pool_allocate_paged_tagged(size, tag, zeroed)
    } else {
        crate::ntoskrnl::rtl_runtime::pool_allocate_tagged(size, tag, zeroed)
    }
}

/// ExAllocatePool3 accepts the same base flags but extended parameters are not
/// implemented yet. Reject non-empty parameter lists rather than ignoring policy.
pub(crate) extern "win64" fn ex_allocate_pool3(
    flags: u64,
    size: usize,
    tag: u32,
    _extended_parameters: *const core::ffi::c_void,
    extended_parameter_count: u32,
) -> *mut u8 {
    if extended_parameter_count != 0 { return core::ptr::null_mut(); }
    ex_allocate_pool2(flags, size, tag)
}

/// Priority currently does not change allocation policy; only documented basic
/// basic nonpaged and demand-zero paged types are accepted; session pool remains unsupported.
pub(crate) extern "win64" fn ex_allocate_pool_with_tag_priority(
    pool_type: u32,
    size: usize,
    tag: u32,
    priority: u32,
) -> *mut u8 {
    if priority != 0 && priority != 16 && priority != 32 { return core::ptr::null_mut(); }
    ex_allocate_pool_with_tag(pool_type, size, tag)
}

/// Release a block only when its original pool tag matches.
pub(crate) extern "win64" fn ex_free_pool_with_tag(ptr: *mut u8, tag: u32) {
    if ptr.is_null() { return; }
    let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(ptr, Some(tag));
}

/// Free a live allocation without requiring its tag.
pub(crate) extern "win64" fn ex_free_pool(ptr: *mut u8) {
    if ptr.is_null() { return; }
    let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(ptr, None);
}

/// ExFreePool2 subset. A zero tag means no tag check; extended free parameters
/// are rejected until their lifetime/priority semantics are implemented.
pub(crate) extern "win64" fn ex_free_pool2(
    ptr: *mut u8,
    tag: u32,
    _extended_parameters: *const core::ffi::c_void,
    extended_parameter_count: u32,
) {
    if ptr.is_null() || extended_parameter_count != 0 { return; }
    let expected = if tag == 0 { None } else { Some(tag) };
    let _ = crate::ntoskrnl::rtl_runtime::pool_free_tagged(ptr, expected);
}

/// Return requested size for an exact live allocation; unknown pointers return zero.
pub(crate) extern "win64" fn ex_query_pool_block_size(ptr: *const u8, quota_charged: *mut u8) -> usize {
    // This early implementation never charges process quota.
    if !quota_charged.is_null() { unsafe { quota_charged.write(0); } }
    crate::ntoskrnl::rtl_runtime::pool_allocation_info(ptr).map(|(size, _)| size).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_api_checks_tag_ownership_and_zeroes_by_default() {
        let paged = ex_allocate_pool_with_tag(1, 64, 0x50414745);
        assert!(!paged.is_null());
        assert_eq!(ex_query_pool_block_size(paged, core::ptr::null_mut()), 64);
        ex_free_pool_with_tag(paged, 0x50414745);
        assert!(ex_allocate_pool_with_tag(2, 64, 0x54455354).is_null());
        assert!(ex_allocate_pool2(0x100, 64, 0x54455354).is_null());
        assert!(ex_allocate_pool3(0x40, 64, 0x54455354, core::ptr::null(), 1).is_null());

        let paged = ex_allocate_pool2(0x1, 64, 0x50473230);
        assert!(!paged.is_null());
        for i in 0..64 { assert_eq!(unsafe { *paged.add(i) }, 0); }
        ex_free_pool2(paged, 0x50473230, core::ptr::null(), 0);
        assert!(ex_allocate_pool2(0x41, 64, 0x42414446).is_null());
        let p = ex_allocate_pool2(0x40, 32, 0x5a45524f);
        assert!(!p.is_null());
        for i in 0..32 { assert_eq!(unsafe { *p.add(i) }, 0); }
        let mut quota_charged = 0xffu8;
        assert_eq!(ex_query_pool_block_size(p, &mut quota_charged), 32);
        assert_eq!(quota_charged, 0);

        ex_free_pool_with_tag(p, 0x42414421);
        assert_eq!(ex_query_pool_block_size(p, core::ptr::null_mut()), 32);
        ex_free_pool2(p, 0x42414421, core::ptr::null(), 0);
        assert_eq!(ex_query_pool_block_size(p, core::ptr::null_mut()), 32);
        ex_free_pool2(p, 0x5a45524f, core::ptr::null(), 0);
        assert_eq!(ex_query_pool_block_size(p, core::ptr::null_mut()), 0);
    }

    #[test]
    fn pool_api_handles_null_and_zero_size() {
        assert!(ex_allocate_pool2(0x40, 0, 0x54455354).is_null());
        assert!(ex_allocate_pool3(0x40, 0, 0x54455354, core::ptr::null(), 0).is_null());
        ex_free_pool(core::ptr::null_mut());
        ex_free_pool_with_tag(core::ptr::null_mut(), 0);
        ex_free_pool2(core::ptr::null_mut(), 0, core::ptr::null(), 0);
    }

    #[test]
    fn pool3_and_priority_wrappers_preserve_tagged_ownership() {
        let p = ex_allocate_pool3(0x40, 48, 0x50334c31, core::ptr::null(), 0);
        assert!(!p.is_null());
        assert_eq!(ex_query_pool_block_size(p, core::ptr::null_mut()), 48);
        ex_free_pool2(p, 0x50334c31, core::ptr::null(), 0);

        let p = ex_allocate_pool_with_tag_priority(512, 24, 0x5052494f, 16);
        assert!(!p.is_null());
        ex_free_pool_with_tag(p, 0x5052494f);
        assert!(ex_allocate_pool_with_tag_priority(1, 24, 0, 16).is_null());
        assert!(ex_allocate_pool_with_tag_priority(512, 24, 0, 1).is_null());
    }
}

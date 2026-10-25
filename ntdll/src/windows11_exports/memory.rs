//! Windows 11 x64 NTDLL memory export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemoryEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFlushVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFreeVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtLockVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtProtectVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReadVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReadVirtualMemoryEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtUnlockVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWriteVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddRefMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenMembership() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenMembershipEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCloneMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCmDecodeMemIoResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCmEncodeMemIoResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCommitMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyMappedMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemoryNonTemporal() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemoryStreamTo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyOutOfProcessMemoryStreamTo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeregisterSecureMemoryCacheCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlExtendMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlExtendMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFillMemoryNonTemporal() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFillNonVolatileMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFinalReleaseOutOfProcessMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemoryRanges() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlushSecureMemoryCache() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitOutOfProcessMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIoDecodeMemIoResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIoEncodeMemIoResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryStreamRegion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryInterfaceMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReadMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReadOutOfProcessMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRegisterSecureMemoryCacheCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleaseMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlResetMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlResetMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRevertMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSeekMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetMemoryStreamSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlStatMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryBlockLookaside() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryStreamRegion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryZone() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWriteMemoryStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWriteNonVolatileMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAllocateVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAllocateVirtualMemoryEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFlushVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFreeVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwLockVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwProtectVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemoryEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwUnlockVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWriteVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

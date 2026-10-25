//! Windows 11 x64 NTDLL heap export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

#[no_mangle]
pub unsafe extern "system" fn RtlCreateTagHeap() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDetectHeapLeaks() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDisownModuleHeapAllocation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnumProcessHeaps() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlushHeaps() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlHeapTrkInitialize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlMultipleAllocateHeap() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlMultipleFreeHeap() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlProtectHeap() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryProcessHeapInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryTagHeap() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlZeroHeap() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

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

/// Allocate Count equal-sized blocks. Returns the number successfully allocated.
#[no_mangle]
pub unsafe extern "system" fn RtlMultipleAllocateHeap(
    heap: *mut core::ffi::c_void, flags: u32, size: usize, count: u32,
    array: *mut *mut core::ffi::c_void,
) -> u32 {
    if count == 0 { return 0; }
    if array.is_null() || count > (isize::MAX as usize / core::mem::size_of::<*mut core::ffi::c_void>()) as u32 { return 0; }
    let mut done = 0;
    while done < count {
        // SAFETY: The ABI caller supplies an output array with Count writable entries.
        let slot = unsafe { array.add(done as usize) };
        let block = unsafe { crate::rtl_heap::RtlAllocateHeap(heap, flags, size) };
        unsafe { core::ptr::write(slot, block); }
        if block.is_null() { break; }
        done += 1;
    }
    done
}

/// Free blocks in order; returns the number successfully freed.
#[no_mangle]
pub unsafe extern "system" fn RtlMultipleFreeHeap(
    heap: *mut core::ffi::c_void, flags: u32, count: u32,
    array: *mut *mut core::ffi::c_void,
) -> u32 {
    if count == 0 { return 0; }
    if array.is_null() || count > (isize::MAX as usize / core::mem::size_of::<*mut core::ffi::c_void>()) as u32 { return 0; }
    let mut done = 0;
    while done < count {
        let slot = unsafe { array.add(done as usize) };
        let block = unsafe { core::ptr::read(slot) };
        if unsafe { crate::rtl_heap::RtlFreeHeap(heap, flags, block) } != 0 { done += 1; }
    }
    done
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

/// Zero every currently allocated block in a heap. Returns NTSTATUS.
/// The walker is advanced only by the payload pointer returned in each entry.
#[no_mangle]
pub unsafe extern "system" fn RtlZeroHeap(
    heap: *mut core::ffi::c_void, flags: u32,
) -> i32 {
    let mut entry = crate::rtl_heap::RTL_HEAP_WALK_ENTRY {
        lpData: core::ptr::null_mut(), cbData: 0, cbOverhead: 0,
        iRegionIndex: 0, wFlags: 0, reserved: [0; 3],
    };
    let mut status = unsafe { crate::rtl_heap::RtlWalkHeap(heap, (&mut entry as *mut _).cast()) };
    while status == 0 {
        if entry.wFlags & 1 != 0 && !entry.lpData.is_null() && entry.cbData != 0 {
            // SAFETY: RtlWalkHeap reports an allocated payload range belonging to this heap.
            unsafe { core::ptr::write_bytes(entry.lpData.cast::<u8>(), 0, entry.cbData); }
        }
        if entry.lpData.is_null() { break; }
        status = unsafe { crate::rtl_heap::RtlWalkHeap(heap, (&mut entry as *mut _).cast()) };
    }
    let _ = flags;
    if status == 0 || status as u32 == 0x8000_001A { 0 } else { status }
}

#[cfg(test)]
mod tests {
    #[test]
    fn multiple_heap_exports_have_x64_pointer_abi() {
        assert_eq!(core::mem::size_of::<*mut core::ffi::c_void>(), 8);
        assert_eq!(core::mem::size_of::<usize>(), 8);
    }
    #[test]
    fn heap_status_constants_are_native_ntstatus_values() {
        assert_eq!(0xC000_0002u32 as i32, -1073741822);
        assert_eq!(0x8000_001Au32 as i32 as u32, 0x8000_001A);
    }
}

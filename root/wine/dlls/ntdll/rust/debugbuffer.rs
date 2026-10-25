/*
 * Rust port of Wine dlls/ntdll/debugbuffer.c.
 *
 * Copyright 2004 Raphael Junqueira.
 * Ported for CognacOS; based on Wine's LGPL-2.1-or-later implementation.
 *
 * This file intentionally preserves the current Wine implementation's scope:
 * it allocates the requested debug-information records but does not yet query
 * remote module, heap, or lock details.  The NtOpenProcess permission check is
 * retained.  Build integration and ABI validation are deferred.
 */

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use core::ffi::c_void;
use core::mem::{size_of, zeroed};
use core::ptr::{null_mut, write_bytes};

type ULONG = u32;
type USHORT = u16;
type BOOLEAN = u8;
type SIZE_T = usize;
type NTSTATUS = i32;
type HANDLE = *mut c_void;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const PROCESS_QUERY_LIMITED_INFORMATION: ULONG = 0x1000;

const PDI_MODULES: ULONG = 0x01;
const PDI_BACKTRACE: ULONG = 0x02;
const PDI_HEAPS: ULONG = 0x04;
const PDI_HEAP_TAGS: ULONG = 0x08;
const PDI_HEAP_BLOCKS: ULONG = 0x10;
const PDI_LOCKS: ULONG = 0x20;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CLIENT_ID {
    pub UniqueProcess: HANDLE,
    pub UniqueThread: HANDLE,
}

#[repr(C)]
pub struct DEBUG_BUFFER {
    pub SectionHandle: HANDLE,
    pub SectionBase: *mut c_void,
    pub RemoteSectionBase: *mut c_void,
    pub SectionBaseDelta: ULONG,
    pub EventPairHandle: HANDLE,
    pub Unknown: [SIZE_T; 2],
    pub RemoteThreadHandle: HANDLE,
    pub InfoClassMask: ULONG,
    pub SizeOfInfo: SIZE_T,
    pub AllocatedSize: SIZE_T,
    pub SectionSize: ULONG,
    pub ModuleInformation: *mut DEBUG_MODULE_INFORMATION,
    pub BackTraceInformation: *mut c_void,
    pub HeapInformation: *mut DEBUG_HEAP_INFORMATION,
    pub LockInformation: *mut DEBUG_LOCK_INFORMATION,
    pub Reserved: [*mut c_void; 8],
}

#[repr(C)]
pub struct DEBUG_MODULE_INFORMATION {
    pub Reserved: [ULONG; 2],
    pub Base: ULONG,
    pub Size: ULONG,
    pub Flags: ULONG,
    pub Index: USHORT,
    pub Unknown: USHORT,
    pub LoadCount: USHORT,
    pub ModuleNameOffset: USHORT,
    pub ImageName: [i8; 256],
}

#[repr(C)]
pub struct DEBUG_HEAP_INFORMATION {
    pub Base: ULONG,
    pub Flags: ULONG,
    pub Granularity: USHORT,
    pub Unknown: USHORT,
    pub Allocated: ULONG,
    pub Committed: ULONG,
    pub TagCount: ULONG,
    pub BlockCount: ULONG,
    pub Reserved: [ULONG; 7],
    pub Tags: *mut c_void,
    pub Blocks: *mut c_void,
}

#[repr(C)]
pub struct DEBUG_LOCK_INFORMATION {
    pub Address: *mut c_void,
    pub Type: USHORT,
    pub CreatorBackTraceIndex: USHORT,
    pub OwnerThreadId: ULONG,
    pub ActiveCount: ULONG,
    pub ContentionCount: ULONG,
    pub EntryCount: ULONG,
    pub RecursionCount: ULONG,
    pub NumberOfSharedWaiters: ULONG,
    pub NumberOfExclusiveWaiters: ULONG,
}

extern "system" {
    fn GetProcessHeap() -> HANDLE;
    fn RtlAllocateHeap(heap: HANDLE, flags: ULONG, size: SIZE_T) -> *mut c_void;
    fn RtlFreeHeap(heap: HANDLE, flags: ULONG, memory: *mut c_void) -> BOOLEAN;
    fn NtOpenProcess(
        process_handle: *mut HANDLE,
        desired_access: ULONG,
        object_attributes: *mut c_void,
        client_id: *mut CLIENT_ID,
    ) -> NTSTATUS;
    fn NtClose(handle: HANDLE) -> NTSTATUS;
}

#[inline]
unsafe fn heap_alloc_zeroed(size: usize) -> *mut c_void {
    let heap = GetProcessHeap();
    let ptr = RtlAllocateHeap(heap, 0, size);
    if !ptr.is_null() {
        write_bytes(ptr.cast::<u8>(), 0, size);
    }
    ptr
}

#[inline]
unsafe fn heap_free(ptr: *mut c_void) {
    if !ptr.is_null() {
        let _ = RtlFreeHeap(GetProcessHeap(), 0, ptr);
    }
}

/// Wine-compatible RtlCreateQueryDebugBuffer entry point.
#[no_mangle]
pub unsafe extern "system" fn RtlCreateQueryDebugBuffer(
    requested_size: ULONG,
    _event_pair: BOOLEAN,
) -> *mut DEBUG_BUFFER {
    let size = core::cmp::max(requested_size as usize, size_of::<DEBUG_BUFFER>());
    let buffer = heap_alloc_zeroed(size).cast::<DEBUG_BUFFER>();
    if buffer.is_null() {
        return null_mut();
    }

    // The Wine implementation currently leaves section/event-pair setup
    // unimplemented, so the zero-initialized fields remain null/zero.
    buffer
}

/// Wine-compatible RtlDestroyQueryDebugBuffer entry point.
#[no_mangle]
pub unsafe extern "system" fn RtlDestroyQueryDebugBuffer(
    buffer: *mut DEBUG_BUFFER,
) -> NTSTATUS {
    if buffer.is_null() {
        return STATUS_SUCCESS;
    }

    let b = &mut *buffer;
    heap_free(b.ModuleInformation.cast::<c_void>());
    b.ModuleInformation = null_mut();

    heap_free(b.HeapInformation.cast::<c_void>());
    b.HeapInformation = null_mut();

    heap_free(b.LockInformation.cast::<c_void>());
    b.LockInformation = null_mut();

    heap_free(buffer.cast::<c_void>());
    STATUS_SUCCESS
}

/// Wine-compatible RtlQueryProcessDebugInformation entry point.
///
/// As in the source C file, this is currently a structural stub: it validates
/// that the target process can be opened, sets InfoClassMask, and allocates
/// empty result records for the requested categories. Actual remote process
/// enumeration is not implemented by the upstream Wine routine yet.
#[no_mangle]
pub unsafe extern "system" fn RtlQueryProcessDebugInformation(
    process_id: ULONG,
    debug_info_mask: ULONG,
    buffer: *mut DEBUG_BUFFER,
) -> NTSTATUS {
    if buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }

    let mut client_id = CLIENT_ID {
        UniqueProcess: process_id as usize as HANDLE,
        UniqueThread: null_mut(),
    };
    let mut process: HANDLE = null_mut();

    let status = NtOpenProcess(
        &mut process,
        PROCESS_QUERY_LIMITED_INFORMATION,
        null_mut(),
        &mut client_id,
    );
    if status != STATUS_SUCCESS {
        return status;
    }
    let _ = NtClose(process);

    let b = &mut *buffer;
    b.InfoClassMask = debug_info_mask;

    if debug_info_mask & PDI_MODULES != 0 {
        let info = heap_alloc_zeroed(size_of::<DEBUG_MODULE_INFORMATION>())
            .cast::<DEBUG_MODULE_INFORMATION>();
        if info.is_null() {
            return STATUS_NO_MEMORY;
        }
        b.ModuleInformation = info;
    }

    if debug_info_mask & PDI_HEAPS != 0 {
        let info = heap_alloc_zeroed(size_of::<DEBUG_HEAP_INFORMATION>())
            .cast::<DEBUG_HEAP_INFORMATION>();
        if info.is_null() {
            return STATUS_NO_MEMORY;
        }
        // PDI_HEAP_TAGS and PDI_HEAP_BLOCKS are represented in the mask, but
        // the Wine C source leaves their population empty.
        let _tags_requested = debug_info_mask & PDI_HEAP_TAGS != 0;
        let _blocks_requested = debug_info_mask & PDI_HEAP_BLOCKS != 0;
        b.HeapInformation = info;
    }

    if debug_info_mask & PDI_LOCKS != 0 {
        let info = heap_alloc_zeroed(size_of::<DEBUG_LOCK_INFORMATION>())
            .cast::<DEBUG_LOCK_INFORMATION>();
        if info.is_null() {
            return STATUS_NO_MEMORY;
        }
        b.LockInformation = info;
    }

    // PDI_BACKTRACE has no backing record in the upstream stub.
    let _backtrace_requested = debug_info_mask & PDI_BACKTRACE != 0;
    STATUS_SUCCESS
}

// Keep a compile-time type reference to the C-compatible zero-initialization
// path; this is also useful when the module is integrated into the Rust build.
const _: unsafe fn() -> DEBUG_BUFFER = || unsafe { zeroed() };

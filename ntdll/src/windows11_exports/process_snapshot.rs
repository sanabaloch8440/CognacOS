//! Native Process Snapshotting (PSS) NTDLL exports.
//!
//! ABI declarations follow the System Informer/phnt definitions in
//! phnt/include/ntpsapi.h. The snapshot object manager is not implemented yet;
//! unsupported operations therefore return STATUS_NOT_IMPLEMENTED rather than
//! pretending that a snapshot was created or freed.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;

type HANDLE = *mut c_void;
type PVOID = *mut c_void;
type ULONG = u32;
type NTSTATUS = i32;

const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;

/// Native x64 PSS walk state; distinct from the Win32 HPSSWALK handle.
#[repr(C)]
pub struct PSSNT_WALK_MARKER {
    pub MappedBase: PVOID,
    pub Position: usize,
}

#[no_mangle]
pub unsafe extern "system" fn PssNtCaptureSnapshot(
    snapshot_handle: *mut HANDLE,
    process_handle: HANDLE,
    capture_flags: ULONG,
    thread_context_flags: ULONG,
) -> NTSTATUS {
    let _ = (snapshot_handle, process_handle, capture_flags, thread_context_flags);
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtDuplicateSnapshot(
    source_process_handle: HANDLE,
    snapshot_handle: HANDLE,
    target_process_handle: HANDLE,
    target_snapshot_handle: *mut HANDLE,
    flags: ULONG,
) -> NTSTATUS {
    let _ = (source_process_handle, snapshot_handle, target_process_handle, target_snapshot_handle, flags);
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtFreeRemoteSnapshot(
    process_handle: HANDLE,
    snapshot_handle: HANDLE,
) -> NTSTATUS {
    let _ = (process_handle, snapshot_handle);
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtFreeSnapshot(snapshot_handle: HANDLE) -> NTSTATUS {
    let _ = snapshot_handle;
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtFreeWalkMarker(marker: *mut PSSNT_WALK_MARKER) {
    // Without the snapshot section-view manager we cannot safely unmap MappedBase.
    // Preserve the caller-owned record and avoid corrupting its state.
    let _ = marker;
}

#[no_mangle]
pub unsafe extern "system" fn PssNtQuerySnapshot(
    snapshot_handle: HANDLE,
    information_class: ULONG,
    buffer: PVOID,
    buffer_length: ULONG,
) -> NTSTATUS {
    let _ = (snapshot_handle, information_class, buffer, buffer_length);
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtValidateDescriptor(
    snapshot_handle: HANDLE,
    exception_address: PVOID,
) -> NTSTATUS {
    let _ = (snapshot_handle, exception_address);
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtWalkSnapshot(
    snapshot_handle: HANDLE,
    information_class: ULONG,
    walk_marker: *mut PSSNT_WALK_MARKER,
    buffer: PVOID,
    buffer_length: ULONG,
) -> NTSTATUS {
    let _ = (snapshot_handle, information_class, walk_marker, buffer, buffer_length);
    STATUS_NOT_IMPLEMENTED
}

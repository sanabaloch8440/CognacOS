//! In-memory and section-backed IStream-compatible helpers.
#![allow(non_snake_case, clippy::missing_safety_doc)]
use core::{ffi::c_void, ptr};
use super::virtual_memory::{self, HANDLE, PVOID, SIZE_T, ULONG, NTSTATUS};
use super::virtual_memory::{cognacos_rtl_create_section, cognacos_rtl_dereference_section, cognacos_rtl_reference_section, cognacos_rtl_section_copy, cognacos_nt_read_virtual_memory};
use super::zones::{native_vm_allocate, native_vm_release};
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;

// In-memory and section-backed IStream-compatible layouts. Section streams use
// CognacOS's private section-object bridge; they are bounded and pagefile-backed
// until file-backed sections and section-view mapping are implemented.
// Start/Current/End are caller-owned bounds; these APIs never free the backing
// buffer. Only clones allocated internally by this implementation own metadata.
pub(super) const S_OK: NTSTATUS = 0;
const S_FALSE: NTSTATUS = 1;
const STG_E_MEDIUMFULL: NTSTATUS = 0x8003_0070u32 as i32;
const E_NOTIMPL: NTSTATUS = 0x8000_4001u32 as i32;
const E_NOINTERFACE: NTSTATUS = 0x8000_4002u32 as i32;
const E_INVALIDARG: NTSTATUS = 0x8007_0057u32 as i32;
const STG_E_INVALIDPOINTER: NTSTATUS = 0x8003_0009u32 as i32;
pub(super) const STREAM_SEEK_SET: ULONG = 0;
const STREAM_SEEK_CUR: ULONG = 1;
const STREAM_SEEK_END: ULONG = 2;
const STGTY_STREAM: ULONG = 2;
const RTL_STREAM_SECTION_BACKED: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Guid { data1: u32, data2: u16, data3: u16, data4: [u8; 8] }
const IID_IUNKNOWN: Guid = Guid { data1: 0, data2: 0, data3: 0, data4: [0xC0,0,0,0,0,0,0,0x46] };
const IID_ISEQUENTIALSTREAM: Guid = Guid { data1: 0x0C733A30, data2: 0x2A1C, data3: 0x11CE, data4: [0xAD,0xE5,0,0xAA,0x00,0x44,0x77,0x3D] };
const IID_ISTREAM: Guid = Guid { data1: 0x0000000C, data2: 0, data3: 0, data4: [0xC0,0,0,0,0,0,0,0x46] };

#[repr(C)]
pub struct RTL_MEMORY_STREAM {
    pub(super) vtbl: *const IStreamVtable,
    pub(super) ref_count: i32,
    pub(super) unknown: u32,
    pub(super) current: PVOID,
    pub(super) start: PVOID,
    pub(super) end: PVOID,
    pub(super) final_release: Option<unsafe extern "system" fn(*mut RTL_MEMORY_STREAM)>,
    pub(super) process_handle: HANDLE,
}
#[repr(C)]
struct IStreamVtable {
    query_interface: unsafe extern "system" fn(PVOID, PVOID, *mut PVOID) -> NTSTATUS,
    add_ref: unsafe extern "system" fn(PVOID) -> ULONG,
    release: unsafe extern "system" fn(PVOID) -> ULONG,
    read: unsafe extern "system" fn(PVOID, PVOID, ULONG, *mut ULONG) -> NTSTATUS,
    write: unsafe extern "system" fn(PVOID, *const c_void, ULONG, *mut ULONG) -> NTSTATUS,
    seek: unsafe extern "system" fn(PVOID, i64, ULONG, *mut u64) -> NTSTATUS,
    set_size: unsafe extern "system" fn(PVOID, u64) -> NTSTATUS,
    copy_to: unsafe extern "system" fn(PVOID, PVOID, u64, *mut u64, *mut u64) -> NTSTATUS,
    commit: unsafe extern "system" fn(PVOID, ULONG) -> NTSTATUS,
    revert: unsafe extern "system" fn(PVOID) -> NTSTATUS,
    lock_region: unsafe extern "system" fn(PVOID, u64, u64, ULONG) -> NTSTATUS,
    unlock_region: unsafe extern "system" fn(PVOID, u64, u64, ULONG) -> NTSTATUS,
    stat: unsafe extern "system" fn(PVOID, PVOID, ULONG) -> NTSTATUS,
    clone_stream: unsafe extern "system" fn(PVOID, *mut PVOID) -> NTSTATUS,
}
#[repr(C)]
pub(super) struct StatStg {
    pub(super) name: PVOID,
    pub(super) kind: ULONG,
    pub(super) _pad: ULONG,
    pub(super) size: u64,
    pub(super) modified: [u64; 2],
    pub(super) accessed: u64,
    pub(super) mode: ULONG,
    pub(super) locks_supported: ULONG,
    pub(super) class_id: Guid,
    pub(super) state_bits: ULONG,
    pub(super) reserved: ULONG,
}
unsafe extern "system" fn stream_write(this: PVOID, buffer: PVOID, length: ULONG, written: *mut ULONG) -> NTSTATUS {
    let _ = (this, buffer, length);
    if !written.is_null() { unsafe { ptr::write(written, 0); } }
    E_NOTIMPL
}
unsafe extern "system" fn stream_set_size(this: PVOID, size: u64) -> NTSTATUS { let _ = (this, size); E_NOTIMPL }
unsafe extern "system" fn stream_commit(this: PVOID, flags: ULONG) -> NTSTATUS { let _ = (this, flags); E_NOTIMPL }
unsafe extern "system" fn stream_revert(this: PVOID) -> NTSTATUS { let _ = this; E_NOTIMPL }
unsafe extern "system" fn stream_lock_region(this: PVOID, offset: u64, length: u64, kind: ULONG) -> NTSTATUS { let _ = (this, offset, length, kind); E_NOTIMPL }
unsafe extern "system" fn stream_unlock_region(this: PVOID, offset: u64, length: u64, kind: ULONG) -> NTSTATUS { let _ = (this, offset, length, kind); E_NOTIMPL }
static MEMORY_STREAM_VTABLE: IStreamVtable = IStreamVtable {
    query_interface: RtlQueryInterfaceMemoryStream,
    add_ref: RtlAddRefMemoryStream,
    release: RtlReleaseMemoryStream,
    read: RtlReadMemoryStream,
    write: RtlWriteMemoryStream,
    seek: RtlSeekMemoryStream,
    set_size: RtlSetMemoryStreamSize,
    copy_to: RtlCopyMemoryStreamTo,
    commit: RtlCommitMemoryStream,
    revert: RtlRevertMemoryStream,
    lock_region: RtlLockMemoryStreamRegion,
    unlock_region: RtlUnlockMemoryStreamRegion,
    stat: RtlStatMemoryStream,
    clone_stream: RtlCloneMemoryStream,
};
static OUT_OF_PROCESS_MEMORY_STREAM_VTABLE: IStreamVtable = IStreamVtable {
    query_interface: RtlQueryInterfaceMemoryStream,
    add_ref: RtlAddRefMemoryStream,
    release: RtlReleaseMemoryStream,
    read: RtlReadOutOfProcessMemoryStream,
    write: RtlWriteMemoryStream,
    seek: RtlSeekMemoryStream,
    set_size: stream_set_size,
    copy_to: RtlCopyMemoryStreamTo,
    commit: stream_commit,
    revert: stream_revert,
    lock_region: stream_lock_region,
    unlock_region: stream_unlock_region,
    stat: RtlStatMemoryStream,
    clone_stream: RtlCloneMemoryStream,
};
#[no_mangle]
pub unsafe extern "system" fn RtlInitMemoryStream(stream: *mut RTL_MEMORY_STREAM) {
    if stream.is_null() { return; }
    unsafe {
        ptr::write(stream, RTL_MEMORY_STREAM {
            vtbl: &MEMORY_STREAM_VTABLE, ref_count: 0, unknown: 0,
            current: ptr::null_mut(), start: ptr::null_mut(), end: ptr::null_mut(),
            final_release: None, process_handle: ptr::null_mut(),
        });
    }
}
/// Initialize an IStream-compatible object backed by a kernel section object.
/// The stream storage remains caller-owned; the section reference is released
/// when its final COM-style reference is dropped.
pub unsafe extern "system" fn RtlInitSectionMemoryStream(stream: *mut RTL_MEMORY_STREAM, size: SIZE_T) -> NTSTATUS {
    if stream.is_null() || size == 0 { return STATUS_INVALID_PARAMETER; }
    let mut handle: PVOID = ptr::null_mut();
    let status = unsafe { cognacos_rtl_create_section(size, &mut handle) };
    if status < 0 { return status; }
    unsafe {
        ptr::write(stream, RTL_MEMORY_STREAM {
            vtbl: &MEMORY_STREAM_VTABLE, ref_count: 1, unknown: RTL_STREAM_SECTION_BACKED,
            current: ptr::null_mut(), start: ptr::null_mut(), end: size as PVOID,
            final_release: Some(RtlFinalReleaseSectionMemoryStream), process_handle: handle,
        });
    }
    STATUS_SUCCESS
}
pub unsafe extern "system" fn RtlFinalReleaseSectionMemoryStream(stream: *mut RTL_MEMORY_STREAM) {
    if stream.is_null() { return; }
    let handle = unsafe { (*stream).process_handle };
    if !handle.is_null() { let _ = unsafe { cognacos_rtl_dereference_section(handle) }; }
    unsafe { (*stream).process_handle = ptr::null_mut(); (*stream).vtbl = ptr::null(); }
}
unsafe extern "system" fn release_owned_section_memory_stream(stream: *mut RTL_MEMORY_STREAM) {
    if stream.is_null() { return; }
    unsafe { RtlFinalReleaseSectionMemoryStream(stream); native_vm_release(stream.cast()); }
}
#[no_mangle]
pub unsafe extern "system" fn RtlInitOutOfProcessMemoryStream(stream: *mut RTL_MEMORY_STREAM) {
    if stream.is_null() { return; }
    unsafe {
        ptr::write(stream, RTL_MEMORY_STREAM {
            vtbl: &OUT_OF_PROCESS_MEMORY_STREAM_VTABLE, ref_count: 0, unknown: 0,
            current: ptr::null_mut(), start: ptr::null_mut(), end: ptr::null_mut(),
            final_release: Some(RtlFinalReleaseOutOfProcessMemoryStream), process_handle: ptr::null_mut(),
        });
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlFinalReleaseOutOfProcessMemoryStream(stream: *mut RTL_MEMORY_STREAM) {
    // The process handle is borrowed, not owned. The caller controls the stream
    // storage and process-handle lifetime.
    let _ = stream;
}
#[no_mangle]
pub unsafe extern "system" fn RtlQueryInterfaceMemoryStream(this: PVOID, iid: PVOID, result: *mut PVOID) -> NTSTATUS {
    if this.is_null() || iid.is_null() || result.is_null() { return STG_E_INVALIDPOINTER; }
    let requested = unsafe { &*iid.cast::<Guid>() };
    if *requested == IID_IUNKNOWN || *requested == IID_ISEQUENTIALSTREAM || *requested == IID_ISTREAM {
        unsafe { RtlAddRefMemoryStream(this); ptr::write(result, this); }
        S_OK
    } else {
        unsafe { ptr::write(result, ptr::null_mut()); }
        E_NOINTERFACE
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlAddRefMemoryStream(this: PVOID) -> ULONG {
    if this.is_null() { return 0; }
    let stream = this.cast::<RTL_MEMORY_STREAM>();
    unsafe { (*stream).ref_count = (*stream).ref_count.wrapping_add(1); (*stream).ref_count.max(0) as ULONG }
}
#[no_mangle]
pub unsafe extern "system" fn RtlReleaseMemoryStream(this: PVOID) -> ULONG {
    if this.is_null() { return 0; }
    let stream = this.cast::<RTL_MEMORY_STREAM>();
    let (count, final_release) = unsafe {
        (*stream).ref_count = (*stream).ref_count.wrapping_sub(1);
        ((*stream).ref_count.max(0) as ULONG, if (*stream).ref_count == 0 { (*stream).final_release } else { None })
    };
    if let Some(callback) = final_release { unsafe { callback(stream); } }
    count
}
#[no_mangle]
pub unsafe extern "system" fn RtlReadMemoryStream(this: PVOID, buffer: PVOID, length: ULONG, bytes_read: *mut ULONG) -> NTSTATUS {
    if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, 0); } }
    if this.is_null() || (length != 0 && buffer.is_null()) { return STG_E_INVALIDPOINTER; }
    if length == 0 { return S_OK; }
    let stream = unsafe { &mut *this.cast::<RTL_MEMORY_STREAM>() };
    let current = stream.current as usize;
    let start = stream.start as usize;
    let end = stream.end as usize;
    if start > current || current > end { return STG_E_INVALIDPOINTER; }
    let amount = (end - current).min(length as usize);
    if stream.unknown == RTL_STREAM_SECTION_BACKED {
        if amount != 0 {
            let status = unsafe { cognacos_rtl_section_copy(stream.process_handle, current as u64, buffer, amount, 0) };
            if status < 0 { return status; }
            stream.current = (current + amount) as PVOID;
        }
    } else if amount != 0 {
        unsafe { ptr::copy(current as *const u8, buffer.cast::<u8>(), amount); }
        stream.current = (current + amount) as PVOID;
    }
    if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, amount as ULONG); } }
    if amount < length as usize { S_FALSE } else { S_OK }
}
#[no_mangle]
pub unsafe extern "system" fn RtlReadOutOfProcessMemoryStream(this: PVOID, buffer: PVOID, length: ULONG, bytes_read: *mut ULONG) -> NTSTATUS {
    if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, 0); } }
    if this.is_null() || (length != 0 && buffer.is_null()) { return STG_E_INVALIDPOINTER; }
    let stream = unsafe { &mut *this.cast::<RTL_MEMORY_STREAM>() };
    if stream.unknown == RTL_STREAM_SECTION_BACKED { return unsafe { RtlReadMemoryStream(this, buffer, length, bytes_read) }; }
    let current = stream.current as usize;
    let end = stream.end as usize;
    if current > end { return STG_E_INVALIDPOINTER; }
    let amount = (end - current).min(length as usize);
    if amount == 0 { return S_OK; }
    let mut copied = 0usize;
    let status = unsafe { cognacos_nt_read_virtual_memory(stream.process_handle, stream.current, buffer, amount, &mut copied) };
    if status < 0 { return status; }
    stream.current = (current + copied) as PVOID;
    if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, copied as ULONG); } }
    if copied < length as usize { S_FALSE } else { S_OK }
}
#[no_mangle]
pub unsafe extern "system" fn RtlSeekMemoryStream(this: PVOID, offset: i64, origin: ULONG, new_position: *mut u64) -> NTSTATUS {
    if this.is_null() { return STG_E_INVALIDPOINTER; }
    let stream = unsafe { &mut *this.cast::<RTL_MEMORY_STREAM>() };
    let start = stream.start as usize;
    let current = stream.current as usize;
    let end = stream.end as usize;
    if start > current || current > end { return STG_E_INVALIDPOINTER; }
    let base = match origin { STREAM_SEEK_SET => start, STREAM_SEEK_CUR => current, STREAM_SEEK_END => end, _ => return E_INVALIDARG };
    let candidate = if offset >= 0 { base.checked_add(offset as usize) } else { base.checked_sub(offset.unsigned_abs() as usize) };
    let Some(candidate) = candidate else { return E_INVALIDARG; };
    if candidate < start || candidate > end { return STG_E_INVALIDPOINTER; }
    stream.current = candidate as PVOID;
    if !new_position.is_null() { unsafe { ptr::write(new_position, (candidate - start) as u64); } }
    S_OK
}
#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemoryStreamTo(this: PVOID, target: PVOID, length: u64, bytes_read: *mut u64, bytes_written: *mut u64) -> NTSTATUS {
    if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, 0); } }
    if !bytes_written.is_null() { unsafe { ptr::write(bytes_written, 0); } }
    if this.is_null() { return STG_E_INVALIDPOINTER; }
    if target.is_null() || length == 0 { return S_OK; }
    let mut total = 0u64;
    let mut scratch = [0u8; 1024];
    while total < length {
        let chunk = (length - total).min(scratch.len() as u64) as ULONG;
        let mut got = 0u32;
        let read_fn = unsafe { (*(*this.cast::<RTL_MEMORY_STREAM>()).vtbl).read };
        let status = unsafe { read_fn(this, scratch.as_mut_ptr().cast(), chunk, &mut got) };
        if status < 0 || got == 0 { if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, total); } } return status; }
        let vtbl = unsafe { (*target.cast::<RTL_MEMORY_STREAM>()).vtbl };
        if vtbl.is_null() { return STG_E_INVALIDPOINTER; }
        let mut put = 0u32;
        let status = unsafe { ((*vtbl).write)(target, scratch.as_ptr().cast(), got, &mut put) };
        total += got as u64;
        if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, total); } }
        if !bytes_written.is_null() { unsafe { ptr::write(bytes_written, (*bytes_written).saturating_add(put as u64)); } }
        if status < 0 || put != got { return if status < 0 { status } else { STG_E_MEDIUMFULL }; }
    }
    S_OK
}
#[no_mangle]
pub unsafe extern "system" fn RtlStatMemoryStream(this: PVOID, stat: PVOID, flags: ULONG) -> NTSTATUS {
    let _ = flags;
    if this.is_null() || stat.is_null() { return STG_E_INVALIDPOINTER; }
    let stream = unsafe { &*this.cast::<RTL_MEMORY_STREAM>() };
    let start = stream.start as usize;
    let end = stream.end as usize;
    if end < start { return STG_E_INVALIDPOINTER; }
    let out = stat.cast::<StatStg>();
    unsafe { ptr::write_bytes(out.cast::<u8>(), 0, core::mem::size_of::<StatStg>()); (*out).kind = STGTY_STREAM; (*out).size = (end - start) as u64; }
    S_OK
}
#[no_mangle]
pub unsafe extern "system" fn RtlCloneMemoryStream(this: PVOID, result: *mut PVOID) -> NTSTATUS {
    if this.is_null() || result.is_null() { return STG_E_INVALIDPOINTER; }
    unsafe { ptr::write(result, ptr::null_mut()); }
    let source = unsafe { &*this.cast::<RTL_MEMORY_STREAM>() };
    if source.unknown == RTL_STREAM_SECTION_BACKED {
        let status = unsafe { cognacos_rtl_reference_section(source.process_handle) };
        if status < 0 { return status; }
    }
    let Some((base, _)) = (unsafe { native_vm_allocate(core::mem::size_of::<RTL_MEMORY_STREAM>()) }) else {
        if source.unknown == RTL_STREAM_SECTION_BACKED { let _ = unsafe { cognacos_rtl_dereference_section(source.process_handle) }; }
        return STATUS_NO_MEMORY;
    };
    let clone = base.cast::<RTL_MEMORY_STREAM>();
    unsafe {
        ptr::write(clone, RTL_MEMORY_STREAM {
            vtbl: source.vtbl, ref_count: 1, unknown: source.unknown,
            current: source.current, start: source.start, end: source.end,
            final_release: Some(if source.unknown == RTL_STREAM_SECTION_BACKED { release_owned_section_memory_stream } else { release_owned_memory_stream }),
            process_handle: source.process_handle,
        });
        ptr::write(result, clone.cast());
    }
    S_OK
}
unsafe extern "system" fn release_owned_memory_stream(stream: *mut RTL_MEMORY_STREAM) {
    if !stream.is_null() {
        unsafe { (*stream).ref_count = 0; (*stream).vtbl = ptr::null(); native_vm_release(stream.cast()); }
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlCommitMemoryStream(this: PVOID, flags: ULONG) -> NTSTATUS {
    let _ = flags;
    if this.is_null() { STG_E_INVALIDPOINTER } else { S_OK }
}
#[no_mangle]
pub unsafe extern "system" fn RtlRevertMemoryStream(this: PVOID) -> NTSTATUS { let _ = this; E_NOTIMPL }
#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryStreamRegion(this: PVOID, offset: u64, length: u64, lock_type: ULONG) -> NTSTATUS { let _ = (this, offset, length, lock_type); E_NOTIMPL }
#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryStreamRegion(this: PVOID, offset: u64, length: u64, lock_type: ULONG) -> NTSTATUS { let _ = (this, offset, length, lock_type); E_NOTIMPL }
#[no_mangle]
pub unsafe extern "system" fn RtlSetMemoryStreamSize(this: PVOID, new_size: u64) -> NTSTATUS {
    if this.is_null() { return STG_E_INVALIDPOINTER; }
    let stream = unsafe { &mut *this.cast::<RTL_MEMORY_STREAM>() };
    if stream.unknown == RTL_STREAM_SECTION_BACKED { return E_NOTIMPL; }
    let start = stream.start as usize;
    let end = stream.end as usize;
    let current = stream.current as usize;
    if end < start || current < start || current > end { return STG_E_INVALIDPOINTER; }
    let old_size = end - start;
    let Ok(requested) = usize::try_from(new_size) else { return E_INVALIDARG; };
    // The caller-owned stream ABI exposes no capacity field. Shrinking is safe;
    // growing would write beyond the known buffer and is rejected.
    if requested > old_size { return E_NOTIMPL; }
    let new_end = match start.checked_add(requested) { Some(v) => v, None => return E_INVALIDARG };
    stream.end = new_end as PVOID;
    if current > new_end { stream.current = new_end as PVOID; }
    S_OK
}
#[no_mangle]
pub unsafe extern "system" fn RtlWriteMemoryStream(this: PVOID, buffer: *const c_void, length: ULONG, written: *mut ULONG) -> NTSTATUS {
    if !written.is_null() { unsafe { ptr::write(written, 0); } }
    if this.is_null() || (length != 0 && buffer.is_null()) { return STG_E_INVALIDPOINTER; }
    if length == 0 { return S_OK; }
    let stream = unsafe { &mut *this.cast::<RTL_MEMORY_STREAM>() };
    let current = stream.current as usize;
    let start = stream.start as usize;
    let end = stream.end as usize;
    if start > current || current > end { return STG_E_INVALIDPOINTER; }
    let amount = (end - current).min(length as usize);
    if stream.unknown == RTL_STREAM_SECTION_BACKED {
        if amount != 0 {
            let status = unsafe { cognacos_rtl_section_copy(stream.process_handle, current as u64, buffer.cast_mut(), amount, 1) };
            if status < 0 { return status; }
            stream.current = (current + amount) as PVOID;
        }
    } else if amount != 0 {
        unsafe { ptr::copy(buffer.cast::<u8>(), current as *mut u8, amount); }
        stream.current = (current + amount) as PVOID;
    }
    if !written.is_null() { unsafe { ptr::write(written, amount as ULONG); } }
    if amount < length as usize { S_FALSE } else { S_OK }
}


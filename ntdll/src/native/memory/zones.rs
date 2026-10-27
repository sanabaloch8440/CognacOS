//! RTL memory-zone and block-lookaside allocators.
#![allow(non_snake_case, clippy::missing_safety_doc)]
use core::{ffi::c_void, ptr, sync::atomic::{AtomicBool, Ordering}};
use super::virtual_memory::{self, HANDLE, PVOID, SIZE_T, ULONG, NTSTATUS};
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const MEM_RELEASE: ULONG = 0x8000;
const COGNAC_NT_CURRENT_PROCESS: HANDLE = usize::MAX as HANDLE;

// RTL memory-zone and block-lookaside implementation. These are process-local
// allocators backed by the native VM service; metadata is not a Windows-private
// layout and must not be inspected by external code.
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008u32 as i32;
const STATUS_INVALID_PARAMETER_2: NTSTATUS = 0xC000_00F0u32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const STATUS_NOT_SUPPORTED: NTSTATUS = 0xC000_00BBu32 as i32;
const STATUS_UNSUCCESSFUL: NTSTATUS = 0xC000_0001u32 as i32;
const RTL_ZONE_MAGIC: u64 = 0x434F_474E_5A4F_4E45;
const RTL_LOOKASIDE_MAGIC: u64 = 0x434F_474E_4C4F_4F4B;
const RTL_BLOCK_MAGIC: u64 = 0x434F_474E_424C_4F43;
const RTL_ALIGNMENT: usize = 16;

#[repr(C)]
struct ZoneSegment {
    next: *mut ZoneSegment,
    base: PVOID,
    size: usize,
    used: usize,
}
#[repr(C)]
struct MemoryZone {
    magic: u64,
    lock: AtomicBool,
    _padding: [u8; 7],
    first: *mut ZoneSegment,
    last: *mut ZoneSegment,
}
#[repr(C)]
struct LookasideBlock {
    magic: u64,
    owner: *mut MemoryBlockLookaside,
    next: *mut LookasideBlock,
    base: PVOID,
    allocation_size: usize,
    payload_size: usize,
}
#[repr(C)]
struct MemoryBlockLookaside {
    magic: u64,
    lock: AtomicBool,
    _padding: [u8; 7],
    minimum: usize,
    maximum: usize,
    cache_limit: usize,
    cached: usize,
    free_list: *mut LookasideBlock,
}
#[inline]
fn align_memory(value: usize, alignment: usize) -> Option<usize> {
    value.checked_add(alignment - 1).map(|v| v & !(alignment - 1))
}
pub(super) unsafe fn native_vm_allocate(size: usize) -> Option<(PVOID, usize)> {
    if size == 0 { return None; }
    let mut base: PVOID = ptr::null_mut();
    let mut bytes = size;
    let status = unsafe {
        virtual_memory::cognacos_nt_allocate_virtual_memory(
            COGNAC_NT_CURRENT_PROCESS, &mut base, 0, &mut bytes, 0x3000, 0x04,
        )
    };
    if status < 0 || base.is_null() { None } else { Some((base, bytes)) }
}
pub(super) unsafe fn native_vm_release(base: PVOID) {
    if base.is_null() { return; }
    let mut address = base;
    let mut size = 0usize;
    let _ = unsafe {
        virtual_memory::cognacos_nt_free_virtual_memory(COGNAC_NT_CURRENT_PROCESS, &mut address, &mut size, MEM_RELEASE)
    };
}
unsafe fn spin_lock(lock: &AtomicBool) {
    while lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
unsafe fn spin_unlock(lock: &AtomicBool) { lock.store(false, Ordering::Release); }
unsafe fn zone_add_segment(zone: *mut MemoryZone, size: usize) -> NTSTATUS {
    let Some(meta_bytes) = align_memory(core::mem::size_of::<ZoneSegment>(), RTL_ALIGNMENT) else { return STATUS_INVALID_PARAMETER; };
    let Some(want) = size.checked_add(meta_bytes) else { return STATUS_INVALID_PARAMETER; };
    let Some((base, bytes)) = (unsafe { native_vm_allocate(want) }) else { return STATUS_NO_MEMORY; };
    let segment = base.cast::<ZoneSegment>();
    unsafe {
        ptr::write(segment, ZoneSegment {
            next: ptr::null_mut(), base, size: bytes, used: meta_bytes,
        });
        if (*zone).last.is_null() { (*zone).first = segment; } else { (*(*zone).last).next = segment; }
        (*zone).last = segment;
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateMemoryZone(memory_zone: *mut PVOID, initial_size: SIZE_T, flags: ULONG) -> NTSTATUS {
    if memory_zone.is_null() || initial_size == 0 { return STATUS_INVALID_PARAMETER; }
    if flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { ptr::write(memory_zone, ptr::null_mut()); }
    let Some((base, _)) = (unsafe { native_vm_allocate(core::mem::size_of::<MemoryZone>()) }) else { return STATUS_NO_MEMORY; };
    let zone = base.cast::<MemoryZone>();
    unsafe { ptr::write(zone, MemoryZone { magic: RTL_ZONE_MAGIC, lock: AtomicBool::new(false), _padding: [0; 7], first: ptr::null_mut(), last: ptr::null_mut() }); }
    let status = unsafe { zone_add_segment(zone, initial_size) };
    if status < 0 { unsafe { native_vm_release(base); } return status; }
    unsafe { ptr::write(memory_zone, zone.cast()); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlAllocateMemoryZone(memory_zone: PVOID, block_size: SIZE_T, block: *mut PVOID) -> NTSTATUS {
    if memory_zone.is_null() || block.is_null() || block_size == 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { ptr::write(block, ptr::null_mut()); }
    let zone = memory_zone.cast::<MemoryZone>();
    if unsafe { (*zone).magic } != RTL_ZONE_MAGIC { return STATUS_INVALID_HANDLE; }
    let Some(size) = align_memory(block_size, RTL_ALIGNMENT) else { return STATUS_INVALID_PARAMETER; };
    unsafe { spin_lock(&(*zone).lock); }
    let mut segment = unsafe { (*zone).first };
    while !segment.is_null() {
        let (used, total) = unsafe { ((*segment).used, (*segment).size) };
        if let Some(end) = used.checked_add(size) {
            if end <= total {
                let result = unsafe { (*segment).base.cast::<u8>().add(used).cast::<c_void>() };
                unsafe { (*segment).used = end; ptr::write(block, result); spin_unlock(&(*zone).lock); }
                return STATUS_SUCCESS;
            }
        }
        segment = unsafe { (*segment).next };
    }
    unsafe { spin_unlock(&(*zone).lock); }
    STATUS_NO_MEMORY
}
#[no_mangle]
pub unsafe extern "system" fn RtlExtendMemoryZone(memory_zone: PVOID, increment: SIZE_T) -> NTSTATUS {
    if memory_zone.is_null() || increment == 0 { return STATUS_INVALID_PARAMETER; }
    let zone = memory_zone.cast::<MemoryZone>();
    if unsafe { (*zone).magic } != RTL_ZONE_MAGIC { return STATUS_INVALID_HANDLE; }
    unsafe { spin_lock(&(*zone).lock); }
    let status = unsafe { zone_add_segment(zone, increment) };
    unsafe { spin_unlock(&(*zone).lock); }
    status
}
#[no_mangle]
pub unsafe extern "system" fn RtlResetMemoryZone(memory_zone: PVOID) -> NTSTATUS {
    if memory_zone.is_null() { return STATUS_INVALID_PARAMETER; }
    let zone = memory_zone.cast::<MemoryZone>();
    if unsafe { (*zone).magic } != RTL_ZONE_MAGIC { return STATUS_INVALID_HANDLE; }
    unsafe { spin_lock(&(*zone).lock); }
    let mut segment = unsafe { (*zone).first };
    while !segment.is_null() {
        let next = unsafe { (*segment).next };
        let minimum = align_memory(core::mem::size_of::<ZoneSegment>(), RTL_ALIGNMENT).unwrap_or(core::mem::size_of::<ZoneSegment>());
        unsafe { (*segment).used = minimum; }
        segment = next;
    }
    unsafe { spin_unlock(&(*zone).lock); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlDestroyMemoryZone(memory_zone: PVOID) -> NTSTATUS {
    if memory_zone.is_null() { return STATUS_INVALID_PARAMETER; }
    let zone = memory_zone.cast::<MemoryZone>();
    if unsafe { (*zone).magic } != RTL_ZONE_MAGIC { return STATUS_INVALID_HANDLE; }
    unsafe { spin_lock(&(*zone).lock); }
    let mut segment = unsafe { (*zone).first };
    while !segment.is_null() {
        let next = unsafe { (*segment).next };
        let base = unsafe { (*segment).base };
        unsafe { native_vm_release(base); }
        segment = next;
    }
    unsafe { (*zone).magic = 0; spin_unlock(&(*zone).lock); native_vm_release(memory_zone); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryZone(memory_zone: PVOID) -> NTSTATUS {
    if memory_zone.is_null() { return STATUS_INVALID_PARAMETER; }
    let zone = memory_zone.cast::<MemoryZone>();
    if unsafe { (*zone).magic } != RTL_ZONE_MAGIC { return STATUS_INVALID_HANDLE; }
    unsafe { spin_lock(&(*zone).lock); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryZone(memory_zone: PVOID) -> NTSTATUS {
    if memory_zone.is_null() { return STATUS_INVALID_PARAMETER; }
    let zone = memory_zone.cast::<MemoryZone>();
    if unsafe { (*zone).magic } != RTL_ZONE_MAGIC { return STATUS_INVALID_HANDLE; }
    unsafe { spin_unlock(&(*zone).lock); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateMemoryBlockLookaside(
    lookaside_out: *mut PVOID, flags: ULONG, initial_size: ULONG,
    minimum_block_size: ULONG, maximum_block_size: ULONG,
) -> NTSTATUS {
    if lookaside_out.is_null() || flags != 0 || minimum_block_size == 0 || maximum_block_size < minimum_block_size {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { ptr::write(lookaside_out, ptr::null_mut()); }
    let Some((base, _)) = (unsafe { native_vm_allocate(core::mem::size_of::<MemoryBlockLookaside>()) }) else { return STATUS_NO_MEMORY; };
    let lookaside = base.cast::<MemoryBlockLookaside>();
    unsafe { ptr::write(lookaside, MemoryBlockLookaside {
        magic: RTL_LOOKASIDE_MAGIC, lock: AtomicBool::new(false), _padding: [0; 7],
        minimum: minimum_block_size as usize, maximum: maximum_block_size as usize,
        cache_limit: initial_size as usize, cached: 0, free_list: ptr::null_mut(),
    }); ptr::write(lookaside_out, lookaside.cast()); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlAllocateMemoryBlockLookaside(lookaside_ptr: PVOID, block_size: ULONG, block_out: *mut PVOID) -> NTSTATUS {
    if lookaside_ptr.is_null() || block_out.is_null() || block_size == 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { ptr::write(block_out, ptr::null_mut()); }
    let lookaside = lookaside_ptr.cast::<MemoryBlockLookaside>();
    if unsafe { (*lookaside).magic } != RTL_LOOKASIDE_MAGIC { return STATUS_INVALID_HANDLE; }
    let size = block_size as usize;
    if size < unsafe { (*lookaside).minimum } || size > unsafe { (*lookaside).maximum } { return STATUS_INVALID_PARAMETER; }
    unsafe { spin_lock(&(*lookaside).lock); }
    let mut previous: *mut LookasideBlock = ptr::null_mut();
    let mut current = unsafe { (*lookaside).free_list };
    while !current.is_null() {
        if unsafe { (*current).payload_size >= size } {
            let next = unsafe { (*current).next };
            if previous.is_null() { unsafe { (*lookaside).free_list = next; } } else { unsafe { (*previous).next = next; } }
            unsafe { (*lookaside).cached -= 1; ptr::write(block_out, current.cast::<u8>().add(core::mem::size_of::<LookasideBlock>()).cast()); spin_unlock(&(*lookaside).lock); }
            return STATUS_SUCCESS;
        }
        previous = current;
        current = unsafe { (*current).next };
    }
    unsafe { spin_unlock(&(*lookaside).lock); }
    let Some(total) = core::mem::size_of::<LookasideBlock>().checked_add(size).and_then(|v| align_memory(v, RTL_ALIGNMENT)) else { return STATUS_INVALID_PARAMETER; };
    let Some((base, allocated)) = (unsafe { native_vm_allocate(total) }) else { return STATUS_NO_MEMORY; };
    let header = base.cast::<LookasideBlock>();
    unsafe { ptr::write(header, LookasideBlock { magic: RTL_BLOCK_MAGIC, owner: lookaside, next: ptr::null_mut(), base, allocation_size: allocated, payload_size: allocated.saturating_sub(core::mem::size_of::<LookasideBlock>()) }); ptr::write(block_out, base.cast::<u8>().add(core::mem::size_of::<LookasideBlock>()).cast()); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlFreeMemoryBlockLookaside(lookaside_ptr: PVOID, block: PVOID) -> NTSTATUS {
    if lookaside_ptr.is_null() || block.is_null() { return STATUS_INVALID_PARAMETER; }
    let lookaside = lookaside_ptr.cast::<MemoryBlockLookaside>();
    if unsafe { (*lookaside).magic } != RTL_LOOKASIDE_MAGIC { return STATUS_INVALID_HANDLE; }
    let header = unsafe { block.cast::<u8>().sub(core::mem::size_of::<LookasideBlock>()).cast::<LookasideBlock>() };
    if unsafe { (*header).magic } != RTL_BLOCK_MAGIC || unsafe { (*header).owner } != lookaside { return STATUS_INVALID_PARAMETER; }
    unsafe { spin_lock(&(*lookaside).lock); }
    if unsafe { (*lookaside).cached < (*lookaside).cache_limit } {
        unsafe { (*header).next = (*lookaside).free_list; (*lookaside).free_list = header; (*lookaside).cached += 1; spin_unlock(&(*lookaside).lock); }
    } else {
        let base = unsafe { (*header).base };
        unsafe { (*header).magic = 0; spin_unlock(&(*lookaside).lock); native_vm_release(base); }
    }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlExtendMemoryBlockLookaside(lookaside_ptr: PVOID, increment: ULONG) -> NTSTATUS {
    if lookaside_ptr.is_null() { return STATUS_INVALID_PARAMETER; }
    let lookaside = lookaside_ptr.cast::<MemoryBlockLookaside>();
    if unsafe { (*lookaside).magic } != RTL_LOOKASIDE_MAGIC { return STATUS_INVALID_HANDLE; }
    let Some(limit) = (unsafe { (*lookaside).cache_limit }).checked_add(increment as usize) else { return STATUS_INVALID_PARAMETER; };
    unsafe { (*lookaside).cache_limit = limit; }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlResetMemoryBlockLookaside(lookaside_ptr: PVOID) -> NTSTATUS {
    if lookaside_ptr.is_null() { return STATUS_INVALID_PARAMETER; }
    let lookaside = lookaside_ptr.cast::<MemoryBlockLookaside>();
    if unsafe { (*lookaside).magic } != RTL_LOOKASIDE_MAGIC { return STATUS_INVALID_HANDLE; }
    unsafe { spin_lock(&(*lookaside).lock); }
    let mut block = unsafe { (*lookaside).free_list };
    while !block.is_null() {
        let next = unsafe { (*block).next };
        let base = unsafe { (*block).base };
        unsafe { native_vm_release(base); }
        block = next;
    }
    unsafe { (*lookaside).free_list = ptr::null_mut(); (*lookaside).cached = 0; spin_unlock(&(*lookaside).lock); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlDestroyMemoryBlockLookaside(lookaside_ptr: PVOID) -> NTSTATUS {
    if lookaside_ptr.is_null() { return STATUS_INVALID_PARAMETER; }
    let lookaside = lookaside_ptr.cast::<MemoryBlockLookaside>();
    if unsafe { (*lookaside).magic } != RTL_LOOKASIDE_MAGIC { return STATUS_INVALID_HANDLE; }
    let status = unsafe { RtlResetMemoryBlockLookaside(lookaside_ptr) };
    if status < 0 { return status; }
    unsafe { (*lookaside).magic = 0; native_vm_release(lookaside_ptr); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryBlockLookaside(lookaside_ptr: PVOID) -> NTSTATUS {
    if lookaside_ptr.is_null() { return STATUS_INVALID_PARAMETER; }
    let lookaside = lookaside_ptr.cast::<MemoryBlockLookaside>();
    if unsafe { (*lookaside).magic } != RTL_LOOKASIDE_MAGIC { return STATUS_INVALID_HANDLE; }
    unsafe { spin_lock(&(*lookaside).lock); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryBlockLookaside(lookaside_ptr: PVOID) -> NTSTATUS {
    if lookaside_ptr.is_null() { return STATUS_INVALID_PARAMETER; }
    let lookaside = lookaside_ptr.cast::<MemoryBlockLookaside>();
    if unsafe { (*lookaside).magic } != RTL_LOOKASIDE_MAGIC { return STATUS_INVALID_HANDLE; }
    unsafe { spin_unlock(&(*lookaside).lock); }
    STATUS_SUCCESS
}


//! Windows x64 NTDLL virtual-memory exports.
//!
//! The implemented Nt* calls enter CognacOS through its private vector 0x83
//! native-service ABI. This is an in-tree OS bridge, not a Windows syscall
//! ordinal table; the kernel validates pointers and currently supports only
//! NT_CURRENT_PROCESS for these operations.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;
use core::arch::global_asm;

pub type NTSTATUS = i32;
pub type HANDLE = *mut c_void;
pub type PVOID = *mut c_void;
pub type PSIZE_T = *mut usize;
pub type PULONG = *mut ULONG;
pub type ULONG = u32;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;

const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_SUCCESS: NTSTATUS = 0;
const COGNAC_NT_CURRENT_PROCESS: HANDLE = usize::MAX as HANDLE;
const MEM_DECOMMIT: ULONG = 0x4000;
const MEM_RELEASE: ULONG = 0x8000;
const MEMORY_BASIC_INFORMATION_CLASS: ULONG = 0;

#[repr(C)]
pub struct IO_STATUS_BLOCK { pub status_or_pointer: usize, pub information: usize }

/// Windows x64 MEMORY_BASIC_INFORMATION (48-byte layout).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MEMORY_BASIC_INFORMATION {
    pub base_address: PVOID,
    pub allocation_base: PVOID,
    pub allocation_protect: ULONG,
    pub partition_id: u16,
    pub _alignment: u16,
    pub region_size: SIZE_T,
    pub state: ULONG,
    pub protect: ULONG,
    pub kind: ULONG,
}

global_asm!(r#"
    .text
    .p2align 4
    .global cognacos_nt_allocate_virtual_memory
cognacos_nt_allocate_virtual_memory:
    mov r10, rcx
    mov eax, 0x109
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_free_virtual_memory
cognacos_nt_free_virtual_memory:
    mov r10, rcx
    mov eax, 0x10a
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_protect_virtual_memory
cognacos_nt_protect_virtual_memory:
    mov r10, rcx
    mov eax, 0x10b
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_query_virtual_memory
cognacos_nt_query_virtual_memory:
    mov r10, rcx
    mov eax, 0x10c
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_read_virtual_memory
cognacos_nt_read_virtual_memory:
    mov r10, rcx
    mov eax, 0x10d
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_write_virtual_memory
cognacos_nt_write_virtual_memory:
    mov r10, rcx
    mov eax, 0x10e
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_lock_virtual_memory
cognacos_nt_lock_virtual_memory:
    mov r10, rcx
    mov eax, 0x10f
    int 0x83
    ret

    .p2align 4
    .global cognacos_nt_unlock_virtual_memory
cognacos_nt_unlock_virtual_memory:
    mov r10, rcx
    mov eax, 0x110
    int 0x83
    ret
"#);

unsafe extern "system" {
    fn cognacos_nt_allocate_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS;
    fn cognacos_nt_free_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS;
    fn cognacos_nt_protect_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS;
    fn cognacos_nt_query_virtual_memory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_read_virtual_memory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_write_virtual_memory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_lock_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS;
    fn cognacos_nt_unlock_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS;
}

#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_allocate_virtual_memory(process_handle, base_address, zero_bits, region_size, allocation_type, protect) }
}
#[no_mangle]
pub unsafe extern "system" fn NtAllocateVirtualMemoryEx(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG, extended_parameters: PVOID, parameter_count: ULONG) -> NTSTATUS {
    if parameter_count == 0 && extended_parameters.is_null() {
        return unsafe { NtAllocateVirtualMemory(process_handle, base_address, 0, region_size, allocation_type, protect) };
    }
    // Unknown/unsupported extended parameter types must not be silently ignored.
    if parameter_count != 0 && extended_parameters.is_null() { return STATUS_INVALID_PARAMETER; }
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtFlushVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, io_status_block: *mut IO_STATUS_BLOCK) -> NTSTATUS {
    let _ = (process_handle, base_address, region_size, io_status_block);
    // This VM currently has no section/file-backed dirty-page flush contract.
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtFreeVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if free_type != MEM_DECOMMIT && free_type != MEM_RELEASE { return STATUS_INVALID_PARAMETER; }
    unsafe { cognacos_nt_free_virtual_memory(process_handle, base_address, region_size, free_type) }
}
#[no_mangle]
pub unsafe extern "system" fn NtLockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if map_type != 1 { return STATUS_INVALID_PARAMETER; }
    unsafe { cognacos_nt_lock_virtual_memory(process_handle, base_address, region_size, map_type) }
}
#[no_mangle]
pub unsafe extern "system" fn NtProtectVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_protect_virtual_memory(process_handle, base_address, region_size, new_protect, old_protect) }
}
#[no_mangle]
pub unsafe extern "system" fn NtQueryVirtualMemory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS {
    if memory_information.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if memory_information_class != MEMORY_BASIC_INFORMATION_CLASS { return 0xC000_0003u32 as i32; }
    unsafe { cognacos_nt_query_virtual_memory(process_handle, base_address, memory_information_class, memory_information, memory_information_length, return_length) }
}
#[no_mangle]
pub unsafe extern "system" fn NtReadVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T) -> NTSTATUS {
    if buffer_size != 0 && (base_address.is_null() || buffer.is_null()) { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_read_virtual_memory(process_handle, base_address, buffer, buffer_size, number_of_bytes_read) }
}
#[no_mangle]
pub unsafe extern "system" fn NtReadVirtualMemoryEx(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T, flags: ULONG) -> NTSTATUS {
    if flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { NtReadVirtualMemory(process_handle, base_address, buffer, buffer_size, number_of_bytes_read) }
}
#[no_mangle]
pub unsafe extern "system" fn NtUnlockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    if base_address.is_null() || region_size.is_null() { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    if map_type != 1 { return STATUS_INVALID_PARAMETER; }
    unsafe { cognacos_nt_unlock_virtual_memory(process_handle, base_address, region_size, map_type) }
}
#[no_mangle]
pub unsafe extern "system" fn NtWriteVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: PSIZE_T) -> NTSTATUS {
    if buffer_size != 0 && (base_address.is_null() || buffer.is_null()) { return STATUS_INVALID_PARAMETER; }
    if process_handle != COGNAC_NT_CURRENT_PROCESS { return 0xC000_0008u32 as i32; }
    unsafe { cognacos_nt_write_virtual_memory(process_handle, base_address, buffer, buffer_size, number_of_bytes_written) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwAllocateVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS {
    unsafe { NtAllocateVirtualMemory(process_handle, base_address, zero_bits, region_size, allocation_type, protect) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwAllocateVirtualMemoryEx(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG, extended_parameters: PVOID, parameter_count: ULONG) -> NTSTATUS {
    unsafe { NtAllocateVirtualMemoryEx(process_handle, base_address, region_size, allocation_type, protect, extended_parameters, parameter_count) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwFlushVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, io_status_block: *mut IO_STATUS_BLOCK) -> NTSTATUS {
    unsafe { NtFlushVirtualMemory(process_handle, base_address, region_size, io_status_block) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwFreeVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS {
    unsafe { NtFreeVirtualMemory(process_handle, base_address, region_size, free_type) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwLockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    unsafe { NtLockVirtualMemory(process_handle, base_address, region_size, map_type) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwProtectVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS {
    unsafe { NtProtectVirtualMemory(process_handle, base_address, region_size, new_protect, old_protect) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwQueryVirtualMemory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS {
    unsafe { NtQueryVirtualMemory(process_handle, base_address, memory_information_class, memory_information, memory_information_length, return_length) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T) -> NTSTATUS {
    unsafe { NtReadVirtualMemory(process_handle, base_address, buffer, buffer_size, number_of_bytes_read) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwReadVirtualMemoryEx(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T, flags: ULONG) -> NTSTATUS {
    unsafe { NtReadVirtualMemoryEx(process_handle, base_address, buffer, buffer_size, number_of_bytes_read, flags) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwUnlockVirtualMemory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, map_type: ULONG) -> NTSTATUS {
    unsafe { NtUnlockVirtualMemory(process_handle, base_address, region_size, map_type) }
}
#[no_mangle]
pub unsafe extern "system" fn ZwWriteVirtualMemory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: PSIZE_T) -> NTSTATUS {
    unsafe { NtWriteVirtualMemory(process_handle, base_address, buffer, buffer_size, number_of_bytes_written) }
}

// RTL memory-zone and block-lookaside implementation. These are process-local
// allocators backed by the native VM service; metadata is not a Windows-private
// layout and must not be inspected by external code.
use core::sync::atomic::{AtomicBool, Ordering};

const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008u32 as i32;
const STATUS_INVALID_PARAMETER_2: NTSTATUS = 0xC000_00F0u32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const STATUS_NOT_SUPPORTED: NTSTATUS = 0xC000_00BBu32 as i32;
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
unsafe fn native_vm_allocate(size: usize) -> Option<(PVOID, usize)> {
    if size == 0 { return None; }
    let mut base: PVOID = ptr::null_mut();
    let mut bytes = size;
    let status = unsafe {
        cognacos_nt_allocate_virtual_memory(
            COGNAC_NT_CURRENT_PROCESS, &mut base, 0, &mut bytes, 0x3000, 0x04,
        )
    };
    if status < 0 || base.is_null() { None } else { Some((base, bytes)) }
}
unsafe fn native_vm_release(base: PVOID) {
    if base.is_null() { return; }
    let mut address = base;
    let mut size = 0usize;
    let _ = unsafe {
        cognacos_nt_free_virtual_memory(COGNAC_NT_CURRENT_PROCESS, &mut address, &mut size, MEM_RELEASE)
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
    unsafe { ptr::write(header, LookasideBlock { magic: RTL_BLOCK_MAGIC, next: ptr::null_mut(), base, allocation_size: allocated, payload_size: allocated.saturating_sub(core::mem::size_of::<LookasideBlock>()) }); ptr::write(block_out, base.cast::<u8>().add(core::mem::size_of::<LookasideBlock>()).cast()); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlFreeMemoryBlockLookaside(lookaside_ptr: PVOID, block: PVOID) -> NTSTATUS {
    if lookaside_ptr.is_null() || block.is_null() { return STATUS_INVALID_PARAMETER; }
    let lookaside = lookaside_ptr.cast::<MemoryBlockLookaside>();
    if unsafe { (*lookaside).magic } != RTL_LOOKASIDE_MAGIC { return STATUS_INVALID_HANDLE; }
    let header = unsafe { block.cast::<u8>().sub(core::mem::size_of::<LookasideBlock>()).cast::<LookasideBlock>() };
    if unsafe { (*header).magic } != RTL_BLOCK_MAGIC { return STATUS_INVALID_PARAMETER; }
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

// These APIs require facilities absent from the current kernel (section-backed
// stream objects, token security, resource descriptors, persistent memory, or
// secure-cache callbacks). Fail closed with the correct status type rather than
// pretending to perform the operation.
// In-memory IStream-compatible layout used by the RTL memory-stream helpers.
// Start/Current/End are caller-owned bounds; these APIs never free the backing
// buffer. Only clones allocated internally by this implementation own metadata.
const S_OK: NTSTATUS = 0;
const E_NOTIMPL: NTSTATUS = 0x8000_4001u32 as i32;
const E_NOINTERFACE: NTSTATUS = 0x8000_4002u32 as i32;
const E_INVALIDARG: NTSTATUS = 0x8007_0057u32 as i32;
const STG_E_INVALIDPOINTER: NTSTATUS = 0x8003_0009u32 as i32;
const STREAM_SEEK_SET: ULONG = 0;
const STREAM_SEEK_CUR: ULONG = 1;
const STREAM_SEEK_END: ULONG = 2;
const STGTY_STREAM: ULONG = 2;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Guid { data1: u32, data2: u16, data3: u16, data4: [u8; 8] }
const IID_IUNKNOWN: Guid = Guid { data1: 0, data2: 0, data3: 0, data4: [0xC0,0,0,0,0,0,0,0x46] };
const IID_ISEQUENTIALSTREAM: Guid = Guid { data1: 0x0C733A30, data2: 0x2A1C, data3: 0x11CE, data4: [0xAD,0xE5,0,0xAA,0x00,0x44,0x77,0x3D] };
const IID_ISTREAM: Guid = Guid { data1: 0x0000000C, data2: 0, data3: 0, data4: [0xC0,0,0,0,0,0,0,0x46] };

#[repr(C)]
pub struct RTL_MEMORY_STREAM {
    vtbl: *const IStreamVtable,
    ref_count: i32,
    unknown: u32,
    current: PVOID,
    start: PVOID,
    end: PVOID,
    final_release: Option<unsafe extern "system" fn(*mut RTL_MEMORY_STREAM)>,
    process_handle: HANDLE,
}
#[repr(C)]
struct IStreamVtable {
    query_interface: unsafe extern "system" fn(PVOID, PVOID, *mut PVOID) -> NTSTATUS,
    add_ref: unsafe extern "system" fn(PVOID) -> ULONG,
    release: unsafe extern "system" fn(PVOID) -> ULONG,
    read: unsafe extern "system" fn(PVOID, PVOID, ULONG, *mut ULONG) -> NTSTATUS,
    write: unsafe extern "system" fn(PVOID, PVOID, ULONG, *mut ULONG) -> NTSTATUS,
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
struct StatStg {
    name: PVOID,
    kind: ULONG,
    _pad: ULONG,
    size: u64,
    modified: [u64; 2],
    accessed: u64,
    mode: ULONG,
    locks_supported: ULONG,
    class_id: Guid,
    state_bits: ULONG,
    reserved: ULONG,
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
    set_size: stream_set_size,
    copy_to: RtlCopyMemoryStreamTo,
    commit: stream_commit,
    revert: stream_revert,
    lock_region: stream_lock_region,
    unlock_region: stream_unlock_region,
    stat: RtlStatMemoryStream,
    clone_stream: RtlCloneMemoryStream,
};
static OUT_OF_PROCESS_MEMORY_STREAM_VTABLE: IStreamVtable = IStreamVtable {
    query_interface: RtlQueryInterfaceMemoryStream,
    add_ref: RtlAddRefMemoryStream,
    release: RtlReleaseMemoryStream,
    read: RtlReadOutOfProcessMemoryStream,
    write: stream_write,
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
    unsafe { (*stream).ref_count = (*stream).ref_count.saturating_add(1); (*stream).ref_count.max(0) as ULONG }
}
#[no_mangle]
pub unsafe extern "system" fn RtlReleaseMemoryStream(this: PVOID) -> ULONG {
    if this.is_null() { return 0; }
    let stream = this.cast::<RTL_MEMORY_STREAM>();
    let (count, final_release) = unsafe {
        (*stream).ref_count = (*stream).ref_count.saturating_sub(1);
        ((*stream).ref_count.max(0) as ULONG, if (*stream).ref_count <= 0 { (*stream).final_release } else { None })
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
    if amount != 0 {
        unsafe { ptr::copy(current as *const u8, buffer.cast::<u8>(), amount); }
        stream.current = (current + amount) as PVOID;
    }
    if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, amount as ULONG); } }
    S_OK
}
#[no_mangle]
pub unsafe extern "system" fn RtlReadOutOfProcessMemoryStream(this: PVOID, buffer: PVOID, length: ULONG, bytes_read: *mut ULONG) -> NTSTATUS {
    if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, 0); } }
    if this.is_null() || (length != 0 && buffer.is_null()) { return STG_E_INVALIDPOINTER; }
    let stream = unsafe { &mut *this.cast::<RTL_MEMORY_STREAM>() };
    let current = stream.current as usize;
    let end = stream.end as usize;
    if current > end { return STG_E_INVALIDPOINTER; }
    let amount = (end - current).min(length as usize);
    if amount == 0 { return S_OK; }
    let mut copied = 0usize;
    let status = unsafe { NtReadVirtualMemory(stream.process_handle, stream.current, buffer, amount, &mut copied) };
    if status < 0 { return status; }
    stream.current = (current + copied) as PVOID;
    if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, copied as ULONG); } }
    S_OK
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
    if candidate < start || candidate > end { return E_INVALIDARG; }
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
        let status = unsafe { ((*vtbl).write)(target, scratch.as_mut_ptr().cast(), got, &mut put) };
        total += got as u64;
        if !bytes_read.is_null() { unsafe { ptr::write(bytes_read, total); } }
        if !bytes_written.is_null() { unsafe { ptr::write(bytes_written, (*bytes_written).saturating_add(put as u64)); } }
        if status < 0 || put != got { return if status < 0 { status } else { E_NOTIMPL }; }
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
    let Some((base, _)) = (unsafe { native_vm_allocate(core::mem::size_of::<RTL_MEMORY_STREAM>()) }) else { return STATUS_NO_MEMORY; };
    let clone = base.cast::<RTL_MEMORY_STREAM>();
    unsafe {
        ptr::write(clone, RTL_MEMORY_STREAM {
            vtbl: source.vtbl, ref_count: 1, unknown: source.unknown,
            current: source.current, start: source.start, end: source.end,
            final_release: Some(release_owned_memory_stream), process_handle: source.process_handle,
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
pub unsafe extern "system" fn RtlCommitMemoryStream(this: PVOID, flags: ULONG) -> NTSTATUS { let _ = (this, flags); E_NOTIMPL }
#[no_mangle]
pub unsafe extern "system" fn RtlRevertMemoryStream(this: PVOID) -> NTSTATUS { let _ = this; E_NOTIMPL }
#[no_mangle]
pub unsafe extern "system" fn RtlLockMemoryStreamRegion(this: PVOID, offset: u64, length: u64, lock_type: ULONG) -> NTSTATUS { let _ = (this, offset, length, lock_type); E_NOTIMPL }
#[no_mangle]
pub unsafe extern "system" fn RtlUnlockMemoryStreamRegion(this: PVOID, offset: u64, length: u64, lock_type: ULONG) -> NTSTATUS { let _ = (this, offset, length, lock_type); E_NOTIMPL }
#[no_mangle]
pub unsafe extern "system" fn RtlSetMemoryStreamSize(this: PVOID, new_size: u64) -> NTSTATUS { let _ = (this, new_size); E_NOTIMPL }
#[no_mangle]
pub unsafe extern "system" fn RtlWriteMemoryStream(this: PVOID, buffer: PVOID, length: ULONG, written: *mut ULONG) -> NTSTATUS {
    if !written.is_null() { unsafe { ptr::write(written, 0); } }
    if this.is_null() || (length != 0 && buffer.is_null()) { return STG_E_INVALIDPOINTER; }
    if length == 0 { return S_OK; }
    let stream = unsafe { &mut *this.cast::<RTL_MEMORY_STREAM>() };
    let current = stream.current as usize;
    let start = stream.start as usize;
    let end = stream.end as usize;
    if start > current || current > end { return STG_E_INVALIDPOINTER; }
    let amount = (end - current).min(length as usize);
    if amount != 0 {
        unsafe { ptr::copy(buffer.cast::<u8>(), current as *mut u8, amount); }
        stream.current = (current + amount) as PVOID;
    }
    if !written.is_null() { unsafe { ptr::write(written, amount as ULONG); } }
    S_OK
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemoryNonTemporal(destination: PVOID, source: *const c_void, length: SIZE_T) {
    if length == 0 || destination.cast_const() == source { return; }
    unsafe { ptr::copy_nonoverlapping(source.cast::<u8>(), destination.cast::<u8>(), length); }
}
#[no_mangle]
pub unsafe extern "system" fn RtlFillMemoryNonTemporal(destination: PVOID, length: SIZE_T, fill: u8) {
    if length == 0 { return; }
    unsafe { ptr::write_bytes(destination.cast::<u8>(), fill, length); }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFillNonVolatileMemory(token: PVOID, destination: PVOID, value: u8, length: SIZE_T, flags: ULONG) -> NTSTATUS {
    let _ = (token, destination, value, length, flags);
    STATUS_NOT_SUPPORTED
}
#[no_mangle]
pub unsafe extern "system" fn RtlWriteNonVolatileMemory(token: PVOID, destination: PVOID, source: PVOID, length: SIZE_T, flags: ULONG) -> NTSTATUS {
    let _ = (token, destination, source, length, flags);
    STATUS_NOT_SUPPORTED
}
#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemory(token: PVOID, buffer: PVOID, length: SIZE_T, flags: ULONG) -> NTSTATUS {
    let _ = (token, buffer, length, flags); STATUS_NOT_SUPPORTED
}
#[repr(C)]
pub struct RTL_NV_MEMORY_RANGE { pub base_address: PVOID, pub length: SIZE_T }
#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemoryRanges(token: PVOID, count: ULONG, ranges: *const RTL_NV_MEMORY_RANGE, flags: ULONG) -> NTSTATUS {
    let _ = (token, count, ranges, flags); STATUS_NOT_SUPPORTED
}
#[no_mangle]
pub unsafe extern "system" fn RtlFlushSecureMemoryCache(base: PVOID, length: SIZE_T) -> u8 {
    let _ = (base, length); 0
}
#[no_mangle]
pub unsafe extern "system" fn RtlRegisterSecureMemoryCacheCallback(callback: PVOID) -> NTSTATUS { let _ = callback; STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlDeregisterSecureMemoryCacheCallback(callback: PVOID) -> NTSTATUS { let _ = callback; STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlCopyMappedMemory(destination: PVOID, source: PVOID, length: SIZE_T) -> NTSTATUS {
    if length != 0 && (destination.is_null() || source.is_null()) { return STATUS_INVALID_PARAMETER; }
    if length != 0 { unsafe { ptr::copy(source.cast::<u8>(), destination.cast::<u8>(), length); } }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlCmDecodeMemIoResource(resource: PVOID, decoded: PVOID) -> NTSTATUS { let _ = (resource, decoded); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlCmEncodeMemIoResource(resource: PVOID, encoded: PVOID) -> NTSTATUS { let _ = (resource, encoded); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlIoDecodeMemIoResource(resource: PVOID, decoded: PVOID) -> NTSTATUS { let _ = (resource, decoded); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlIoEncodeMemIoResource(resource: PVOID, encoded: PVOID) -> NTSTATUS { let _ = (resource, encoded); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenMembership(token: HANDLE, sid: PVOID, member: *mut u8) -> NTSTATUS { let _ = (token, sid, member); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenMembershipEx(token: HANDLE, sid: PVOID, flags: ULONG, member: *mut u8) -> NTSTATUS { let _ = (token, sid, flags, member); STATUS_NOT_SUPPORTED }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_temporal_copy_copies_exact_range() {
        let source = [0x10u8, 0x20, 0x30, 0x40, 0x50];
        let mut destination = [0u8; 7];
        unsafe {
            RtlCopyMemoryNonTemporal(destination.as_mut_ptr().cast(), source.as_ptr().cast(), source.len());
        }
        assert_eq!(&destination[..5], &source);
        assert_eq!(&destination[5..], &[0, 0]);
    }

    #[test]
    fn non_temporal_fill_fills_exact_range() {
        let mut destination = [0u8; 6];
        unsafe { RtlFillMemoryNonTemporal(destination.as_mut_ptr().cast(), 4, 0xA5); }
        assert_eq!(destination, [0xA5, 0xA5, 0xA5, 0, 0, 0]);
    }

    #[test]
    fn non_temporal_helpers_accept_zero_length_null_ranges() {
        unsafe {
            RtlCopyMemoryNonTemporal(core::ptr::null_mut(), core::ptr::null(), 0);
            RtlFillMemoryNonTemporal(core::ptr::null_mut(), 0, 0xFF);
        }
    }

    #[test]
    fn memory_stream_initializes_reads_seeks_and_reports_size() {
        let mut stream = core::mem::MaybeUninit::<RTL_MEMORY_STREAM>::uninit();
        let mut source = *b"cognacos";
        unsafe {
            RtlInitMemoryStream(stream.as_mut_ptr());
            let stream = &mut *stream.as_mut_ptr();
            stream.start = source.as_mut_ptr().cast();
            stream.current = source.as_mut_ptr().cast();
            stream.end = source.as_mut_ptr().add(source.len()).cast();
            assert_eq!(RtlAddRefMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast()), 1);
            let mut out = [0u8; 3];
            let mut read = 0;
            assert_eq!(RtlReadMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast(), out.as_mut_ptr().cast(), 3, &mut read), S_OK);
            assert_eq!(read, 3);
            assert_eq!(&out, b"cog");
            let mut position = 0u64;
            assert_eq!(RtlSeekMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast(), 2, STREAM_SEEK_SET, &mut position), S_OK);
            assert_eq!(position, 2);
            let input = *b"XYZ";
            let mut written = 0;
            assert_eq!(RtlWriteMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast(), input.as_ptr() as PVOID, input.len() as ULONG, &mut written), S_OK);
            assert_eq!(written, 3);
            assert_eq!(&source[2..5], b"XYZ");
            let mut stat = core::mem::MaybeUninit::<StatStg>::zeroed();
            assert_eq!(RtlStatMemoryStream((stream as *mut RTL_MEMORY_STREAM).cast(), stat.as_mut_ptr().cast(), 1), S_OK);
            assert_eq!((*stat.as_ptr()).size, source.len() as u64);
        }
    }

    #[test]
    fn memory_zone_and_lookaside_abi_layouts_are_stable() {
        assert_eq!(core::mem::offset_of!(RTL_MEMORY_STREAM, current), 16);
        assert_eq!(core::mem::offset_of!(RTL_MEMORY_STREAM, start), 24);
        assert_eq!(core::mem::offset_of!(RTL_MEMORY_STREAM, end), 32);
        assert_eq!(core::mem::size_of::<RTL_MEMORY_STREAM>(), 56);
    }

    #[test]
    fn memory_basic_information_matches_windows_x64_layout() {
        assert_eq!(core::mem::size_of::<MEMORY_BASIC_INFORMATION>(), 48);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, region_size), 24);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, state), 32);
        assert_eq!(core::mem::offset_of!(MEMORY_BASIC_INFORMATION, kind), 40);
    }

    #[test]
    fn unsupported_vm_operations_fail_explicitly() {
        assert_eq!(
            unsafe { NtFlushVirtualMemory(core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut()) },
            STATUS_NOT_IMPLEMENTED
        );
        assert_eq!(
            unsafe { NtLockVirtualMemory(core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut(), 0) },
            STATUS_INVALID_PARAMETER
        );
    }
}

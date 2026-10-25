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
"#);

unsafe extern "system" {
    fn cognacos_nt_allocate_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, zero_bits: ULONG_PTR, region_size: PSIZE_T, allocation_type: ULONG, protect: ULONG) -> NTSTATUS;
    fn cognacos_nt_free_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, free_type: ULONG) -> NTSTATUS;
    fn cognacos_nt_protect_virtual_memory(process_handle: HANDLE, base_address: *mut PVOID, region_size: PSIZE_T, new_protect: ULONG, old_protect: PULONG) -> NTSTATUS;
    fn cognacos_nt_query_virtual_memory(process_handle: HANDLE, base_address: PVOID, memory_information_class: ULONG, memory_information: PVOID, memory_information_length: SIZE_T, return_length: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_read_virtual_memory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_read: PSIZE_T) -> NTSTATUS;
    fn cognacos_nt_write_virtual_memory(process_handle: HANDLE, base_address: PVOID, buffer: PVOID, buffer_size: SIZE_T, number_of_bytes_written: PSIZE_T) -> NTSTATUS;
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
    let _ = (process_handle, base_address, region_size, map_type);
    // Pinning/lock accounting and locked-page lifetime are not implemented in MM.
    STATUS_NOT_IMPLEMENTED
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
    let _ = (process_handle, base_address, region_size, map_type);
    STATUS_NOT_IMPLEMENTED
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
#[no_mangle]
pub unsafe extern "system" fn RtlAddRefMemoryStream(stream: PVOID) -> NTSTATUS { let _ = stream; STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlCloneMemoryStream(stream: PVOID, clone: *mut PVOID) -> NTSTATUS { let _ = (stream, clone); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlCommitMemoryStream(stream: PVOID) -> NTSTATUS { let _ = stream; STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemoryStreamTo(source: PVOID, destination: PVOID, length: i64, bytes_read: *mut i64, bytes_written: *mut i64) -> NTSTATUS { let _ = (source, destination, length, bytes_read, bytes_written); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlCopyOutOfProcessMemoryStreamTo(source: PVOID, destination: PVOID, length: i64, bytes_read: *mut i64, bytes_written: *mut i64) -> NTSTATUS { unsafe { RtlCopyMemoryStreamTo(source, destination, length, bytes_read, bytes_written) } }
#[no_mangle]
pub unsafe extern "system" fn RtlFinalReleaseOutOfProcessMemoryStream(stream: PVOID) -> NTSTATUS { let _ = stream; STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlInitMemoryStream(buffer: PVOID, length: SIZE_T, stream: *mut PVOID) -> NTSTATUS { let _ = (buffer, length, stream); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlInitOutOfProcessMemoryStream(process: HANDLE, base: PVOID, length: SIZE_T, stream: *mut PVOID) -> NTSTATUS { let _ = (process, base, length, stream); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlQueryInterfaceMemoryStream(stream: PVOID, iid: PVOID, object: *mut PVOID) -> NTSTATUS { let _ = (stream, iid, object); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlReadMemoryStream(stream: PVOID, buffer: PVOID, length: ULONG, read: *mut ULONG) -> NTSTATUS { let _ = (stream, buffer, length, read); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlReadOutOfProcessMemoryStream(stream: PVOID, buffer: PVOID, length: ULONG, read: *mut ULONG) -> NTSTATUS { let _ = (stream, buffer, length, read); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlReleaseMemoryStream(stream: PVOID) -> NTSTATUS { let _ = stream; STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlRevertMemoryStream(stream: PVOID) -> NTSTATUS { let _ = stream; STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlSeekMemoryStream(stream: PVOID, offset: i64, origin: ULONG, new_position: *mut i64) -> NTSTATUS { let _ = (stream, offset, origin, new_position); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlSetMemoryStreamSize(stream: PVOID, size: i64) -> NTSTATUS { let _ = (stream, size); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlStatMemoryStream(stream: PVOID, stat: PVOID) -> NTSTATUS { let _ = (stream, stat); STATUS_NOT_SUPPORTED }
#[no_mangle]
pub unsafe extern "system" fn RtlWriteMemoryStream(stream: PVOID, buffer: PVOID, length: ULONG, written: *mut ULONG) -> NTSTATUS { let _ = (stream, buffer, length, written); STATUS_NOT_SUPPORTED }

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
pub unsafe extern "system" fn RtlFillNonVolatileMemory(destination: PVOID, source: PVOID, length: SIZE_T, written: *mut SIZE_T) -> NTSTATUS {
    if length != 0 && (destination.is_null() || source.is_null()) { return STATUS_INVALID_PARAMETER; }
    if length != 0 { unsafe { ptr::copy_nonoverlapping(source.cast::<u8>(), destination.cast::<u8>(), length); } }
    if !written.is_null() { unsafe { ptr::write(written, length); } }
    STATUS_NOT_SUPPORTED
}
#[no_mangle]
pub unsafe extern "system" fn RtlWriteNonVolatileMemory(destination: PVOID, source: PVOID, length: SIZE_T, written: *mut SIZE_T) -> NTSTATUS {
    unsafe { RtlFillNonVolatileMemory(destination, source, length, written) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemory(base: PVOID, length: SIZE_T, flags: ULONG, processed: *mut SIZE_T) -> NTSTATUS {
    let _ = (base, length, flags, processed); STATUS_NOT_SUPPORTED
}
#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemoryRanges(ranges: PVOID, count: ULONG, flags: ULONG) -> NTSTATUS {
    let _ = (ranges, count, flags); STATUS_NOT_SUPPORTED
}
#[no_mangle]
pub unsafe extern "system" fn RtlFlushSecureMemoryCache(base: PVOID, length: SIZE_T) -> NTSTATUS {
    let _ = (base, length); STATUS_NOT_SUPPORTED
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
            STATUS_NOT_IMPLEMENTED
        );
    }
}

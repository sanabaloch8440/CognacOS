//! Source-first Rust port of CognacOS's bounded native NTDLL process heap.
//!
//! Source correspondence: root/wine/dlls/ntdll/cognac_heap.c.
//! This deliberately preserves the bootstrap heap's scope: one fixed 256 KiB
//! arena, first-fit allocation, 16-byte payload alignment, block splitting and
//! coalescing, and basic allocate/free/reallocate/size/validate operations.
//! Build integration is deferred. This is not the complete Windows heap ABI.
//!
//! # Safety
//! Exported routines accept raw Windows ABI pointers. Callers must provide valid
//! handles and buffers according to the routine contract. Heap metadata is
//! protected by a spin lock; invalid pointers are rejected where the C source
//! checks them, but arbitrary corrupt metadata cannot be made fully safe.

#![allow(non_camel_case_types, non_snake_case, static_mut_refs)]

use core::ptr;
use core::sync::atomic::{AtomicU8, Ordering};

pub type UCHAR = u8;
pub type ULONG = u32;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;
pub type BOOLEAN = u8;
pub type HANDLE = *mut core::ffi::c_void;
pub type PVOID = *mut core::ffi::c_void;

pub const HEAP_ZERO_MEMORY: ULONG = 0x0000_0008;
const HEAP_MAGIC: u64 = 0x434F_4748;
const BLOCK_MAGIC: ULONG = 0x434F_4742;
const PROCESS_HEAP_BYTES: usize = 256 * 1024;
const ALIGNMENT: usize = 16;
const INVALID_SIZE: SIZE_T = SIZE_T::MAX;

#[repr(C)]
struct CognacHeapBlock {
    size: SIZE_T,
    previous: *mut CognacHeapBlock,
    next: *mut CognacHeapBlock,
    magic: ULONG,
    free: ULONG,
}

#[repr(C)]
pub struct CognacHeap {
    magic: u64,
    first: *mut CognacHeapBlock,
    lock: AtomicU8,
}

// Exported object address is installed into PEB.ProcessHeap by the native loader.
#[no_mangle]
pub static mut CognacNtdllProcessHeapObject: CognacHeap = CognacHeap {
    magic: 0,
    first: ptr::null_mut(),
    lock: AtomicU8::new(0),
};

#[repr(align(16))]
struct AlignedArena([u8; PROCESS_HEAP_BYTES]);

static mut PROCESS_HEAP_ARENA: AlignedArena = AlignedArena([0; PROCESS_HEAP_BYTES]);

#[inline]
fn heap_ptr() -> *mut CognacHeap {
    ptr::addr_of_mut!(CognacNtdllProcessHeapObject)
}

#[inline]
fn arena_start() -> ULONG_PTR {
    ptr::addr_of_mut!(PROCESS_HEAP_ARENA.0) as ULONG_PTR
}

#[inline]
fn arena_end() -> ULONG_PTR {
    arena_start() + PROCESS_HEAP_BYTES
}

#[inline]
fn block_address_valid(block: *const CognacHeapBlock) -> bool {
    let address = block as ULONG_PTR;
    address >= arena_start()
        && address <= arena_end().saturating_sub(core::mem::size_of::<CognacHeapBlock>())
}

#[inline]
unsafe fn lock_heap(heap: *mut CognacHeap) {
    // SAFETY: heap points to the static process heap object.
    while unsafe { (*heap).lock.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed) }.is_err() {
        core::hint::spin_loop();
    }
}

#[inline]
unsafe fn unlock_heap(heap: *mut CognacHeap) {
    // SAFETY: heap points to the static process heap object.
    unsafe { (*heap).lock.store(0, Ordering::Release) };
}

unsafe fn initialize_locked(heap: *mut CognacHeap) {
    // SAFETY: Caller holds the heap lock.
    if unsafe { (*heap).magic == HEAP_MAGIC } {
        return;
    }
    let first = arena_start() as *mut CognacHeapBlock;
    // SAFETY: Arena is suitably aligned and large enough for the initial block.
    unsafe {
        ptr::addr_of_mut!((*first).size).write(PROCESS_HEAP_BYTES - core::mem::size_of::<CognacHeapBlock>());
        ptr::addr_of_mut!((*first).previous).write(ptr::null_mut());
        ptr::addr_of_mut!((*first).next).write(ptr::null_mut());
        ptr::addr_of_mut!((*first).magic).write(BLOCK_MAGIC);
        ptr::addr_of_mut!((*first).free).write(1);
        ptr::addr_of_mut!((*heap).first).write(first);
        ptr::addr_of_mut!((*heap).magic).write(HEAP_MAGIC);
    }
}

unsafe fn find_block(address: PVOID) -> *mut CognacHeapBlock {
    let pointer = address as ULONG_PTR;
    if pointer < arena_start() + core::mem::size_of::<CognacHeapBlock>()
        || pointer >= arena_end()
    {
        return ptr::null_mut();
    }
    let block = (pointer - core::mem::size_of::<CognacHeapBlock>()) as *mut CognacHeapBlock;
    if !block_address_valid(block) {
        return ptr::null_mut();
    }
    // SAFETY: Address range was checked before reading the block header.
    unsafe {
        if (*block).magic != BLOCK_MAGIC
            || (*block).size > arena_end().saturating_sub(
                block as ULONG_PTR + core::mem::size_of::<CognacHeapBlock>(),
            )
        {
            return ptr::null_mut();
        }
    }
    block
}

unsafe fn merge_next(block: *mut CognacHeapBlock) {
    // SAFETY: Caller holds heap lock and block is a validated heap block.
    let next = unsafe { (*block).next };
    if next.is_null() || !block_address_valid(next) {
        return;
    }
    if unsafe { (*next).magic != BLOCK_MAGIC || (*next).free == 0 } {
        return;
    }
    unsafe {
        (*block).size += core::mem::size_of::<CognacHeapBlock>() + (*next).size;
        (*block).next = (*next).next;
        if !(*block).next.is_null() {
            (*(*block).next).previous = block;
        }
        (*next).magic = 0;
        (*next).previous = ptr::null_mut();
        (*next).next = ptr::null_mut();
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetProcessHeap() -> HANDLE {
    let heap = heap_ptr();
    // SAFETY: This is the process heap's synchronization and initialization path.
    unsafe {
        lock_heap(heap);
        initialize_locked(heap);
        unlock_heap(heap);
    }
    heap.cast()
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateHeap(
    handle: HANDLE,
    flags: ULONG,
    requested_size: SIZE_T,
) -> PVOID {
    let heap = heap_ptr();
    let mut size = if requested_size == 0 { 1 } else { requested_size };
    if handle != heap.cast()
        || size > PROCESS_HEAP_BYTES - core::mem::size_of::<CognacHeapBlock>()
        || size > SIZE_T::MAX - (ALIGNMENT - 1)
    {
        return ptr::null_mut();
    }
    size = (size + ALIGNMENT - 1) & !(ALIGNMENT - 1);

    // SAFETY: Heap metadata is accessed under its lock.
    unsafe {
        lock_heap(heap);
        initialize_locked(heap);
        let mut block = (*heap).first;
        while !block.is_null() {
            if !block_address_valid(block) || (*block).magic != BLOCK_MAGIC {
                unlock_heap(heap);
                return ptr::null_mut();
            }
            if (*block).free != 0 && (*block).size >= size {
                if (*block).size >= size + core::mem::size_of::<CognacHeapBlock>() + ALIGNMENT {
                    let split = (block.cast::<u8>().add(core::mem::size_of::<CognacHeapBlock>() + size))
                        .cast::<CognacHeapBlock>();
                    (*split).size = (*block).size - size - core::mem::size_of::<CognacHeapBlock>();
                    (*split).previous = block;
                    (*split).next = (*block).next;
                    (*split).magic = BLOCK_MAGIC;
                    (*split).free = 1;
                    if !(*split).next.is_null() {
                        (*(*split).next).previous = split;
                    }
                    (*block).next = split;
                    (*block).size = size;
                }
                (*block).free = 0;
                let payload = block.cast::<u8>().add(core::mem::size_of::<CognacHeapBlock>());
                if flags & HEAP_ZERO_MEMORY != 0 {
                    ptr::write_bytes(payload, 0, (*block).size);
                }
                unlock_heap(heap);
                return payload.cast();
            }
            block = (*block).next;
        }
        unlock_heap(heap);
    }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "system" fn RtlSizeHeap(
    handle: HANDLE,
    _flags: ULONG,
    address: PVOID,
) -> SIZE_T {
    let heap = heap_ptr();
    if handle != heap.cast() || address.is_null() {
        return INVALID_SIZE;
    }
    // SAFETY: The heap lock protects block lookup and metadata reads.
    unsafe {
        lock_heap(heap);
        initialize_locked(heap);
        let block = find_block(address);
        let result = if !block.is_null() && (*block).free == 0 {
            (*block).size
        } else {
            INVALID_SIZE
        };
        unlock_heap(heap);
        result
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeHeap(
    handle: HANDLE,
    _flags: ULONG,
    address: PVOID,
) -> BOOLEAN {
    let heap = heap_ptr();
    if handle != heap.cast() || address.is_null() {
        return 0;
    }
    // SAFETY: The heap lock protects validation, free state and coalescing.
    unsafe {
        lock_heap(heap);
        initialize_locked(heap);
        let mut block = find_block(address);
        if block.is_null() || (*block).free != 0 {
            unlock_heap(heap);
            return 0;
        }
        (*block).free = 1;
        merge_next(block);
        let previous = (*block).previous;
        if !previous.is_null()
            && block_address_valid(previous)
            && (*previous).magic == BLOCK_MAGIC
            && (*previous).free != 0
        {
            block = previous;
            merge_next(block);
        }
        unlock_heap(heap);
    }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlReAllocateHeap(
    handle: HANDLE,
    flags: ULONG,
    address: PVOID,
    size: SIZE_T,
) -> PVOID {
    if address.is_null() {
        // SAFETY: Forwarded arguments preserve the allocation contract.
        return unsafe { RtlAllocateHeap(handle, flags, size) };
    }
    if size == 0 {
        // SAFETY: Reallocation to zero follows this bootstrap C implementation.
        let _ = unsafe { RtlFreeHeap(handle, flags, address) };
        return ptr::null_mut();
    }

    // SAFETY: RtlSizeHeap validates that address is an allocated block.
    let old_size = unsafe { RtlSizeHeap(handle, 0, address) };
    if old_size == INVALID_SIZE {
        return ptr::null_mut();
    }
    // Preserve old bytes even when HEAP_ZERO_MEMORY is requested; zero only the
    // newly added tail, matching the source C implementation.
    let replacement = unsafe { RtlAllocateHeap(handle, flags & !HEAP_ZERO_MEMORY, size) };
    if replacement.is_null() {
        return ptr::null_mut();
    }
    let copy_size = core::cmp::min(old_size, size);
    // SAFETY: Both allocations are valid and non-overlapping arena blocks.
    unsafe {
        ptr::copy_nonoverlapping(address.cast::<u8>(), replacement.cast::<u8>(), copy_size);
        if flags & HEAP_ZERO_MEMORY != 0 && size > copy_size {
            ptr::write_bytes(replacement.cast::<u8>().add(copy_size), 0, size - copy_size);
        }
    }
    if unsafe { RtlFreeHeap(handle, flags, address) } == 0 {
        let _ = unsafe { RtlFreeHeap(handle, 0, replacement) };
        return ptr::null_mut();
    }
    replacement
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidateHeap(
    handle: HANDLE,
    _flags: ULONG,
    address: PVOID,
) -> BOOLEAN {
    let heap = heap_ptr();
    if handle != heap.cast() {
        return 0;
    }
    // SAFETY: The lock stabilizes the linked list during complete validation.
    unsafe {
        lock_heap(heap);
        initialize_locked(heap);
        let mut valid = true;
        if !address.is_null() {
            let block = find_block(address);
            valid = !block.is_null() && (*block).free == 0;
        } else {
            let mut expected = arena_start();
            let mut block = (*heap).first;
            let mut traversed = 0usize;
            while !block.is_null() {
                traversed += 1;
                if traversed > PROCESS_HEAP_BYTES / core::mem::size_of::<CognacHeapBlock>()
                    || !block_address_valid(block)
                    || block as ULONG_PTR != expected
                    || (*block).magic != BLOCK_MAGIC
                    || (*block).size > arena_end().saturating_sub(
                        block as ULONG_PTR + core::mem::size_of::<CognacHeapBlock>(),
                    )
                {
                    valid = false;
                    break;
                }
                expected = block as ULONG_PTR
                    + core::mem::size_of::<CognacHeapBlock>()
                    + (*block).size;
                let next = (*block).next;
                if !next.is_null()
                    && (!block_address_valid(next) || (*next).previous != block)
                {
                    valid = false;
                    break;
                }
                block = next;
            }
            if expected != arena_end() {
                valid = false;
            }
        }
        unlock_heap(heap);
        valid as BOOLEAN
    }
}


const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_NO_MORE_ENTRIES: i32 = 0x8000_001Au32 as i32;

/// Matches the caller-facing x64 layout used by Wine's undocumented RtlWalkHeap
/// entry. The union is represented as its largest member to preserve its ABI size.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RTL_HEAP_WALK_ENTRY {
    pub lpData: PVOID,
    pub cbData: SIZE_T,
    pub cbOverhead: u8,
    pub iRegionIndex: u8,
    pub wFlags: u16,
    pub reserved: [usize; 4],
}

/// Compact the bootstrap arena by coalescing adjacent free blocks and return the
/// largest free payload size. The fixed arena cannot release pages back to the OS.
#[no_mangle]
pub unsafe extern "system" fn RtlCompactHeap(handle: HANDLE, _flags: ULONG) -> SIZE_T {
    let heap = heap_ptr();
    if handle != heap.cast() {
        return 0;
    }
    unsafe {
        lock_heap(heap);
        initialize_locked(heap);
        let mut block = (*heap).first;
        let mut largest = 0usize;
        while !block.is_null() {
            if !block_address_valid(block) || (*block).magic != BLOCK_MAGIC {
                unlock_heap(heap);
                return 0;
            }
            if (*block).free != 0 {
                merge_next(block);
                largest = largest.max((*block).size);
            }
            block = (*block).next;
        }
        unlock_heap(heap);
        largest
    }
}

/// The allocator already serializes every operation internally. These entry points
/// validate the handle but intentionally do not hold the non-recursive spinlock
/// across calls, which would deadlock allocations made by the caller while locked.
#[no_mangle]
pub unsafe extern "system" fn RtlLockHeap(handle: HANDLE) -> BOOLEAN {
    (handle == heap_ptr().cast()) as BOOLEAN
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockHeap(handle: HANDLE) -> BOOLEAN {
    (handle == heap_ptr().cast()) as BOOLEAN
}

/// Validate the sole process heap. A process with additional heaps requires the
/// process heap registry and per-heap validation to be expanded together.
#[no_mangle]
pub unsafe extern "system" fn RtlValidateProcessHeaps() -> ULONG {
    unsafe { RtlValidateHeap(heap_ptr().cast(), 0, ptr::null_mut()) as ULONG }
}

/// Enumerate the fixed arena's blocks using the undocumented RtlWalkHeap entry
/// contract. The caller initializes lpData to null and passes the returned lpData
/// back to continue. Region metadata and large-allocation records are not present
/// in this bootstrap allocator.
#[no_mangle]
pub unsafe extern "system" fn RtlWalkHeap(
    handle: HANDLE,
    entry_ptr: PVOID,
) -> NTSTATUS {
    if handle != heap_ptr().cast() {
        return STATUS_INVALID_HANDLE;
    }
    if entry_ptr.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let entry = entry_ptr.cast::<RTL_HEAP_WALK_ENTRY>();
    unsafe {
        lock_heap(heap_ptr());
        initialize_locked(heap_ptr());
        let requested = (*entry).lpData as ULONG_PTR;
        let mut block = (*heap_ptr()).first;
        if requested != 0 {
            let mut found = false;
            while !block.is_null() {
                if !block_address_valid(block) || (*block).magic != BLOCK_MAGIC {
                    unlock_heap(heap_ptr());
                    return STATUS_INVALID_PARAMETER;
                }
                let payload = block.cast::<u8>().add(core::mem::size_of::<CognacHeapBlock>());
                if payload as ULONG_PTR == requested {
                    block = (*block).next;
                    found = true;
                    break;
                }
                block = (*block).next;
            }
            if !found {
                unlock_heap(heap_ptr());
                return STATUS_INVALID_PARAMETER;
            }
        }
        if block.is_null() {
            (*entry).lpData = ptr::null_mut();
            (*entry).cbData = 0;
            (*entry).cbOverhead = 0;
            (*entry).iRegionIndex = 0;
            (*entry).wFlags = 0;
            unlock_heap(heap_ptr());
            return STATUS_NO_MORE_ENTRIES;
        }
        if !block_address_valid(block) || (*block).magic != BLOCK_MAGIC {
            unlock_heap(heap_ptr());
            return STATUS_INVALID_PARAMETER;
        }
        (*entry).lpData = block.cast::<u8>()
            .add(core::mem::size_of::<CognacHeapBlock>()).cast();
        (*entry).cbData = (*block).size;
        (*entry).cbOverhead = core::mem::size_of::<CognacHeapBlock>() as u8;
        (*entry).iRegionIndex = 0;
        (*entry).wFlags = if (*block).free == 0 { 0x4000 | 0x0010 | 0x0001 } else { 0 };
        unlock_heap(heap_ptr());
    }
    STATUS_SUCCESS
}

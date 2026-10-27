//! CognacOS native heap manager (Windows NT RTL ABI).
//!
//! This is an in-tree implementation of the public NT heap contracts, not a Wine
//! source translation. It uses the native virtual-memory system calls for heap
//! objects and independently growing segments. It intentionally does not claim
//! binary equivalence with private Windows 11 heap internals (LFH/segment-heap
//! metadata is undocumented and build-dependent).
//!
//! Safety: exported functions implement NT ABI contracts and therefore require
//! valid caller buffers. User pointers are checked against known segment ranges
//! before heap metadata is accessed. Native VM calls must be available in ntdll.

#![allow(non_camel_case_types, non_snake_case, static_mut_refs)]

use core::{ffi::c_void, mem, ptr};
use core::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

pub type ULONG = u32;
pub type SIZE_T = usize;
pub type BOOLEAN = u8;
pub type HANDLE = *mut c_void;
pub type PVOID = *mut c_void;
pub type NTSTATUS = i32;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
const STATUS_NO_MORE_ENTRIES: NTSTATUS = 0x8000_001Au32 as i32;
const HEAP_MAGIC: u64 = 0x434F_474E_4143_4848;
const BLOCK_MAGIC: u64 = 0x434F_474E_4143_4842;
const SEGMENT_MAGIC: u64 = 0x434F_474E_4143_4853;
const PAGE_SIZE: usize = 4096;
const DEFAULT_RESERVE: usize = 1024 * 1024;
const ALIGN: usize = 16;
const MEM_COMMIT_RESERVE: ULONG = 0x3000;
const MEM_RELEASE: ULONG = 0x8000;
const HEAP_GROWABLE: ULONG = 0x0000_0002;
const HEAP_GENERATE_EXCEPTIONS: ULONG = 0x0000_0004;
pub const HEAP_ZERO_MEMORY: ULONG = 0x0000_0008;
const HEAP_NO_SERIALIZE: ULONG = 0x0000_0001;
const HEAP_REALLOC_IN_PLACE_ONLY: ULONG = 0x0000_0010;
const HEAP_DISABLE_COALESCE_ON_FREE: ULONG = 0x0000_0200;
const HEAP_SETTABLE_USER_VALUE: ULONG = 0x0000_0100;
const HEAP_SETTABLE_USER_FLAG1: ULONG = 0x0000_0200;
const HEAP_SETTABLE_USER_FLAG2: ULONG = 0x0000_0400;
const HEAP_SETTABLE_USER_FLAG3: ULONG = 0x0000_0800;
const HEAP_USER_FLAGS: ULONG = HEAP_SETTABLE_USER_FLAG1 | HEAP_SETTABLE_USER_FLAG2 | HEAP_SETTABLE_USER_FLAG3;
const HEAP_WALK_BUSY: u16 = 0x0001;
const HEAP_WALK_REGION: u16 = 0x0002;
const HEAP_WALK_UNCOMMITTED: u16 = 0x0004;

extern "system" {
    fn NtAllocateVirtualMemory(
        process: HANDLE, base: *mut PVOID, zero_bits: usize,
        region_size: *mut SIZE_T, allocation_type: ULONG, protect: ULONG,
    ) -> NTSTATUS;
    fn NtFreeVirtualMemory(
        process: HANDLE, base: *mut PVOID, region_size: *mut SIZE_T, free_type: ULONG,
    ) -> NTSTATUS;
}

#[repr(C)]
struct Block {
    magic: u64,
    size: SIZE_T,
    previous: *mut Block,
    next: *mut Block,
    user_value: PVOID,
    user_flags: ULONG,
    free: ULONG,
}
#[repr(C)]
struct Segment {
    magic: u64,
    base: PVOID,
    size: SIZE_T,
    next: *mut Segment,
    first: *mut Block,
}
#[repr(C)]
pub struct CognacHeap {
    magic: u64,
    flags: ULONG,
    reserve_size: SIZE_T,
    commit_size: SIZE_T,
    segments: *mut Segment,
    next_heap: *mut CognacHeap,
    lock: AtomicU8,
    protected: AtomicU8,
    _padding: [u8; 6],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RTL_HEAP_WALK_ENTRY {
    pub lpData: PVOID,
    pub cbData: SIZE_T,
    pub cbOverhead: u8,
    pub iRegionIndex: u8,
    pub wFlags: u16,
    pub reserved: [usize; 3],
}

static PROCESS_HEAP: AtomicPtr<CognacHeap> = AtomicPtr::new(ptr::null_mut());
static PROCESS_HEAP_INIT_LOCK: AtomicU8 = AtomicU8::new(0);

/// Exported data object referenced by PEB.ProcessHeap. It is the actual heap
/// object, not a pointer slot, and is initialized in place on first use.
static mut CognacNtdllProcessHeapObject: CognacHeap = CognacHeap {
    magic: 0, flags: HEAP_GROWABLE, reserve_size: DEFAULT_RESERVE, commit_size: PAGE_SIZE,
    segments: ptr::null_mut(), next_heap: ptr::null_mut(),
    lock: AtomicU8::new(0), protected: AtomicU8::new(0), _padding: [0; 6],
};
static HEAP_LIST_LOCK: AtomicU8 = AtomicU8::new(0);
static mut HEAP_LIST: *mut CognacHeap = ptr::null_mut();

#[inline] fn align_up(n: usize, a: usize) -> Option<usize> { n.checked_add(a - 1).map(|v| v & !(a - 1)) }
#[inline] fn page_up(n: usize) -> Option<usize> { align_up(n, PAGE_SIZE) }
#[inline] fn current_process() -> HANDLE { (-1isize) as HANDLE }
#[inline] unsafe fn lock(flag: &AtomicU8) {
    while flag.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
#[inline] unsafe fn unlock(flag: &AtomicU8) { flag.store(0, Ordering::Release); }

unsafe fn vm_alloc(size: usize) -> PVOID {
    let mut base: PVOID = ptr::null_mut();
    let mut bytes = size;
    // SAFETY: Native VM ABI; the kernel validates the request and returns a mapped region.
    let status = unsafe { NtAllocateVirtualMemory(current_process(), &mut base, 0, &mut bytes, MEM_COMMIT_RESERVE, 0x04) };
    if status < 0 { ptr::null_mut() } else { base }
}
unsafe fn vm_free(base: PVOID) {
    if base.is_null() { return; }
    let mut address = base;
    let mut bytes = 0usize;
    // MEM_RELEASE requires RegionSize == 0.
    let _ = unsafe { NtFreeVirtualMemory(current_process(), &mut address, &mut bytes, MEM_RELEASE) };
}
unsafe fn segment_create(heap: *mut CognacHeap, minimum_payload: usize) -> *mut Segment {
    let block_offset = match align_up(mem::size_of::<Segment>(), ALIGN) { Some(v) => v, None => return ptr::null_mut() };
    let overhead = block_offset + mem::size_of::<Block>();
    let configured = if unsafe { (*heap).segments.is_null() } {
        unsafe { (*heap).reserve_size }
    } else {
        unsafe { (*heap).commit_size.max(PAGE_SIZE) }
    };
    let want = match page_up(minimum_payload.saturating_add(overhead).max(configured.max(PAGE_SIZE))) {
        Some(v) => v, None => return ptr::null_mut(),
    };
    let region = unsafe { vm_alloc(want) };
    if region.is_null() { return ptr::null_mut(); }
    let seg = region.cast::<Segment>();
    let block = unsafe { region.cast::<u8>().add(block_offset).cast::<Block>() };
    let available = want - block_offset - mem::size_of::<Block>();
    unsafe {
        ptr::write(seg, Segment { magic: SEGMENT_MAGIC, base: region, size: want, next: (*heap).segments, first: block });
        ptr::write(block, Block { magic: BLOCK_MAGIC, size: available & !(ALIGN - 1), previous: ptr::null_mut(), next: ptr::null_mut(), user_value: ptr::null_mut(), user_flags: 0, free: 1 });
        (*heap).segments = seg;
    }
    seg
}
unsafe fn heap_valid(handle: HANDLE) -> *mut CognacHeap {
    // Do not dereference a caller-supplied handle until it matches an entry in
    // the process-owned registry. Random and stale unmapped handles fail closed.
    if handle.is_null() { return ptr::null_mut(); }
    let wanted = handle.cast::<CognacHeap>();
    unsafe { lock(&HEAP_LIST_LOCK); }
    let mut current = unsafe { HEAP_LIST };
    let mut found = ptr::null_mut();
    let mut seen = 0usize;
    while !current.is_null() && seen < 1_000_000 {
        if current == wanted { found = current; break; }
        current = unsafe { (*current).next_heap };
        seen += 1;
    }
    if !found.is_null() && unsafe { (*found).magic } != HEAP_MAGIC { found = ptr::null_mut(); }
    unsafe { unlock(&HEAP_LIST_LOCK); }
    found
}
unsafe fn block_payload(block: *mut Block) -> PVOID {
    unsafe { block.cast::<u8>().add(mem::size_of::<Block>()).cast() }
}
unsafe fn block_from_payload(heap: *mut CognacHeap, payload: PVOID) -> *mut Block {
    if payload.is_null() { return ptr::null_mut(); }
    let p = payload as usize;
    let mut seg = unsafe { (*heap).segments };
    while !seg.is_null() {
        let base = unsafe { (*seg).base as usize };
        let Some(end) = base.checked_add(unsafe { (*seg).size }) else { return ptr::null_mut(); };
        if p >= base.saturating_add(mem::size_of::<Segment>() + mem::size_of::<Block>())
            && p < end && unsafe { (*seg).magic } == SEGMENT_MAGIC
        {
            let b = (p - mem::size_of::<Block>()) as *mut Block;
            if unsafe { (*b).magic } == BLOCK_MAGIC
                && (b as usize) >= base + align_up(mem::size_of::<Segment>(), ALIGN).unwrap_or(mem::size_of::<Segment>())
                && unsafe { (*b).size } <= end.saturating_sub(b as usize + mem::size_of::<Block>())
                && unsafe { block_payload(b) } == payload
            { return b; }
            return ptr::null_mut();
        }
        seg = unsafe { (*seg).next };
    }
    ptr::null_mut()
}
unsafe fn merge_next(block: *mut Block) {
    let next = unsafe { (*block).next };
    if next.is_null() || unsafe { (*next).magic } != BLOCK_MAGIC || unsafe { (*next).free } == 0 { return; }
    unsafe {
        (*block).size = (*block).size.saturating_add(mem::size_of::<Block>() + (*next).size);
        (*block).next = (*next).next;
        if !(*block).next.is_null() { (*(*block).next).previous = block; }
        (*next).magic = 0;
    }
}
unsafe fn create_heap(flags: ULONG, reserve: SIZE_T, commit: SIZE_T) -> *mut CognacHeap {
    let object_mem = unsafe { vm_alloc(page_up(mem::size_of::<CognacHeap>()).unwrap_or(PAGE_SIZE)) };
    if object_mem.is_null() { return ptr::null_mut(); }
    let heap = object_mem.cast::<CognacHeap>();
    let reserve_size = if reserve == 0 { DEFAULT_RESERVE } else { reserve };
    let commit_size = if commit == 0 { PAGE_SIZE } else { commit.min(reserve_size) };
    unsafe {
        ptr::write(heap, CognacHeap { magic: HEAP_MAGIC, flags, reserve_size, commit_size, segments: ptr::null_mut(), next_heap: ptr::null_mut(), lock: AtomicU8::new(0), protected: AtomicU8::new(0), _padding: [0; 6] });
        if segment_create(heap, 1).is_null() {
            (*heap).magic = 0;
            vm_free(object_mem);
            return ptr::null_mut();
        }
        lock(&HEAP_LIST_LOCK);
        (*heap).next_heap = HEAP_LIST;
        HEAP_LIST = heap;
        unlock(&HEAP_LIST_LOCK);
    }
    heap
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateHeap(flags: ULONG, base: PVOID, reserve: SIZE_T, commit: SIZE_T, _lock_ptr: PVOID, _parameters: PVOID) -> HANDLE {
    // Caller-provided HeapBase and external resource locks are not supported yet.
    // A zero Flags value is a valid non-growable heap. Caller-provided bases
    // and custom lock/parameter structures remain unsupported by this manager.
    let effective_reserve = if reserve == 0 { DEFAULT_RESERVE } else { reserve };
    if !base.is_null() || commit > effective_reserve || effective_reserve > isize::MAX as usize || commit > isize::MAX as usize {
        return ptr::null_mut();
    }
    unsafe { create_heap(flags, reserve, commit).cast() }
}

unsafe fn initialize_process_heap_object() -> *mut CognacHeap {
    let heap = ptr::addr_of_mut!(CognacNtdllProcessHeapObject);
    unsafe { lock(&PROCESS_HEAP_INIT_LOCK); }
    if unsafe { (*heap).magic != HEAP_MAGIC } {
        unsafe {
            ptr::write(heap, CognacHeap {
                magic: HEAP_MAGIC, flags: HEAP_GROWABLE, reserve_size: DEFAULT_RESERVE,
                commit_size: PAGE_SIZE, segments: ptr::null_mut(), next_heap: ptr::null_mut(),
                lock: AtomicU8::new(0), protected: AtomicU8::new(0), _padding: [0; 6],
            });
            if segment_create(heap, 1).is_null() {
                (*heap).magic = 0;
                unlock(&PROCESS_HEAP_INIT_LOCK);
                return ptr::null_mut();
            }
            lock(&HEAP_LIST_LOCK);
            (*heap).next_heap = HEAP_LIST;
            HEAP_LIST = heap;
            unlock(&HEAP_LIST_LOCK);
        }
    }
    unsafe { unlock(&PROCESS_HEAP_INIT_LOCK); }
    heap
}

pub unsafe extern "system" fn RtlGetProcessHeap() -> HANDLE {
    let existing = PROCESS_HEAP.load(Ordering::Acquire);
    let heap = if !existing.is_null() {
        existing
    } else {
        let created = unsafe { initialize_process_heap_object() };
        if created.is_null() { return ptr::null_mut(); }
        match PROCESS_HEAP.compare_exchange(ptr::null_mut(), created, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => created,
            Err(winner) => winner,
        }
    };
    // Match Windows NTDLL: publish the actual default heap in PEB.ProcessHeap.
    // The kernel creates the PEB in the current user address space; never write
    // through a null or non-user GS base.
    #[cfg(target_os = "windows")]
    unsafe {
        let peb: usize;
        core::arch::asm!("mov {}, gs:[0x60]", out(reg) peb, options(nostack, preserves_flags));
        if peb != 0 && peb < 0x0000_8000_0000_0000usize {
            (peb as *mut u8).add(0x30).cast::<HANDLE>().write(heap.cast());
        }
    }
    heap.cast()
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyHeap(handle: HANDLE) -> HANDLE {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() { return handle; }
    unsafe {
        lock(&HEAP_LIST_LOCK);
        let mut link: *mut *mut CognacHeap = ptr::addr_of_mut!(HEAP_LIST);
        while !(*link).is_null() && *link != heap { link = ptr::addr_of_mut!((**link).next_heap); }
        if !(*link).is_null() { *link = (*heap).next_heap; }
        unlock(&HEAP_LIST_LOCK);
        if PROCESS_HEAP.load(Ordering::Acquire) == heap { PROCESS_HEAP.store(ptr::null_mut(), Ordering::Release); }
        let mut seg = (*heap).segments;
        while !seg.is_null() {
            let next = (*seg).next;
            let region = (*seg).base;
            (*seg).magic = 0;
            vm_free(region);
            seg = next;
        }
        (*heap).magic = 0;
        (*heap).segments = ptr::null_mut();
        (*heap).next_heap = ptr::null_mut();
        if heap != ptr::addr_of_mut!(CognacNtdllProcessHeapObject) {
            vm_free(heap.cast());
        }
    }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateHeap(handle: HANDLE, flags: ULONG, requested: SIZE_T) -> PVOID {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() || requested > isize::MAX as usize || unsafe { (*heap).protected.load(Ordering::Acquire) != 0 } { return ptr::null_mut(); }
    let size = match align_up(requested.max(1), ALIGN) { Some(v) => v, None => return ptr::null_mut() };
    unsafe {
        lock(&(*heap).lock);
        let mut seg = (*heap).segments;
        loop {
            while !seg.is_null() {
                let mut b = (*seg).first;
                while !b.is_null() {
                    if (*b).magic != BLOCK_MAGIC { unlock(&(*heap).lock); return ptr::null_mut(); }
                    if (*b).free != 0 && (*b).size >= size {
                        if (*b).size >= size.saturating_add(mem::size_of::<Block>() + ALIGN) {
                            let split = b.cast::<u8>().add(mem::size_of::<Block>() + size).cast::<Block>();
                            ptr::write(split, Block { magic: BLOCK_MAGIC, size: (*b).size - size - mem::size_of::<Block>(), previous: b, next: (*b).next, user_value: ptr::null_mut(), user_flags: 0, free: 1 });
                            if !(*split).next.is_null() { (*(*split).next).previous = split; }
                            (*b).next = split;
                            (*b).size = size;
                        }
                        (*b).free = 0;
                        (*b).user_value = ptr::null_mut();
                        (*b).user_flags = 0;
                        let out = block_payload(b);
                        if (flags & HEAP_ZERO_MEMORY) != 0 { ptr::write_bytes(out.cast::<u8>(), 0, (*b).size); }
                        unlock(&(*heap).lock);
                        return out;
                    }
                    b = (*b).next;
                }
                seg = (*seg).next;
            }
            if (*heap).flags & HEAP_GROWABLE == 0 { unlock(&(*heap).lock); return ptr::null_mut(); }
            seg = segment_create(heap, size);
            if seg.is_null() { unlock(&(*heap).lock); return ptr::null_mut(); }
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeHeap(handle: HANDLE, flags: ULONG, address: PVOID) -> BOOLEAN {
    if address.is_null() { return 1; }
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() || unsafe { (*heap).protected.load(Ordering::Acquire) != 0 } { return 0; }
    unsafe {
        lock(&(*heap).lock);
        let mut b = block_from_payload(heap, address);
        if b.is_null() || (*b).free != 0 { unlock(&(*heap).lock); return 0; }
        (*b).free = 1;
        (*b).user_value = ptr::null_mut();
        (*b).user_flags = 0;
        if flags & HEAP_DISABLE_COALESCE_ON_FREE == 0 {
            merge_next(b);
            if !(*b).previous.is_null() && (*(*b).previous).free != 0 {
                b = (*b).previous;
                merge_next(b);
            }
        }
        unlock(&(*heap).lock);
    }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlSizeHeap(handle: HANDLE, _flags: ULONG, address: PVOID) -> SIZE_T {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() { return SIZE_T::MAX; }
    unsafe {
        lock(&(*heap).lock);
        let b = block_from_payload(heap, address);
        let size = if !b.is_null() && (*b).free == 0 { (*b).size } else { SIZE_T::MAX };
        unlock(&(*heap).lock);
        size
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlReAllocateHeap(handle: HANDLE, flags: ULONG, address: PVOID, size: SIZE_T) -> PVOID {
    if address.is_null() { return unsafe { RtlAllocateHeap(handle, flags, size) }; }
    if size == 0 { let _ = unsafe { RtlFreeHeap(handle, flags, address) }; return ptr::null_mut(); }
    let old_size = unsafe { RtlSizeHeap(handle, 0, address) };
    if old_size == SIZE_T::MAX { return ptr::null_mut(); }
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() || unsafe { (*heap).protected.load(Ordering::Acquire) != 0 } { return ptr::null_mut(); }
    let wanted = match align_up(size, ALIGN) { Some(v) => v, None => return ptr::null_mut() };
    unsafe {
        lock(&(*heap).lock);
        let b = block_from_payload(heap, address);
        if b.is_null() || (*b).free != 0 { unlock(&(*heap).lock); return ptr::null_mut(); }
        if (*b).size >= wanted {
            if wanted.checked_add(mem::size_of::<Block>() + ALIGN).map_or(false, |minimum| (*b).size >= minimum) {
                let split = b.cast::<u8>().add(mem::size_of::<Block>() + wanted).cast::<Block>();
                ptr::write(split, Block { magic: BLOCK_MAGIC, size: (*b).size - wanted - mem::size_of::<Block>(), previous: b, next: (*b).next, user_value: ptr::null_mut(), user_flags: 0, free: 1 });
                if !(*split).next.is_null() { (*(*split).next).previous = split; }
                (*b).next = split; (*b).size = wanted;
            }
            if flags & HEAP_ZERO_MEMORY != 0 && size > old_size { ptr::write_bytes(address.cast::<u8>().add(old_size), 0, size - old_size); }
            unlock(&(*heap).lock);
            return address;
        }
        if !(*b).next.is_null() && (*(*b).next).free != 0 && (*b).size + mem::size_of::<Block>() + (*(*b).next).size >= wanted {
            merge_next(b);
            if (*b).size >= wanted {
                if flags & HEAP_ZERO_MEMORY != 0 && size > old_size { ptr::write_bytes(address.cast::<u8>().add(old_size), 0, size - old_size); }
                unlock(&(*heap).lock);
                return address;
            }
        }
        unlock(&(*heap).lock);
    }
    if flags & HEAP_REALLOC_IN_PLACE_ONLY != 0 { return ptr::null_mut(); }
    let new_ptr = unsafe { RtlAllocateHeap(handle, flags & !HEAP_ZERO_MEMORY, size) };
    if new_ptr.is_null() { return ptr::null_mut(); }
    unsafe {
        ptr::copy_nonoverlapping(address.cast::<u8>(), new_ptr.cast::<u8>(), old_size.min(size));
        if flags & HEAP_ZERO_MEMORY != 0 && size > old_size { ptr::write_bytes(new_ptr.cast::<u8>().add(old_size), 0, size - old_size); }
        if RtlFreeHeap(handle, 0, address) == 0 { let _ = RtlFreeHeap(handle, 0, new_ptr); return ptr::null_mut(); }
    }
    new_ptr
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidateHeap(handle: HANDLE, _flags: ULONG, address: PVOID) -> BOOLEAN {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() { return 0; }
    unsafe {
        lock(&(*heap).lock);
        let mut seg = (*heap).segments;
        let mut valid = true;
        let mut found = address.is_null();
        while !seg.is_null() {
            if (*seg).magic != SEGMENT_MAGIC { valid = false; break; }
            let mut expected = (*seg).base as usize + align_up(mem::size_of::<Segment>(), ALIGN).unwrap_or(mem::size_of::<Segment>());
            let end = (*seg).base as usize + (*seg).size;
            let mut b = (*seg).first;
            let mut count = 0usize;
            while !b.is_null() {
                count += 1;
                if count > (*seg).size / mem::size_of::<Block>() || b as usize != expected || (*b).magic != BLOCK_MAGIC || (*b).size > end.saturating_sub(b as usize + mem::size_of::<Block>()) {
                    valid = false; break;
                }
                if !address.is_null() && block_payload(b) == address && (*b).free == 0 { found = true; }
                expected = b as usize + mem::size_of::<Block>() + (*b).size;
                if !(*b).next.is_null() && (*(*b).next).previous != b { valid = false; break; }
                b = (*b).next;
            }
            if expected != end { valid = false; }
            if !valid { break; }
            seg = (*seg).next;
        }
        unlock(&(*heap).lock);
        (valid && found) as BOOLEAN
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetProcessHeaps(count: ULONG, out: *mut HANDLE) -> ULONG {
    unsafe {
        let _ = RtlGetProcessHeap();
        lock(&HEAP_LIST_LOCK);
        let mut total = 0usize;
        let mut heap = HEAP_LIST;
        while !heap.is_null() {
            if total < count as usize && !out.is_null() { ptr::write(out.add(total), heap.cast()); }
            total += 1;
            heap = (*heap).next_heap;
        }
        unlock(&HEAP_LIST_LOCK);
        total.min(ULONG::MAX as usize) as ULONG
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidateProcessHeaps() -> ULONG {
    // Do not hold the registry lock while RtlValidateHeap rechecks membership.
    let mut total = 0u32;
    let mut index = 0usize;
    loop {
        unsafe { lock(&HEAP_LIST_LOCK); }
        let mut heap = unsafe { HEAP_LIST };
        let mut i = 0usize;
        while !heap.is_null() && i < index {
            heap = unsafe { (*heap).next_heap };
            i += 1;
        }
        unsafe { unlock(&HEAP_LIST_LOCK); }
        if heap.is_null() { break; }
        if unsafe { RtlValidateHeap(heap.cast(), 0, ptr::null_mut()) } == 0 { return 0; }
        total = total.saturating_add(1);
        index += 1;
        if index > 1_000_000 { return 0; }
    }
    total
}

#[no_mangle]
pub unsafe extern "system" fn RtlCompactHeap(handle: HANDLE, _flags: ULONG) -> SIZE_T {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() { return 0; }
    unsafe {
        lock(&(*heap).lock);
        let mut seg = (*heap).segments;
        let mut largest = 0usize;
        while !seg.is_null() {
            let mut b = (*seg).first;
            while !b.is_null() {
                if (*b).free != 0 {
                    merge_next(b);
                    largest = largest.max((*b).size);
                }
                b = (*b).next;
            }
            seg = (*seg).next;
        }
        unlock(&(*heap).lock);
        largest
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetUserValueHeap(handle: HANDLE, _flags: ULONG, address: PVOID, value: PVOID) -> BOOLEAN {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() || unsafe { (*heap).flags & HEAP_SETTABLE_USER_VALUE == 0 } { return 0; }
    unsafe {
        lock(&(*heap).lock);
        let b = block_from_payload(heap, address);
        let ok = !b.is_null() && (*b).free == 0;
        if ok { (*b).user_value = value; }
        unlock(&(*heap).lock);
        ok as BOOLEAN
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlGetUserInfoHeap(handle: HANDLE, _flags: ULONG, address: PVOID, user_value: *mut PVOID, user_flags: *mut ULONG) -> BOOLEAN {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() { return 0; }
    unsafe {
        lock(&(*heap).lock);
        let b = block_from_payload(heap, address);
        let ok = !b.is_null() && (*b).free == 0;
        if ok {
            if !user_value.is_null() { ptr::write(user_value, (*b).user_value); }
            if !user_flags.is_null() { ptr::write(user_flags, (*b).user_flags); }
        }
        unlock(&(*heap).lock);
        ok as BOOLEAN
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlSetUserFlagsHeap(handle: HANDLE, _flags: ULONG, address: PVOID, mask: ULONG, values: ULONG) -> BOOLEAN {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() || unsafe { (*heap).flags & HEAP_USER_FLAGS == 0 } { return 0; }
    unsafe {
        lock(&(*heap).lock);
        let b = block_from_payload(heap, address);
        let ok = !b.is_null() && (*b).free == 0;
        if ok { (*b).user_flags = ((*b).user_flags & !mask) | (values & mask & HEAP_USER_FLAGS); }
        unlock(&(*heap).lock);
        ok as BOOLEAN
    }
}

/// Internal bridge used by the Windows-compatible RtlProtectHeap export.
pub unsafe extern "system" fn RtlSetHeapProtected(handle: HANDLE, protect: BOOLEAN) -> BOOLEAN {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() { return 0; }
    unsafe { (*heap).protected.store((protect != 0) as u8, Ordering::Release); }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockHeap(handle: HANDLE) -> BOOLEAN {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() { return 0; }
    // A permanent lock would deadlock same-thread heap calls. Windows exposes
    // recursive locking; until thread-id-aware recursion exists, only validate.
    1
}
#[no_mangle]
pub unsafe extern "system" fn RtlUnlockHeap(handle: HANDLE) -> BOOLEAN {
    (!unsafe { heap_valid(handle) }.is_null()) as BOOLEAN
}

#[no_mangle]
pub unsafe extern "system" fn RtlWalkHeap(handle: HANDLE, entry_ptr: PVOID) -> NTSTATUS {
    let heap = unsafe { heap_valid(handle) };
    if heap.is_null() { return STATUS_INVALID_HANDLE; }
    if entry_ptr.is_null() { return STATUS_INVALID_PARAMETER; }
    let entry = entry_ptr.cast::<RTL_HEAP_WALK_ENTRY>();
    unsafe {
        lock(&(*heap).lock);
        let requested = (*entry).lpData;
        let mut seg = (*heap).segments;
        let mut candidate: *mut Block = ptr::null_mut();
        if requested.is_null() {
            if !seg.is_null() { candidate = (*seg).first; }
        } else {
            let mut found = false;
            while !seg.is_null() && !found {
                let mut b = (*seg).first;
                while !b.is_null() {
                    if block_payload(b) == requested { candidate = (*b).next; found = true; break; }
                    b = (*b).next;
                }
                if !found { seg = (*seg).next; }
            }
            if !found { unlock(&(*heap).lock); return STATUS_INVALID_PARAMETER; }
            if candidate.is_null() {
                // Advance to first block of next segment after the current segment.
                let mut s = (*heap).segments;
                while !s.is_null() {
                    let mut b = (*s).first;
                    while !b.is_null() {
                        if block_payload(b) == requested { s = (*s).next; break; }
                        b = (*b).next;
                    }
                    if b.is_null() { s = (*s).next; } else { break; }
                }
                if !s.is_null() { candidate = (*s).first; }
            }
        }
        if candidate.is_null() {
            (*entry).lpData = ptr::null_mut(); (*entry).cbData = 0; (*entry).cbOverhead = 0; (*entry).iRegionIndex = 0; (*entry).wFlags = 0;
            unlock(&(*heap).lock);
            return STATUS_NO_MORE_ENTRIES;
        }
        (*entry).lpData = block_payload(candidate);
        (*entry).cbData = (*candidate).size;
        (*entry).cbOverhead = mem::size_of::<Block>() as u8;
        (*entry).iRegionIndex = 0;
        (*entry).wFlags = if (*candidate).free == 0 { HEAP_WALK_BUSY } else { 0 };
        unlock(&(*heap).lock);
    }
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn abi_walk_entry_is_x64_sized() {
        assert_eq!(mem::size_of::<RTL_HEAP_WALK_ENTRY>(), 48);
        assert_eq!(mem::offset_of!(RTL_HEAP_WALK_ENTRY, lpData), 0);
        assert_eq!(mem::offset_of!(RTL_HEAP_WALK_ENTRY, cbData), 8);
        assert_eq!(mem::offset_of!(RTL_HEAP_WALK_ENTRY, wFlags), 18);
        assert_eq!(mem::offset_of!(RTL_HEAP_WALK_ENTRY, reserved), 24);
    }
    #[test] fn constants_match_native_heap_contract() {
        assert_eq!(HEAP_ZERO_MEMORY, 8);
        assert_eq!(HEAP_NO_SERIALIZE, 1);
        assert_eq!(HEAP_GROWABLE, 2);
        assert_eq!(STATUS_NO_MEMORY as u32, 0xC000_0017);
        assert_eq!(STATUS_INVALID_HANDLE as u32, 0xC000_0008);
    }
    #[test] fn arbitrary_handles_are_rejected_before_dereference() {
        unsafe {
            assert!(heap_valid(ptr::null_mut()).is_null());
            assert!(heap_valid(1usize as HANDLE).is_null());
            assert!(heap_valid(usize::MAX as HANDLE).is_null());
        }
    }
}

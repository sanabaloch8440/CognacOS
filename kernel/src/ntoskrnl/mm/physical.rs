//! Early physical-page allocator built only from UEFI EfiConventionalMemory ranges.
//! Mutations are serialized with an interrupt-safe lock on the kernel target.
//! Page-table frames stay below 4 GiB; data frames may use high RAM through the direct map.

use core::ptr;

const PAGE_SIZE: u64 = 4096;
const LOW_RESERVED_END: u64 = 0x10_0000;
const PHYSICAL_LIMIT: u64 = 0x1_0000_0000;
const EFI_CONVENTIONAL_MEMORY: u32 = 7;
const MAX_REGIONS: usize = 256;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryDescriptor {
    pub kind: u32,
    _padding: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attributes: u64,
}

#[derive(Clone, Copy)]
struct Region {
    next: u64,
    end: u64,
}
impl Region {
    const EMPTY: Self = Self { next: 0, end: 0 };
}

struct RegionStore(core::cell::UnsafeCell<[Region; MAX_REGIONS]>);
unsafe impl Sync for RegionStore {}
static REGIONS: RegionStore = RegionStore(core::cell::UnsafeCell::new([Region::EMPTY; MAX_REGIONS]));
static REGION_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static HIGH_REGIONS: RegionStore = RegionStore(core::cell::UnsafeCell::new([Region::EMPTY; MAX_REGIONS]));
static HIGH_REGION_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
// The immutable final UEFI map is retained for validating high-memory direct-map faults.
static FIRMWARE_MAP: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static FIRMWARE_MAP_ENTRIES: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static FIRMWARE_MAP_STRIDE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
struct DirectMapReadOnlyRanges(core::cell::UnsafeCell<[(u64, u64); 2]>);
unsafe impl Sync for DirectMapReadOnlyRanges {}
static DIRECT_MAP_READ_ONLY: DirectMapReadOnlyRanges =
    DirectMapReadOnlyRanges(core::cell::UnsafeCell::new([(0, 0); 2]));
static DIRECT_MAP_PROTECTIONS_READY: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

pub const DIRECT_MAP_BASE: u64 = 0xffff_9000_0000_0000;
/// Highest physical address representable in this canonical direct-map window.
pub const DIRECT_MAP_PHYSICAL_LIMIT: u64 = 0x0000_6f00_0000_0000;

/// Return true only for physical pages described by UEFI as RAM-like memory.
/// MMIO, reserved, unusable, and port-I/O descriptors are deliberately excluded.
pub fn is_ram_physical_address(address: u64) -> bool {
    let base = FIRMWARE_MAP.load(core::sync::atomic::Ordering::Acquire);
    let entries = FIRMWARE_MAP_ENTRIES.load(core::sync::atomic::Ordering::Acquire);
    let stride = FIRMWARE_MAP_STRIDE.load(core::sync::atomic::Ordering::Acquire);
    if base == 0 || stride < core::mem::size_of::<MemoryDescriptor>() { return false; }
    for i in 0..entries {
        let Some(offset) = i.checked_mul(stride) else { return false; };
        let Some(pointer) = (base as usize).checked_add(offset) else { return false; };
        let d = unsafe { ptr::read_unaligned(pointer as *const MemoryDescriptor) };
        if !matches!(d.kind, 1..=7 | 9 | 10 | 14) || d.number_of_pages == 0 { continue; }
        let Some(bytes) = d.number_of_pages.checked_mul(PAGE_SIZE) else { continue; };
        let Some(end) = d.physical_start.checked_add(bytes) else { continue; };
        if address >= d.physical_start && address < end { return true; }
    }
    false
}

/// Return true only when the retained UEFI memory map explicitly marks this
/// physical address as non-volatile (EFI_MEMORY_NV). Ordinary RAM and MMIO do
/// not qualify, even if their addresses happen to be mapped.
pub fn is_nonvolatile_physical_address(address: u64) -> bool {
    const EFI_MEMORY_NV: u64 = 0x8000;
    let base = FIRMWARE_MAP.load(core::sync::atomic::Ordering::Acquire);
    let entries = FIRMWARE_MAP_ENTRIES.load(core::sync::atomic::Ordering::Acquire);
    let stride = FIRMWARE_MAP_STRIDE.load(core::sync::atomic::Ordering::Acquire);
    if base == 0 || stride < core::mem::size_of::<MemoryDescriptor>() { return false; }
    for i in 0..entries {
        let Some(offset) = i.checked_mul(stride) else { return false; };
        let Some(pointer) = (base as usize).checked_add(offset) else { return false; };
        let d = unsafe { ptr::read_unaligned(pointer as *const MemoryDescriptor) };
        if d.number_of_pages == 0 || d.attributes & EFI_MEMORY_NV == 0 { continue; }
        let Some(bytes) = d.number_of_pages.checked_mul(PAGE_SIZE) else { continue; };
        let Some(end) = d.physical_start.checked_add(bytes) else { continue; };
        if address >= d.physical_start && address < end { return true; }
    }
    false
}

/// Convert a high physical address to its canonical direct-map virtual address.
/// Mapping is demand-created on first access; only UEFI-described RAM is eligible.
pub fn direct_map_address(physical_address: u64) -> Option<u64> {
    if physical_address >= DIRECT_MAP_PHYSICAL_LIMIT
        || !is_ram_physical_address(physical_address)
    { return None; }
    DIRECT_MAP_BASE.checked_add(physical_address)
}

/// Protect physical kernel text and rodata in the direct map as read-only/NX,
/// so the direct map cannot bypass the high-half W^X aliases.
pub fn set_direct_map_readonly_ranges(mut ranges: [(u64, u64); 2]) {
    for range in ranges.iter_mut() {
        range.0 &= !(PAGE_SIZE - 1);
        range.1 = align_up(range.1, PAGE_SIZE).unwrap_or(DIRECT_MAP_PHYSICAL_LIMIT);
    }
    unsafe { *DIRECT_MAP_READ_ONLY.0.get() = ranges; }
    DIRECT_MAP_PROTECTIONS_READY.store(true, core::sync::atomic::Ordering::Release);
}

pub fn direct_map_page_writable(physical_address: u64) -> bool {
    if !DIRECT_MAP_PROTECTIONS_READY.load(core::sync::atomic::Ordering::Acquire) {
        return false;
    }
    let ranges = unsafe { &*DIRECT_MAP_READ_ONLY.0.get() };
    !ranges.iter().any(|(start, end)| *start < *end
        && physical_address >= *start && physical_address < *end)
}

/// Return a demand-mapped direct-map pointer for any UEFI-described RAM page.
pub fn physical_access_pointer(physical_address: u64) -> Option<*mut u8> {
    direct_map_address(physical_address).map(|address| address as *mut u8)
}


/// Enumerate RAM-like firmware descriptors as sorted, merged (base, length)
/// ranges. The caller supplies bounded storage; the result never exceeds it.
pub fn physical_memory_ranges(output: &mut [(u64, u64)]) -> usize {
    let base = FIRMWARE_MAP.load(core::sync::atomic::Ordering::Acquire);
    let entries = FIRMWARE_MAP_ENTRIES.load(core::sync::atomic::Ordering::Acquire);
    let stride = FIRMWARE_MAP_STRIDE.load(core::sync::atomic::Ordering::Acquire);
    if base == 0 || stride < core::mem::size_of::<MemoryDescriptor>() || output.is_empty() {
        return 0;
    }
    let mut count = 0usize;
    for i in 0..entries {
        let Some(offset) = i.checked_mul(stride) else { break; };
        let Some(pointer) = (base as usize).checked_add(offset) else { break; };
        let descriptor = unsafe { ptr::read_unaligned(pointer as *const MemoryDescriptor) };
        if !matches!(descriptor.kind, 1..=7 | 9 | 10 | 14) || descriptor.number_of_pages == 0 {
            continue;
        }
        let Some(length) = descriptor.number_of_pages.checked_mul(PAGE_SIZE) else { continue; };
        let start = descriptor.physical_start & !(PAGE_SIZE - 1);
        let Some(end) = descriptor.physical_start.checked_add(length) else { continue; };
        if end <= start { continue; }
        if count == output.len() { break; }
        output[count] = (start, end - start);
        count += 1;
    }
    // Insertion-sort by base, then merge overlap/adjacency. Descriptor maps are
    // normally sorted, but the ABI must not rely on firmware ordering.
    for i in 1..count {
        let item = output[i];
        let mut j = i;
        while j > 0 && output[j - 1].0 > item.0 {
            output[j] = output[j - 1];
            j -= 1;
        }
        output[j] = item;
    }
    let mut merged = 0usize;
    for i in 0..count {
        let (start, length) = output[i];
        let Some(end) = start.checked_add(length) else { continue; };
        if merged != 0 {
            let previous = output[merged - 1];
            let previous_end = previous.0.saturating_add(previous.1);
            if start <= previous_end {
                output[merged - 1].1 = previous_end.max(end).saturating_sub(previous.0);
                continue;
            }
        }
        output[merged] = (start, end - start);
        merged += 1;
    }
    merged
}

/// Find a conventional-memory page above 4 GiB for a non-destructive direct-map test.
pub fn first_high_conventional_page() -> Option<u64> {
    let base = FIRMWARE_MAP.load(core::sync::atomic::Ordering::Acquire);
    let entries = FIRMWARE_MAP_ENTRIES.load(core::sync::atomic::Ordering::Acquire);
    let stride = FIRMWARE_MAP_STRIDE.load(core::sync::atomic::Ordering::Acquire);
    if base == 0 || stride < core::mem::size_of::<MemoryDescriptor>() { return None; }
    for i in 0..entries {
        let offset = i.checked_mul(stride)?;
        let pointer = (base as usize).checked_add(offset)?;
        let d = unsafe { ptr::read_unaligned(pointer as *const MemoryDescriptor) };
        if d.kind != EFI_CONVENTIONAL_MEMORY || d.number_of_pages == 0 { continue; }
        let bytes = d.number_of_pages.checked_mul(PAGE_SIZE)?;
        let end = d.physical_start.checked_add(bytes)?;
        let start = align_up(d.physical_start.max(PHYSICAL_LIMIT), PAGE_SIZE)?;
        if start < end && start < DIRECT_MAP_PHYSICAL_LIMIT { return Some(start); }
    }
    None
}
const FREE_CAPACITY: usize = 65536;
struct FreePages(core::cell::UnsafeCell<([u64; FREE_CAPACITY], usize)>);
unsafe impl Sync for FreePages {}
static FREE_PAGES: FreePages = FreePages(core::cell::UnsafeCell::new(([0; FREE_CAPACITY], 0)));
struct HighFreePages(core::cell::UnsafeCell<([u64; FREE_CAPACITY], usize)>);
unsafe impl Sync for HighFreePages {}
static HIGH_FREE_PAGES: HighFreePages = HighFreePages(core::cell::UnsafeCell::new(([0; FREE_CAPACITY], 0)));

#[cfg(target_os = "none")]
static MEMORY_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
#[cfg(not(target_os = "none"))]
static MEMORY_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

struct MemoryGuard {
    #[cfg(target_os = "none")]
    _guard: crate::hal::IrqSpinLockGuard<'static, ()>,
}
impl MemoryGuard {
    #[inline]
    fn try_acquire() -> Option<Self> {
        #[cfg(target_os = "none")]
        { MEMORY_LOCK.try_lock().map(|guard| Self { _guard: guard }) }
        #[cfg(not(target_os = "none"))]
        { MEMORY_LOCK.compare_exchange(false, true, core::sync::atomic::Ordering::Acquire, core::sync::atomic::Ordering::Relaxed).ok().map(|_| Self {}) }
    }
    #[inline]
    fn acquire() -> Self {
        #[cfg(target_os = "none")]
        { Self { _guard: MEMORY_LOCK.lock() } }
        #[cfg(not(target_os = "none"))]
        {
            while MEMORY_LOCK.compare_exchange_weak(false, true, core::sync::atomic::Ordering::Acquire, core::sync::atomic::Ordering::Relaxed).is_err() {
                core::hint::spin_loop();
            }
            Self {}
        }
    }
}
#[cfg(not(target_os = "none"))]
impl Drop for MemoryGuard {
    fn drop(&mut self) { MEMORY_LOCK.store(false, core::sync::atomic::Ordering::Release); }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError { InvalidMap, TooManyRegions, OutOfMemory, InvalidPage, FreeListFull, Busy }

fn align_up(value: u64, align: u64) -> Option<u64> {
    value.checked_add(align - 1).map(|v| v & !(align - 1))
}

/// Initialize from the final UEFI map. Descriptor stride is supplied by firmware
/// and is not assumed to equal Rust's descriptor struct size.
/// Parse descriptors into a temporary array before publishing any allocator state.
/// This makes malformed maps and capacity failures transactional.
fn collect_regions(map: *const u8, entries: usize, stride: usize,
                   out: &mut [Region; MAX_REGIONS]) -> Result<usize, AllocError> {
    if (entries != 0 && map.is_null())
        || stride < core::mem::size_of::<MemoryDescriptor>()
        || (entries > 1 && entries.checked_mul(stride).is_none()) {
        return Err(AllocError::InvalidMap);
    }
    let mut count = 0usize;
    for i in 0..entries {
        let offset = i.checked_mul(stride).ok_or(AllocError::InvalidMap)?;
        let address = (map as usize).checked_add(offset).ok_or(AllocError::InvalidMap)?;
        let d = unsafe { ptr::read_unaligned(address as *const MemoryDescriptor) };
        // Ignore non-conventional ranges before interpreting their size. Only
        // conventional RAM is allocator-owned; MMIO/reserved/loader ranges stay out.
        if d.kind != EFI_CONVENTIONAL_MEMORY || d.number_of_pages == 0 { continue; }
        let bytes = d.number_of_pages.checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
        let raw_end = d.physical_start.checked_add(bytes).ok_or(AllocError::InvalidMap)?;
        let start = align_up(d.physical_start.max(LOW_RESERVED_END), PAGE_SIZE)
            .ok_or(AllocError::InvalidMap)?;
        let end = raw_end.min(PHYSICAL_LIMIT) & !(PAGE_SIZE - 1);
        if start >= end { continue; }
        if count == MAX_REGIONS { return Err(AllocError::TooManyRegions); }
        out[count] = Region { next: start, end };
        count += 1;
    }
    validate_region_edges(out, count)?;
    Ok(count)
}

/// Reject overlapping normalized descriptors: duplicate physical frames in
/// allocator input can otherwise be handed out to two independent callers.
fn validate_region_edges(regions: &[Region; MAX_REGIONS], count: usize) -> Result<(), AllocError> {
    for i in 0..count {
        if regions[i].next >= regions[i].end
            || regions[i].next & (PAGE_SIZE - 1) != 0
            || regions[i].end & (PAGE_SIZE - 1) != 0 {
            return Err(AllocError::InvalidMap);
        }
        for j in 0..i {
            if regions[i].next < regions[j].end && regions[j].next < regions[i].end {
                return Err(AllocError::InvalidMap);
            }
        }
    }
    Ok(())
}

fn collect_high_regions(map: *const u8, entries: usize, stride: usize,
                        out: &mut [Region; MAX_REGIONS]) -> Result<usize, AllocError> {
    if (entries != 0 && map.is_null())
        || stride < core::mem::size_of::<MemoryDescriptor>()
        || (entries > 1 && entries.checked_mul(stride).is_none()) {
        return Err(AllocError::InvalidMap);
    }
    let mut count = 0usize;
    for i in 0..entries {
        let offset = i.checked_mul(stride).ok_or(AllocError::InvalidMap)?;
        let address = (map as usize).checked_add(offset).ok_or(AllocError::InvalidMap)?;
        let d = unsafe { ptr::read_unaligned(address as *const MemoryDescriptor) };
        if d.kind != EFI_CONVENTIONAL_MEMORY || d.number_of_pages == 0 { continue; }
        let bytes = d.number_of_pages.checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
        let raw_end = d.physical_start.checked_add(bytes).ok_or(AllocError::InvalidMap)?;
        let start = align_up(d.physical_start.max(PHYSICAL_LIMIT), PAGE_SIZE)
            .ok_or(AllocError::InvalidMap)?;
        let end = raw_end.min(DIRECT_MAP_PHYSICAL_LIMIT) & !(PAGE_SIZE - 1);
        if start >= end { continue; }
        if count == MAX_REGIONS { return Err(AllocError::TooManyRegions); }
        out[count] = Region { next: start, end };
        count += 1;
    }
    validate_region_edges(out, count)?;
    Ok(count)
}

pub unsafe fn initialize(map: *const u8, entries: usize, stride: usize) -> Result<usize, AllocError> {
    let _guard = MemoryGuard::acquire();
    let mut staged = [Region::EMPTY; MAX_REGIONS];
    let mut staged_high = [Region::EMPTY; MAX_REGIONS];
    let count = collect_regions(map, entries, stride, &mut staged)?;
    let high_count = collect_high_regions(map, entries, stride, &mut staged_high)?;
    let regions = unsafe { &mut *REGIONS.0.get() };
    regions.copy_from_slice(&staged);
    let high_regions = unsafe { &mut *HIGH_REGIONS.0.get() };
    high_regions.copy_from_slice(&staged_high);
    // Reinitialization must not retain frames returned by a previous map.
    unsafe { (&mut *FREE_PAGES.0.get()).1 = 0; }
    unsafe { (&mut *HIGH_FREE_PAGES.0.get()).1 = 0; }
    FIRMWARE_MAP.store(map as u64, core::sync::atomic::Ordering::Release);
    FIRMWARE_MAP_ENTRIES.store(entries, core::sync::atomic::Ordering::Release);
    FIRMWARE_MAP_STRIDE.store(stride, core::sync::atomic::Ordering::Release);
    REGION_COUNT.store(count, core::sync::atomic::Ordering::Release);
    HIGH_REGION_COUNT.store(high_count, core::sync::atomic::Ordering::Release);
    Ok(count)
}

/// Exclude a bootloader-owned physical interval from the early allocator.
///
/// UEFI memory maps should already classify allocated pages as loader data, but
/// explicitly reserving the active bootstrap stack also protects against
/// firmware/map-copy inconsistencies. The interval is half-open [start, end).
pub unsafe fn reserve_range(start: u64, end: u64) -> Result<(), AllocError> {
    let _guard = MemoryGuard::acquire();

    if start >= end || end > PHYSICAL_LIMIT {
        return Err(AllocError::InvalidMap);
    }
    let start = start & !(PAGE_SIZE - 1);
    let end = align_up(end, PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
    let regions = unsafe { &mut *REGIONS.0.get() };
    let count = REGION_COUNT.load(core::sync::atomic::Ordering::Acquire);
    let mut rebuilt = [Region::EMPTY; MAX_REGIONS];
    let mut rebuilt_count = 0usize;
    for region in regions.iter().take(count).copied() {
        if end <= region.next || start >= region.end {
            if rebuilt_count == MAX_REGIONS { return Err(AllocError::TooManyRegions); }
            rebuilt[rebuilt_count] = region;
            rebuilt_count += 1;
            continue;
        }
        if region.next < start {
            if rebuilt_count == MAX_REGIONS { return Err(AllocError::TooManyRegions); }
            rebuilt[rebuilt_count] = Region { next: region.next, end: start.min(region.end) };
            rebuilt_count += 1;
        }
        if end < region.end {
            if rebuilt_count == MAX_REGIONS { return Err(AllocError::TooManyRegions); }
            rebuilt[rebuilt_count] = Region { next: end.max(region.next), end: region.end };
            rebuilt_count += 1;
        }
    }
    regions.copy_from_slice(&rebuilt);
    REGION_COUNT.store(rebuilt_count, core::sync::atomic::Ordering::Release);
    Ok(())
}

/// Allocate a zeroed data frame, preferring conventional RAM above 4 GiB.
/// Must only be used after the kernel page-fault/direct-map handler is initialized.
/// Page-table pages must continue to use allocate_page(), which is guaranteed low.
pub unsafe fn allocate_data_page() -> Result<u64, AllocError> {
    let high_page = {
        let _guard = MemoryGuard::acquire();
        let free = unsafe { &mut *HIGH_FREE_PAGES.0.get() };
        if free.1 != 0 {
            free.1 -= 1;
            Some(free.0[free.1])
        } else {
            let count = HIGH_REGION_COUNT.load(core::sync::atomic::Ordering::Acquire);
            let regions = unsafe { &mut *HIGH_REGIONS.0.get() };
            let mut found = None;
            for region in regions.iter_mut().take(count) {
                if region.next.checked_add(PAGE_SIZE).is_some_and(|end| end <= region.end) {
                    let page = region.next;
                    region.next += PAGE_SIZE;
                    found = Some(page);
                    break;
                }
            }
            found
        }
    };
    if let Some(page) = high_page {
        let pointer = direct_map_address(page).ok_or(AllocError::InvalidMap)?;
        unsafe { ptr::write_bytes(pointer as *mut u8, 0, PAGE_SIZE as usize); }
        Ok(page)
    } else {
        unsafe { allocate_page() }
    }
}

/// Allocate one page satisfying an inclusive physical-address window and
/// optional power-of-two boundary. Pages skipped by the window remain available
/// to later callers; regions are split instead of advancing past skipped RAM.
pub unsafe fn allocate_page_in_range(low: u64, high: u64, boundary: u64) -> Result<u64, AllocError> {
    unsafe { allocate_page_in_range_mode(low, high, boundary, false) }
}

/// Bounded nonblocking variant used by MM_ALLOCATE_NO_WAIT. It never spins
/// waiting for the physical allocator lock.
pub unsafe fn try_allocate_page_in_range(low: u64, high: u64, boundary: u64) -> Result<u64, AllocError> {
    unsafe { allocate_page_in_range_mode(low, high, boundary, true) }
}

unsafe fn allocate_page_in_range_mode(low: u64, mut high: u64, boundary: u64, try_only: bool) -> Result<u64, AllocError> {
    const PAGE: u64 = PAGE_SIZE;
    if low > high || (boundary != 0 && (!boundary.is_power_of_two() || boundary < PAGE)) {
        return Err(AllocError::InvalidPage);
    }
    // The current no-wait path only touches the already identity-mapped low
    // pool. Demand-creating a high direct-map PTE could fault or allocate.
    if try_only {
        high = high.min(PHYSICAL_LIMIT - 1);
        if low > high { return Err(AllocError::OutOfMemory); }
    }
    let _guard = if try_only {
        MemoryGuard::try_acquire().ok_or(AllocError::Busy)?
    } else { MemoryGuard::acquire() };
    fn allowed(page: u64, low: u64, high: u64, boundary: u64) -> bool {
        let Some(end) = page.checked_add(PAGE_SIZE - 1) else { return false; };
        page >= low && end <= high && (boundary == 0 || page / boundary == end / boundary)
    }
    // Reuse returned frames without consuming unrelated pages.
    {
        let free = unsafe { &mut *FREE_PAGES.0.get() };
        let mut i = 0;
        while i < free.1 {
            let page = free.0[i];
            if allowed(page, low, high, boundary) {
                free.1 -= 1;
                free.0[i] = free.0[free.1];
                unsafe { ptr::write_bytes(page as *mut u8, 0, PAGE as usize); }
                return Ok(page);
            }
            i += 1;
        }
    }
    {
        let free = unsafe { &mut *HIGH_FREE_PAGES.0.get() };
        let mut i = 0;
        while i < free.1 {
            let page = free.0[i];
            if allowed(page, low, high, boundary) {
                // Validate the direct-map alias before removing the PFN from
                // the free stack, so a failed mapping check cannot leak it.
                let pointer = direct_map_address(page).ok_or(AllocError::InvalidMap)? as *mut u8;
                free.1 -= 1;
                free.0[i] = free.0[free.1];
                unsafe { ptr::write_bytes(pointer, 0, PAGE as usize); }
                return Ok(page);
            }
            i += 1;
        }
    }
    // Carve a matching page out of either allocator region while preserving
    // both the prefix and suffix as allocatable extents.
    for (store, count_atomic, high_memory) in [
        (&REGIONS, &REGION_COUNT, false),
        (&HIGH_REGIONS, &HIGH_REGION_COUNT, true),
    ] {
        let regions = unsafe { &mut *store.0.get() };
        let mut count = count_atomic.load(core::sync::atomic::Ordering::Acquire);
        let mut i = 0;
        while i < count {
            let original = regions[i];
            let mut candidate = align_up(original.next.max(low), PAGE).ok_or(AllocError::InvalidPage)?;
            if boundary != 0 && candidate / boundary != candidate.saturating_add(PAGE - 1) / boundary {
                candidate = align_up(candidate, boundary).ok_or(AllocError::InvalidPage)?;
            }
            if candidate.checked_add(PAGE).is_some_and(|end| end <= original.end && end - 1 <= high)
                && allowed(candidate, low, high, boundary)
            {
                // Resolve high-memory mapping before mutating the extent list.
                let pointer = if high_memory {
                    match direct_map_address(candidate) { Some(v) => v as *mut u8, None => return Err(AllocError::InvalidMap) }
                } else { candidate as *mut u8 };
                if candidate == original.next {
                    regions[i].next = candidate + PAGE;
                } else {
                    if count == MAX_REGIONS { return Err(AllocError::TooManyRegions); }
                    regions[i] = Region { next: original.next, end: candidate };
                    regions[count] = Region { next: candidate + PAGE, end: original.end };
                    count += 1;
                    count_atomic.store(count, core::sync::atomic::Ordering::Release);
                }
                unsafe { ptr::write_bytes(pointer, 0, PAGE as usize); }
                return Ok(candidate);
            }
            i += 1;
        }
    }
    Err(AllocError::OutOfMemory)
}

/// Allocate a single physically contiguous run within an inclusive address
/// window. Used when callers request contiguous MDL chunks or prefer a run.
pub unsafe fn allocate_contiguous_pages_in_range(
    pages: usize, low: u64, high: u64, boundary: u64,
) -> Result<u64, AllocError> {
    unsafe { allocate_contiguous_pages_in_range_mode(pages, low, high, boundary, false) }
}

/// Nonblocking contiguous-run allocation. Returns Busy immediately on lock contention.
pub unsafe fn try_allocate_contiguous_pages_in_range(
    pages: usize, low: u64, high: u64, boundary: u64,
) -> Result<u64, AllocError> {
    unsafe { allocate_contiguous_pages_in_range_mode(pages, low, high, boundary, true) }
}

unsafe fn allocate_contiguous_pages_in_range_mode(
    pages: usize, low: u64, mut high: u64, boundary: u64, try_only: bool,
) -> Result<u64, AllocError> {
    if pages == 0 || low > high || (boundary != 0 && (!boundary.is_power_of_two() || boundary < PAGE_SIZE)) {
        return Err(AllocError::InvalidPage);
    }
    if try_only {
        high = high.min(PHYSICAL_LIMIT - 1);
        if low > high { return Err(AllocError::OutOfMemory); }
    }
    let bytes = (pages as u64).checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidPage)?;
    let _guard = if try_only {
        MemoryGuard::try_acquire().ok_or(AllocError::Busy)?
    } else { MemoryGuard::acquire() };
    // First try to reclaim a contiguous run already returned to either free
    // stack. Remove every PFN only after proving the complete run is present.
    for high_memory in [false, true] {
        let (ptr, len_ptr): (*mut u64, *mut usize) = if high_memory {
            let free = unsafe { &mut *HIGH_FREE_PAGES.0.get() };
            (free.0.as_mut_ptr(), &mut free.1)
        } else {
            let free = unsafe { &mut *FREE_PAGES.0.get() };
            (free.0.as_mut_ptr(), &mut free.1)
        };
        let len = unsafe { *len_ptr };
        if pages > len { continue; }
        for i in 0..len {
            let base = unsafe { ptr.add(i).read() };
            let Some(end) = base.checked_add(bytes) else { continue; };
            if base < low || end - 1 > high
                || (boundary != 0
                    && (base % boundary != 0 || base / boundary != (end - 1) / boundary)) {
                continue;
            }
            let pointer = if high_memory {
                match direct_map_address(base) { Some(v) => v as *mut u8, None => continue }
            } else { base as *mut u8 };
            let mut found_all = true;
            for n in 1..pages {
                let wanted = base + n as u64 * PAGE_SIZE;
                if !(0..len).any(|j| unsafe { ptr.add(j).read() } == wanted) {
                    found_all = false;
                    break;
                }
            }
            if !found_all { continue; }
            for n in 0..pages {
                let wanted = base + n as u64 * PAGE_SIZE;
                let current_len = unsafe { *len_ptr };
                let Some(index) = (0..current_len).find(|j| unsafe { ptr.add(*j).read() } == wanted) else {
                    return Err(AllocError::InvalidMap);
                };
                unsafe {
                    *len_ptr = current_len - 1;
                    ptr.add(index).write(ptr.add(current_len - 1).read());
                }
            }
            unsafe { ptr::write_bytes(pointer, 0, bytes as usize); }
            return Ok(base);
        }
    }
    // Contiguous free-list runs are uncommon; scan region extents first. The
    // region split retains unallocated prefixes and suffixes exactly.
    for (store, count_atomic, high_memory) in [
        (&REGIONS, &REGION_COUNT, false),
        (&HIGH_REGIONS, &HIGH_REGION_COUNT, true),
    ] {
        let regions = unsafe { &mut *store.0.get() };
        let mut count = count_atomic.load(core::sync::atomic::Ordering::Acquire);
        for i in 0..count {
            let original = regions[i];
            let Some(mut candidate) = align_up(original.next.max(low), PAGE_SIZE) else { continue; };
            // Windows MM_ALLOCATE_REQUIRE_CONTIGUOUS_CHUNKS requires
            // each nonzero-SkipBytes chunk to start on its SkipBytes boundary,
            // not merely to avoid crossing a boundary.
            if boundary != 0 {
                let Some(next) = align_up(candidate, boundary) else { continue; };
                candidate = next;
            }
            let Some(end) = candidate.checked_add(bytes) else { continue; };
            if candidate < original.next || end > original.end || end - 1 > high
                || (boundary != 0 && candidate / boundary != (end - 1) / boundary) { continue; }
            let pointer = if high_memory {
                match direct_map_address(candidate) { Some(v) => v as *mut u8, None => return Err(AllocError::InvalidMap) }
            } else { candidate as *mut u8 };
            if candidate == original.next {
                regions[i].next = end;
            } else {
                if count == MAX_REGIONS { return Err(AllocError::TooManyRegions); }
                regions[i] = Region { next: original.next, end: candidate };
                regions[count] = Region { next: end, end: original.end };
                count += 1;
                count_atomic.store(count, core::sync::atomic::Ordering::Release);
            }
            unsafe { ptr::write_bytes(pointer, 0, bytes as usize); }
            return Ok(candidate);
        }
    }
    Err(AllocError::OutOfMemory)
}

/// Nonblocking batch allocation from already-populated free lists only.
/// The lock is acquired once with try_lock; this routine never carves allocator
/// regions, waits for pages, or performs reclaim. Address bounds are inclusive.
pub unsafe fn try_allocate_pages_in_range(
    output: &mut [u64], low: u64, high: u64, require_all: bool,
) -> Result<usize, AllocError> {
    if low > high { return Err(AllocError::InvalidPage); }
    let _guard = MemoryGuard::try_acquire().ok_or(AllocError::Busy)?;
    let low_free = unsafe { &mut *FREE_PAGES.0.get() };
    let high_free = unsafe { &mut *HIGH_FREE_PAGES.0.get() };
    let mut count = 0usize;
    while count < output.len() {
        let find = |pages: &mut ([u64; FREE_CAPACITY], usize)| -> Option<u64> {
            let mut i = 0;
            while i < pages.1 {
                let page = pages.0[i];
                let end = page.checked_add(PAGE_SIZE - 1);
                if page >= low && end.is_some_and(|end| end <= high) {
                    pages.1 -= 1;
                    pages.0[i] = pages.0[pages.1];
                    return Some(page);
                }
                i += 1;
            }
            None
        };
        let page = match find(&mut *low_free) {
            Some(page) => Some(page),
            None => find(&mut *high_free),
        };
        let Some(page) = page else { break; };
        output[count] = page;
        count += 1;
    }

    // Validate every high-memory alias before committing the allocation. If
    // the direct map cannot represent one of the selected frames, return all
    // selected frames to their original lists instead of leaking them.
    let map_valid = output[..count].iter().all(|page| {
        *page < PHYSICAL_LIMIT || direct_map_address(*page).is_some()
    });
    if (require_all && count != output.len()) || !map_valid {
        for &page in &output[..count] {
            let pages = if page >= PHYSICAL_LIMIT { &mut *high_free } else { &mut *low_free };
            if pages.1 < FREE_CAPACITY {
                pages.0[pages.1] = page;
                pages.1 += 1;
            }
        }
        return Err(if map_valid { AllocError::OutOfMemory } else { AllocError::InvalidMap });
    }
    for &page in &output[..count] {
        let pointer = if page >= PHYSICAL_LIMIT {
            // The preflight above guarantees this lookup succeeds unless the
            // direct-map contract itself changes concurrently (it is static).
            match direct_map_address(page) {
                Some(alias) => alias as *mut u8,
                None => return Err(AllocError::InvalidMap),
            }
        } else { page as *mut u8 };
        unsafe { ptr::write_bytes(pointer, 0, PAGE_SIZE as usize); }
    }
    if count == 0 { Err(AllocError::OutOfMemory) } else { Ok(count) }
}

/// Allocate a contiguous run wholly inside one SRAT-owned memory range.
/// Address windows are intersected with node ranges before the generic physical
/// allocator is called, so a successful run cannot cross a NUMA ownership edge.
#[cfg(target_os = "none")]
pub unsafe fn allocate_contiguous_pages_in_node_range(
    pages: usize, node: u32, low: u64, high: u64, boundary: u64,
) -> Result<u64, AllocError> {
    unsafe { allocate_contiguous_pages_in_node_range_mode(pages, node, low, high, boundary, false) }
}
#[cfg(target_os = "none")]
pub unsafe fn try_allocate_contiguous_pages_in_node_range(
    pages: usize, node: u32, low: u64, high: u64, boundary: u64,
) -> Result<u64, AllocError> {
    unsafe { allocate_contiguous_pages_in_node_range_mode(pages, node, low, high, boundary, true) }
}
#[cfg(target_os = "none")]
unsafe fn allocate_contiguous_pages_in_node_range_mode(
    pages: usize, node: u32, low: u64, mut high: u64, boundary: u64, try_only: bool,
) -> Result<u64, AllocError> {
    if try_only {
        high = high.min(PHYSICAL_LIMIT - 1);
        if low > high { return Err(AllocError::OutOfMemory); }
    }
    if pages == 0 || low > high || !crate::ntoskrnl::mm::topology::is_ready()
        || node as usize >= crate::ntoskrnl::mm::topology::memory_node_count() {
        return Err(AllocError::InvalidPage);
    }
    let mut ranges = [(0u64, 0u64); crate::ntoskrnl::mm::topology::MAX_RANGES];
    let count = crate::ntoskrnl::mm::topology::memory_ranges_for_node(node, &mut ranges);
    let mut saw_range = false;
    for &(start, length) in &ranges[..count] {
        let Some(end) = start.checked_add(length) else { continue; };
        let low_bound = low.max(start);
        let high_bound = high.min(end.saturating_sub(1));
        if low_bound > high_bound { continue; }
        saw_range = true;
        if let Ok(base) = unsafe {
            if try_only { try_allocate_contiguous_pages_in_range(pages, low_bound, high_bound, boundary) } else { allocate_contiguous_pages_in_range(pages, low_bound, high_bound, boundary) }
        } {
            let bytes = (pages as u64).checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidPage)?;
            if crate::ntoskrnl::mm::topology::node_for_range(base, bytes) == Some(node) {
                return Ok(base);
            }
            // Defensive rollback: never return a run whose entire extent is not
            // explicitly assigned to the requested proximity domain.
            for index in (0..pages).rev() {
                let _ = unsafe { if try_only { try_free_page(base + index as u64 * PAGE_SIZE) } else { free_page(base + index as u64 * PAGE_SIZE) } };
            }
        }
    }
    Err(if saw_range { AllocError::OutOfMemory } else { AllocError::InvalidPage })
}

/// Allocate zeroed pages only from firmware ranges explicitly assigned to
/// one NUMA proximity domain. This deliberately searches the returned-page
/// lists only: bump regions are not split by SRAT ownership yet, so consuming
/// from them here could silently discard pages belonging to other nodes.
#[cfg(target_os = "none")]
pub unsafe fn allocate_pages_in_node_range(
    output: &mut [u64], node: u32, low: u64, high: u64, require_all: bool,
) -> Result<usize, AllocError> {
    unsafe { allocate_pages_in_node_range_mode(output, node, low, high, require_all, false) }
}
#[cfg(target_os = "none")]
pub unsafe fn try_allocate_pages_in_node_range(
    output: &mut [u64], node: u32, low: u64, high: u64, require_all: bool,
) -> Result<usize, AllocError> {
    unsafe { allocate_pages_in_node_range_mode(output, node, low, high, require_all, true) }
}
#[cfg(target_os = "none")]
unsafe fn allocate_pages_in_node_range_mode(
    output: &mut [u64], node: u32, low: u64, mut high: u64, require_all: bool, try_only: bool,
) -> Result<usize, AllocError> {
    if try_only {
        high = high.min(PHYSICAL_LIMIT - 1);
        if low > high { return Err(AllocError::OutOfMemory); }
    }
    if low > high || !crate::ntoskrnl::mm::topology::is_ready()
        || node as usize >= crate::ntoskrnl::mm::topology::memory_node_count() {
        return Err(AllocError::InvalidPage);
    }
    // SRAT ranges constrain ownership; intersect them with UEFI conventional
    // memory extents so this path can safely allocate never-before-freed pages.
    let mut owned_ranges = [(0u64, 0u64); crate::ntoskrnl::mm::topology::MAX_RANGES];
    let owned_count = crate::ntoskrnl::mm::topology::memory_ranges_for_node(node, &mut owned_ranges);
    if owned_count == 0 { return Err(AllocError::OutOfMemory); }
    let mut allocated = 0usize;
    while allocated < output.len() {
        // This is the blocking NT allocation path; transient lock contention is
        // not out-of-memory. MM_ALLOCATE_NO_WAIT is rejected by the caller until
        // its entire pool/registry/rollback path can be made nonblocking.
        let _guard = if try_only { MemoryGuard::try_acquire().ok_or(AllocError::Busy)? } else { MemoryGuard::acquire() };
        let low_free = unsafe { &mut *FREE_PAGES.0.get() };
        let high_free = unsafe { &mut *HIGH_FREE_PAGES.0.get() };
        let mut found = None;
        for pages in if try_only { vec![&mut *low_free] } else { vec![&mut *low_free, &mut *high_free] } {
            let mut i = 0usize;
            while i < pages.1 {
                let page = pages.0[i];
                if page >= low && page.checked_add(PAGE_SIZE - 1).is_some_and(|end| end <= high)
                    && crate::ntoskrnl::mm::topology::node_for_range(page, PAGE_SIZE) == Some(node)
                    && (page < PHYSICAL_LIMIT || direct_map_address(page).is_some()) {
                    pages.1 -= 1;
                    pages.0[i] = pages.0[pages.1];
                    found = Some(page);
                    break;
                }
                i += 1;
            }
            if found.is_some() { break; }
        }
        if found.is_none() {
            'regions: for (store, count_atomic, high_memory) in if try_only { vec![(&REGIONS, &REGION_COUNT, false)] } else { vec![(&REGIONS, &REGION_COUNT, false), (&HIGH_REGIONS, &HIGH_REGION_COUNT, true)] } {
                let regions = unsafe { &mut *store.0.get() };
                let mut region_count = count_atomic.load(core::sync::atomic::Ordering::Acquire);
                for index in 0..region_count {
                    let original = regions[index];
                    for &(node_start, node_length) in &owned_ranges[..owned_count] {
                        let Some(node_end) = node_start.checked_add(node_length) else { continue; };
                        let start = original.next.max(low).max(node_start);
                        let Some(candidate) = align_up(start, PAGE_SIZE) else { continue; };
                        let end_limit = original.end.min(node_end);
                        let Some(page_end) = candidate.checked_add(PAGE_SIZE) else { continue; };
                        if candidate < original.next || page_end > end_limit
                            || candidate.checked_add(PAGE_SIZE - 1).is_none_or(|end| end > high) {
                            continue;
                        }
                        if high_memory && direct_map_address(candidate).is_none() { continue; }
                        if candidate == original.next {
                            regions[index].next = page_end;
                        } else if page_end == original.end {
                            regions[index].end = candidate;
                        } else {
                            if region_count == MAX_REGIONS { continue; }
                            regions[index] = Region { next: original.next, end: candidate };
                            regions[region_count] = Region { next: page_end, end: original.end };
                            region_count += 1;
                            count_atomic.store(region_count, core::sync::atomic::Ordering::Release);
                        }
                        found = Some(candidate);
                        break 'regions;
                    }
                }
            }
        }
        let Some(page) = found else { break; };
        let pointer = if page >= PHYSICAL_LIMIT {
            match direct_map_address(page) {
                Some(alias) => alias as *mut u8,
                None => { drop(_guard); break; }
            }
        } else { page as *mut u8 };
        unsafe { ptr::write_bytes(pointer, 0, PAGE_SIZE as usize); }
        // The output contract is a PFN array (as consumed by MDLs), not a
        // byte-address array. Returning the physical byte address here made
        // the MDL multiply it by PAGE_SIZE a second time and fail node checks.
        output[allocated] = page / PAGE_SIZE;
        allocated += 1;
    }
    if allocated == 0 || (require_all && allocated != output.len()) {
        // Roll back in reverse allocation order so pages carved from an extent
        // can coalesce back toward the original range.
        for &page in output[..allocated].iter().rev() {
            let _ = unsafe { if try_only { try_free_page(page) } else { free_page(page) } };
        }
        return Err(AllocError::OutOfMemory);
    }
    Ok(allocated)
}

/// Allocate one zeroed physical page. The early kernel uses identity mappings,
/// so the returned physical address is also directly writable as a pointer.
pub unsafe fn allocate_page() -> Result<u64, AllocError> {
    let _guard = MemoryGuard::acquire();

    let free = unsafe { &mut *FREE_PAGES.0.get() };
    if free.1 != 0 {
        free.1 -= 1;
        let page = free.0[free.1];
        unsafe { ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE as usize); }
        return Ok(page);
    }
    let count = REGION_COUNT.load(core::sync::atomic::Ordering::Acquire);
    let regions = unsafe { &mut *REGIONS.0.get() };
    for region in regions.iter_mut().take(count) {
        if region.next.checked_add(PAGE_SIZE).is_some_and(|end| end <= region.end) {
            let page = region.next;
            region.next += PAGE_SIZE;
            unsafe { ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE as usize); }
            return Ok(page);
        }
    }
    Err(AllocError::OutOfMemory)
}

/// Nonblocking allocation for MM_ALLOCATE_NO_WAIT: never spins or reclaims.
pub unsafe fn try_allocate_page() -> Result<u64, AllocError> {
    let _guard = MemoryGuard::try_acquire().ok_or(AllocError::Busy)?;
    let free = unsafe { &mut *FREE_PAGES.0.get() };
    if free.1 != 0 { free.1 -= 1; let page=free.0[free.1]; unsafe { ptr::write_bytes(page as *mut u8,0,PAGE_SIZE as usize); } return Ok(page); }
    let count=REGION_COUNT.load(core::sync::atomic::Ordering::Acquire); let regions=unsafe { &mut *REGIONS.0.get() };
    for region in regions.iter_mut().take(count) { if region.next.checked_add(PAGE_SIZE).is_some_and(|end|end<=region.end) { let page=region.next; region.next+=PAGE_SIZE; unsafe { ptr::write_bytes(page as *mut u8,0,PAGE_SIZE as usize); } return Ok(page); } }
    Err(AllocError::OutOfMemory)
}

/// Allocate a physically contiguous, zeroed run whose end stays below an
/// exclusive physical-address limit. AP bootstrap stacks use this because the
/// early startup path relies on the bootloader's low 4-GiB identity mapping.
pub unsafe fn allocate_contiguous_pages_below(
    pages: usize,
    exclusive_limit: u64,
) -> Result<u64, AllocError> {
    let _guard = MemoryGuard::acquire();
    if pages == 0 || exclusive_limit < PAGE_SIZE {
        return Err(AllocError::InvalidPage);
    }
    let bytes = (pages as u64)
        .checked_mul(PAGE_SIZE)
        .ok_or(AllocError::InvalidPage)?;

    let free = unsafe { &mut *FREE_PAGES.0.get() };
    if pages <= free.1 {
        let mut i = 0usize;
        while i + pages <= free.1 {
            let base = free.0[i];
            let end = base.checked_add(bytes);
            let mut run = end.is_some_and(|end| end <= exclusive_limit);
            for j in 1..pages {
                if free.0[i + j] != base + (j as u64 * PAGE_SIZE) {
                    run = false;
                    break;
                }
            }
            if run {
                let old_len = free.1;
                free.0.copy_within(i + pages..old_len, i);
                free.1 = old_len - pages;
                unsafe { ptr::write_bytes(base as *mut u8, 0, bytes as usize); }
                return Ok(base);
            }
            i += 1;
        }
    }

    let count = REGION_COUNT.load(core::sync::atomic::Ordering::Acquire);
    let regions = unsafe { &mut *REGIONS.0.get() };
    for region in regions.iter_mut().take(count) {
        if let Some(end) = region.next.checked_add(bytes) {
            if end <= region.end && end <= exclusive_limit {
                let base = region.next;
                region.next = end;
                unsafe { ptr::write_bytes(base as *mut u8, 0, bytes as usize); }
                return Ok(base);
            }
        }
    }
    Err(AllocError::OutOfMemory)
}

/// Allocate a physically contiguous, zeroed run of pages from returned free runs
/// or a conventional-memory region. Allocation is serialized across CPUs and IRQs.
pub unsafe fn allocate_contiguous_pages(pages: usize) -> Result<u64, AllocError> {
    let _guard = MemoryGuard::acquire();
    if pages == 0 { return Err(AllocError::InvalidPage); }
    let bytes = (pages as u64).checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidPage)?;

    // Reuse returned contiguous runs before consuming more of the bump regions.
    let free = unsafe { &mut *FREE_PAGES.0.get() };
    if pages <= free.1 {
        let mut i = 0usize;
        while i + pages <= free.1 {
            let base = free.0[i];
            let mut run = true;
            for j in 1..pages {
                if free.0[i + j] != base + (j as u64 * PAGE_SIZE) {
                    run = false;
                    break;
                }
            }
            if run {
                let old_len = free.1;
                free.0.copy_within(i + pages..old_len, i);
                free.1 = old_len - pages;
                unsafe { ptr::write_bytes(base as *mut u8, 0, bytes as usize); }
                return Ok(base);
            }
            i += 1;
        }
    }

    let count = REGION_COUNT.load(core::sync::atomic::Ordering::Acquire);
    let regions = unsafe { &mut *REGIONS.0.get() };
    for region in regions.iter_mut().take(count) {
        if let Some(end) = region.next.checked_add(bytes) {
            if end <= region.end {
                let base = region.next;
                region.next = end;
                unsafe { ptr::write_bytes(base as *mut u8, 0, bytes as usize); }
                return Ok(base);
            }
        }
    }
    Err(AllocError::OutOfMemory)
}

/// Return a previously allocated low physical page to the shared free list.
/// The caller must ensure no active mapping or DMA operation references the page.
#[cfg(target_os = "none")]
fn page_is_locked(page: u64) -> bool {
    crate::ntoskrnl::mm::runtime::is_page_locked(page)
}
#[cfg(target_os = "none")]
fn page_is_locked_try(page: u64) -> bool {
    crate::ntoskrnl::mm::runtime::try_is_page_locked(page)
}
#[cfg(not(target_os = "none"))]
fn page_is_locked_try(_page: u64) -> bool { true }
#[cfg(not(target_os = "none"))]
fn page_is_locked(_page: u64) -> bool { false }

pub unsafe fn free_page(page: u64) -> Result<(), AllocError> {
    unsafe { free_page_mode(page, false) }
}

/// Nonblocking page return used by rollback from MM_ALLOCATE_NO_WAIT.
pub unsafe fn try_free_page(page: u64) -> Result<(), AllocError> {
    unsafe { free_page_mode(page, true) }
}

unsafe fn free_page_mode(page: u64, try_only: bool) -> Result<(), AllocError> {
    if page >= PHYSICAL_LIMIT {
        if page >= DIRECT_MAP_PHYSICAL_LIMIT || page & (PAGE_SIZE - 1) != 0
            || !is_ram_physical_address(page) {
            return Err(AllocError::InvalidPage);
        }
        let _guard = if try_only { MemoryGuard::try_acquire().ok_or(AllocError::Busy)? } else { MemoryGuard::acquire() };
        if (if try_only { page_is_locked_try(page) } else { page_is_locked(page) }) { return Err(AllocError::InvalidPage); }
        let free = unsafe { &mut *HIGH_FREE_PAGES.0.get() };
        if free.1 < FREE_CAPACITY {
            free.0[free.1] = page;
            free.1 += 1;
            return Ok(());
        }
        let regions = unsafe { &mut *HIGH_REGIONS.0.get() };
        let count = HIGH_REGION_COUNT.load(core::sync::atomic::Ordering::Acquire);
        let end = page.checked_add(PAGE_SIZE).ok_or(AllocError::InvalidPage)?;
        for region in regions.iter_mut().take(count) {
            if end == region.next { region.next = page; return Ok(()); }
            if page == region.end { region.end = end; return Ok(()); }
        }
        if count == MAX_REGIONS { return Err(AllocError::FreeListFull); }
        regions[count] = Region { next: page, end };
        HIGH_REGION_COUNT.store(count + 1, core::sync::atomic::Ordering::Release);
        return Ok(());
    }

    let _guard = MemoryGuard::acquire();
    if page < LOW_RESERVED_END || page & (PAGE_SIZE - 1) != 0 {
        return Err(AllocError::InvalidPage);
    }
    if page_is_locked(page) { return Err(AllocError::InvalidPage); }
    let free = unsafe { &mut *FREE_PAGES.0.get() };
    if free.1 < FREE_CAPACITY {
        free.0[free.1] = page;
        free.1 += 1;
        return Ok(());
    }

    // When the LIFO free-page cache is full, return this page as a one-page
    // extent instead of losing the frame. The allocator lock protects both
    // structures, so the page is never simultaneously visible in both.
    let end = page + PAGE_SIZE;
    let regions = unsafe { &mut *REGIONS.0.get() };
    let count = REGION_COUNT.load(core::sync::atomic::Ordering::Acquire);
    for region in regions.iter_mut().take(count) {
        if end == region.next {
            region.next = page;
            return Ok(());
        }
        if page == region.end {
            region.end = end;
            return Ok(());
        }
    }
    if count == MAX_REGIONS { return Err(AllocError::FreeListFull); }
    regions[count] = Region { next: page, end };
    REGION_COUNT.store(count + 1, core::sync::atomic::Ordering::Release);
    Ok(())
}

/// Return a contiguous allocation to the physical free-page stack atomically.
/// No pages are released if the free stack cannot hold the entire run.
pub unsafe fn free_contiguous_pages(base: u64, pages: usize) -> Result<(), AllocError> {
    if pages == 0 || base < LOW_RESERVED_END || base & (PAGE_SIZE - 1) != 0 {
        return Err(AllocError::InvalidPage);
    }
    let bytes = (pages as u64).checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidPage)?;
    let end = base.checked_add(bytes).filter(|end| *end <= PHYSICAL_LIMIT).ok_or(AllocError::InvalidPage)?;
    let _guard = MemoryGuard::acquire();
    let mut locked_page = base;
    while locked_page < end {
        if page_is_locked(locked_page) { return Err(AllocError::InvalidPage); }
        locked_page += PAGE_SIZE;
    }
    let free = unsafe { &mut *FREE_PAGES.0.get() };
    if pages <= FREE_CAPACITY - free.1 {
        let mut page = base;
        while page < end {
            free.0[free.1] = page;
            free.1 += 1;
            page += PAGE_SIZE;
        }
        return Ok(());
    }

    // If the stack is full, retain the whole run as a free region instead of
    // partially freeing it. Adjacent runs can extend an existing free extent.
    let regions = unsafe { &mut *REGIONS.0.get() };
    let count = REGION_COUNT.load(core::sync::atomic::Ordering::Acquire);
    for region in regions.iter_mut().take(count) {
        if end == region.next {
            region.next = base;
            return Ok(());
        }
        if base == region.end {
            region.end = end;
            return Ok(());
        }
    }
    if count == MAX_REGIONS { return Err(AllocError::FreeListFull); }
    regions[count] = Region { next: base, end };
    REGION_COUNT.store(count + 1, core::sync::atomic::Ordering::Release);
    Ok(())
}

pub fn region_count() -> usize {
    REGION_COUNT.load(core::sync::atomic::Ordering::Acquire)
}


const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const LARGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
const LARGE_ADDRESS_MASK: u64 = 0x000f_ffff_ffe0_0000;

const HUGE_ADDRESS_MASK: u64 = 0x000f_ffff_c000_0000;
const HUGE: u64 = 1 << 7;

/// Read-only summary of a live x86-64 page-table translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappingInfo {
    pub physical_address: u64,
    pub page_size: u64,
    pub writable: bool,
    pub executable: bool,
    pub user_accessible: bool,
}

/// Identifies the first invalid entry encountered during a page-table walk.
/// Stage: 1=input, 2=PML4, 3=PDPT, 4=PD, 5=PT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappingInspectionFailure {
    pub stage: u8,
    pub pml4e: u64,
    pub pdpte: u64,
    pub pde: u64,
    pub pte: u64,
}

/// Inspect the bootstrap's four-level page tables and retain the entries
/// observed at the first failure. This is diagnostic-only and does not mutate tables.
pub unsafe fn inspect_mapping_diagnostic(pml4_physical: u64, virtual_address: u64)
    -> Result<MappingInfo, MappingInspectionFailure>
{
    const USER: u64 = 1 << 2;
    if pml4_physical == 0 || pml4_physical >= PHYSICAL_LIMIT
        || pml4_physical & (PAGE_SIZE - 1) != 0
        || !(virtual_address <= 0x0000_7fff_ffff_ffff
            || virtual_address >= 0xffff_8000_0000_0000)
    {
        return Err(MappingInspectionFailure{stage:1,pml4e:0,pdpte:0,pde:0,pte:0});
    }
    let pml4 = pml4_physical as *const u64;
    let pml4e = unsafe { pml4.add(((virtual_address >> 39) & 0x1ff) as usize).read() };
    if pml4e & PRESENT == 0 || pml4e & LARGE != 0 { return Err(MappingInspectionFailure{stage:2,pml4e,pdpte:0,pde:0,pte:0}); }
    let pdpt = (pml4e & ADDRESS_MASK) as *const u64;
    let pdpte = unsafe { pdpt.add(((virtual_address >> 30) & 0x1ff) as usize).read() };
    if pdpte & PRESENT == 0 { return Err(MappingInspectionFailure{stage:3,pml4e,pdpte,pde:0,pte:0}); }
    if pdpte & LARGE != 0 {
        let base = pdpte & HUGE_ADDRESS_MASK;
        let offset = virtual_address & ((1u64 << 30) - 1);
        return Ok(MappingInfo {
            physical_address: base + offset, page_size: 1u64 << 30,
            writable: pml4e & pdpte & WRITABLE != 0,
            executable: (pml4e | pdpte) & NO_EXECUTE == 0,
            user_accessible: pml4e & pdpte & USER != 0,
        });
    }
    let pd = (pdpte & ADDRESS_MASK) as *const u64;
    let pde = unsafe { pd.add(((virtual_address >> 21) & 0x1ff) as usize).read() };
    if pde & PRESENT == 0 { return Err(MappingInspectionFailure{stage:4,pml4e,pdpte,pde,pte:0}); }
    if pde & LARGE != 0 {
        let base = pde & LARGE_ADDRESS_MASK;
        let offset = virtual_address & ((1u64 << 21) - 1);
        return Ok(MappingInfo {
            physical_address: base + offset, page_size: 1u64 << 21,
            writable: pml4e & pdpte & pde & WRITABLE != 0,
            executable: (pml4e | pdpte | pde) & NO_EXECUTE == 0,
            user_accessible: pml4e & pdpte & pde & USER != 0,
        });
    }
    let pt = (pde & ADDRESS_MASK) as *const u64;
    let pte = unsafe { pt.add(((virtual_address >> 12) & 0x1ff) as usize).read() };
    if pte & PRESENT == 0 { return Err(MappingInspectionFailure{stage:5,pml4e,pdpte,pde,pte}); }
    Ok(MappingInfo {
        physical_address: (pte & ADDRESS_MASK) | (virtual_address & (PAGE_SIZE - 1)),
        page_size: PAGE_SIZE,
        writable: pml4e & pdpte & pde & pte & WRITABLE != 0,
        executable: (pml4e | pdpte | pde | pte) & NO_EXECUTE == 0,
        user_accessible: pml4e & pdpte & pde & pte & USER != 0,
    })
}

/// Compatibility wrapper for callers that only need success/failure.
pub unsafe fn inspect_mapping(pml4_physical: u64, virtual_address: u64)
    -> Result<MappingInfo, AllocError>
{
    unsafe { inspect_mapping_diagnostic(pml4_physical, virtual_address) }
        .map_err(|_| AllocError::InvalidMap)
}

/// Verify every page touched by a bootstrap dependency is present and identity
/// mapped. This deliberately validates the existing bootstrap contract before
/// any permission hardening or page-table restructuring is attempted.
pub unsafe fn validate_identity_range(
    pml4_physical: u64, start: u64, length: u64,
) -> Result<(), AllocError> {
    if length == 0 { return Ok(()); }
    let end = start.checked_add(length).ok_or(AllocError::InvalidMap)?;
    if end > PHYSICAL_LIMIT { return Err(AllocError::InvalidMap); }
    let mut address = start & !(PAGE_SIZE - 1);
    while address < end {
        let mapping = unsafe { inspect_mapping(pml4_physical, address)? };
        if mapping.physical_address != address { return Err(AllocError::InvalidMap); }
        address = address.checked_add(PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
    }
    Ok(())
}

/// Validate a virtual range against its expected physical backing, page by page.
/// Supports high-half aliases as well as identity mappings.
pub unsafe fn validate_translation_range(
    pml4_physical: u64, virtual_start: u64, physical_start: u64, length: u64,
) -> Result<(), AllocError> {
    if length == 0 { return Ok(()); }
    let virtual_end = virtual_start.checked_add(length).ok_or(AllocError::InvalidMap)?;
    let physical_end = physical_start.checked_add(length).ok_or(AllocError::InvalidMap)?;
    if physical_end > PHYSICAL_LIMIT || virtual_end <= virtual_start {
        return Err(AllocError::InvalidMap);
    }
    let mut va = virtual_start & !(PAGE_SIZE - 1);
    let mut pa = physical_start & !(PAGE_SIZE - 1);
    let end = align_up(virtual_end, PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
    while va < end {
        let mapping = unsafe { inspect_mapping(pml4_physical, va)? };
        if mapping.physical_address != pa { return Err(AllocError::InvalidMap); }
        va = va.checked_add(PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
        pa = pa.checked_add(PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
    }
    Ok(())
}

/// Enforce section W^X and guarded-stack permissions on the 4 KiB bootstrap
/// mappings. Low identity aliases of the kernel image and stack are removed
/// after their high-half aliases have been validated.
///
/// # Safety
/// NXE must already be enabled, pml4 must be the active identity-mapped root,
/// and the physical allocator must have been initialized from the final UEFI map.
pub unsafe fn harden_page_tables(
    pml4_physical: u64,
    text: (u64, u64),
    rodata: (u64, u64),
    data: (u64, u64),
    bss: (u64, u64),
    guard_page: u64,
    stack: (u64, u64),
    virtual_guard_page: u64,
    physical_sections: [(u64, u64); 4],
) -> Result<(), AllocError> {
    if pml4_physical == 0 || pml4_physical >= PHYSICAL_LIMIT || pml4_physical & 0xfff != 0 {
        return Err(AllocError::InvalidMap);
    }

    unsafe fn split(pml4_physical: u64, address: u64) -> Result<*mut u64, AllocError> {
        let pml4 = pml4_physical as *mut u64;
        let root_entry = unsafe { pml4.add(((address >> 39) & 0x1ff) as usize).read() };
        if root_entry & PRESENT == 0 { return Err(AllocError::InvalidMap); }
        let pdpt = (root_entry & ADDRESS_MASK) as *mut u64;
        let pdpt_entry = unsafe { pdpt.add(((address >> 30) & 0x1ff) as usize).read() };
        if pdpt_entry & PRESENT == 0 { return Err(AllocError::InvalidMap); }
        let pd = (pdpt_entry & ADDRESS_MASK) as *mut u64;
        let pde_ptr = unsafe { pd.add(((address >> 21) & 0x1ff) as usize) };
        let pde = unsafe { pde_ptr.read() };
        if pde & PRESENT == 0 { return Err(AllocError::InvalidMap); }
        if pde & LARGE == 0 { return Ok((pde & ADDRESS_MASK) as *mut u64); }
        let table = unsafe { allocate_page()? } as *mut u64;
        let base = pde & LARGE_ADDRESS_MASK;
        let inherited = pde & (WRITABLE | NO_EXECUTE);
        for i in 0..512usize {
            unsafe { table.add(i).write(base + (i as u64 * PAGE_SIZE) | PRESENT | inherited); }
        }
        unsafe { pde_ptr.write((table as u64) | PRESENT | WRITABLE); }
        Ok(table)
    }

    unsafe fn split_range(pml4_physical: u64, range: (u64, u64)) -> Result<(), AllocError> {
        if range.0 >= range.1 { return Ok(()); }
        let mut address = range.0 & !(PAGE_SIZE - 1);
        let end = align_up(range.1, PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
        while address < end {
            unsafe { split(pml4_physical, address)?; }
            address = (address & !(0x20_0000 - 1)) + 0x20_0000;
        }
        Ok(())
    }

    unsafe fn unmap_identity_range(
        pml4_physical: u64, range: (u64, u64),
    ) -> Result<(), AllocError> {
        if range.0 >= range.1 { return Ok(()); }
        let mut address = range.0 & !(PAGE_SIZE - 1);
        let end = align_up(range.1, PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
        while address < end {
            let table = unsafe { split(pml4_physical, address)? };
            let leaf = unsafe { table.add(((address >> 12) & 0x1ff) as usize) };
            if unsafe { leaf.read() } & PRESENT == 0 { return Err(AllocError::InvalidMap); }
            unsafe { leaf.write(0); }
            address = address.checked_add(PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
        }
        Ok(())
    }

    unsafe fn set_range(
        pml4_physical: u64, range: (u64, u64), writable: bool, executable: bool,
    ) -> Result<(), AllocError> {
        if range.0 >= range.1 { return Ok(()); }
        let start = range.0 & !(PAGE_SIZE - 1);
        let end = align_up(range.1, PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
        let mut address = start;
        while address < end {
            let table = unsafe { split(pml4_physical, address)? };
            let pte_ptr = unsafe { table.add(((address >> 12) & 0x1ff) as usize) };
            let mut pte = unsafe { pte_ptr.read() };
            if pte & PRESENT == 0 { return Err(AllocError::InvalidMap); }
            if writable { pte |= WRITABLE; } else { pte &= !WRITABLE; }
            if executable { pte &= !NO_EXECUTE; } else { pte |= NO_EXECUTE; }
            unsafe { pte_ptr.write(pte); }
            address += PAGE_SIZE;
        }
        Ok(())
    }

    // Split every 2 MiB region containing kernel sections or the stack guard
    // before setting NX globally. Otherwise the instruction currently executing
    // could be marked NX by its own large-page PDE and fault mid-update.
    unsafe {
        split_range(pml4_physical, text)?;
        split_range(pml4_physical, rodata)?;
        split_range(pml4_physical, data)?;
        split_range(pml4_physical, bss)?;
        split_range(pml4_physical, stack)?;
        split(pml4_physical, guard_page)?;
    }

    let text_page_start = text.0 & !(PAGE_SIZE - 1);
    let text_page_end = align_up(text.1, PAGE_SIZE).ok_or(AllocError::InvalidMap)?;
    let pml4 = pml4_physical as *mut u64;
    let root_entry = unsafe { pml4.read() };
    if root_entry & PRESENT == 0 { return Err(AllocError::InvalidMap); }
    let pdpt = (root_entry & ADDRESS_MASK) as *mut u64;
    for pdpt_index in 0..4usize {
        let entry = unsafe { pdpt.add(pdpt_index).read() };
        if entry & PRESENT == 0 { return Err(AllocError::InvalidMap); }
        let pd = (entry & ADDRESS_MASK) as *mut u64;
        for pd_index in 0..512usize {
            let pde_ptr = unsafe { pd.add(pd_index) };
            let pde = unsafe { pde_ptr.read() };
            if pde & PRESENT == 0 { continue; }
            if pde & LARGE != 0 {
                unsafe { pde_ptr.write(pde | NO_EXECUTE | WRITABLE); }
            } else {
                let table = (pde & ADDRESS_MASK) as *mut u64;
                for pte_index in 0..512usize {
                    let pte_ptr = unsafe { table.add(pte_index) };
                    let pte = unsafe { pte_ptr.read() };
                    if pte & PRESENT != 0 {
                        let page_address = pte & ADDRESS_MASK;
                        // Never make the currently executing kernel text NX, even
                        // transiently: instruction fetch could fault before policy
                        // restoration below. All other pages become NX immediately.
                        if page_address >= text_page_start && page_address < text_page_end {
                            unsafe { pte_ptr.write(pte & !WRITABLE); }
                        } else {
                            unsafe { pte_ptr.write(pte | NO_EXECUTE | WRITABLE); }
                        }
                    }
                }
            }
        }
    }

    unsafe {
        set_range(pml4_physical, text, false, true)?;
        set_range(pml4_physical, rodata, false, false)?;
        set_range(pml4_physical, data, true, false)?;
        set_range(pml4_physical, bss, true, false)?;
        set_range(pml4_physical, stack, true, false)?;
        if inspect_mapping(pml4_physical, virtual_guard_page).is_ok() {
            return Err(AllocError::InvalidMap);
        }
        // Remove writable low identity aliases for the kernel image and its
        // active stack. Their validated high-half aliases are now authoritative,
        // preventing a writable alias from bypassing text/rodata protections.
        for range in physical_sections {
            unmap_identity_range(pml4_physical, range)?;
        }
        unmap_identity_range(pml4_physical, (
            guard_page + PAGE_SIZE,
            guard_page.checked_add(stack.1 - stack.0 + PAGE_SIZE).ok_or(AllocError::InvalidMap)?,
        ))?;
        let guard_table = split(pml4_physical, guard_page)?;
        let guard_pte = guard_table.add(((guard_page >> 12) & 0x1ff) as usize);
        guard_pte.write(0);
        core::arch::asm!("mov cr3, {}", in(reg) pml4_physical, options(nostack, preserves_flags));
    }
    Ok(())
}


/// Configure one identity-mapped 4 KiB page after splitting its 2 MiB mapping
/// if necessary. Cache-disable/write-through are used for device MMIO; callers
/// must avoid creating cache-type aliases for the same physical device page.
pub unsafe fn configure_identity_page(
    pml4_physical: u64,
    address: u64,
    writable: bool,
    executable: bool,
    cache_disable: bool,
    write_through: bool,
) -> Result<(), AllocError> {
    const PWT: u64 = 1 << 3;
    const PCD: u64 = 1 << 4;
    if pml4_physical == 0 || pml4_physical >= PHYSICAL_LIMIT
        || pml4_physical & (PAGE_SIZE - 1) != 0
        || address >= PHYSICAL_LIMIT
    {
        return Err(AllocError::InvalidMap);
    }
    let address = address & !(PAGE_SIZE - 1);
    let pml4 = pml4_physical as *mut u64;
    let pml4e = unsafe { pml4.add(((address >> 39) & 0x1ff) as usize).read() };
    if pml4e & PRESENT == 0 { return Err(AllocError::InvalidMap); }
    let pdpt = (pml4e & ADDRESS_MASK) as *mut u64;
    let pdpte = unsafe { pdpt.add(((address >> 30) & 0x1ff) as usize).read() };
    if pdpte & PRESENT == 0 { return Err(AllocError::InvalidMap); }
    let pd = (pdpte & ADDRESS_MASK) as *mut u64;
    let pde_ptr = unsafe { pd.add(((address >> 21) & 0x1ff) as usize) };
    let pde = unsafe { pde_ptr.read() };
    if pde & PRESENT == 0 { return Err(AllocError::InvalidMap); }
    let table = if pde & LARGE != 0 {
        let table_phys = unsafe { allocate_page()? };
        let table = table_phys as *mut u64;
        let base = pde & LARGE_ADDRESS_MASK;
        let inherited = pde & (WRITABLE | NO_EXECUTE | PWT | PCD);
        for i in 0..512usize {
            unsafe { table.add(i).write(base + i as u64 * PAGE_SIZE | PRESENT | inherited); }
        }
        unsafe { pde_ptr.write(table_phys | PRESENT | WRITABLE); }
        table
    } else {
        (pde & ADDRESS_MASK) as *mut u64
    };
    let pte_ptr = unsafe { table.add(((address >> 12) & 0x1ff) as usize) };
    let mut pte = unsafe { pte_ptr.read() };
    if pte & PRESENT == 0 { return Err(AllocError::InvalidMap); }
    if writable { pte |= WRITABLE; } else { pte &= !WRITABLE; }
    if executable { pte &= !NO_EXECUTE; } else { pte |= NO_EXECUTE; }
    if cache_disable { pte |= PCD; } else { pte &= !PCD; }
    if write_through { pte |= PWT; } else { pte &= !PWT; }
    unsafe { pte_ptr.write(pte); }
    // Splitting a large page changes neighboring translations too; flush the
    // entire local TLB rather than invalidating only this single page.
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) pml4_physical, options(nostack, preserves_flags)); }
    Ok(())
}


#[cfg(test)]
mod allocator_map_tests {
    use super::*;

    fn descriptor(kind: u32, start: u64, pages: u64) -> MemoryDescriptor {
        MemoryDescriptor { kind, _padding: 0, physical_start: start,
            virtual_start: 0, number_of_pages: pages, attributes: 0 }
    }

    #[test]
    fn descriptor_stride_filtering_and_page_alignment() {
        let stride = core::mem::size_of::<MemoryDescriptor>() + 16;
        let mut bytes = vec![0xa5u8; stride * 3];
        let descriptors = [
            descriptor(EFI_CONVENTIONAL_MEMORY, 0x100003, 4),
            descriptor(6, 0x200000, 8),
            descriptor(EFI_CONVENTIONAL_MEMORY, 0x300000, 0),
        ];
        for (i, d) in descriptors.iter().enumerate() {
            unsafe { ptr::write_unaligned(bytes.as_mut_ptr().add(i * stride).cast::<MemoryDescriptor>(), *d); }
        }
        let mut regions = [Region::EMPTY; MAX_REGIONS];
        let count = collect_regions(bytes.as_ptr(), 3, stride, &mut regions).unwrap();
        assert_eq!(count, 1);
        assert_eq!(regions[0].next, 0x101000);
        assert_eq!(regions[0].end, 0x104000);
    }

    #[test]
    fn invalid_maps_are_rejected_without_publishing_regions() {
        let mut regions = [Region::EMPTY; MAX_REGIONS];
        assert_eq!(collect_regions(core::ptr::null(), 1, core::mem::size_of::<MemoryDescriptor>(), &mut regions), Err(AllocError::InvalidMap));
        assert_eq!(collect_regions(core::ptr::NonNull::<u8>::dangling().as_ptr(), 1, 8, &mut regions), Err(AllocError::InvalidMap));
        let d = descriptor(EFI_CONVENTIONAL_MEMORY, u64::MAX - 0xfff, 4);
        assert_eq!(collect_regions((&d as *const MemoryDescriptor).cast(), 1, core::mem::size_of::<MemoryDescriptor>(), &mut regions), Err(AllocError::InvalidMap));
    }

    #[test]
    fn region_capacity_is_checked_before_state_commit() {
        let stride = core::mem::size_of::<MemoryDescriptor>();
        let mut descriptors: Vec<_> = (0..=MAX_REGIONS).map(|i| descriptor(EFI_CONVENTIONAL_MEMORY, 0x100000 + (i as u64) * 0x2000, 1)).collect();
        let mut regions = [Region::EMPTY; MAX_REGIONS];
        assert_eq!(collect_regions(descriptors.as_mut_ptr().cast(), descriptors.len(), stride, &mut regions), Err(AllocError::TooManyRegions));
    }

    #[test]
    fn high_regions_clip_to_direct_map_and_filter_nonconventional_memory() {
        let descriptors = [
            descriptor(EFI_CONVENTIONAL_MEMORY, 0xffff_f000, 4),
            descriptor(6, 0x1_0000_0000, 16),
            descriptor(EFI_CONVENTIONAL_MEMORY, DIRECT_MAP_PHYSICAL_LIMIT - PAGE_SIZE, 4),
        ];
        let mut regions = [Region::EMPTY; MAX_REGIONS];
        let count = collect_high_regions(
            descriptors.as_ptr().cast(), descriptors.len(),
            core::mem::size_of::<MemoryDescriptor>(), &mut regions,
        ).unwrap();
        assert_eq!(count, 2);
        assert_eq!(regions[0].next, PHYSICAL_LIMIT);
        assert_eq!(regions[0].end, PHYSICAL_LIMIT + 3 * PAGE_SIZE);
        assert_eq!(regions[1].next, DIRECT_MAP_PHYSICAL_LIMIT - PAGE_SIZE);
        assert_eq!(regions[1].end, DIRECT_MAP_PHYSICAL_LIMIT);
    }

    #[test]
    fn alignment_helper_rejects_overflow_and_rounds_up() {
        assert_eq!(align_up(0x1001, PAGE_SIZE), Some(0x2000));
        assert_eq!(align_up(0x2000, PAGE_SIZE), Some(0x2000));
        assert_eq!(align_up(u64::MAX, PAGE_SIZE), None);
    }

    #[test]
    fn overlapping_descriptors_are_rejected_to_prevent_double_allocation() {
        let descriptors = [
            descriptor(EFI_CONVENTIONAL_MEMORY, 0x100000, 4),
            descriptor(EFI_CONVENTIONAL_MEMORY, 0x102000, 4),
        ];
        let mut regions = [Region::EMPTY; MAX_REGIONS];
        assert_eq!(collect_regions(descriptors.as_ptr().cast(), descriptors.len(),
            core::mem::size_of::<MemoryDescriptor>(), &mut regions),
            Err(AllocError::InvalidMap));
    }

    #[test]
    fn adjacent_descriptors_and_empty_normalized_ranges_are_handled() {
        let descriptors = [
            descriptor(EFI_CONVENTIONAL_MEMORY, 0x100000, 1),
            descriptor(EFI_CONVENTIONAL_MEMORY, 0x101000, 2),
            descriptor(EFI_CONVENTIONAL_MEMORY, 0x200001, 1),
        ];
        let mut regions = [Region::EMPTY; MAX_REGIONS];
        let count = collect_regions(descriptors.as_ptr().cast(), descriptors.len(),
            core::mem::size_of::<MemoryDescriptor>(), &mut regions).unwrap();
        assert_eq!(count, 2);
        assert_eq!((regions[0].next, regions[0].end), (0x100000, 0x101000));
        assert_eq!((regions[1].next, regions[1].end), (0x101000, 0x103000));
    }

    #[test]
    fn nonconventional_overflowing_descriptors_are_ignored() {
        let descriptors = [
            descriptor(6, u64::MAX - 0xfff, u64::MAX),
            descriptor(EFI_CONVENTIONAL_MEMORY, 0x100000, 1),
        ];
        let mut regions = [Region::EMPTY; MAX_REGIONS];
        assert_eq!(collect_regions(descriptors.as_ptr().cast(), descriptors.len(),
            core::mem::size_of::<MemoryDescriptor>(), &mut regions).unwrap(), 1);
    }
}

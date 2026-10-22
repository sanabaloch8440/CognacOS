//! Early physical-page allocator built only from UEFI EfiConventionalMemory ranges.
//! This allocator is single-threaded and intentionally capped at the first 4 GiB
//! until direct-map and high-memory support are implemented.

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

#[derive(Debug, Clone, Copy)]
pub enum AllocError { InvalidMap, TooManyRegions, OutOfMemory }

fn align_up(value: u64, align: u64) -> Option<u64> {
    value.checked_add(align - 1).map(|v| v & !(align - 1))
}

/// Initialize from the final UEFI map. Descriptor stride is supplied by firmware
/// and is not assumed to equal Rust's descriptor struct size.
pub unsafe fn initialize(map: *const u8, entries: usize, stride: usize) -> Result<usize, AllocError> {
    if (entries != 0 && map.is_null()) || stride < core::mem::size_of::<MemoryDescriptor>() {
        return Err(AllocError::InvalidMap);
    }
    let regions = unsafe { &mut *REGIONS.0.get() };
    for r in regions.iter_mut() { *r = Region::EMPTY; }
    let mut count = 0usize;
    for i in 0..entries {
        let address = (map as usize).checked_add(i.checked_mul(stride).ok_or(AllocError::InvalidMap)?)
            .ok_or(AllocError::InvalidMap)? as *const MemoryDescriptor;
        let d = unsafe { ptr::read_unaligned(address) };
        if d.kind != EFI_CONVENTIONAL_MEMORY || d.number_of_pages == 0 { continue; }
        let start = d.physical_start.max(LOW_RESERVED_END);
        let raw_end = d.physical_start.checked_add(d.number_of_pages.checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidMap)?)
            .ok_or(AllocError::InvalidMap)?;
        let end = raw_end.min(PHYSICAL_LIMIT);
        let Some(start) = align_up(start, PAGE_SIZE) else { continue };
        let end = end & !(PAGE_SIZE - 1);
        if start >= end { continue; }
        if count == MAX_REGIONS { return Err(AllocError::TooManyRegions); }
        regions[count] = Region { next: start, end };
        count += 1;
    }
    REGION_COUNT.store(count, core::sync::atomic::Ordering::Release);
    Ok(count)
}

/// Allocate one zeroed physical page. The early kernel uses identity mappings,
/// so the returned physical address is also directly writable as a pointer.
pub unsafe fn allocate_page() -> Result<u64, AllocError> {
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

pub fn region_count() -> usize {
    REGION_COUNT.load(core::sync::atomic::Ordering::Acquire)
}

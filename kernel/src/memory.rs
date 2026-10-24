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

/// Convert a high physical address to its canonical direct-map virtual address.
/// Mapping is demand-created on first access; only UEFI-described RAM is eligible.
pub fn direct_map_address(physical_address: u64) -> Option<u64> {
    if physical_address < PHYSICAL_LIMIT
        || physical_address >= DIRECT_MAP_PHYSICAL_LIMIT
        || !is_ram_physical_address(physical_address)
    { return None; }
    DIRECT_MAP_BASE.checked_add(physical_address)
}

/// Return a virtual pointer suitable for CPU access to a physical RAM page.
/// Low memory uses its identity alias; high memory uses the fault-created direct map.
pub fn physical_access_pointer(physical_address: u64) -> Option<*mut u8> {
    if physical_address < PHYSICAL_LIMIT {
        Some(physical_address as *mut u8)
    } else {
        direct_map_address(physical_address).map(|address| address as *mut u8)
    }
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
pub enum AllocError { InvalidMap, TooManyRegions, OutOfMemory, InvalidPage, FreeListFull }

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
    Ok(count)
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
pub unsafe fn free_page(page: u64) -> Result<(), AllocError> {
    if page >= PHYSICAL_LIMIT {
        if page >= DIRECT_MAP_PHYSICAL_LIMIT || page & (PAGE_SIZE - 1) != 0
            || !is_ram_physical_address(page) {
            return Err(AllocError::InvalidPage);
        }
        let _guard = MemoryGuard::acquire();
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

/// Inspect the active bootstrap's four-level page tables without allocating or
/// modifying anything. Supports 1 GiB, 2 MiB and 4 KiB leaves. The caller must
/// ensure the root and every referenced table are identity-accessible.
pub unsafe fn inspect_mapping(pml4_physical: u64, virtual_address: u64)
    -> Result<MappingInfo, AllocError>
{
    const USER: u64 = 1 << 2;
    if pml4_physical == 0 || pml4_physical >= PHYSICAL_LIMIT
        || pml4_physical & (PAGE_SIZE - 1) != 0
        || !(virtual_address <= 0x0000_7fff_ffff_ffff
            || virtual_address >= 0xffff_8000_0000_0000)
    {
        return Err(AllocError::InvalidMap);
    }
    let pml4 = pml4_physical as *const u64;
    let pml4e = unsafe { pml4.add(((virtual_address >> 39) & 0x1ff) as usize).read() };
    if pml4e & PRESENT == 0 || pml4e & LARGE != 0 { return Err(AllocError::InvalidMap); }
    let pdpt = (pml4e & ADDRESS_MASK) as *const u64;
    let pdpte = unsafe { pdpt.add(((virtual_address >> 30) & 0x1ff) as usize).read() };
    if pdpte & PRESENT == 0 { return Err(AllocError::InvalidMap); }
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
    if pde & PRESENT == 0 { return Err(AllocError::InvalidMap); }
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
    if pte & PRESENT == 0 { return Err(AllocError::InvalidMap); }
    Ok(MappingInfo {
        physical_address: (pte & ADDRESS_MASK) | (virtual_address & (PAGE_SIZE - 1)),
        page_size: PAGE_SIZE,
        writable: pml4e & pdpte & pde & pte & WRITABLE != 0,
        executable: (pml4e | pdpte | pde | pte) & NO_EXECUTE == 0,
        user_accessible: pml4e & pdpte & pde & pte & USER != 0,
    })
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
    fn alignment_helper_rejects_overflow_and_rounds_up() {
        assert_eq!(align_up(0x1001, PAGE_SIZE), Some(0x2000));
        assert_eq!(align_up(0x2000, PAGE_SIZE), Some(0x2000));
        assert_eq!(align_up(u64::MAX, PAGE_SIZE), None);
    }
}

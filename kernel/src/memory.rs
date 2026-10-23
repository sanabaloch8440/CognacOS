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
const FREE_CAPACITY: usize = 65536;
struct FreePages(core::cell::UnsafeCell<([u64; FREE_CAPACITY], usize)>);
unsafe impl Sync for FreePages {}
static FREE_PAGES: FreePages = FreePages(core::cell::UnsafeCell::new(([0; FREE_CAPACITY], 0)));

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
pub unsafe fn initialize(map: *const u8, entries: usize, stride: usize) -> Result<usize, AllocError> {
    let _guard = MemoryGuard::acquire();

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

/// Allocate a physically contiguous, zeroed run of pages from one conventional
/// memory region. The early allocator is single-threaded; callers must use this
/// before APs are enabled or otherwise serialize allocation.
pub unsafe fn allocate_contiguous_pages(pages: usize) -> Result<u64, AllocError> {
    let _guard = MemoryGuard::acquire();

    if pages == 0 { return Err(AllocError::InvalidPage); }
    let bytes = (pages as u64).checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidPage)?;
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

/// Return a previously allocated low physical page to the single-core free list.
/// The caller must ensure no active mapping or DMA operation references the page.
pub unsafe fn free_page(page: u64) -> Result<(), AllocError> {
    let _guard = MemoryGuard::acquire();

    if page < LOW_RESERVED_END || page >= PHYSICAL_LIMIT || page & (PAGE_SIZE - 1) != 0 {
        return Err(AllocError::InvalidPage);
    }
    let free = unsafe { &mut *FREE_PAGES.0.get() };
    if free.1 == FREE_CAPACITY { return Err(AllocError::FreeListFull); }
    free.0[free.1] = page;
    free.1 += 1;
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
    if pages > FREE_CAPACITY - free.1 { return Err(AllocError::FreeListFull); }
    let mut page = base;
    while page < end {
        free.0[free.1] = page;
        free.1 += 1;
        page += PAGE_SIZE;
    }
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

/// Split the bootstrap's 2 MiB identity mapping into 4 KiB pages where
/// fine-grained permissions or a stack guard are required. All other bootstrap
/// large pages become writable, supervisor-only, and non-executable.
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

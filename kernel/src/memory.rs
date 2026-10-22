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
                        unsafe { pte_ptr.write(pte | NO_EXECUTE | WRITABLE); }
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

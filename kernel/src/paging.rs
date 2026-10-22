//! Bootstrap identity page tables: 0..4 GiB mapped with 2 MiB pages.
//! Permissions are intentionally permissive for this transition stage; later
//! work replaces these mappings with per-section W^X/NX policies and a direct map.

use crate::memory::{self, AllocError};

const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const LARGE_PAGE: u64 = 1 << 7;
const TWO_MIB: u64 = 2 * 1024 * 1024;
const ENTRIES: usize = 512;
const IDENTITY_PD_COUNT: usize = 4;

#[derive(Debug, Clone, Copy)]
pub enum PagingError { Allocation(AllocError) }

impl From<AllocError> for PagingError {
    fn from(value: AllocError) -> Self { Self::Allocation(value) }
}

/// Construct a fresh PML4 and identity-map the first 4 GiB. This keeps the
/// loaded kernel, its stack, the boot-info structure and UEFI map reachable
/// while CR3 changes. The allocator only returns EfiConventionalMemory pages.
pub unsafe fn build_bootstrap_identity_map() -> Result<u64, PagingError> {
    let pml4 = unsafe { memory::allocate_page()? };
    let pdpt = unsafe { memory::allocate_page()? };
    let mut page_directories = [0u64; IDENTITY_PD_COUNT];
    for pd in page_directories.iter_mut() {
        *pd = unsafe { memory::allocate_page()? };
    }

    unsafe {
        (pml4 as *mut u64).add(0).write(pdpt | PRESENT | WRITABLE);
        let pdpt_ptr = pdpt as *mut u64;
        for index in 0..IDENTITY_PD_COUNT {
            pdpt_ptr.add(index).write(page_directories[index] | PRESENT | WRITABLE);
            let pd = page_directories[index] as *mut u64;
            for entry in 0..ENTRIES {
                let physical = (index as u64 * ENTRIES as u64 + entry as u64) * TWO_MIB;
                pd.add(entry).write(physical | PRESENT | WRITABLE | LARGE_PAGE);
            }
        }
    }
    Ok(pml4)
}

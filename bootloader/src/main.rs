#![no_main]
#![no_std]

extern crate alloc;

mod elf;
mod sha256;

include!(concat!(env!("OUT_DIR"), "/kernel_digest.rs"));

use alloc::vec;
use uefi::prelude::*;
use uefi::mem::memory_map::MemoryMap;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::{boot, cstr16};

const KERNEL_PATH: &uefi::CStr16 = cstr16!("\\EFI\\BOOT\\rdotos-kernel.elf");
const MAX_KERNEL_FILE: usize = 32 * 1024 * 1024;
const KERNEL_STACK_PAGES: usize = 64;
const KERNEL_STACK_ALLOCATION_PAGES: usize = KERNEL_STACK_PAGES + 1;
const LOW_MEMORY_MAP_PAGES: usize = 64;
const BOOT_INFO_MAGIC: u64 = 0x5244_4F54_4F53_4249; // "RDOTOSBI"
const BOOT_INFO_VERSION: u32 = 5;
const HIGH_HALF_PT_PAGES: usize = 64;
const KERNEL_STACK_VIRTUAL_GUARD: u64 = 0xffff_ff00_0000_0000;

#[repr(C)]
struct BootInfo {
    magic: u64,
    version: u32,
    struct_size: u32,
    pml4_physical: u64,
    memory_map: *const uefi::boot::MemoryDescriptor,
    memory_map_entries: usize,
    memory_descriptor_size: usize,
    memory_descriptor_version: u32,
    kernel_stack_guard_page: u64,
    kernel_stack_pages: u64,
    kernel_stack_virtual_guard: u64,
    rsdp_address: u64,
    ap_trampoline_physical: u64,
}

#[entry]
fn main() -> Status {
    if let Err(error) = uefi::helpers::init() { return error.status(); }
    let image_handle = boot::image_handle();
    uefi::println!("RdotOS Rust UEFI bootloader");
    uefi::println!("Opening kernel ELF: \\EFI\\BOOT\\rdotos-kernel.elf");

    let mut fs = match boot::get_image_file_system(image_handle) {
        Ok(fs) => fs,
        Err(err) => { uefi::println!("Cannot access boot volume: {:?}", err); return err.status(); }
    };
    let mut root = match fs.open_volume() {
        Ok(root) => root,
        Err(err) => { uefi::println!("Cannot open boot volume: {:?}", err); return err.status(); }
    };
    let handle = match root.open(KERNEL_PATH, FileMode::Read, FileAttribute::empty()) {
        Ok(file) => file,
        Err(err) => { uefi::println!("Cannot open kernel ELF: {:?}", err); return err.status(); }
    };
    let mut kernel_file = match handle.into_regular_file() {
        Some(file) => file,
        None => { uefi::println!("Kernel path is not a regular file"); return Status::LOAD_ERROR; }
    };

    // Bound file size to prevent a corrupt filesystem image from forcing an
    // unbounded allocation. A full-file read is required before parsing offsets.
    let mut image = vec![0u8; MAX_KERNEL_FILE];
    let mut used = 0usize;
    loop {
        if used == image.len() {
            uefi::println!("Kernel ELF exceeds the 32 MiB limit");
            return Status::BAD_BUFFER_SIZE;
        }
        match kernel_file.read(&mut image[used..]) {
            Ok(0) => break,
            Ok(n) => used += n,
            Err(err) => { uefi::println!("Kernel ELF read failed: {:?}", err); return err.status(); }
        }
    }
    image.truncate(used);
    let Some(expected_digest) = EXPECTED_KERNEL_SHA256 else {
        uefi::println!("FATAL: no build-time kernel trust anchor; refusing unsigned/unpinned kernel");
        return Status::SECURITY_VIOLATION;
    };
    let actual_digest = sha256::digest(&image);
    if !sha256::constant_time_eq(&actual_digest, &expected_digest) {
        uefi::println!("FATAL: kernel SHA-256 mismatch; component rejected");
        return Status::SECURITY_VIOLATION;
    }
    uefi::println!("Kernel SHA-256 matches the bootloader's build-time trust anchor");
    drop(kernel_file);
    drop(root);
    drop(fs);

    uefi::println!("Read {} bytes; validating ELF64 headers and load segments", image.len());
    let loaded_kernel = match unsafe { elf::load(&image) } {
        Ok(kernel) => kernel,
        Err(err) => { uefi::println!("Rejected kernel ELF: {:?}", err); return Status::LOAD_ERROR; }
    };
    let entry = loaded_kernel.entry;
    // No heap-backed value or protocol handle may be dropped after this point.
    // Leak the file buffer so allocator cleanup cannot call boot services.
    core::mem::forget(image);

    uefi::println!("Kernel segments loaded; entry = {:#x}", entry);
    let stack = match boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xffff_ffff),
        boot::MemoryType::LOADER_DATA,
        KERNEL_STACK_ALLOCATION_PAGES,
    ) {
        Ok(stack) => stack,
        Err(err) => { uefi::println!("Kernel stack allocation failed: {:?}", err); return err.status(); }
    };
    // The first allocated page is deliberately left unmapped as a stack guard.
    let kernel_stack_guard_page = stack.as_ptr() as u64;
    let stack_top = kernel_stack_guard_page + (KERNEL_STACK_ALLOCATION_PAGES * boot::PAGE_SIZE) as u64;
    // SysV x86-64 function-entry alignment: RSP % 16 == 8, with a dummy
    // return slot because the kernel entry is reached by a tail jump.
    unsafe { ((stack_top - 8) as *mut u64).write(0); }
    // The physical stack remains identity-mapped for bootstrap safety, while
    // the kernel enters on a separate high-half alias with an unmapped guard.
    let kernel_stack = KERNEL_STACK_VIRTUAL_GUARD
        + (KERNEL_STACK_ALLOCATION_PAGES * boot::PAGE_SIZE) as u64 - 8;
    uefi::println!("Allocated {} KiB kernel stack plus one unmapped guard page", KERNEL_STACK_PAGES * 4);

    // Allocate bootstrap page tables while UEFI allocation services are still
    // available. Keep every table below 4 GiB because the initial map is identity.
    let (pml4_physical, pt_pool_physical) = match build_bootstrap_page_tables() {
        Ok(addresses) => addresses,
        Err(status) => { uefi::println!("Page-table allocation failed: {:?}", status); return status; }
    };
    uefi::println!("Prepared kernel PML4 at {:#x}; 4 KiB PT pool at {:#x}",
        pml4_physical, pt_pool_physical);
    let high_pt_pool = match boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xffff_ffff),
        boot::MemoryType::LOADER_DATA,
        HIGH_HALF_PT_PAGES,
    ) {
        Ok(pool) => {
            unsafe { core::ptr::write_bytes(pool.as_ptr(), 0, HIGH_HALF_PT_PAGES * boot::PAGE_SIZE); }
            pool.as_ptr() as u64
        }
        Err(error) => { uefi::println!("High-half page-table pool allocation failed: {:?}", error); return error.status(); }
    };
    let transition_page_physical = match allocate_boot_page() {
        Ok(page) => page,
        Err(error) => { uefi::println!("Low transition trampoline allocation failed: {:?}", error); return error; }
    };
    // Position-independent trampoline bytes: mov cr3,rcx; mov rsp,rsi;
    // xor rbp,rbp; jmp rax. It executes from a low identity-mapped page after
    // the firmware-loaded bootloader image (which may reside above 4 GiB)
    // jumps to it, so no instruction fetch depends on the old CR3 after switch.
    unsafe {
        let trampoline = transition_page_physical as *mut u8;
        let bytes = [0x0f, 0x22, 0xd9, 0x48, 0x89, 0xf4, 0x48, 0x31, 0xed, 0xff, 0xe0];
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), trampoline, bytes.len());
    }
    // Reserve low identity-mapped storage for the final map and boot-info.
    // The UEFI memory-map helper may return a buffer outside the bootstrap map.
    let map_copy = match boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xffff_ffff),
        boot::MemoryType::LOADER_DATA,
        LOW_MEMORY_MAP_PAGES,
    ) {
        Ok(p) => p,
        Err(err) => { uefi::println!("Low memory-map buffer allocation failed: {:?}", err); return err.status(); }
    };
    let boot_info_storage = match boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xffff_ffff),
        boot::MemoryType::LOADER_DATA,
        1,
    ) {
        Ok(p) => p,
        Err(err) => { uefi::println!("Low boot-info allocation failed: {:?}", err); return err.status(); }
    };
    // Reserve a SIPI-vector page below 1 MiB now, while UEFI allocation is
    // available. It is only storage at this stage; the kernel must copy and
    // validate a position-independent trampoline before sending any SIPI.
    let ap_trampoline_physical = match boot::allocate_pages(
        boot::AllocateType::MaxAddress(0x000f_ffff),
        boot::MemoryType::LOADER_DATA,
        1,
    ) {
        Ok(page) => {
            unsafe { core::ptr::write_bytes(page.as_ptr(), 0, boot::PAGE_SIZE); }
            uefi::println!("Reserved low-memory AP trampoline page at {:#x}", page.as_ptr() as u64);
            page.as_ptr() as u64
        }
        Err(err) => {
            uefi::println!("WARNING: no low-memory AP trampoline page; single-CPU fallback: {:?}", err);
            0
        }
    };

    let rsdp_address = find_rsdp();
    if rsdp_address == 0 {
        uefi::println!("WARNING: no ACPI RSDP found in UEFI configuration tables");
    } else {
        uefi::println!("Found ACPI RSDP at {:#x}", rsdp_address);
    }

    uefi::println!("Exiting UEFI boot services");
    // SAFETY: All filesystem protocols are closed. The helper retries a stale map key.
    let memory_map = unsafe { boot::exit_boot_services(None) };
    let descriptor_size = memory_map.meta().desc_size;
    let map_bytes = match memory_map.len().checked_mul(descriptor_size) {
        Some(bytes) if bytes <= LOW_MEMORY_MAP_PAGES * boot::PAGE_SIZE => bytes,
        _ => loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } },
    };
    unsafe {
        core::ptr::copy_nonoverlapping(
            memory_map.buffer().as_ptr().cast::<u8>(),
            map_copy.as_ptr(),
            map_bytes,
        );
    }
    if unsafe { populate_bootstrap_page_tables(
        pml4_physical,
        pt_pool_physical,
        map_copy.as_ptr().cast::<u8>(),
        memory_map.len(),
        descriptor_size,
    ) }.is_err() {
        uefi::println!("FATAL: could not build explicit identity mappings from final UEFI map");
        loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
    }
    if unsafe { populate_high_half_mappings(
        pml4_physical, high_pt_pool, &loaded_kernel, kernel_stack_guard_page,
    ) }.is_err() {
        uefi::println!("FATAL: could not establish high-half kernel/stack aliases");
        loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
    }
    let boot_info = BootInfo {
        magic: BOOT_INFO_MAGIC,
        version: BOOT_INFO_VERSION,
        struct_size: core::mem::size_of::<BootInfo>() as u32,
        pml4_physical,
        memory_map: map_copy.as_ptr().cast::<uefi::boot::MemoryDescriptor>(),
        memory_map_entries: memory_map.len(),
        memory_descriptor_size: descriptor_size,
        memory_descriptor_version: memory_map.meta().desc_version,
        kernel_stack_guard_page,
        kernel_stack_pages: KERNEL_STACK_PAGES as u64,
        kernel_stack_virtual_guard: KERNEL_STACK_VIRTUAL_GUARD,
        rsdp_address,
        ap_trampoline_physical,
    };
    let boot_info_ptr = boot_info_storage.as_ptr().cast::<BootInfo>();
    unsafe { boot_info_ptr.write(boot_info); }
    // Refuse the handoff unless each live physical dependency has an explicit
    // identity leaf in the new tables. Validate all pages of the kernel stack.
    let stack_range_ok = (0..KERNEL_STACK_ALLOCATION_PAGES).all(|page| unsafe {
        bootstrap_identity_mapped(pml4_physical, kernel_stack_guard_page + page as u64 * boot::PAGE_SIZE as u64)
    });
    let dependencies = [
        pml4_physical,
        pt_pool_physical,
        high_pt_pool,
        transition_page_physical,
        map_copy.as_ptr() as u64,
        boot_info_storage.as_ptr() as u64,
        kernel_stack_guard_page,
        if rsdp_address < 0x1_0000_0000 { rsdp_address } else { 0 },
        ap_trampoline_physical,
    ];
    let kernel_aliases_ok = loaded_kernel.segments[..loaded_kernel.segment_count].iter().all(|segment| {
        let va = segment.virtual_start;
        let pa = segment.physical_start;
        unsafe { bootstrap_physical_at(pml4_physical, va) == Some(pa) }
    }) && unsafe {
        bootstrap_physical_at(pml4_physical, entry).is_some()
            && bootstrap_physical_at(pml4_physical, KERNEL_STACK_VIRTUAL_GUARD + boot::PAGE_SIZE as u64)
                == Some(kernel_stack_guard_page + boot::PAGE_SIZE as u64)
            && bootstrap_physical_at(pml4_physical, KERNEL_STACK_VIRTUAL_GUARD).is_none()
    };
    if !stack_range_ok || !kernel_aliases_ok
        || dependencies.into_iter().filter(|address| *address != 0)
            .any(|address| unsafe { !bootstrap_identity_mapped(pml4_physical, address) }) {
        uefi::println!("FATAL: final UEFI map omitted a live bootstrap dependency or high-half alias");
        loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
    }
    core::mem::forget(memory_map);

    // x86-64 System V kernel ABI: BootInfo pointer in RDI. This first-stage
    // kernel is linked below 1 GiB and uses the firmware's existing identity map.
    // SAFETY: ELF validation ensured the entry lies inside a loaded PT_LOAD range.
    unsafe {
        core::arch::asm!(
            "cli",
            "cld",
            "jmp rdx",
            in("rax") entry,
            in("rdi") boot_info_ptr,
            in("rsi") kernel_stack,
            in("rcx") pml4_physical,
            in("rdx") transition_page_physical,
            options(noreturn)
        );
    }
}

/// Prefer the ACPI 2.0 configuration table and fall back to ACPI 1.0.
/// UEFI configuration-table addresses remain physical across ExitBootServices;
/// this bootstrap only accepts an RSDP inside its low 4 GiB identity map.
fn find_rsdp() -> u64 {
    let acpi2 = core::cell::Cell::new(0u64);
    let acpi1 = core::cell::Cell::new(0u64);
    uefi::system::with_config_table(|entries| {
        for entry in entries {
            if entry.address.is_null() { continue; }
            if entry.guid == uefi::table::cfg::ACPI2_GUID {
                acpi2.set(entry.address as u64);
            } else if entry.guid == uefi::table::cfg::ACPI_GUID {
                acpi1.set(entry.address as u64);
            }
        }
    });
    let candidate = if acpi2.get() != 0 { acpi2.get() } else { acpi1.get() };
    if candidate < 0x1_0000_0000 { candidate } else { 0 }
}

/// Allocate a zeroed page below 4 GiB using UEFI services before the handoff.
fn allocate_boot_page() -> Result<u64, Status> {
    let page = boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xffff_ffff),
        boot::MemoryType::LOADER_DATA,
        1,
    ).map_err(|error| error.status())?;
    unsafe { core::ptr::write_bytes(page.as_ptr(), 0, boot::PAGE_SIZE); }
    Ok(page.as_ptr() as u64)
}

const LOW_IDENTITY_PT_PAGES: usize = 2048; // one PT per 2 MiB in the first 4 GiB

/// Allocate a four-level identity map with a 4 KiB PT for every 2 MiB window.
/// This avoids mapping reserved holes just because they share a large page.
fn build_bootstrap_page_tables() -> Result<(u64, u64), Status> {
    const PRESENT_WRITE: u64 = 0x003;
    let pml4 = allocate_boot_page()?;
    let pdpt = allocate_boot_page()?;
    let mut dirs = [0u64; 4];
    for dir in dirs.iter_mut() { *dir = allocate_boot_page()?; }
    let pt_pool = boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xffff_ffff),
        boot::MemoryType::LOADER_DATA,
        LOW_IDENTITY_PT_PAGES,
    ).map_err(|error| error.status())?;
    unsafe {
        core::ptr::write_bytes(pt_pool.as_ptr(), 0, LOW_IDENTITY_PT_PAGES * boot::PAGE_SIZE);
        (pml4 as *mut u64).write(pdpt | PRESENT_WRITE);
        for index in 0..4 {
            (pdpt as *mut u64).add(index).write(dirs[index] | PRESENT_WRITE);
            let directory = dirs[index] as *mut u64;
            for slot in 0..512 {
                let physical = pt_pool.as_ptr() as u64
                    + ((index * 512 + slot) * boot::PAGE_SIZE) as u64;
                directory.add(slot).write(physical | PRESENT_WRITE);
            }
        }
    }
    Ok((pml4, pt_pool.as_ptr() as u64))
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawMemoryDescriptor {
    kind: u32,
    _padding: u32,
    physical_start: u64,
    virtual_start: u64,
    number_of_pages: u64,
    attributes: u64,
}

/// Populate identity mappings only for physical regions described by the final
/// UEFI map. The 2 MiB leaf granularity is retained for bootstrap simplicity;
/// uncovered physical holes remain not-present. All mapped regions start
/// supervisor/writable; kernel section permissions are a later kernel phase.
unsafe fn populate_bootstrap_page_tables(
    pml4_physical: u64, pt_pool_physical: u64,
    map: *const u8, entries: usize, stride: usize,
) -> Result<(), ()> {
    const PRESENT_WRITE: u64 = 0x003;
    const FOUR_GIB: u64 = 0x1_0000_0000;
    const PWT: u64 = 1 << 3;
    const PCD: u64 = 1 << 4;
    if pml4_physical == 0 || pml4_physical >= FOUR_GIB || pml4_physical & 0xfff != 0
        || (entries != 0 && map.is_null())
        || stride < core::mem::size_of::<RawMemoryDescriptor>()
        || entries.checked_mul(stride).is_none()
    { return Err(()); }

    let pml4 = pml4_physical as *mut u64;
    let root_entry = unsafe { pml4.read() };
    if root_entry & 1 == 0 { return Err(()); }
    let pdpt = (root_entry & 0x000f_ffff_ffff_f000) as *mut u64;
    for i in 0..entries {
        let offset = i.checked_mul(stride).ok_or(())?;
        let address = (map as usize).checked_add(offset).ok_or(())?;
        let descriptor = unsafe { core::ptr::read_unaligned(address as *const RawMemoryDescriptor) };
        if descriptor.number_of_pages == 0 { continue; }
        if descriptor.physical_start & 0xfff != 0 { return Err(()); }
        let bytes = descriptor.number_of_pages.checked_mul(4096).ok_or(())?;
        let raw_end = descriptor.physical_start.checked_add(bytes).ok_or(())?;
        let mut physical = descriptor.physical_start;
        let end = raw_end.min(FOUR_GIB);
        if physical >= end || physical >= FOUR_GIB { continue; }
        while physical < end {
            let pt_index = (physical >> 21) as usize;
            if pt_index >= LOW_IDENTITY_PT_PAGES { return Err(()); }
            let table = (pt_pool_physical + pt_index as u64 * 4096) as *mut u64;
            let slot = ((physical >> 12) & 0x1ff) as usize;
            if unsafe { table.add(slot).read() } & 1 != 0 { return Err(()); }
            let cache_flags = if physical == 0xfec0_0000 || physical == 0xfee0_0000 {
                PWT | PCD
            } else { 0 };
            unsafe { table.add(slot).write(physical | PRESENT_WRITE | cache_flags); }
            physical = physical.checked_add(4096).ok_or(())?;
        }
    }

    // Firmware may omit APIC MMIO windows from its descriptor list. Map only
    // the exact 4 KiB register pages, not surrounding physical apertures.
    for physical in [0xfec0_0000u64, 0xfee0_0000u64] {
        let table = (pt_pool_physical + ((physical >> 21) * 4096)) as *mut u64;
        let slot = ((physical >> 12) & 0x1ff) as usize;
        unsafe { table.add(slot).write(physical | PRESENT_WRITE | PWT | PCD); }
    }
    Ok(())
}

unsafe fn allocate_high_table(pool: u64, used: &mut usize) -> Result<u64, ()> {
    if *used >= HIGH_HALF_PT_PAGES { return Err(()); }
    let page = pool.checked_add((*used as u64).checked_mul(4096).ok_or(())?).ok_or(())?;
    *used += 1;
    unsafe { core::ptr::write_bytes(page as *mut u8, 0, 4096); }
    Ok(page)
}

unsafe fn ensure_high_child(
    parent: *mut u64, index: usize, pool: u64, used: &mut usize,
) -> Result<*mut u64, ()> {
    const PRESENT_WRITE: u64 = 3;
    const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
    let slot = unsafe { parent.add(index) };
    let mut entry = unsafe { slot.read() };
    if entry & 1 == 0 {
        let child = unsafe { allocate_high_table(pool, used)? };
        entry = child | PRESENT_WRITE;
        unsafe { slot.write(entry); }
    } else if entry & (1 << 7) != 0 {
        return Err(());
    }
    Ok((entry & ADDRESS_MASK) as *mut u64)
}

/// Map one high virtual page to its low physical backing using preallocated
/// page-table pages; this runs after ExitBootServices and cannot allocate via UEFI.
unsafe fn map_high_page(
    pml4_physical: u64, pool: u64, used: &mut usize, virtual_address: u64, physical_address: u64,
) -> Result<(), ()> {
    const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
    const PRESENT_WRITE: u64 = 3;
    if virtual_address < 0xffff_8000_0000_0000
        || virtual_address & 0xfff != physical_address & 0xfff
        || physical_address >= 0x1_0000_0000
    { return Err(()); }
    let pml4 = pml4_physical as *mut u64;
    let pdpt = unsafe { ensure_high_child(pml4, ((virtual_address >> 39) & 0x1ff) as usize, pool, used)? };
    let pd = unsafe { ensure_high_child(pdpt, ((virtual_address >> 30) & 0x1ff) as usize, pool, used)? };
    let pt = unsafe { ensure_high_child(pd, ((virtual_address >> 21) & 0x1ff) as usize, pool, used)? };
    let leaf = unsafe { pt.add(((virtual_address >> 12) & 0x1ff) as usize) };
    if unsafe { leaf.read() } & 1 != 0 { return Err(()); }
    unsafe { leaf.write((physical_address & ADDRESS_MASK) | PRESENT_WRITE); }
    Ok(())
}

unsafe fn populate_high_half_mappings(
    pml4_physical: u64, pool: u64, kernel: &elf::LoadedKernel, stack_guard_physical: u64,
) -> Result<(), ()> {
    let mut used = 0usize;
    for segment in kernel.segments[..kernel.segment_count].iter().copied() {
        if segment.virtual_start & 0xfff != segment.physical_start & 0xfff { return Err(()); }
        let virtual_start = segment.virtual_start & !0xfff;
        let physical_start = segment.physical_start & !0xfff;
        let end = segment.virtual_start.checked_add(segment.memory_size).ok_or(())?;
        let virtual_end = end.checked_add(0xfff).ok_or(())? & !0xfff;
        let mut va = virtual_start;
        while va < virtual_end {
            let pa = physical_start.checked_add(va - virtual_start).ok_or(())?;
            unsafe { map_high_page(pml4_physical, pool, &mut used, va, pa)?; }
            va = va.checked_add(4096).ok_or(())?;
        }
    }
    // Leave the first virtual stack page unmapped as a guard. Physical identity
    // mappings remain until kernel hardening has validated the new stack.
    for page in 1..KERNEL_STACK_ALLOCATION_PAGES {
        let va = KERNEL_STACK_VIRTUAL_GUARD + page as u64 * 4096;
        let pa = stack_guard_physical + page as u64 * 4096;
        unsafe { map_high_page(pml4_physical, pool, &mut used, va, pa)?; }
    }
    Ok(())
}

unsafe fn bootstrap_physical_at(pml4_physical: u64, address: u64) -> Option<u64> {
    const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
    let pml4 = pml4_physical as *const u64;
    let pml4e = unsafe { pml4.add(((address >> 39) & 0x1ff) as usize).read() };
    if pml4e & 1 == 0 { return None; }
    let pdpt = (pml4e & ADDRESS_MASK) as *const u64;
    let pdpte = unsafe { pdpt.add(((address >> 30) & 0x1ff) as usize).read() };
    if pdpte & 1 == 0 { return None; }
    if pdpte & (1 << 7) != 0 {
        return Some((pdpte & 0x000f_ffff_c000_0000) + (address & 0x3fff_ffff));
    }
    let pd = (pdpte & ADDRESS_MASK) as *const u64;
    let pde = unsafe { pd.add(((address >> 21) & 0x1ff) as usize).read() };
    if pde & 1 == 0 { return None; }
    if pde & (1 << 7) != 0 {
        return Some((pde & 0x000f_ffff_ffe0_0000) + (address & 0x1f_ffff));
    }
    let pt = (pde & ADDRESS_MASK) as *const u64;
    let pte = unsafe { pt.add(((address >> 12) & 0x1ff) as usize).read() };
    if pte & 1 == 0 { return None; }
    Some((pte & ADDRESS_MASK) + (address & 0xfff))
}

/// Confirm that a physical address is currently identity-mapped by a 2 MiB leaf.
unsafe fn bootstrap_identity_mapped(pml4_physical: u64, address: u64) -> bool {
    const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
    if address >= 0x1_0000_0000 { return false; }
    let pml4 = pml4_physical as *const u64;
    let pml4e = unsafe { pml4.read() };
    if pml4e & 1 == 0 { return false; }
    let pdpt = (pml4e & ADDRESS_MASK) as *const u64;
    let pdpte = unsafe { pdpt.add(((address >> 30) & 0x3) as usize).read() };
    if pdpte & 1 == 0 { return false; }
    let pd = (pdpte & ADDRESS_MASK) as *const u64;
    let pde = unsafe { pd.add(((address >> 21) & 0x1ff) as usize).read() };
    if pde & 1 == 0 { return false; }
    if pde & (1 << 7) != 0 {
        return (pde & 0x000f_ffff_ffe0_0000) + (address & 0x1f_ffff) == address;
    }
    let pt = (pde & ADDRESS_MASK) as *const u64;
    let pte = unsafe { pt.add(((address >> 12) & 0x1ff) as usize).read() };
    pte & 1 != 0 && (pte & ADDRESS_MASK) + (address & 0xfff) == address
}

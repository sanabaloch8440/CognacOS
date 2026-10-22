#![no_main]
#![no_std]

extern crate alloc;

mod elf;

use alloc::vec;
use uefi::prelude::*;
use uefi::mem::memory_map::MemoryMap;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::{boot, cstr16};

const KERNEL_PATH: &uefi::CStr16 = cstr16!("\\EFI\\BOOT\\rdotos-kernel.elf");
const MAX_KERNEL_FILE: usize = 32 * 1024 * 1024;
const KERNEL_STACK_PAGES: usize = 16;
const LOW_MEMORY_MAP_PAGES: usize = 64;
const BOOT_INFO_MAGIC: u64 = 0x5244_4F54_4F53_4249; // "RDOTOSBI"
const BOOT_INFO_VERSION: u32 = 1;

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
    drop(kernel_file);
    drop(root);
    drop(fs);

    uefi::println!("Read {} bytes; validating ELF64 headers and load segments", image.len());
    let entry = match unsafe { elf::load(&image) } {
        Ok(entry) => entry,
        Err(err) => { uefi::println!("Rejected kernel ELF: {:?}", err); return Status::LOAD_ERROR; }
    };
    // No heap-backed value or protocol handle may be dropped after this point.
    // Leak the file buffer so allocator cleanup cannot call boot services.
    core::mem::forget(image);

    uefi::println!("Kernel segments loaded; entry = {:#x}", entry);
    let stack = match boot::allocate_pages(
        boot::AllocateType::MaxAddress(0xffff_ffff),
        boot::MemoryType::LOADER_DATA,
        KERNEL_STACK_PAGES,
    ) {
        Ok(stack) => stack,
        Err(err) => { uefi::println!("Kernel stack allocation failed: {:?}", err); return err.status(); }
    };
    let stack_top = stack.as_ptr() as u64 + (KERNEL_STACK_PAGES * boot::PAGE_SIZE) as u64;
    // SysV x86-64 function-entry alignment: RSP % 16 == 8, with a dummy
    // return slot because the kernel entry is reached by a tail jump.
    unsafe { ((stack_top - 8) as *mut u64).write(0); }
    let kernel_stack = stack_top - 8;
    uefi::println!("Allocated {} KiB kernel stack", KERNEL_STACK_PAGES * 4);

    // Allocate bootstrap page tables while UEFI allocation services are still
    // available. Keep every table below 4 GiB because the initial map is identity.
    let pml4_physical = match build_bootstrap_page_tables() {
        Ok(address) => address,
        Err(status) => { uefi::println!("Page-table allocation failed: {:?}", status); return status; }
    };
    uefi::println!("Prepared kernel PML4 at {:#x}", pml4_physical);
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
    let boot_info = BootInfo {
        magic: BOOT_INFO_MAGIC,
        version: BOOT_INFO_VERSION,
        struct_size: core::mem::size_of::<BootInfo>() as u32,
        pml4_physical,
        memory_map: map_copy.as_ptr().cast::<uefi::boot::MemoryDescriptor>(),
        memory_map_entries: memory_map.len(),
        memory_descriptor_size: descriptor_size,
        memory_descriptor_version: memory_map.meta().desc_version,
    };
    let boot_info_ptr = boot_info_storage.as_ptr().cast::<BootInfo>();
    unsafe { boot_info_ptr.write(boot_info); }
    core::mem::forget(memory_map);

    // x86-64 System V kernel ABI: BootInfo pointer in RDI. This first-stage
    // kernel is linked below 1 GiB and uses the firmware's existing identity map.
    // SAFETY: ELF validation ensured the entry lies inside a loaded PT_LOAD range.
    unsafe {
        core::arch::asm!(
            "cli",
            "cld",
            "mov rsp, rsi",
            "xor rbp, rbp",
            "jmp rax",
            in("rax") entry,
            in("rdi") boot_info_ptr,
            in("rsi") kernel_stack,
            options(noreturn)
        );
    }
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

/// Build page tables that identity-map 0..4 GiB with 2 MiB pages. The kernel
/// takes ownership by loading this PML4 into CR3 immediately after entry.
fn build_bootstrap_page_tables() -> Result<u64, Status> {
    const PRESENT_WRITE: u64 = 0x003;
    const LARGE_PAGE: u64 = 1 << 7;
    const TWO_MIB: u64 = 2 * 1024 * 1024;
    let pml4 = allocate_boot_page()?;
    let pdpt = allocate_boot_page()?;
    let mut dirs = [0u64; 4];
    for dir in dirs.iter_mut() { *dir = allocate_boot_page()?; }
    unsafe {
        (pml4 as *mut u64).write(pdpt | PRESENT_WRITE);
        for index in 0..4 {
            (pdpt as *mut u64).add(index).write(dirs[index] | PRESENT_WRITE);
            let table = dirs[index] as *mut u64;
            for entry in 0..512 {
                let physical = (index as u64 * 512 + entry as u64) * TWO_MIB;
                table.add(entry).write(physical | PRESENT_WRITE | LARGE_PAGE);
            }
        }
    }
    Ok(pml4)
}

#![no_main]
#![no_std]

extern crate alloc;

mod elf;

use alloc::vec;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::mem::memory_map::MemoryMap;
use uefi::{cstr16, boot};

const KERNEL_PATH: &uefi::CStr16 = cstr16!("\\EFI\\BOOT\\rdotos-kernel.elf");
const MAX_KERNEL_FILE: usize = 32 * 1024 * 1024;
const BOOT_INFO_MAGIC: u64 = 0x5244_4F54_4F53_4249; // "RDOTOSBI"
const BOOT_INFO_VERSION: u32 = 1;

#[repr(C)]
struct BootInfo {
    magic: u64,
    version: u32,
    struct_size: u32,
    memory_map: *const uefi::boot::MemoryDescriptor,
    memory_map_entries: usize,
    memory_descriptor_size: usize,
    memory_descriptor_version: u32,
}

#[entry]
fn main(image_handle: Handle, _system_table: SystemTable<Boot>) -> Status {
    if let Err(status) = uefi::helpers::init() { return status; }
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
    uefi::println!("Exiting UEFI boot services");

    // SAFETY: All filesystem protocols are closed and no UEFI allocator-backed
    // object is accessed after the transition. The wrapper retries a stale map key.
    let memory_map = unsafe { boot::exit_boot_services(None) };
    let mut boot_info = BootInfo {
        magic: BOOT_INFO_MAGIC,
        version: BOOT_INFO_VERSION,
        struct_size: core::mem::size_of::<BootInfo>() as u32,
        memory_map: memory_map.buffer().as_ptr().cast::<uefi::boot::MemoryDescriptor>(),
        memory_map_entries: memory_map.len(),
        memory_descriptor_size: memory_map.meta().desc_size,
        memory_descriptor_version: memory_map.meta().desc_version,
    };
    let boot_info_ptr = &mut boot_info as *mut BootInfo;

    // x86-64 System V kernel ABI: BootInfo pointer in RDI. This first-stage
    // kernel is linked below 1 GiB and uses the firmware's existing identity map.
    // SAFETY: ELF validation ensured the entry lies inside a loaded PT_LOAD range.
    unsafe {
        core::arch::asm!(
            "cli",
            "cld",
            "jmp rax",
            in("rax") entry,
            in("rdi") boot_info_ptr,
            options(noreturn)
        );
    }
}

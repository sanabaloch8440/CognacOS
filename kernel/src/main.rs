#![no_std]
#![no_main]

mod arch;
mod memory;
mod security;
mod vm;
mod user;
mod pe;
mod process;
mod syscall;
mod winabi;
mod pe_runtime;
mod usermode;

use core::arch::asm;
use core::panic::PanicInfo;

const BOOT_INFO_MAGIC: u64 = 0x5244_4F54_4F53_4249;

unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
}

#[repr(C)]
struct BootInfo {
    magic: u64,
    version: u32,
    struct_size: u32,
    pml4_physical: u64,
    memory_map: *const memory::MemoryDescriptor,
    memory_map_entries: usize,
    memory_descriptor_size: usize,
    memory_descriptor_version: u32,
    kernel_stack_guard_page: u64,
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(boot_info: *const BootInfo) -> ! {
    unsafe {
        serial_init();
        serial_write(b"RdotOS Rust kernel entered.\r\n");
        if boot_info.is_null() {
            serial_write(b"FATAL: null boot information pointer.\r\n");
            halt();
        }
        let info = &*boot_info;
        if info.magic != BOOT_INFO_MAGIC || info.version != 2
            || (info.struct_size as usize) < core::mem::size_of::<BootInfo>()
        {
            serial_write(b"FATAL: invalid boot information structure.\r\n");
            halt();
        }
        if info.memory_map_entries > 0 && info.memory_map.is_null() {
            serial_write(b"FATAL: missing memory map.\r\n");
            halt();
        }
        if info.memory_descriptor_size < core::mem::size_of::<memory::MemoryDescriptor>() {
            serial_write(b"FATAL: unsupported UEFI memory descriptor size.\r\n");
            halt();
        }

        if info.pml4_physical == 0 || (info.pml4_physical & 0xfff) != 0 || info.pml4_physical >= 0x1_0000_0000 {
            serial_write(b"FATAL: invalid bootstrap PML4 address.\r\n");
            halt();
        }
        serial_write(b"UEFI boot services exited successfully.\r\n");
        serial_write(b"Boot information version: 2\r\n");
        serial_write(b"Memory map entries: ");
        serial_write_decimal(info.memory_map_entries);
        serial_write(b"\r\n");

        // Install a private descriptor table, then immediately switch CR3 to
        // the UEFI-allocated page tables. These identity-map the boot chain.
        arch::initialize();
        user::initialize();
        syscall::initialize();
        usermode::initialize();
        arch::enable_kernel_paging(info.pml4_physical);
        serial_write(b"Kernel-owned CR3 activated successfully.\r\n");

        // Only after the new identity map is active may physical pages be touched
        // through their physical addresses. The allocator uses conventional RAM.
        let region_count = match memory::initialize(
            info.memory_map.cast::<u8>(),
            info.memory_map_entries,
            info.memory_descriptor_size,
        ) {
            Ok(count) => count,
            Err(_) => {
                serial_write(b"FATAL: invalid or unsupported physical memory map.\r\n");
                halt();
            }
        };
        serial_write(b"Kernel GDT and exception IDT installed (vectors 0-31).\r\n");
        serial_write(b"Physical allocator initialized; conventional regions: ");
        serial_write_decimal(region_count);
        serial_write(b"\r\n");

        // Activate processor-enforced baseline mitigations only after the
        // kernel-owned page tables are live. NXE prepares the CPU to honor NX
        // PTE bits; strict per-page NX/W^X mappings are a subsequent step.
        let mitigations = security::initialize();
        serial_write(b"Security: CR0.WP ");
        serial_write(if mitigations.write_protect { b"enabled\r\n" } else { b"FAILED\r\n" });
        serial_write(b"Security: NX ");
        serial_write(if mitigations.nx_enabled { b"enabled; strict page policy follows\r\n" } else if mitigations.nx_supported { b"supported but enable failed\r\n" } else { b"unsupported\r\n" });
        serial_write(b"Security: SMEP ");
        serial_write(if mitigations.smep_enabled { b"enabled\r\n" } else if mitigations.smep_supported { b"supported but enable failed\r\n" } else { b"unsupported\r\n" });
        serial_write(b"Security: SMAP ");
        serial_write(if mitigations.smap_supported { b"supported; deferred until safe user-copy accessors\r\n" } else { b"unsupported\r\n" });

        if !mitigations.nx_enabled {
            serial_write(b"FATAL: NX unavailable; refusing to enable strict page policy.\r\n");
            halt();
        }
        let text = (core::ptr::addr_of!(__text_start) as u64, core::ptr::addr_of!(__text_end) as u64);
        let rodata = (core::ptr::addr_of!(__rodata_start) as u64, core::ptr::addr_of!(__rodata_end) as u64);
        let data = (core::ptr::addr_of!(__data_start) as u64, core::ptr::addr_of!(__data_end) as u64);
        let bss = (core::ptr::addr_of!(__bss_start) as u64, core::ptr::addr_of!(__bss_end) as u64);
        if unsafe { memory::harden_page_tables(info.pml4_physical, text, rodata, data, bss, info.kernel_stack_guard_page) }.is_err() {
            serial_write(b"FATAL: kernel page-permission hardening failed.\r\n");
            halt();
        }
        serial_write(b"Security: 4 KiB page policy active; W^X enforced for kernel sections.\r\n");
        serial_write(b"Security: kernel stack guard page unmapped.\r\n");

        // Exercise an independent user address space without activating ring 3.
        let mut address_space = match unsafe { vm::AddressSpace::new(info.pml4_physical) } {
            Ok(space) => space,
            Err(_) => { serial_write(b"FATAL: address-space root allocation failed.\r\n"); halt(); }
        };
        let user_page = match unsafe { memory::allocate_page() } {
            Ok(page) => page,
            Err(_) => { serial_write(b"FATAL: user test page allocation failed.\r\n"); halt(); }
        };
        const TEST_USER_ADDRESS: u64 = 0x0000_0080_0000_0000;
        if unsafe { address_space.map_user_page(TEST_USER_ADDRESS, user_page, true, false) }.is_err() {
            serial_write(b"FATAL: user mapping setup failed.\r\n");
            halt();
        }
        let sample = *b"RdotOS user-copy check";
        if unsafe { address_space.copy_to_user(TEST_USER_ADDRESS, &sample) }.is_err() {
            serial_write(b"FATAL: checked copy-to-user failed.\r\n");
            halt();
        }
        let mut roundtrip = [0u8; 22];
        if unsafe { address_space.copy_from_user(TEST_USER_ADDRESS, &mut roundtrip) }.is_err()
            || roundtrip != sample
        {
            serial_write(b"FATAL: checked user-copy validation failed.\r\n");
            halt();
        }
        serial_write(b"Process address-space and checked user-copy self-test passed.\r\n");
        serial_write(b"Security: ring-3 GDT/TSS RSP0 installed; IRETQ transition available.\r\n");
        serial_write(b"Security: ring-3 GDT/TSS RSP0 installed; IRETQ transition available.\r\n");
        let loaded_pe = match unsafe { pe::self_test(&mut address_space) } {
            Ok(image) => image,
            Err(_) => { serial_write(b"FATAL: PE32+ loader self-test failed.\r\n"); halt(); }
        };
        serial_write(b"PE32+ loader self-test passed; image base=");
        serial_write_hex(loaded_pe.base);
        serial_write(b" entry=");
        serial_write_hex(loaded_pe.entry);
        serial_write(b" (RX section and RO/NX headers).\r\n");
        let mut tls_callbacks = [0u64; 64];
        let tls_count = match pe_runtime::tls_callbacks(loaded_pe.base, &mut tls_callbacks) {
            Ok(count) => count,
            Err(_) => { serial_write(b"FATAL: PE TLS callback registration failed.\r\n"); halt(); }
        };
        if tls_count != 1 || pe_runtime::lookup_function(loaded_pe.base, loaded_pe.base + 0x1100).is_none() {
            serial_write(b"FATAL: PE TLS or unwind registration self-test failed.\r\n");
            halt();
        }
        if !matches!(winabi::api_set_host(b"api-ms-win-core-file-l1-1-0"), Some(host) if host == b"kernel32.dll") {
            serial_write(b"FATAL: API-set contract resolution self-test failed.\r\n");
            halt();
        }
        serial_write(b"Windows API-set resolution and PE TLS/unwind registration passed.\r\n");
        if unsafe { usermode::run_loaded_image(&mut address_space, loaded_pe) }.is_err() {
            serial_write(b"FATAL: ring-3 syscall/process lifecycle self-test failed.\r\n");
            halt();
        }
        serial_write(b"Ring-3 syscall dispatch, process exit, and PEB/TEB setup passed.\r\n");

        serial_write(b"Kernel-owned identity map active for 0..4 GiB.\r\n");
        serial_write(b"Physical page allocator online (single-core bootstrap mode).\r\n");
        serial_write(b"Interrupts remain disabled until APIC/timer setup.\r\n");
    }
    unsafe { halt() }
}

unsafe fn serial_init() {
    outb(0x3f9, 0x00);
    outb(0x3fb, 0x80);
    outb(0x3f8, 0x03);
    outb(0x3f9, 0x00);
    outb(0x3fb, 0x03);
    outb(0x3fa, 0xC7);
    outb(0x3fc, 0x0B);
}

unsafe fn outb(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}

pub unsafe fn serial_write(bytes: &[u8]) {
    for byte in bytes {
        unsafe {
            let mut spins = 0u32;
            while spins < 100_000 {
                let status: u8;
                asm!("in al, dx", in("dx") (0x3f8 + 5u16), out("al") status, options(nomem, nostack, preserves_flags));
                if status & 0x20 != 0 { break; }
                spins += 1;
            }
            outb(0x3f8, *byte);
        }
    }
}

pub unsafe fn serial_write_hex(value: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 18];
    out[0] = b'0';
    out[1] = b'x';
    for i in 0..16 {
        out[2 + i] = DIGITS[((value >> ((15 - i) * 4)) & 0xf) as usize];
    }
    serial_write(&out);
}

unsafe fn serial_write_decimal(mut value: usize) {
    let mut digits = [0u8; 20];
    let mut index = digits.len();
    if value == 0 {
        serial_write(b"0");
        return;
    }
    while value != 0 && index > 0 {
        index -= 1;
        digits[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    serial_write(&digits[index..]);
}

pub unsafe fn halt() -> ! {
    loop { unsafe { asm!("cli; hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    unsafe {
        serial_write(b"\r\nRdotOS FATAL: Rust panic; system halted.\r\n");
        halt()
    }
}

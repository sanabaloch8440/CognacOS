#![no_std]
#![no_main]

mod arch;

use core::arch::asm;
use core::panic::PanicInfo;

const BOOT_INFO_MAGIC: u64 = 0x5244_4F54_4F53_4249;

#[repr(C)]
struct BootInfo {
    magic: u64,
    version: u32,
    struct_size: u32,
    memory_map: *const MemoryDescriptor,
    memory_map_entries: usize,
    memory_descriptor_size: usize,
    memory_descriptor_version: u32,
}

#[repr(C)]
struct MemoryDescriptor {
    kind: u32,
    _padding: u32,
    physical_start: u64,
    virtual_start: u64,
    number_of_pages: u64,
    attributes: u64,
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
        if info.magic != BOOT_INFO_MAGIC || info.version != 1
            || (info.struct_size as usize) < core::mem::size_of::<BootInfo>()
        {
            serial_write(b"FATAL: invalid boot information structure.\r\n");
            halt();
        }
        if info.memory_map_entries > 0 && info.memory_map.is_null() {
            serial_write(b"FATAL: missing memory map.\r\n");
            halt();
        }
        if info.memory_descriptor_size != core::mem::size_of::<MemoryDescriptor>() {
            serial_write(b"FATAL: unsupported UEFI memory descriptor size.\r\n");
            halt();
        }

        serial_write(b"UEFI boot services exited successfully.\r\n");
        serial_write(b"Boot information version: 1\r\n");
        serial_write(b"Memory map entries: ");
        serial_write_decimal(info.memory_map_entries);
        serial_write(b"\r\n");

        // Install kernel-owned descriptor tables before expanding kernel
        // initialization. External interrupts remain disabled in this stage.
        arch::initialize();
        serial_write(b"Kernel GDT and exception IDT installed (vectors 0-31).\r\n");
        serial_write(b"Interrupts remain disabled until APIC/timer setup.\r\n");
        serial_write(b"Early exception infrastructure ready; paging/allocator pending.\r\n");
    }
    halt()
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

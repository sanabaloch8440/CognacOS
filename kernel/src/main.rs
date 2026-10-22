#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

// The bootloader does not call this entry yet. The next milestone is a defined
// UEFI memory-map handoff and an architecture-specific transition into this ELF.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe {
        // COM1 serial diagnostics are useful in QEMU before a console driver exists.
        outb(0x3f8 + 1, 0x00);
        outb(0x3f8 + 3, 0x80);
        outb(0x3f8 + 0, 0x03);
        outb(0x3f8 + 1, 0x00);
        outb(0x3f8 + 3, 0x03);
        outb(0x3f8 + 2, 0xC7);
        outb(0x3f8 + 4, 0x0B);
        serial_write(b"RdotOS Rust kernel entry reached.\r\n");
        serial_write(b"Memory management and interrupt setup are pending.\r\n");
    }
    loop { unsafe { asm!("cli; hlt", options(nomem, nostack)); } }
}

unsafe fn outb(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}

unsafe fn serial_write(bytes: &[u8]) {
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

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop { unsafe { asm!("cli; hlt", options(nomem, nostack)); } }
}

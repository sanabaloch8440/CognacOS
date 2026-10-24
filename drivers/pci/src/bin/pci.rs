#![no_std]
#![no_main]
use core::ffi::c_void;
#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"PCI_____";
#[used]
#[unsafe(link_section = ".data")]
static DRIVER_COOKIE: u64 = 0x5244_4F54_4F53_0001;
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(_driver: *mut c_void, _context: *mut c_void) -> i32 { 0 }
#[panic_handler] fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } } }

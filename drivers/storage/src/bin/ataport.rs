#![no_std]
#![no_main]

// ATA port driver (ataport.sys): owns ATA channel queues, request lifecycle, DMA/interrupt plumbing and miniport registration.
// This image is intentionally not loadable yet: the matching WDM/PnP contract
// and hardware lifecycle are being implemented in subsequent changes. Returning
// STATUS_NOT_SUPPORTED prevents boot from treating a placeholder as a working
// storage driver.
use cognacos_nt_abi::types::PVOID;

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"ATAPORT_";

const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(_driver: PVOID, _context: PVOID) -> i32 {
    STATUS_NOT_SUPPORTED
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

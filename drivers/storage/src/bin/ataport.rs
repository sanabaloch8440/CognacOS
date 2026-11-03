#![no_std]
#![no_main]

// ATA port driver (ataport.sys). DriverEntry is loadable; disk requests remain
// fail-closed until a controller miniport and WDM request path are connected.
use cognacos_nt_abi::{io::{DRIVER_OBJECT, IRP, DEVICE_OBJECT}, ntstatus::*, types::PVOID};
const IRP_MJ_MAXIMUM_FUNCTION: usize = 27;
unsafe extern "win64" fn dispatch(_device: *mut DEVICE_OBJECT, _irp: *mut IRP) -> i32 { STATUS_NOT_SUPPORTED }
#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"ATAPORT_";
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: *mut DRIVER_OBJECT, _context: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe {
        for index in 0..=IRP_MJ_MAXIMUM_FUNCTION { (*driver).major_function[index] = Some(dispatch); }
        (*driver).driver_unload = None;
    }
    STATUS_SUCCESS
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

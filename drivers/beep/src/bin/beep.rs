#![no_std]
#![no_main]
use cognacos_beep::{BeepSetParameters,Speaker,SpeakerIo,BeepError};
#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE:[u8;8]=*b"BEEP____";
static mut SPEAKER:Speaker=Speaker::new();
struct PortIo;
impl SpeakerIo for PortIo {
 fn read_port_u8(&mut self,p:u16)->Result<u8,BeepError>{let v:u8;unsafe{core::arch::asm!("in al, dx",in("dx")p,out("al")v,options(nomem,nostack,preserves_flags));}Ok(v)}
 fn write_port_u8(&mut self,p:u16,v:u8)->Result<(),BeepError>{unsafe{core::arch::asm!("out dx, al",in("dx")p,in("al")v,options(nomem,nostack,preserves_flags));}Ok(())}
}
/// Native x64 DriverEntry. The image exposes the protocol engine; the WDM
/// dispatch/device-object binding is enabled only when the kernel publishes
/// the matching port-I/O and timer service contract.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(_driver:*mut core::ffi::c_void,_context:*mut core::ffi::c_void)->i32{
 // Fail closed: never claim a WDM device is installed when no kernel service
 // contract is available to create the device or schedule duration expiry.
 let _=core::mem::size_of::<BeepSetParameters>();
 let _=core::ptr::addr_of!(SPEAKER);
 0xC00000BBu32 as i32
}
#[panic_handler]fn panic(_: &core::panic::PanicInfo<'_>)->!{loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}}

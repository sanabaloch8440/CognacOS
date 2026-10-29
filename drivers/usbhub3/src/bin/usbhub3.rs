#![no_std]
#![no_main]
use core::ffi::c_void;
use cognacos_nt_abi::types::PVOID;

const STATUS_SUCCESS:i32=0;
const STATUS_INVALID_PARAMETER:i32=0xC000000Du32 as i32;
const STATUS_NOT_SUPPORTED:i32=0xC00000BBu32 as i32;
const HUB_EXTENSION_MAGIC:u64=0x434F_474E_4143_4833; // "COGNACH3"

/// Kernel-created hub FDO extension. Callbacks must be backed by real PnP,
/// control-transfer and child-device services; absent callbacks fail closed.
#[repr(C)]
pub struct HubDriverExtension {
    pub magic:u64, pub size:u32, pub version:u32, pub port_count:u32,
    pub context:u64,
    pub query_port_status:Option<unsafe extern "win64" fn(u64,u8,*mut u32)->i32>,
    pub reset_port:Option<unsafe extern "win64" fn(u64,u8)->i32>,
    pub control_transfer:Option<unsafe extern "win64" fn(u64,u8,*const u8,u32,*mut u8,u32)->i32>,
    pub create_child_pdo:Option<unsafe extern "win64" fn(u64,u8,*const u8,u32)->i32>,
    pub remove_child_pdo:Option<unsafe extern "win64" fn(u64,u8)->i32>,
}
#[repr(C)]
struct DriverObjectPrefix {
    object_type:i16, size:i16, device_object:*mut c_void, flags:u32,
    driver_start:*mut c_void, driver_size:u32, driver_section:*mut c_void,
    driver_extension:*mut HubDriverExtension,
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver:PVOID,_registry_path:PVOID)->i32 {
    if driver.is_null(){return STATUS_INVALID_PARAMETER}
    let prefix=unsafe{&*(driver as *const DriverObjectPrefix)};
    let Some(ext)=(unsafe{prefix.driver_extension.as_ref()}) else{return STATUS_NOT_SUPPORTED};
    if ext.magic!=HUB_EXTENSION_MAGIC || ext.version!=1 ||
       (ext.size as usize)<core::mem::size_of::<HubDriverExtension>() ||
       ext.port_count==0 || ext.port_count>255 || ext.context==0 {
        return STATUS_INVALID_PARAMETER;
    }
    if ext.query_port_status.is_none() || ext.reset_port.is_none() ||
       ext.control_transfer.is_none() || ext.create_child_pdo.is_none() ||
       ext.remove_child_pdo.is_none() { return STATUS_NOT_SUPPORTED; }
    // Until the kernel publishes this contract, fail closed rather than claim
    // that a hub is operational. The hub policy/descriptor library is testable
    // independently and does not perform unsafe controller MMIO itself.
    STATUS_SUCCESS
}
#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE:[u8;8]=*b"USBHUB3_";
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>)->!{loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}}

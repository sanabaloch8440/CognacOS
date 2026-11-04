#![no_std]
#![no_main]

use cognacos_hal_contract::{
    hal, kernel_services, HalBootstrapInfo, HalKernelServices,
    HAL_BOOTSTRAP_INFO_VERSION,
};

#[used]
#[unsafe(link_section = ".rodata")]
static HAL_SIGNATURE: [u8; 8] = *b"HAL_____";
#[used]
#[unsafe(link_section = ".data")]
static HAL_COOKIE: u64 = 0x5244_4F54_4F53_0001;

/// Standalone HAL image initialization entry.
///
/// This is a CognacOS private boot ABI, not WDM DriverEntry and not a claim
/// that the Windows-private HalInitSystem ABI has been implemented. The kernel
/// passes an immutable executive service table with static lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacHalInitialize(info: *const HalBootstrapInfo) -> i32 {
    const STATUS_SUCCESS: i32 = 0;
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
    const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BB_u32 as i32;
    if info.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let info = unsafe { &*info };
    if !info.compatible() || info.version != HAL_BOOTSTRAP_INFO_VERSION {
        return STATUS_INVALID_PARAMETER;
    }
    // SAFETY: the boot handoff contract requires the table to remain immutable
    // and allocated for the complete kernel/HAL lifetime.
    let services: &'static HalKernelServices = unsafe { &*info.kernel_services };
    if unsafe { kernel_services::install(services) }.is_err() {
        return STATUS_NOT_SUPPORTED;
    }
    match hal::initialize_bsp(info.bsp_apic_id) {
        Ok(0) => {}
        _ => return STATUS_NOT_SUPPORTED,
    }
    if unsafe { hal::install_cpu_local(0) }.is_err() {
        return STATUS_NOT_SUPPORTED;
    }
    STATUS_SUCCESS
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); }
    }
}

#![no_std]
#![no_main]

use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};
use cognacos_acpi as acpi;
use cognacos_nt_abi::{io::DRIVER_OBJECT, ntstatus::*, types::PVOID};

const HANDOFF_MAGIC: u64 = 0x434f_474e_4143_4143; // COGNACAC
const HANDOFF_VERSION: u32 = 1;
const MAX_HANDOFF_SIZE: usize = core::mem::size_of::<AcpiDriverHandoff>();

#[repr(C)]
pub struct AcpiDriverHandoff {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    /// Physical RSDP address from the UEFI boot handoff. Firmware tables must
    /// be identity-mapped and remain resident for the duration of discovery.
    pub rsdp_address: u64,
}

struct AcpiState {
    ready: AtomicBool,
    madt: UnsafeCell<MaybeUninit<acpi::MadtInfo>>,
    mcfg: UnsafeCell<MaybeUninit<acpi::McfgInfo>>,
    mcfg_count: core::sync::atomic::AtomicUsize,
    dmar_present: AtomicBool,
}
unsafe impl Sync for AcpiState {}
static STATE: AcpiState = AcpiState {
    ready: AtomicBool::new(false),
    madt: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg_count: core::sync::atomic::AtomicUsize::new(0),
    dmar_present: AtomicBool::new(false),
};

/// DriverEntry performs driver-wide firmware validation only. Device objects,
/// ACPI namespace PDOs, AML evaluation, and per-device resources belong to the
/// later PnP/ACPI namespace integration path, not to DriverEntry.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    let object = unsafe { &*(driver as *const DRIVER_OBJECT) };
    if object.driver_extension.is_null() { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(object.driver_extension as *const AcpiDriverHandoff) };
    if handoff.magic != HANDOFF_MAGIC || handoff.version != HANDOFF_VERSION
        || (handoff.size as usize) < MAX_HANDOFF_SIZE || handoff.rsdp_address == 0 {
        return STATUS_INVALID_PARAMETER;
    }

    let madt = match unsafe { acpi::discover_madt(handoff.rsdp_address) } {
        Ok(info) => info,
        Err(error) => return acpi::error_to_ntstatus(error),
    };
    unsafe { (*STATE.madt.get()).write(madt); }

    // MCFG and DMAR are optional ACPI tables. If present, they must validate;
    // malformed firmware data is not silently ignored.
    match unsafe { acpi::find_table(handoff.rsdp_address, *b"MCFG", 0) } {
        Ok(bytes) => match acpi::parse_mcfg(bytes) {
            Ok(info) => {
                STATE.mcfg_count.store(info.count, Ordering::Release);
                unsafe { (*STATE.mcfg.get()).write(info); }
            }
            Err(error) => return acpi::error_to_ntstatus(error),
        },
        Err(acpi::AcpiError::MissingRoot) | Err(acpi::AcpiError::BadSignature) => {}
        Err(acpi::AcpiError::Truncated) | Err(acpi::AcpiError::BadLength)
        | Err(acpi::AcpiError::BadChecksum) | Err(acpi::AcpiError::MalformedEntry)
        | Err(acpi::AcpiError::TooManyEntries) | Err(acpi::AcpiError::AddressOutOfRange)
        | Err(acpi::AcpiError::MissingMadt) => return STATUS_INVALID_PARAMETER,
    }
    match unsafe { acpi::find_table(handoff.rsdp_address, *b"DMAR", 0) } {
        Ok(bytes) => match acpi::parse_dmar(bytes) {
            Ok(_) => STATE.dmar_present.store(true, Ordering::Release),
            Err(error) => return acpi::error_to_ntstatus(error),
        },
        Err(acpi::AcpiError::BadSignature) | Err(acpi::AcpiError::MissingRoot) => {}
        Err(_) => return STATUS_INVALID_PARAMETER,
    }
    STATE.ready.store(true, Ordering::Release);
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacAcpiQueryMadt(output: *mut acpi::MadtInfo) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    unsafe { output.write((*STATE.madt.get()).assume_init_read()); }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacAcpiQueryMcfg(output: *mut acpi::McfgInfo) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    if STATE.mcfg_count.load(Ordering::Acquire) == 0 { return STATUS_NOT_FOUND; }
    unsafe { output.write((*STATE.mcfg.get()).assume_init_read()); }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub extern "win64" fn CognacAcpiDmarPresent() -> u32 {
    STATE.dmar_present.load(Ordering::Acquire) as u32
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"ACPI____";
#[used]
#[unsafe(link_section = ".data")]
static DRIVER_COOKIE: u64 = 0x5244_4F54_4F53_0001;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

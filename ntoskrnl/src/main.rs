#![no_std]
#![no_main]

// Canonical Windows NT-style ABI types are shared with NTDLL and driver crates.
pub(crate) use cognacos_nt_abi as nt_abi;

pub(crate) use ntoskrnl::mm::{kernel_space as kernel_vm, pagefile, physical as memory};
pub(crate) use ntoskrnl::se::runtime as security;
#[path = "ps/user.rs"]
mod user;
#[path = "pe/pe.rs"]
mod pe;
#[path = "abi/winabi.rs"]
mod winabi;
#[path = "pe/pe_runtime.rs"]
mod pe_runtime;
#[path = "pe/pe_modules.rs"]
mod pe_modules;
#[path = "pe/pe_loader.rs"]
mod pe_loader;
#[path = "pe/pe_dependencies.rs"]
mod pe_dependencies;
#[path = "ps/usermode.rs"]
mod usermode;
use cognacos_acpi as acpi;
use cognacos_pci as pci;
#[path = "../../Hal/mod.rs"]
mod hal;
#[path = "ke/smp.rs"]
mod smp;
#[path = "io/platform_drivers.rs"]
mod platform_drivers;
#[path = "boot/boot_orchestration.rs"]
mod boot_orchestration;
static BOOT_COORDINATOR: boot_orchestration::BootCoordinator = boot_orchestration::BootCoordinator::new();
pub(crate) use ntoskrnl::ke::{dispatcher, scheduler, thread};
pub(crate) use ntoskrnl::ob::object_table as object;
use cognacos_ext4 as extfs;
pub(crate) use ntoskrnl::rtl_runtime::nls;
#[path = "ntoskrnl.rs"]
mod ntoskrnl;
pub(crate) use ntoskrnl::ps::process;
pub(crate) use ntoskrnl::ke::syscall;
pub(crate) use ntoskrnl::io as io;
pub(crate) use ntoskrnl::io::{ata_disk, dma, file_services, wdm};
#[path = "dbg/console.rs"]
mod debug_console;

use core::arch::asm;
use core::panic::PanicInfo;

#[path = "boot/platform_devices.rs"]
mod platform_devices;
#[path = "io/driver_self_test.rs"]
mod driver_self_test;
pub(crate) use driver_self_test::{beep_test_worker, run_beep_runtime_test};
pub use platform_devices::{dequeue_keyboard_event, keyboard_event_drop_count, KeyboardEvent};

#[path = "ke/self_test.rs"]
mod ke_self_test;
pub(crate) use ke_self_test::{
    thread_block_smoke_entry, thread_smoke_entry, thread_wake_smoke_entry,
    THREAD_BLOCK_STAGE, THREAD_BLOCK_STACK, THREAD_SMOKE_RAN, THREAD_SMOKE_STACK,
    THREAD_WAKE_STACK,
};

#[path = "boot/handoff.rs"]
mod boot_handoff;
pub(crate) use boot_handoff::{
    boot_beep_bytes, boot_hive_bytes, boot_kernel_digest, boot_kernel_signature_verified,
    boot_nls_bytes, boot_ntdll_bytes, boot_secure_boot_enabled, boot_security_flags,
    boot_system_dll_bytes, boot_tpm_kernel_measurement_recorded, BootInfo, BOOT_INFO_MAGIC,
    __bss_end, __bss_phys_end, __bss_phys_start, __bss_start, __data_end,
    __data_phys_end, __data_phys_start, __data_start, __rodata_end,
    __rodata_phys_end, __rodata_phys_start, __rodata_start, __text_end,
    __text_phys_end, __text_phys_start, __text_start,
};

#[path = "boot/initialize.rs"]
mod boot_initialize;

#[unsafe(no_mangle)]
pub extern "C" fn _start(boot_info: *const BootInfo) -> ! {
    boot_initialize::initialize(boot_info)
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


pub fn serial_write(bytes: &[u8]) {
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

pub fn serial_write_hex(value: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 18];
    out[0] = b'0';
    out[1] = b'x';
    for i in 0..16 {
        out[2 + i] = DIGITS[((value >> ((15 - i) * 4)) & 0xf) as usize];
    }
    serial_write(&out);
}

fn serial_write_decimal(mut value: usize) {
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

#[cfg(debug_assertions)]
fn serial_write_kernel_vm_diagnostics() {
    let d = kernel_vm::self_test_diagnostics();
    serial_write(b"DEBUG VM: phase=");
    unsafe { serial_write_decimal(d[0] as usize); }
    serial_write(b" (1=direct-map,2=high-RAM,3=extent,4=protection,5=demand-zero,6=pagefile,7=final-reclaim) source-line=");
    unsafe { serial_write_decimal(d[1] as usize); }
    serial_write(b" TLB-IPI-before=");
    unsafe { serial_write_decimal(d[2] as usize); }
    serial_write(b" TLB-IPI-after=");
    unsafe { serial_write_decimal(d[3] as usize); }
    serial_write(b" owner-CR3=");
    unsafe { serial_write_hex(d[4]); }
    serial_write(b" active-CR3=");
    unsafe { serial_write_hex(d[5]); }
    serial_write(b" online-CPUs=");
    unsafe { serial_write_decimal(d[6] as usize); }
    serial_write(b" pagefile-active=");
    unsafe { serial_write_decimal(d[7] as usize); }
    serial_write(b" IPI-shootdowns-enabled=");
    unsafe { serial_write_decimal(d[8] as usize); }
    serial_write(b"\r\n");
    #[cfg(feature = "diagnostic-vm")]
    {
        let map = kernel_vm::self_test_mapping_failure();
        if map[3] != 0 {
            serial_write(b"DEBUG VM WALK: stage=");
            unsafe { serial_write_decimal(map[3] as usize); }
            serial_write(b" (1=input,2=PML4,3=PDPT,4=PD,5=PT) root=");
            unsafe { serial_write_hex(map[0]); }
            serial_write(b" expected-pa=");
            unsafe { serial_write_hex(map[1]); }
            serial_write(b" va=");
            unsafe { serial_write_hex(map[2]); }
            serial_write(b" PML4E=");
            unsafe { serial_write_hex(map[4]); }
            serial_write(b" PDPTE=");
            unsafe { serial_write_hex(map[5]); }
            serial_write(b" PDE=");
            unsafe { serial_write_hex(map[6]); }
            serial_write(b" PTE=");
            unsafe { serial_write_hex(map[7]); }
            serial_write(b"\r\n");
        }
    }
}

pub unsafe fn halt() -> ! {
    // Persist the most recent trace when the disk-backed debug partition is
    // available, then keep the polled COM1 debugger prompt available.
    let saved = crate::ntoskrnl::debug::persist_to_storage();
    unsafe {
        if saved { serial_write(b"\r\nNTOSKRNL trace snapshot persisted.\r\n"); }
        else { serial_write(b"\r\nNTOSKRNL trace persistence unavailable.\r\n"); }
        serial_write(b"cognacos-kd> type help for debugger commands\r\n");
    }
    loop {
        crate::debug_console::poll();
        unsafe { asm!("pause", options(nomem, nostack)); }
    }
}

struct PanicSerialWriter;

impl core::fmt::Write for PanicSerialWriter {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        unsafe { serial_write(value.as_bytes()); }
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    use core::fmt::Write as _;

    let panic_line = info.location().map(|location| location.line() as u64).unwrap_or(0);
    crate::ntoskrnl::ke::diagnostics::record(crate::ntoskrnl::ke::diagnostics::EVENT_PANIC, panic_line, 0, 0);
    crate::ntoskrnl::debug::record_unfiltered(
        crate::ntoskrnl::debug::COMPONENT_EXCEPTION,
        crate::ntoskrnl::debug::LEVEL_ERROR,
        crate::ntoskrnl::debug::EVENT_KERNEL_PANIC,
        panic_line, 0, 0,
    );
    unsafe { serial_write(b"\r\nCognacOS FATAL: Rust panic"); }
    if let Some(location) = info.location() {
        let mut writer = PanicSerialWriter;
        let _ = write!(&mut writer, " at {}:{}", location.file(), location.line());
    }
    unsafe { serial_write(b"\r\n  message: "); }
    let mut writer = PanicSerialWriter;
    let _ = write!(&mut writer, "{}\r\n", info.message());
    unsafe {
        serial_write(b"System halted.\r\n");
        crate::ntoskrnl::debug::dump_serial(32);
        crate::ntoskrnl::ke::diagnostics::dump_serial(16);
        halt()
    }
}

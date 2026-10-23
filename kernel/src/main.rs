#![no_std]
#![no_main]

mod arch;
mod memory;
mod pagefile;
mod kernel_vm;
mod security;
mod vm;
mod user;
mod pe;
mod process;
mod syscall;
mod winabi;
mod pe_runtime;
mod usermode;
mod acpi;
mod apic;
mod hal;
mod smp;
mod scheduler;
mod dispatcher;
mod object;
mod io;
mod extfs;
mod ntoskrnl;

use core::arch::asm;
use core::panic::PanicInfo;

fn io_smoke_dispatch(irp: &mut io::Irp) -> io::IoStatus {
    irp.information = irp.length;
    io::IoStatus::Success
}

fn io_smoke_pending_dispatch(_irp: &mut io::Irp) -> io::IoStatus {
    io::IoStatus::Pending
}

const BOOT_INFO_MAGIC: u64 = 0x5244_4F54_4F53_4249;

unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
}

#[repr(C)]
struct BootInfo {
    magic: u64,
    version: u32,
    struct_size: u32,
    pml4_physical: u64,
    memory_map: *const memory::MemoryDescriptor,
    memory_map_entries: usize,
    memory_descriptor_size: usize,
    memory_descriptor_version: u32,
    kernel_stack_guard_page: u64,
    rsdp_address: u64,
    ap_trampoline_physical: u64,
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
        if info.magic != BOOT_INFO_MAGIC || info.version != 4
            || (info.struct_size as usize) < core::mem::size_of::<BootInfo>()
        {
            serial_write(b"FATAL: invalid boot information structure.\r\n");
            halt();
        }
        if info.memory_map_entries > 0 && info.memory_map.is_null() {
            serial_write(b"FATAL: missing memory map.\r\n");
            halt();
        }
        if info.memory_descriptor_size < core::mem::size_of::<memory::MemoryDescriptor>() {
            serial_write(b"FATAL: unsupported UEFI memory descriptor size.\r\n");
            halt();
        }

        if info.pml4_physical == 0 || (info.pml4_physical & 0xfff) != 0 || info.pml4_physical >= 0x1_0000_0000 {
            serial_write(b"FATAL: invalid bootstrap PML4 address.\r\n");
            halt();
        }
        serial_write(b"UEFI boot services exited successfully.\r\n");
        serial_write(b"Boot information version: 4\r\n");
        if info.ap_trampoline_physical != 0
            && (info.ap_trampoline_physical >= 0x10_0000 || info.ap_trampoline_physical & 0xfff != 0)
        {
            serial_write(b"FATAL: invalid low-memory AP trampoline reservation.\r\n");
            halt();
        }
        if info.ap_trampoline_physical == 0 {
            serial_write(b"WARNING: AP trampoline page unavailable; SMP startup disabled.\r\n");
        } else {
            serial_write(b"AP trampoline storage reserved at: ");
            serial_write_hex(info.ap_trampoline_physical);
            serial_write(b" (not yet executable).\r\n");
        }
        serial_write(b"Memory map entries: ");
        serial_write_decimal(info.memory_map_entries);
        serial_write(b"\r\n");

        // Install a private descriptor table, then immediately switch CR3 to
        // the UEFI-allocated page tables. These identity-map the boot chain.
        arch::initialize();
        let bsp_apic_id = (core::arch::x86_64::__cpuid(1).ebx >> 24) & 0xff;
        if hal::initialize_bsp(bsp_apic_id).is_err()
            || unsafe { hal::install_cpu_local(0) }.is_err()
        {
            serial_write(b"FATAL: bootstrap CPU-local HAL setup failed.\r\n");
            halt();
        }
        user::initialize();
        syscall::initialize();
        usermode::initialize();
        arch::enable_kernel_paging(info.pml4_physical);
        serial_write(b"Kernel-owned CR3 activated successfully.\r\n");

        // Only after the new identity map is active may physical pages be touched
        // through their physical addresses. The allocator uses conventional RAM.
        let region_count = match memory::initialize(
            info.memory_map.cast::<u8>(),
            info.memory_map_entries,
            info.memory_descriptor_size,
        ) {
            Ok(count) => count,
            Err(_) => {
                serial_write(b"FATAL: invalid or unsupported physical memory map.\r\n");
                halt();
            }
        };
        serial_write(b"Kernel GDT and exception IDT installed (vectors 0-31).\r\n");
        // The bootloader reserves one guard page followed by 16 stack pages.
        // Keep the entire allocation out of the physical allocator even if a
        // firmware memory-map snapshot incorrectly reports part as conventional.
        if unsafe {
            memory::reserve_range(
                info.kernel_stack_guard_page,
                info.kernel_stack_guard_page + 17 * 4096,
            )
        }.is_err() {
            serial_write(b"FATAL: cannot reserve bootstrap stack from physical allocator.\r\n");
            halt();
        }
        if pagefile::initialize() {
            serial_write(b"Pagefile storage initialized on dedicated ATA primary-slave disk; slots: ");
            serial_write_decimal(pagefile::capacity_slots() as usize);
            serial_write(b"\r\n");
            if !pagefile::self_test() {
                serial_write(b"FATAL: pagefile storage read/write self-test failed.\r\n");
                halt();
            }
            serial_write(b"Pagefile storage read/write self-test passed.\r\n");
        } else {
            serial_write(b"WARNING: no dedicated pagefile disk; pageable eviction disabled.\r\n");
        }
        if kernel_vm::initialize(info.pml4_physical).is_err() {
            serial_write(b"FATAL: kernel virtual-memory manager initialization failed.\r\n");
            halt();
        }
        if !kernel_vm::self_test() {
            serial_write(b"FATAL: kernel virtual mapping/reclamation self-test failed.\r\n");
            halt();
        }
        serial_write(b"Kernel virtual-address arena and mapping self-test passed.\r\n");
        if pagefile::is_active() {
            serial_write(b"Pagefile-backed pageable eviction and fault-driven page-in self-test passed.\r\n");
        }
        if !ntoskrnl::memory_manager_self_test() {
            serial_write(b"FATAL: NT memory-manager translation/copy self-test failed.\r\n");
            halt();
        }
        serial_write(b"NT memory manager: physical translation, validity, and checked-copy self-test passed.\r\n");
        serial_write(b"Physical allocator initialized; conventional regions: ");
        serial_write_decimal(region_count);
        serial_write(b"\r\n");

        // Validate the separate HAL before using its synchronization primitives.
        if !hal::self_test() {
            serial_write(b"FATAL: HAL self-test failed.\r\n");
            halt();
        }
        serial_write(b"HAL CPU-local registry, MMIO bounds, and x86-64 primitives initialized.\r\n");

        if !smp::self_test() {
            serial_write(b"FATAL: SMP startup-target validation self-test failed.\r\n");
            halt();
        }
        serial_write(b"SMP startup target validation passed.\r\n");

        // Validate the bounded ACPI parser against a minimal synthetic MADT.
        let mut madt = [0u8; 44];
        madt[0..4].copy_from_slice(b"APIC");
        madt[4..8].copy_from_slice(&(44u32).to_le_bytes());
        madt[36..40].copy_from_slice(&(0xfee0_0000u32).to_le_bytes());
        madt[40..44].copy_from_slice(&(1u32).to_le_bytes());
        let sum = madt.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        madt[9] = 0u8.wrapping_sub(sum);
        if !matches!(acpi::parse_madt(&madt), Ok(info) if info.local_apic_address == 0xfee0_0000 && info.flags == 1) {
            serial_write(b"FATAL: ACPI MADT parser self-test failed.\r\n");
            halt();
        }
        serial_write(b"ACPI MADT parser self-test passed; APIC discovery primitives available.\r\n");

        if info.rsdp_address != 0 {
            match acpi::discover_madt(info.rsdp_address) {
                Ok(firmware_madt) => {
                    match hal::register_madt_cpus(&firmware_madt, bsp_apic_id) {
                        Ok(discovered) => {
                            serial_write(b"Firmware ACPI MADT validated; enabled APs discovered: ");
                            serial_write_decimal(discovered);
                            serial_write(b" (not started).\r\n");
                            serial_write(b"Firmware MADT Local APIC address: ");
                            serial_write_hex(firmware_madt.local_apic_address);
                            serial_write(b"\r\n");
                        }
                        Err(_) => {
                            serial_write(b"WARNING: MADT parsed, but HAL CPU registration failed; SMP remains disabled.\r\n");
                        }
                    }
                }
                Err(_) => {
                    serial_write(b"WARNING: firmware ACPI RSDP/root/MADT validation failed; SMP remains disabled.\r\n");
                }
            }
        } else {
            serial_write(b"WARNING: no firmware ACPI RSDP handoff; SMP discovery skipped.\r\n");
        }

        // Exercise scheduler metadata/priority/affinity without switching stacks.
        scheduler::initialize();
        let low = match scheduler::create_task(1, 1, 0x7000, 0x1000) {
            Ok(id) => id, Err(_) => { serial_write(b"FATAL: scheduler task creation failed.\r\n"); halt(); }
        };
        if scheduler::create_task(6, 1, 0x9000, 0x2000).is_err()
            || !matches!(scheduler::choose_next(0), Ok(Some(task)) if task.priority == 6)
            || scheduler::yield_current(0).is_err()
            || scheduler::set_affinity(low, 0).is_ok()
        {
            serial_write(b"FATAL: scheduler priority/affinity self-test failed.\r\n");
            halt();
        }
        serial_write(b"Scheduler priority, affinity, and cooperative-yield metadata self-test passed.\r\n");

        // Exercise typed handles and explicit access checks.
        let event = match object::create(object::ObjectType::Event, object::ACCESS_READ) {
            Ok(h) => h, Err(_) => { serial_write(b"FATAL: object handle creation failed.\r\n"); halt(); }
        };
        if !matches!(object::lookup(event.handle, object::ACCESS_READ), Ok(h) if h.object_id == event.object_id)
            || !matches!(object::lookup(event.handle, object::ACCESS_WRITE), Err(object::ObjectError::AccessDenied))
            || object::name_object(b"\\BaseNamedObjects\\SmokeEvent", event.object_id, object::ObjectType::Event, object::ACCESS_READ).is_err()
            || !matches!(object::open_named(b"\\BaseNamedObjects\\SmokeEvent", object::ACCESS_READ), Ok(h) if h.object_id == event.object_id)
            || object::close(event.handle).is_err()
        {
            serial_write(b"FATAL: object manager handle/access self-test failed.\r\n");
            halt();
        }
        serial_write(b"Object handles, named lookup, and access checks self-test passed.\r\n");

        // Exercise an IRP through a registered synthetic device dispatch routine.
        let mut dispatch: [Option<io::DispatchFn>; 16] = [None; 16];
        dispatch[io::IRP_MJ_READ as usize] = Some(io_smoke_dispatch);
        let device = match io::register_device(b"\\Device\\Smoke", dispatch) {
            Ok(id) => id, Err(_) => { serial_write(b"FATAL: I/O device registration failed.\r\n"); halt(); }
        };
        let irp = match io::submit(device, io::IRP_MJ_READ, 0, 128) {
            Ok(id) => id, Err(_) => { serial_write(b"FATAL: IRP allocation failed.\r\n"); halt(); }
        };
        if !matches!(io::dispatch_one(irp), Ok(io::IoStatus::Success))
            || !matches!(io::query_irp(irp), Some(r) if r.completed && r.information == 128)
            || io::release_irp(irp).is_err()
        {
            serial_write(b"FATAL: I/O manager IRP dispatch self-test failed.\r\n");
            halt();
        }
        serial_write(b"I/O manager device dispatch and IRP completion self-test passed.\r\n");

        // Verify pre-dispatch cancellation and asynchronous single-owner completion.
        let cancelled = match io::submit(device, io::IRP_MJ_READ, 0, 16) {
            Ok(id) => id, Err(_) => { serial_write(b"FATAL: I/O cancel fixture allocation failed.\r\n"); halt(); }
        };
        if io::cancel_irp(cancelled).is_err()
            || !matches!(io::query_irp(cancelled), Some(r) if r.status == io::IoStatus::Cancelled && r.completed)
            || io::release_irp(cancelled).is_err()
        {
            serial_write(b"FATAL: I/O cancellation self-test failed.\r\n"); halt();
        }
        let mut pending_dispatch: [Option<io::DispatchFn>; 16] = [None; 16];
        pending_dispatch[io::IRP_MJ_READ as usize] = Some(io_smoke_pending_dispatch);
        let pending_device = match io::register_device(b"\\Device\\AsyncSmoke", pending_dispatch) {
            Ok(id) => id, Err(_) => { serial_write(b"FATAL: asynchronous I/O device registration failed.\r\n"); halt(); }
        };
        let pending_irp = match io::submit(pending_device, io::IRP_MJ_READ, 0, 64) {
            Ok(id) => id, Err(_) => { serial_write(b"FATAL: pending IRP allocation failed.\r\n"); halt(); }
        };
        if io::dispatch_one(pending_irp) != Ok(io::IoStatus::Pending)
            || io::dispatch_one(pending_irp) != Err(io::IoError::AlreadyInFlight)
            || io::complete_irp(pending_irp, io::IoStatus::Success, 64).is_err()
            || !matches!(io::query_irp(pending_irp), Some(r) if r.completed && !r.in_flight && r.information == 64)
            || io::release_irp(pending_irp).is_err()
        {
            serial_write(b"FATAL: asynchronous IRP ownership/completion self-test failed.\r\n"); halt();
        }
        serial_write(b"I/O manager cancellation, in-flight ownership, and asynchronous completion tests passed.\r\n");
        // Read-only native Rust ext filesystem parser self-test.
        if !extfs::self_test() {
            serial_write(b"FATAL: ext filesystem parser self-test failed.\r\n");
            halt();
        }
        serial_write(b"Ext filesystem parser self-test passed (read-only ext2/ext3/ext4 metadata; unsupported features rejected).\r\n");

        // APIC registers are intentionally not enabled here: no firmware MADT
        // handoff, AP startup trampoline, timer calibration, or complete IRQ
        // routing is installed yet. Keep IF clear until those prerequisites exist.

        // Activate processor-enforced baseline mitigations only after the
        // kernel-owned page tables are live. NXE prepares the CPU to honor NX
        // PTE bits; strict per-page NX/W^X mappings are a subsequent step.
        let mitigations = security::initialize();
        serial_write(b"Security: CR0.WP ");
        serial_write(if mitigations.write_protect { b"enabled\r\n" } else { b"FAILED\r\n" });
        serial_write(b"Security: NX ");
        serial_write(if mitigations.nx_enabled { b"enabled; strict page policy follows\r\n" } else if mitigations.nx_supported { b"supported but enable failed\r\n" } else { b"unsupported\r\n" });
        serial_write(b"Security: SMEP ");
        serial_write(if mitigations.smep_enabled { b"enabled\r\n" } else if mitigations.smep_supported { b"supported but enable failed\r\n" } else { b"unsupported\r\n" });
        serial_write(b"Security: SMAP ");
        serial_write(if mitigations.smap_supported { b"supported; deferred until safe user-copy accessors\r\n" } else { b"unsupported\r\n" });

        if !mitigations.nx_enabled {
            serial_write(b"FATAL: NX unavailable; refusing to enable strict page policy.\r\n");
            halt();
        }
        let text = (core::ptr::addr_of!(__text_start) as u64, core::ptr::addr_of!(__text_end) as u64);
        let rodata = (core::ptr::addr_of!(__rodata_start) as u64, core::ptr::addr_of!(__rodata_end) as u64);
        let data = (core::ptr::addr_of!(__data_start) as u64, core::ptr::addr_of!(__data_end) as u64);
        let bss = (core::ptr::addr_of!(__bss_start) as u64, core::ptr::addr_of!(__bss_end) as u64);
        if unsafe { memory::harden_page_tables(info.pml4_physical, text, rodata, data, bss, info.kernel_stack_guard_page) }.is_err() {
            serial_write(b"FATAL: kernel page-permission hardening failed.\r\n");
            halt();
        }
        serial_write(b"Security: 4 KiB page policy active; W^X enforced for kernel sections.\r\n");
        serial_write(b"Security: kernel stack guard page unmapped.\r\n");
        // Start discovered application processors only after the shared kernel
        // mappings are hardened and the executable low-memory trampoline can be
        // installed with explicit page permissions.
        if info.rsdp_address != 0 && info.ap_trampoline_physical != 0 {
            match acpi::discover_madt(info.rsdp_address) {
                Ok(madt) => match unsafe {
                    smp::start_discovered_aps_with_ipi(
                        info.pml4_physical,
                        info.ap_trampoline_physical,
                        &madt,
                        bsp_apic_id,
                    )
                } {
                    Ok((online, ipi_enabled)) => {
                        serial_write(b"SMP AP startup complete; online application processors: ");
                        serial_write_decimal(online);
                        serial_write(if ipi_enabled { b"; interrupt-driven TLB shootdowns enabled.\r\n" } else { b"; using polling TLB fallback (IPI setup unavailable).\r\n" });
                        serial_write(b"SMP startup diagnostic error code: ");
                        serial_write_decimal(smp::last_start_error_code());
                        serial_write(b"; last target APIC ID: ");
                        serial_write_decimal(smp::last_target_apic_id());
                        serial_write(b"; AP entry functions reached: ");
                        serial_write_decimal(smp::ap_entry_reached_count());
                        serial_write(b"\r\n");
                    }
                    Err(_) => serial_write(b"WARNING: AP startup initialization failed; retaining BSP-only operation.\r\n"),
                },
                Err(_) => serial_write(b"WARNING: MADT unavailable at AP startup; retaining BSP-only operation.\r\n"),
            }
        } else {
            serial_write(b"SMP startup skipped: firmware MADT or low-memory trampoline unavailable.\r\n");
        }


        // Re-run the VM extent stress test after AP startup. This forces the
        // pool's shared-kernel mapping path to complete its SMP TLB rendezvous.
        if !kernel_vm::self_test() {
            serial_write(b"FATAL: SMP kernel VM/TLB shootdown self-test failed.\r\n");
            halt();
        }
        serial_write(b"Kernel VM extent stress and SMP TLB shootdown self-test passed.\r\n");

        // Exercise an independent user address space without activating ring 3.
        let mut address_space = match unsafe { vm::AddressSpace::new(info.pml4_physical) } {
            Ok(space) => space,
            Err(_) => { serial_write(b"FATAL: address-space root allocation failed.\r\n"); halt(); }
        };
        let user_page = match unsafe { memory::allocate_page() } {
            Ok(page) => page,
            Err(_) => { serial_write(b"FATAL: user test page allocation failed.\r\n"); halt(); }
        };
        const TEST_USER_ADDRESS: u64 = 0x0000_0080_0000_0000;
        if unsafe { address_space.map_user_page(TEST_USER_ADDRESS, user_page, true, false) }.is_err() {
            serial_write(b"FATAL: user mapping setup failed.\r\n");
            halt();
        }
        let sample = *b"RdotOS user-copy check";
        if unsafe { address_space.copy_to_user(TEST_USER_ADDRESS, &sample) }.is_err() {
            serial_write(b"FATAL: checked copy-to-user failed.\r\n");
            halt();
        }
        let mut roundtrip = [0u8; 22];
        if unsafe { address_space.copy_from_user(TEST_USER_ADDRESS, &mut roundtrip) }.is_err()
            || roundtrip != sample
        {
            serial_write(b"FATAL: checked user-copy validation failed.\r\n");
            halt();
        }
        serial_write(b"Process address-space and checked user-copy self-test passed.\r\n");
        serial_write(b"Security: ring-3 GDT/TSS RSP0 installed; IRETQ transition available.\r\n");
        serial_write(b"Security: ring-3 GDT/TSS RSP0 installed; IRETQ transition available.\r\n");
        let loaded_pe = match unsafe { pe::self_test(&mut address_space) } {
            Ok(image) => image,
            Err(_) => { serial_write(b"FATAL: PE32+ loader self-test failed.\r\n"); halt(); }
        };
        serial_write(b"PE32+ loader self-test passed; image base=");
        serial_write_hex(loaded_pe.base);
        serial_write(b" entry=");
        serial_write_hex(loaded_pe.entry);
        serial_write(b" (RX section and RO/NX headers).\r\n");
        let mut tls_callbacks = [0u64; 64];
        let tls_count = match pe_runtime::tls_callbacks(loaded_pe.base, &mut tls_callbacks) {
            Ok(count) => count,
            Err(_) => { serial_write(b"FATAL: PE TLS callback registration failed.\r\n"); halt(); }
        };
        if tls_count != 1 || pe_runtime::lookup_function(loaded_pe.base, loaded_pe.base + 0x1100).is_none() {
            serial_write(b"FATAL: PE TLS or unwind registration self-test failed.\r\n");
            halt();
        }
        if !matches!(winabi::api_set_host(b"api-ms-win-core-file-l1-1-0"), Some(host) if host == b"kernel32.dll") {
            serial_write(b"FATAL: API-set contract resolution self-test failed.\r\n");
            halt();
        }
        serial_write(b"Windows API-set resolution and PE TLS/unwind registration passed.\r\n");
        if unsafe { usermode::run_loaded_image(&mut address_space, loaded_pe) }.is_err() {
            serial_write(b"FATAL: ring-3 syscall/process lifecycle self-test failed.\r\n");
            halt();
        }
        serial_write(b"Ring-3 syscall dispatch, process exit, and PEB/TEB setup passed.\r\n");

        serial_write(b"Kernel-owned identity map active for 0..4 GiB.\r\n");
        serial_write(b"Physical page allocator online (single-core bootstrap mode).\r\n");
        serial_write(b"Interrupts remain disabled until APIC/timer setup.\r\n");
    }
    unsafe { halt() }
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

pub unsafe fn serial_write_hex(value: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 18];
    out[0] = b'0';
    out[1] = b'x';
    for i in 0..16 {
        out[2 + i] = DIGITS[((value >> ((15 - i) * 4)) & 0xf) as usize];
    }
    serial_write(&out);
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

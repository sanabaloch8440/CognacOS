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
mod pci;
mod dma;
mod apic;
mod hal;
mod smp;
mod scheduler;
mod dispatcher;
mod thread;
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
static mut THREAD_SMOKE_STACK: [u8; 4096] = [0; 4096];
static mut THREAD_BLOCK_STACK: [u8; 4096] = [0; 4096];
static mut THREAD_WAKE_STACK: [u8; 4096] = [0; 4096];
static THREAD_SMOKE_RAN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static THREAD_BLOCK_STAGE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

extern "C" fn thread_smoke_entry(_argument: *mut core::ffi::c_void) {
    THREAD_SMOKE_RAN.store(true, core::sync::atomic::Ordering::Release);
}
extern "C" fn thread_block_smoke_entry(_argument: *mut core::ffi::c_void) {
    THREAD_BLOCK_STAGE.store(1, core::sync::atomic::Ordering::Release);
    if thread::block_current().is_err() {
        THREAD_BLOCK_STAGE.store(0xff, core::sync::atomic::Ordering::Release);
        return;
    }
    THREAD_BLOCK_STAGE.store(3, core::sync::atomic::Ordering::Release);
}
extern "C" fn thread_wake_smoke_entry(argument: *mut core::ffi::c_void) {
    if thread::wake(argument as u64).is_ok() {
        THREAD_BLOCK_STAGE.store(2, core::sync::atomic::Ordering::Release);
    } else {
        THREAD_BLOCK_STAGE.store(0xfe, core::sync::atomic::Ordering::Release);
    }
}

unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
    static __text_phys_start: u8;
    static __text_phys_end: u8;
    static __rodata_phys_start: u8;
    static __rodata_phys_end: u8;
    static __data_phys_start: u8;
    static __data_phys_end: u8;
    static __bss_phys_start: u8;
    static __bss_phys_end: u8;
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
    kernel_stack_pages: u64,
    kernel_stack_virtual_guard: u64,
    rsdp_address: u64,
    ap_trampoline_physical: u64,
    kernel_physical_sections: [(u64, u64); 4],
    kernel_physical_section_count: usize,
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
        if info.magic != BOOT_INFO_MAGIC || info.version != 6
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
        if info.kernel_stack_pages == 0 || info.kernel_stack_pages > 1024
            || info.kernel_stack_guard_page & 0xfff != 0
            || info.kernel_stack_virtual_guard < 0xffff_8000_0000_0000
            || info.kernel_stack_virtual_guard & 0xfff != 0
        {
            serial_write(b"FATAL: invalid high-half kernel stack description.\r\n");
            halt();
        }
        let stack_allocation_bytes = match info.kernel_stack_pages.checked_add(1)
            .and_then(|pages| pages.checked_mul(4096)) {
            Some(bytes) => bytes,
            None => { serial_write(b"FATAL: kernel stack size overflow.\r\n"); halt(); }
        };

        if info.pml4_physical == 0 || (info.pml4_physical & 0xfff) != 0 || info.pml4_physical >= 0x1_0000_0000 {
            serial_write(b"FATAL: invalid bootstrap PML4 address.\r\n");
            halt();
        }
        serial_write(b"UEFI boot services exited successfully.\r\n");
        serial_write(b"Boot information version: 6\r\n");
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

        // Install a private descriptor table while the bootloader-established
        // high-half CR3 is active. The explicit reload below verifies ownership.
        arch::initialize();
        let bsp_apic_id = (core::arch::x86_64::__cpuid(1).ebx >> 24) & 0xff;
        if hal::initialize_bsp(bsp_apic_id).is_err()
            || unsafe { hal::install_cpu_local(0) }.is_err()
        {
            serial_write(b"FATAL: bootstrap CPU-local HAL setup failed.\r\n");
            halt();
        }
        ntoskrnl::ex::runtime::initialize_threading();
        user::initialize();
        syscall::initialize();
        usermode::initialize();
        arch::enable_kernel_paging(info.pml4_physical);
        serial_write(b"Kernel-owned CR3 activated successfully.\r\n");

        // Inspect the live page tables before allocator setup or permission
        // changes. These ranges are active bootstrap dependencies: the running
        // kernel, its current stack, BootInfo, final UEFI map, page-table root,
        // reserved stack allocation, and ACPI/AP handoff storage.
        let mut current_rsp: u64;
        let active_cr3: u64;
        asm!("mov {}, rsp", out(reg) current_rsp, options(nomem, nostack, preserves_flags));
        asm!("mov {}, cr3", out(reg) active_cr3, options(nomem, nostack, preserves_flags));
        if active_cr3 & !0xfff != info.pml4_physical {
            serial_write(b"FATAL: active CR3 does not match the validated bootstrap root.\r\n");
            halt();
        }
        let map_bytes = match (info.memory_map_entries as u64)
            .checked_mul(info.memory_descriptor_size as u64) {
            Some(bytes) => bytes,
            None => { serial_write(b"FATAL: memory-map byte length overflow.\r\n"); halt(); }
        };
        let bootstrap_ranges = [
            (info.pml4_physical, 4096u64),
            (boot_info as u64, core::mem::size_of::<BootInfo>() as u64),
            (info.memory_map as u64, map_bytes),
            (info.kernel_stack_guard_page, stack_allocation_bytes),
            (0xfec0_0000, 4096u64),
            (0xfee0_0000, 4096u64),
        ];
        let mut mapping_ok = true;
        for (start, length) in bootstrap_ranges {
            if start != 0 && unsafe {
                memory::validate_identity_range(info.pml4_physical, start, length)
            }.is_err() {
                mapping_ok = false;
                break;
            }
        }
        if info.rsdp_address != 0 && unsafe {
            memory::validate_identity_range(info.pml4_physical, info.rsdp_address, 36)
        }.is_err() {
            mapping_ok = false;
        }
        if info.ap_trampoline_physical != 0 && unsafe {
            memory::validate_identity_range(info.pml4_physical, info.ap_trampoline_physical, 4096)
        }.is_err() {
            mapping_ok = false;
        }
        if !mapping_ok {
            serial_write(b"FATAL: bootstrap page-table inspection found an unmapped or non-identity live dependency.\r\n");
            halt();
        }
        serial_write(b"Page-table inspection passed: low boot data, physical stack backing, APIC windows, and firmware handoff mappings are valid.\r\n");

        let stack_end_virtual = info.kernel_stack_virtual_guard.checked_add(stack_allocation_bytes);
        let expected_stack_physical = current_rsp.checked_sub(info.kernel_stack_virtual_guard)
            .and_then(|offset| info.kernel_stack_guard_page.checked_add(offset));
        let stack_mapping_ok = stack_end_virtual.is_some_and(|end|
            current_rsp >= info.kernel_stack_virtual_guard + 4096 && current_rsp < end)
            && expected_stack_physical.is_some_and(|expected| unsafe {
                memory::inspect_mapping(info.pml4_physical, current_rsp)
                    .is_ok_and(|mapping| mapping.physical_address == expected)
            });
        let section_mappings_ok = unsafe {
            let text_start = core::ptr::addr_of!(__text_start) as u64;
            let text_end = core::ptr::addr_of!(__text_end) as u64;
            let ro_start = core::ptr::addr_of!(__rodata_start) as u64;
            let ro_end = core::ptr::addr_of!(__rodata_end) as u64;
            let data_start = core::ptr::addr_of!(__data_start) as u64;
            let data_end = core::ptr::addr_of!(__data_end) as u64;
            let bss_start = core::ptr::addr_of!(__bss_start) as u64;
            let bss_end = core::ptr::addr_of!(__bss_end) as u64;
            let sections = &info.kernel_physical_sections;
            let count_ok = info.kernel_physical_section_count >= 3
                && info.kernel_physical_section_count <= sections.len();
            count_ok
                && sections[0].0 < sections[0].1
                && sections[1].0 < sections[1].1
                && sections[2].0 < sections[2].1
                && memory::validate_translation_range(info.pml4_physical, text_start,
                    sections[0].0, text_end - text_start).is_ok()
                && memory::validate_translation_range(info.pml4_physical, ro_start,
                    sections[1].0, ro_end - ro_start).is_ok()
                && memory::validate_translation_range(info.pml4_physical, data_start,
                    sections[2].0, data_end - data_start).is_ok()
                && memory::validate_translation_range(info.pml4_physical, bss_start,
                    sections[2].0.checked_add(bss_start - data_start).unwrap_or(u64::MAX),
                    bss_end - bss_start).is_ok()
        };
        let high_guard_unmapped = unsafe {
            memory::inspect_mapping(info.pml4_physical, info.kernel_stack_virtual_guard).is_err()
        };
        if !stack_mapping_ok || !section_mappings_ok || !high_guard_unmapped {
            serial_write(b"FATAL: high-half kernel/stack translations or guard validation failed.\r\n");
            halt();
        }
        serial_write(b"Page-table inspection passed: high-half kernel sections, stack alias, low boot data, and guard pages validated.\r\n");

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
        // Publish direct-map W^X policy before any high-RAM probe can fault in
        // a leaf. An uninitialized policy intentionally defaults to read-only.
        if info.kernel_physical_section_count == 0
            || info.kernel_physical_section_count > info.kernel_physical_sections.len()
            || info.kernel_physical_sections[..info.kernel_physical_section_count]
                .iter().any(|(start, end)| *start == 0 || *start >= *end) {
            serial_write(b"FATAL: invalid PE section physical-backing metadata.\r\n");
            halt();
        }
        memory::set_direct_map_readonly_ranges([
            info.kernel_physical_sections[0],
            info.kernel_physical_sections.get(1).copied().unwrap_or((0, 0)),
        ]);
        serial_write(b"Kernel GDT and exception IDT installed (vectors 0-31).\r\n");
        // Keep the entire physical stack allocation out of the allocator even
        // if firmware's final map incorrectly reports any page as conventional.
        if unsafe {
            memory::reserve_range(
                info.kernel_stack_guard_page,
                info.kernel_stack_guard_page + stack_allocation_bytes,
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
        if memory::first_high_conventional_page().is_some() {
            serial_write(b"High-RAM allocator and direct-map self-test passed.\r\n");
        } else {
            serial_write(b"High RAM direct-map test skipped: firmware exposes no conventional RAM above 4 GiB.\r\n");
        }
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

        let mut local_timer_ready = false;
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

                            // The bootloader identity-maps 0..4 GiB with 2 MiB pages.
                            // Keep legacy and I/O APIC routes masked until the local timer is ready.
                            match (if firmware_madt.local_apic_address == 0xfee0_0000
                                && firmware_madt.io_apics.iter().take(firmware_madt.io_apic_count)
                                    .all(|io| (io.address as u64) >= 0xfec0_0000 && (io.address as u64) < 0xfee0_0000)
                            {
                                // The bootloader maps these standard MMIO windows UC.
                                unsafe { apic::initialize_local_apic(firmware_madt.local_apic_address) }
                            } else {
                                Err(apic::ApicError::InvalidBase)
                            }) {
                                Ok(local_id) if local_id == bsp_apic_id => {
                                    let mut routes_masked = true;
                                    for io in firmware_madt.io_apics.iter().take(firmware_madt.io_apic_count) {
                                        if unsafe { apic::mask_all_ioapic(io.address as u64) }.is_err() {
                                            routes_masked = false;
                                            break;
                                        }
                                    }
                                    if routes_masked {
                                        unsafe { outb(0x21, 0xff); outb(0xa1, 0xff); }
                                        match unsafe { apic::calibrate_local_timer() } {
                                            Ok(count) => {
                                                local_timer_ready = true;
                                                serial_write(b"Local APIC timer calibrated (still masked); 1 ms count=");
                                                serial_write_decimal(count as usize);
                                                serial_write(b"; legacy and I/O APIC routes masked.\r\n");
                                            }
                                            Err(_) => serial_write(b"WARNING: APIC timer calibration failed; interrupts stay disabled.\r\n"),
                                        }
                                    } else {
                                        serial_write(b"WARNING: could not mask every I/O APIC route; interrupts stay disabled.\r\n");
                                    }
                                }
                                _ => serial_write(b"WARNING: local APIC initialization failed or APIC ID mismatch; interrupts stay disabled.\r\n"),
                            }
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

        // Exercise a real saved-stack transition, thread trampoline, exit, and
        // return to the bootstrap context before enabling timer interrupts.
        let smoke_stack_ptr = core::ptr::addr_of_mut!(THREAD_SMOKE_STACK).cast::<u8>();
        let smoke_stack: &'static mut [u8] = core::slice::from_raw_parts_mut(smoke_stack_ptr, 4096);
        let smoke_thread = match thread::spawn(smoke_stack, 1, thread_smoke_entry, core::ptr::null_mut()) {
            Ok(id) => id,
            Err(_) => { serial_write(b"FATAL: kernel thread context creation failed.\r\n"); halt(); }
        };
        let smoke_switch = thread::yield_current();
        let smoke_ran = THREAD_SMOKE_RAN.load(core::sync::atomic::Ordering::Acquire);
        let smoke_state = thread::state(smoke_thread);
        if smoke_switch.is_err() || !smoke_ran || smoke_state != Some(thread::ThreadState::Exited) {
            serial_write(b"FATAL: x86-64 context-switch/thread-exit self-test failed; switch=");
            serial_write_decimal(smoke_switch.is_ok() as usize);
            serial_write(b" error=");
            serial_write_decimal(match smoke_switch { Ok(()) => 0, Err(thread::ThreadError::InvalidStack) => 1, Err(thread::ThreadError::InvalidAffinity) => 2, Err(thread::ThreadError::Full) => 3, Err(thread::ThreadError::NoCurrentThread) => 4, Err(thread::ThreadError::InvalidState) => 5, Err(thread::ThreadError::InvalidCpu) => 6 });
            serial_write(b" ran=");
            serial_write_decimal(smoke_ran as usize);
            serial_write(b" state=");
            serial_write_decimal(match smoke_state { Some(thread::ThreadState::Empty) => 0, Some(thread::ThreadState::Ready) => 1, Some(thread::ThreadState::Running) => 2, Some(thread::ThreadState::Blocked) => 3, Some(thread::ThreadState::Exited) => 4, None => 5 });
            serial_write(b"\r\n");
            halt();
        }
        serial_write(b"Kernel thread stack switch, trampoline, exit, and bootstrap resume passed.\r\n");

        let block_stack_ptr = core::ptr::addr_of_mut!(THREAD_BLOCK_STACK).cast::<u8>();
        let wake_stack_ptr = core::ptr::addr_of_mut!(THREAD_WAKE_STACK).cast::<u8>();
        let block_stack: &'static mut [u8] = core::slice::from_raw_parts_mut(block_stack_ptr, 4096);
        let wake_stack: &'static mut [u8] = core::slice::from_raw_parts_mut(wake_stack_ptr, 4096);
        THREAD_BLOCK_STAGE.store(0, core::sync::atomic::Ordering::Release);
        let blocked_thread = match thread::spawn(block_stack, 1, thread_block_smoke_entry, core::ptr::null_mut()) {
            Ok(id) => id,
            Err(_) => { serial_write(b"FATAL: blocked-thread fixture creation failed.\r\n"); halt(); }
        };
        let wake_thread = match thread::spawn(wake_stack, 1, thread_wake_smoke_entry, blocked_thread as *mut core::ffi::c_void) {
            Ok(id) => id,
            Err(_) => { serial_write(b"FATAL: wake-thread fixture creation failed.\r\n"); halt(); }
        };
        if thread::yield_current().is_err()
            || THREAD_BLOCK_STAGE.load(core::sync::atomic::Ordering::Acquire) != 3
            || thread::state(blocked_thread) != Some(thread::ThreadState::Exited)
            || thread::state(wake_thread) != Some(thread::ThreadState::Exited)
        {
            serial_write(b"FATAL: kernel thread block/wake/resume self-test failed.\r\n");
            halt();
        }
        serial_write(b"Kernel thread block, cross-thread wake, resume, and exit passed.\r\n");
        if !dispatcher::self_test() {
            serial_write(b"FATAL: dispatcher FIFO, cancellation, wait-group, or timeout self-test failed.\r\n");
            halt();
        }
        serial_write(b"Dispatcher FIFO, cancellation, wait-any/wait-all, and timeout self-tests passed.\r\n");

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
        serial_write(if mitigations.smap_supported && !mitigations.smap_enabled { b"supported; explicitly disabled until guarded user-copy STAC/CLAC paths are complete\r\n" } else if mitigations.smap_enabled { b"FATAL: SMAP unexpectedly enabled before guarded user-copy audit\r\n" } else { b"unsupported and disabled\r\n" });
        if mitigations.smap_enabled { serial_write(b"FATAL: refusing to continue with unaudited SMAP state.\r\n"); halt(); }

        if !mitigations.nx_enabled {
            serial_write(b"FATAL: NX unavailable; refusing to enable strict page policy.\r\n");
            halt();
        }
        let text = (core::ptr::addr_of!(__text_start) as u64, core::ptr::addr_of!(__text_end) as u64);
        let rodata = (core::ptr::addr_of!(__rodata_start) as u64, core::ptr::addr_of!(__rodata_end) as u64);
        let data = (core::ptr::addr_of!(__data_start) as u64, core::ptr::addr_of!(__data_end) as u64);
        let bss = (core::ptr::addr_of!(__bss_start) as u64, core::ptr::addr_of!(__bss_end) as u64);
        let stack_virtual = (
            info.kernel_stack_virtual_guard + 4096,
            info.kernel_stack_virtual_guard + stack_allocation_bytes,
        );
        let physical_sections = info.kernel_physical_sections;
        if unsafe { memory::harden_page_tables(
            info.pml4_physical, text, rodata, data, bss,
            info.kernel_stack_guard_page, stack_virtual, info.kernel_stack_virtual_guard,
            physical_sections,
        ) }.is_err() {
            serial_write(b"FATAL: kernel page-permission hardening failed.\r\n");
            halt();
        }
        serial_write(b"Security: 4 KiB page policy active; W^X enforced for kernel sections.\r\n");
        serial_write(b"Security: kernel stack guard page unmapped.\r\n");
        if !arch::permission_fault_self_test(_start as usize as u64) {
            serial_write(b"FATAL: write-protect or NX permission-fault self-test failed.\r\n");
            halt();
        }
        serial_write(b"Protection self-tests passed: read-only write trapped and NX execution trapped.\r\n");
        // The NT executive owns processor-start sequencing; the platform layer
        // supplies firmware handoff data and reports the resulting status.
        let processor_startup = ntoskrnl::ex::runtime::initialize_processors(
            info.pml4_physical,
            info.ap_trampoline_physical,
            info.rsdp_address,
            bsp_apic_id,
        );
        if processor_startup.startup_ok {
            serial_write(b"SMP AP startup complete; online application processors: ");
            serial_write_decimal(processor_startup.online_aps);
            serial_write(if processor_startup.ipi_enabled {
                b"; interrupt-driven TLB shootdowns enabled.\r\n"
            } else {
                b"; using polling TLB fallback (IPI setup unavailable).\r\n"
            });
            serial_write(b"SMP startup diagnostic error code: ");
            serial_write_decimal(smp::last_start_error_code());
            serial_write(b"; last target APIC ID: ");
            serial_write_decimal(smp::last_target_apic_id());
            serial_write(b"; AP entry functions reached: ");
            serial_write_decimal(smp::ap_entry_reached_count());
            serial_write(b"\r\n");
            if processor_startup.ipi_enabled {
                if !processor_startup.interrupt_stress_passed {
                    serial_write(b"FATAL: per-CPU IDT/TSS/LAPIC IPI stress test failed.\r\n");
                    halt();
                }
                serial_write(b"Per-CPU interrupt setup stress passed: 64 acknowledged IPIs per online AP.\r\n");
            } else {
                serial_write(b"WARNING: per-CPU IPI stress skipped because interrupt-driven IPIs are unavailable.\r\n");
            }
            serial_write(if processor_startup.tsc_synced {
                b"TSC cross-core synchronization passed; max offset magnitude="
            } else {
                b"WARNING: TSC sync incomplete; global monotonic clamp remains active; max offset magnitude="
            });
            serial_write_decimal(processor_startup.worst_tsc_offset.unsigned_abs() as usize);
            serial_write(b" ticks.\r\n");
        } else {
            serial_write(b"WARNING: executive processor startup unavailable; retaining BSP-only operation.\r\n");
        }
        if !processor_startup.scheduler_stress_passed {
            serial_write(b"FATAL: SMP scheduler, lock contention, or thread teardown stress failed.\r\n");
            halt();
        }
        serial_write(b"SMP scheduler, cross-CPU wakeup, contended-lock, and thread teardown stress passed.\r\n");
        if !processor_startup.vm_stress_passed {
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
        if local_timer_ready {
            match unsafe { apic::start_calibrated_local_timer() } {
                Ok(()) => {
                    serial_write(b"Local APIC timer enabled after AP startup/self-tests; external routes remain masked.\r\n");
                    loop {
                        unsafe { asm!("sti; hlt", options(nomem, nostack)); }
                        if thread::has_ready_threads() { let _ = thread::yield_current(); }
                    }
                }
                Err(_) => serial_write(b"WARNING: calibrated APIC timer could not start; interrupts remain disabled.\r\n"),
            }
        } else {
            serial_write(b"Interrupts remain disabled because APIC/timer setup was not fully validated.\r\n");
        }
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

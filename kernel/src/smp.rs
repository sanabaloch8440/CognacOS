//! Early x86-64 AP startup. APs are brought online with interrupts disabled;
//! scheduling and external interrupt delivery remain separate milestones.
//! SPDX-License-Identifier: GPL-2.0-or-later
//!
//! The temporary AP page-table and trampoline startup design is adapted from
//! RustOS-Dev/RustOS, src/arch/x86_64/smp.rs
//! (https://github.com/RustOS-Dev/RustOS/blob/main/src/arch/x86_64/smp.rs),
//! licensed GPL-2.0-or-later.
//! Copyright (C) RustOS-Dev contributors; RdotOS-specific modifications
//! Copyright (C) 2026 RdotOS contributors. Modified for RdotOS on 2026-10-23.
use crate::{hal, memory};
use crate::ntoskrnl::ke::arch;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use core::arch::global_asm;

const PAGE_SIZE: usize = 4096;
static LOCAL_APIC_BASE: AtomicU64 = AtomicU64::new(0);
static ONLINE_APS: AtomicUsize = AtomicUsize::new(0);
static LAST_START_ERROR: AtomicUsize = AtomicUsize::new(0);
static AP_ENTRY_REACHED: AtomicUsize = AtomicUsize::new(0);
static LAST_TARGET_APIC_ID: AtomicUsize = AtomicUsize::new(usize::MAX);
static INTERRUPTS_ENABLED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static IPI_ACTIVE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static CPU_INTERRUPT_READY: [core::sync::atomic::AtomicBool; hal::MAX_CPUS] = [const { core::sync::atomic::AtomicBool::new(false) }; hal::MAX_CPUS];
static TLB_IPI_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

global_asm!(r#"
    .section .rodata.ap_trampoline,"a",@progbits
    .balign 16
    .global rdotos_ap_trampoline_start
    .global rdotos_ap_trampoline_end
    .global rdotos_ap_trampoline_pm32
    .global rdotos_ap_trampoline_long64
    .global rdotos_ap_trampoline_gdt
    .global rdotos_ap_trampoline_gdt_base_patch
    .global rdotos_ap_trampoline_gdt_pointer
    .global rdotos_ap_trampoline_gdt_offset_patch
    .global rdotos_ap_trampoline_stack_offset_patch
    .global rdotos_ap_trampoline_entry_offset_patch
    .global rdotos_ap_trampoline_logical_offset_patch
    .global rdotos_ap_trampoline_pm32_patch
    .global rdotos_ap_trampoline_long64_patch
    .global rdotos_ap_trampoline_pml4_absolute_patch
    .global rdotos_ap_trampoline_long64_base_patch
    .global rdotos_ap_trampoline_pml4
    .global rdotos_ap_trampoline_stack
    .global rdotos_ap_trampoline_entry
    .global rdotos_ap_trampoline_logical
    .global rdotos_ap_trampoline_kernel_pml4
    .global rdotos_ap_trampoline_kernel_pml4_offset_patch
    .global rdotos_ap_trampoline_cr4
    .global rdotos_ap_trampoline_cr4_offset_patch
rdotos_ap_trampoline_start:
    .code16
    cli
    cld
    mov ax, cs
    mov ds, ax
    mov es, ax
    .byte 0x0f, 0x01, 0x16
rdotos_ap_trampoline_gdt_offset_patch:
    .word 0
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    .byte 0x66, 0xea
rdotos_ap_trampoline_pm32_patch:
    .long 0
    .word 0x08

    .code32
rdotos_ap_trampoline_pm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    # Load CR3 through an absolute low-memory address; no stack is valid yet.
    .byte 0xa1
rdotos_ap_trampoline_pml4_absolute_patch:
    .long 0
    mov cr3, eax
    # Match the BSP CR4 state, force PAE, and clear PCIDE before paging.
    .byte 0x8b, 0x86
rdotos_ap_trampoline_cr4_offset_patch:
    .long 0
    or eax, 0x20
    and eax, 0xfffdffff
    mov cr4, eax
    mov ecx, 0xc0000080
    rdmsr
    # Enable SCE, LME and NXE before activating page tables with NX entries.
    or eax, 0x901
    wrmsr
    mov eax, cr0
    # Enable paging, write protection, and protected mode as on the BSP.
    or eax, 0x80010001
    mov cr0, eax
    .byte 0xea
rdotos_ap_trampoline_long64_patch:
    .long 0
    .word 0x18

    .code64
rdotos_ap_trampoline_long64:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    # Establish the trampoline physical base without touching the AP stack.
    .byte 0x48, 0xbe
rdotos_ap_trampoline_long64_base_patch:
    .quad 0
    .byte 0x48, 0x8b, 0xa6
rdotos_ap_trampoline_stack_offset_patch:
    .long 0
    and rsp, -16
    sub rsp, 8
    .byte 0x8b, 0xbe
rdotos_ap_trampoline_logical_offset_patch:
    .long 0
    .byte 0x48, 0x8b, 0x86
rdotos_ap_trampoline_entry_offset_patch:
    .long 0
    # Pass permanent kernel CR3 as the second SysV argument to AP entry.
    .byte 0x48, 0x8b, 0x96
rdotos_ap_trampoline_kernel_pml4_offset_patch:
    .long 0
    mov rsi, rdx
    # Third argument is the original top of this AP's bootstrap stack.
    lea rdx, [rsp + 8]
    jmp rax
3:  pause
    jmp 3b

    .balign 8
rdotos_ap_trampoline_gdt:
    .quad 0
    .quad 0x00cf9a000000ffff
    .quad 0x00cf92000000ffff
    .quad 0x00af9a000000ffff
rdotos_ap_trampoline_gdt_pointer:
    .word 31
rdotos_ap_trampoline_gdt_base_patch:
    .long 0
    .balign 8
rdotos_ap_trampoline_pml4:
    .long 0
rdotos_ap_trampoline_stack:
    .quad 0
rdotos_ap_trampoline_entry:
    .quad 0
rdotos_ap_trampoline_logical:
    .long 0
    .balign 8
rdotos_ap_trampoline_kernel_pml4:
    .quad 0
    .balign 8
rdotos_ap_trampoline_cr4:
    .long 0
rdotos_ap_trampoline_end:
"#);

unsafe extern "C" {
    static rdotos_ap_trampoline_start: u8;
    static rdotos_ap_trampoline_end: u8;
    static rdotos_ap_trampoline_pm32: u8;
    static rdotos_ap_trampoline_long64: u8;
    static rdotos_ap_trampoline_gdt: u8;
    static rdotos_ap_trampoline_gdt_base_patch: u8;
    static rdotos_ap_trampoline_gdt_pointer: u8;
    static rdotos_ap_trampoline_gdt_offset_patch: u8;
    static rdotos_ap_trampoline_stack_offset_patch: u8;
    static rdotos_ap_trampoline_entry_offset_patch: u8;
    static rdotos_ap_trampoline_logical_offset_patch: u8;
    static rdotos_ap_trampoline_pm32_patch: u8;
    static rdotos_ap_trampoline_long64_patch: u8;
    static rdotos_ap_trampoline_pml4_absolute_patch: u8;
    static rdotos_ap_trampoline_long64_base_patch: u8;
    static rdotos_ap_trampoline_pml4: u8;
    static rdotos_ap_trampoline_stack: u8;
    static rdotos_ap_trampoline_entry: u8;
    static rdotos_ap_trampoline_logical: u8;
    static rdotos_ap_trampoline_kernel_pml4: u8;
    static rdotos_ap_trampoline_kernel_pml4_offset_patch: u8;
    static rdotos_ap_trampoline_cr4: u8;
    static rdotos_ap_trampoline_cr4_offset_patch: u8;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmpError {
    InvalidTarget,
    UnsupportedApicId,
    InvalidCpuState,
    StartupTimeout,
    Apic(hal::HalError),
    Hal(hal::HalError),
    Memory(memory::AllocError),
    DelayUnavailable,
    TrampolineTooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApStartupTarget {
    pub logical_id: usize,
    pub apic_id: u32,
    pub trampoline_physical: u64,
    pub stack_top: u64,
    pub pml4_physical: u64,
}
impl ApStartupTarget {
    pub fn validate(self) -> Result<Self, SmpError> {
        if self.logical_id == 0 || self.trampoline_physical == 0
            || self.trampoline_physical >= 0x10_0000 || self.trampoline_physical & 0xfff != 0
            || self.stack_top == 0 || self.stack_top & 0xf != 0
            || self.stack_top >= 0x1_0000_0000 || self.pml4_physical == 0
            || self.pml4_physical & 0xfff != 0 || self.pml4_physical >= 0x1_0000_0000
        { return Err(SmpError::InvalidTarget); }
        if self.apic_id > u8::MAX as u32 { return Err(SmpError::UnsupportedApicId); }
        Ok(self)
    }
}

#[inline]
unsafe fn outb(port: u16, value: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}
#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe { core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags)); }
    value
}

/// Calibrated early delay from the PC-compatible PIT channel 2 oscillator.
/// Returns false if OUT2 never transitions before the bounded poll expires.
fn pit_delay_us(microseconds: u32) -> bool {
    if microseconds == 0 { return true; }
    let ticks = ((microseconds as u64 * 1_193_182) + 999_999) / 1_000_000;
    if ticks == 0 || ticks > u16::MAX as u64 { return false; }
    unsafe {
        let speaker = inb(0x61);
        outb(0x61, speaker & !0x03);
        outb(0x43, 0xB0); // channel 2, lo/hi byte, mode 0, binary
        outb(0x42, ticks as u8);
        outb(0x42, (ticks >> 8) as u8);
        outb(0x61, (speaker & !0x02) | 0x01); // gate on, speaker off
        for _ in 0..10_000_000usize {
            if inb(0x61) & 0x20 != 0 {
                outb(0x61, inb(0x61) & !0x01);
                return true;
            }
            core::hint::spin_loop();
        }
        outb(0x61, inb(0x61) & !0x01);
    }
    false
}

unsafe fn symbol_offset(symbol: *const u8) -> usize {
    // Linker-defined labels are contiguous in the same trampoline section; use
    // integer addresses rather than Rust pointer provenance across extern symbols.
    (symbol as usize).wrapping_sub(core::ptr::addr_of!(rdotos_ap_trampoline_start) as usize)
}
unsafe fn write_patch_u16(page: u64, symbol: *const u8, value: u16) {
    let offset = unsafe { symbol_offset(symbol) };
    unsafe { core::ptr::write_unaligned((page as *mut u8).add(offset).cast::<u16>(), value); }
}
unsafe fn write_patch_u32(page: u64, symbol: *const u8, value: u32) {
    let offset = unsafe { symbol_offset(symbol) };
    unsafe { core::ptr::write_unaligned((page as *mut u8).add(offset).cast::<u32>(), value); }
}
unsafe fn write_patch_u64(page: u64, symbol: *const u8, value: u64) {
    let offset = unsafe { symbol_offset(symbol) };
    unsafe { core::ptr::write_unaligned((page as *mut u8).add(offset).cast::<u64>(), value); }
}

/// Build a temporary CR3 from the kernel page tables while guaranteeing a
/// 2 MiB identity mapping for the low-memory SIPI trampoline. This follows the
/// RustOS-Dev/RustOS AP-startup design: preserve existing mappings, clone the
/// low PDPT/PD, and replace only PD[0] with an identity-mapped 2 MiB page.
/// The temporary root is used only during trampoline execution; AP entry
/// switches immediately to the permanent kernel CR3.
unsafe fn prepare_ap_temporary_page_tables(kernel_pml4: u64) -> Result<u64, SmpError> {
    const PRESENT: u64 = 1;
    const WRITABLE: u64 = 1 << 1;
    const LARGE: u64 = 1 << 7;
    const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
    const LARGE_1G_MASK: u64 = 0x000f_ffff_c000_0000;
    const LARGE_2M_MASK: u64 = 0x000f_ffff_ffe0_0000;
    if kernel_pml4 == 0 || kernel_pml4 >= 0x1_0000_0000 || kernel_pml4 & 0xfff != 0 {
        return Err(SmpError::InvalidTarget);
    }
    let temporary_pml4 = unsafe { memory::allocate_page() }.map_err(SmpError::Memory)?;
    let temporary_pdpt = unsafe { memory::allocate_page() }.map_err(SmpError::Memory)?;
    let temporary_pd = unsafe { memory::allocate_page() }.map_err(SmpError::Memory)?;
    unsafe {
        let source_pml4 = kernel_pml4 as *const u64;
        let target_pml4 = temporary_pml4 as *mut u64;
        core::ptr::copy_nonoverlapping(source_pml4, target_pml4, 512);

        let source_pdpt_entry = *source_pml4;
        let target_pdpt = temporary_pdpt as *mut u64;
        if source_pdpt_entry & PRESENT != 0 {
            core::ptr::copy_nonoverlapping(
                (source_pdpt_entry & ADDRESS_MASK) as *const u64,
                target_pdpt,
                512,
            );
        } else {
            core::ptr::write_bytes(target_pdpt, 0, 512);
        }

        let old_pdpt0 = *target_pdpt;
        let target_pd = temporary_pd as *mut u64;
        if old_pdpt0 & PRESENT != 0 && old_pdpt0 & LARGE != 0 {
            let base = old_pdpt0 & LARGE_1G_MASK;
            let inherited = old_pdpt0 & (WRITABLE | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 8) | (1 << 63));
            for index in 0..512usize {
                target_pd.add(index).write(
                    (base + index as u64 * 0x20_0000) & LARGE_2M_MASK
                        | PRESENT | LARGE | inherited,
                );
            }
        } else if old_pdpt0 & PRESENT != 0 {
            core::ptr::copy_nonoverlapping(
                (old_pdpt0 & ADDRESS_MASK) as *const u64,
                target_pd,
                512,
            );
        } else {
            core::ptr::write_bytes(target_pd, 0, 512);
        }

        // The temporary mapping guarantees the AP can fetch the SIPI vector
        // and read its patched parameters before switching to the kernel CR3.
        target_pd.write(0x0000_0000_0000_0083);
        target_pdpt.write(temporary_pd | PRESENT | WRITABLE);
        target_pml4.write(temporary_pdpt | PRESENT | WRITABLE);
    }
    Ok(temporary_pml4)
}

/// Copy and relocate the self-contained real-mode trampoline. The page is made
/// writable/NX while patched and RX/RO before any SIPI is sent.
unsafe fn prepare_trampoline(
    pml4: u64,
    startup_pml4: u64,
    page: u64,
    stack_top: u64,
    logical_id: usize,
) -> Result<(), SmpError> {
    let start = core::ptr::addr_of!(rdotos_ap_trampoline_start);
    let end = core::ptr::addr_of!(rdotos_ap_trampoline_end);
    let length = (end as usize).wrapping_sub(start as usize);
    if length == 0 || length > PAGE_SIZE { return Err(SmpError::TrampolineTooLarge); }
    unsafe { memory::configure_identity_page(pml4, page, true, false, false, false) }
        .map_err(SmpError::Memory)?;
    unsafe {
        core::ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE);
        core::ptr::copy_nonoverlapping(start, page as *mut u8, length);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_gdt_base_patch),
            (page + symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_gdt)) as u64) as u32);
        write_patch_u16(page, core::ptr::addr_of!(rdotos_ap_trampoline_gdt_offset_patch),
            symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_gdt_pointer)) as u16);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_pml4_absolute_patch),
            (page + symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_pml4)) as u64) as u32);
        write_patch_u64(page, core::ptr::addr_of!(rdotos_ap_trampoline_long64_base_patch), page);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_stack_offset_patch),
            symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_stack)) as u32);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_logical_offset_patch),
            symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_logical)) as u32);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_entry_offset_patch),
            symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_entry)) as u32);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_pm32_patch),
            (page + symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_pm32)) as u64) as u32);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_long64_patch),
            (page + symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_long64)) as u64) as u32);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_pml4), startup_pml4 as u32);
        write_patch_u64(page, core::ptr::addr_of!(rdotos_ap_trampoline_stack), stack_top);
        write_patch_u64(page, core::ptr::addr_of!(rdotos_ap_trampoline_entry), rdotos_ap_entry as *const () as usize as u64);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_logical), logical_id as u32);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_kernel_pml4_offset_patch),
            symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_kernel_pml4)) as u32);
        write_patch_u64(page, core::ptr::addr_of!(rdotos_ap_trampoline_kernel_pml4), pml4);
        let bsp_cr4: u64;
        core::arch::asm!("mov {}, cr4", out(reg) bsp_cr4, options(nomem, nostack, preserves_flags));
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_cr4_offset_patch),
            symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_cr4)) as u32);
        write_patch_u32(page, core::ptr::addr_of!(rdotos_ap_trampoline_cr4), (bsp_cr4 as u32) & !(1 << 17));
        memory::configure_identity_page(pml4, page, false, true, false, false)
            .map_err(SmpError::Memory)?;

    }
    Ok(())
}

/// Start one discovered AP. The supplied delay must be a calibrated monotonic
/// delay; IF stays clear and no AP is marked online until its entry confirms setup.
pub unsafe fn start_one_ap(
    local_apic_base: u64,
    target: ApStartupTarget,
    delay_us: fn(u32) -> bool,
) -> Result<(), SmpError> {
    let target = target.validate()?;
    if hal::cpu_state(target.logical_id).ok() != Some(hal::CPU_DISCOVERED)
        || hal::cpu_local(target.logical_id)
            .map(|cpu| cpu.apic_id.load(Ordering::Acquire)).ok() != Some(target.apic_id)
    { return Err(SmpError::InvalidCpuState); }
    hal::mark_cpu_starting(target.logical_id).map_err(SmpError::Hal)?;
    let startup = unsafe { hal::send_processor_init(local_apic_base, target.apic_id as u8) };
    if let Err(error) = startup { let _ = hal::mark_cpu_failed(target.logical_id); return Err(SmpError::Apic(error)); }
    if !delay_us(10_000) { let _ = hal::mark_cpu_failed(target.logical_id); return Err(SmpError::DelayUnavailable); }
    // Do not send the legacy INIT-deassert IPI here. Its delivery mode is
    // deprecated and firmware/APIC implementations differ in how they handle
    // it. After the INIT recovery interval, send the first SIPI immediately;
    // the mandatory 200 us inter-SIPI delay belongs after that first SIPI.
    if let Err(error) = unsafe { hal::send_processor_startup(local_apic_base, target.apic_id as u8, target.trampoline_physical) } {
        let _ = hal::mark_cpu_failed(target.logical_id); return Err(SmpError::Apic(error));
    }
    if !delay_us(200) { let _ = hal::mark_cpu_failed(target.logical_id); return Err(SmpError::DelayUnavailable); }
    // Follow the conventional INIT-SIPI-SIPI sequence: issue the second
    // SIPI after the 200 us startup interval if the AP has not reported online.
    // A SIPI received by an AP that is already executing is ignored by hardware,
    // while a delayed retry can miss the compatibility window on some platforms.
    if hal::cpu_state(target.logical_id).ok() != Some(hal::CPU_ONLINE) {
        if let Err(error) = unsafe {
            hal::send_processor_startup(local_apic_base, target.apic_id as u8, target.trampoline_physical)
        } {
            let _ = hal::mark_cpu_failed(target.logical_id);
            return Err(SmpError::Apic(error));
        }
    }
    let mut waited = 0u32;
    while waited < 100_000 {
        if hal::cpu_state(target.logical_id).ok() == Some(hal::CPU_ONLINE) { return Ok(()); }
        if !delay_us(100) {
            let _ = hal::mark_cpu_failed(target.logical_id);
            return Err(SmpError::DelayUnavailable);
        }
        waited += 100;
    }
    let _ = hal::mark_cpu_failed(target.logical_id);
    Err(SmpError::StartupTimeout)
}

fn error_code(error: SmpError) -> usize {
    match error {
        SmpError::InvalidTarget => 1,
        SmpError::UnsupportedApicId => 2,
        SmpError::InvalidCpuState => 3,
        SmpError::StartupTimeout => 4,
        SmpError::Apic(_) => 5,
        SmpError::Hal(_) => 6,
        SmpError::Memory(_) => 7,
        SmpError::DelayUnavailable => 8,
        SmpError::TrampolineTooLarge => 9,
    }
}
pub fn last_start_error_code() -> usize { LAST_START_ERROR.load(Ordering::Acquire) }
pub fn ap_entry_reached_count() -> usize { AP_ENTRY_REACHED.load(Ordering::Acquire) }
pub fn last_target_apic_id() -> usize { LAST_TARGET_APIC_ID.load(Ordering::Acquire) }

/// Complete the interrupt-safety sequence around AP startup. All legacy PIC
/// and firmware I/O APIC routes must be masked before enabling IF for IPIs.
pub unsafe fn start_discovered_aps_with_ipi(
    pml4_physical: u64,
    trampoline_physical: u64,
    madt: &crate::acpi::MadtInfo,
    bsp_apic_id: u32,
) -> Result<(usize, bool), SmpError> {
    let sources_masked = unsafe { mask_interrupt_sources(pml4_physical, madt) };
    let online = unsafe { start_discovered_aps(pml4_physical, trampoline_physical, madt.local_apic_address, bsp_apic_id)? };
    let ipi_enabled = sources_masked && enable_ipi_interrupts();
    if ipi_enabled {
        // Calibrate every online AP into the BSP's TSC epoch only after the
        // dedicated sync IPI gate is installed and target CPUs have enabled IF.
        for logical_id in 1..hal::MAX_CPUS {
            if hal::cpu_state(logical_id).ok() != Some(hal::CPU_ONLINE) { continue; }
            let Ok(cpu) = hal::cpu_local(logical_id) else { continue; };
            let apic_id = cpu.apic_id.load(Ordering::Acquire);
            if apic_id <= u8::MAX as u32 {
                let _ = unsafe { hal::synchronize_processor_clock(logical_id, apic_id as u8) };
            }
        }
    }
    Ok((online, ipi_enabled))
}

/// Enable xAPIC MMIO as uncached, install the real-mode trampoline, and bring
/// each MADT-discovered AP online. APs are left in a safe interrupt-disabled
/// idle loop until per-CPU interrupt routing and scheduling are implemented.
pub unsafe fn start_discovered_aps(
    pml4_physical: u64,
    trampoline_physical: u64,
    local_apic_base: u64,
    bsp_apic_id: u32,
) -> Result<usize, SmpError> {
    if trampoline_physical == 0 || trampoline_physical >= 0x10_0000
        || trampoline_physical & 0xfff != 0 || local_apic_base == 0
        || local_apic_base >= 0x1_0000_0000 || local_apic_base & 0xfff != 0
    { return Err(SmpError::InvalidTarget); }
    unsafe { memory::configure_identity_page(pml4_physical, local_apic_base, true, false, true, true) }
        .map_err(SmpError::Memory)?;
    let current_id = unsafe { hal::initialize_processor_interrupt_controller(local_apic_base) }.map_err(SmpError::Apic)?;
    if current_id != bsp_apic_id { return Err(SmpError::InvalidCpuState); }
    LOCAL_APIC_BASE.store(local_apic_base, Ordering::Release);
    let startup_pml4 = unsafe { prepare_ap_temporary_page_tables(pml4_physical)? };

    let mut online = 0usize;
    for logical_id in 1..hal::MAX_CPUS {
        if hal::cpu_state(logical_id).ok() != Some(hal::CPU_DISCOVERED) { continue; }
        let cpu = hal::cpu_local(logical_id).map_err(SmpError::Hal)?;
        let apic_id = cpu.apic_id.load(Ordering::Acquire);
        LAST_TARGET_APIC_ID.store(apic_id as usize, Ordering::Release);
        // Never send INIT to the BSP: a mistaken MADT/HAL association would
        // reset the CPU coordinating startup and restart the boot sequence.
        if apic_id == bsp_apic_id {
            LAST_START_ERROR.store(error_code(SmpError::InvalidCpuState), Ordering::Release);
            let _ = hal::mark_cpu_failed(logical_id);
            continue;
        }
        if apic_id > u8::MAX as u32 {
            LAST_START_ERROR.store(error_code(SmpError::UnsupportedApicId), Ordering::Release);
            let _ = hal::mark_cpu_failed(logical_id);
            continue;
        }
        // Match the proven RustOS bring-up pattern: give each AP a real kernel
        // stack, not a single 4 KiB page that can overflow during initialization.
        const AP_STACK_PAGES: usize = 16;
        let stack_base = unsafe {
            memory::allocate_contiguous_pages_below(
                AP_STACK_PAGES,
                0xffff_f000,
            )
        }.map_err(SmpError::Memory)?;
        let stack_top = stack_base + (AP_STACK_PAGES * PAGE_SIZE) as u64;
        if stack_top >= 0x1_0000_0000 {
            let _ = hal::mark_cpu_failed(logical_id);
            return Err(SmpError::InvalidTarget);
        }
        let target = ApStartupTarget { logical_id, apic_id, trampoline_physical, stack_top, pml4_physical };
        unsafe { prepare_trampoline(pml4_physical, startup_pml4, trampoline_physical, stack_top, logical_id)?; }
        unsafe {
            let long_target = core::ptr::read_unaligned(
                (trampoline_physical as *const u8)
                    .add(symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_long64_patch)))
                    .cast::<u32>(),
            );
            let entry_target = core::ptr::read_unaligned(
                (trampoline_physical as *const u8)
                    .add(symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_entry)))
                    .cast::<u64>(),
            );
            let stack_target = core::ptr::read_unaligned(
                (trampoline_physical as *const u8)
                    .add(symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_stack)))
                    .cast::<u64>(),
            );
            let logical_target = core::ptr::read_unaligned(
                (trampoline_physical as *const u8)
                    .add(symbol_offset(core::ptr::addr_of!(rdotos_ap_trampoline_logical)))
                    .cast::<u32>(),
            );
            crate::serial_write(b"SMP starting logical CPU ");
            crate::serial_write_decimal(logical_id);
            crate::serial_write(b" APIC ID ");
            crate::serial_write_decimal(apic_id as usize);
            crate::serial_write(b" long-mode target ");
            crate::serial_write_hex(long_target as u64);
            crate::serial_write(b" entry target ");
            crate::serial_write_hex(entry_target);
            crate::serial_write(b" stack target ");
            crate::serial_write_hex(stack_target);
            crate::serial_write(b" logical field ");
            crate::serial_write_decimal(logical_target as usize);
            crate::serial_write(b"\r\n");
        }
        match unsafe { start_one_ap(local_apic_base, target, pit_delay_us) } {
            Ok(()) => { online += 1; }
            Err(error) => {
                LAST_START_ERROR.store(error_code(error), Ordering::Release);
                let _ = hal::mark_cpu_failed(logical_id);
                // A timed-out AP may still be executing the low-memory page.
                // Never repatch that single-target trampoline after a failure.
                break;
            }
        }
    }
    Ok(online)
}

#[unsafe(no_mangle)]
pub extern "C" fn rdotos_ap_entry(logical_id: u32, kernel_pml4: u64, ap_stack_top: u64) -> ! {
    // The trampoline uses temporary tables to guarantee its low-memory mapping.
    // Switch to the permanent kernel root before touching global CPU state.
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) kernel_pml4, options(nostack, preserves_flags)); }
    AP_ENTRY_REACHED.fetch_add(1, Ordering::AcqRel);
    unsafe { crate::serial_write(b"SMP AP entry reached.\r\n"); }
    let index = logical_id as usize;
    // Reject corrupt trampoline metadata before indexing CPU-local state. The
    // AP has already switched to the permanent CR3, but is not yet published.
    if index == 0 || index >= hal::MAX_CPUS {
        unsafe { crate::serial_write(b"SMP AP rejected invalid logical CPU index.\r\n"); }
        loop { core::hint::spin_loop(); }
    }
    // Install a private TSS/GDT and dedicated IST stacks on this CPU before
    // it can receive interrupts. Use the original stack top, not the current
    // deeper call-frame RSP, for future privilege transitions and idle returns.
    if ap_stack_top == 0 || ap_stack_top & 0xf != 0
        || unsafe { arch::load_cpu_descriptor_tables(index, ap_stack_top) }.is_err()
        || !crate::thread::set_idle_stack_top(index, ap_stack_top) {
        let _ = hal::mark_cpu_failed(index);
        loop { core::hint::spin_loop(); }
    }
    let mitigations = unsafe { crate::ntoskrnl::ke::mitigations::initialize() };
    if !mitigations.write_protect || !mitigations.nx_enabled {
        let _ = hal::mark_cpu_failed(index);
        loop { core::hint::spin_loop(); }
    }
    let base = LOCAL_APIC_BASE.load(Ordering::Acquire);
    let apic_id = if base != 0 {
        unsafe { hal::initialize_processor_interrupt_controller(base) }.ok()
    } else { None };
    let expected = hal::cpu_local(index)
        .map(|cpu| cpu.apic_id.load(Ordering::Acquire)).ok();
    if apic_id.is_none() || apic_id != expected
        || unsafe { hal::install_cpu_local(index) }.is_err()
    {
        let _ = hal::mark_cpu_failed(index);
        loop { core::hint::spin_loop(); }
    }
    // Keep the Ki processor-initialization gate common to BSP and APs. The
    // descriptor tables and GS-local pointer are installed before validation;
    // only a validated processor is published online.
    if !crate::ntoskrnl::ke::runtime::ki_initialize_processor(index)
        || hal::mark_cpu_online(index).is_err()
    {
        let _ = hal::mark_cpu_failed(index);
        loop { core::hint::spin_loop(); }
    }
    ONLINE_APS.fetch_add(1, Ordering::AcqRel);
    // If the BSP calibrated the timer quantum before AP startup, arm this AP's
    // local timer too. Failure leaves the AP online for IPI/polling work.
    let _ = unsafe { hal::start_local_clock_event() };
    // Until the BSP masks all legacy and I/O APIC routes, keep IF clear. Once
    // enabled, retain polling as a fail-safe if an IPI send cannot be delivered.
    loop {
        if INTERRUPTS_ENABLED.load(Ordering::Acquire) {
            unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
            CPU_INTERRUPT_READY[index].store(true, Ordering::Release);
            // Once the IPI path is fully active, sleep until an interrupt arrives.
            // If setup timed out, keep polling so the documented fallback cannot deadlock.
            loop {
                if IPI_ACTIVE.load(Ordering::Acquire) {
                    // STI;HLT closes the check-to-sleep lost-wakeup window:
                    // an IPI arriving after the check will wake this HLT.
                    unsafe { core::arch::asm!("sti; hlt", options(nomem, nostack)); }
                    crate::ntoskrnl::ke::runtime::dispatch_threaded_dpcs();
                    if hal::take_pending_ipi(index).unwrap_or(false)
                        || crate::thread::has_ready_threads() {
                        let _ = crate::thread::yield_current();
                    }
                } else {
                    crate::kernel_vm::smp_tlb_poll(index);
                    core::hint::spin_loop();
                }
            }
        }
        crate::kernel_vm::smp_tlb_poll(index);
        core::hint::spin_loop();
    }
}

pub fn online_ap_count() -> usize { ONLINE_APS.load(Ordering::Acquire) }

/// Mask legacy PIC and every firmware-described I/O APIC route before IF is
/// enabled. I/O APIC MMIO is explicitly configured uncached first.
pub unsafe fn mask_interrupt_sources(pml4_physical: u64, madt: &crate::acpi::MadtInfo) -> bool {
    unsafe { hal::write_port_u8(0x21, 0xff); hal::write_port_u8(0xa1, 0xff); }
    for io in madt.io_apics.iter().take(madt.io_apic_count) {
        let base = io.address as u64;
        if base == 0 || base >= 0x1_0000_0000 || base & 0xfff != 0 { return false; }
        if unsafe { memory::configure_identity_page(pml4_physical, base, true, false, true, true) }.is_err() { return false; }
        if unsafe { hal::mask_interrupt_controller_routes(base) }.is_err() { return false; }
    }
    true
}

/// Enable only the dedicated TLB IPI path after all external interrupt sources
/// are masked. APs acknowledge that STI has executed before the BSP enables IF.
pub fn enable_ipi_interrupts() -> bool {
    INTERRUPTS_ENABLED.store(true, Ordering::Release);
    for cpu in 1..hal::MAX_CPUS {
        if hal::cpu_state(cpu).ok() != Some(hal::CPU_ONLINE) { continue; }
        let mut spins = 0usize;
        while !CPU_INTERRUPT_READY[cpu].load(Ordering::Acquire) && spins < 10_000_000 {
            spins += 1; core::hint::spin_loop();
        }
        if !CPU_INTERRUPT_READY[cpu].load(Ordering::Acquire) { return false; }
    }
    IPI_ACTIVE.store(true, Ordering::Release);
    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    true
}

pub fn ipi_shootdowns_enabled() -> bool { IPI_ACTIVE.load(Ordering::Acquire) }
pub fn tlb_ipi_count() -> u64 { TLB_IPI_COUNT.load(Ordering::Acquire) }

/// Stress the actual per-CPU IDT/LAPIC path after all APs have installed their
/// descriptor tables, enabled IF, and acknowledged interrupt readiness. Each
/// AP must service every directed IPI before the bounded deadline; a failed
/// delivery disables the test result rather than being mistaken for a pass.
pub fn interrupt_setup_stress_test(rounds: u32) -> bool {
    if rounds == 0 || !IPI_ACTIVE.load(Ordering::Acquire) { return false; }
    for logical_id in 1..hal::MAX_CPUS {
        if hal::cpu_state(logical_id).ok() != Some(hal::CPU_ONLINE) { continue; }
        if !CPU_INTERRUPT_READY[logical_id].load(Ordering::Acquire) { return false; }
        for _ in 0..rounds {
            let before = TLB_IPI_COUNT.load(Ordering::Acquire);
            if send_tlb_shootdown(logical_id).is_err() { return false; }
            let mut spins = 0usize;
            while TLB_IPI_COUNT.load(Ordering::Acquire) == before && spins < 5_000_000 {
                spins += 1;
                core::hint::spin_loop();
            }
            if TLB_IPI_COUNT.load(Ordering::Acquire) == before { return false; }
        }
    }
    true
}

/// Send a fixed-vector IPI to a logical CPU. The target's IDT gate and LAPIC
/// are already initialized by the AP startup protocol.
/// Notify an online CPU that runnable kernel-thread work may be available.
/// This uses a distinct HAL vector and never overloads the synchronous TLB IPI.
pub fn request_reschedule(logical_id: usize) -> bool {
    if !IPI_ACTIVE.load(Ordering::Acquire) || logical_id >= hal::MAX_CPUS
        || hal::cpu_state(logical_id).ok() != Some(hal::CPU_ONLINE) {
        return false;
    }
    let Ok(cpu) = hal::cpu_local(logical_id) else { return false; };
    let apic_id = cpu.apic_id.load(Ordering::Acquire);
    if apic_id > u8::MAX as u32 { return false; }
    let base = LOCAL_APIC_BASE.load(Ordering::Acquire);
    base != 0 && unsafe { hal::send_fixed_ipi(base, apic_id as u8, hal::HAL_IPI_VECTOR) }.is_ok()
}

/// Send a dedicated Ke rendezvous IPI. This vector is distinct from the
/// scheduler/reschedule vector so unrelated scheduler IPIs cannot consume a
/// broadcast callback or accidentally execute it twice.
pub fn request_ke_broadcast(logical_id:usize)->bool{
    if !IPI_ACTIVE.load(Ordering::Acquire)||logical_id>=hal::MAX_CPUS
        ||hal::cpu_state(logical_id).ok()!=Some(hal::CPU_ONLINE){return false}
    let Ok(cpu)=hal::cpu_local(logical_id) else{return false};
    let apic_id=cpu.apic_id.load(Ordering::Acquire);
    if apic_id>u8::MAX as u32{return false}
    let base=LOCAL_APIC_BASE.load(Ordering::Acquire);
    base!=0&&unsafe{hal::send_fixed_ipi(base,apic_id as u8,hal::KE_BROADCAST_VECTOR)}.is_ok()
}

pub fn send_tlb_shootdown(logical_id: usize) -> Result<(), hal::HalError> {
    if !ipi_shootdowns_enabled() { return Err(hal::HalError::PlatformFailure); }
    let cpu = hal::cpu_local(logical_id).map_err(|_| hal::HalError::InvalidCpu)?;
    let apic_id = cpu.apic_id.load(Ordering::Acquire);
    if apic_id > u8::MAX as u32 { return Err(hal::HalError::PlatformFailure); }
    let base = LOCAL_APIC_BASE.load(Ordering::Acquire);
    unsafe { hal::send_fixed_ipi(base, apic_id as u8, hal::TLB_SHOOTDOWN_VECTOR) }
}

#[unsafe(no_mangle)]
#[export_name = "cognacos_tlb_ipi_handler"]
pub extern "C" fn rdotos_tlb_ipi_handler() {
    TLB_IPI_COUNT.fetch_add(1, Ordering::Relaxed);
    crate::kernel_vm::handle_tlb_ipi();
    let base = LOCAL_APIC_BASE.load(Ordering::Acquire);
    if base != 0 { unsafe { hal::end_local_interrupt(); } }
}

pub fn self_test() -> bool {
    let target = ApStartupTarget { logical_id: 1, apic_id: 2, trampoline_physical: 0x8000, stack_top: 0x204000, pml4_physical: 0x1000 };
    target.validate() == Ok(target)
        && ApStartupTarget { apic_id: 256, ..target }.validate() == Err(SmpError::UnsupportedApicId)
        && ApStartupTarget { trampoline_physical: 0, ..target }.validate() == Err(SmpError::InvalidTarget)
        && ApStartupTarget { stack_top: 0x204003, ..target }.validate() == Err(SmpError::InvalidTarget)
}

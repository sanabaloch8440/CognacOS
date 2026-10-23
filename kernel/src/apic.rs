//! xAPIC/I/O APIC register primitives. Interrupt delivery remains disabled until
//! IDT vectors, timer calibration, and all required routes are installed.
use core::sync::atomic::{compiler_fence, Ordering};
const IA32_APIC_BASE: u32 = 0x1b;
const APIC_ENABLE: u64 = 1 << 11;
const X2APIC_ENABLE: u64 = 1 << 10;
const APIC_ID: usize = 0x020;
const APIC_EOI: usize = 0x0b0;
const APIC_SVR: usize = 0x0f0;
const APIC_LVT_TIMER: usize = 0x320;
const APIC_LVT_ERROR: usize = 0x370;
const APIC_LVT_TIMER: usize = 0x320;
pub const TLB_SHOOTDOWN_VECTOR: u8 = 0xF1;
const APIC_SPURIOUS_VECTOR: u32 = 0x100 | 0xff;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApicError { Unsupported, X2ApicActive, InvalidBase, InvalidVector, InvalidGsi, Timeout }
#[inline] unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32; let high: u32;
    unsafe { core::arch::asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nomem, nostack)); }
    ((high as u64) << 32) | low as u64
}
#[inline] unsafe fn wrmsr(msr: u32, value: u64) {
    unsafe { core::arch::asm!("wrmsr", in("ecx") msr, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nomem, nostack)); }
}
/// Enable xAPIC mode and its spurious vector. Requires CPUID APIC support and an
/// identity-mapped, uncached MMIO mapping for the physical APIC page.
pub unsafe fn initialize_local_apic(mmio_base: u64) -> Result<u32, ApicError> {
    if mmio_base == 0 || mmio_base & 0xfff != 0 || mmio_base >= 0x1_0000_0000 { return Err(ApicError::InvalidBase); }
    let max = unsafe { core::arch::x86_64::__cpuid(1) };
    if max.edx & (1 << 9) == 0 { return Err(ApicError::Unsupported); }
    let current = unsafe { rdmsr(IA32_APIC_BASE) };
    if current & X2APIC_ENABLE != 0 { return Err(ApicError::X2ApicActive); }
    unsafe { wrmsr(IA32_APIC_BASE, (current & !0x000f_ffff_ffff_f000) | mmio_base | APIC_ENABLE); }
    let id = unsafe { read_local(mmio_base, APIC_ID) } >> 24;
    // Keep asynchronous local sources masked until the kernel has installed
    // the IPI gate and masked legacy/I/O APIC routes.
    unsafe { write_local(mmio_base, APIC_LVT_ERROR, 1 << 16); }
    unsafe { write_local(mmio_base, APIC_LVT_TIMER, 1 << 16); }
    unsafe { write_local(mmio_base, APIC_SVR, APIC_SPURIOUS_VECTOR); }
    Ok(id)
}
#[inline] unsafe fn read_local(base: u64, offset: usize) -> u32 {
    compiler_fence(Ordering::SeqCst);
    let value = unsafe { core::ptr::read_volatile((base + offset as u64) as *const u32) };
    compiler_fence(Ordering::SeqCst); value
}
#[inline] unsafe fn write_local(base: u64, offset: usize, value: u32) {
    compiler_fence(Ordering::SeqCst);
    unsafe { core::ptr::write_volatile((base + offset as u64) as *mut u32, value); }
    compiler_fence(Ordering::SeqCst);
}
pub unsafe fn local_apic_eoi(mmio_base: u64) { unsafe { write_local(mmio_base, APIC_EOI, 0); } }
/// Configure one I/O APIC redirection entry masked. Does not unmask the IRQ:
/// callers must install a matching IDT handler and confirm the GSI belongs here.
pub unsafe fn configure_ioapic_masked(io_base: u64, gsi_base: u32, gsi: u32, vector: u8, destination_apic_id: u8) -> Result<(), ApicError> {
    if io_base == 0 || io_base & 0xfff != 0 || vector < 0x20 || vector == 0xff { return Err(ApicError::InvalidVector); }
    if gsi < gsi_base || gsi - gsi_base >= 120 { return Err(ApicError::InvalidGsi); }
    unsafe fn select(base: u64, reg: u8) {
        unsafe { core::ptr::write_volatile(base as *mut u32, reg as u32); }
    }
    unsafe fn read(base: u64) -> u32 { unsafe { core::ptr::read_volatile((base + 0x10) as *const u32) } }
    unsafe fn write(base: u64, value: u32) { unsafe { core::ptr::write_volatile((base + 0x10) as *mut u32, value); } }
    let index = (gsi - gsi_base) as u8;
    let low_reg = 0x10u8.wrapping_add(index.wrapping_mul(2));
    unsafe { select(io_base, 0x01); }
    let version = unsafe { read(io_base) };
    let max_pin = (version >> 16) & 0xff;
    if index as u32 > max_pin { return Err(ApicError::InvalidGsi); }
    unsafe { select(io_base, low_reg); write(io_base, (1 << 16) | vector as u32); }
    unsafe { select(io_base, low_reg.wrapping_add(1)); write(io_base, (destination_apic_id as u32) << 24); }
    Ok(())
}
/// Timer remains masked; calibration and scheduler integration are separate steps.
pub unsafe fn mask_local_timer(mmio_base: u64) { unsafe { write_local(mmio_base, APIC_LVT_TIMER, 1 << 16); } }

/// Mask every redirection entry in one I/O APIC before enabling IF on any CPU.
/// The caller must first map the register page as uncached device memory.
pub unsafe fn mask_all_ioapic(io_base: u64) -> Result<(), ApicError> {
    if io_base == 0 || io_base & 0xfff != 0 { return Err(ApicError::InvalidBase); }
    unsafe fn select(base: u64, reg: u8) { unsafe { core::ptr::write_volatile(base as *mut u32, reg as u32); } }
    unsafe fn read(base: u64) -> u32 { unsafe { core::ptr::read_volatile((base + 0x10) as *const u32) } }
    unsafe fn write(base: u64, value: u32) { unsafe { core::ptr::write_volatile((base + 0x10) as *mut u32, value); } }
    unsafe { select(io_base, 0x01); }
    let max_pin = (unsafe { read(io_base) } >> 16) & 0xff;
    if max_pin > 119 { return Err(ApicError::InvalidGsi); }
    for pin in 0..=max_pin {
        let low = 0x10u8.wrapping_add((pin as u8).wrapping_mul(2));
        unsafe { select(io_base, low); let value = read(io_base); write(io_base, value | (1 << 16)); }
    }
    Ok(())
}


const APIC_ICR_LOW: usize = 0x300;
const APIC_ICR_HIGH: usize = 0x310;
const APIC_ICR_DELIVERY_PENDING: u32 = 1 << 12;
const APIC_ICR_INIT: u32 = 0b101 << 8;
const APIC_ICR_STARTUP: u32 = 0b110 << 8;

/// Wait for the local APIC's previous IPI transaction to finish. The bounded
/// loop avoids hanging the boot CPU forever if the controller is unavailable.
unsafe fn wait_icr_idle(base: u64) -> Result<(), ApicError> {
    for _ in 0..1_000_000usize {
        if unsafe { read_local(base, APIC_ICR_LOW) } & APIC_ICR_DELIVERY_PENDING == 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(ApicError::Timeout)
}

/// Send a fixed-vector IPI using legacy xAPIC destination IDs (8-bit).
/// Requires a mapped, uncached local APIC MMIO page and a valid IDT vector on
/// the target. This primitive does not configure the target's IDT or scheduler.
pub unsafe fn send_fixed_ipi(base: u64, destination_apic_id: u8, vector: u8) -> Result<(), ApicError> {
    if base == 0 || base & 0xfff != 0 || vector < 0x20 || vector == 0xff {
        return Err(ApicError::InvalidVector);
    }
    unsafe { wait_icr_idle(base)?; }
    unsafe { write_local(base, APIC_ICR_HIGH, (destination_apic_id as u32) << 24); }
    unsafe { write_local(base, APIC_ICR_LOW, vector as u32); }
    unsafe { wait_icr_idle(base) }
}

/// Issue an INIT assert IPI. Startup sequencing and required hardware delays
/// remain the responsibility of the SMP coordinator.
pub unsafe fn send_init_ipi(base: u64, destination_apic_id: u8) -> Result<(), ApicError> {
    if base == 0 || base & 0xfff != 0 { return Err(ApicError::InvalidBase); }
    unsafe { wait_icr_idle(base)?; }
    unsafe { write_local(base, APIC_ICR_HIGH, (destination_apic_id as u32) << 24); }
    unsafe { write_local(base, APIC_ICR_LOW, APIC_ICR_INIT | (1 << 14) | (1 << 15)); }
    unsafe { wait_icr_idle(base) }
}

/// Issue the architecturally specified INIT deassert broadcast.
/// INIT deassert is broadcast to all processors; the destination field is
/// ignored by hardware for this delivery mode. Keep the argument for API
/// compatibility and reject no APIC IDs here; the subsequent SIPI is targeted.
pub unsafe fn send_init_deassert_ipi(base: u64, _destination_apic_id: u8) -> Result<(), ApicError> {
    const APIC_ICR_DESTINATION_SHORTHAND_ALL_EXCLUDING_SELF: u32 = 0b11 << 18;
    if base == 0 || base & 0xfff != 0 { return Err(ApicError::InvalidBase); }
    unsafe { wait_icr_idle(base)?; }
    // INIT delivery mode, level-triggered, deassert level, broadcast excluding BSP.
    unsafe { write_local(base, APIC_ICR_HIGH, 0); }
    unsafe {
        write_local(
            base,
            APIC_ICR_LOW,
            APIC_ICR_INIT
                | (1 << 15)
                | APIC_ICR_DESTINATION_SHORTHAND_ALL_EXCLUDING_SELF,
        );
    }
    unsafe { wait_icr_idle(base) }
}

/// Send one Startup IPI to a 4 KiB-aligned trampoline below 1 MiB.
/// This is only a low-level primitive; it does not by itself make AP startup
/// safe. A caller must prepare the trampoline, stack, CR3, GDT, IDT, and state.
pub unsafe fn send_startup_ipi(base: u64, destination_apic_id: u8, trampoline_physical: u64) -> Result<(), ApicError> {
    if base == 0 || base & 0xfff != 0 { return Err(ApicError::InvalidBase); }
    if trampoline_physical == 0 || trampoline_physical >= 0x10_0000 || trampoline_physical & 0xfff != 0 {
        return Err(ApicError::InvalidBase);
    }
    unsafe { wait_icr_idle(base)?; }
    unsafe { write_local(base, APIC_ICR_HIGH, (destination_apic_id as u32) << 24); }
    unsafe { write_local(base, APIC_ICR_LOW, APIC_ICR_STARTUP | ((trampoline_physical >> 12) as u32)); }
    unsafe { wait_icr_idle(base) }
}

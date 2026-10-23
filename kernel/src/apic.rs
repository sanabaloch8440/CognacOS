//! xAPIC/I/O APIC register primitives. Interrupt delivery remains disabled until
//! IDT vectors, timer calibration, and all required routes are installed.
use core::sync::atomic::{compiler_fence, AtomicU64, Ordering};
const IA32_APIC_BASE: u32 = 0x1b;
const APIC_ENABLE: u64 = 1 << 11;
const X2APIC_ENABLE: u64 = 1 << 10;
const APIC_ID: usize = 0x020;
const APIC_TPR: usize = 0x080;
const APIC_TIMER_INITIAL_COUNT: usize = 0x380;
const APIC_TIMER_CURRENT_COUNT: usize = 0x390;
const APIC_TIMER_DIVIDE: usize = 0x3e0;
pub const LOCAL_TIMER_VECTOR: u8 = 0xE0;
static LOCAL_APIC_BASE: AtomicU64 = AtomicU64::new(0);
static LAST_TSC: AtomicU64 = AtomicU64::new(0);
static LOCAL_TIMER_ACTIVE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
const APIC_EOI: usize = 0x0b0;
const APIC_SVR: usize = 0x0f0;
const APIC_LVT_TIMER: usize = 0x320;
const APIC_LVT_ERROR: usize = 0x370;
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
    LOCAL_APIC_BASE.store(mmio_base, Ordering::Release);
    unsafe { write_local(mmio_base, APIC_TPR, 0); }
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


// Hardware priority and timer controls. The timer remains masked until the
// caller supplies a calibrated initial count and explicitly starts it.
#[inline]
pub fn local_timer_active() -> bool { LOCAL_TIMER_ACTIVE.load(Ordering::Acquire) }

pub fn local_apic_base() -> Option<u64> {
    let base = LOCAL_APIC_BASE.load(Ordering::Acquire);
    (base != 0).then_some(base)
}

/// Set the xAPIC task-priority threshold from an NT IRQL value (0..=15).
/// Hardware priority classes are four-bit fields in TPR; this is a conservative
/// mapping and does not replace the full Windows HAL IRQL policy.
pub unsafe fn set_task_priority(irql: u8) -> Result<(), ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    if irql > 15 { return Err(ApicError::InvalidVector); }
    unsafe { write_local(base, APIC_TPR, (irql as u32) << 4); }
    Ok(())
}
pub unsafe fn task_priority() -> Result<u8, ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    Ok(((unsafe { read_local(base, APIC_TPR) } >> 4) & 0x0f) as u8)
}

/// Read the x86 TSC as the kernel's monotonic deadline domain. Timeout values
/// must be converted to TSC ticks by a calibrated platform clock before use.
pub fn monotonic_ticks() -> u64 {
    let low: u32;
    let high: u32;
    unsafe { core::arch::asm!("lfence", "rdtsc", out("eax") low, out("edx") high, options(nomem, nostack)); }
    let now = ((high as u64) << 32) | low as u64;
    let previous = LAST_TSC.fetch_max(now, Ordering::AcqRel);
    now.max(previous)
}

/// Program a local APIC timer with a previously calibrated count. Valid divider
/// encodings are 0,1,2,3,8,9,10,11. This function intentionally does not enable
/// interrupts or guess a frequency; the caller owns calibration and startup.
pub unsafe fn start_local_timer(initial_count: u32, divide: u8, periodic: bool) -> Result<(), ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    if initial_count == 0 || !matches!(divide, 0 | 1 | 2 | 3 | 8 | 9 | 10 | 11) {
        return Err(ApicError::InvalidVector);
    }
    unsafe {
        write_local(base, APIC_LVT_TIMER, 1 << 16);
        write_local(base, APIC_TIMER_DIVIDE, divide as u32);
        write_local(base, APIC_TIMER_INITIAL_COUNT, initial_count);
        write_local(base, APIC_LVT_TIMER, LOCAL_TIMER_VECTOR as u32 | if periodic { 1 << 17 } else { 0 });
    }
    LOCAL_TIMER_ACTIVE.store(true, Ordering::Release);
    Ok(())
}
pub unsafe fn stop_local_timer() {
    if let Some(base) = local_apic_base() {
        unsafe { write_local(base, APIC_LVT_TIMER, (1 << 16) | LOCAL_TIMER_VECTOR as u32); }
        unsafe { write_local(base, APIC_TIMER_INITIAL_COUNT, 0); }
    }
    LOCAL_TIMER_ACTIVE.store(false, Ordering::Release);
}

/// Interrupt-gate callback: update the clock domain, expire due dispatcher
/// waits, ready their threads, then acknowledge the local APIC interrupt.
pub fn timer_interrupt() {
    let now = monotonic_ticks();
    let mut expired = [crate::dispatcher::ExpiredWait { thread: 0, object: 0 }; 64];
    let count = crate::dispatcher::expire_detailed(now, &mut expired);
    for wait in expired[..count].iter().copied() {
        let status = if crate::dispatcher::is_delay_object(wait.object) { 0 } else { 0x0000_0102 };
        let _ = crate::thread::complete_wait(wait.thread, status);
    }
    if let Some(base) = local_apic_base() {
        unsafe { local_apic_eoi(base); }
    }
}

/// Best-effort architectural TSC frequency discovery (CPUID 0x15, then 0x16).
/// Returns None when firmware/hardware exposes no usable frequency.
pub fn tsc_frequency_hz() -> Option<u64> {
    let max = unsafe { core::arch::x86_64::__cpuid(0) }.eax;
    if max >= 0x15 {
        let leaf = unsafe { core::arch::x86_64::__cpuid(0x15) };
        if leaf.eax != 0 && leaf.ebx != 0 && leaf.ecx != 0 {
            let hz = (leaf.ecx as u128).checked_mul(leaf.ebx as u128)? / leaf.eax as u128;
            if hz > 0 && hz <= u64::MAX as u128 { return Some(hz as u64); }
        }
    }
    if max >= 0x16 {
        let mhz = unsafe { core::arch::x86_64::__cpuid(0x16) }.eax as u64;
        if mhz != 0 { return mhz.checked_mul(1_000_000); }
    }
    None
}

/// Convert a negative NT relative timeout (100 ns units) to an absolute TSC
/// deadline. Positive absolute wall-clock deadlines need a system-time source
/// and are intentionally not interpreted as relative durations.
pub fn relative_timeout_deadline(interval_100ns: i64) -> Option<u64> {
    if interval_100ns >= 0 { return None; }
    let hz = tsc_frequency_hz()?;
    let units = interval_100ns.unsigned_abs() as u128;
    let ticks = units.checked_mul(hz as u128)? / 10_000_000u128;
    let ticks = u64::try_from(ticks).ok()?;
    monotonic_ticks().checked_add(ticks)
}

/// Calibrate the local APIC decrementer against the architectural TSC for a
/// nominal 1 ms periodic tick, then start it. Requires an identity-mapped,
/// uncached local-APIC MMIO page and a usable TSC frequency; it does not enable IF.
pub unsafe fn calibrate_and_start_local_timer() -> Result<u32, ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    let tsc_hz = tsc_frequency_hz().ok_or(ApicError::Unsupported)?;
    let sample_ticks = (tsc_hz / 100).max(1); // 10 ms calibration window.
    unsafe {
        write_local(base, APIC_LVT_TIMER, (1 << 16) | LOCAL_TIMER_VECTOR as u32);
        write_local(base, APIC_TIMER_DIVIDE, 0x3); // divide by 16
        write_local(base, APIC_TIMER_INITIAL_COUNT, u32::MAX);
    }
    let start = monotonic_ticks();
    let target = start.checked_add(sample_ticks).ok_or(ApicError::Timeout)?;
    let mut spins = 0usize;
    while monotonic_ticks() < target {
        core::hint::spin_loop();
        spins += 1;
        if spins == 100_000_000 { unsafe { mask_local_timer(base); } return Err(ApicError::Timeout); }
    }
    let remaining = unsafe { read_local(base, APIC_TIMER_CURRENT_COUNT) };
    let elapsed = u32::MAX.saturating_sub(remaining);
    let per_ms = elapsed / 10;
    if per_ms == 0 { unsafe { mask_local_timer(base); } return Err(ApicError::Unsupported); }
    unsafe {
        write_local(base, APIC_TIMER_INITIAL_COUNT, per_ms);
        write_local(base, APIC_LVT_TIMER, LOCAL_TIMER_VECTOR as u32 | (1 << 17));
    }
    LOCAL_TIMER_ACTIVE.store(true, Ordering::Release);
    Ok(per_ms)
}

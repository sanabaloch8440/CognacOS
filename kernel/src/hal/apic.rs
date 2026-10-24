//! xAPIC/I/O APIC register primitives. Interrupt delivery remains disabled until
//! IDT vectors, timer calibration, and all required routes are installed.
use core::sync::atomic::{compiler_fence, AtomicI64, AtomicU64, Ordering};
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
static TSC_CPU_OFFSETS: [AtomicI64; crate::hal::MAX_CPUS] = [const { AtomicI64::new(0) }; crate::hal::MAX_CPUS];
static TSC_CPU_SYNCED: [core::sync::atomic::AtomicBool; crate::hal::MAX_CPUS] = [const { core::sync::atomic::AtomicBool::new(false) }; crate::hal::MAX_CPUS];
static CALIBRATED_TSC_HZ: AtomicU64 = AtomicU64::new(0);
static LOCAL_TIMER_ACTIVE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static TIMER_OWNER_APIC_ID: AtomicU64 = AtomicU64::new(u64::MAX);
static CALIBRATED_TIMER_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
const APIC_EOI: usize = 0x0b0;
const APIC_SVR: usize = 0x0f0;
const APIC_LVT_TIMER: usize = 0x320;
const APIC_LVT_THERMAL: usize = 0x330;
const APIC_LVT_PERFORMANCE: usize = 0x340;
const APIC_LVT_LINT0: usize = 0x350;
const APIC_LVT_LINT1: usize = 0x360;
const APIC_LVT_ERROR: usize = 0x370;
pub const TLB_SHOOTDOWN_VECTOR: u8 = 0xF1;
pub const HAL_IPI_VECTOR: u8 = 0xF0;
pub const KE_BROADCAST_VECTOR: u8 = 0xEF;
pub const TSC_SYNC_VECTOR: u8 = 0xF2;
static TSC_SYNC_SAMPLES: [AtomicU64; crate::hal::MAX_CPUS] = [const { AtomicU64::new(0) }; crate::hal::MAX_CPUS];
static TSC_SYNC_SEQUENCE: [AtomicU64; crate::hal::MAX_CPUS] = [const { AtomicU64::new(0) }; crate::hal::MAX_CPUS];
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
    unsafe { write_local(mmio_base, APIC_LVT_THERMAL, 1 << 16); }
    unsafe { write_local(mmio_base, APIC_LVT_PERFORMANCE, 1 << 16); }
    unsafe { write_local(mmio_base, APIC_LVT_LINT0, 1 << 16); }
    unsafe { write_local(mmio_base, APIC_LVT_LINT1, 1 << 16); }
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
pub fn local_timer_active() -> bool {
    let owner=TIMER_OWNER_APIC_ID.load(Ordering::Acquire);
    let current=((unsafe{core::arch::x86_64::__cpuid(1)}.ebx>>24)&0xff) as u64;
    owner==current&&LOCAL_TIMER_ACTIVE.load(Ordering::Acquire)
}

pub fn local_apic_base() -> Option<u64> {
    let base = LOCAL_APIC_BASE.load(Ordering::Acquire);
    (base != 0).then_some(base)
}

/// Map NT IRQL onto the four-bit xAPIC task-priority class. NT IRQL has
/// more levels than the APIC TPR, so adjacent high DIRQLs share a hardware
/// class; software still tracks the exact IRQL in the executive.
#[inline]
pub const fn irql_to_apic_class(irql: u8) -> u8 {
    if irql <= 1 { 0 } else if irql == 2 { 1 } else {
        let class = irql.saturating_sub(1);
        if class > 15 { 15 } else { class }
    }
}
pub unsafe fn set_task_priority(irql: u8) -> Result<(), ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    unsafe { write_local(base, APIC_TPR, (irql_to_apic_class(irql) as u32) << 4); }
    Ok(())
}
pub unsafe fn task_priority() -> Result<u8, ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    Ok(((unsafe { read_local(base, APIC_TPR) } >> 4) & 0x0f) as u8)
}

/// Enable or disable an already configured I/O-APIC GSI. Unknown/unconfigured
/// entries are never unmasked: an unmask request requires a valid vector in
/// the existing redirection entry. Caller must ensure the matching IDT gate
/// and ISR are installed before enabling the source.
pub unsafe fn set_ioapic_irq_enabled(
    io_base: u64, gsi_base: u32, gsi: u32, enabled: bool,
) -> Result<(), ApicError> {
    if io_base == 0 || io_base & 0xfff != 0 { return Err(ApicError::InvalidBase); }
    if gsi < gsi_base || gsi - gsi_base >= 120 { return Err(ApicError::InvalidGsi); }
    unsafe fn select(base: u64, reg: u8) {
        unsafe { core::ptr::write_volatile(base as *mut u32, reg as u32); }
    }
    unsafe fn read(base: u64) -> u32 {
        unsafe { core::ptr::read_volatile((base + 0x10) as *const u32) }
    }
    unsafe fn write(base: u64, value: u32) {
        unsafe { core::ptr::write_volatile((base + 0x10) as *mut u32, value); }
    }
    let index = (gsi - gsi_base) as u8;
    unsafe { select(io_base, 0x01); }
    let max_pin = (unsafe { read(io_base) } >> 16) & 0xff;
    if index as u32 > max_pin { return Err(ApicError::InvalidGsi); }
    let low_reg = 0x10u8.wrapping_add(index.wrapping_mul(2));
    unsafe { select(io_base, low_reg); }
    let current = unsafe { read(io_base) };
    let vector = (current & 0xff) as u8;
    if enabled && (!(VECTOR_FIRST..=VECTOR_LAST).contains(&vector)
        || !VECTOR_ALLOCATED[vector as usize].load(Ordering::Acquire)
        || VECTOR_HANDLERS[vector as usize].load(Ordering::Acquire) == 0) {
        return Err(ApicError::InvalidVector);
    }
    unsafe { write(io_base, if enabled { current & !(1 << 16) } else { current | (1 << 16) }); }
    Ok(())
}

/// Read the x86 TSC as the kernel's monotonic deadline domain. Timeout values
/// must be converted to TSC ticks by a calibrated platform clock before use.
/// Read the current CPU's TSC after applying a measured per-CPU epoch offset.
/// A global atomic high-water mark makes the returned domain nondecreasing even
/// before all APs have completed synchronization; it cannot replace calibration.
pub fn monotonic_ticks() -> u64 {
    let low: u32;
    let high: u32;
    unsafe { core::arch::asm!("lfence", "rdtsc", out("eax") low, out("edx") high, options(nomem, nostack)); }
    let raw = ((high as u64) << 32) | low as u64;
    let slot = crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Relaxed) as usize)
        .filter(|id| *id < crate::hal::MAX_CPUS).unwrap_or(0);
    let offset = TSC_CPU_OFFSETS[slot].load(Ordering::Acquire);
    let adjusted = if offset >= 0 {
        raw.saturating_add(offset as u64)
    } else {
        raw.saturating_sub(offset.unsigned_abs())
    };
    let previous = LAST_TSC.fetch_max(adjusted, Ordering::AcqRel);
    adjusted.max(previous)
}

/// Capture a raw TSC sample on the CPU that received TSC_SYNC_VECTOR.
/// The dedicated IDT stub acknowledges the IPI after this publication.
pub fn capture_tsc_sync_sample() {
    let Some(cpu) = crate::hal::current_cpu_local() else { return; };
    let slot = cpu.logical_id.load(Ordering::Acquire) as usize;
    if slot == 0 || slot >= crate::hal::MAX_CPUS { return; }
    let low: u32;
    let high: u32;
    unsafe { core::arch::asm!("lfence", "rdtsc", out("eax") low, out("edx") high, options(nomem, nostack)); }
    TSC_SYNC_SAMPLES[slot].store(((high as u64) << 32) | low as u64, Ordering::Release);
    TSC_SYNC_SEQUENCE[slot].fetch_add(1, Ordering::AcqRel);
}

/// Calibrate one AP against the BSP using repeated bracketed fixed-vector IPIs.
/// The narrowest successful sample is used to minimise interrupt-delivery bias.
pub unsafe fn synchronize_tsc_with_cpu(logical_cpu: usize, destination_apic_id: u8) -> Result<i64, ApicError> {
    if logical_cpu == 0 || logical_cpu >= crate::hal::MAX_CPUS { return Err(ApicError::InvalidBase); }
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    let mut best_width = u64::MAX;
    let mut best_before = 0u64;
    let mut best_after = 0u64;
    let mut best_remote = 0u64;
    for _ in 0..8 {
        let sequence = TSC_SYNC_SEQUENCE[logical_cpu].load(Ordering::Acquire);
        let before = read_tsc_raw();
        unsafe { send_fixed_ipi(base, destination_apic_id, TSC_SYNC_VECTOR)?; }
        let mut spins = 0usize;
        while TSC_SYNC_SEQUENCE[logical_cpu].load(Ordering::Acquire) == sequence && spins < 5_000_000 {
            core::hint::spin_loop();
            spins += 1;
        }
        let after = read_tsc_raw();
        if TSC_SYNC_SEQUENCE[logical_cpu].load(Ordering::Acquire) == sequence || after < before { continue; }
        let width = after - before;
        if width < best_width {
            best_width = width;
            best_before = before;
            best_after = after;
            best_remote = TSC_SYNC_SAMPLES[logical_cpu].load(Ordering::Acquire);
        }
    }
    if best_width == u64::MAX { return Err(ApicError::Timeout); }
    record_tsc_sync_sample(logical_cpu, best_before, best_remote, best_after)
}

#[inline]
fn read_tsc_raw() -> u64 {
    let low: u32;
    let high: u32;
    unsafe { core::arch::asm!("lfence", "rdtsc", out("eax") low, out("edx") high, options(nomem, nostack)); }
    ((high as u64) << 32) | low as u64
}

/// Record an AP's TSC offset from a BSP-side bracketed sample. local_before and
/// local_after are BSP TSC readings surrounding remote_sample (captured on the
/// target CPU); midpoint minimises IPI latency bias. Repeat and retain the sample
/// with the narrowest bracket in the SMP startup coordinator for best accuracy.
pub fn record_tsc_sync_sample(logical_cpu: usize, local_before: u64, remote_sample: u64, local_after: u64) -> Result<i64, ApicError> {
    if logical_cpu == 0 || logical_cpu >= crate::hal::MAX_CPUS || local_after < local_before || local_after - local_before > 10_000_000 {
        return Err(ApicError::InvalidBase);
    }
    let midpoint = local_before + (local_after - local_before) / 2;
    let delta = midpoint as i128 - remote_sample as i128;
    let offset = i64::try_from(delta).map_err(|_| ApicError::InvalidBase)?;
    TSC_CPU_OFFSETS[logical_cpu].store(offset, Ordering::Release);
    TSC_CPU_SYNCED[logical_cpu].store(true, Ordering::Release);
    Ok(offset)
}

/// Check whether every online CPU has an epoch sample and report the worst
/// measured offset magnitude. The monotonic high-water clamp is not a substitute
/// for this check: large skew can distort elapsed-time measurements.
pub fn tsc_cross_core_status() -> (bool, u64) {
    let mut synchronized = true;
    let mut worst = 0u64;
    for cpu in 0..crate::hal::MAX_CPUS {
        let Some(local) = crate::hal::cpu_local(cpu).ok() else { continue; };
        if local.state.load(Ordering::Acquire) != crate::hal::CPU_ONLINE { continue; }
        if cpu != 0 && !TSC_CPU_SYNCED[cpu].load(Ordering::Acquire) {
            synchronized = false;
            continue;
        }
        worst = worst.max(TSC_CPU_OFFSETS[cpu].load(Ordering::Acquire).unsigned_abs());
    }
    (synchronized, worst)
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

/// Publish the monotonic clock event through the HAL callback contract, then
/// acknowledge the local APIC interrupt. No compile-time NT dependency remains.
pub fn timer_interrupt() {
    let now = monotonic_ticks();
    let _ = crate::hal::publish_clock_interrupt(now);
    if let Some(base) = local_apic_base() {
        unsafe { local_apic_eoi(base); }
    }
}

/// Best-effort architectural TSC frequency discovery (CPUID 0x15, then 0x16).
/// Returns None when firmware/hardware exposes no usable frequency.
pub fn tsc_frequency_hz() -> Option<u64> {
    let calibrated = CALIBRATED_TSC_HZ.load(Ordering::Acquire);
    if calibrated != 0 { return Some(calibrated); }
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
pub unsafe fn calibrate_local_timer() -> Result<u32, ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    const PIT_COMMAND: u16 = 0x43;
    const PIT_CHANNEL2: u16 = 0x42;
    const PIT_GATE_SPEAKER: u16 = 0x61;
    const PIT_RELOAD_10MS: u16 = 11_932; // 1,193,182 Hz / 100 ~= 10 ms.
    let old_speaker = unsafe { crate::hal::read_port_u8(PIT_GATE_SPEAKER) };
    unsafe {
        write_local(base, APIC_LVT_TIMER, (1 << 16) | LOCAL_TIMER_VECTOR as u32);
        write_local(base, APIC_TIMER_DIVIDE, 0x3); // divide by 16
        write_local(base, APIC_TIMER_INITIAL_COUNT, u32::MAX);
        // PIT channel 2, low/high byte, mode 0, binary counter. Poll OUT2;
        // no PIC interrupt or calibrated TSC frequency is required.
        crate::hal::write_port_u8(PIT_GATE_SPEAKER, old_speaker & !0x03);
        crate::hal::write_port_u8(PIT_COMMAND, 0xb0);
        crate::hal::write_port_u8(PIT_CHANNEL2, PIT_RELOAD_10MS as u8);
        crate::hal::write_port_u8(PIT_CHANNEL2, (PIT_RELOAD_10MS >> 8) as u8);
    }
    TIMER_OWNER_APIC_ID.store(((unsafe{core::arch::x86_64::__cpuid(1)}.ebx>>24)&0xff) as u64, Ordering::Release);
    let tsc_start = monotonic_ticks();
    unsafe { crate::hal::write_port_u8(PIT_GATE_SPEAKER, (old_speaker & !0x02) | 0x01); }
    let mut spins = 0usize;
    let mut elapsed = false;
    while spins < 100_000_000 {
        if unsafe { crate::hal::read_port_u8(PIT_GATE_SPEAKER) } & 0x20 != 0 {
            elapsed = true;
            break;
        }
        core::hint::spin_loop();
        spins += 1;
    }
    let tsc_end = monotonic_ticks();
    unsafe { crate::hal::write_port_u8(PIT_GATE_SPEAKER, old_speaker); }
    if !elapsed {
        unsafe { mask_local_timer(base); }
        return Err(ApicError::Timeout);
    }
    if tsc_end > tsc_start {
        if let Some(hz) = (tsc_end - tsc_start).checked_mul(100) {
            CALIBRATED_TSC_HZ.store(hz, Ordering::Release);
        }
    }
    let remaining = unsafe { read_local(base, APIC_TIMER_CURRENT_COUNT) };
    let elapsed_apic = u32::MAX.saturating_sub(remaining);
    let per_ms = elapsed_apic / 10;
    if per_ms == 0 {
        unsafe { mask_local_timer(base); }
        return Err(ApicError::Unsupported);
    }
    CALIBRATED_TIMER_COUNT.store(per_ms, Ordering::Release);
    LOCAL_TIMER_ACTIVE.store(false, Ordering::Release);
    // Keep the LVT masked until AP startup and kernel self-tests have completed.
    unsafe { mask_local_timer(base); }
    Ok(per_ms)
}

/// Start the calibrated BSP local timer only after all interrupt-routing checks pass.
pub unsafe fn start_calibrated_local_timer() -> Result<(), ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    let count = CALIBRATED_TIMER_COUNT.load(Ordering::Acquire);
    if count == 0 { return Err(ApicError::Unsupported); }
    unsafe {
        write_local(base, APIC_TIMER_DIVIDE, 0x3);
        write_local(base, APIC_TIMER_INITIAL_COUNT, count);
        write_local(base, APIC_LVT_TIMER, LOCAL_TIMER_VECTOR as u32 | (1 << 17));
    }
    LOCAL_TIMER_ACTIVE.store(true, Ordering::Release);
    Ok(())
}


/// Interrupt handler ABI used by the internal vector dispatcher. The frame is
/// architecture-owned; handlers must not retain it after returning.
pub type InterruptHandler = unsafe extern "win64" fn(vector: u8, frame: *mut core::ffi::c_void, context: *mut core::ffi::c_void) -> bool;
const VECTOR_FIRST: u8 = 0x20;
const VECTOR_LAST: u8 = 0xdf;
static VECTOR_ALLOCATED: [core::sync::atomic::AtomicBool; 256] =
    [const { core::sync::atomic::AtomicBool::new(false) }; 256];
static VECTOR_HANDLERS: [core::sync::atomic::AtomicUsize; 256] =
    [const { core::sync::atomic::AtomicUsize::new(0) }; 256];
static VECTOR_CONTEXTS: [core::sync::atomic::AtomicUsize; 256] =
    [const { core::sync::atomic::AtomicUsize::new(0) }; 256];
static VECTOR_REGISTRATION_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

/// Reserve a device interrupt vector. Architectural exceptions, timer, IPI,
/// and spurious/error vectors are deliberately outside the allocatable range.
pub fn allocate_interrupt_vector() -> Result<u8, ApicError> {
    for vector in VECTOR_FIRST..=VECTOR_LAST {
        if VECTOR_ALLOCATED[vector as usize].compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            return Ok(vector);
        }
    }
    Err(ApicError::InvalidVector)
}

/// Release an unused vector. Call only after the source is masked and its IDT
/// gate is detached; the caller owns synchronization with in-flight interrupts.
pub fn release_interrupt_vector(vector: u8) -> Result<(), ApicError> {
    if !(VECTOR_FIRST..=VECTOR_LAST).contains(&vector) { return Err(ApicError::InvalidVector); }
    let _guard = VECTOR_REGISTRATION_LOCK.lock();
    VECTOR_HANDLERS[vector as usize].store(0, Ordering::Release);
    VECTOR_CONTEXTS[vector as usize].store(0, Ordering::Release);
    VECTOR_ALLOCATED[vector as usize].store(false, Ordering::Release);
    Ok(())
}

/// Register a handler for a vector already allocated by allocate_interrupt_vector.
/// Install the corresponding architecture IDT gate before unmasking its source.
pub fn register_interrupt_handler(vector: u8, handler: InterruptHandler, context: *mut core::ffi::c_void) -> Result<(), ApicError> {
    if !(VECTOR_FIRST..=VECTOR_LAST).contains(&vector) {
        return Err(ApicError::InvalidVector);
    }
    let _guard = VECTOR_REGISTRATION_LOCK.lock();
    if !VECTOR_ALLOCATED[vector as usize].load(Ordering::Acquire)
        || VECTOR_HANDLERS[vector as usize].load(Ordering::Acquire) != 0 {
        return Err(ApicError::InvalidVector);
    }
    VECTOR_CONTEXTS[vector as usize].store(context as usize, Ordering::Relaxed);
    VECTOR_HANDLERS[vector as usize].store(handler as usize, Ordering::Release);
    Ok(())
}

/// Dispatch an external interrupt from the IDT common entry stub. Exactly one
/// local-APIC EOI is issued after the callback, even when it reports unhandled.
/// The entry stub must not also EOI this vector (avoid double acknowledgement).
pub unsafe fn dispatch_external_interrupt(vector: u8, frame: *mut core::ffi::c_void) -> bool {
    let raw = VECTOR_HANDLERS[vector as usize].load(Ordering::Acquire);
    let handled = if raw == 0 { false } else {
        let callback: InterruptHandler = unsafe { core::mem::transmute(raw) };
        let context = VECTOR_CONTEXTS[vector as usize].load(Ordering::Acquire) as *mut core::ffi::c_void;
        unsafe { callback(vector, frame, context) }
    };
    if let Some(base) = local_apic_base() {
        unsafe { local_apic_eoi(base); }
    }
    handled
}

/// Acknowledge a local-APIC interrupt when the handler itself owns EOI. Use
/// this for timer/IPI handlers that do not pass through dispatch_external_interrupt.
pub unsafe fn end_of_interrupt() -> Result<(), ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    unsafe { local_apic_eoi(base); }
    Ok(())
}

/// Program a full I/O-APIC redirection entry while keeping it masked. Supports
/// polarity/trigger flags from ACPI Interrupt Source Overrides. The IDT handler
/// and vector must be ready before a separate unmask operation is attempted.
pub unsafe fn configure_ioapic_route(
    io_base: u64, gsi_base: u32, gsi: u32, vector: u8,
    destination_apic_id: u8, active_low: bool, level_triggered: bool,
) -> Result<(), ApicError> {
    if io_base == 0 || io_base & 0xfff != 0 || !(VECTOR_FIRST..=VECTOR_LAST).contains(&vector)
        || !VECTOR_ALLOCATED[vector as usize].load(Ordering::Acquire) {
        return Err(ApicError::InvalidVector);
    }
    if gsi < gsi_base || gsi - gsi_base >= 120 { return Err(ApicError::InvalidGsi); }
    unsafe fn select(base: u64, reg: u8) { unsafe { core::ptr::write_volatile(base as *mut u32, reg as u32); } }
    unsafe fn read(base: u64) -> u32 { unsafe { core::ptr::read_volatile((base + 0x10) as *const u32) } }
    unsafe fn write(base: u64, value: u32) { unsafe { core::ptr::write_volatile((base + 0x10) as *mut u32, value); } }
    let pin = (gsi - gsi_base) as u8;
    unsafe { select(io_base, 1); }
    if pin as u32 > ((unsafe { read(io_base) } >> 16) & 0xff) { return Err(ApicError::InvalidGsi); }
    let low_reg = 0x10u8.wrapping_add(pin.wrapping_mul(2));
    let mut low = vector as u32 | (1 << 16); // masked until explicitly enabled
    if active_low { low |= 1 << 13; }
    if level_triggered { low |= 1 << 15; }
    unsafe {
        select(io_base, low_reg);
        write(io_base, low);
        select(io_base, low_reg.wrapping_add(1));
        write(io_base, (destination_apic_id as u32) << 24);
    }
    Ok(())
}

/// Install and unmask one legacy ISA IRQ using validated MADT overrides.
/// The I/O-APIC MMIO page must already be mapped uncached and the target APIC
/// ID must be addressable by xAPIC. Returns the allocated vector on success.
pub unsafe fn install_isa_irq_route(
    madt: &crate::acpi::MadtInfo,
    isa_irq: u8,
    destination_apic_id: u8,
    handler: InterruptHandler,
    context: *mut core::ffi::c_void,
) -> Result<u8, ApicError> {
    let (gsi, active_low, level_triggered) = crate::acpi::resolve_isa_irq(madt, isa_irq);
    let mut selected: Option<crate::acpi::IoApicEntry> = None;
    for entry in madt.io_apics.iter().take(madt.io_apic_count) {
        if entry.gsi_base <= gsi && selected.is_none_or(|old| entry.gsi_base > old.gsi_base) {
            selected = Some(*entry);
        }
    }
    let io = selected.ok_or(ApicError::InvalidGsi)?;
    let vector = allocate_interrupt_vector()?;
    if let Err(error) = register_interrupt_handler(vector, handler, context) {
        let _ = release_interrupt_vector(vector);
        return Err(error);
    }
    if let Err(error) = unsafe {
        configure_ioapic_route(io.address as u64, io.gsi_base, gsi, vector, destination_apic_id, active_low, level_triggered)
    } {
        let _ = release_interrupt_vector(vector);
        return Err(error);
    }
    if let Err(error) = unsafe { set_ioapic_irq_enabled(io.address as u64, io.gsi_base, gsi, true) } {
        let _ = release_interrupt_vector(vector);
        return Err(error);
    }
    Ok(vector)
}

/// Send an ordinary fixed-vector IPI through the initialized local xAPIC.
/// This is deliberately separate from the TLB shootdown vector and does not
/// assume any particular semantics for the target handler.
pub unsafe fn request_ipi(destination_apic_id: u8, vector: u8) -> Result<(), ApicError> {
    let base = local_apic_base().ok_or(ApicError::InvalidBase)?;
    if vector < 0x20 || vector == 0xff { return Err(ApicError::InvalidVector); }
    unsafe { send_fixed_ipi(base, destination_apic_id, vector) }
}

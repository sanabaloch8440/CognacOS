//! Platform Hardware Abstraction Layer (HAL) for CognacOS.
//!
//! This layer isolates x86-64 CPU/interrupt/port primitives from executive,
//! scheduler, and driver code. It is Windows-NT-inspired, not a binary-compatible
//! Windows HAL or WDK implementation. Interrupt delivery remains disabled
//! until the IDT, APIC routing, and timer are completely installed.

#[path = "hal/apic.rs"]
mod apic;

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

pub const MAX_CPUS: usize = 64;
pub const CPU_OFFLINE: u8 = 0;
pub const CPU_DISCOVERED: u8 = 1;
pub const CPU_STARTING: u8 = 2;
pub const CPU_ONLINE: u8 = 3;
pub const CPU_FAILED: u8 = 4;

#[repr(C, align(64))]
pub struct CpuLocal {
    self_pointer: AtomicU64,
    pub logical_id: AtomicU32,
    pub apic_id: AtomicU32,
    pub state: AtomicU8,
    pub irq_depth: AtomicU32,
    pub preempt_count: AtomicU32,
    pub current_thread: AtomicU64,
    pub idle_ticks: AtomicU64,
    pub ipi_pending: AtomicU32,
}
impl CpuLocal {
    const fn new(id: u32) -> Self {
        Self {
            self_pointer: AtomicU64::new(0), logical_id: AtomicU32::new(id), apic_id: AtomicU32::new(u32::MAX),
            state: AtomicU8::new(CPU_OFFLINE), irq_depth: AtomicU32::new(0),
            preempt_count: AtomicU32::new(0), current_thread: AtomicU64::new(0),
            idle_ticks: AtomicU64::new(0), ipi_pending: AtomicU32::new(0),
        }
    }
}
/// NT executive lifecycle observer. HAL publishes hardware topology transitions
/// through this narrow callback without importing or depending on ntoskrnl.
pub type ProcessorChangeNotifier = fn(usize, u32);
static PROCESSOR_CHANGE_NOTIFIER: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// NT clock/timer callback registered by Ki. HAL publishes clock events without
/// depending on dispatcher, timer, or scheduler internals.
pub type ClockInterruptNotifier = fn(u64);
static CLOCK_INTERRUPT_NOTIFIER: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
pub fn register_clock_interrupt_notifier(notifier: ClockInterruptNotifier) -> Result<(), HalError> {
    let raw = notifier as usize;
    match CLOCK_INTERRUPT_NOTIFIER.compare_exchange(0, raw, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Ok(()),
        Err(existing) if existing == raw => Ok(()),
        Err(_) => Err(HalError::InvalidTransition),
    }
}
fn notify_clock_interrupt(now: u64) -> bool {
    let raw = CLOCK_INTERRUPT_NOTIFIER.load(Ordering::Acquire);
    if raw == 0 { return false; }
    let notifier: ClockInterruptNotifier = unsafe { core::mem::transmute(raw) };
    notifier(now);
    true
}

/// Install the NT executive's processor-change observer. Registration is
/// idempotent for the same callback and rejects attempts to replace an observer.
pub fn register_processor_change_notifier(notifier: ProcessorChangeNotifier) -> Result<(), HalError> {
    let raw = notifier as usize;
    match PROCESSOR_CHANGE_NOTIFIER.compare_exchange(0, raw, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Ok(()),
        Err(existing) if existing == raw => Ok(()),
        Err(_) => Err(HalError::InvalidTransition),
    }
}

fn notify_processor_change(index: usize, change_type: u32) {
    let raw = PROCESSOR_CHANGE_NOTIFIER.load(Ordering::Acquire);
    if raw != 0 {
        // Function pointers are only written by register_processor_change_notifier
        // and never removed while processors can transition.
        let notifier: ProcessorChangeNotifier = unsafe { core::mem::transmute(raw) };
        notifier(index, change_type);
    }
}

static CPU_LOCALS: [CpuLocal; MAX_CPUS] = [
    CpuLocal::new(0), CpuLocal::new(1), CpuLocal::new(2), CpuLocal::new(3),
    CpuLocal::new(4), CpuLocal::new(5), CpuLocal::new(6), CpuLocal::new(7),
    CpuLocal::new(8), CpuLocal::new(9), CpuLocal::new(10), CpuLocal::new(11),
    CpuLocal::new(12), CpuLocal::new(13), CpuLocal::new(14), CpuLocal::new(15),
    CpuLocal::new(16), CpuLocal::new(17), CpuLocal::new(18), CpuLocal::new(19),
    CpuLocal::new(20), CpuLocal::new(21), CpuLocal::new(22), CpuLocal::new(23),
    CpuLocal::new(24), CpuLocal::new(25), CpuLocal::new(26), CpuLocal::new(27),
    CpuLocal::new(28), CpuLocal::new(29), CpuLocal::new(30), CpuLocal::new(31),
    CpuLocal::new(32), CpuLocal::new(33), CpuLocal::new(34), CpuLocal::new(35),
    CpuLocal::new(36), CpuLocal::new(37), CpuLocal::new(38), CpuLocal::new(39),
    CpuLocal::new(40), CpuLocal::new(41), CpuLocal::new(42), CpuLocal::new(43),
    CpuLocal::new(44), CpuLocal::new(45), CpuLocal::new(46), CpuLocal::new(47),
    CpuLocal::new(48), CpuLocal::new(49), CpuLocal::new(50), CpuLocal::new(51),
    CpuLocal::new(52), CpuLocal::new(53), CpuLocal::new(54), CpuLocal::new(55),
    CpuLocal::new(56), CpuLocal::new(57), CpuLocal::new(58), CpuLocal::new(59),
    CpuLocal::new(60), CpuLocal::new(61), CpuLocal::new(62), CpuLocal::new(63),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HalError { InvalidCpu, DuplicateApicId, InvalidTransition, CpuTableFull, InvalidMmioRange, InvalidInterruptBase, PlatformFailure }

/// Reserve a logical CPU slot for an ACPI-discovered APIC ID. Does not start it.
/// Mark the bootstrap processor online in logical slot zero before AP discovery.
pub fn initialize_bsp(apic_id: u32) -> Result<usize, HalError> {
    let cpu = &CPU_LOCALS[0];
    cpu.state.compare_exchange(CPU_OFFLINE, CPU_ONLINE, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| HalError::InvalidTransition)?;
    cpu.apic_id.store(apic_id, Ordering::Release);
    Ok(0)
}

pub fn discover_cpu(apic_id: u32) -> Result<usize, HalError> {
    if cpu_index_for_apic(apic_id).is_some() { return Err(HalError::DuplicateApicId); }
    for (i, cpu) in CPU_LOCALS.iter().enumerate() {
        if cpu.state.compare_exchange(CPU_OFFLINE, CPU_DISCOVERED, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            cpu.apic_id.store(apic_id, Ordering::Release);
            return Ok(i);
        }
    }
    Err(HalError::CpuTableFull)
}

pub fn cpu_index_for_apic(apic_id: u32) -> Option<usize> {
    CPU_LOCALS.iter().position(|cpu| cpu.state.load(Ordering::Acquire) != CPU_OFFLINE
        && cpu.apic_id.load(Ordering::Acquire) == apic_id)
}
/// Number of generic HAL IPIs awaiting processing on this logical CPU.
pub fn take_pending_ipi(index: usize) -> Result<bool, HalError> {
    let cpu = cpu_local(index)?;
    Ok(cpu.ipi_pending.swap(0, Ordering::AcqRel) != 0)
}
pub fn mark_ipi_pending(index: usize) -> Result<(), HalError> {
    let cpu = cpu_local(index)?;
    cpu.ipi_pending.store(1, Ordering::Release);
    Ok(())
}

pub fn cpu_state(index: usize) -> Result<u8, HalError> {
    CPU_LOCALS.get(index).map(|c| c.state.load(Ordering::Acquire)).ok_or(HalError::InvalidCpu)
}
pub fn mark_cpu_starting(index: usize) -> Result<(), HalError> {
    let cpu = CPU_LOCALS.get(index).ok_or(HalError::InvalidCpu)?;
    cpu.state.compare_exchange(CPU_DISCOVERED, CPU_STARTING, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ()).map_err(|_| HalError::InvalidTransition)
}
pub fn mark_cpu_online(index: usize) -> Result<(), HalError> {
    let cpu = CPU_LOCALS.get(index).ok_or(HalError::InvalidCpu)?;
    cpu.state.compare_exchange(CPU_STARTING, CPU_ONLINE, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| HalError::InvalidTransition)?;
    notify_processor_change(index, 1);
    Ok(())
}
/// Publish processor removal after the caller has quiesced that CPU and migrated
/// its runnable work. The BSP cannot be offlined through this helper.
pub fn mark_cpu_offline(index: usize) -> Result<(), HalError> {
    if index==0 { return Err(HalError::InvalidTransition); }
    let cpu=CPU_LOCALS.get(index).ok_or(HalError::InvalidCpu)?;
    cpu.state.compare_exchange(CPU_ONLINE,CPU_OFFLINE,Ordering::AcqRel,Ordering::Acquire)
        .map_err(|_|HalError::InvalidTransition)?;
    notify_processor_change(index, 2);
    Ok(())
}
pub fn mark_cpu_failed(index: usize) -> Result<(), HalError> {
    let cpu = CPU_LOCALS.get(index).ok_or(HalError::InvalidCpu)?;
    let state = cpu.state.load(Ordering::Acquire);
    if state != CPU_DISCOVERED && state != CPU_STARTING { return Err(HalError::InvalidTransition); }
    cpu.state.store(CPU_FAILED, Ordering::Release);
    Ok(())
}
pub fn online_cpu_count() -> usize {
    CPU_LOCALS.iter().filter(|c| c.state.load(Ordering::Acquire) == CPU_ONLINE).count()
}
pub fn cpu_local(index: usize) -> Result<&'static CpuLocal, HalError> {
    CPU_LOCALS.get(index).ok_or(HalError::InvalidCpu)
}

/// Install this CPU's GS base. Must run separately on every processor after its
/// stack, GDT/TSS and interrupt state are initialized. GS:[0] then yields CpuLocal.
pub unsafe fn install_cpu_local(index: usize) -> Result<(), HalError> {
    let cpu = cpu_local(index)?;
    let local = cpu as *const CpuLocal as u64;
    cpu.self_pointer.store(local, Ordering::Release);
    const IA32_GS_BASE: u32 = 0xC000_0101;
    const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;
    unsafe {
        core::arch::asm!("wrmsr", in("ecx") IA32_GS_BASE, in("eax") local as u32,
            in("edx") (local >> 32) as u32, options(nostack, preserves_flags));
        core::arch::asm!("wrmsr", in("ecx") IA32_KERNEL_GS_BASE, in("eax") 0u32,
            in("edx") 0u32, options(nostack, preserves_flags));
    }
    Ok(())
}
#[inline]
pub fn current_cpu_local() -> Option<&'static CpuLocal> {
    let ptr: u64;
    unsafe { core::arch::asm!("mov {}, gs:[0]", out(reg) ptr, options(nostack, readonly, preserves_flags)); }
    if ptr == 0 { None } else { Some(unsafe { &*(ptr as *const CpuLocal) }) }
}

/// A non-poisoning IRQ-save spin lock. It disables local interrupts before
/// spinning, preventing same-CPU interrupt re-entry from deadlocking on a lock
/// held by the interrupted code. Never hold across blocking, allocation, device
/// waits, or calls that may sleep.
pub struct IrqSpinLock<T> { held: AtomicBool, value: UnsafeCell<T> }
unsafe impl<T: Send> Sync for IrqSpinLock<T> {}
unsafe impl<T: Send> Send for IrqSpinLock<T> {}
impl<T> IrqSpinLock<T> {
    pub const fn new(value: T) -> Self { Self { held: AtomicBool::new(false), value: UnsafeCell::new(value) } }
    pub fn lock(&self) -> IrqSpinLockGuard<'_, T> {
        let flags: u64;
        unsafe {
            core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags));
            core::arch::asm!("cli", options(nomem, nostack));
        }
        while self.held.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            // APs currently run with interrupts disabled. Service shared-kernel
            // TLB requests even while contending on pool/VM locks, or a shootdown
            // waiter could deadlock behind the very lock it is waiting to acquire.
            #[cfg(target_os = "none")]
            if let Some(cpu) = current_cpu_local() {
                crate::kernel_vm::smp_tlb_poll(cpu.logical_id.load(Ordering::Relaxed) as usize);
            }
            core::hint::spin_loop();
        }
        IrqSpinLockGuard { lock: self, restore_interrupts: flags & (1 << 9) != 0 }
    }
    pub fn try_lock(&self) -> Option<IrqSpinLockGuard<'_, T>> {
        let flags: u64;
        unsafe {
            core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags));
            core::arch::asm!("cli", options(nomem, nostack));
        }
        if self.held.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            if flags & (1 << 9) != 0 { unsafe { core::arch::asm!("sti", options(nomem, nostack)); } }
            return None;
        }
        Some(IrqSpinLockGuard { lock: self, restore_interrupts: flags & (1 << 9) != 0 })
    }
}
pub struct IrqSpinLockGuard<'a, T> { lock: &'a IrqSpinLock<T>, restore_interrupts: bool }
impl<T> core::ops::Deref for IrqSpinLockGuard<'_, T> { type Target=T; fn deref(&self)->&T { unsafe { &*self.lock.value.get() } } }
impl<T> core::ops::DerefMut for IrqSpinLockGuard<'_, T> { fn deref_mut(&mut self)->&mut T { unsafe { &mut *self.lock.value.get() } } }
impl<T> Drop for IrqSpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.held.store(false, Ordering::Release);
        if self.restore_interrupts { unsafe { core::arch::asm!("sti", options(nomem, nostack)); } }
    }
}

/// Save RFLAGS and disable maskable interrupts on the current CPU. The
/// returned token must be passed to restore_interrupts on the same CPU and
/// execution path; it is not an IRQL token and does not mask NMI.
#[inline]
pub unsafe fn disable_interrupts() -> u64 {
    let flags: u64;
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags));
        core::arch::asm!("cli", options(nomem, nostack));
    }
    flags
}

/// Restore only the caller's prior IF state. Never unconditionally execute STI
/// from a nested critical section that entered with interrupts already disabled.
#[inline]
pub unsafe fn restore_interrupts(saved_rflags: u64) {
    if saved_rflags & (1 << 9) != 0 {
        unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    } else {
        unsafe { core::arch::asm!("cli", options(nomem, nostack)); }
    }
}

#[inline]
pub fn interrupts_enabled() -> bool {
    let flags: u64;
    unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags)); }
    flags & (1 << 9) != 0
}

/// Typed port-I/O operations live here instead of in individual device drivers.
#[inline] pub unsafe fn read_port_u32(port: u16) -> u32 {
    let value: u32;
    unsafe { core::arch::asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack, preserves_flags)); }
    value
}
#[inline] pub unsafe fn write_port_u32(port: u16, value: u32) {
    unsafe { core::arch::asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags)); }
}

#[inline] pub unsafe fn read_port_u16(port:u16)->u16{
    let value:u16;
    unsafe{core::arch::asm!("in ax, dx",in("dx") port,out("ax") value,options(nomem,nostack,preserves_flags));}
    value
}
#[inline] pub unsafe fn write_port_u16(port:u16,value:u16){
    unsafe{core::arch::asm!("out dx, ax",in("dx") port,in("ax") value,options(nomem,nostack,preserves_flags));}
}

/// Volatile MMIO accessors. The caller must first map the device aperture with
/// correct cache attributes and guarantee the complete range is mapped.
#[inline] pub unsafe fn read_mmio_u8(address:*const u8)->u8{unsafe{core::ptr::read_volatile(address)}}
#[inline] pub unsafe fn read_mmio_u16(address:*const u16)->u16{unsafe{core::ptr::read_volatile(address)}}
#[inline] pub unsafe fn read_mmio_u64(address:*const u64)->u64{unsafe{core::ptr::read_volatile(address)}}
#[inline] pub unsafe fn write_mmio_u8(address:*mut u8,value:u8){unsafe{core::ptr::write_volatile(address,value);}}
#[inline] pub unsafe fn write_mmio_u16(address:*mut u16,value:u16){unsafe{core::ptr::write_volatile(address,value);core::sync::atomic::fence(Ordering::SeqCst);}}
#[inline] pub unsafe fn write_mmio_u64(address:*mut u64,value:u64){unsafe{core::ptr::write_volatile(address,value);core::sync::atomic::fence(Ordering::SeqCst);}}

/// Validate a device MMIO span before a platform-specific mapper maps it.
/// Mapping/cache attributes are deliberately not guessed by this helper.
pub fn validate_mmio_range(base: u64, length: u64, physical_limit: u64) -> Result<(), HalError> {
    if length == 0 || base & 0xfff != 0 || base.checked_add(length).filter(|end| *end <= physical_limit).is_none() {
        return Err(HalError::InvalidMmioRange);
    }
    Ok(())
}


/// Pure validation checks safe to run before interrupt controllers are enabled.
pub fn self_test() -> bool {
    validate_mmio_range(0xfee0_0000, 0x1000, 0x1_0000_0000).is_ok()
        && validate_mmio_access(0x10, 4, 0x1000).is_ok()
        && validate_mmio_access(0x11, 4, 0x1000) == Err(HalError::InvalidMmioRange)
        && validate_mmio_access(0xfff, 4, 0x1000) == Err(HalError::InvalidMmioRange)
        && validate_mmio_access(0, 3, 0x1000) == Err(HalError::InvalidMmioRange)
        && validate_mmio_range(0xfee0_0001, 0x1000, 0x1_0000_0000) == Err(HalError::InvalidMmioRange)
        && validate_mmio_range(u64::MAX - 0xfff, 0x2000, u64::MAX) == Err(HalError::InvalidMmioRange)
        && CPU_LOCALS[0].logical_id.load(Ordering::Relaxed) == 0
        && CPU_LOCALS[MAX_CPUS - 1].logical_id.load(Ordering::Relaxed) == (MAX_CPUS - 1) as u32
        && CPU_LOCALS[0].state.load(Ordering::Relaxed) == CPU_ONLINE
        && CPU_LOCALS.iter().skip(1).all(|cpu| cpu.state.load(Ordering::Relaxed) == CPU_OFFLINE)
        && apic::irql_to_apic_class(0) == 0
        && apic::irql_to_apic_class(1) == 0
        && apic::irql_to_apic_class(2) == 1
        && apic::irql_to_apic_class(3) == 2
        && apic::irql_to_apic_class(12) == 11
        && apic::irql_to_apic_class(31) == 15
        && exports::self_test()
}

/// Platform clock services consumed by the NT executive. Keep APIC/TSC
/// details private to the HAL implementation rather than importing them in Ke.
#[inline]
pub fn query_performance_counter() -> u64 { apic::monotonic_ticks() }

#[inline]
pub fn query_performance_frequency_hz() -> Option<u64> { apic::tsc_frequency_hz() }

#[inline]
pub fn relative_timeout_deadline(interval_100ns: i64) -> Option<u64> {
    apic::relative_timeout_deadline(interval_100ns)
}

#[inline]
pub fn local_clock_event_active() -> bool { apic::local_timer_active() }

#[inline]
pub fn query_cross_core_clock_status() -> (bool, u64) { apic::tsc_cross_core_status() }

/// Apply an NT logical IRQL to the platform interrupt-priority mechanism.
#[inline]
pub unsafe fn set_interrupt_priority(irql: u8) -> Result<(), ()> {
    unsafe { apic::set_task_priority(irql).map_err(|_| ()) }
}

/// Platform interrupt-controller services. These are the only intended
/// consumers of the xAPIC/I/O-APIC implementation outside the HAL.
pub const LOCAL_TIMER_VECTOR: u8 = 0xE0;
pub const KE_BROADCAST_VECTOR: u8 = 0xEF;
pub const HAL_IPI_VECTOR: u8 = 0xF0;
pub const TLB_SHOOTDOWN_VECTOR: u8 = 0xF1;
pub const TSC_SYNC_VECTOR: u8 = 0xF2;

#[inline]
pub unsafe fn initialize_local_interrupt_controller(base: u64) -> Result<u32, HalError> {
    unsafe { apic::initialize_local_apic(base) }.map_err(|error| match error {
        apic::ApicError::InvalidBase => HalError::InvalidInterruptBase,
        _ => HalError::PlatformFailure,
    })
}
#[inline]
pub unsafe fn mask_interrupt_controller_routes(base: u64) -> Result<(), HalError> {
    unsafe { apic::mask_all_ioapic(base) }.map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub unsafe fn calibrate_local_clock_event() -> Result<u32, HalError> {
    unsafe { apic::calibrate_local_timer() }.map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub unsafe fn start_local_clock_event() -> Result<(), HalError> {
    unsafe { apic::start_calibrated_local_timer() }.map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub fn local_interrupt_controller_base() -> Option<u64> { apic::local_apic_base() }
#[inline]
pub unsafe fn end_local_interrupt() {
    if let Some(base) = local_interrupt_controller_base() {
        unsafe { apic::local_apic_eoi(base); }
    }
}
#[inline]
pub fn capture_clock_synchronization_sample() { apic::capture_tsc_sync_sample(); }
#[inline]
pub unsafe fn dispatch_platform_interrupt(vector: u8, frame: *mut core::ffi::c_void) -> bool {
    unsafe { apic::dispatch_external_interrupt(vector, frame) }
}
#[inline]
pub fn local_clock_event_interrupt() { apic::timer_interrupt(); }
#[inline]
pub fn publish_clock_interrupt(now: u64) -> bool { notify_clock_interrupt(now) }

#[inline]
pub unsafe fn send_fixed_ipi(base: u64, destination_apic_id: u8, vector: u8) -> Result<(), HalError> {
    unsafe { apic::send_fixed_ipi(base, destination_apic_id, vector) }.map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub unsafe fn send_processor_init(base: u64, destination_apic_id: u8) -> Result<(), HalError> {
    unsafe { apic::send_init_ipi(base, destination_apic_id) }.map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub unsafe fn send_processor_init_deassert(base: u64, destination_apic_id: u8) -> Result<(), HalError> {
    unsafe { apic::send_init_deassert_ipi(base, destination_apic_id) }.map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub unsafe fn send_processor_startup(base: u64, destination_apic_id: u8, trampoline_physical: u64) -> Result<(), HalError> {
    unsafe { apic::send_startup_ipi(base, destination_apic_id, trampoline_physical) }.map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub unsafe fn initialize_processor_interrupt_controller(base: u64) -> Result<u32, HalError> {
    unsafe { apic::initialize_local_apic(base) }.map_err(|_| HalError::PlatformFailure)
}
pub unsafe fn synchronize_processor_clock(logical_cpu: usize, destination_apic_id: u8) -> Result<i64, HalError> {
    unsafe { apic::synchronize_tsc_with_cpu(logical_cpu, destination_apic_id) }.map_err(|_| HalError::PlatformFailure)
}
pub unsafe fn request_processor_ipi(destination_apic_id: u8, vector: u8) -> Result<(), HalError> {
    unsafe { apic::request_ipi(destination_apic_id, vector) }.map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub unsafe fn configure_interrupt_route(
    io_base: u64, gsi_base: u32, gsi: u32, vector: u8,
    destination_apic_id: u8, active_low: bool, level_triggered: bool,
) -> Result<(), HalError> {
    unsafe { apic::configure_ioapic_route(io_base, gsi_base, gsi, vector, destination_apic_id, active_low, level_triggered) }
        .map_err(|_| HalError::PlatformFailure)
}
#[inline]
pub unsafe fn set_interrupt_route_enabled(io_base: u64, gsi_base: u32, gsi: u32, enabled: bool) -> Result<(), HalError> {
    unsafe { apic::set_ioapic_irq_enabled(io_base, gsi_base, gsi, enabled) }.map_err(|_| HalError::PlatformFailure)
}

/// Windows HAL export compatibility inventory and fail-closed stub metadata.
pub mod exports;


// ---- Low-level device I/O primitives for port and MMIO-backed drivers ----
/// Read an x86 I/O-port byte. Caller must run at a privileged level and hold any
/// device-specific serialization lock; this is not safe to expose directly to user mode.
#[inline]
pub unsafe fn read_port_u8(port: u16) -> u8 {
    let value: u8;
    unsafe { core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags)); }
    value
}
/// Write an x86 I/O-port byte. Caller must validate resource ownership and serialize
/// controller transactions before issuing the instruction.
#[inline]
pub unsafe fn write_port_u8(port: u16, value: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}
/// Read a 32-bit MMIO register from an already mapped, correctly-sized device region.
/// The caller must ensure the mapping uses the device's required uncached memory type.
#[inline]
pub unsafe fn read_mmio_u32(address: *const u32) -> u32 {
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
    let value = unsafe { core::ptr::read_volatile(address) };
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
    value
}
/// Write a 32-bit MMIO register in an already mapped device region.
#[inline]
pub unsafe fn write_mmio_u32(address: *mut u32, value: u32) {
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
    unsafe { core::ptr::write_volatile(address, value); }
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
}


/// Validate one register access against an already mapped device aperture.
/// The span is the mapping length, not a guessed physical address range.
pub fn validate_mmio_access(offset: u64, width: u64, mapping_length: u64) -> Result<(), HalError> {
    if !matches!(width, 1 | 2 | 4 | 8) || offset % width != 0
        || offset.checked_add(width).filter(|end| *end <= mapping_length).is_none() {
        return Err(HalError::InvalidMmioRange);
    }
    Ok(())
}

/// Device-ordering barriers for register/data-structure handoff. These do not
/// replace cache maintenance for non-coherent DMA or a correctly typed mapping.
#[inline]
pub fn device_read_barrier() { core::sync::atomic::fence(Ordering::Acquire); }
#[inline]
pub fn device_write_barrier() { core::sync::atomic::fence(Ordering::Release); }
#[inline]
pub fn device_full_barrier() { core::sync::atomic::fence(Ordering::SeqCst); }

/// Bounds-checked 32-bit register access for a previously mapped MMIO aperture.
#[inline]
pub unsafe fn read_mmio_u32_checked(base: *const u8, mapping_length: u64, offset: u64) -> Result<u32, HalError> {
    validate_mmio_access(offset, 4, mapping_length)?;
    let address = (base as u64).checked_add(offset).ok_or(HalError::InvalidMmioRange)?;
    device_read_barrier();
    // SAFETY: caller guarantees base describes a live mapping of mapping_length bytes.
    let value = unsafe { read_mmio_u32(address as *const u32) };
    device_read_barrier();
    Ok(value)
}
#[inline]
pub unsafe fn write_mmio_u32_checked(base: *mut u8, mapping_length: u64, offset: u64, value: u32) -> Result<(), HalError> {
    validate_mmio_access(offset, 4, mapping_length)?;
    let address = (base as u64).checked_add(offset).ok_or(HalError::InvalidMmioRange)?;
    device_write_barrier();
    // SAFETY: caller guarantees base describes a live writable mapping of mapping_length bytes.
    unsafe { write_mmio_u32(address as *mut u32, value); }
    device_full_barrier();
    Ok(())
}


// ---- Versioned driver-facing HAL service publication -----------------------
// The kernel is the owner of this table; drivers consume only the stable
// contract crate. Hardware services which do not have a safe, complete backend
// remain None rather than being exposed as successful no-op implementations.
type DriverHalStatus = cognacos_hal_contract::HalStatus;
fn driver_hal_success() -> DriverHalStatus { DriverHalStatus::SUCCESS }
fn driver_hal_invalid() -> DriverHalStatus { DriverHalStatus::INVALID_PARAMETER }
fn driver_hal_unsupported() -> DriverHalStatus { DriverHalStatus::NOT_SUPPORTED }

unsafe extern "C" fn service_read_port_u8(port:u16,out:*mut u8)->DriverHalStatus {
    if out.is_null(){return driver_hal_invalid()}
    // SAFETY: this callback is called only by trusted kernel-mode driver code.
    unsafe{out.write(read_port_u8(port));}
    driver_hal_success()
}
unsafe extern "C" fn service_write_port_u8(port:u16,value:u8)->DriverHalStatus {
    // SAFETY: privileged port access is intentionally restricted to kernel drivers.
    unsafe{write_port_u8(port,value);}
    driver_hal_success()
}
unsafe extern "C" fn service_read_mmio_u32(address:u64,out:*mut u32)->DriverHalStatus {
    if address==0 || address&3!=0 || out.is_null(){return driver_hal_invalid()}
    // SAFETY: driver must pass an address within a live device mapping it owns.
    unsafe{out.write(read_mmio_u32(address as *const u32));}
    device_read_barrier();
    driver_hal_success()
}
unsafe extern "C" fn service_write_mmio_u32(address:u64,value:u32)->DriverHalStatus {
    if address==0 || address&3!=0{return driver_hal_invalid()}
    // SAFETY: driver must pass an address within a live writable device mapping.
    unsafe{write_mmio_u32(address as *mut u32,value);}
    device_full_barrier();
    driver_hal_success()
}
extern "C" fn service_query_counter(value:*mut u64,frequency:*mut u64)->DriverHalStatus {
    if value.is_null() || frequency.is_null(){return driver_hal_invalid()}
    let now=apic::monotonic_ticks();
    let Some(hz)=apic::tsc_frequency_hz() else{return driver_hal_unsupported()};
    // SAFETY: checked output pointers are supplied by trusted kernel-mode callers.
    unsafe{value.write(now);frequency.write(hz);}
    driver_hal_success()
}
unsafe extern "C" fn service_send_ipi(apic_id:u32,vector:u8)->DriverHalStatus {
    if apic_id>u8::MAX as u32{return driver_hal_unsupported()}
    // SAFETY: APIC service validates the vector and requires an initialized local APIC.
    match unsafe{apic::request_ipi(apic_id as u8,vector)} {
        Ok(())=>driver_hal_success(),
        Err(_)=>DriverHalStatus::DEVICE_ERROR,
    }
}

/// Return the stable base HAL contract for kernel-mode driver adapters.
/// MMIO callbacks operate on an already mapped virtual address; this function
/// does not map physical BARs or authorize arbitrary physical addresses.
pub fn driver_service_table()->cognacos_hal_contract::HalServiceTable {
    cognacos_hal_contract::HalServiceTable {
        size:core::mem::size_of::<cognacos_hal_contract::HalServiceTable>() as u32,
        version:cognacos_hal_contract::HAL_CONTRACT_VERSION,
        read_port_u8:Some(service_read_port_u8),
        write_port_u8:Some(service_write_port_u8),
        read_mmio_u32:Some(service_read_mmio_u32),
        write_mmio_u32:Some(service_write_mmio_u32),
        query_counter:Some(service_query_counter),
        send_ipi:Some(service_send_ipi),
        // IRQL transitions require the executive's per-thread IRQL state and
        // restoration token; never publish an incomplete implementation.
        set_irql:None,
    }
}


/// Map a validated physical PCI BAR into the dedicated uncached MMIO aperture.
/// The memory manager rejects ordinary RAM and owns virtual-slot lifetime.
unsafe extern "C" fn service_map_device_io_space(physical:u64,length:u64,flags:u32,out:*mut u64)->DriverHalStatus {
    use cognacos_hal_contract::{validate_device_mapping_flags,HAL_MEMORY_DEVICE,HAL_MEMORY_UNCACHED,HAL_MEMORY_READ_WRITE};
    if out.is_null() || !validate_device_mapping_flags(flags)
        || flags & (HAL_MEMORY_DEVICE|HAL_MEMORY_UNCACHED|HAL_MEMORY_READ_WRITE)
            != (HAL_MEMORY_DEVICE|HAL_MEMORY_UNCACHED|HAL_MEMORY_READ_WRITE)
        || length==0 || physical.checked_add(length).is_none() {
        return driver_hal_invalid();
    }
    match crate::kernel_vm::map_io_space(physical,length) {
        Ok(address) if address!=0 => { unsafe { out.write(address); } driver_hal_success() }
        Ok(_) => driver_hal_invalid(),
        Err(_) => driver_hal_unsupported(),
    }
}
unsafe extern "C" fn service_unmap_device_io_space(address:u64,length:u64)->DriverHalStatus {
    if address==0 || length==0 { return driver_hal_invalid(); }
    match crate::kernel_vm::unmap_io_space(address,length) {
        Ok(())=>driver_hal_success(),
        Err(_)=>driver_hal_invalid(),
    }
}
extern "C" fn service_memory_barrier() { device_full_barrier(); }

/// Extended HAL services used by the PnP resource arbiter. MMIO mapping is
/// advertised only after the kernel VM has initialized its dedicated aperture.
pub fn driver_extended_service_table()->cognacos_hal_contract::HalExtendedServiceTable {
    let ready=crate::kernel_vm::is_initialized();
    cognacos_hal_contract::HalExtendedServiceTable {
        size:core::mem::size_of::<cognacos_hal_contract::HalExtendedServiceTable>() as u32,
        version:cognacos_hal_contract::HAL_EXTENDED_SERVICES_VERSION,
        map_device_io_space:if ready {Some(service_map_device_io_space)} else {None},
        unmap_device_io_space:if ready {Some(service_unmap_device_io_space)} else {None},
        synchronize_interrupt:None,
        memory_barrier:Some(service_memory_barrier),
    }
}

/// Allocate a common buffer in the requesting device's IOMMU domain.
/// The device must already have a validated PCI requester identity and the real
/// IOMMU backend must be active. There is intentionally no physical-address fallback.
unsafe extern "C" fn service_allocate_dma(
    device_object:u64,size:u64,alignment:u64,maximum_address:u64,flags:u32,
    cpu_out:*mut u64,device_out:*mut u64,
)->DriverHalStatus {
    if device_object==0 || size==0 || size>u32::MAX as u64 || alignment==0
        || !alignment.is_power_of_two() || alignment>crate::dma::PAGE_SIZE || maximum_address==0
        || flags & 3 != 3 || flags & !7 != 0 || cpu_out.is_null() || device_out.is_null() {
        return driver_hal_invalid();
    }
    if !crate::dma::iommu::available() { return driver_hal_unsupported(); }
    let object=device_object as *mut core::ffi::c_void;
    let adapter=crate::dma::create_device_adapter(object,3,maximum_address);
    if adapter.is_null() { return driver_hal_unsupported(); }
    match unsafe { crate::dma::iommu_allocate_common(adapter,size as usize,maximum_address) } {
        Ok((cpu,logical)) if !cpu.is_null() && logical!=0 && (cpu as u64)%alignment==0 => {
            unsafe { cpu_out.write(cpu as u64); device_out.write(logical); }
            driver_hal_success()
        }
        Ok((cpu,logical)) => {
            let _=unsafe { crate::dma::free_common_buffer_for_adapter(adapter,cpu,logical,size as usize) };
            driver_hal_invalid()
        }
        Err(_) => driver_hal_unsupported(),
    }
}
unsafe extern "C" fn service_free_dma(device_object:u64,cpu:u64,device_address:u64,size:u64)->DriverHalStatus {
    if device_object==0 || cpu==0 || device_address==0 || size==0 || size>usize::MAX as u64 {
        return driver_hal_invalid();
    }
    let adapter=crate::dma::create_device_adapter(device_object as *mut core::ffi::c_void,3,u64::MAX);
    if adapter.is_null() { return driver_hal_unsupported(); }
    match unsafe { crate::dma::free_common_buffer_for_adapter(adapter,cpu as *mut u8,device_address,size as usize) } {
        Ok(())=>driver_hal_success(),
        Err(_)=>driver_hal_unsupported(),
    }
}

/// Publish the real IOMMU-backed DMA services. Interrupt callbacks remain absent
/// until the kernel has a routed device vector and a safe deferred-DPC queue;
/// consumers must not start an interrupt-driven controller without those services.
pub fn driver_device_service_table()->cognacos_hal_contract::DeviceServiceTable {
    let dma_ready=crate::dma::iommu::available();
    cognacos_hal_contract::DeviceServiceTable {
        size:core::mem::size_of::<cognacos_hal_contract::DeviceServiceTable>() as u32,
        version:cognacos_hal_contract::DEVICE_SERVICES_VERSION,
        allocate_dma:if dma_ready {Some(service_allocate_dma)} else {None},
        free_dma:if dma_ready {Some(service_free_dma)} else {None},
        register_interrupt:None,
        unregister_interrupt:None,
        set_interrupt_enabled:None,
    }
}

#[cfg(test)]
mod driver_service_publication_tests {
    use super::*;
    #[test]
    fn base_service_table_is_versioned_and_exposes_only_existing_primitives() {
        let table=driver_service_table();
        assert!(table.compatible());
        assert!(table.read_port_u8.is_some());
        assert!(table.write_port_u8.is_some());
        assert!(table.read_mmio_u32.is_some());
        assert!(table.write_mmio_u32.is_some());
        assert!(table.query_counter.is_some());
        assert!(table.send_ipi.is_some());
        assert!(table.set_irql.is_none());
    }
    #[test]
    fn device_table_fails_closed_until_dma_and_interrupt_backends_are_real() {
        let table=driver_device_service_table();
        assert!(table.compatible());
        assert!(!table.has_dma());
        assert!(!table.has_interrupts());
    }
}

//! Platform Hardware Abstraction Layer (HAL) for RdotOS.
//!
//! This layer isolates x86-64 CPU/interrupt/port primitives from executive,
//! scheduler, and driver code. It is Windows-NT-inspired, not a binary-compatible
//! Windows HAL or WDK implementation. Interrupt delivery remains disabled
//! until the IDT, APIC routing, and timer are completely installed.

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
pub enum HalError { InvalidCpu, DuplicateApicId, InvalidTransition, CpuTableFull, InvalidMmioRange }

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
/// Merge enabled processors from a validated ACPI MADT into the HAL CPU
/// registry. The BSP must already have been registered with initialize_bsp().
/// This only discovers/reserves AP slots; it does not send INIT/SIPI or mark APs online.
pub fn register_madt_cpus(madt: &crate::acpi::MadtInfo, bsp_apic_id: u32) -> Result<usize, HalError> {
    if cpu_index_for_apic(bsp_apic_id).is_none() { initialize_bsp(bsp_apic_id)?; }
    let mut discovered = 0usize;
    for entry in madt.cpus.iter().take(madt.cpu_count) {
        if !entry.enabled || entry.apic_id == bsp_apic_id { continue; }
        if cpu_index_for_apic(entry.apic_id).is_some() { continue; }
        discover_cpu(entry.apic_id)?;
        discovered += 1;
    }
    Ok(discovered)
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
#[inline] pub unsafe fn read_port_u8(port: u16) -> u8 {
    let value: u8;
    unsafe { core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags)); }
    value
}
#[inline] pub unsafe fn write_port_u8(port: u16, value: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}
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
#[inline] pub unsafe fn read_mmio_u32(address:*const u32)->u32{unsafe{core::ptr::read_volatile(address)}}
#[inline] pub unsafe fn read_mmio_u64(address:*const u64)->u64{unsafe{core::ptr::read_volatile(address)}}
#[inline] pub unsafe fn write_mmio_u8(address:*mut u8,value:u8){unsafe{core::ptr::write_volatile(address,value);}}
#[inline] pub unsafe fn write_mmio_u16(address:*mut u16,value:u16){unsafe{core::ptr::write_volatile(address,value);core::sync::atomic::fence(Ordering::SeqCst);}}
#[inline] pub unsafe fn write_mmio_u32(address:*mut u32,value:u32){unsafe{core::ptr::write_volatile(address,value);core::sync::atomic::fence(Ordering::SeqCst);}}
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
        && validate_mmio_range(0xfee0_0001, 0x1000, 0x1_0000_0000) == Err(HalError::InvalidMmioRange)
        && validate_mmio_range(u64::MAX - 0xfff, 0x2000, u64::MAX) == Err(HalError::InvalidMmioRange)
        && CPU_LOCALS[0].logical_id.load(Ordering::Relaxed) == 0
        && CPU_LOCALS[MAX_CPUS - 1].logical_id.load(Ordering::Relaxed) == (MAX_CPUS - 1) as u32
        && CPU_LOCALS[0].state.load(Ordering::Relaxed) == CPU_ONLINE
        && CPU_LOCALS.iter().skip(1).all(|cpu| cpu.state.load(Ordering::Relaxed) == CPU_OFFLINE)
        && crate::apic::irql_to_apic_class(0) == 0
        && crate::apic::irql_to_apic_class(1) == 0
        && crate::apic::irql_to_apic_class(2) == 1
        && crate::apic::irql_to_apic_class(3) == 2
        && crate::apic::irql_to_apic_class(12) == 11
        && crate::apic::irql_to_apic_class(31) == 15
        && exports::self_test()
}

/// Windows HAL export compatibility inventory and fail-closed stub metadata.
pub mod exports;

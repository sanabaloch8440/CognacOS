//! Callable NT kernel executive (Ke) routines with ABI-compatible signatures.
use core::sync::atomic::Ordering;

const ALL_PROCESSOR_GROUPS: u16 = 0xffff;
const PASSIVE_LEVEL: u8 = 0;
const DISPATCH_LEVEL: u8 = 2;
const IRQL_STACK_DEPTH: usize = 16;
static DISPATCHER_OBJECT_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
static CURRENT_IRQL: [core::sync::atomic::AtomicU8; 64] =
    [const { core::sync::atomic::AtomicU8::new(PASSIVE_LEVEL) }; 64];
static IRQL_DEPTH: [core::sync::atomic::AtomicU8; 64] =
    [const { core::sync::atomic::AtomicU8::new(0) }; 64];
static IRQL_FLAGS: [core::sync::atomic::AtomicU64; 64 * IRQL_STACK_DEPTH] =
    [const { core::sync::atomic::AtomicU64::new(0) }; 64 * IRQL_STACK_DEPTH];

#[inline]
fn cpu_slot() -> usize {
    crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Relaxed) as usize)
        .filter(|id| *id < 64)
        .unwrap_or(0)
}
#[inline]
unsafe fn read_rflags() -> u64 {
    let flags: u64;
    unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags)); }
    flags
}
#[inline]
unsafe fn irq_disable() { unsafe { core::arch::asm!("cli", options(nomem, nostack)); } }
#[inline]
unsafe fn irq_restore(flags: u64) {
    if flags & (1 << 9) != 0 {
        unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    }
}
fn push_irq_state(flags: u64) {
    let cpu = cpu_slot();
    let depth = IRQL_DEPTH[cpu].load(Ordering::Relaxed) as usize;
    if depth < IRQL_STACK_DEPTH {
        IRQL_FLAGS[cpu * IRQL_STACK_DEPTH + depth].store(flags, Ordering::Relaxed);
        IRQL_DEPTH[cpu].store((depth + 1) as u8, Ordering::Release);
    }
}
fn pop_irq_state() -> u64 {
    let cpu = cpu_slot();
    let depth = IRQL_DEPTH[cpu].load(Ordering::Acquire) as usize;
    if depth == 0 { return 0; }
    IRQL_DEPTH[cpu].store((depth - 1) as u8, Ordering::Release);
    IRQL_FLAGS[cpu * IRQL_STACK_DEPTH + depth - 1].load(Ordering::Relaxed)
}
#[inline]
fn atomic_lock_word(lock: *mut u64) -> Option<&'static core::sync::atomic::AtomicU64> {
    if lock.is_null() || (lock as usize) % core::mem::align_of::<u64>() != 0 { return None; }
    Some(unsafe { &*(lock.cast::<core::sync::atomic::AtomicU64>()) })
}


/// Windows PROCESSOR_NUMBER layout: group, group-relative number, reserved.
#[repr(C)]
pub struct ProcessorNumber { pub group: u16, pub number: u8, pub reserved: u8 }

/// Minimal x64 DISPATCHER_HEADER prefix shared by KEVENT/KSEMAPHORE.
#[repr(C)]
pub struct DispatcherHeader {
    kind: u8,
    absolute: u8,
    size: u8,
    inserted: u8,
    signal_state: i32,
    wait_list_flink: u64,
    wait_list_blink: u64,
}

const EVENT_NOTIFICATION: u8 = 0;
const EVENT_SYNCHRONIZATION: u8 = 1;
const MUTANT_OBJECT: u8 = 2;
const QUEUE_OBJECT: u8 = 4;
const EVENT_SIZE: u8 = 6; // sizeof(KEVENT) / sizeof(ULONG).

/// IRQL is PASSIVE_LEVEL until the interrupt-priority layer is brought online.
pub unsafe extern "win64" fn ke_get_current_irql() -> u8 { CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire) }

pub unsafe extern "win64" fn ke_query_active_processor_count_ex(group: u16) -> u32 {
    let count = crate::hal::online_cpu_count() as u32;
    if group == 0 || group == ALL_PROCESSOR_GROUPS { count } else { 0 }
}
pub unsafe extern "win64" fn ke_query_active_processor_count() -> u32 {
    unsafe { ke_query_active_processor_count_ex(0) }
}
pub unsafe extern "win64" fn ke_query_active_group_count() -> u16 {
    if crate::hal::online_cpu_count() == 0 { 0 } else { 1 }
}
pub unsafe extern "win64" fn ke_query_maximum_processor_count_ex(group: u16) -> u32 {
    if group == 0 || group == ALL_PROCESSOR_GROUPS { crate::hal::MAX_CPUS as u32 } else { 0 }
}
pub unsafe extern "win64" fn ke_query_maximum_processor_count() -> u32 {
    crate::hal::MAX_CPUS as u32
}
/// ULONG KeGetCurrentProcessorNumberEx(PPROCESSOR_NUMBER ProcNumber).
/// The output pointer is optional; the return is always the systemwide index.
pub unsafe extern "win64" fn ke_get_current_processor_number_ex(out: *mut ProcessorNumber) -> u32 {
    let logical = crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Acquire))
        .unwrap_or(0);
    if !out.is_null() && logical < 64 {
        unsafe { out.write(ProcessorNumber { group: 0, number: logical as u8, reserved: 0 }); }
    }
    logical
}
pub unsafe extern "win64" fn ke_get_current_processor_index() -> u32 {
    crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Acquire))
        .unwrap_or(0)
}
pub unsafe extern "win64" fn ke_get_current_node_number() -> u32 { 0 }
pub unsafe extern "win64" fn ke_get_recommended_shared_data_alignment() -> u32 { 64 }

/// Initialize a Windows x64 KSPIN_LOCK word.
pub unsafe extern "win64" fn ke_initialize_spin_lock(lock: *mut u64) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
}
pub unsafe extern "win64" fn ke_test_spin_lock(lock: *const u64) -> u8 {
    if lock.is_null() || (lock as usize) % core::mem::align_of::<u64>() != 0 { return 0; }
    let word = unsafe { &*(lock.cast::<core::sync::atomic::AtomicU64>()) };
    (word.load(Ordering::Relaxed) == 0) as u8
}

/// Raise to the kernel's conservative DISPATCH_LEVEL model. Until an APIC TPR
/// policy is installed, local interrupts are masked rather than priority-filtered.
pub unsafe extern "win64" fn ke_raise_irql_to_dpc_level() -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(DISPATCH_LEVEL, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(DISPATCH_LEVEL) };
    old
}
pub unsafe extern "win64" fn ke_lower_irql(new_irql: u8) {
    let cpu = cpu_slot();
    CURRENT_IRQL[cpu].store(new_irql, Ordering::Release);
    let _ = unsafe { crate::apic::set_task_priority(new_irql) };
    let flags = pop_irq_state();
    unsafe { irq_restore(flags); }
    if new_irql < DISPATCH_LEVEL { dispatch_queued_dpcs(); }
}

/// KeAcquireSpinLock: disable local interrupts, raise logical IRQL, then acquire
/// atomically so an interrupt handler cannot recursively deadlock this CPU.
pub unsafe extern "win64" fn ke_acquire_spin_lock(lock: *mut u64, old_irql: *mut u8) {
    if old_irql.is_null() { return; }
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(DISPATCH_LEVEL, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(DISPATCH_LEVEL) };
    unsafe { old_irql.write(old); }
    let Some(word) = atomic_lock_word(lock) else {
        CURRENT_IRQL[cpu_slot()].store(old, Ordering::Release);
        let _ = unsafe { crate::apic::set_task_priority(old) };
        let saved = pop_irq_state(); unsafe { irq_restore(saved); }
        return;
    };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
/// Raise to SYNCH_LEVEL. Local interrupts are conservatively masked because
/// this kernel does not yet program APIC task-priority registers.
pub unsafe extern "win64" fn ke_raise_irql_to_synch_level() -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(12, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(12) };
    old
}
/// Acquire a spin lock at SYNCH_LEVEL and return the prior IRQL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_raise_to_synch(lock: *mut u64) -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(12, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(12) };
    let Some(word) = atomic_lock_word(lock) else {
        CURRENT_IRQL[cpu_slot()].store(old, Ordering::Release);
        let saved = pop_irq_state(); unsafe { irq_restore(saved); }
        return old;
    };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    old
}
/// Acquire a spin lock while raising to DISPATCH_LEVEL; returns the prior IRQL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_raise_to_dpc(lock: *mut u64) -> u8 {
    let mut old_irql = PASSIVE_LEVEL;
    unsafe { ke_acquire_spin_lock(lock, &mut old_irql as *mut u8); }
    old_irql
}
pub unsafe extern "win64" fn ke_release_spin_lock(lock: *mut u64, old_irql: u8) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
    unsafe { ke_lower_irql(old_irql); }
}
pub unsafe extern "win64" fn ke_acquire_spin_lock_at_dpc_level(lock: *mut u64) {
    let Some(word) = atomic_lock_word(lock) else { return; };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
pub unsafe extern "win64" fn ke_release_spin_lock_from_dpc_level(lock: *mut u64) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
}
pub unsafe extern "win64" fn ke_try_to_acquire_spin_lock_at_dpc_level(lock: *mut u64) -> u8 {
    match atomic_lock_word(lock) {
        Some(word) => word.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_ok() as u8,
        None => 0,
    }
}
/// Query the architectural monotonic TSC using the Windows x64 LARGE_INTEGER ABI.
/// Frequency is reported in ticks per second and is only advertised when the
/// platform exposes a defensible TSC frequency.
pub unsafe extern "win64" fn ke_query_performance_counter(frequency: *mut i64) -> i64 {
    if let Some(hz) = crate::apic::tsc_frequency_hz() {
        if !frequency.is_null() {
            unsafe { frequency.write(hz.min(i64::MAX as u64) as i64); }
        }
    } else if !frequency.is_null() {
        unsafe { frequency.write(0); }
    }
    crate::apic::monotonic_ticks().min(i64::MAX as u64) as i64
}

/// Serialize prior writes before returning to a caller that requires the
/// legacy KeFlushWriteBuffer ordering guarantee.
pub unsafe extern "win64" fn ke_flush_write_buffer() {
    core::sync::atomic::fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)); }
}

/// Busy-wait for a bounded microsecond interval using the calibrated/stable TSC.
/// If no usable frequency is known, fail closed by returning without pretending
/// that a guessed delay was accurate; HAL import resolution hides this export.
pub unsafe extern "win64" fn ke_stall_execution_processor(microseconds: u32) {
    if microseconds == 0 { return; }
    let Some(hz) = crate::apic::tsc_frequency_hz() else { return; };
    let ticks = ((hz as u128) * (microseconds as u128) + 999_999) / 1_000_000;
    let Ok(ticks) = u64::try_from(ticks) else { return; };
    let start = crate::apic::monotonic_ticks();
    while crate::apic::monotonic_ticks().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
}

/// Initialize notification (type 0) or synchronization (type 1) events.
pub unsafe extern "win64" fn ke_initialize_event(event: *mut DispatcherHeader, event_type: i32, state: u8) {
    if event.is_null() || !(event_type == 0 || event_type == 1) { return; }
    unsafe {
        event.write(DispatcherHeader {
            kind: if event_type == 0 { EVENT_NOTIFICATION } else { EVENT_SYNCHRONIZATION },
            absolute: 0, size: EVENT_SIZE, inserted: 0,
            signal_state: if state != 0 { 1 } else { 0 },
            wait_list_flink: event as u64 + 8,
            wait_list_blink: event as u64 + 8,
        });
    }
}
pub unsafe extern "win64" fn ke_clear_event(event: *mut DispatcherHeader) -> i32 {
    if event.is_null() { return 0; }
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    unsafe { (&*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>()).swap(0, Ordering::AcqRel) }
}
pub unsafe extern "win64" fn ke_reset_event(event: *mut DispatcherHeader) -> i32 {
    unsafe { ke_clear_event(event) }
}
pub unsafe extern "win64" fn ke_read_state_event(event: *const DispatcherHeader) -> i32 {
    if event.is_null() { return 0; }
    unsafe { (&*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire) }
}
pub unsafe extern "win64" fn ke_set_event(event: *mut DispatcherHeader, _increment: i32, _wait: u8) -> i32 {
    if event.is_null() { return 0; }
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    let state = unsafe { &*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>() };
    let old = state.swap(1, Ordering::AcqRel);
    complete_ready_wait_all_groups();
    if unsafe { (*event).kind } == EVENT_SYNCHRONIZATION {
        if state.load(Ordering::Acquire) > 0 {
            if let Some(waiter) = crate::dispatcher::wake_one_timed(event as u64) {
                let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
                let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
                let _ = crate::thread::complete_wait(waiter.thread, status);
                if status == STATUS_SUCCESS {
                    let _ = state.compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire);
                }
            }
        }
    } else {
        let mut waiters = [crate::dispatcher::WokenWaiter { thread: 0, timed_out: false }; crate::dispatcher::MAX_WAITERS];
        let count = crate::dispatcher::wake_all_timed(event as u64, &mut waiters);
        for waiter in waiters[..count].iter().copied() {
            let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
            let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
            let _ = crate::thread::complete_wait(waiter.thread, status);
        }
    }
    old
}

/// Semaphore header followed by the NT limit field.
#[repr(C)]
pub struct SemaphoreObject { header: DispatcherHeader, limit: i32 }
pub unsafe extern "win64" fn ke_initialize_semaphore(semaphore: *mut SemaphoreObject, count: i32, limit: i32) {
    if semaphore.is_null() || count < 0 || limit <= 0 || count > limit { return; }
    unsafe {
        semaphore.write(SemaphoreObject {
            header: DispatcherHeader {
                kind: 5, absolute: 0, size: 8, inserted: 0, signal_state: count,
                wait_list_flink: semaphore as u64 + 8,
                wait_list_blink: semaphore as u64 + 8,
            },
            limit,
        });
    }
}
pub unsafe extern "win64" fn ke_read_state_semaphore(semaphore: *const SemaphoreObject) -> i32 {
    if semaphore.is_null() { return 0; }
    unsafe { (&*core::ptr::addr_of!((*semaphore).header.signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire) }
}
pub unsafe extern "win64" fn ke_release_semaphore(semaphore: *mut SemaphoreObject, increment: i32, adjustment: i32, _wait: u8) -> i32 {
    if semaphore.is_null() || adjustment <= 0 { return 0; }
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    let state = unsafe { &*core::ptr::addr_of!((*semaphore).header.signal_state).cast::<core::sync::atomic::AtomicI32>() };
    let limit = unsafe { (*semaphore).limit };
    let old = state.load(Ordering::Acquire);
    let Some(next) = old.checked_add(adjustment) else { return 0; };
    if next > limit { return 0; }
    state.store(next, Ordering::Release);
    complete_ready_wait_all_groups();
    // Each waiter consumes one available count before becoming runnable.
    loop {
        if state.load(Ordering::Acquire) <= 0 { break; }
        let Some(waiter) = crate::dispatcher::wake_one_timed(semaphore as u64) else { break; };
        let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
        if !waiter.timed_out { state.fetch_sub(1, Ordering::AcqRel); }
        let status = crate::dispatcher::complete_wait_group(waiter.thread, semaphore as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
        if status == STATUS_TIMEOUT && !waiter.timed_out { state.fetch_add(1, Ordering::AcqRel); }
        let _ = crate::thread::complete_wait(waiter.thread, status);
    }
    let _ = increment; // priority boosts require a scheduler priority model.
    old
}


static INTERRUPT_TIME_ORIGIN: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

#[repr(C)]
pub struct GroupAffinity {
    pub mask: u64,
    pub group: u16,
    pub reserved: [u16; 3],
}

fn interrupt_time_100ns() -> u64 {
    let Some(hz) = crate::apic::tsc_frequency_hz().filter(|hz| *hz != 0) else { return 0; };
    let now = crate::apic::monotonic_ticks();
    let origin = match INTERRUPT_TIME_ORIGIN.compare_exchange(0, now.max(1), Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => now.max(1),
        Err(existing) => existing,
    };
    ((now.saturating_sub(origin) as u128) * 10_000_000u128 / hz as u128)
        .min(u64::MAX as u128) as u64
}

/// VOID KeRaiseIrql(KIRQL NewIrql, PKIRQL OldIrql).
pub unsafe extern "win64" fn ke_raise_irql(new_irql: u8, old_irql: *mut u8) {
    if old_irql.is_null() || new_irql > 31 { return; }
    let cpu = cpu_slot();
    let current = CURRENT_IRQL[cpu].load(Ordering::Acquire);
    unsafe { old_irql.write(current); }
    if new_irql < current { return; }
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    CURRENT_IRQL[cpu].store(new_irql, Ordering::Release);
    let _ = unsafe { crate::apic::set_task_priority(new_irql) };
}

/// ULONGLONG KeQueryInterruptTime(void), in 100-nanosecond units.
pub unsafe extern "win64" fn ke_query_interrupt_time() -> u64 { interrupt_time_100ns() }

/// ULONGLONG KeQueryInterruptTimePrecise(PULONG64 QpcTimeStamp).
pub unsafe extern "win64" fn ke_query_interrupt_time_precise(qpc_timestamp: *mut u64) -> u64 {
    let qpc = crate::apic::monotonic_ticks();
    if !qpc_timestamp.is_null() { unsafe { qpc_timestamp.write(qpc); } }
    interrupt_time_100ns()
}

/// ULONGLONG KeQueryUnbiasedInterruptTime(void). Sleep-state bias is not yet modeled.
pub unsafe extern "win64" fn ke_query_unbiased_interrupt_time() -> u64 { interrupt_time_100ns() }

/// VOID KeQueryTickCount(PLARGE_INTEGER TickCount), nominal 1-ms ticks.
pub unsafe extern "win64" fn ke_query_tick_count(tick_count: *mut i64) {
    if !tick_count.is_null() { unsafe { tick_count.write((interrupt_time_100ns() / 10_000) as i64); } }
}

fn rtc_system_time_100ns() -> Option<u64> {
    let mut fields = crate::hal::exports::HalTimeFields::default();
    if unsafe { crate::hal::exports::hal_query_real_time_clock(&mut fields) } == 0 { return None; }
    let year = fields.year as i64;
    let month = fields.month as i64;
    let day = fields.day as i64;
    let hour = fields.hour as i64;
    let minute = fields.minute as i64;
    let second = fields.second as i64;
    let millis = fields.milliseconds as i64;
    if !(1601..=9999).contains(&year) || !(1..=12).contains(&month)
        || !(1..=31).contains(&day) || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute) || !(0..=60).contains(&second)
        || !(0..=999).contains(&millis) { return None; }
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_unix = era * 146097 + doe - 719468;
    let unix_seconds = days_since_unix.checked_mul(86_400)?
        .checked_add(hour * 3600 + minute * 60 + second)?;
    let windows_seconds = unix_seconds.checked_add(11_644_473_600)?;
    u64::try_from(windows_seconds as i128 * 10_000_000 + millis as i128 * 10_000).ok()
}

static SYSTEM_TIME_BASE_STATE: core::sync::atomic::AtomicU8 =
    core::sync::atomic::AtomicU8::new(0); // 0=uninitialized, 1=initializing, 2=ready
static SYSTEM_TIME_BASE_100NS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
static SYSTEM_TIME_BASE_TSC: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Anchor wall-clock time to the first valid RTC sample, then interpolate it
/// with the calibrated monotonic counter. This gives sub-millisecond resolution
/// after initialization, but is not a substitute for OS time-setting, NTP, or
/// suspend/resume correction and inherits the RTC sample's initial accuracy.
fn precise_system_time_100ns() -> Option<u64> {
    let Some(hz) = crate::apic::tsc_frequency_hz().filter(|hz| *hz != 0) else {
        return rtc_system_time_100ns();
    };
    loop {
        match SYSTEM_TIME_BASE_STATE.load(Ordering::Acquire) {
            2 => {
                let base = SYSTEM_TIME_BASE_100NS.load(Ordering::Relaxed);
                let start = SYSTEM_TIME_BASE_TSC.load(Ordering::Relaxed);
                let elapsed = crate::apic::monotonic_ticks().wrapping_sub(start);
                let delta = ((elapsed as u128) * 10_000_000u128 / hz as u128)
                    .min(u64::MAX as u128) as u64;
                return Some(base.saturating_add(delta));
            }
            1 => core::hint::spin_loop(),
            _ => {
                if SYSTEM_TIME_BASE_STATE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_err() {
                    continue;
                }
                let Some(base) = rtc_system_time_100ns() else {
                    SYSTEM_TIME_BASE_STATE.store(0, Ordering::Release);
                    return None;
                };
                let start = crate::apic::monotonic_ticks();
                SYSTEM_TIME_BASE_100NS.store(base, Ordering::Relaxed);
                SYSTEM_TIME_BASE_TSC.store(start, Ordering::Relaxed);
                SYSTEM_TIME_BASE_STATE.store(2, Ordering::Release);
                return Some(base);
            }
        }
    }
}

/// VOID KeQuerySystemTime(PLARGE_INTEGER CurrentTime).
pub unsafe extern "win64" fn ke_query_system_time(current_time: *mut i64) {
    if !current_time.is_null() {
        unsafe { current_time.write(precise_system_time_100ns().unwrap_or(0) as i64); }
    }
}

/// VOID KeQuerySystemTimePrecise(PLARGE_INTEGER CurrentTime).
/// Uses the same bootstrapped wall-clock base with monotonic-counter interpolation.
pub unsafe extern "win64" fn ke_query_system_time_precise(current_time: *mut i64) {
    if !current_time.is_null() {
        unsafe { current_time.write(precise_system_time_100ns().unwrap_or(0) as i64); }
    }
}

/// PKTHREAD KeGetCurrentThread(void); the pointer is an opaque thread-record token.
pub unsafe extern "win64" fn ke_get_current_thread() -> *mut core::ffi::c_void {
    crate::thread::current_thread_object()
}


/// ULONG KeQueryHighestNodeNumber(void). RdotOS currently models NUMA node zero only.
pub unsafe extern "win64" fn ke_query_highest_node_number() -> u32 { 0 }

/// USHORT KeQueryMaximumGroupCount(void). RdotOS currently exposes processor group zero only.
pub unsafe extern "win64" fn ke_query_maximum_group_count() -> u16 { 1 }

/// KAFFINITY KeQueryGroupAffinity(USHORT GroupNumber).
pub unsafe extern "win64" fn ke_query_group_affinity(group: u16) -> u64 {
    if group != 0 { return 0; }
    let mut mask = 0u64;
    for cpu in 0..crate::hal::MAX_CPUS.min(64) {
        if crate::hal::cpu_state(cpu).ok() == Some(crate::hal::CPU_ONLINE) { mask |= 1u64 << cpu; }
    }
    mask
}

/// ULONG KeQueryNodeActiveProcessorCount(USHORT NodeNumber).
pub unsafe extern "win64" fn ke_query_node_active_processor_count(node: u16) -> u32 {
    if node == 0 { crate::hal::online_cpu_count() as u32 } else { 0 }
}

/// USHORT KeQueryNodeMaximumProcessorCount(USHORT NodeNumber).
pub unsafe extern "win64" fn ke_query_node_maximum_processor_count(node: u16) -> u16 {
    if node == 0 { crate::hal::MAX_CPUS as u16 } else { 0 }
}

/// VOID KeQueryNodeActiveAffinity(USHORT, PGROUP_AFFINITY, PUSHORT).
pub unsafe extern "win64" fn ke_query_node_active_affinity(
    node: u16, affinity: *mut GroupAffinity, count: *mut u16,
) {
    if node != 0 {
        if !count.is_null() { unsafe { count.write(0); } }
        return;
    }
    let mask = unsafe { ke_query_group_affinity(0) };
    if !affinity.is_null() {
        unsafe { affinity.write(GroupAffinity { mask, group: 0, reserved: [0; 3] }); }
    }
    if !count.is_null() { unsafe { count.write(mask.count_ones() as u16); } }
}

/// ULONG KeGetProcessorIndexFromNumber(PCPROCESSOR_NUMBER).
pub unsafe extern "win64" fn ke_get_processor_index_from_number(number: *const ProcessorNumber) -> u32 {
    if number.is_null() { return u32::MAX; }
    let n = unsafe { &*number };
    if n.group != 0 || n.number as usize >= crate::hal::MAX_CPUS
        || crate::hal::cpu_state(n.number as usize).ok() != Some(crate::hal::CPU_ONLINE) {
        return u32::MAX;
    }
    n.number as u32
}

/// NTSTATUS KeGetProcessorNumberFromIndex(ULONG, PPROCESSOR_NUMBER).
pub unsafe extern "win64" fn ke_get_processor_number_from_index(
    index: u32, number: *mut ProcessorNumber,
) -> i32 {
    if number.is_null() || index as usize >= crate::hal::MAX_CPUS
        || crate::hal::cpu_state(index as usize).ok() != Some(crate::hal::CPU_ONLINE) {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { number.write(ProcessorNumber { group: 0, number: index as u8, reserved: 0 }); }
    STATUS_SUCCESS
}

// The BSP/bootstrap path can call APC-region helpers before a schedulable
// thread exists. These counters are used only for that no-thread context.
static BOOTSTRAP_CRITICAL_DEPTH:[core::sync::atomic::AtomicU32;64]=[const{core::sync::atomic::AtomicU32::new(0)};64];
static BOOTSTRAP_GUARDED_DEPTH:[core::sync::atomic::AtomicU32;64]=[const{core::sync::atomic::AtomicU32::new(0)};64];
/// Critical and guarded region nesting lives in the current thread record.
/// Bootstrap/no-thread contexts cannot retain per-thread APC state and therefore
/// do not manufacture a process-global substitute.
pub unsafe extern "win64" fn ke_enter_critical_region() {
    if !crate::thread::adjust_apc_region(false, true) { BOOTSTRAP_CRITICAL_DEPTH[cpu_slot()].fetch_add(1,Ordering::AcqRel); }
}
pub unsafe extern "win64" fn ke_leave_critical_region() {
    if !crate::thread::adjust_apc_region(false, false) { let depth=&BOOTSTRAP_CRITICAL_DEPTH[cpu_slot()];let _=depth.fetch_update(Ordering::AcqRel,Ordering::Acquire,|v|Some(v.saturating_sub(1))); }
}
pub unsafe extern "win64" fn ke_enter_guarded_region() {
    if !crate::thread::adjust_apc_region(true, true) { BOOTSTRAP_GUARDED_DEPTH[cpu_slot()].fetch_add(1,Ordering::AcqRel); }
}
pub unsafe extern "win64" fn ke_leave_guarded_region() {
    if !crate::thread::adjust_apc_region(true, false) { let depth=&BOOTSTRAP_GUARDED_DEPTH[cpu_slot()];let _=depth.fetch_update(Ordering::AcqRel,Ordering::Acquire,|v|Some(v.saturating_sub(1))); }
}
pub unsafe extern "win64" fn ke_are_apcs_disabled() -> u8 {
    let (critical, _) = crate::thread::apc_region_depths();
    let critical=if crate::thread::current_thread_id().is_some(){critical as u32}else{BOOTSTRAP_CRITICAL_DEPTH[cpu_slot()].load(Ordering::Acquire)};
    (CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire) >= 1 || critical != 0) as u8
}
pub unsafe extern "win64" fn ke_are_all_apcs_disabled() -> u8 {
    let (critical, guarded) = crate::thread::apc_region_depths();
    let (critical,guarded)=if crate::thread::current_thread_id().is_some(){(critical as u32,guarded as u32)}else{(BOOTSTRAP_CRITICAL_DEPTH[cpu_slot()].load(Ordering::Acquire),BOOTSTRAP_GUARDED_DEPTH[cpu_slot()].load(Ordering::Acquire))};
    (CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire) >= 1 || critical != 0 || guarded != 0) as u8
}
/// BOOLEAN KeAlertThread(PKTHREAD Thread, KPROCESSOR_MODE AlertMode).
/// This implements thread alert state, not kernel/user APC object delivery.
pub unsafe extern "win64" fn ke_alert_thread(thread:*mut core::ffi::c_void,_alert_mode:i32)->u8{
    let Some(id)=crate::thread::thread_id_from_object(thread) else{return 0};
    crate::thread::set_alert_pending(id).unwrap_or(false) as u8
}
/// BOOLEAN KeTestAlertThread(KPROCESSOR_MODE AlertMode).
pub unsafe extern "win64" fn ke_test_alert_thread(_alert_mode:i32)->u8{
    crate::thread::take_alert_pending() as u8
}

/// LONG KePulseEvent(PKEVENT, KPRIORITY, BOOLEAN).
pub unsafe extern "win64" fn ke_pulse_event(
    event: *mut DispatcherHeader, increment: i32, wait: u8,
) -> i32 {
    if event.is_null() { return 0; }
    // Serialize the entire pulse against concurrent state changes.
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    let state = unsafe { &*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>() };
    let previous = state.swap(1, Ordering::AcqRel);
    complete_ready_wait_all_groups();
    match unsafe { (*event).kind } {
        EVENT_NOTIFICATION => {
            let mut waiters = [crate::dispatcher::WokenWaiter { thread: 0, timed_out: false }; crate::dispatcher::MAX_WAITERS];
            let count = crate::dispatcher::wake_all_timed(event as u64, &mut waiters);
            for waiter in waiters[..count].iter().copied() {
                let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
                let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
                let _ = crate::thread::complete_wait(waiter.thread, status);
            }
        }
        EVENT_SYNCHRONIZATION => {
            if let Some(waiter) = crate::dispatcher::wake_one_timed(event as u64) {
                let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
                let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
                let _ = crate::thread::complete_wait(waiter.thread, status);
            }
        }
        _ => {}
    }
    state.store(0, Ordering::Release);
    let _ = (increment, wait); // Priority boosts and atomic Wait handoff remain unsupported.
    previous
}

/// NTSTATUS KeQueryNodeActiveAffinity2(USHORT, PGROUP_AFFINITY, USHORT, PUSHORT).
/// The current topology is one node represented by one affinity group.
pub unsafe extern "win64" fn ke_query_node_active_affinity2(
    node: u16, affinities: *mut GroupAffinity, capacity: u16, required: *mut u16,
) -> i32 {
    if !required.is_null() { unsafe { required.write(if node == 0 { 1 } else { 0 }); } }
    if node != 0 { return STATUS_INVALID_PARAMETER; }
    if capacity < 1 || affinities.is_null() { return 0xC000_0023u32 as i32; }
    unsafe {
        affinities.write(GroupAffinity {
            mask: ke_query_group_affinity(0), group: 0, reserved: [0; 3],
        });
    }
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dispatcher_header_layout_matches_x64_nt() {
        assert_eq!(core::mem::size_of::<DispatcherHeader>(), 24);
        assert_eq!(core::mem::size_of::<SemaphoreObject>(), 32);
    }
    #[test]
    fn event_type_values_match_nt() {
        assert_eq!(EVENT_NOTIFICATION, 0);
        assert_eq!(EVENT_SYNCHRONIZATION, 1);
    }
    #[test]
    fn critical_and_guarded_region_nesting_is_balanced() {
        let cpu = cpu_slot();
        let prior_irql = CURRENT_IRQL[cpu].load(Ordering::Acquire);
        unsafe {
            ke_enter_critical_region();
            assert_eq!(ke_are_apcs_disabled(), 1);
            ke_enter_critical_region();
            ke_leave_critical_region();
            assert_eq!(ke_are_apcs_disabled(), 1);
            ke_leave_critical_region();
            assert_eq!(ke_are_apcs_disabled(), (prior_irql >= 1) as u8);

            ke_enter_guarded_region();
            assert_eq!(ke_are_all_apcs_disabled(), 1);
            ke_enter_guarded_region();
            ke_leave_guarded_region();
            assert_eq!(ke_are_all_apcs_disabled(), 1);
            ke_leave_guarded_region();
            assert_eq!(ke_are_all_apcs_disabled(), (prior_irql >= 1) as u8);
        }
    }

    #[test]
    fn current_processor_number_return_matches_output() {
        let mut number = ProcessorNumber { group: u16::MAX, number: u8::MAX, reserved: u8::MAX };
        let index = unsafe { ke_get_current_processor_number_ex(&mut number) };
        assert_eq!(number.group, 0);
        assert_eq!(number.number as u32, index);
        assert_eq!(number.reserved, 0);
        assert_eq!(unsafe { ke_get_current_processor_number_ex(core::ptr::null_mut()) }, index);
    }

    #[test]
    fn executive_queue_fifo_lifo_and_rundown() {
        let mut queue=core::mem::MaybeUninit::<ExecutiveQueue>::uninit();
        unsafe{ke_initialize_queue(queue.as_mut_ptr(),2);}
        let queue=unsafe{&mut *queue.as_mut_ptr()};
        let mut first=ListEntry{flink:core::ptr::null_mut(),blink:core::ptr::null_mut()};
        let mut second=ListEntry{flink:core::ptr::null_mut(),blink:core::ptr::null_mut()};
        assert_eq!(unsafe{ke_insert_queue(queue,&mut first)},0);
        assert_eq!(unsafe{ke_insert_head_queue(queue,&mut second)},1);
        assert_eq!(queue.header.signal_state,2);
        let no_wait=0i64;
        assert!(core::ptr::eq(unsafe{ke_remove_queue(queue,0,&no_wait)},&mut second));
        assert!(core::ptr::eq(unsafe{ke_remove_queue(queue,0,&no_wait)},&mut first));
        assert_eq!(queue.header.signal_state,0);
        assert!(unsafe{ke_rundown_queue(queue)}.is_null());
    }

    static DPC_TEST_COUNT:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
    unsafe extern "win64" fn dpc_test_routine(_: *mut core::ffi::c_void,_: *mut core::ffi::c_void,_: *mut core::ffi::c_void,_: *mut core::ffi::c_void){
        DPC_TEST_COUNT.fetch_add(1,Ordering::AcqRel);
    }
    #[test]
    fn dpc_queue_rejects_duplicates_and_runs_once(){
        let mut object=0u64;
        let dpc=(&mut object as *mut u64).cast::<core::ffi::c_void>();
        DPC_TEST_COUNT.store(0,Ordering::Release);
        let cpu=cpu_slot();let prior=CURRENT_IRQL[cpu].swap(DISPATCH_LEVEL,Ordering::AcqRel);
        unsafe{ke_initialize_dpc(dpc,Some(dpc_test_routine),core::ptr::null_mut());}
        assert_eq!(unsafe{ke_insert_queue_dpc(dpc,core::ptr::null_mut(),core::ptr::null_mut())},1);
        assert_eq!(unsafe{ke_insert_queue_dpc(dpc,core::ptr::null_mut(),core::ptr::null_mut())},0);
        CURRENT_IRQL[cpu].store(prior,Ordering::Release);
        unsafe{ke_flush_queued_dpcs();}
        assert_eq!(DPC_TEST_COUNT.load(Ordering::Acquire),1);
        assert_eq!(unsafe{ke_remove_queue_dpc(dpc)},0);
    }

    #[test]
    fn timer_object_expires_and_signals(){
        let mut storage=core::mem::MaybeUninit::<DispatcherHeader>::uninit();
        let timer=storage.as_mut_ptr();
        unsafe{ke_initialize_timer(timer);}
        assert_eq!(unsafe{ke_read_state_timer(timer)},0);
        assert_eq!(unsafe{ke_set_timer(timer,0,core::ptr::null_mut())},0);
        timer_tick(crate::apic::monotonic_ticks());
        assert_eq!(unsafe{ke_read_state_timer(timer)},1);
        assert_eq!(unsafe{ke_cancel_timer(timer)},0);
    }

    #[test]
    fn mutant_initial_state_is_signalled(){
        let mut storage=core::mem::MaybeUninit::<DispatcherHeader>::uninit();
        let mutant=storage.as_mut_ptr();
        unsafe{ke_initialize_mutant(mutant,0);}
        assert_eq!(unsafe{ke_read_state_mutant(mutant)},1);
    }
}


const STATUS_SUCCESS: i32 = 0;
const STATUS_ALERTED: i32 = 0x0000_0101;
const STATUS_TIMEOUT: i32 = 0x0000_0102;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002_u32 as i32;

/// Consume a signalled event/semaphore state using an atomic compare/exchange.
unsafe fn try_consume_dispatcher_object_locked(object: *mut DispatcherHeader) -> bool {
    if object.is_null() { return false; }
    let state = unsafe { &*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>() };
    match unsafe { (*object).kind } {
        EVENT_NOTIFICATION | TIMER_NOTIFICATION => state.load(Ordering::Acquire) > 0,
        MUTANT_OBJECT => try_acquire_mutant(object),
        EVENT_SYNCHRONIZATION | 5 | TIMER_SYNCHRONIZATION | QUEUE_OBJECT => {
            let old = state.load(Ordering::Acquire);
            if old <= 0 { return false; }
            state.store(old - 1, Ordering::Release);
            true
        }
        _ => false,
    }
}
unsafe fn try_consume_dispatcher_object(object: *mut DispatcherHeader) -> bool {
    let _guard = DISPATCHER_OBJECT_LOCK.lock();
    unsafe { try_consume_dispatcher_object_locked(object) }
}

fn wait_all_object_signalled(object_id:u64)->bool{
    let object=object_id as *mut DispatcherHeader;
    if object.is_null(){return false}
    let kind=unsafe{(*object).kind};
    if !matches!(kind,EVENT_NOTIFICATION|EVENT_SYNCHRONIZATION|5|TIMER_NOTIFICATION|TIMER_SYNCHRONIZATION){return false}
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)>0}
}
fn consume_wait_all_object(object_id:u64){
    let object=object_id as *mut DispatcherHeader;
    if object.is_null()||matches!(unsafe{(*object).kind},EVENT_NOTIFICATION|TIMER_NOTIFICATION){return}
    let state=unsafe{&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()};
    state.store(state.load(Ordering::Acquire)-1,Ordering::Release);
}
fn complete_ready_wait_all_groups(){
    let mut ids=[0u64;crate::dispatcher::MAX_WAIT_GROUPS];
    let count=crate::dispatcher::complete_ready_wait_all(&mut ids,wait_all_object_signalled,consume_wait_all_object);
    for id in ids[..count].iter().copied(){let _=crate::thread::complete_wait(id,STATUS_SUCCESS);}
}

/// Block the current RdotOS kernel thread on a dispatcher object. This requires
/// a thread created by the kernel thread runtime; unsupported caller contexts
/// return STATUS_NOT_IMPLEMENTED instead of pretending that a wait occurred.
pub unsafe extern "win64" fn ke_wait_for_single_object(
    object: *mut DispatcherHeader,
    _wait_reason: i32,
    _wait_mode: i32,
    alertable: u8,
    timeout: *const i64,
) -> i32 {
    if object.is_null() { return STATUS_INVALID_PARAMETER; }
    if !matches!(unsafe { (*object).kind }, EVENT_NOTIFICATION | EVENT_SYNCHRONIZATION | 5 | TIMER_NOTIFICATION | TIMER_SYNCHRONIZATION | QUEUE_OBJECT | MUTANT_OBJECT) {
        return STATUS_NOT_IMPLEMENTED;
    }
    let Some(thread_id) = crate::thread::current_thread_id() else { return STATUS_NOT_IMPLEMENTED; };
    if alertable != 0 && crate::thread::take_alert_pending() { return STATUS_ALERTED; }
    if unsafe{(*object).kind}==MUTANT_OBJECT{
        if let Some(status)=try_acquire_mutant_status(object){return status}
    }else if unsafe { try_consume_dispatcher_object(object) } { return STATUS_SUCCESS; }
    let deadline = if timeout.is_null() { 0 } else {
        let interval = unsafe { timeout.read_unaligned() };
        if interval == 0 { return STATUS_TIMEOUT; }
        if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
        match timer_deadline(interval) {
            Some(value) => value,
            None => return STATUS_NOT_IMPLEMENTED,
        }
    };
    let _ = crate::thread::set_wait_alertable(alertable != 0);
    let token = match crate::dispatcher::enqueue(object as u64, thread_id, deadline) {
        Ok(value) => value,
        Err(_) => { let _=crate::thread::set_wait_alertable(false); return STATUS_NOT_IMPLEMENTED; },
    };
    if alertable != 0 && crate::thread::take_alert_pending() {
        let _=crate::dispatcher::cancel(token);let _=crate::thread::set_wait_alertable(false);return STATUS_ALERTED;
    }
    let acquired=if unsafe{(*object).kind}==MUTANT_OBJECT{
        try_acquire_mutant_status(object)
    }else if unsafe{try_consume_dispatcher_object(object)}{Some(STATUS_SUCCESS)}else{None};
    if let Some(status)=acquired{
        let _ = crate::dispatcher::cancel(token);
        let _ = crate::thread::clear_wake_pending();
        let _=crate::thread::set_wait_alertable(false);
        return status;
    }
    if crate::thread::block_current().is_err() {
        let _ = crate::dispatcher::cancel(token);
        let _=crate::thread::set_wait_alertable(false);
        return STATUS_NOT_IMPLEMENTED;
    }
    let status=crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
    let _=crate::thread::set_wait_alertable(false);
    status
}

/// Multiple-object waits are rejected explicitly until shared wait-block groups
/// can provide atomic wait-all reservation and safe cancellation of wait-any peers.
pub unsafe extern "win64" fn ke_wait_for_multiple_objects(
    count: u32,
    objects: *const *mut DispatcherHeader,
    wait_type: i32,
    wait_reason: i32,
    wait_mode: i32,
    alertable: u8,
    timeout: *const i64,
    _wait_block_array: *mut core::ffi::c_void,
) -> i32 {
    if count == 0 || count > 64 || objects.is_null() || !(wait_type == 0 || wait_type == 1) {
        return STATUS_INVALID_PARAMETER;
    }
    let _ = (wait_reason, wait_mode);
    if alertable != 0 && crate::thread::take_alert_pending() { return STATUS_ALERTED; }
    let object_list = unsafe { core::slice::from_raw_parts(objects, count as usize) };
    if object_list.iter().any(|object| object.is_null()) { return STATUS_INVALID_PARAMETER; }
    for i in 0..object_list.len() {
        let kind = unsafe { (*object_list[i]).kind };
        if !matches!(kind, EVENT_NOTIFICATION | EVENT_SYNCHRONIZATION | 5 | TIMER_NOTIFICATION | TIMER_SYNCHRONIZATION) {
            return STATUS_NOT_IMPLEMENTED;
        }
        // Mutants need an abandoned-status result and queues need entry transfer.
        // Keep both out of the generic multi-wait path until their per-object
        // reservations can be committed atomically for wait-any and wait-all.
        if matches!(kind, MUTANT_OBJECT | QUEUE_OBJECT) {
            return STATUS_NOT_IMPLEMENTED;
        }
        if object_list[..i].iter().any(|previous| *previous == object_list[i]) {
            return STATUS_INVALID_PARAMETER;
        }
    }
    let current_id = crate::thread::current_thread_id();
    let guard = DISPATCHER_OBJECT_LOCK.lock();
    if wait_type == 1 {
        for (index, object) in object_list.iter().copied().enumerate() {
            if unsafe { try_consume_dispatcher_object_locked(object) } {
                return index as i32;
            }
        }
    } else {
        let all_signalled = object_list.iter().copied().all(|object| {
            unsafe { (&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire) > 0 }
        });
        if all_signalled {
            for object in object_list.iter().copied() { consume_wait_all_object(object as u64); }
            return STATUS_SUCCESS;
        }
    }
    if !timeout.is_null() && unsafe { timeout.read_unaligned() } == 0 { return STATUS_TIMEOUT; }
    let Some(thread_id) = current_id else { return STATUS_NOT_IMPLEMENTED; };
    let deadline = if timeout.is_null() { 0 } else {
        let interval = unsafe { timeout.read_unaligned() };
        if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
        match timer_deadline(interval) {
            Some(value) => value,
            None => return STATUS_NOT_IMPLEMENTED,
        }
    };
    let mut object_ids = [0u64; 64];
    for (index, object) in object_list.iter().copied().enumerate() { object_ids[index] = object as u64; }

    if wait_type == 0 {
        let timeout_token = if deadline != 0 {
            match crate::dispatcher::enqueue(crate::dispatcher::wait_all_timeout_object(thread_id), thread_id, deadline) {
                Ok(token) => Some(token),
                Err(_) => return STATUS_NOT_IMPLEMENTED,
            }
        } else { None };
        let _=crate::thread::set_wait_alertable(alertable!=0);
        if crate::dispatcher::register_wait_all(thread_id, &object_ids[..count as usize], timeout_token, deadline).is_err() {
            if let Some(token) = timeout_token { let _ = crate::dispatcher::cancel(token); }
            let _=crate::thread::set_wait_alertable(false);return STATUS_NOT_IMPLEMENTED;
        }
        drop(guard);
        if alertable!=0 && crate::thread::take_alert_pending(){
            crate::dispatcher::clear_wait_group(thread_id);if let Some(token)=timeout_token{let _=crate::dispatcher::cancel(token);}
            let _=crate::thread::set_wait_alertable(false);return STATUS_ALERTED;
        }
        if crate::thread::block_current().is_err() {
            crate::dispatcher::clear_wait_group(thread_id);
            if let Some(token) = timeout_token { let _ = crate::dispatcher::cancel(token); }
            let _=crate::thread::set_wait_alertable(false);return STATUS_NOT_IMPLEMENTED;
        }
        let status=crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
        let _=crate::thread::set_wait_alertable(false);
        return status;
    }

    let mut tokens = [0u64; 64];
    let mut registered = 0usize;
    for object in object_ids[..count as usize].iter().copied() {
        match crate::dispatcher::enqueue(object, thread_id, deadline) {
            Ok(token) => { tokens[registered] = token; registered += 1; }
            Err(_) => {
                for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
                return STATUS_NOT_IMPLEMENTED;
            }
        }
    }
    let _=crate::thread::set_wait_alertable(alertable!=0);
    if crate::dispatcher::register_wait_any(thread_id, &object_ids[..count as usize], &tokens[..registered], deadline).is_err() {
        for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
        let _=crate::thread::set_wait_alertable(false);return STATUS_NOT_IMPLEMENTED;
    }
    drop(guard);
    if alertable!=0 && crate::thread::take_alert_pending(){
        crate::dispatcher::clear_wait_group(thread_id);for token in tokens[..registered].iter().copied(){let _=crate::dispatcher::cancel(token);}
        let _=crate::thread::set_wait_alertable(false);return STATUS_ALERTED;
    }
    if crate::thread::block_current().is_err() {
        crate::dispatcher::clear_wait_group(thread_id);
        for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
        let _=crate::thread::set_wait_alertable(false);return STATUS_NOT_IMPLEMENTED;
    }
    let status=crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
    let _=crate::thread::set_wait_alertable(false);
    status
}




/// Processor-group affinity is limited to group 0 until multi-group topology
/// exists. The cooperative scheduler refuses masks that would migrate a live
/// context away from its owning CPU.
pub unsafe extern "win64" fn ke_set_affinity_thread(thread:*mut core::ffi::c_void,affinity:u64)->u64{
 let Some(id)=priority_id(thread) else{return 0};
 crate::thread::set_thread_affinity(id,affinity).unwrap_or_else(||crate::thread::thread_affinity(id).unwrap_or(0))
}
const MAX_AFFINITY_RESTORE:usize=128;
#[derive(Clone,Copy)]struct AffinityRestore{id:u64,mask:u64}
impl AffinityRestore{const EMPTY:Self=Self{id:0,mask:0};}
struct AffinityRestoreTable{records:[AffinityRestore;MAX_AFFINITY_RESTORE]}
impl AffinityRestoreTable{const fn new()->Self{Self{records:[AffinityRestore::EMPTY;MAX_AFFINITY_RESTORE]}}}
static AFFINITY_RESTORE:crate::hal::IrqSpinLock<AffinityRestoreTable>=crate::hal::IrqSpinLock::new(AffinityRestoreTable::new());
pub unsafe extern "win64" fn ke_set_system_group_affinity_thread(thread:*mut core::ffi::c_void,new_affinity:*const GroupAffinity,previous_affinity:*mut GroupAffinity){
 if new_affinity.is_null(){return}
 let Some(id)=priority_id(thread) else{return};
 let requested=unsafe{new_affinity.read_unaligned()};
 if requested.group!=0||requested.mask==0{return}
 let Some(old)=crate::thread::set_thread_affinity(id,requested.mask) else{return};
 if !previous_affinity.is_null(){unsafe{previous_affinity.write_unaligned(GroupAffinity{mask:old,group:0,reserved:[0;3]});}}
 let mut table=AFFINITY_RESTORE.lock();
 let slot=table.records.iter().position(|r|r.id==id).or_else(||table.records.iter().position(|r|r.id==0));
 if let Some(slot)=slot{table.records[slot]=AffinityRestore{id,mask:old};}
}
pub unsafe extern "win64" fn ke_revert_to_user_group_affinity_thread(thread:*mut core::ffi::c_void){
 let Some(id)=priority_id(thread) else{return};
 let old={let mut table=AFFINITY_RESTORE.lock();let Some(slot)=table.records.iter().position(|r|r.id==id) else{return};let mask=table.records[slot].mask;table.records[slot]=AffinityRestore::EMPTY;mask};
 if old!=0{let _=crate::thread::set_thread_affinity(id,old);}
}

/// Interrupt-object synchronization side table. Until HAL interrupt objects expose
/// their real SynchronizeIrql and ISR lock, this uses a conservative DPC-level
/// critical section; it serializes RdotOS callers but is not a Windows DIRQL clone.
const MAX_INTERRUPT_LOCKS:usize=128;
struct InterruptLockSlot{object:core::sync::atomic::AtomicUsize,word:core::sync::atomic::AtomicU64}
impl InterruptLockSlot{const fn new()->Self{Self{object:core::sync::atomic::AtomicUsize::new(0),word:core::sync::atomic::AtomicU64::new(0)}}}
static INTERRUPT_LOCKS:[InterruptLockSlot;MAX_INTERRUPT_LOCKS]=[const{InterruptLockSlot::new()};MAX_INTERRUPT_LOCKS];
static INTERRUPT_LOCK_REGISTRY:crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
fn interrupt_lock_slot(interrupt:*mut core::ffi::c_void)->Option<usize>{
 if interrupt.is_null(){return None}
 let _guard=INTERRUPT_LOCK_REGISTRY.lock();
 if let Some(i)=INTERRUPT_LOCKS.iter().position(|s|s.object.load(Ordering::Acquire)==interrupt as usize){return Some(i)}
 let i=INTERRUPT_LOCKS.iter().position(|s|s.object.load(Ordering::Acquire)==0)?;
 INTERRUPT_LOCKS[i].object.store(interrupt as usize,Ordering::Release);Some(i)
}
fn acquire_interrupt_lock(interrupt:*mut core::ffi::c_void)->Option<u8>{
 let i=interrupt_lock_slot(interrupt)?;let old=unsafe{ke_raise_irql_to_dpc_level()};let word=&INTERRUPT_LOCKS[i].word;
 while word.compare_exchange_weak(0,1,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}
 Some(old)
}
pub unsafe extern "win64" fn ke_acquire_interrupt_spin_lock(interrupt:*mut core::ffi::c_void)->u8{
 acquire_interrupt_lock(interrupt).unwrap_or_else(||unsafe{ke_get_current_irql()})
}
pub unsafe extern "win64" fn ke_release_interrupt_spin_lock(interrupt:*mut core::ffi::c_void,old_irql:u8){
 if let Some(i)=interrupt_lock_slot(interrupt){INTERRUPT_LOCKS[i].word.store(0,Ordering::Release);unsafe{ke_lower_irql(old_irql);}}
}
pub type SynchronizeRoutine=unsafe extern "win64" fn(*mut core::ffi::c_void)->u8;
pub unsafe extern "win64" fn ke_synchronize_execution(interrupt:*mut core::ffi::c_void,routine:Option<SynchronizeRoutine>,context:*mut core::ffi::c_void)->u8{
 let Some(routine)=routine else{return 0};
 let Some(old)=acquire_interrupt_lock(interrupt) else{return 0};
 let result=unsafe{routine(context)};
 unsafe{ke_release_interrupt_spin_lock(interrupt,old);}
 result
}

/// Precise monotonic time; suspend/resume bias is not modelled yet.
pub unsafe extern "win64" fn ke_query_unbiased_interrupt_time_precise(qpc_timestamp:*mut u64)->u64{
 if !qpc_timestamp.is_null(){unsafe{qpc_timestamp.write(crate::apic::monotonic_ticks());}}
 interrupt_time_100ns()
}
/// Clock resolution in 100-ns units. Until dynamic clock programming is wired,
/// all three values report the configured 1-ms quantum.
pub unsafe extern "win64" fn ke_get_clock_timer_resolution(maximum:*mut u32,minimum:*mut u32,current:*mut u32){
 for p in [maximum,minimum,current]{if !p.is_null(){unsafe{p.write(10_000);}}}
}
pub unsafe extern "win64" fn ke_get_next_clock_tick_duration()->u32{
 let hz=crate::apic::tsc_frequency_hz().unwrap_or(0);if hz==0{return 0}
 let now=crate::apic::monotonic_ticks();
 let earliest={let table=TIMERS.lock();table.records.iter().filter(|r|r.active).map(|r|r.due).min()};
 let Some(deadline)=earliest else{return 10_000};
 ((deadline.saturating_sub(now) as u128*10_000_000u128/hz as u128).min(u32::MAX as u128)) as u32
}
/// x86-64 coherent-DMA fallback: enforce ordering; non-coherent cache maintenance
/// requires a platform DMA coherency operation and is not claimed here.
pub unsafe extern "win64" fn ke_flush_io_buffers(_mdl:*mut core::ffi::c_void,read_operation:u8,_dma_operation:u8){
 if read_operation!=0{core::sync::atomic::fence(Ordering::SeqCst);}
 unsafe{core::arch::asm!("mfence",options(nostack,preserves_flags));}
}
static BUGCHECK_CODE:core::sync::atomic::AtomicU32=core::sync::atomic::AtomicU32::new(0);
static BUGCHECK_PARAMETERS:[core::sync::atomic::AtomicU64;4]=[const{core::sync::atomic::AtomicU64::new(0)};4];
pub unsafe extern "win64" fn ke_bug_check(code:u32)->!{unsafe{ke_bug_check_ex(code,0,0,0,0)}}
pub unsafe extern "win64" fn ke_bug_check_ex(code:u32,p1:usize,p2:usize,p3:usize,p4:usize)->!{
 BUGCHECK_PARAMETERS[0].store(p1 as u64,Ordering::Release);BUGCHECK_PARAMETERS[1].store(p2 as u64,Ordering::Release);
 BUGCHECK_PARAMETERS[2].store(p3 as u64,Ordering::Release);BUGCHECK_PARAMETERS[3].store(p4 as u64,Ordering::Release);
 BUGCHECK_CODE.store(code,Ordering::Release);
 loop{unsafe{core::arch::asm!("cli","hlt",options(nomem,nostack));}}
}


/// Limited scheduler priority metadata, keyed by opaque RdotOS thread tokens.
/// Valid Windows priorities are 0..31; quantum boosts and scheduler reordering
/// are not yet represented by the cooperative scheduler.
const MAX_THREAD_PRIORITIES:usize=128;
#[derive(Clone,Copy)] struct ThreadPriorityRecord{id:u64,base:i8,current:i8}
impl ThreadPriorityRecord{const EMPTY:Self=Self{id:0,base:8,current:8};}
struct ThreadPriorityTable{records:[ThreadPriorityRecord;MAX_THREAD_PRIORITIES]}
impl ThreadPriorityTable{const fn new()->Self{Self{records:[ThreadPriorityRecord::EMPTY;MAX_THREAD_PRIORITIES]}}}
static THREAD_PRIORITIES:crate::hal::IrqSpinLock<ThreadPriorityTable>=crate::hal::IrqSpinLock::new(ThreadPriorityTable::new());
fn priority_id(thread:*mut core::ffi::c_void)->Option<u64>{if thread.is_null(){crate::thread::current_thread_id()}else{crate::thread::thread_id_from_object(thread)}}
pub unsafe extern "win64" fn ke_query_priority_thread(thread:*mut core::ffi::c_void)->i8{
 let Some(id)=priority_id(thread) else{return 8};let table=THREAD_PRIORITIES.lock();
 table.records.iter().find(|r|r.id==id).map(|r|r.current).unwrap_or(8)
}
pub unsafe extern "win64" fn ke_set_priority_thread(thread:*mut core::ffi::c_void,priority:i8)->i8{
 let Some(id)=priority_id(thread) else{return 8};if !(0..=31).contains(&priority){return unsafe{ke_query_priority_thread(thread)}}
 let mut table=THREAD_PRIORITIES.lock();
 let Some(slot)=table.records.iter().position(|r|r.id==id).or_else(||table.records.iter().position(|r|r.id==0)) else{return 8};
 let old=if table.records[slot].id==id{table.records[slot].current}else{8};
 let base=if table.records[slot].id==id{table.records[slot].base}else{priority};
 table.records[slot]=ThreadPriorityRecord{id,base,current:priority};old
}
pub unsafe extern "win64" fn ke_set_base_priority_thread(thread:*mut core::ffi::c_void,increment:i32)->i32{
 let Some(id)=priority_id(thread) else{return 0};
 let mut table=THREAD_PRIORITIES.lock();
 let Some(slot)=table.records.iter().position(|r|r.id==id) else{return 0};
 let old=table.records[slot].base as i32;
 let base=(old+increment).clamp(0,31) as i8;
 table.records[slot].base=base;table.records[slot].current=base;
 old
}

/// Never busy-loops when the thread runtime or calibrated delay source is absent.
pub unsafe extern "win64" fn ke_delay_execution_thread(
    wait_mode: i32,
    alertable: u8,
    interval: *const i64,
) -> i32 {
    if interval.is_null() { return STATUS_INVALID_PARAMETER; }
    let value = unsafe { interval.read_unaligned() };
    if value == 0 { return STATUS_SUCCESS; }
    let Some(thread_id) = crate::thread::current_thread_id() else { return STATUS_NOT_IMPLEMENTED; };
    if alertable != 0 && crate::thread::take_alert_pending() { return STATUS_ALERTED; }
    if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
    let deadline = match timer_deadline(value) {
        Some(value) => value,
        None => return STATUS_NOT_IMPLEMENTED,
    };
    let _ = wait_mode;
    let _ = crate::thread::set_wait_alertable(alertable != 0);
    let token = match crate::dispatcher::enqueue(crate::dispatcher::delay_object(thread_id), thread_id, deadline) {
        Ok(value) => value,
        Err(_) => { let _=crate::thread::set_wait_alertable(false); return STATUS_NOT_IMPLEMENTED; },
    };
    if alertable != 0 && crate::thread::take_alert_pending() {
        let _=crate::dispatcher::cancel(token);let _=crate::thread::set_wait_alertable(false);return STATUS_ALERTED;
    }
    if crate::thread::block_current().is_err() {
        let _ = crate::dispatcher::cancel(token);
        let _=crate::thread::set_wait_alertable(false);
        return STATUS_NOT_IMPLEMENTED;
    }
    let status=crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
    let _=crate::thread::set_wait_alertable(false);
    status
}

pub unsafe extern "win64" fn ke_initialize_guarded_mutex(object:*mut DispatcherHeader){unsafe{initialize_mutant(object,0)}}
pub unsafe extern "win64" fn ke_acquire_guarded_mutex(object:*mut DispatcherHeader){
    unsafe{ke_enter_guarded_region();}
    let status=unsafe{ke_wait_for_single_object(object,0,0,0,core::ptr::null())};
    if status!=STATUS_SUCCESS{unsafe{ke_leave_guarded_region();}}
}
pub unsafe extern "win64" fn ke_release_guarded_mutex(object:*mut DispatcherHeader){
    let _=release_mutant(object,false);unsafe{ke_leave_guarded_region();}
}
pub unsafe extern "win64" fn ke_try_to_acquire_guarded_mutex(object:*mut DispatcherHeader)->u8{
    unsafe{ke_enter_guarded_region();}
    if unsafe{try_consume_dispatcher_object(object)}{1}else{unsafe{ke_leave_guarded_region();}0}
}
pub unsafe extern "win64" fn ke_acquire_guarded_mutex_unsafe(object:*mut DispatcherHeader){
    let _=unsafe{ke_wait_for_single_object(object,0,0,0,core::ptr::null())};
}
pub unsafe extern "win64" fn ke_release_guarded_mutex_unsafe(object:*mut DispatcherHeader){
    let _=release_mutant(object,false);
}


/// Public x64 KDEVICE_QUEUE / KDEVICE_QUEUE_ENTRY prefix. Queue manipulation is
/// serialized by an IRQ-safe lock; the caller-supplied lock is retained for ABI.
#[repr(C)]
pub struct DeviceQueueEntry{pub entry_type:i16,pub size:i16,pub device_list_entry:ListEntry,pub sort_key:u32,pub inserted:u8,pub reserved:[u8;3]}
#[repr(C)]
pub struct DeviceQueue{pub queue_type:i16,pub size:i16,pub device_list_head:ListEntry,pub lock:u64,pub busy:u8,pub reserved:[u8;3],pub hint:*mut DeviceQueueEntry}
static DEVICE_QUEUE_LOCK:crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
pub unsafe extern "win64" fn ke_initialize_device_queue(queue:*mut DeviceQueue){
 if queue.is_null(){return}
 unsafe{
  (*queue).queue_type=0x15;(*queue).size=core::mem::size_of::<DeviceQueue>() as i16;
  list_initialize(core::ptr::addr_of_mut!((*queue).device_list_head));
  (*queue).lock=0;(*queue).busy=0;(*queue).reserved=[0;3];(*queue).hint=core::ptr::null_mut();
 }
}
unsafe fn device_queue_insert(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry,key:u32,sorted:bool)->u8{
 if queue.is_null()||entry.is_null(){return 0}
 let _guard=DEVICE_QUEUE_LOCK.lock();
 if unsafe{(*queue).busy==0}{unsafe{(*queue).busy=1;(*entry).inserted=0;}return 0}
 let head=unsafe{core::ptr::addr_of_mut!((*queue).device_list_head)};
 unsafe{
  (*entry).sort_key=key;
  if sorted{
   let mut cursor=(*head).flink;
   while cursor!=head{
    let queued=(cursor as *mut u8).sub(8).cast::<DeviceQueueEntry>();
    if (*queued).sort_key>key{break} cursor=(*cursor).flink;
   }
   let prev=(*cursor).blink;let link=core::ptr::addr_of_mut!((*entry).device_list_entry);
   (*link).flink=cursor;(*link).blink=prev;(*prev).flink=link;(*cursor).blink=link;
  }else{list_insert_tail(head,core::ptr::addr_of_mut!((*entry).device_list_entry));}
  (*entry).inserted=1;
 }
 1
}
pub unsafe extern "win64" fn ke_insert_device_queue(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry)->u8{unsafe{device_queue_insert(queue,entry,0,false)}}
pub unsafe extern "win64" fn ke_insert_by_key_device_queue(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry,key:u32)->u8{unsafe{device_queue_insert(queue,entry,key,true)}}
unsafe fn device_queue_remove_entry(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry)->bool{
 if queue.is_null()||entry.is_null(){return false}
 let _guard=DEVICE_QUEUE_LOCK.lock();
 if unsafe{(*entry).inserted}==0{return false}
 let link=unsafe{core::ptr::addr_of_mut!((*entry).device_list_entry)};
 unsafe{let prev=(*link).blink;let next=(*link).flink;(*prev).flink=next;(*next).blink=prev;(*link).flink=link;(*link).blink=link;(*entry).inserted=0;}
 true
}
pub unsafe extern "win64" fn ke_remove_entry_device_queue(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry)->u8{
 (unsafe{device_queue_remove_entry(queue,entry)}) as u8
}
pub unsafe extern "win64" fn ke_remove_by_key_device_queue(queue:*mut DeviceQueue,key:u32)->*mut DeviceQueueEntry{
 if queue.is_null(){return core::ptr::null_mut()}
 let _guard=DEVICE_QUEUE_LOCK.lock();
 let head=unsafe{core::ptr::addr_of_mut!((*queue).device_list_head)};
 let mut link=unsafe{(*head).flink};
 while link!=head{
  let entry=unsafe{(link as *mut u8).sub(8).cast::<DeviceQueueEntry>()};
  if unsafe{(*entry).sort_key}==key{
   unsafe{let prev=(*link).blink;let next=(*link).flink;(*prev).flink=next;(*next).blink=prev;(*link).flink=link;(*link).blink=link;(*entry).inserted=0;(*queue).hint=entry;}
   return entry
  }
  link=unsafe{(*link).flink};
 }
 core::ptr::null_mut()
}
pub unsafe extern "win64" fn ke_remove_by_key_device_queue_if_busy(queue:*mut DeviceQueue,key:u32)->*mut DeviceQueueEntry{
 if queue.is_null()||unsafe{(*queue).busy}==0{return core::ptr::null_mut()}
 unsafe{ke_remove_by_key_device_queue(queue,key)}
}
pub unsafe extern "win64" fn ke_remove_device_queue(queue:*mut DeviceQueue)->*mut DeviceQueueEntry{
 if queue.is_null(){return core::ptr::null_mut()}
 let _guard=DEVICE_QUEUE_LOCK.lock();let head=unsafe{core::ptr::addr_of_mut!((*queue).device_list_head)};
 let link=unsafe{list_remove_head(head)};
 if link.is_null(){unsafe{(*queue).busy=0;(*queue).hint=core::ptr::null_mut();}return core::ptr::null_mut()}
 let entry=unsafe{(link as *mut u8).sub(8).cast::<DeviceQueueEntry>()};
 unsafe{(*entry).inserted=0;(*queue).hint=entry;}entry
}

// ---- Executive KQUEUE --------------------------------------------------------
#[repr(C)]
pub struct ListEntry{pub flink:*mut ListEntry,pub blink:*mut ListEntry}
#[repr(C)]
pub struct ExecutiveQueue{
    pub header:DispatcherHeader,
    pub entry_list_head:ListEntry,
    pub current_count:u32,
    pub maximum_count:u32,
    pub thread_list_head:ListEntry,
}
unsafe fn list_initialize(head:*mut ListEntry){unsafe{(*head).flink=head;(*head).blink=head;}}
unsafe fn list_insert_tail(head:*mut ListEntry,entry:*mut ListEntry){
    unsafe{let tail=(*head).blink;(*entry).flink=head;(*entry).blink=tail;(*tail).flink=entry;(*head).blink=entry;}
}
unsafe fn list_insert_head(head:*mut ListEntry,entry:*mut ListEntry){
    unsafe{let first=(*head).flink;(*entry).flink=first;(*entry).blink=head;(*first).blink=entry;(*head).flink=entry;}
}
unsafe fn list_remove_head(head:*mut ListEntry)->*mut ListEntry{
    unsafe{let first=(*head).flink;if first==head{return core::ptr::null_mut()}let next=(*first).flink;(*head).flink=next;(*next).blink=head;(*first).flink=first;(*first).blink=first;first}
}
#[derive(Clone,Copy)]
struct QueueFlag{object:usize,rundown:bool}
impl QueueFlag{const EMPTY:Self=Self{object:0,rundown:false};}
struct QueueFlagTable{records:[QueueFlag;128]}
impl QueueFlagTable{const fn new()->Self{Self{records:[QueueFlag::EMPTY;128]}}}
static QUEUE_FLAGS:crate::hal::IrqSpinLock<QueueFlagTable>=crate::hal::IrqSpinLock::new(QueueFlagTable::new());
fn queue_rundown_state(object:usize,set:Option<bool>)->bool{
    let mut table=QUEUE_FLAGS.lock();
    let index=table.records.iter().position(|r|r.object==object).or_else(||table.records.iter().position(|r|r.object==0));
    let Some(i)=index else{return false};
    if table.records[i].object==0{table.records[i].object=object;}
    if let Some(value)=set{table.records[i].rundown=value;}
    table.records[i].rundown
}
pub unsafe extern "win64" fn ke_initialize_queue(queue:*mut ExecutiveQueue,count:u32){
    if queue.is_null(){return}
    let max=if count==0{crate::hal::online_cpu_count().max(1) as u32}else{count};
    let _=queue_rundown_state(queue as usize,Some(false));
    unsafe{
        core::ptr::addr_of_mut!((*queue).header).write(DispatcherHeader{kind:QUEUE_OBJECT,absolute:0,size:(core::mem::size_of::<ExecutiveQueue>()/4) as u8,inserted:0,signal_state:0,wait_list_flink:queue as u64+8,wait_list_blink:queue as u64+8});
        list_initialize(core::ptr::addr_of_mut!((*queue).entry_list_head));
        (*queue).current_count=0;(*queue).maximum_count=max;
        list_initialize(core::ptr::addr_of_mut!((*queue).thread_list_head));
    }
}
unsafe fn queue_insert(queue:*mut ExecutiveQueue,entry:*mut ListEntry,at_head:bool)->i32{
    if queue.is_null()||entry.is_null(){return 0}
    if queue_rundown_state(queue as usize,None){return STATUS_ABANDONED_WAIT_0}
    let _guard=DISPATCHER_OBJECT_LOCK.lock();
    let state=unsafe{&*core::ptr::addr_of!((*queue).header.signal_state).cast::<core::sync::atomic::AtomicI32>()};
    let previous=state.load(Ordering::Acquire);
    loop{
        let Some(waiter)=crate::dispatcher::wake_one_timed(queue as u64) else{break};
        if waiter.timed_out{
            let status=crate::dispatcher::complete_wait_group(waiter.thread,queue as u64,STATUS_TIMEOUT,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_TIMEOUT);
            let _=crate::thread::complete_wait(waiter.thread,status);continue
        }
        let _=crate::thread::set_queue_entry(waiter.thread,entry as u64);
        let status=crate::dispatcher::complete_wait_group(waiter.thread,queue as u64,STATUS_SUCCESS,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_SUCCESS);
        let _=crate::thread::complete_wait(waiter.thread,status);
        return previous
    }
    unsafe{if at_head{list_insert_head(core::ptr::addr_of_mut!((*queue).entry_list_head),entry)}else{list_insert_tail(core::ptr::addr_of_mut!((*queue).entry_list_head),entry)}}
    state.fetch_add(1,Ordering::AcqRel);previous
}
pub unsafe extern "win64" fn ke_insert_queue(queue:*mut ExecutiveQueue,entry:*mut ListEntry)->i32{unsafe{queue_insert(queue,entry,false)}}
pub unsafe extern "win64" fn ke_insert_head_queue(queue:*mut ExecutiveQueue,entry:*mut ListEntry)->i32{unsafe{queue_insert(queue,entry,true)}}
pub unsafe extern "win64" fn ke_remove_queue(queue:*mut ExecutiveQueue,wait_mode:i32,timeout:*const i64)->*mut ListEntry{
    if queue.is_null(){return STATUS_INVALID_PARAMETER as isize as *mut ListEntry}
    if queue_rundown_state(queue as usize,None){return STATUS_ABANDONED_WAIT_0 as isize as *mut ListEntry}
    let popped={
        let _guard=DISPATCHER_OBJECT_LOCK.lock();
        let state=unsafe{&*core::ptr::addr_of!((*queue).header.signal_state).cast::<core::sync::atomic::AtomicI32>()};
        let entry=unsafe{list_remove_head(core::ptr::addr_of_mut!((*queue).entry_list_head))};
        if !entry.is_null(){state.fetch_sub(1,Ordering::AcqRel);}entry
    };
    if !popped.is_null(){return popped}
    if !timeout.is_null()&&unsafe{timeout.read_unaligned()}==0{return STATUS_TIMEOUT as isize as *mut ListEntry}
    let status=unsafe{ke_wait_for_single_object(core::ptr::addr_of_mut!((*queue).header),0,wait_mode,0,timeout)};
    if status!=STATUS_SUCCESS{return status as isize as *mut ListEntry}
    if let Some(entry)=crate::thread::take_queue_entry(){return entry as *mut ListEntry}
    let _guard=DISPATCHER_OBJECT_LOCK.lock();
    let entry=unsafe{list_remove_head(core::ptr::addr_of_mut!((*queue).entry_list_head))};
    if entry.is_null(){STATUS_TIMEOUT as isize as *mut ListEntry}else{entry}
}
pub unsafe extern "win64" fn ke_rundown_queue(queue:*mut ExecutiveQueue)->*mut ListEntry{
    if queue.is_null(){return core::ptr::null_mut()}
    let _guard=DISPATCHER_OBJECT_LOCK.lock();
    let _=queue_rundown_state(queue as usize,Some(true));
    let head=unsafe{core::ptr::addr_of_mut!((*queue).entry_list_head)};
    let first=unsafe{(*head).flink};
    unsafe{
        (&*core::ptr::addr_of!((*queue).header.signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);
        if first==head{list_initialize(head)}else{
            let tail=(*head).blink;(*tail).flink=first;(*first).blink=tail;
            (*head).flink=head;(*head).blink=head;
        }
    }
    let mut waiters=[crate::dispatcher::WokenWaiter{thread:0,timed_out:false};crate::dispatcher::MAX_WAITERS];
    let count=crate::dispatcher::wake_all_timed(queue as u64,&mut waiters);
    for waiter in waiters[..count].iter().copied(){
        let status=crate::dispatcher::complete_wait_group(waiter.thread,queue as u64,STATUS_ABANDONED_WAIT_0,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_ABANDONED_WAIT_0);
        let _=crate::thread::complete_wait(waiter.thread,status);
    }
    if first==head{core::ptr::null_mut()}else{first}
}

// ---- Queued spin locks -------------------------------------------------------
const MAX_QUEUED_LOCKS:usize=128;
struct TicketLockSlot{key:core::sync::atomic::AtomicUsize,next:core::sync::atomic::AtomicU32,owner:core::sync::atomic::AtomicU32}
impl TicketLockSlot{const fn new()->Self{Self{key:core::sync::atomic::AtomicUsize::new(0),next:core::sync::atomic::AtomicU32::new(0),owner:core::sync::atomic::AtomicU32::new(0)}}}
static QUEUED_LOCKS:[TicketLockSlot;MAX_QUEUED_LOCKS]=[const{TicketLockSlot::new()};MAX_QUEUED_LOCKS];
static QUEUED_LOCK_REGISTRY:crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
#[repr(C)]
pub struct LockQueueHandle{pub entry_next:u64,pub entry_lock:u64,pub lock_queue:*mut u64,pub old_irql:u8,pub reserved:[u8;7]}
fn ticket_slot(key:usize)->Option<usize>{
    if key==0{return None}
    let _guard=QUEUED_LOCK_REGISTRY.lock();
    if let Some(i)=QUEUED_LOCKS.iter().position(|s|s.key.load(Ordering::Acquire)==key){return Some(i)}
    let i=QUEUED_LOCKS.iter().position(|s|s.key.load(Ordering::Acquire)==0)?;
    QUEUED_LOCKS[i].key.store(key,Ordering::Release);Some(i)
}
fn acquire_ticket(key:usize)->Option<usize>{
    let index=ticket_slot(key)?;
    let slot=&QUEUED_LOCKS[index];
    let ticket=slot.next.fetch_add(1,Ordering::AcqRel);
    while slot.owner.load(Ordering::Acquire)!=ticket{core::hint::spin_loop();}
    Some(index)
}
fn try_acquire_ticket(key:usize)->Option<usize>{
    let index=ticket_slot(key)?;
    let slot=&QUEUED_LOCKS[index];
    loop{
        let owner=slot.owner.load(Ordering::Acquire);
        let next=slot.next.load(Ordering::Acquire);
        if next!=owner{return None}
        if slot.next.compare_exchange_weak(next,next.wrapping_add(1),Ordering::AcqRel,Ordering::Acquire).is_ok(){return Some(index)}
    }
}
fn release_ticket(index:usize){
    if index<MAX_QUEUED_LOCKS{QUEUED_LOCKS[index].owner.fetch_add(1,Ordering::Release);}
}
unsafe fn acquire_in_stack(lock:*mut u64,handle:*mut LockQueueHandle,raise:bool){
    if lock.is_null()||handle.is_null()||(lock as usize)%8!=0{return}
    let old=if raise{unsafe{ke_raise_irql_to_dpc_level()}}else{unsafe{ke_get_current_irql()}};
    let Some(index)=acquire_ticket(lock as usize) else{if raise{unsafe{ke_lower_irql(old)}}return};
    if let Some(word)=atomic_lock_word(lock){
        while word.compare_exchange_weak(0,1,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}
    }else{release_ticket(index);if raise{unsafe{ke_lower_irql(old)}}return}
    unsafe{handle.write(LockQueueHandle{entry_next:0,entry_lock:lock as u64,lock_queue:lock,old_irql:old,reserved:[0;7]});}
}
unsafe fn release_in_stack(handle:*mut LockQueueHandle,lower:bool){
    if handle.is_null(){return}
    let lock=unsafe{(*handle).lock_queue};
    if lock.is_null(){return}
    let old=unsafe{(*handle).old_irql};
    if let Some(word)=atomic_lock_word(lock){word.store(0,Ordering::Release);}
    if let Some(index)=ticket_slot(lock as usize){release_ticket(index);}
    unsafe{(*handle).lock_queue=core::ptr::null_mut();}
    if lower{unsafe{ke_lower_irql(old)}}
}
pub unsafe extern "win64" fn ke_acquire_in_stack_queued_spin_lock_raise_to_synch(lock:*mut u64,handle:*mut LockQueueHandle){
    if lock.is_null()||handle.is_null()||(lock as usize)%8!=0{return}
    let old=unsafe{ke_raise_irql_to_synch_level()};
    let Some(index)=acquire_ticket(lock as usize) else{unsafe{ke_lower_irql(old)};return};
    if let Some(word)=atomic_lock_word(lock){
        while word.compare_exchange_weak(0,1,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}
    }else{release_ticket(index);unsafe{ke_lower_irql(old)};return}
    unsafe{handle.write(LockQueueHandle{entry_next:0,entry_lock:lock as u64,lock_queue:lock,old_irql:old,reserved:[0;7]});}
}
pub unsafe extern "win64" fn ke_acquire_in_stack_queued_spin_lock(lock:*mut u64,handle:*mut LockQueueHandle){unsafe{acquire_in_stack(lock,handle,true)}}
pub unsafe extern "win64" fn ke_release_in_stack_queued_spin_lock(handle:*mut LockQueueHandle){unsafe{release_in_stack(handle,true)}}
pub unsafe extern "win64" fn ke_acquire_in_stack_queued_spin_lock_at_dpc_level(lock:*mut u64,handle:*mut LockQueueHandle){unsafe{acquire_in_stack(lock,handle,false)}}
pub unsafe extern "win64" fn ke_release_in_stack_queued_spin_lock_from_dpc_level(handle:*mut LockQueueHandle){unsafe{release_in_stack(handle,false)}}
pub unsafe extern "win64" fn ke_acquire_in_stack_queued_spin_lock_for_dpc(lock:*mut u64,handle:*mut LockQueueHandle){unsafe{acquire_in_stack(lock,handle,true)}}
pub unsafe extern "win64" fn ke_release_in_stack_queued_spin_lock_for_dpc(handle:*mut LockQueueHandle){unsafe{release_in_stack(handle,true)}}
/// Threaded-DPC spin lock: raise only when the callback entered below DISPATCH_LEVEL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_for_dpc(lock:*mut u64)->u8{
    unsafe{ke_acquire_spin_lock_raise_to_dpc(lock)}
}
pub unsafe extern "win64" fn ke_release_spin_lock_for_dpc(lock:*mut u64,old_irql:u8){
    unsafe{ke_release_spin_lock(lock,old_irql)}
}
pub unsafe extern "win64" fn ke_try_to_acquire_queued_spin_lock(queue:u32,old_irql:*mut u8)->u8{
    if old_irql.is_null(){return 0}
    let old=unsafe{ke_raise_irql_to_dpc_level()};
    if try_acquire_ticket(queue_number_key(queue)).is_some(){
        unsafe{old_irql.write(old)};1
    }else{unsafe{ke_lower_irql(old)};0}
}
pub unsafe extern "win64" fn ke_try_to_acquire_queued_spin_lock_raise_to_synch(queue:u32,old_irql:*mut u8)->u8{
    if old_irql.is_null(){return 0}
    let old=unsafe{ke_raise_irql_to_synch_level()};
    if try_acquire_ticket(queue_number_key(queue)).is_some(){
        unsafe{old_irql.write(old)};1
    }else{unsafe{ke_lower_irql(old)};0}
}
fn queue_number_key(queue:u32)->usize{0x8000_0000usize.wrapping_add(queue as usize+1)}
pub unsafe extern "win64" fn ke_acquire_queued_spin_lock(queue:u32)->u8{
    let old=unsafe{ke_raise_irql_to_dpc_level()};
    if acquire_ticket(queue_number_key(queue)).is_none(){unsafe{ke_lower_irql(old)};}
    old
}
pub unsafe extern "win64" fn ke_release_queued_spin_lock(queue:u32,old_irql:u8){
    if let Some(index)=ticket_slot(queue_number_key(queue)){release_ticket(index);}
    unsafe{ke_lower_irql(old_irql);}
}
pub unsafe extern "win64" fn ke_acquire_queued_spin_lock_raise_to_synch(queue:u32)->u8{
    let old=unsafe{ke_raise_irql_to_synch_level()};
    if acquire_ticket(queue_number_key(queue)).is_none(){unsafe{ke_lower_irql(old)};}
    old
}
pub unsafe extern "win64" fn ke_release_queued_spin_lock_from_synch_level(queue:u32,old_irql:u8){
    if let Some(index)=ticket_slot(queue_number_key(queue)){release_ticket(index);}
    unsafe{ke_lower_irql(old_irql);}
}

/// LONG KeReadStateQueue(PRKQUEUE Queue).
pub unsafe extern "win64" fn ke_read_state_queue(queue:*const ExecutiveQueue)->i32{
    if queue.is_null(){return 0}
    unsafe{(&*core::ptr::addr_of!((*queue).header.signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)}
}

// ---- Mutant/mutex dispatcher ownership --------------------------------------
const STATUS_ABANDONED_WAIT_0: i32 = 0x0000_0080;
#[derive(Clone,Copy)]
struct MutantRecord{object:usize,owner:u64,recursion:u32,abandoned:bool}
impl MutantRecord{const EMPTY:Self=Self{object:0,owner:0,recursion:0,abandoned:false};}
struct MutantTable{records:[MutantRecord;128]}
impl MutantTable{const fn new()->Self{Self{records:[MutantRecord::EMPTY;128]}}}
static MUTANTS:crate::hal::IrqSpinLock<MutantTable>=crate::hal::IrqSpinLock::new(MutantTable::new());
unsafe fn initialize_mutant(object:*mut DispatcherHeader,initial_owner:u8){
    if object.is_null(){return}
    let current=crate::thread::current_thread_id().unwrap_or(0);
    let owns=initial_owner!=0&&current!=0;
    unsafe{object.write(DispatcherHeader{kind:MUTANT_OBJECT,absolute:0,size:14,inserted:0,signal_state:if owns{0}else{1},wait_list_flink:object as u64+8,wait_list_blink:object as u64+8});}
    let mut table=MUTANTS.lock();
    let slot=table.records.iter().position(|r|r.object==object as usize).or_else(||table.records.iter().position(|r|r.object==0));
    if let Some(i)=slot{table.records[i]=MutantRecord{object:object as usize,owner:if owns{current}else{0},recursion:if owns{1}else{0},abandoned:false};}
}
pub unsafe extern "win64" fn ke_initialize_mutant(object:*mut DispatcherHeader,initial_owner:u8){unsafe{initialize_mutant(object,initial_owner)}}
pub unsafe extern "win64" fn ke_initialize_mutex(object:*mut DispatcherHeader,_level:u32){unsafe{initialize_mutant(object,0)}}
fn try_acquire_mutant_status(object:*mut DispatcherHeader)->Option<i32>{
    if object.is_null(){return None}
    let thread=crate::thread::current_thread_id()?;
    let mut table=MUTANTS.lock();
    let record=table.records.iter_mut().find(|r|r.object==object as usize)?;
    if record.owner==thread{
        record.recursion=record.recursion.saturating_add(1);
        unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1-(record.recursion as i32),Ordering::Release);}
        return Some(STATUS_SUCCESS)
    }
    if record.owner!=0{return None}
    record.owner=thread;record.recursion=1;
    let abandoned=record.abandoned;record.abandoned=false;
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
    Some(if abandoned{STATUS_ABANDONED_WAIT_0}else{STATUS_SUCCESS})
}
fn try_acquire_mutant(object:*mut DispatcherHeader)->bool{try_acquire_mutant_status(object).is_some()}
fn release_mutant(object:*mut DispatcherHeader,abandoned:bool)->i32{
    if object.is_null(){return 0}
    let Some(current)=crate::thread::current_thread_id() else{return 0};
    let _object_guard=DISPATCHER_OBJECT_LOCK.lock();
    let mut timed_out=[0u64;crate::dispatcher::MAX_WAITERS];let mut timed_count=0usize;
    let (old,waiter_id,final_status)={
        let mut table=MUTANTS.lock();
        let Some(record)=table.records.iter_mut().find(|r|r.object==object as usize) else{return 0};
        if record.owner!=current||record.recursion==0{return unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)}}
        let old=1-record.recursion as i32;
        if record.recursion>1{
            record.recursion-=1;
            unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1-record.recursion as i32,Ordering::Release);}
            return old;
        }
        record.owner=0;record.recursion=0;record.abandoned=abandoned;
        unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1,Ordering::Release);}
        loop{
            let Some(waiter)=crate::dispatcher::wake_one_timed(object as u64) else{break (old,None,STATUS_SUCCESS)};
            if waiter.timed_out{
                if timed_count<timed_out.len(){timed_out[timed_count]=waiter.thread;timed_count+=1;}
                continue
            }
            record.owner=waiter.thread;record.recursion=1;
            let was_abandoned=record.abandoned;record.abandoned=false;
            unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
            break (old,Some(waiter.thread),if was_abandoned{STATUS_ABANDONED_WAIT_0}else{STATUS_SUCCESS})
        }
    };
    for id in timed_out[..timed_count].iter().copied(){let status=crate::dispatcher::complete_wait_group(id,object as u64,STATUS_TIMEOUT,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_TIMEOUT);let _=crate::thread::complete_wait(id,status);}
    if let Some(id)=waiter_id{
        let status=crate::dispatcher::complete_wait_group(id,object as u64,final_status,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(final_status);
        let _=crate::thread::complete_wait(id,status);
    }
    old
}
pub unsafe extern "win64" fn ke_release_mutex(object:*mut DispatcherHeader,wait:u8)->i32{
    let old=release_mutant(object,false);let _=wait;old
}
pub unsafe extern "win64" fn ke_release_mutant(object:*mut DispatcherHeader,_increment:i32,abandoned:u8,_wait:u8)->i32{
    release_mutant(object,abandoned!=0)
}
pub unsafe extern "win64" fn ke_read_state_mutex(object:*const DispatcherHeader)->i32{
    if object.is_null(){return 0}
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)}
}
pub unsafe extern "win64" fn ke_read_state_mutant(object:*const DispatcherHeader)->i32{
    unsafe{ke_read_state_mutex(object)}
}
/// Mark mutexes owned by a terminating thread abandoned and wake their next waiter.
pub fn thread_exit_cleanup(thread:u64){
    if thread==0{return}
    let mut objects=[0usize;128];let mut count=0;
    {
        let mut table=MUTANTS.lock();
        for record in table.records.iter_mut(){
            if record.object!=0&&record.owner==thread{
                record.owner=0;record.recursion=0;record.abandoned=true;
                objects[count]=record.object;count+=1;
            }
        }
    }
    for object in objects[..count].iter().copied(){
        let header=object as *mut DispatcherHeader;
        let _guard=DISPATCHER_OBJECT_LOCK.lock();
        unsafe{(&*core::ptr::addr_of!((*header).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1,Ordering::Release);}
        let mut timed_out=[0u64;crate::dispatcher::MAX_WAITERS];let mut timed_count=0usize;
        let mut owner_waiter=None;
        loop{
            let Some(waiter)=crate::dispatcher::wake_one_timed(object as u64) else{break};
            if waiter.timed_out{
                if timed_count<timed_out.len(){timed_out[timed_count]=waiter.thread;timed_count+=1;}
                continue
            }
            owner_waiter=Some(waiter.thread);break
        }
        for id in timed_out[..timed_count].iter().copied(){let status=crate::dispatcher::complete_wait_group(id,object as u64,STATUS_TIMEOUT,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_TIMEOUT);let _=crate::thread::complete_wait(id,status);}
        if let Some(id)=owner_waiter{
            let mut table=MUTANTS.lock();
            if let Some(record)=table.records.iter_mut().find(|r|r.object==object){record.owner=id;record.recursion=1;record.abandoned=false;}
            unsafe{(&*core::ptr::addr_of!((*header).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
            let status=crate::dispatcher::complete_wait_group(id,object as u64,STATUS_ABANDONED_WAIT_0,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_ABANDONED_WAIT_0);
            let _=crate::thread::complete_wait(id,status);
        }
    }
}


// ---- Windows x64 extended affinity bitmap (current topology: group zero) ----
// KAFFINITY_EX starts with USHORT Count, USHORT Size, ULONG Reserved and a
// KAFFINITY bitmap. RdotOS currently exposes one processor group, so these
// routines deliberately operate on the first bitmap word and never touch
// higher group words supplied by a Windows driver.
#[repr(C)]
pub struct AffinityEx {
    pub count:u16,
    pub size:u16,
    pub reserved:u32,
    pub bitmap:[u64;1],
}
unsafe fn affinity_word(ptr:*mut AffinityEx)->Option<&'static core::sync::atomic::AtomicU64>{
    if ptr.is_null() || (ptr as usize)%8!=0{return None}
    Some(unsafe{&*core::ptr::addr_of!((*ptr).bitmap[0]).cast::<core::sync::atomic::AtomicU64>()})
}
unsafe fn affinity_mask(ptr:*const AffinityEx)->u64{
    if ptr.is_null(){return 0}
    unsafe{(*ptr).bitmap[0]}
}
pub unsafe extern "win64" fn ke_initialize_affinity_ex(a:*mut AffinityEx){
    if a.is_null(){return}
    unsafe{(*a).count=1;(*a).size=1;(*a).reserved=0;(*a).bitmap[0]=0;}
}
pub unsafe extern "win64" fn ke_add_processor_affinity_ex(a:*mut AffinityEx,processor:u32){
    if processor>=64{return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_or(1u64<<processor,Ordering::AcqRel);}
    if !a.is_null(){unsafe{(*a).count=1;}}
}
pub unsafe extern "win64" fn ke_remove_processor_affinity_ex(a:*mut AffinityEx,processor:u32){
    if processor>=64{return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_and(!(1u64<<processor),Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_and_affinity_ex(a:*mut AffinityEx,b:*const AffinityEx){
    if a.is_null()||b.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_and(unsafe{affinity_mask(b)},Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_or_affinity_ex(a:*mut AffinityEx,b:*const AffinityEx){
    if a.is_null()||b.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_or(unsafe{affinity_mask(b)},Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_subtract_affinity_ex(a:*mut AffinityEx,b:*const AffinityEx){
    if a.is_null()||b.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_and(!unsafe{affinity_mask(b)},Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_complement_affinity_ex(a:*mut AffinityEx){
    if a.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{
        let valid=if crate::hal::online_cpu_count()>=64{u64::MAX}else{(1u64<<crate::hal::online_cpu_count().max(1))-1};
        w.fetch_xor(valid,Ordering::AcqRel);
    }
}
pub unsafe extern "win64" fn ke_copy_affinity_ex(dst:*mut AffinityEx,src:*const AffinityEx){
    if dst.is_null()||src.is_null()||dst==src as *mut AffinityEx{return}
    let value=unsafe{affinity_mask(src)};
    if let Some(w)=unsafe{affinity_word(dst)}{w.store(value,Ordering::Release);}
    unsafe{(*dst).count=1;(*dst).size=1;(*dst).reserved=0;}
}
pub unsafe extern "win64" fn ke_count_set_bits_affinity_ex(a:*const AffinityEx)->u32{unsafe{affinity_mask(a).count_ones()}}
pub unsafe extern "win64" fn ke_find_first_set_right_affinity_ex(a:*const AffinityEx)->u32{
    let bits=unsafe{affinity_mask(a)};if bits==0{u32::MAX}else{bits.trailing_zeros()}
}
pub unsafe extern "win64" fn ke_find_first_set_left_affinity_ex(a:*const AffinityEx)->u32{
    let bits=unsafe{affinity_mask(a)};if bits==0{u32::MAX}else{63-bits.leading_zeros()}
}
pub unsafe extern "win64" fn ke_check_processor_affinity_ex(a:*const AffinityEx,processor:u32)->u8{
    (processor<64&&(unsafe{affinity_mask(a)}&(1u64<<processor))!=0) as u8
}
pub unsafe extern "win64" fn ke_is_empty_affinity_ex(a:*const AffinityEx)->u8{(unsafe{affinity_mask(a)}==0) as u8}
pub unsafe extern "win64" fn ke_is_equal_affinity_ex(a:*const AffinityEx,b:*const AffinityEx)->u8{
    (unsafe{affinity_mask(a)}==affinity_mask(b)) as u8
}
pub unsafe extern "win64" fn ke_is_single_group_affinity_ex(_a:*const AffinityEx)->u8{1}
pub unsafe extern "win64" fn ke_is_subset_affinity_ex(a:*const AffinityEx,b:*const AffinityEx)->u8{
    let av=unsafe{affinity_mask(a)};let bv=unsafe{affinity_mask(b)};(av&!bv==0) as u8
}
pub unsafe extern "win64" fn ke_interlocked_set_processor_affinity_ex(a:*mut AffinityEx,processor:u32){
    unsafe{ke_add_processor_affinity_ex(a,processor)}
}
pub unsafe extern "win64" fn ke_interlocked_clear_processor_affinity_ex(a:*mut AffinityEx,processor:u32){
    unsafe{ke_remove_processor_affinity_ex(a,processor)}
}
pub unsafe extern "win64" fn ke_add_group_affinity_ex(a:*mut AffinityEx,group:u16,mask:u64){
    if group!=0||a.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_or(mask,Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_remove_group_affinity_ex(a:*mut AffinityEx,group:u16,mask:u64){
    if group!=0||a.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_and(!mask,Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_and_group_affinity_ex(a:*mut AffinityEx,group:u16,mask:u64){
    if group!=0||a.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_and(mask,Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_query_group_affinity_ex(a:*const AffinityEx,group:u16)->u64{
    if group!=0{0}else{unsafe{affinity_mask(a)}}
}
pub unsafe extern "win64" fn ke_xor_affinity_ex(a:*mut AffinityEx,b:*const AffinityEx){
    if a.is_null()||b.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_xor(unsafe{affinity_mask(b)},Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_get_xsave_feature_flags()->u64{
    let max=unsafe{core::arch::x86_64::__cpuid(0).eax};
    if max<0xD{return 0}
    let leaf1=unsafe{core::arch::x86_64::__cpuid(1)};
    if leaf1.ecx&(1<<26)==0||leaf1.ecx&(1<<27)==0{return 0}
    let lo:u32;let hi:u32;
    unsafe{core::arch::asm!("xgetbv",in("ecx")0u32,out("eax")lo,out("edx")hi,options(nomem,nostack));}
    ((hi as u64)<<32)|lo as u64
}
pub unsafe extern "win64" fn ke_dynamic_partitioning_supported()->u8{0}
pub unsafe extern "win64" fn ke_should_yield_processor()->u8{
    crate::thread::has_ready_threads() as u8
}

// ---- Timer and DPC executive ------------------------------------------------
// Driver-owned timer/DPC object storage is opaque to this implementation. The
// side tables track only lifecycle metadata, avoiding assumptions about private
// Windows LIST_ENTRY/KDPC internals while preserving the public DDI signatures.
const MAX_TIMER_OBJECTS: usize = 128;
const MAX_DPC_OBJECTS: usize = 128;
const TIMER_NOTIFICATION: u8 = 8;
const TIMER_SYNCHRONIZATION: u8 = 9;
pub type DpcRoutine = unsafe extern "win64" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void);
#[derive(Clone, Copy)]
struct TimerRecord { object: usize, active: bool, due: u64, period: u64, dpc: usize }
impl TimerRecord { const EMPTY: Self = Self { object:0, active:false, due:0, period:0, dpc:0 }; }
struct TimerTable { records: [TimerRecord; MAX_TIMER_OBJECTS] }
impl TimerTable { const fn new()->Self { Self { records:[TimerRecord::EMPTY;MAX_TIMER_OBJECTS] } } }
static TIMERS: crate::hal::IrqSpinLock<TimerTable> = crate::hal::IrqSpinLock::new(TimerTable::new());
#[derive(Clone, Copy)]
struct DpcRecord { object: usize, routine: usize, context: usize, arg1: usize, arg2: usize, target_cpu: usize, importance:u8, threaded: bool, active: bool, queued: bool }
impl DpcRecord { const EMPTY: Self = Self { object:0,routine:0,context:0,arg1:0,arg2:0,target_cpu:0,importance:1,threaded:false,active:false,queued:false }; }
struct DpcTable { records:[DpcRecord;MAX_DPC_OBJECTS] }
impl DpcTable { const fn new()->Self { Self { records:[DpcRecord::EMPTY;MAX_DPC_OBJECTS] } } }
static DPCS: crate::hal::IrqSpinLock<DpcTable> = crate::hal::IrqSpinLock::new(DpcTable::new());
static DPC_DISPATCHING: [core::sync::atomic::AtomicBool; 64] =
    [const { core::sync::atomic::AtomicBool::new(false) };64];

fn timer_slot(object:usize)->Option<usize>{
    let table=TIMERS.lock();
    table.records.iter().position(|r|r.object==object)
}
fn dpc_slot(object:usize)->Option<usize>{
    let table=DPCS.lock();
    table.records.iter().position(|r|r.active&&r.object==object)
}
fn timer_deadline(due:i64)->Option<u64>{
    if due==0{return Some(crate::apic::monotonic_ticks())}
    if due<0{return crate::apic::relative_timeout_deadline(due)}
    let now=precise_system_time_100ns()? as i64;
    let delta=(due-now).max(0) as u64;
    let hz=crate::apic::tsc_frequency_hz()?;
    let ticks=((delta as u128).checked_mul(hz as u128)?/10_000_000u128).min(u64::MAX as u128) as u64;
    crate::apic::monotonic_ticks().checked_add(ticks)
}
unsafe fn initialize_timer(timer:*mut DispatcherHeader, timer_type:i32){
    if timer.is_null()||!(timer_type==0||timer_type==1){return}
    let kind=if timer_type==0{TIMER_NOTIFICATION}else{TIMER_SYNCHRONIZATION};
    unsafe{timer.write(DispatcherHeader{kind,absolute:0,size:16,inserted:0,signal_state:0,wait_list_flink:timer as u64+8,wait_list_blink:timer as u64+8});}
    let mut table=TIMERS.lock();
    let slot=table.records.iter().position(|r|r.object==timer as usize)
        .or_else(||table.records.iter().position(|r|r.object==0));
    if let Some(i)=slot{table.records[i]=TimerRecord{object:timer as usize,..TimerRecord::EMPTY};}
}
pub unsafe extern "win64" fn ke_initialize_timer(timer:*mut DispatcherHeader){
    unsafe{initialize_timer(timer,0)}
}
pub unsafe extern "win64" fn ke_initialize_timer_ex(timer:*mut DispatcherHeader,timer_type:i32){
    unsafe{initialize_timer(timer,timer_type)}
}
pub unsafe extern "win64" fn ke_read_state_timer(timer:*const DispatcherHeader)->i32{
    if timer.is_null(){return 0}
    unsafe{(&*core::ptr::addr_of!((*timer).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)}
}
/// BOOLEAN KeSetTimer(PKTIMER Timer, LARGE_INTEGER DueTime, PKDPC Dpc).
/// LARGE_INTEGER is passed by value in the x64 Windows ABI.
pub unsafe extern "win64" fn ke_set_timer(timer:*mut DispatcherHeader,due_time:i64,dpc:*mut core::ffi::c_void)->u8{
    unsafe{ke_set_timer_ex(timer,due_time,0,dpc)}
}
/// BOOLEAN KeSetTimerEx(PKTIMER Timer, LARGE_INTEGER DueTime, LONG Period, PKDPC Dpc).
pub unsafe extern "win64" fn ke_set_timer_ex(timer:*mut DispatcherHeader,due_time:i64,period_ms:i32,dpc:*mut core::ffi::c_void)->u8{
    if timer.is_null()||!matches!(unsafe{(*timer).kind},TIMER_NOTIFICATION|TIMER_SYNCHRONIZATION){return 0}
    let value=due_time;
    let Some(deadline)=timer_deadline(value) else{return 0};
    let hz=crate::apic::tsc_frequency_hz().unwrap_or(0);
    let period=if period_ms<=0||hz==0{0}else{((hz as u128)*(period_ms as u32 as u128)/1000).min(u64::MAX as u128) as u64};
    let (was_active,old_dpc)={
        let mut table=TIMERS.lock();
        let Some(record)=table.records.iter_mut().find(|r|r.object==timer as usize) else{return 0};
        let old=(record.active,record.dpc);
        record.active=true;record.due=deadline;record.period=period;record.dpc=dpc as usize;
        old
    };
    if was_active&&old_dpc!=0{let _=unsafe{ke_remove_queue_dpc(old_dpc as *mut core::ffi::c_void)};}
    unsafe{(&*core::ptr::addr_of!((*timer).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
    was_active as u8
}
pub unsafe extern "win64" fn ke_cancel_timer(timer:*mut DispatcherHeader)->u8{
    if timer.is_null(){return 0}
    let (was_active,dpc)={
        let mut table=TIMERS.lock();
        let Some(record)=table.records.iter_mut().find(|r|r.object==timer as usize) else{return 0};
        let old=(record.active,record.dpc);record.active=false;record.dpc=0;old
    };
    if was_active&&dpc!=0{let _=unsafe{ke_remove_queue_dpc(dpc as *mut core::ffi::c_void)};}
    was_active as u8
}
pub unsafe extern "win64" fn ke_initialize_dpc(dpc:*mut core::ffi::c_void,routine:Option<DpcRoutine>,context:*mut core::ffi::c_void){
    if dpc.is_null(){return}
    let mut table=DPCS.lock();
    let slot=table.records.iter().position(|r|r.object==dpc as usize)
        .or_else(||table.records.iter().position(|r|!r.active));
    if let Some(i)=slot{table.records[i]=DpcRecord{object:dpc as usize,routine:routine.map(|f|f as usize).unwrap_or(0),context:context as usize,arg1:0,arg2:0,target_cpu:cpu_slot(),importance:1,threaded:false,active:true,queued:false};}
}
/// Threaded DPCs share the public callback ABI; execution remains at DISPATCH_LEVEL
/// until the scheduler grows a dedicated PASSIVE_LEVEL worker queue.
pub unsafe extern "win64" fn ke_initialize_threaded_dpc(dpc:*mut core::ffi::c_void,routine:Option<DpcRoutine>,context:*mut core::ffi::c_void){
    unsafe{ke_initialize_dpc(dpc,routine,context);}
    if let Some(index)=dpc_slot(dpc as usize){DPCS.lock().records[index].threaded=true;}
}
pub unsafe extern "win64" fn ke_set_target_processor_dpc(dpc:*mut core::ffi::c_void,processor:u8){
    if dpc.is_null()||processor as usize>=crate::hal::MAX_CPUS||crate::hal::cpu_state(processor as usize).ok()!=Some(crate::hal::CPU_ONLINE){return}
    let mut table=DPCS.lock();
    if let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize){record.target_cpu=processor as usize;}
}
pub unsafe extern "win64" fn ke_set_target_processor_dpc_ex(dpc:*mut core::ffi::c_void,processor:*const ProcessorNumber){
    if processor.is_null(){return}
    let number=unsafe{&*processor};
    if number.group!=0{return}
    unsafe{ke_set_target_processor_dpc(dpc,number.number);}
}
pub unsafe extern "win64" fn ke_set_importance_dpc(dpc:*mut core::ffi::c_void,importance:i32){
    if dpc.is_null(){return}
    let mut table=DPCS.lock();
    if let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize){
        record.importance=importance.clamp(0,2) as u8;
    }
}
pub unsafe extern "win64" fn ke_is_executing_dpc()->u8{
    DPC_DISPATCHING[cpu_slot()].load(Ordering::Acquire) as u8
}
pub unsafe extern "win64" fn ke_insert_queue_dpc(dpc:*mut core::ffi::c_void,arg1:*mut core::ffi::c_void,arg2:*mut core::ffi::c_void)->u8{
    if dpc.is_null(){return 0}
    let mut table=DPCS.lock();
    let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize) else{return 0};
    if record.queued{return 0}
    record.arg1=arg1 as usize;record.arg2=arg2 as usize;record.queued=true;
    let target=record.target_cpu;
    drop(table);
    let current=cpu_slot();
    if target!=current{let _=crate::smp::request_reschedule(target);}
    else if CURRENT_IRQL[current].load(Ordering::Acquire)<DISPATCH_LEVEL { dispatch_queued_dpcs(); }
    1
}
pub unsafe extern "win64" fn ke_remove_queue_dpc(dpc:*mut core::ffi::c_void)->u8{
    if dpc.is_null(){return 0}
    let mut table=DPCS.lock();
    let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize) else{return 0};
    let was=record.queued;record.queued=false;was as u8
}
pub fn dispatch_queued_dpcs(){
    let cpu=cpu_slot();
    if CURRENT_IRQL[cpu].load(Ordering::Acquire)>DISPATCH_LEVEL{return}
    if DPC_DISPATCHING[cpu].swap(true,Ordering::AcqRel){return}
    let prior=CURRENT_IRQL[cpu].swap(DISPATCH_LEVEL,Ordering::AcqRel);
    for _ in 0..MAX_DPC_OBJECTS {
        let callback={
            let mut table=DPCS.lock();
            let Some(record)=table.records.iter_mut().find(|r|r.active&&r.queued&&r.target_cpu==cpu) else{break};
            record.queued=false;
            if record.routine==0{None}else{Some((record.routine,record.object,record.context,record.arg1,record.arg2))}
        };
        if let Some((routine,object,context,arg1,arg2))=callback{
            let function:DpcRoutine=unsafe{core::mem::transmute(routine)};
            unsafe{function(object as *mut _,context as *mut _,arg1 as *mut _,arg2 as *mut _);}
        }
    }
    CURRENT_IRQL[cpu].store(prior,Ordering::Release);
    DPC_DISPATCHING[cpu].store(false,Ordering::Release);
}
pub unsafe extern "win64" fn ke_flush_queued_dpcs(){
    let current=cpu_slot();
    for cpu in 0..crate::hal::MAX_CPUS{
        if cpu!=current&&crate::hal::cpu_state(cpu).ok()==Some(crate::hal::CPU_ONLINE){
            let _=crate::smp::request_reschedule(cpu);
        }
    }
    for _ in 0..10_000_000usize{
        dispatch_queued_dpcs();
        let pending={let table=DPCS.lock();table.records.iter().any(|r|r.active&&r.queued)};
        if !pending{return}
        for cpu in 0..crate::hal::MAX_CPUS{
            if cpu!=current&&crate::hal::cpu_state(cpu).ok()==Some(crate::hal::CPU_ONLINE){
                let _=crate::smp::request_reschedule(cpu);
            }
        }
        core::hint::spin_loop();
    }
}
/// Called by the APIC clock interrupt after dispatcher timeout processing.
/// Expiration is serialized with timer cancellation; callbacks run outside the
/// timer-table lock and at logical DISPATCH_LEVEL.
pub fn timer_tick(now:u64){
    let mut expired=[TimerRecord::EMPTY;MAX_TIMER_OBJECTS];
    let mut count=0usize;
    {
        let mut table=TIMERS.lock();
        for record in table.records.iter_mut(){
            if !record.active||record.due>now{continue}
            if count<expired.len(){expired[count]=*record;count+=1}
            if record.period!=0{
                record.due=record.due.saturating_add(record.period);
                if record.due<=now{record.due=now.saturating_add(record.period);}
            }else{record.active=false;}
        }
    }
    for record in expired[..count].iter().copied(){
        let timer=record.object as *mut DispatcherHeader;
        if timer.is_null(){continue}
        unsafe{(&*core::ptr::addr_of!((*timer).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1,Ordering::Release);}
        let _guard=DISPATCHER_OBJECT_LOCK.lock();
        complete_ready_wait_all_groups();
        let mut waiters=[crate::dispatcher::WokenWaiter{thread:0,timed_out:false};crate::dispatcher::MAX_WAITERS];
        let woke=if unsafe{(*timer).kind}==TIMER_NOTIFICATION{
            crate::dispatcher::wake_all_timed(timer as u64,&mut waiters)
        }else if let Some(waiter)=crate::dispatcher::wake_one_timed(timer as u64){
            waiters[0]=waiter;1
        }else{0};
        let consumed=woke!=0&&waiters[..woke].iter().any(|waiter|!waiter.timed_out);
        if consumed && unsafe{(*timer).kind}==TIMER_SYNCHRONIZATION{
            unsafe{(&*core::ptr::addr_of!((*timer).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
        }
        for waiter in waiters[..woke].iter().copied(){
            let signal_status=if waiter.timed_out{STATUS_TIMEOUT}else{STATUS_SUCCESS};
            let status=crate::dispatcher::complete_wait_group(waiter.thread,timer as u64,signal_status,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(signal_status);
            let _=crate::thread::complete_wait(waiter.thread,status);
        }
        drop(_guard);
        if record.dpc!=0{let _=unsafe{ke_insert_queue_dpc(record.dpc as *mut _,core::ptr::null_mut(),core::ptr::null_mut())};}
    }
    dispatch_queued_dpcs();
}

/// Result of the Ki processor-start phase. A successful AP count means APs
/// reached the online state; it does not imply Windows binary compatibility.
pub(crate) struct ProcessorStartupReport {
    pub online_aps: usize,
    pub startup_ok: bool,
    pub ipi_enabled: bool,
    pub interrupt_stress_passed: bool,
    pub tsc_synced: bool,
    pub worst_tsc_offset: u64,
    pub scheduler_stress_passed: bool,
    pub vm_stress_passed: bool,
}

/// KeInitializeExecutive-style early executive initialization. The hardware
/// startup path remains in Ki routines and architecture/HAL modules.
pub(crate) fn ke_initialize_executive() {
    crate::scheduler::initialize();
    crate::thread::initialize();
}

/// KiInitializeProcessor performs the software-visible checks that can be
/// verified after the CPU's private GDT/TSS/IDT and GS-local state are installed.
pub(crate) fn ki_initialize_processor(logical_id: usize) -> bool {
    if logical_id >= crate::hal::MAX_CPUS
        || crate::hal::cpu_state(logical_id).is_err()
    {
        return false;
    }
    crate::arch::per_cpu_tss_self_test(logical_id)
}

/// KiStartAllProcessors coordinates ACPI topology discovery, AP startup,
/// per-CPU IPI stress and cross-core TSC diagnostics. BSP remains usable if
/// firmware topology or AP startup is unavailable.
pub(crate) fn ki_start_all_processors(
    pml4_physical: u64,
    trampoline_physical: u64,
    rsdp_address: u64,
    bsp_apic_id: u32,
) -> ProcessorStartupReport {
    let mut report = ProcessorStartupReport {
        online_aps: 0,
        startup_ok: false,
        ipi_enabled: false,
        interrupt_stress_passed: false,
        tsc_synced: false,
        worst_tsc_offset: 0,
        scheduler_stress_passed: false,
        vm_stress_passed: false,
    };
    if rsdp_address != 0 && trampoline_physical != 0 {
        if let Ok(madt) = unsafe { crate::acpi::discover_madt(rsdp_address) } {
            if let Ok((online, ipi_enabled)) = unsafe {
                crate::smp::start_discovered_aps_with_ipi(
                    pml4_physical, trampoline_physical, &madt, bsp_apic_id,
                )
            } {
                report.online_aps = online;
                report.startup_ok = true;
                report.ipi_enabled = ipi_enabled;
                report.interrupt_stress_passed = ipi_enabled
                    && crate::smp::interrupt_setup_stress_test(64);
                let (synced, offset) = crate::apic::tsc_cross_core_status();
                report.tsc_synced = synced;
                report.worst_tsc_offset = offset;
            }
        }
    }
    report.scheduler_stress_passed =
        crate::thread::smp_scheduler_stress_test(crate::hal::online_cpu_count());
    report.vm_stress_passed = crate::kernel_vm::self_test();
    report
}

/// KiInitializeKernel is the fail-closed validation gate used after the
/// processor-start phase and before later boot-time self-tests continue.
pub(crate) fn ki_initialize_kernel(report: &ProcessorStartupReport) -> bool {
    report.scheduler_stress_passed && report.vm_stress_passed
}

/// Minimal Ke processor-query routines backed by the live HAL processor table.
pub unsafe extern "win64" fn ke_get_current_processor_number() -> u8 {
    unsafe { ke_get_current_processor_index().min(u8::MAX as u32) as u8 }
}
pub unsafe extern "win64" fn ke_query_active_processors() -> u64 {
    let mut mask = 0u64;
    for cpu in 0..crate::hal::MAX_CPUS.min(64) {
        if crate::hal::cpu_state(cpu).ok() == Some(crate::hal::CPU_ONLINE) {
            mask |= 1u64 << cpu;
        }
    }
    mask
}
pub unsafe extern "win64" fn ke_query_time_increment() -> u32 {
    // NT's default 100-ns time increment is not exposed until the timer source
    // is calibrated; return zero rather than advertise a fabricated tick rate.
    if crate::apic::local_timer_active() { 156_250 } else { 0 }
}
pub unsafe extern "win64" fn ke_should_yield_processor() -> u8 {
    crate::thread::has_ready_threads() as u8
}

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
pub unsafe extern "win64" fn ke_get_current_processor_number_ex(out: *mut ProcessorNumber) {
    if out.is_null() { return; }
    let Some(cpu) = crate::hal::current_cpu_local() else { return; };
    let logical = cpu.logical_id.load(Ordering::Acquire);
    if logical < 64 {
        unsafe { out.write(ProcessorNumber { group: 0, number: logical as u8, reserved: 0 }); }
    }
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
    let y = year - i64::from(month <= 2);
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

/// VOID KeQuerySystemTime(PLARGE_INTEGER CurrentTime), RTC-derived.
pub unsafe extern "win64" fn ke_query_system_time(current_time: *mut i64) {
    if !current_time.is_null() { unsafe { current_time.write(rtc_system_time_100ns().unwrap_or(0) as i64); } }
}

/// VOID KeQuerySystemTimePrecise(PLARGE_INTEGER CurrentTime).
/// Current RTC source is only second-resolution; this does not yet meet Windows' microsecond accuracy bound.
pub unsafe extern "win64" fn ke_query_system_time_precise(current_time: *mut i64) {
    unsafe { ke_query_system_time(current_time); }
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

/// VOID KeInitializeNotificationEvent(PKEVENT).
pub unsafe extern "win64" fn ke_initialize_notification_event(event: *mut DispatcherHeader) {
    unsafe { ke_initialize_event(event, EVENT_NOTIFICATION as i32, 0); }
}

/// VOID KeInitializeSynchronizationEvent(PKEVENT).
pub unsafe extern "win64" fn ke_initialize_synchronization_event(event: *mut DispatcherHeader) {
    unsafe { ke_initialize_event(event, EVENT_SYNCHRONIZATION as i32, 0); }
}

/// LONG KePulseEvent(PKEVENT, KPRIORITY, BOOLEAN).
pub unsafe extern "win64" fn ke_pulse_event(
    event: *mut DispatcherHeader, increment: i32, wait: u8,
) -> i32 {
    let previous = unsafe { ke_set_event(event, increment, wait) };
    let _ = unsafe { ke_reset_event(event) };
    previous
}

/// LONG KeSetEventBoostPriority(PKEVENT). Event signaling is supported;
/// Windows-compatible scheduler priority boosting is not yet modeled.
pub unsafe extern "win64" fn ke_set_event_boost_priority(event: *mut DispatcherHeader) -> i32 {
    unsafe { ke_set_event(event, 0, 0) }
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
        EVENT_NOTIFICATION => state.load(Ordering::Acquire) > 0,
        EVENT_SYNCHRONIZATION | 5 => {
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
    if !matches!(kind,EVENT_NOTIFICATION|EVENT_SYNCHRONIZATION|5){return false}
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)>0}
}
fn consume_wait_all_object(object_id:u64){
    let object=object_id as *mut DispatcherHeader;
    if object.is_null()||unsafe{(*object).kind}==EVENT_NOTIFICATION{return}
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
    if !matches!(unsafe { (*object).kind }, EVENT_NOTIFICATION | EVENT_SYNCHRONIZATION | 5) {
        return STATUS_NOT_IMPLEMENTED;
    }
    if alertable != 0 { return STATUS_NOT_IMPLEMENTED; } // APC delivery is not yet implemented.
    let Some(thread_id) = crate::thread::current_thread_id() else { return STATUS_NOT_IMPLEMENTED; };
    if unsafe { try_consume_dispatcher_object(object) } { return STATUS_SUCCESS; }
    let deadline = if timeout.is_null() { 0 } else {
        let interval = unsafe { timeout.read_unaligned() };
        if interval == 0 { return STATUS_TIMEOUT; }
        if interval > 0 { return STATUS_NOT_IMPLEMENTED; }
        if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
        match crate::apic::relative_timeout_deadline(interval) {
            Some(value) => value,
            None => return STATUS_NOT_IMPLEMENTED,
        }
    };
    let token = match crate::dispatcher::enqueue(object as u64, thread_id, deadline) {
        Ok(value) => value,
        Err(_) => return STATUS_NOT_IMPLEMENTED,
    };
    if unsafe { try_consume_dispatcher_object(object) } {
        let _ = crate::dispatcher::cancel(token);
        let _ = crate::thread::clear_wake_pending();
        return STATUS_SUCCESS;
    }
    if crate::thread::block_current().is_err() {
        let _ = crate::dispatcher::cancel(token);
        return STATUS_NOT_IMPLEMENTED;
    }
    crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS)
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
    if alertable != 0 { return STATUS_NOT_IMPLEMENTED; }
    let object_list = unsafe { core::slice::from_raw_parts(objects, count as usize) };
    if object_list.iter().any(|object| object.is_null()) { return STATUS_INVALID_PARAMETER; }
    for i in 0..object_list.len() {
        let kind = unsafe { (*object_list[i]).kind };
        if !matches!(kind, EVENT_NOTIFICATION | EVENT_SYNCHRONIZATION | 5) {
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
        if interval > 0 || !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
        match crate::apic::relative_timeout_deadline(interval) {
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
        if crate::dispatcher::register_wait_all(thread_id, &object_ids[..count as usize], timeout_token, deadline).is_err() {
            if let Some(token) = timeout_token { let _ = crate::dispatcher::cancel(token); }
            return STATUS_NOT_IMPLEMENTED;
        }
        drop(guard);
        if crate::thread::block_current().is_err() {
            crate::dispatcher::clear_wait_group(thread_id);
            if let Some(token) = timeout_token { let _ = crate::dispatcher::cancel(token); }
            return STATUS_NOT_IMPLEMENTED;
        }
        return crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
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
    if crate::dispatcher::register_wait_any(thread_id, &object_ids[..count as usize], &tokens[..registered], deadline).is_err() {
        for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
        return STATUS_NOT_IMPLEMENTED;
    }
    drop(guard);
    if crate::thread::block_current().is_err() {
        crate::dispatcher::clear_wait_group(thread_id);
        for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
        return STATUS_NOT_IMPLEMENTED;
    }
    crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS)
}

/// Never busy-loops when the thread runtime or calibrated delay source is absent.
pub unsafe extern "win64" fn ke_delay_execution_thread(
    wait_mode: i32,
    alertable: u8,
    interval: *const i64,
) -> i32 {
    if interval.is_null() { return STATUS_INVALID_PARAMETER; }
    if alertable != 0 { return STATUS_NOT_IMPLEMENTED; }
    let value = unsafe { interval.read_unaligned() };
    if value == 0 { return STATUS_SUCCESS; }
    if value > 0 { return STATUS_NOT_IMPLEMENTED; }
    let _ = wait_mode;
    let Some(thread_id) = crate::thread::current_thread_id() else { return STATUS_NOT_IMPLEMENTED; };
    if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
    let deadline = match crate::apic::relative_timeout_deadline(value) {
        Some(value) => value,
        None => return STATUS_NOT_IMPLEMENTED,
    };
    let token = match crate::dispatcher::enqueue(crate::dispatcher::delay_object(thread_id), thread_id, deadline) {
        Ok(value) => value,
        Err(_) => return STATUS_NOT_IMPLEMENTED,
    };
    if crate::thread::block_current().is_err() {
        let _ = crate::dispatcher::cancel(token);
        return STATUS_NOT_IMPLEMENTED;
    }
    crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS)
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

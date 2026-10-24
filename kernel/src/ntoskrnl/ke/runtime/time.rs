// timekeeping and processor topology queries.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

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


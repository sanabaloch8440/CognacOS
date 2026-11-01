use core::sync::atomic::AtomicU64;
// Ke executive clock/interrupt dispatch and scheduler notification glue.


static CLOCK_INTERRUPTS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static LAST_CLOCK_TICK: core::sync::atomic::AtomicU64 = AtomicU64::new(0);

/// Register the executive clock consumer before enabling the HAL clock event.
pub(crate) fn register_clock_interrupt_handler() -> Result<(), crate::hal::HalError> {
    crate::hal::register_clock_interrupt_notifier(ki_clock_interrupt)
}

/// Runs timer/DPC expiry before dispatcher timeout completion on a clock tick.
/// This is deliberately a project-internal routine, not a Windows export.
pub(crate) fn ki_clock_interrupt(now: u64) {
    LAST_CLOCK_TICK.store(now, Ordering::Release);
    CLOCK_INTERRUPTS.fetch_add(1, Ordering::Relaxed);
    // Keep tick accounting and peer-clock watchdog checks on the same path as
    // the actual HAL clock notifier, not in a polling thread that could starve.
    crate::ntoskrnl::ke::runtime::ke_accumulate_ticks(now);
    crate::ntoskrnl::ke::runtime::timer_tick(now);
    crate::syscall::expire_user_address_waits(now);
    let mut expired = [crate::dispatcher::ExpiredWait { thread: 0, object: 0 }; 64];
    let count = crate::dispatcher::expire_detailed(now, &mut expired);
    for wait in expired[..count].iter().copied() {
        let status = if crate::dispatcher::is_delay_object(wait.object) { 0 } else { 0x0000_0102 };
        let final_status = crate::dispatcher::complete_wait_group(
            wait.thread, wait.object, status, 0, 0x0000_0102,
        ).unwrap_or(status);
        let _ = crate::thread::complete_wait(wait.thread, final_status);
    }
}

/// A monotonic diagnostic counter used by internal timer stress tests.
pub(crate) fn ki_clock_interrupt_count() -> u64 {
    CLOCK_INTERRUPTS.load(Ordering::Acquire)
}

/// Last monotonic tick passed through Ki's interrupt path.
pub(crate) fn ki_last_clock_tick() -> u64 {
    LAST_CLOCK_TICK.load(Ordering::Acquire)
}

/// Requests a scheduler reschedule on another online processor.
pub(crate) fn ki_request_dispatch(cpu: usize) -> bool {
    if cpu >= crate::hal::MAX_CPUS || crate::hal::cpu_state(cpu).ok() != Some(crate::hal::CPU_ONLINE) {
        return false;
    }
    crate::smp::request_reschedule(cpu)
}

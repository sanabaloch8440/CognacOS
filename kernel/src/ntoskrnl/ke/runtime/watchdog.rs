// Ke tick accounting and watchdogs for Windows-compatible bug checks.
//
// This is a defensive implementation of the documented observable contract,
// not a byte-for-byte clone of Windows' private PRCB internals. Counters use
// the HAL's monotonic performance counter, so thresholds are independent of
// the clock-tick frequency. Defaults are intentionally conservative.

const CPU_COUNT: usize = 64;
const CLOCK_WATCHDOG_TIMEOUT: u32 = 0x0000_0101;
const DPC_WATCHDOG_VIOLATION: u32 = 0x0000_0133;
const DPC_SINGLE_TIMEOUT_SECONDS: u64 = 20;
const HIGH_IRQL_TIMEOUT_SECONDS: u64 = 120;
const CLOCK_TIMEOUT_SECONDS: u64 = 10;

static LAST_TICK_COUNTER: [core::sync::atomic::AtomicU64; CPU_COUNT] =
    [const { AtomicU64::new(0) }; CPU_COUNT];
static TICK_COUNT: [AtomicU64; CPU_COUNT] =
    [const { AtomicU64::new(0) }; CPU_COUNT];
static LAST_CLOCK_SEEN: [AtomicU64; CPU_COUNT] =
    [const { AtomicU64::new(0) }; CPU_COUNT];
static HIGH_IRQL_START: [AtomicU64; CPU_COUNT] =
    [const { AtomicU64::new(0) }; CPU_COUNT];
static HIGH_IRQL_TOTAL: [AtomicU64; CPU_COUNT] =
    [const { AtomicU64::new(0) }; CPU_COUNT];
static ACTIVE_DPC_START: [AtomicU64; CPU_COUNT] =
    [const { AtomicU64::new(0) }; CPU_COUNT];
static ACTIVE_DPC_ROUTINE: [AtomicU64; CPU_COUNT] =
    [const { AtomicU64::new(0) }; CPU_COUNT];
static WATCHDOG_REPORTED: [core::sync::atomic::AtomicU8; CPU_COUNT] =
    [const { AtomicU8::new(0) }; CPU_COUNT];

#[inline]
fn counter_frequency() -> Option<u64> {
    crate::hal::query_performance_frequency_hz().filter(|hz| *hz != 0)
}

#[inline]
fn now_counter() -> u64 {
    crate::hal::query_performance_counter()
}

#[inline]
fn seconds_to_ticks(seconds: u64, hz: u64) -> u64 {
    (seconds as u128 * hz as u128).min(u64::MAX as u128) as u64
}

#[inline]
fn current_cpu() -> usize {
    crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Relaxed) as usize)
        .filter(|cpu| *cpu < CPU_COUNT)
        .unwrap_or(0)
}

/// Ke-owned clock tick accounting. Called from the HAL clock notifier on each
/// processor that services a clock interrupt. It records the per-CPU heartbeat,
/// counts ticks, accounts high-IRQL residence, and checks peer clock progress.
pub(crate) fn ke_accumulate_ticks(_hal_timestamp: u64) {
    let cpu = current_cpu();
    let now = now_counter();
    LAST_TICK_COUNTER[cpu].store(now, Ordering::Release);
    LAST_CLOCK_SEEN[cpu].store(now, Ordering::Release);
    TICK_COUNT[cpu].fetch_add(1, Ordering::Relaxed);

    // The clock interrupt can preempt a stuck DISPATCH_LEVEL callback. Check
    // the in-flight DPC here as well as on normal callback return, otherwise
    // an infinite DPC would never reach watchdog_dpc_end().
    if let Some(hz) = counter_frequency() {
        let dpc_start = ACTIVE_DPC_START[cpu].load(Ordering::Acquire);
        if dpc_start != 0 {
            let elapsed = now.saturating_sub(dpc_start);
            let limit = seconds_to_ticks(DPC_SINGLE_TIMEOUT_SECONDS, hz);
            if elapsed > limit {
                let routine = ACTIVE_DPC_ROUTINE[cpu].load(Ordering::Acquire);
                unsafe {
                    crate::ntoskrnl::ke::runtime::ke_bug_check_ex(
                        DPC_WATCHDOG_VIOLATION, 0, elapsed as usize, limit as usize, routine as usize,
                    );
                }
            }
        }
    }

    let irql = CURRENT_IRQL[cpu].load(Ordering::Acquire);
    if irql >= 2 {
        let start = HIGH_IRQL_START[cpu].load(Ordering::Acquire);
        if start == 0 {
            HIGH_IRQL_START[cpu].store(now.max(1), Ordering::Release);
        } else if let Some(hz) = counter_frequency() {
            let elapsed = now.saturating_sub(start);
            if elapsed >= seconds_to_ticks(HIGH_IRQL_TIMEOUT_SECONDS, hz)
                && WATCHDOG_REPORTED[cpu].compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok()
            {
                // Arg1=1: cumulative time at DISPATCH_LEVEL or above.
                unsafe {
                    crate::ntoskrnl::ke::runtime::ke_bug_check_ex(
                        DPC_WATCHDOG_VIOLATION, 1, HIGH_IRQL_TOTAL[cpu].load(Ordering::Relaxed).saturating_add(elapsed) as usize,
                        seconds_to_ticks(HIGH_IRQL_TIMEOUT_SECONDS, hz) as usize, 0,
                    );
                }
            }
        }
    } else {
        let start = HIGH_IRQL_START[cpu].swap(0, Ordering::AcqRel);
        if start != 0 {
            HIGH_IRQL_TOTAL[cpu].fetch_add(now.saturating_sub(start), Ordering::Relaxed);
        }
        WATCHDOG_REPORTED[cpu].store(0, Ordering::Release);
    }

    // A clock watchdog is only meaningful once multiple processors are online
    // and a processor has demonstrated clock progress at least once. The first
    // observed heartbeat is a baseline, not a timeout.
    if let Some(hz) = counter_frequency() {
        let timeout = seconds_to_ticks(CLOCK_TIMEOUT_SECONDS, hz);
        for peer in 0..CPU_COUNT {
            if peer == cpu
                || crate::hal::cpu_state(peer).ok() != Some(crate::hal::CPU_ONLINE)
            {
                continue;
            }
            let last = LAST_CLOCK_SEEN[peer].load(Ordering::Acquire);
            if last == 0 {
                continue;
            }
            let elapsed = now.saturating_sub(last);
            if elapsed >= timeout {
                // Windows 0x101: Arg1 is the nominal timeout interval, Arg2=0,
                // Arg3 is the PRCB address, Arg4 is the unresponsive CPU index.
                // CognacOS currently has no public PRCB ABI, so Arg3 is zero.
                unsafe {
                    crate::ntoskrnl::ke::runtime::ke_bug_check_ex(
                        CLOCK_WATCHDOG_TIMEOUT, timeout as usize, 0, 0, peer,
                    );
                }
            }
        }
    }
}

/// Mark the start of one DPC callback. DPC callbacks are expected to be short;
/// the Windows bug-check threshold is intentionally much larger than the
/// 100-microsecond engineering guideline to avoid noisy false positives.
pub(crate) fn watchdog_dpc_begin(routine: usize) {
    let cpu = current_cpu();
    ACTIVE_DPC_ROUTINE[cpu].store(routine as u64, Ordering::Release);
    ACTIVE_DPC_START[cpu].store(now_counter().max(1), Ordering::Release);
}

/// Check one DPC callback's elapsed time before returning to the dispatcher.
pub(crate) fn watchdog_dpc_end() {
    let cpu = current_cpu();
    let start = ACTIVE_DPC_START[cpu].swap(0, Ordering::AcqRel);
    let routine = ACTIVE_DPC_ROUTINE[cpu].swap(0, Ordering::AcqRel);
    if start == 0 { return; }
    if let Some(hz) = counter_frequency() {
        let elapsed = now_counter().saturating_sub(start);
        let limit = seconds_to_ticks(DPC_SINGLE_TIMEOUT_SECONDS, hz);
        if elapsed > limit {
            // Arg1=0: a single DPC exceeded its allotment.
            unsafe {
                crate::ntoskrnl::ke::runtime::ke_bug_check_ex(
                    DPC_WATCHDOG_VIOLATION, 0, elapsed as usize, limit as usize, routine as usize,
                );
            }
        }
    }
}

/// Stable diagnostic counters for debugger extensions and kernel self-tests.
pub(crate) fn watchdog_tick_count(cpu: usize) -> u64 {
    TICK_COUNT.get(cpu).map_or(0, |count| count.load(Ordering::Acquire))
}

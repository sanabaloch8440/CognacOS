//! Rust port of the deterministic calendar and epoch conversion core from
//! Wine's dlls/ntdll/time.c. See that source's LGPL-2.1-or-later notice and
//! original ReactOS attribution. Time-zone APIs and unbiased interrupt-time
//! queries are not ported by this module.
//!
//! NT system time is a signed count of 100-nanosecond intervals since 1601-01-01.

#![allow(non_camel_case_types, non_snake_case)]
pub type BOOLEAN = u8;
pub type ULONG = u32;
pub const TICKS_PER_SECOND: i64 = 10_000_000;
pub const TICKS_PER_MILLISECOND: i64 = 10_000;
const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_HOUR: i64 = 3_600;
const SECONDS_PER_MINUTE: i64 = 60;
const DAYS_1601_TO_1970: i64 = 134_774;
const DAYS_1601_TO_1980: i64 = 138_426;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LARGE_INTEGER { pub QuadPart: i64 }

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TIME_FIELDS {
    pub Year: i16, pub Month: i16, pub Day: i16, pub Hour: i16,
    pub Minute: i16, pub Second: i16, pub Milliseconds: i16, pub Weekday: i16,
}

const MONTH_LENGTHS: [[i16; 12]; 2] = [
    [31,28,31,30,31,30,31,31,30,31,30,31],
    [31,29,31,30,31,30,31,31,30,31,30,31],
];
#[inline] fn leap(y: i32) -> bool { y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) }

/// Converts NT ticks to TIME_FIELDS. Invalid/null pointers are ignored because
/// the Windows ABI has no status return for this routine.
///
/// # Safety
/// Non-null pointers must be aligned; time must be readable and fields writable.
#[no_mangle]
pub unsafe extern "system" fn RtlTimeToTimeFields(time: *const LARGE_INTEGER, fields: *mut TIME_FIELDS) {
    if time.is_null() || fields.is_null() { return; }
    let ticks = unsafe { (*time).QuadPart };
    let out = unsafe { &mut *fields };
    out.Milliseconds = ((ticks % TICKS_PER_SECOND) / TICKS_PER_MILLISECOND) as i16;
    let seconds = ticks / TICKS_PER_SECOND;
    let days = seconds / SECONDS_PER_DAY;
    let mut sod = seconds % SECONDS_PER_DAY;
    out.Hour = (sod / SECONDS_PER_HOUR) as i16;
    sod %= SECONDS_PER_HOUR;
    out.Minute = (sod / SECONDS_PER_MINUTE) as i16;
    out.Second = (sod % SECONDS_PER_MINUTE) as i16;
    out.Weekday = ((1 + days) % 7) as i16;

    let cleaps = (3 * ((4 * days + 1227) / 146_097) + 3) / 4;
    let adjusted = days + 28_188 + cleaps;
    let years = (20 * adjusted - 2442) / (5 * 1461);
    let yearday = adjusted - (years * 1461) / 4;
    let months = (64 * yearday) / 1959;
    if months < 14 {
        out.Month = (months - 1) as i16;
        out.Year = (years + 1524) as i16;
    } else {
        out.Month = (months - 13) as i16;
        out.Year = (years + 1525) as i16;
    }
    out.Day = (yearday - (1959 * months) / 64) as i16;
}

/// Converts validated TIME_FIELDS to NT ticks. Returns FALSE for invalid data.
///
/// # Safety
/// fields must be readable and time writable and aligned.
#[no_mangle]
pub unsafe extern "system" fn RtlTimeFieldsToTime(fields: *const TIME_FIELDS, time: *mut LARGE_INTEGER) -> BOOLEAN {
    if fields.is_null() || time.is_null() { return 0; }
    let f = unsafe { &*fields };
    let y = i32::from(f.Year);
    let m = i32::from(f.Month);
    if f.Milliseconds < 0 || f.Milliseconds > 999 || f.Second < 0 || f.Second > 59
        || f.Minute < 0 || f.Minute > 59 || f.Hour < 0 || f.Hour > 23
        || !(1..=12).contains(&m) || f.Day < 1 || y < 1601
        || f.Day > MONTH_LENGTHS[usize::from(m == 2 || leap(y))][(m - 1) as usize] { return 0; }
    let (month, year) = if m < 3 { (m + 13, y - 1) } else { (m + 1, y) };
    let century_leaps = (3 * (year / 100) + 3) / 4;
    let days = (36_525 * year) / 100 - century_leaps + (1959 * month) / 64
        + i32::from(f.Day) - 584_817;
    let millis = (((((i64::from(days) * 24 + i64::from(f.Hour)) * 60
        + i64::from(f.Minute)) * 60 + i64::from(f.Second)) * 1000)
        + i64::from(f.Milliseconds));
    unsafe { (*time).QuadPart = millis * TICKS_PER_MILLISECOND; }
    1
}

unsafe fn nt_time_to_seconds(time: *const LARGE_INTEGER, out: *mut ULONG, epoch_days: i64) -> BOOLEAN {
    if time.is_null() || out.is_null() { return 0; }
    let ticks = unsafe { (*time).QuadPart };
    let epoch = epoch_days * SECONDS_PER_DAY * TICKS_PER_SECOND;
    if ticks < epoch { return 0; }
    let seconds = ticks / TICKS_PER_SECOND - epoch_days * SECONDS_PER_DAY;
    if seconds > i64::from(ULONG::MAX) { return 0; }
    unsafe { out.write(seconds as ULONG); }
    1
}
unsafe fn seconds_to_nt_time(seconds: ULONG, out: *mut LARGE_INTEGER, epoch_days: i64) {
    if !out.is_null() {
        unsafe { (*out).QuadPart = (i64::from(seconds) + epoch_days * SECONDS_PER_DAY) * TICKS_PER_SECOND; }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlTimeToSecondsSince1970(t: *const LARGE_INTEGER, s: *mut ULONG) -> BOOLEAN {
    unsafe { nt_time_to_seconds(t, s, DAYS_1601_TO_1970) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlSecondsSince1970ToTime(s: ULONG, t: *mut LARGE_INTEGER) {
    unsafe { seconds_to_nt_time(s, t, DAYS_1601_TO_1970) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlTimeToSecondsSince1980(t: *const LARGE_INTEGER, s: *mut ULONG) -> BOOLEAN {
    unsafe { nt_time_to_seconds(t, s, DAYS_1601_TO_1980) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlSecondsSince1980ToTime(s: ULONG, t: *mut LARGE_INTEGER) {
    unsafe { seconds_to_nt_time(s, t, DAYS_1601_TO_1980) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn date(y: i16, m: i16, d: i16) -> TIME_FIELDS {
        TIME_FIELDS { Year:y, Month:m, Day:d, ..TIME_FIELDS::default() }
    }
    #[test] fn unix_epoch_round_trip() {
        let mut t = LARGE_INTEGER::default();
        unsafe { RtlSecondsSince1970ToTime(0, &mut t); }
        let mut s = u32::MAX;
        assert_eq!(unsafe { RtlTimeToSecondsSince1970(&t, &mut s) }, 1);
        assert_eq!(s, 0);
    }
    #[test] fn year_1980_epoch_round_trip() {
        let mut t = LARGE_INTEGER::default();
        unsafe { RtlSecondsSince1980ToTime(0, &mut t); }
        let mut s = u32::MAX;
        assert_eq!(unsafe { RtlTimeToSecondsSince1980(&t, &mut s) }, 1);
        assert_eq!(s, 0);
    }
    #[test] fn validates_gregorian_leap_days() {
        let mut t = LARGE_INTEGER::default();
        assert_eq!(unsafe { RtlTimeFieldsToTime(&date(2000,2,29), &mut t) }, 1);
        assert_eq!(unsafe { RtlTimeFieldsToTime(&date(1900,2,29), &mut t) }, 0);
    }
    #[test] fn calendar_round_trip() {
        let f = date(2024,2,29);
        let mut t = LARGE_INTEGER::default();
        assert_eq!(unsafe { RtlTimeFieldsToTime(&f, &mut t) }, 1);
        let mut result = TIME_FIELDS::default();
        unsafe { RtlTimeToTimeFields(&t, &mut result); }
        assert_eq!((result.Year,result.Month,result.Day), (2024,2,29));
    }
    #[test] fn rejects_pre_epoch() {
        let t = LARGE_INTEGER::default();
        let mut s = 0;
        assert_eq!(unsafe { RtlTimeToSecondsSince1970(&t, &mut s) }, 0);
    }
}


// Native service dependencies used by the remaining time APIs. These imports
// must be resolved by the NTDLL native-service link/export layer.
pub type NTSTATUS = i32;
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_PRIVILEGE_NOT_HELD: NTSTATUS = 0xC000_0061u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const SYSTEM_TIME_OF_DAY_INFORMATION: ULONG = 3;
const SYSTEM_CURRENT_TIME_ZONE_INFORMATION: ULONG = 44;
const SYSTEM_DYNAMIC_TIME_ZONE_INFORMATION: ULONG = 104;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SYSTEM_TIME {
    pub Year: u16, pub Month: u16, pub DayOfWeek: u16, pub Day: u16,
    pub Hour: u16, pub Minute: u16, pub Second: u16, pub Milliseconds: u16,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RTL_TIME_ZONE_INFORMATION {
    pub Bias: i32,
    pub StandardName: [u16; 32],
    pub StandardDate: SYSTEM_TIME,
    pub StandardBias: i32,
    pub DaylightName: [u16; 32],
    pub DaylightDate: SYSTEM_TIME,
    pub DaylightBias: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RTL_DYNAMIC_TIME_ZONE_INFORMATION {
    pub Bias: i32,
    pub StandardName: [u16; 32],
    pub StandardDate: SYSTEM_TIME,
    pub StandardBias: i32,
    pub DaylightName: [u16; 32],
    pub DaylightDate: SYSTEM_TIME,
    pub DaylightBias: i32,
    pub TimeZoneKeyName: [u16; 128],
    pub DynamicDaylightTimeDisabled: u8,
    pub _padding: [u8; 3],
}
impl Default for RTL_DYNAMIC_TIME_ZONE_INFORMATION {
    fn default() -> Self {
        Self { Bias: 0, StandardName: [0;32], StandardDate: SYSTEM_TIME::default(),
            StandardBias: 0, DaylightName: [0;32], DaylightDate: SYSTEM_TIME::default(),
            DaylightBias: 0, TimeZoneKeyName: [0;128], DynamicDaylightTimeDisabled: 0,
            _padding: [0;3] }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SYSTEM_TIMEOFDAY_INFORMATION {
    BootTime: LARGE_INTEGER,
    CurrentTime: LARGE_INTEGER,
    TimeZoneBias: LARGE_INTEGER,
    CurrentTimeZoneId: ULONG,
    Reserved: ULONG,
}

unsafe extern "system" {
    fn NtQuerySystemInformation(class: ULONG, info: *mut core::ffi::c_void,
        length: ULONG, return_length: *mut ULONG) -> NTSTATUS;
    fn NtQueryPerformanceCounter(counter: *mut LARGE_INTEGER,
        frequency: *mut LARGE_INTEGER) -> NTSTATUS;
    fn NtQuerySystemTime(time: *mut LARGE_INTEGER) -> NTSTATUS;
    fn RtlSetLastWin32ErrorAndNtStatusFromNtStatus(status: NTSTATUS);
}

/// Convert local time to system time using the current system time-zone bias.
#[no_mangle]
pub unsafe extern "system" fn RtlLocalTimeToSystemTime(local: *const LARGE_INTEGER,
    system: *mut LARGE_INTEGER) -> NTSTATUS {
    if local.is_null() || system.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut info = SYSTEM_TIMEOFDAY_INFORMATION::default();
    let status = unsafe { NtQuerySystemInformation(SYSTEM_TIME_OF_DAY_INFORMATION,
        (&mut info as *mut SYSTEM_TIMEOFDAY_INFORMATION).cast(),
        core::mem::size_of::<SYSTEM_TIMEOFDAY_INFORMATION>() as ULONG, core::ptr::null_mut()) };
    if status < 0 { return status; }
    unsafe { (*system).QuadPart = (*local).QuadPart.wrapping_add(info.TimeZoneBias.QuadPart); }
    STATUS_SUCCESS
}

/// Convert system time to local time using the current system time-zone bias.
#[no_mangle]
pub unsafe extern "system" fn RtlSystemTimeToLocalTime(system: *const LARGE_INTEGER,
    local: *mut LARGE_INTEGER) -> NTSTATUS {
    if system.is_null() || local.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut info = SYSTEM_TIMEOFDAY_INFORMATION::default();
    let status = unsafe { NtQuerySystemInformation(SYSTEM_TIME_OF_DAY_INFORMATION,
        (&mut info as *mut SYSTEM_TIMEOFDAY_INFORMATION).cast(),
        core::mem::size_of::<SYSTEM_TIMEOFDAY_INFORMATION>() as ULONG, core::ptr::null_mut()) };
    if status < 0 { return status; }
    unsafe { (*local).QuadPart = (*system).QuadPart.wrapping_sub(info.TimeZoneBias.QuadPart); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlTimeToElapsedTimeFields(time: *const LARGE_INTEGER,
    fields: *mut TIME_FIELDS) {
    if time.is_null() || fields.is_null() { return; }
    let ticks = unsafe { (*time).QuadPart };
    let seconds = ticks / TICKS_PER_SECOND;
    let out = unsafe { &mut *fields };
    out.Milliseconds = ((ticks % TICKS_PER_SECOND) / TICKS_PER_MILLISECOND) as i16;
    out.Year = 0;
    out.Month = 0;
    out.Day = (seconds / SECONDS_PER_DAY) as i16;
    let mut rem = (seconds % SECONDS_PER_DAY) as i32;
    out.Second = (rem % 60) as i16;
    rem /= 60;
    out.Minute = (rem % 60) as i16;
    out.Hour = (rem / 60) as i16;
}

/// Query the native performance counter; failures are reflected as FALSE.
#[no_mangle]
pub unsafe extern "system" fn RtlQueryPerformanceCounter(counter: *mut LARGE_INTEGER) -> BOOLEAN {
    if counter.is_null() { return 0; }
    (unsafe { NtQueryPerformanceCounter(counter, core::ptr::null_mut()) } >= 0) as BOOLEAN
}

/// Windows NTDLL exposes the counter frequency in 100-ns units.
#[no_mangle]
pub unsafe extern "system" fn RtlQueryPerformanceFrequency(frequency: *mut LARGE_INTEGER) -> BOOLEAN {
    if frequency.is_null() { return 0; }
    unsafe { (*frequency).QuadPart = TICKS_PER_SECOND; }
    1
}

// Cached wall-clock/QPC calibration for RtlGetSystemTimePrecise. The sequence
// counter makes the (system-time, QPC, frequency) tuple coherent for readers.
// Calibration is refreshed at least once per second to bound oscillator drift
// and follow wall-clock adjustments.
use core::sync::atomic::{AtomicI64, AtomicU32, Ordering};
static PRECISE_SEQ: AtomicU32 = AtomicU32::new(0);
static PRECISE_SYSTEM_TIME: AtomicI64 = AtomicI64::new(0);
static PRECISE_QPC: AtomicI64 = AtomicI64::new(0);
static PRECISE_QPC_FREQUENCY: AtomicI64 = AtomicI64::new(0);
static PRECISE_REFRESH_QPC: AtomicI64 = AtomicI64::new(0);
static PRECISE_LAST_RETURNED: AtomicI64 = AtomicI64::new(0);
static PRECISE_REFRESH_LOCK: AtomicU32 = AtomicU32::new(0);
const PRECISE_REFRESH_INTERVAL_100NS: i64 = 10_000_000;

fn precise_snapshot() -> Option<(i64, i64, i64, i64)> {
    loop {
        let before = PRECISE_SEQ.load(Ordering::Acquire);
        if before & 1 != 0 { core::hint::spin_loop(); continue; }
        let system = PRECISE_SYSTEM_TIME.load(Ordering::Relaxed);
        let qpc = PRECISE_QPC.load(Ordering::Relaxed);
        let frequency = PRECISE_QPC_FREQUENCY.load(Ordering::Relaxed);
        let refreshed = PRECISE_REFRESH_QPC.load(Ordering::Relaxed);
        let after = PRECISE_SEQ.load(Ordering::Acquire);
        if before == after {
            if frequency > 0 && system > 0 { return Some((system, qpc, frequency, refreshed)); }
            return None;
        }
    }
}

unsafe fn refresh_precise_clock() -> bool {
    // Read QPC on both sides of system time and anchor to the midpoint, reducing
    // skew between the two clocks. NT failures leave the previous calibration intact.
    let mut qpc_before = LARGE_INTEGER::default();
    let mut qpc_after = LARGE_INTEGER::default();
    let mut frequency = LARGE_INTEGER::default();
    let mut system = LARGE_INTEGER::default();
    if unsafe { NtQueryPerformanceCounter(&mut qpc_before, &mut frequency) } < 0
        || frequency.QuadPart <= 0
        || unsafe { NtQuerySystemTime(&mut system) } < 0
        || unsafe { NtQueryPerformanceCounter(&mut qpc_after, core::ptr::null_mut()) } < 0
    { return false; }

    let midpoint = qpc_before.QuadPart.saturating_add(
        qpc_after.QuadPart.saturating_sub(qpc_before.QuadPart) / 2);
    PRECISE_SEQ.fetch_add(1, Ordering::AcqRel); // odd: writer active
    PRECISE_SYSTEM_TIME.store(system.QuadPart, Ordering::Relaxed);
    PRECISE_QPC.store(midpoint, Ordering::Relaxed);
    PRECISE_QPC_FREQUENCY.store(frequency.QuadPart, Ordering::Relaxed);
    PRECISE_REFRESH_QPC.store(qpc_after.QuadPart, Ordering::Relaxed);
    PRECISE_SEQ.fetch_add(1, Ordering::Release); // even: publish tuple
    true
}

/// Return system time in 100-ns units since 1601, interpolating between native
/// system-time samples with the monotonic performance counter. The calibration
/// is refreshed once per second so clock changes and counter drift are bounded.
/// If calibration or QPC is unavailable, fall back to NtQuerySystemTime.
///
/// This is a user-mode implementation: precision is limited by the kernel's
/// QPC and system-time services, and it does not claim nanosecond accuracy.
#[no_mangle]
pub unsafe extern "system" fn RtlGetSystemTimePrecise() -> i64 {
    let mut qpc = LARGE_INTEGER::default();
    if unsafe { NtQueryPerformanceCounter(&mut qpc, core::ptr::null_mut()) } < 0 {
        let mut fallback = LARGE_INTEGER::default();
        return if unsafe { NtQuerySystemTime(&mut fallback) } >= 0 { fallback.QuadPart } else { 0 };
    }

    let mut snapshot = precise_snapshot();
    let stale = snapshot.map(|(system, anchor, _freq, refreshed)| {
        system == 0 || qpc.QuadPart < refreshed
            || qpc.QuadPart.saturating_sub(refreshed) >= PRECISE_REFRESH_INTERVAL_100NS
                .saturating_mul(PRECISE_QPC_FREQUENCY.load(Ordering::Relaxed)) / TICKS_PER_SECOND
            || qpc.QuadPart < anchor
    }).unwrap_or(true);

    if stale && PRECISE_REFRESH_LOCK.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_ok() {
        let _ = unsafe { refresh_precise_clock() };
        PRECISE_REFRESH_LOCK.store(0, Ordering::Release);
        snapshot = precise_snapshot();
    }
    let Some((system_base, qpc_base, frequency, _)) = snapshot else {
        let mut fallback = LARGE_INTEGER::default();
        return if unsafe { NtQuerySystemTime(&mut fallback) } >= 0 { fallback.QuadPart } else { 0 };
    };
    if qpc.QuadPart < qpc_base || frequency <= 0 {
        let mut fallback = LARGE_INTEGER::default();
        return if unsafe { NtQuerySystemTime(&mut fallback) } >= 0 { fallback.QuadPart } else { 0 };
    }

    let delta_qpc = qpc.QuadPart - qpc_base;
    let delta_100ns = ((delta_qpc as i128) * (TICKS_PER_SECOND as i128)
        / (frequency as i128)).clamp(0, i64::MAX as i128) as i64;
    let candidate = system_base.saturating_add(delta_100ns);
    // System wall time can be adjusted backwards. Avoid returning a value lower
    // than the previous result, while still allowing the next calibration to
    // correct the wall-clock anchor.
    let mut last = PRECISE_LAST_RETURNED.load(Ordering::Relaxed);
    loop {
        let monotonic = candidate.max(last);
        match PRECISE_LAST_RETURNED.compare_exchange_weak(last, monotonic,
            Ordering::AcqRel, Ordering::Relaxed) {
            Ok(_) => return monotonic,
            Err(actual) => last = actual,
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryTimeZoneInformation(out: *mut RTL_TIME_ZONE_INFORMATION) -> NTSTATUS {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { NtQuerySystemInformation(SYSTEM_CURRENT_TIME_ZONE_INFORMATION, out.cast(),
        core::mem::size_of::<RTL_TIME_ZONE_INFORMATION>() as ULONG, core::ptr::null_mut()) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryDynamicTimeZoneInformation(out: *mut RTL_DYNAMIC_TIME_ZONE_INFORMATION) -> NTSTATUS {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { NtQuerySystemInformation(SYSTEM_DYNAMIC_TIME_ZONE_INFORMATION, out.cast(),
        core::mem::size_of::<RTL_DYNAMIC_TIME_ZONE_INFORMATION>() as ULONG, core::ptr::null_mut()) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetTimeZoneInformation(_info: *const RTL_TIME_ZONE_INFORMATION) -> NTSTATUS {
    // Matches the imported Wine implementation: changing the system time zone
    // is not permitted by this user-mode entry point.
    STATUS_PRIVILEGE_NOT_HELD
}

/// Read the shared-data interrupt clock with the Windows seqlock pattern.
/// KUSER_SHARED_DATA is mapped at 0x7ffe0000 in the Windows x86/x64 user ABI.
#[no_mangle]
pub unsafe extern "system" fn RtlQueryUnbiasedInterruptTime(out: *mut u64) -> BOOLEAN {
    if out.is_null() {
        unsafe { RtlSetLastWin32ErrorAndNtStatusFromNtStatus(STATUS_INVALID_PARAMETER); }
        return 0;
    }
    // KSYSTEM_TIME InterruptTime is at offset 0x08: High1Time, LowPart,
    // High2Time. Use volatile reads so the compiler preserves the seqlock.
    let base = 0x7ffe_0000usize as *const u8;
    let high1 = base.wrapping_add(0x08).cast::<u32>();
    let low = base.wrapping_add(0x0c).cast::<u32>();
    let high2 = base.wrapping_add(0x10).cast::<u32>();
    loop {
        let h1 = unsafe { core::ptr::read_volatile(high1) };
        let lo = unsafe { core::ptr::read_volatile(low) };
        let h2 = unsafe { core::ptr::read_volatile(high2) };
        if h1 == h2 {
            unsafe { out.write((u64::from(h1) << 32) | u64::from(lo)); }
            return 1;
        }
        core::hint::spin_loop();
    }
}

/// Read TickCount from KUSER_SHARED_DATA. The low 32 bits intentionally wrap.
#[no_mangle]
pub extern "system" fn NtGetTickCount() -> ULONG {
    let base = 0x7ffe_0000usize as *const u8;
    // TickCount KSYSTEM_TIME is at offset 0x320 in the x86/x64 shared-data ABI.
    let high1 = base.wrapping_add(0x320).cast::<u32>();
    let low = base.wrapping_add(0x324).cast::<u32>();
    let high2 = base.wrapping_add(0x328).cast::<u32>();
    loop {
        let h1 = unsafe { core::ptr::read_volatile(high1) };
        let lo = unsafe { core::ptr::read_volatile(low) };
        let h2 = unsafe { core::ptr::read_volatile(high2) };
        if h1 == h2 {
            // TickCountMultiplier is at offset 0x04 and converts 100-ns
            // ticks to milliseconds via a 24-bit fixed-point multiplier.
            let multiplier = unsafe { core::ptr::read_volatile(base.wrapping_add(0x04).cast::<u32>()) };
            let ticks = (u64::from(h1) << 32) | u64::from(lo);
            return ((ticks.wrapping_mul(u64::from(multiplier))) >> 24) as ULONG;
        }
        core::hint::spin_loop();
    }
}

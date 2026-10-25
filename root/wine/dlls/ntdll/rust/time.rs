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

//! Kernel-wide fixed-bias time-zone state used by RTL local/system conversions.
//! Windows convention: local time + bias = UTC; positive bias means west of UTC.
//! Dynamic daylight-saving transitions are not implemented yet.
use core::sync::atomic::{AtomicI32,Ordering};
static BIAS_MINUTES:AtomicI32=AtomicI32::new(0);
/// Configure Windows-style time-zone bias in minutes.
pub(super) fn set_bias_minutes(bias:i32)->bool{if !(-1440..=1440).contains(&bias){return false;}BIAS_MINUTES.store(bias,Ordering::Release);true}
pub(super) fn bias_minutes()->i32{BIAS_MINUTES.load(Ordering::Acquire)}
pub(super) fn local_to_system(local:i64)->Option<i64>{local.checked_add((bias_minutes() as i64).checked_mul(600_000_000)?)}
pub(super) fn system_to_local(system:i64)->Option<i64>{system.checked_sub((bias_minutes() as i64).checked_mul(600_000_000)?)}
#[cfg(test)]mod tests{use super::*;#[test]fn bias_round_trips_and_checks_range(){assert!(set_bias_minutes(300));let local=132_000_000_000_000_000i64;let utc=local_to_system(local).unwrap();assert_eq!(system_to_local(utc),Some(local));assert!(!set_bias_minutes(2000));assert!(set_bias_minutes(0));}}

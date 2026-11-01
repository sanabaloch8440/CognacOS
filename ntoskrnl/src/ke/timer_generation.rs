// Shared timer-expiry generation predicate. Kept standalone so CI can run
// these focused regression tests on the host without building the freestanding
// kernel or emulating hardware.
#[inline]
fn timer_snapshot_is_current(
    snapshot_object: usize,
    snapshot_generation: u64,
    current_object: usize,
    current_generation: u64,
) -> bool {
    snapshot_object != 0
        && snapshot_generation != 0
        && snapshot_object == current_object
        && snapshot_generation == current_generation
}

/// Advance a timer generation without ever producing the reserved zero value.
#[inline]
fn next_timer_generation(current: u64) -> u64 {
    current.wrapping_add(1).max(1)
}

#[cfg(test)]
mod tests {
    use super::{next_timer_generation, timer_snapshot_is_current};

    #[test]
    fn expiry_snapshot_requires_same_object_and_generation() {
        assert!(timer_snapshot_is_current(0x1000, 7, 0x1000, 7));
        assert!(!timer_snapshot_is_current(0x1000, 7, 0x1000, 8));
        assert!(!timer_snapshot_is_current(0x1000, 7, 0x2000, 7));
    }

    #[test]
    fn invalidated_or_uninitialized_snapshot_is_rejected() {
        assert!(!timer_snapshot_is_current(0, 1, 0, 1));
        assert!(!timer_snapshot_is_current(0x1000, 0, 0x1000, 0));
        // A cancel/rearm advances the live generation beyond the snapshot.
        assert!(!timer_snapshot_is_current(0x1000, 41, 0x1000, 42));
    }

    #[test]
    fn generation_increment_skips_reserved_zero_after_wraparound() {
        assert_eq!(next_timer_generation(0), 1);
        assert_eq!(next_timer_generation(41), 42);
        assert_eq!(next_timer_generation(u64::MAX), 1);
    }
}

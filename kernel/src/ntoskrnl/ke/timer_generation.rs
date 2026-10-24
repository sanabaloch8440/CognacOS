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

#[cfg(test)]
mod tests {
    use super::timer_snapshot_is_current;

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
}

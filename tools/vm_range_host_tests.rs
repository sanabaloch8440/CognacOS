//! Host-side regression tests for the exact user-range validator used by the kernel.
#[path = "../kernel/src/vm_range.rs"]
mod vm_range;

#[test]
fn rejects_kernel_and_lower_non_user_addresses() {
    assert!(!vm_range::is_user_range(0, 1));
    assert!(!vm_range::is_user_range(vm_range::USER_START - 1, 1));
    assert!(!vm_range::is_user_range(vm_range::USER_END, 1));
    assert!(!vm_range::is_user_range(u64::MAX, 1));
}

#[test]
fn accepts_valid_ranges_and_exact_upper_boundary() {
    assert!(vm_range::is_user_range(vm_range::USER_START, 1));
    assert!(vm_range::is_user_range(vm_range::USER_START, 0));
    assert!(vm_range::is_user_range(vm_range::USER_END - 1, 1));
    assert!(vm_range::is_user_range(vm_range::USER_END - 4096, 4096));
}

#[test]
fn rejects_overflow_and_ranges_crossing_user_limit() {
    assert!(!vm_range::is_user_range(vm_range::USER_END - 1, 2));
    assert!(!vm_range::is_user_range(vm_range::USER_START, u64::MAX));
    assert!(!vm_range::is_user_range(u64::MAX - 7, 16));
}

#[test]
fn zero_length_still_requires_an_in_range_pointer() {
    assert!(!vm_range::is_user_range(vm_range::USER_START - 1, 0));
    assert!(!vm_range::is_user_range(vm_range::USER_END, 0));
}

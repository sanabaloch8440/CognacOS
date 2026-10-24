//! Shared user virtual-address range validation used by the kernel and host tests.

pub const USER_START: u64 = 0x0000_0080_0000_0000;
pub const USER_END: u64 = 0x0000_8000_0000_0000;

/// Validate a half-open user virtual range without allowing integer overflow.
/// A zero-length range is accepted only when its address itself is inside the user interval.
pub fn is_user_range(address: u64, length: u64) -> bool {
    if length == 0 { return address >= USER_START && address < USER_END; }
    address >= USER_START
        && address < USER_END
        && address.checked_add(length).is_some_and(|end| end <= USER_END)
}

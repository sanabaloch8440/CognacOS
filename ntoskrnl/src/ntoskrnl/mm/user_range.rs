//! Canonical user virtual-address range checks shared by the MM and host tests.
pub const USER_START: u64 = 0x0000_0080_0000_0000;
pub const USER_END: u64 = 0x0000_8000_0000_0000;
/// Windows x64 user mapping for KUSER_SHARED_DATA (read-only to user mode).
pub const KUSER_SHARED_DATA_ADDRESS: u64 = 0x0000_0000_7ffe_0000;
pub const KUSER_SHARED_DATA_SIZE: u64 = 4096;

/// Validate a half-open user virtual range without allowing integer overflow.
/// A zero-length range is accepted only when its address itself is inside the user interval.
pub fn is_user_range(address: u64, length: u64) -> bool {
    let Some(end) = address.checked_add(length) else { return false; };
    if length == 0 {
        return (address >= USER_START && address < USER_END)
            || (address >= KUSER_SHARED_DATA_ADDRESS
                && address < KUSER_SHARED_DATA_ADDRESS + KUSER_SHARED_DATA_SIZE);
    }
    (address >= USER_START && address < USER_END && end <= USER_END)
        || (address >= KUSER_SHARED_DATA_ADDRESS
            && end <= KUSER_SHARED_DATA_ADDRESS + KUSER_SHARED_DATA_SIZE)
}

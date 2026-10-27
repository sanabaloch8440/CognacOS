//! NTSTATUS values shared across kernel, native libraries, and drivers.

pub type NTSTATUS = i32;

pub const STATUS_SUCCESS: NTSTATUS = 0x0000_0000;
pub const STATUS_PENDING: NTSTATUS = 0x0000_0103;
pub const STATUS_TIMEOUT: NTSTATUS = 0x0000_0102;
pub const STATUS_UNSUCCESSFUL: NTSTATUS = 0xC000_0001u32 as i32;
pub const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
pub const STATUS_INVALID_INFO_CLASS: NTSTATUS = 0xC000_0003u32 as i32;
pub const STATUS_INFO_LENGTH_MISMATCH: NTSTATUS = 0xC000_0004u32 as i32;
pub const STATUS_ACCESS_VIOLATION: NTSTATUS = 0xC000_0005u32 as i32;
pub const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008u32 as i32;
pub const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
pub const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
pub const STATUS_ACCESS_DENIED: NTSTATUS = 0xC000_0022u32 as i32;
pub const STATUS_OBJECT_NAME_NOT_FOUND: NTSTATUS = 0xC000_0034u32 as i32;
pub const STATUS_OBJECT_NAME_COLLISION: NTSTATUS = 0xC000_0035u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
pub const STATUS_BUFFER_OVERFLOW: NTSTATUS = 0x8000_0005u32 as i32;

/// NT_SUCCESS(Status): success and informational status values have a clear sign bit.
#[inline]
pub const fn nt_success(status: NTSTATUS) -> bool {
    status >= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_success_uses_signed_ntstatus_semantics() {
        assert!(nt_success(STATUS_SUCCESS));
        assert!(nt_success(STATUS_PENDING));
        assert!(!nt_success(STATUS_ACCESS_DENIED));
        assert!(!nt_success(STATUS_NOT_IMPLEMENTED));
    }
}

//! Bounded Security Reference Monitor primitives for NT-style self-relative descriptors.
//!
//! This module deliberately implements only routines whose ABI and semantics are
//! sufficiently constrained. Token privilege/impersonation, SACL policy, and the
//! full Windows access-state contract are not emulated by permissive stubs.
use core::ffi::c_void;

pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}
fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

fn sid_len(b: &[u8], at: usize) -> Option<usize> {
    let head = b.get(at..at.checked_add(8)?)?;
    if head[0] != 1 || head[1] > 15 { return None; }
    let n = 8usize.checked_add((head[1] as usize).checked_mul(4)?)?;
    b.get(at..at.checked_add(n)?)?;
    Some(n)
}

fn acl_len(b: &[u8], at: usize) -> Option<usize> {
    let head = b.get(at..at.checked_add(8)?)?;
    if head[0] > 4 { return None; }
    let size = u16le(b, at + 2)? as usize;
    let count = u16le(b, at + 4)? as usize;
    let end = at.checked_add(size)?;
    if size < 8 || end > b.len() { return None; }
    let mut p = at + 8;
    for _ in 0..count {
        let ace = b.get(p..p.checked_add(4)?)?;
        let ace_size = u16le(ace, 2)? as usize;
        if ace_size < 4 || p.checked_add(ace_size)? > end { return None; }
        // For basic allow/deny/audit/alarm ACEs, validate the SID after the mask.
        if matches!(ace[0], 0 | 1 | 2 | 3) {
            if ace_size < 16 { return None; }
            let sid_at = p.checked_add(8)?;
            let actual = sid_len(b, sid_at)?;
            if 8usize.checked_add(actual)? > ace_size { return None; }
        }
        p += ace_size;
    }
    if p > end { return None; }
    Some(size)
}

/// Validate a bounded Windows self-relative SECURITY_DESCRIPTOR.
/// Returns FALSE for null, absolute, malformed, overlapping, or out-of-range data.
/// The caller must provide readable storage for the descriptor and all referenced
/// components; this routine can validate offsets but cannot probe arbitrary memory.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeValidSecurityDescriptor(descriptor: *const c_void) -> u8 {
    if descriptor.is_null() { return 0; }
    // NT security descriptors are variable-sized; the public ABI supplies no length.
    // Restrict inspection to a conservative 4-KiB window and reject offsets outside it.
    let b = unsafe { core::slice::from_raw_parts(descriptor.cast::<u8>(), 4096) };
    if b[0] != 1 { return 0; }
    let control = match u16le(b, 2) { Some(v) => v, None => return 0 };
    if control & 0x8000 == 0 { return 0; } // SE_SELF_RELATIVE is required here.
    let mut ranges: [(usize, usize); 4] = [(0, 0); 4];
    let mut count = 0usize;
    for (field, kind) in [(4usize, 0u8), (8, 0), (12, 1), (16, 1)] {
        let off = match u32le(b, field) { Some(v) => v as usize, None => return 0 };
        if off == 0 { continue; }
        if off < 20 || off >= b.len() { return 0; }
        let len = if kind == 0 { match sid_len(b, off) { Some(v) => v, None => return 0 } }
                   else { match acl_len(b, off) { Some(v) => v, None => return 0 } };
        let end = match off.checked_add(len) { Some(v) if v <= b.len() => v, _ => return 0 };
        ranges[count] = (off, end); count += 1;
    }
    // Aliasing components is malformed; otherwise later copies could reinterpret data.
    for i in 0..count { for j in (i + 1)..count {
        if ranges[i].0 < ranges[j].1 && ranges[j].0 < ranges[i].1 { return 0; }
    }}
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_minimal_self_relative_descriptor() {
        let mut sd = [0u8; 20]; sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8000u16.to_le_bytes());
        assert_eq!(unsafe { SeValidSecurityDescriptor(sd.as_ptr().cast()) }, 1);
        sd[2..4].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(unsafe { SeValidSecurityDescriptor(sd.as_ptr().cast()) }, 0);
    }
    #[test]
    fn rejects_bad_owner_offset_and_sid_revision() {
        let mut sd = [0u8; 64]; sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8000u16.to_le_bytes());
        sd[4..8].copy_from_slice(&20u32.to_le_bytes());
        sd[20] = 2; sd[21] = 1;
        assert_eq!(unsafe { SeValidSecurityDescriptor(sd.as_ptr().cast()) }, 0);
        sd[4..8].copy_from_slice(&4095u32.to_le_bytes());
        assert_eq!(unsafe { SeValidSecurityDescriptor(sd.as_ptr().cast()) }, 0);
    }
}

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
        let mut sd = [0u8; 4096]; sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8000u16.to_le_bytes());
        assert_eq!(unsafe { SeValidSecurityDescriptor(sd.as_ptr().cast()) }, 1);
        sd[2..4].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(unsafe { SeValidSecurityDescriptor(sd.as_ptr().cast()) }, 0);
    }
    #[test]
    fn rejects_bad_owner_offset_and_sid_revision() {
        let mut sd = [0u8; 4096]; sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8000u16.to_le_bytes());
        sd[4..8].copy_from_slice(&20u32.to_le_bytes());
        sd[20] = 2; sd[21] = 1;
        assert_eq!(unsafe { SeValidSecurityDescriptor(sd.as_ptr().cast()) }, 0);
        sd[4..8].copy_from_slice(&4095u32.to_le_bytes());
        assert_eq!(unsafe { SeValidSecurityDescriptor(sd.as_ptr().cast()) }, 0);
    }
}


// Bounded RdotOS token metadata, pending integration with real Ps token objects.
// Unknown token pointers fail closed; this is not the private Windows TOKEN layout.
use core::sync::atomic::{AtomicBool, Ordering};
const TOKEN_META_SLOTS: usize = 32;
#[derive(Clone, Copy)]
struct TokenMeta {
    used: bool, token: usize, token_type: u32, impersonation_level: u32,
    admin: bool, restricted: bool, write_restricted: bool, privileges: u64, session_id: u32, authentication_id: u64,
}
impl TokenMeta {
    const EMPTY: Self = Self { used:false, token:0, token_type:0, impersonation_level:0,
        admin:false, restricted:false, write_restricted:false, privileges:0, session_id:0, authentication_id:0 };
}
struct TokenMetaLock(AtomicBool);
impl TokenMetaLock {
    const fn new() -> Self { Self(AtomicBool::new(false)) }
    fn lock(&self) -> TokenMetaGuard<'_> {
        while self.0.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        TokenMetaGuard(&self.0)
    }
}
struct TokenMetaGuard<'a>(&'a AtomicBool);
impl Drop for TokenMetaGuard<'_> { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }
static TOKEN_META_LOCK: TokenMetaLock = TokenMetaLock::new();
static mut TOKEN_META: [TokenMeta; TOKEN_META_SLOTS] = [TokenMeta::EMPTY; TOKEN_META_SLOTS];

/// Register RdotOS-owned token facts. token_type: 1=primary, 2=impersonation.
/// The token must remain alive until explicitly unregistered.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeRegisterTokenMetadata(
    token: *mut c_void, token_type: u32, impersonation_level: u32,
    flags: u32, privileges: u64, session_id: u32, authentication_id: u64,
) -> i32 {
    if token.is_null() || !(token_type == 1 || token_type == 2)
        || (token_type == 1 && impersonation_level != 0)
        || (token_type == 2 && impersonation_level > 3) || flags & !7 != 0 {
        return STATUS_INVALID_PARAMETER;
    }
    let _guard = TOKEN_META_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKEN_META) };
    let slot = entries.iter().position(|e| e.used && e.token == token as usize)
        .or_else(|| entries.iter().position(|e| !e.used));
    let Some(i) = slot else { return 0xC000_009Au32 as i32; };
    entries[i] = TokenMeta { used:true, token:token as usize, token_type,
        impersonation_level, admin:flags & 1 != 0, restricted:flags & 2 != 0,
        write_restricted:flags & 4 != 0, privileges, session_id, authentication_id };
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeUnregisterTokenMetadata(token: *mut c_void) -> i32 {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = TOKEN_META_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKEN_META) };
    if let Some(e) = entries.iter_mut().find(|e| e.used && e.token == token as usize) {
        *e = TokenMeta::EMPTY; STATUS_SUCCESS
    } else { 0xC000_0008u32 as i32 }
}
fn token_meta(token: *mut c_void) -> Option<TokenMeta> {
    if token.is_null() { return None; }
    let _guard = TOKEN_META_LOCK.lock();
    let entries = unsafe { &*core::ptr::addr_of!(TOKEN_META) };
    entries.iter().find(|e| e.used && e.token == token as usize).copied()
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeTokenIsAdmin(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.admin) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeTokenIsRestricted(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.restricted) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeTokenIsWriteRestricted(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.write_restricted) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeTokenType(token: *mut c_void) -> u32 {
    token_meta(token).map_or(0, |m| m.token_type)
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeTokenImpersonationLevel(token: *mut c_void) -> u32 {
    token_meta(token).filter(|m| m.token_type == 2).map_or(0, |m| m.impersonation_level)
}

/// Without a current-thread token binding, user-mode privilege checks deny by
/// default. Kernel-mode checks retain the NT privilege-check bypass behavior.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeSinglePrivilegeCheck(_privilege: u64, previous_mode: i8) -> u8 {
    if previous_mode == 0 { 1 } else { 0 }
}

/// RdotOS extension for checking a privilege bit on an explicitly registered token.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenHasPrivilege(token: *mut c_void, privilege: u32) -> u8 {
    if privilege >= 64 { return 0; }
    token_meta(token).is_some_and(|m| (m.privileges & (1u64 << privilege)) != 0) as u8
}

#[cfg(test)]
mod token_metadata_tests {
    use super::*;
    #[test]
    fn token_metadata_queries_fail_closed_and_round_trip() {
        let mut storage = 0u64;
        let token = (&mut storage as *mut u64).cast::<c_void>();
        assert_eq!(unsafe { SeTokenIsAdmin(token) }, 0);
        assert_eq!(unsafe { RdotSeRegisterTokenMetadata(token, 2, 2, 0b011, 1 << 9, 7, 0x1234) }, STATUS_SUCCESS);
        assert_eq!(unsafe { SeTokenIsAdmin(token) }, 1);
        assert_eq!(unsafe { SeTokenIsRestricted(token) }, 1);
        assert_eq!(unsafe { SeTokenIsWriteRestricted(token) }, 0);
        assert_eq!(unsafe { SeTokenType(token) }, 2);
        assert_eq!(unsafe { SeTokenImpersonationLevel(token) }, 2);
        assert_eq!(unsafe { RdotSeTokenHasPrivilege(token, 9) }, 1);
        assert_eq!(unsafe { RdotSeTokenHasPrivilege(token, 10) }, 0);
        let mut session = 0u32;
        let mut auth = 0u64;
        assert_eq!(unsafe { SeQuerySessionIdToken(token, &mut session) }, STATUS_SUCCESS);
        assert_eq!(session, 7);
        assert_eq!(unsafe { SeQuerySessionIdTokenEx(token, &mut session) }, STATUS_SUCCESS);
        assert_eq!(unsafe { SeQueryAuthenticationIdToken(token, &mut auth) }, STATUS_SUCCESS);
        assert_eq!(auth, 0x1234);
        assert_eq!(unsafe { RdotSeUnregisterTokenMetadata(token) }, STATUS_SUCCESS);
        assert_eq!(unsafe { SeTokenType(token) }, 0);
    }
    #[test]
    fn single_privilege_check_is_fail_closed_without_thread_token() {
        assert_eq!(unsafe { SeSinglePrivilegeCheck(3, 1) }, 0);
        assert_eq!(unsafe { SeSinglePrivilegeCheck(3, 0) }, 1);
    }
}


#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeQuerySessionIdToken(token: *mut c_void, session_id: *mut u32) -> i32 {
    if session_id.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(session_id, meta.session_id); }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeQuerySessionIdTokenEx(token: *mut c_void, session_id: *mut u32) -> i32 {
    unsafe { SeQuerySessionIdToken(token, session_id) }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeQueryAuthenticationIdToken(token: *mut c_void, authentication_id: *mut u64) -> i32 {
    if authentication_id.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(authentication_id, meta.authentication_id); }
    STATUS_SUCCESS
}


// --- Additional bounded RdotOS security primitives ---
// These are internal extension APIs, not Windows-export ABI replacements.
// All operations fail closed for unknown tokens and avoid dereferencing token pointers.
fn token_flags(token: *mut c_void) -> Option<u32> {
    token_meta(token).map(|m| (m.admin as u32) | ((m.restricted as u32) << 1) |
        ((m.write_restricted as u32) << 2) | (((m.token_type == 2) as u32) << 3))
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenIsPrimary(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.token_type == 1) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenIsImpersonation(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.token_type == 2) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenIsKnown(token: *mut c_void) -> u8 {
    token_meta(token).is_some() as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenQueryFlags(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(flags) = token_flags(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, flags); }
    STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenQueryPrivilegeBitmap(token: *mut c_void, out: *mut u64) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, meta.privileges); }
    STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenHasAnyPrivilege(token: *mut c_void, mask: u64) -> u8 {
    token_meta(token).is_some_and(|m| (m.privileges & mask) != 0) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenHasAllPrivileges(token: *mut c_void, mask: u64) -> u8 {
    token_meta(token).is_some_and(|m| (m.privileges & mask) == mask) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenPrivilegeCount(token: *mut c_void) -> u32 {
    token_meta(token).map_or(0, |m| m.privileges.count_ones())
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenQuerySessionId(token: *mut c_void, out: *mut u32) -> i32 {
    unsafe { SeQuerySessionIdToken(token, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenQueryAuthenticationId(token: *mut c_void, out: *mut u64) -> i32 {
    unsafe { SeQueryAuthenticationIdToken(token, out) }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenQueryImpersonationLevel(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, if meta.token_type == 2 { meta.impersonation_level } else { 0 }); }
    STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenQueryType(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, meta.token_type); }
    STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenHasPrivilegeRange(token: *mut c_void, first: u32, count: u32) -> u8 {
    if count == 0 || first >= 64 || count > 64 || first.saturating_add(count) > 64 { return 0; }
    let mask = if count == 64 { u64::MAX } else { ((1u64 << count) - 1) << first };
    token_meta(token).is_some_and(|m| (m.privileges & mask) == mask) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenHasNoPrivileges(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.privileges == 0) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenIsSession(token: *mut c_void, session: u32) -> u8 {
    token_meta(token).is_some_and(|m| m.session_id == session) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenIsLogonSession(token: *mut c_void, luid: u64) -> u8 {
    token_meta(token).is_some_and(|m| m.authentication_id == luid) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenCanImpersonate(token: *mut c_void, required_level: u32) -> u8 {
    if required_level > 3 { return 0; }
    token_meta(token).is_some_and(|m| m.token_type == 2 && m.impersonation_level >= required_level) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenPrivilegeMaskContains(token: *mut c_void, mask: u64) -> u8 {
    token_meta(token).is_some_and(|m| (m.privileges & mask) == mask) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenQueryPrivilegeCount(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, meta.privileges.count_ones()); }
    STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenQueryRestrictions(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    let flags = ((meta.restricted as u32) | ((meta.write_restricted as u32) << 1));
    unsafe { core::ptr::write_unaligned(out, flags); }
    STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeTokenIsAdminAndUnrestricted(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.admin && !m.restricted && !m.write_restricted) as u8
}


#[cfg(test)]
mod extended_token_tests {
    use super::*;
    #[test]
    fn extended_token_queries_validate_and_round_trip() {
        let mut storage = 0u64;
        let token = (&mut storage as *mut u64).cast::<c_void>();
        assert_eq!(unsafe { RdotSeTokenIsKnown(token) }, 0);
        assert_eq!(unsafe { RdotSeTokenQueryFlags(token, core::ptr::null_mut()) }, STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe { RdotSeRegisterTokenMetadata(token, 2, 2, 0b001, 0b10110, 12, 0x5678) }, STATUS_SUCCESS);
        assert_eq!(unsafe { RdotSeTokenIsPrimary(token) }, 0);
        assert_eq!(unsafe { RdotSeTokenIsImpersonation(token) }, 1);
        assert_eq!(unsafe { RdotSeTokenIsKnown(token) }, 1);
        assert_eq!(unsafe { RdotSeTokenHasAnyPrivilege(token, 0b00010) }, 1);
        assert_eq!(unsafe { RdotSeTokenHasAllPrivileges(token, 0b10100) }, 1);
        assert_eq!(unsafe { RdotSeTokenHasAllPrivileges(token, 0b11100) }, 0);
        assert_eq!(unsafe { RdotSeTokenPrivilegeCount(token) }, 3);
        assert_eq!(unsafe { RdotSeTokenHasPrivilegeRange(token, 1, 2) }, 1);
        assert_eq!(unsafe { RdotSeTokenHasPrivilegeRange(token, 63, 2) }, 0);
        assert_eq!(unsafe { RdotSeTokenIsSession(token, 12) }, 1);
        assert_eq!(unsafe { RdotSeTokenIsLogonSession(token, 0x5678) }, 1);
        assert_eq!(unsafe { RdotSeTokenCanImpersonate(token, 2) }, 1);
        assert_eq!(unsafe { RdotSeTokenCanImpersonate(token, 3) }, 0);
        assert_eq!(unsafe { RdotSeTokenIsAdminAndUnrestricted(token) }, 1);
        assert_eq!(unsafe { RdotSeUnregisterTokenMetadata(token) }, STATUS_SUCCESS);
    }
}


// --- Length-aware security descriptor primitives (RdotOS extension ABI) ---
// These helpers deliberately take explicit lengths. Unlike the Windows Se ABI,
// they do not assume that caller memory is readable beyond the supplied span.
// They are not substitutes for a kernel probe/capture routine.
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
const STATUS_DATA_ERROR: i32 = 0xC000_003Eu32 as i32;

fn bounded_sid_size(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 8 || bytes[0] != 1 || bytes[1] > 15 { return None; }
    let n = 8usize.checked_add((bytes[1] as usize).checked_mul(4)?)?;
    (n <= bytes.len()).then_some(n)
}

fn bounded_acl_size(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 8 || bytes[0] > 4 { return None; }
    let size = u16le(bytes, 2)? as usize;
    let count = u16le(bytes, 4)? as usize;
    if size < 8 || size > bytes.len() { return None; }
    let mut cursor = 8usize;
    for _ in 0..count {
        let head = bytes.get(cursor..cursor.checked_add(4)?)?;
        let ace_size = u16le(head, 2)? as usize;
        if ace_size < 4 || cursor.checked_add(ace_size)? > size { return None; }
        // For common SID-bearing ACEs, verify the SID fits wholly inside the ACE.
        if matches!(head[0], 0 | 1 | 2 | 3) {
            if ace_size < 16 { return None; }
            let sid_start = cursor.checked_add(8)?;
            let sid = bytes.get(sid_start..cursor.checked_add(ace_size)?)?;
            if bounded_sid_size(sid).is_none() { return None; }
        }
        cursor = cursor.checked_add(ace_size)?;
    }
    (cursor <= size).then_some(size)
}

/// Validate one SID in an explicitly bounded readable buffer. Returns its byte length, or 0.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeValidateSid(sid: *const u8, length: usize) -> usize {
    if sid.is_null() || length < 8 { return 0; }
    let bytes = unsafe { core::slice::from_raw_parts(sid, length) };
    bounded_sid_size(bytes).unwrap_or(0)
}

/// Compare two explicitly bounded SID buffers; only exact valid SID encodings compare equal.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeSidEqual(
    left: *const u8, left_len: usize, right: *const u8, right_len: usize,
) -> u8 {
    if left.is_null() || right.is_null() || left_len < 8 || right_len < 8 { return 0; }
    let a = unsafe { core::slice::from_raw_parts(left, left_len) };
    let b = unsafe { core::slice::from_raw_parts(right, right_len) };
    match (bounded_sid_size(a), bounded_sid_size(b)) {
        (Some(na), Some(nb)) => (na == nb && a[..na] == b[..nb]) as u8,
        _ => 0,
    }
}

/// Validate an ACL from a caller-provided bounded buffer; returns ACL size or 0.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeValidateAcl(acl: *const u8, length: usize) -> usize {
    if acl.is_null() || length < 8 { return 0; }
    let bytes = unsafe { core::slice::from_raw_parts(acl, length) };
    bounded_acl_size(bytes).unwrap_or(0)
}

/// Read a self-relative descriptor's control word from an explicit bounded span.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeQueryDescriptorControl(
    descriptor: *const u8, length: usize, control_out: *mut u16,
) -> i32 {
    if descriptor.is_null() || control_out.is_null() { return STATUS_INVALID_PARAMETER; }
    if length < 20 { return STATUS_BUFFER_TOO_SMALL; }
    let bytes = unsafe { core::slice::from_raw_parts(descriptor, length) };
    if bytes[0] != 1 { return STATUS_DATA_ERROR; }
    let control = u16le(bytes, 2).unwrap_or(0);
    if control & 0x8000 == 0 { return STATUS_DATA_ERROR; }
    unsafe { core::ptr::write_unaligned(control_out, control); }
    STATUS_SUCCESS
}

/// Return a component's offset and validated length. component: 0 owner, 1 group,
/// 2 SACL, 3 DACL. Zero offset means absent and yields STATUS_SUCCESS with length 0.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeQueryDescriptorComponent(
    descriptor: *const u8, descriptor_len: usize, component: u32,
    offset_out: *mut u32, length_out: *mut u32,
) -> i32 {
    if descriptor.is_null() || offset_out.is_null() || length_out.is_null() || component > 3 {
        return STATUS_INVALID_PARAMETER;
    }
    if descriptor_len < 20 { return STATUS_BUFFER_TOO_SMALL; }
    let bytes = unsafe { core::slice::from_raw_parts(descriptor, descriptor_len) };
    if bytes[0] != 1 { return STATUS_DATA_ERROR; }
    let control = u16le(bytes, 2).unwrap_or(0);
    if control & 0x8000 == 0 { return STATUS_DATA_ERROR; }
    let field = [4usize, 8, 12, 16][component as usize];
    let off = u32le(bytes, field).ok_or(STATUS_DATA_ERROR).unwrap_or(0) as usize;
    if off == 0 {
        unsafe { core::ptr::write_unaligned(offset_out, 0); core::ptr::write_unaligned(length_out, 0); }
        return STATUS_SUCCESS;
    }
    if off < 20 || off >= descriptor_len { return STATUS_DATA_ERROR; }
    let size = if component < 2 {
        bounded_sid_size(&bytes[off..]).ok_or(STATUS_DATA_ERROR)
    } else {
        bounded_acl_size(&bytes[off..]).ok_or(STATUS_DATA_ERROR)
    };
    let size = match size { Ok(n) => n, Err(s) => return s };
    let Some(end) = off.checked_add(size) else { return STATUS_DATA_ERROR; };
    if end > descriptor_len || end > u32::MAX as usize || off > u32::MAX as usize {
        return STATUS_DATA_ERROR;
    }
    unsafe {
        core::ptr::write_unaligned(offset_out, off as u32);
        core::ptr::write_unaligned(length_out, size as u32);
    }
    STATUS_SUCCESS
}

/// Check that an explicit privilege bitmap contains every requested privilege bit.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSePrivilegeBitmapSatisfies(available: u64, required: u64) -> u8 {
    ((available & required) == required) as u8
}

/// Check a bounded access mask using explicit allowed/denied masks. Deny always wins.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeAccessMaskAllowed(
    requested: u32, allowed: u32, denied: u32,
) -> u8 {
    (requested & denied == 0 && requested & allowed == requested) as u8
}

#[cfg(test)]
mod bounded_security_tests {
    use super::*;
    #[test]
    fn bounded_sid_and_acl_validation_reject_truncation() {
        let sid = [1u8, 1, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0];
        assert_eq!(unsafe { RdotSeValidateSid(sid.as_ptr(), sid.len()) }, 12);
        assert_eq!(unsafe { RdotSeValidateSid(sid.as_ptr(), 8) }, 0);
        assert_eq!(unsafe { RdotSeSidEqual(sid.as_ptr(), 12, sid.as_ptr(), 12) }, 1);
        assert_eq!(unsafe { RdotSeSidEqual(sid.as_ptr(), 8, sid.as_ptr(), 12) }, 0);
        let acl = [2u8, 0, 8, 0, 0, 0, 0, 0];
        assert_eq!(unsafe { RdotSeValidateAcl(acl.as_ptr(), acl.len()) }, 8);
        assert_eq!(unsafe { RdotSeValidateAcl(acl.as_ptr(), 7) }, 0);
    }
    #[test]
    fn descriptor_component_query_is_length_aware() {
        let mut sd = [0u8; 32];
        sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8000u16.to_le_bytes());
        sd[4..8].copy_from_slice(&20u32.to_le_bytes());
        sd[20..32].copy_from_slice(&[1, 1, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0]);
        let mut off = 99u32;
        let mut len = 99u32;
        assert_eq!(unsafe { RdotSeQueryDescriptorComponent(sd.as_ptr(), sd.len(), 0, &mut off, &mut len) }, STATUS_SUCCESS);
        assert_eq!((off, len), (20, 12));
        assert_eq!(unsafe { RdotSeQueryDescriptorComponent(sd.as_ptr(), 24, 0, &mut off, &mut len) }, STATUS_DATA_ERROR);
        assert_eq!(unsafe { RdotSeQueryDescriptorControl(sd.as_ptr(), 19, &mut 0u16) }, STATUS_BUFFER_TOO_SMALL);
    }
    #[test]
    fn privilege_and_access_mask_helpers_are_conservative() {
        assert_eq!(unsafe { RdotSePrivilegeBitmapSatisfies(0b1110, 0b0110) }, 1);
        assert_eq!(unsafe { RdotSePrivilegeBitmapSatisfies(0b0010, 0b0110) }, 0);
        assert_eq!(unsafe { RdotSeAccessMaskAllowed(0b011, 0b111, 0) }, 1);
        assert_eq!(unsafe { RdotSeAccessMaskAllowed(0b011, 0b111, 0b001) }, 0);
    }
}


// Further bounded inspection helpers used by the future Se access-check path.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeSidRevision(sid: *const u8, length: usize) -> u32 {
    if sid.is_null() || length < 8 { return u32::MAX; }
    let b = unsafe { core::slice::from_raw_parts(sid, length) };
    if bounded_sid_size(b).is_some() { b[0] as u32 } else { u32::MAX }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeSidSubAuthorityCount(sid: *const u8, length: usize) -> u32 {
    if sid.is_null() || length < 8 { return u32::MAX; }
    let b = unsafe { core::slice::from_raw_parts(sid, length) };
    if bounded_sid_size(b).is_some() { b[1] as u32 } else { u32::MAX }
}
/// Outputs the six-byte SID identifier authority as a numeric value.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeSidIdentifierAuthority(
    sid: *const u8, length: usize, authority_out: *mut u64,
) -> i32 {
    if sid.is_null() || authority_out.is_null() { return STATUS_INVALID_PARAMETER; }
    if length < 8 { return STATUS_BUFFER_TOO_SMALL; }
    let b = unsafe { core::slice::from_raw_parts(sid, length) };
    if bounded_sid_size(b).is_none() { return STATUS_DATA_ERROR; }
    let mut value = 0u64;
    for byte in &b[2..8] { value = (value << 8) | (*byte as u64); }
    unsafe { core::ptr::write_unaligned(authority_out, value); }
    STATUS_SUCCESS
}
/// Query ACL revision, ACE count and ACL byte size. All outputs are optional.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeQueryAclInfo(
    acl: *const u8, length: usize, revision_out: *mut u32,
    ace_count_out: *mut u32, size_out: *mut u32,
) -> i32 {
    if acl.is_null() { return STATUS_INVALID_PARAMETER; }
    if length < 8 { return STATUS_BUFFER_TOO_SMALL; }
    let b = unsafe { core::slice::from_raw_parts(acl, length) };
    let Some(size) = bounded_acl_size(b) else { return STATUS_DATA_ERROR; };
    if size > u32::MAX as usize { return STATUS_DATA_ERROR; }
    unsafe {
        if !revision_out.is_null() { core::ptr::write_unaligned(revision_out, b[0] as u32); }
        if !ace_count_out.is_null() { core::ptr::write_unaligned(ace_count_out, u16le(b, 4).unwrap_or(0) as u32); }
        if !size_out.is_null() { core::ptr::write_unaligned(size_out, size as u32); }
    }
    STATUS_SUCCESS
}
/// Count ACEs of a given type in a bounded, structurally valid ACL; u32::MAX means invalid.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeCountAceType(acl: *const u8, length: usize, ace_type: u32) -> u32 {
    if acl.is_null() || ace_type > u8::MAX as u32 || length < 8 { return u32::MAX; }
    let b = unsafe { core::slice::from_raw_parts(acl, length) };
    let Some(size) = bounded_acl_size(b) else { return u32::MAX; };
    let count = u16le(b, 4).unwrap_or(0) as usize;
    let mut cursor = 8usize;
    let mut found = 0u32;
    for _ in 0..count {
        let Some(head) = b.get(cursor..cursor + 4) else { return u32::MAX; };
        let ace_size = u16le(head, 2).unwrap_or(0) as usize;
        if head[0] as u32 == ace_type { found = found.saturating_add(1); }
        cursor = match cursor.checked_add(ace_size) { Some(v) if v <= size => v, _ => return u32::MAX };
    }
    found
}
/// True if a valid basic SID-bearing ACE in the ACL names the supplied valid SID.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeAclContainsSid(
    acl: *const u8, acl_len: usize, sid: *const u8, sid_len: usize,
) -> u8 {
    if acl.is_null() || sid.is_null() || acl_len < 8 || sid_len < 8 { return 0; }
    let a = unsafe { core::slice::from_raw_parts(acl, acl_len) };
    let s = unsafe { core::slice::from_raw_parts(sid, sid_len) };
    let Some(sid_size) = bounded_sid_size(s) else { return 0; };
    let Some(acl_size) = bounded_acl_size(a) else { return 0; };
    let count = u16le(a, 4).unwrap_or(0) as usize;
    let mut cursor = 8usize;
    for _ in 0..count {
        let Some(head) = a.get(cursor..cursor + 4) else { return 0; };
        let ace_size = u16le(head, 2).unwrap_or(0) as usize;
        if matches!(head[0], 0 | 1 | 2 | 3) && ace_size >= 16 {
            let start = cursor + 8;
            let end = cursor + ace_size;
            if let Some(candidate) = a.get(start..end) {
                if let Some(n) = bounded_sid_size(candidate) {
                    if n == sid_size && candidate[..n] == s[..sid_size] { return 1; }
                }
            }
        }
        cursor = match cursor.checked_add(ace_size) { Some(v) if v <= acl_size => v, _ => return 0 };
    }
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeDescriptorIsSelfRelative(descriptor: *const u8, length: usize) -> u8 {
    if descriptor.is_null() || length < 20 { return 0; }
    let b = unsafe { core::slice::from_raw_parts(descriptor, length) };
    (b[0] == 1 && u16le(b, 2).is_some_and(|c| c & 0x8000 != 0)) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeDescriptorComponentPresent(
    descriptor: *const u8, length: usize, component: u32,
) -> u8 {
    if descriptor.is_null() || length < 20 || component > 3 { return 0; }
    let b = unsafe { core::slice::from_raw_parts(descriptor, length) };
    if b[0] != 1 || !u16le(b, 2).is_some_and(|c| c & 0x8000 != 0) { return 0; }
    u32le(b, [4usize, 8, 12, 16][component as usize]).is_some_and(|v| v != 0) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSeAccessMaskIsSubset(requested: u32, containing: u32) -> u8 {
    (requested & containing == requested) as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSePrivilegeMaskCount(mask: u64) -> u32 {
    mask.count_ones()
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn RdotSePrivilegeMaskFirst(mask: u64) -> u32 {
    if mask == 0 { u32::MAX } else { mask.trailing_zeros() }
}

#[cfg(test)]
mod bounded_security_inspection_tests {
    use super::*;
    #[test]
    fn sid_authority_and_acl_queries_are_bounded() {
        let sid = [1u8, 1, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0];
        assert_eq!(unsafe { RdotSeSidRevision(sid.as_ptr(), sid.len()) }, 1);
        assert_eq!(unsafe { RdotSeSidSubAuthorityCount(sid.as_ptr(), sid.len()) }, 1);
        let mut authority = 0u64;
        assert_eq!(unsafe { RdotSeSidIdentifierAuthority(sid.as_ptr(), sid.len(), &mut authority) }, STATUS_SUCCESS);
        assert_eq!(authority, 5);
        let acl = [2u8, 0, 8, 0, 0, 0, 0, 0];
        let mut revision = 0;
        let mut count = 9;
        let mut size = 0;
        assert_eq!(unsafe { RdotSeQueryAclInfo(acl.as_ptr(), acl.len(), &mut revision, &mut count, &mut size) }, STATUS_SUCCESS);
        assert_eq!((revision, count, size), (2, 0, 8));
        assert_eq!(unsafe { RdotSeCountAceType(acl.as_ptr(), acl.len(), 0) }, 0);
        assert_eq!(unsafe { RdotSeCountAceType(acl.as_ptr(), 7, 0) }, u32::MAX);
    }
    #[test]
    fn descriptor_and_mask_inspection_handles_invalid_inputs() {
        let mut sd = [0u8; 20];
        sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8000u16.to_le_bytes());
        assert_eq!(unsafe { RdotSeDescriptorIsSelfRelative(sd.as_ptr(), sd.len()) }, 1);
        assert_eq!(unsafe { RdotSeDescriptorComponentPresent(sd.as_ptr(), sd.len(), 0) }, 0);
        assert_eq!(unsafe { RdotSeDescriptorComponentPresent(sd.as_ptr(), sd.len(), 4) }, 0);
        assert_eq!(unsafe { RdotSeAccessMaskIsSubset(0b011, 0b111) }, 1);
        assert_eq!(unsafe { RdotSeAccessMaskIsSubset(0b101, 0b011) }, 0);
        assert_eq!(unsafe { RdotSePrivilegeMaskCount(0b10110) }, 3);
        assert_eq!(unsafe { RdotSePrivilegeMaskFirst(0b10110) }, 1);
        assert_eq!(unsafe { RdotSePrivilegeMaskFirst(0) }, u32::MAX);
    }
}

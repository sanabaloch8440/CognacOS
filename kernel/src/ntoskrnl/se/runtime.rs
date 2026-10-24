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

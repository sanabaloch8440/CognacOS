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


/// Copy a user-mode self-relative security descriptor into kernel-owned storage.
/// Every source page is translated and permission-checked through the currently
/// bound process page tables; no user virtual pointer is directly dereferenced.
pub(crate) unsafe fn se_internal_CaptureUserSecurityDescriptor(
    source: *const c_void, destination: *mut u8, destination_length: usize,
) -> i32 {
    const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005u32 as i32;
    if source.is_null() || destination.is_null() || destination_length < 4096 {
        return STATUS_INVALID_PARAMETER;
    }
    let mut local = [0u8; 4096];
    let mut header = [0u8; 20];
    if unsafe { crate::process::copy_from_current_user(source as u64, &mut header) }.is_err() {
        return STATUS_ACCESS_VIOLATION;
    }
    if header[0] != 1 || u16le(&header, 2).is_none_or(|v| v & 0x8000 == 0) {
        return STATUS_INVALID_PARAMETER;
    }
    local[..20].copy_from_slice(&header);
    for (field, is_acl) in [(4usize, false), (8, false), (12, true), (16, true)] {
        let offset = u32le(&local, field).ok_or(STATUS_INVALID_PARAMETER);
        let offset = match offset { Ok(v) => v as usize, Err(s) => return s };
        if offset == 0 { continue; }
        if offset < 20 || offset.checked_add(8).is_none_or(|end| end > local.len()) {
            return STATUS_INVALID_PARAMETER;
        }
        let Some(src) = (source as u64).checked_add(offset as u64) else { return STATUS_INVALID_PARAMETER; };
        let mut head = [0u8; 8];
        if unsafe { crate::process::copy_from_current_user(src, &mut head) }.is_err() {
            return STATUS_ACCESS_VIOLATION;
        }
        let length = if is_acl {
            let size = u16le(&head, 2).unwrap_or(0) as usize;
            if size < 8 { return STATUS_INVALID_PARAMETER; }
            size
        } else {
            if head[0] != 1 || head[1] > 15 { return STATUS_INVALID_PARAMETER; }
            8 + head[1] as usize * 4
        };
        if offset.checked_add(length).is_none_or(|end| end > local.len()) {
            return STATUS_INVALID_PARAMETER;
        }
        let Some(src) = (source as u64).checked_add(offset as u64) else { return STATUS_INVALID_PARAMETER; };
        if unsafe { crate::process::copy_from_current_user(src, &mut local[offset..offset + length]) }.is_err() {
            return STATUS_ACCESS_VIOLATION;
        }
    }
    if unsafe { SeValidSecurityDescriptor(local.as_ptr().cast()) } == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { core::ptr::copy_nonoverlapping(local.as_ptr(), destination, local.len()); }
    STATUS_SUCCESS
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
    admin: bool, restricted: bool, write_restricted: bool, app_container: bool, privileges: u64, session_id: u32, authentication_id: u64, references: u32,
}
impl TokenMeta {
    const EMPTY: Self = Self { used:false, token:0, token_type:0, impersonation_level:0,
        admin:false, restricted:false, write_restricted:false, app_container:false, privileges:0, session_id:0, authentication_id:0, references:0 };
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
pub(crate) unsafe fn se_internal_RegisterTokenMetadata(
    token: *mut c_void, token_type: u32, impersonation_level: u32,
    flags: u32, privileges: u64, session_id: u32, authentication_id: u64,
) -> i32 {
    if token.is_null() || !(token_type == 1 || token_type == 2)
        || (token_type == 1 && impersonation_level != 0)
        || (token_type == 2 && impersonation_level > 3) || flags & !15 != 0 {
        return STATUS_INVALID_PARAMETER;
    }
    let _guard = TOKEN_META_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKEN_META) };
    let slot = entries.iter().position(|e| e.used && e.token == token as usize)
        .or_else(|| entries.iter().position(|e| !e.used));
    let Some(i) = slot else { return 0xC000_009Au32 as i32; };
    if entries[i].used && entries[i].references != 0 { return 0xC000_009Eu32 as i32; }
    entries[i] = TokenMeta { used:true, token:token as usize, token_type,
        impersonation_level, admin:flags & 1 != 0, restricted:flags & 2 != 0,
        write_restricted:flags & 4 != 0, app_container:flags & 8 != 0, privileges, session_id, authentication_id, references:0 };
    STATUS_SUCCESS
}

pub(crate) unsafe fn se_internal_ReferenceToken(token: *mut c_void) -> i32 {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = TOKEN_META_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKEN_META) };
    let Some(entry) = entries.iter_mut().find(|e| e.used && e.token == token as usize) else {
        return 0xC000_0008u32 as i32;
    };
    let Some(next) = entry.references.checked_add(1) else { return 0xC000_009Au32 as i32; };
    entry.references = next;
    STATUS_SUCCESS
}
pub(crate) unsafe fn se_internal_DereferenceToken(token: *mut c_void) -> i32 {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = TOKEN_META_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKEN_META) };
    let Some(entry) = entries.iter_mut().find(|e| e.used && e.token == token as usize) else {
        return 0xC000_0008u32 as i32;
    };
    if entry.references == 0 { return STATUS_INVALID_PARAMETER; }
    entry.references -= 1;
    STATUS_SUCCESS
}
pub(crate) fn se_internal_TokenReferenceCount(token: *mut c_void) -> u32 {
    token_meta(token).map_or(0, |m| m.references)
}

pub(crate) unsafe fn se_internal_UnregisterTokenMetadata(token: *mut c_void) -> i32 {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = TOKEN_META_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKEN_META) };
    if let Some(e) = entries.iter_mut().find(|e| e.used && e.token == token as usize) {
        if e.references != 0 { return 0xC000_009Eu32 as i32; }
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
pub(crate) unsafe fn se_internal_TokenHasPrivilege(token: *mut c_void, privilege: u32) -> u8 {
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
        assert_eq!(unsafe { se_internal_RegisterTokenMetadata(token, 2, 2, 0b011, 1 << 9, 7, 0x1234) }, STATUS_SUCCESS);
        assert_eq!(unsafe { SeTokenIsAdmin(token) }, 1);
        assert_eq!(unsafe { SeTokenIsRestricted(token) }, 1);
        assert_eq!(unsafe { SeTokenIsWriteRestricted(token) }, 0);
        assert_eq!(unsafe { SeTokenType(token) }, 2);
        assert_eq!(unsafe { SeTokenImpersonationLevel(token) }, 2);
        assert_eq!(unsafe { se_internal_TokenHasPrivilege(token, 9) }, 1);
        assert_eq!(unsafe { se_internal_TokenHasPrivilege(token, 10) }, 0);
        let mut session = 0u32;
        let mut auth = 0u64;
        assert_eq!(unsafe { SeQuerySessionIdToken(token, &mut session) }, STATUS_SUCCESS);
        assert_eq!(session, 7);
        assert_eq!(unsafe { SeQuerySessionIdTokenEx(token, &mut session) }, STATUS_SUCCESS);
        assert_eq!(unsafe { SeQueryAuthenticationIdToken(token, &mut auth) }, STATUS_SUCCESS);
        assert_eq!(auth, 0x1234);
        assert_eq!(unsafe { se_internal_UnregisterTokenMetadata(token) }, STATUS_SUCCESS);
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

pub(crate) unsafe fn se_internal_TokenIsPrimary(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.token_type == 1) as u8
}
pub(crate) unsafe fn se_internal_TokenIsImpersonation(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.token_type == 2) as u8
}
pub(crate) unsafe fn se_internal_TokenIsKnown(token: *mut c_void) -> u8 {
    token_meta(token).is_some() as u8
}
pub(crate) unsafe fn se_internal_TokenQueryFlags(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(flags) = token_flags(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, flags); }
    STATUS_SUCCESS
}
pub(crate) unsafe fn se_internal_TokenQueryPrivilegeBitmap(token: *mut c_void, out: *mut u64) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, meta.privileges); }
    STATUS_SUCCESS
}
pub(crate) unsafe fn se_internal_TokenHasAnyPrivilege(token: *mut c_void, mask: u64) -> u8 {
    token_meta(token).is_some_and(|m| (m.privileges & mask) != 0) as u8
}
pub(crate) unsafe fn se_internal_TokenHasAllPrivileges(token: *mut c_void, mask: u64) -> u8 {
    token_meta(token).is_some_and(|m| (m.privileges & mask) == mask) as u8
}
pub(crate) unsafe fn se_internal_TokenPrivilegeCount(token: *mut c_void) -> u32 {
    token_meta(token).map_or(0, |m| m.privileges.count_ones())
}
pub(crate) unsafe fn se_internal_TokenQuerySessionId(token: *mut c_void, out: *mut u32) -> i32 {
    unsafe { SeQuerySessionIdToken(token, out) }
}
pub(crate) unsafe fn se_internal_TokenQueryAuthenticationId(token: *mut c_void, out: *mut u64) -> i32 {
    unsafe { SeQueryAuthenticationIdToken(token, out) }
}
pub(crate) unsafe fn se_internal_TokenQueryImpersonationLevel(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, if meta.token_type == 2 { meta.impersonation_level } else { 0 }); }
    STATUS_SUCCESS
}
pub(crate) unsafe fn se_internal_TokenQueryType(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, meta.token_type); }
    STATUS_SUCCESS
}
pub(crate) unsafe fn se_internal_TokenHasPrivilegeRange(token: *mut c_void, first: u32, count: u32) -> u8 {
    if count == 0 || first >= 64 || count > 64 || first.saturating_add(count) > 64 { return 0; }
    let mask = if count == 64 { u64::MAX } else { ((1u64 << count) - 1) << first };
    token_meta(token).is_some_and(|m| (m.privileges & mask) == mask) as u8
}
pub(crate) unsafe fn se_internal_TokenHasNoPrivileges(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.privileges == 0) as u8
}
pub(crate) unsafe fn se_internal_TokenIsSession(token: *mut c_void, session: u32) -> u8 {
    token_meta(token).is_some_and(|m| m.session_id == session) as u8
}
pub(crate) unsafe fn se_internal_TokenIsLogonSession(token: *mut c_void, luid: u64) -> u8 {
    token_meta(token).is_some_and(|m| m.authentication_id == luid) as u8
}
pub(crate) unsafe fn se_internal_TokenCanImpersonate(token: *mut c_void, required_level: u32) -> u8 {
    if required_level > 3 { return 0; }
    token_meta(token).is_some_and(|m| m.token_type == 2 && m.impersonation_level >= required_level) as u8
}
pub(crate) unsafe fn se_internal_TokenPrivilegeMaskContains(token: *mut c_void, mask: u64) -> u8 {
    token_meta(token).is_some_and(|m| (m.privileges & mask) == mask) as u8
}
pub(crate) unsafe fn se_internal_TokenQueryPrivilegeCount(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    unsafe { core::ptr::write_unaligned(out, meta.privileges.count_ones()); }
    STATUS_SUCCESS
}
pub(crate) unsafe fn se_internal_TokenQueryRestrictions(token: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(token) else { return 0xC000_0008u32 as i32; };
    let flags = ((meta.restricted as u32) | ((meta.write_restricted as u32) << 1));
    unsafe { core::ptr::write_unaligned(out, flags); }
    STATUS_SUCCESS
}
pub(crate) unsafe fn se_internal_TokenIsAdminAndUnrestricted(token: *mut c_void) -> u8 {
    token_meta(token).is_some_and(|m| m.admin && !m.restricted && !m.write_restricted) as u8
}


#[cfg(test)]
mod extended_token_tests {
    use super::*;
    #[test]
    fn extended_token_queries_validate_and_round_trip() {
        let mut storage = 0u64;
        let token = (&mut storage as *mut u64).cast::<c_void>();
        assert_eq!(unsafe { se_internal_TokenIsKnown(token) }, 0);
        assert_eq!(unsafe { se_internal_TokenQueryFlags(token, core::ptr::null_mut()) }, STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe { se_internal_RegisterTokenMetadata(token, 2, 2, 0b001, 0b10110, 12, 0x5678) }, STATUS_SUCCESS);
        assert_eq!(unsafe { se_internal_TokenIsPrimary(token) }, 0);
        assert_eq!(unsafe { se_internal_TokenIsImpersonation(token) }, 1);
        assert_eq!(unsafe { se_internal_TokenIsKnown(token) }, 1);
        assert_eq!(unsafe { se_internal_TokenHasAnyPrivilege(token, 0b00010) }, 1);
        assert_eq!(unsafe { se_internal_TokenHasAllPrivileges(token, 0b10100) }, 1);
        assert_eq!(unsafe { se_internal_TokenHasAllPrivileges(token, 0b11100) }, 0);
        assert_eq!(unsafe { se_internal_TokenPrivilegeCount(token) }, 3);
        assert_eq!(unsafe { se_internal_TokenHasPrivilegeRange(token, 1, 2) }, 1);
        assert_eq!(unsafe { se_internal_TokenHasPrivilegeRange(token, 63, 2) }, 0);
        assert_eq!(unsafe { se_internal_TokenIsSession(token, 12) }, 1);
        assert_eq!(unsafe { se_internal_TokenIsLogonSession(token, 0x5678) }, 1);
        assert_eq!(unsafe { se_internal_TokenCanImpersonate(token, 2) }, 1);
        assert_eq!(unsafe { se_internal_TokenCanImpersonate(token, 3) }, 0);
        assert_eq!(unsafe { se_internal_TokenIsAdminAndUnrestricted(token) }, 1);
        assert_eq!(unsafe { se_internal_UnregisterTokenMetadata(token) }, STATUS_SUCCESS);
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
pub(crate) unsafe fn se_internal_ValidateSid(sid: *const u8, length: usize) -> usize {
    if sid.is_null() || length < 8 { return 0; }
    let bytes = unsafe { core::slice::from_raw_parts(sid, length) };
    bounded_sid_size(bytes).unwrap_or(0)
}

/// Compare two explicitly bounded SID buffers; only exact valid SID encodings compare equal.
pub(crate) unsafe fn se_internal_SidEqual(
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
pub(crate) unsafe fn se_internal_ValidateAcl(acl: *const u8, length: usize) -> usize {
    if acl.is_null() || length < 8 { return 0; }
    let bytes = unsafe { core::slice::from_raw_parts(acl, length) };
    bounded_acl_size(bytes).unwrap_or(0)
}

/// Read a self-relative descriptor's control word from an explicit bounded span.
pub(crate) unsafe fn se_internal_QueryDescriptorControl(
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
pub(crate) unsafe fn se_internal_QueryDescriptorComponent(
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
pub(crate) unsafe fn se_internal_PrivilegeBitmapSatisfies(available: u64, required: u64) -> u8 {
    ((available & required) == required) as u8
}

/// Check a bounded access mask using explicit allowed/denied masks. Deny always wins.
pub(crate) unsafe fn se_internal_AccessMaskAllowed(
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
        assert_eq!(unsafe { se_internal_ValidateSid(sid.as_ptr(), sid.len()) }, 12);
        assert_eq!(unsafe { se_internal_ValidateSid(sid.as_ptr(), 8) }, 0);
        assert_eq!(unsafe { se_internal_SidEqual(sid.as_ptr(), 12, sid.as_ptr(), 12) }, 1);
        assert_eq!(unsafe { se_internal_SidEqual(sid.as_ptr(), 8, sid.as_ptr(), 12) }, 0);
        let acl = [2u8, 0, 8, 0, 0, 0, 0, 0];
        assert_eq!(unsafe { se_internal_ValidateAcl(acl.as_ptr(), acl.len()) }, 8);
        assert_eq!(unsafe { se_internal_ValidateAcl(acl.as_ptr(), 7) }, 0);
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
        assert_eq!(unsafe { se_internal_QueryDescriptorComponent(sd.as_ptr(), sd.len(), 0, &mut off, &mut len) }, STATUS_SUCCESS);
        assert_eq!((off, len), (20, 12));
        assert_eq!(unsafe { se_internal_QueryDescriptorComponent(sd.as_ptr(), 24, 0, &mut off, &mut len) }, STATUS_DATA_ERROR);
        assert_eq!(unsafe { se_internal_QueryDescriptorControl(sd.as_ptr(), 19, &mut 0u16) }, STATUS_BUFFER_TOO_SMALL);
    }
    #[test]
    fn privilege_and_access_mask_helpers_are_conservative() {
        assert_eq!(unsafe { se_internal_PrivilegeBitmapSatisfies(0b1110, 0b0110) }, 1);
        assert_eq!(unsafe { se_internal_PrivilegeBitmapSatisfies(0b0010, 0b0110) }, 0);
        assert_eq!(unsafe { se_internal_AccessMaskAllowed(0b011, 0b111, 0) }, 1);
        assert_eq!(unsafe { se_internal_AccessMaskAllowed(0b011, 0b111, 0b001) }, 0);
    }
}


// Further bounded inspection helpers used by the future Se access-check path.
pub(crate) unsafe fn se_internal_SidRevision(sid: *const u8, length: usize) -> u32 {
    if sid.is_null() || length < 8 { return u32::MAX; }
    let b = unsafe { core::slice::from_raw_parts(sid, length) };
    if bounded_sid_size(b).is_some() { b[0] as u32 } else { u32::MAX }
}
pub(crate) unsafe fn se_internal_SidSubAuthorityCount(sid: *const u8, length: usize) -> u32 {
    if sid.is_null() || length < 8 { return u32::MAX; }
    let b = unsafe { core::slice::from_raw_parts(sid, length) };
    if bounded_sid_size(b).is_some() { b[1] as u32 } else { u32::MAX }
}
/// Outputs the six-byte SID identifier authority as a numeric value.
pub(crate) unsafe fn se_internal_SidIdentifierAuthority(
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
pub(crate) unsafe fn se_internal_QueryAclInfo(
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
pub(crate) unsafe fn se_internal_CountAceType(acl: *const u8, length: usize, ace_type: u32) -> u32 {
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
pub(crate) unsafe fn se_internal_AclContainsSid(
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
pub(crate) unsafe fn se_internal_DescriptorIsSelfRelative(descriptor: *const u8, length: usize) -> u8 {
    if descriptor.is_null() || length < 20 { return 0; }
    let b = unsafe { core::slice::from_raw_parts(descriptor, length) };
    (b[0] == 1 && u16le(b, 2).is_some_and(|c| c & 0x8000 != 0)) as u8
}
pub(crate) unsafe fn se_internal_DescriptorComponentPresent(
    descriptor: *const u8, length: usize, component: u32,
) -> u8 {
    if descriptor.is_null() || length < 20 || component > 3 { return 0; }
    let b = unsafe { core::slice::from_raw_parts(descriptor, length) };
    if b[0] != 1 || !u16le(b, 2).is_some_and(|c| c & 0x8000 != 0) { return 0; }
    u32le(b, [4usize, 8, 12, 16][component as usize]).is_some_and(|v| v != 0) as u8
}
pub(crate) unsafe fn se_internal_AccessMaskIsSubset(requested: u32, containing: u32) -> u8 {
    (requested & containing == requested) as u8
}
pub(crate) unsafe fn se_internal_PrivilegeMaskCount(mask: u64) -> u32 {
    mask.count_ones()
}
pub(crate) unsafe fn se_internal_PrivilegeMaskFirst(mask: u64) -> u32 {
    if mask == 0 { u32::MAX } else { mask.trailing_zeros() }
}

#[cfg(test)]
mod bounded_security_inspection_tests {
    use super::*;
    #[test]
    fn sid_authority_and_acl_queries_are_bounded() {
        let sid = [1u8, 1, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0];
        assert_eq!(unsafe { se_internal_SidRevision(sid.as_ptr(), sid.len()) }, 1);
        assert_eq!(unsafe { se_internal_SidSubAuthorityCount(sid.as_ptr(), sid.len()) }, 1);
        let mut authority = 0u64;
        assert_eq!(unsafe { se_internal_SidIdentifierAuthority(sid.as_ptr(), sid.len(), &mut authority) }, STATUS_SUCCESS);
        assert_eq!(authority, 5);
        let acl = [2u8, 0, 8, 0, 0, 0, 0, 0];
        let mut revision = 0;
        let mut count = 9;
        let mut size = 0;
        assert_eq!(unsafe { se_internal_QueryAclInfo(acl.as_ptr(), acl.len(), &mut revision, &mut count, &mut size) }, STATUS_SUCCESS);
        assert_eq!((revision, count, size), (2, 0, 8));
        assert_eq!(unsafe { se_internal_CountAceType(acl.as_ptr(), acl.len(), 0) }, 0);
        assert_eq!(unsafe { se_internal_CountAceType(acl.as_ptr(), 7, 0) }, u32::MAX);
    }
    #[test]
    fn descriptor_and_mask_inspection_handles_invalid_inputs() {
        let mut sd = [0u8; 20];
        sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8000u16.to_le_bytes());
        assert_eq!(unsafe { se_internal_DescriptorIsSelfRelative(sd.as_ptr(), sd.len()) }, 1);
        assert_eq!(unsafe { se_internal_DescriptorComponentPresent(sd.as_ptr(), sd.len(), 0) }, 0);
        assert_eq!(unsafe { se_internal_DescriptorComponentPresent(sd.as_ptr(), sd.len(), 4) }, 0);
        assert_eq!(unsafe { se_internal_AccessMaskIsSubset(0b011, 0b111) }, 1);
        assert_eq!(unsafe { se_internal_AccessMaskIsSubset(0b101, 0b011) }, 0);
        assert_eq!(unsafe { se_internal_PrivilegeMaskCount(0b10110) }, 3);
        assert_eq!(unsafe { se_internal_PrivilegeMaskFirst(0b10110) }, 1);
        assert_eq!(unsafe { se_internal_PrivilegeMaskFirst(0) }, u32::MAX);
    }
}


// --- Subject-token SID identity and SeAccessCheck implementation ---
// This is a bounded RdotOS token identity registry, not the private Windows TOKEN
// layout. Process/token creation must register validated user/group SIDs and unregister
// them before token storage is reclaimed. Unknown tokens and unsupported DACL ACEs deny.
const SECURITY_TOKEN_SLOTS: usize = 32;
const SECURITY_TOKEN_SIDS: usize = 8;
const SECURITY_SID_BYTES: usize = 68;
#[derive(Clone, Copy)]
struct SecurityTokenIdentity {
    used: bool,
    token: usize,
    count: u8,
    lengths: [u8; SECURITY_TOKEN_SIDS],
    sids: [[u8; SECURITY_SID_BYTES]; SECURITY_TOKEN_SIDS],
    restricted_count: u8,
    restricted_lengths: [u8; SECURITY_TOKEN_SIDS],
    restricted_sids: [[u8; SECURITY_SID_BYTES]; SECURITY_TOKEN_SIDS],
    capability_count: u8,
    capability_lengths: [u8; SECURITY_TOKEN_SIDS],
    capability_sids: [[u8; SECURITY_SID_BYTES]; SECURITY_TOKEN_SIDS],
}
impl SecurityTokenIdentity {
    const EMPTY: Self = Self { used: false, token: 0, count: 0,
        lengths: [0; SECURITY_TOKEN_SIDS], sids: [[0; SECURITY_SID_BYTES]; SECURITY_TOKEN_SIDS],
        restricted_count: 0, restricted_lengths: [0; SECURITY_TOKEN_SIDS],
        restricted_sids: [[0; SECURITY_SID_BYTES]; SECURITY_TOKEN_SIDS],
        capability_count: 0, capability_lengths: [0; SECURITY_TOKEN_SIDS],
        capability_sids: [[0; SECURITY_SID_BYTES]; SECURITY_TOKEN_SIDS] };
}
static SECURITY_TOKEN_LOCK: TokenMetaLock = TokenMetaLock::new();
static mut SECURITY_TOKEN_IDENTITIES: [SecurityTokenIdentity; SECURITY_TOKEN_SLOTS] =
    [SecurityTokenIdentity::EMPTY; SECURITY_TOKEN_SLOTS];

/// Internal integration hook for Ps token creation. Copies the user SID and group SIDs;
/// each SID pointer must refer to readable kernel-owned memory for its supplied length.
pub(crate) unsafe fn se_internal_RegisterTokenSids(
    token: *mut c_void, sid_ptrs: *const *const u8, lengths: *const u8, count: u32,
) -> i32 {
    if token.is_null() || count == 0 || count as usize > SECURITY_TOKEN_SIDS
        || sid_ptrs.is_null() || lengths.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut entry = SecurityTokenIdentity { used: true, token: token as usize, ..SecurityTokenIdentity::EMPTY };
    for i in 0..count as usize {
        let ptr = unsafe { core::ptr::read_unaligned(sid_ptrs.add(i)) };
        let len = unsafe { core::ptr::read_unaligned(lengths.add(i)) } as usize;
        if ptr.is_null() || len < 8 || len > SECURITY_SID_BYTES { return STATUS_INVALID_PARAMETER; }
        let sid = unsafe { core::slice::from_raw_parts(ptr, len) };
        if bounded_sid_size(sid) != Some(len) { return STATUS_INVALID_PARAMETER; }
        entry.lengths[i] = len as u8;
        entry.sids[i][..len].copy_from_slice(sid);
    }
    entry.count = count as u8;
    let _guard = SECURITY_TOKEN_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(SECURITY_TOKEN_IDENTITIES) };
    let slot = entries.iter().position(|e| e.used && e.token == token as usize)
        .or_else(|| entries.iter().position(|e| !e.used));
    let Some(i) = slot else { return 0xC000_009Au32 as i32; };
    entries[i] = entry;
    STATUS_SUCCESS
}


unsafe fn copy_sid_set(
    sid_ptrs: *const *const u8, lengths: *const u8, count: u32,
) -> Option<([[u8; SECURITY_SID_BYTES]; SECURITY_TOKEN_SIDS], [u8; SECURITY_TOKEN_SIDS])> {
    if count == 0 || count as usize > SECURITY_TOKEN_SIDS || sid_ptrs.is_null() || lengths.is_null() { return None; }
    let mut values = [[0u8; SECURITY_SID_BYTES]; SECURITY_TOKEN_SIDS];
    let mut sizes = [0u8; SECURITY_TOKEN_SIDS];
    for i in 0..count as usize {
        let ptr = unsafe { core::ptr::read_unaligned(sid_ptrs.add(i)) };
        let len = unsafe { core::ptr::read_unaligned(lengths.add(i)) } as usize;
        if ptr.is_null() || len < 8 || len > SECURITY_SID_BYTES { return None; }
        let sid = unsafe { core::slice::from_raw_parts(ptr, len) };
        if bounded_sid_size(sid) != Some(len) { return None; }
        values[i][..len].copy_from_slice(sid);
        sizes[i] = len as u8;
    }
    Some((values, sizes))
}

/// Internal token-construction hook for restricting SIDs. The token metadata entry
/// must exist first; all SIDs are validated and copied before publishing the set.
pub(crate) unsafe fn se_internal_RegisterRestrictedTokenSids(
    token: *mut c_void, sid_ptrs: *const *const u8, lengths: *const u8, count: u32,
) -> i32 {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some((sids, sizes)) = (unsafe { copy_sid_set(sid_ptrs, lengths, count) }) else {
        return STATUS_INVALID_PARAMETER;
    };
    let _guard = SECURITY_TOKEN_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(SECURITY_TOKEN_IDENTITIES) };
    let Some(entry) = entries.iter_mut().find(|e| e.used && e.token == token as usize) else {
        return 0xC000_0008u32 as i32;
    };
    entry.restricted_sids = sids;
    entry.restricted_lengths = sizes;
    entry.restricted_count = count as u8;
    STATUS_SUCCESS
}

/// Internal AppContainer capability registration hook. Capability SIDs are kept
/// separately from ordinary group SIDs so access checks can apply the capability pass.
pub(crate) unsafe fn se_internal_RegisterTokenCapabilities(
    token: *mut c_void, sid_ptrs: *const *const u8, lengths: *const u8, count: u32,
) -> i32 {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some((sids, sizes)) = (unsafe { copy_sid_set(sid_ptrs, lengths, count) }) else {
        return STATUS_INVALID_PARAMETER;
    };
    let _guard = SECURITY_TOKEN_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(SECURITY_TOKEN_IDENTITIES) };
    let Some(entry) = entries.iter_mut().find(|e| e.used && e.token == token as usize) else {
        return 0xC000_0008u32 as i32;
    };
    entry.capability_sids = sids;
    entry.capability_lengths = sizes;
    entry.capability_count = count as u8;
    STATUS_SUCCESS
}

/// Copy registered SID sets, restriction/capability state and privilege metadata
/// to a newly allocated token object during Ps/Se token duplication.
pub(crate) unsafe fn se_internal_DuplicateTokenSecurity(
    source: *mut c_void, destination: *mut c_void, token_type: u32, impersonation_level: u32,
) -> i32 {
    if source.is_null() || destination.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(meta) = token_meta(source) else { return 0xC000_0008u32 as i32; };
    let flags = (meta.admin as u32) | ((meta.restricted as u32) << 1)
        | ((meta.write_restricted as u32) << 2) | ((meta.app_container as u32) << 3);
    let status = unsafe { se_internal_RegisterTokenMetadata(destination, token_type, impersonation_level,
        flags, meta.privileges, meta.session_id, meta.authentication_id) };
    if status != STATUS_SUCCESS { return status; }
    let copied = {
        let _guard = SECURITY_TOKEN_LOCK.lock();
        let entries = unsafe { &*core::ptr::addr_of!(SECURITY_TOKEN_IDENTITIES) };
        entries.iter().find(|e| e.used && e.token == source as usize).copied()
    };
    if let Some(mut identity) = copied {
        identity.token = destination as usize;
        let _guard = SECURITY_TOKEN_LOCK.lock();
        let entries = unsafe { &mut *core::ptr::addr_of_mut!(SECURITY_TOKEN_IDENTITIES) };
        let slot = entries.iter().position(|e| e.used && e.token == destination as usize)
            .or_else(|| entries.iter().position(|e| !e.used));
        let Some(i) = slot else {
            let _ = unsafe { se_internal_UnregisterTokenMetadata(destination) };
            return 0xC000_009Au32 as i32;
        };
        entries[i] = identity;
    }
    STATUS_SUCCESS
}

pub(crate) unsafe fn se_internal_UnregisterTokenSids(token: *mut c_void) -> i32 {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    if se_internal_TokenReferenceCount(token) != 0 { return 0xC000_009Eu32 as i32; }
    let _guard = SECURITY_TOKEN_LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(SECURITY_TOKEN_IDENTITIES) };
    if let Some(e) = entries.iter_mut().find(|e| e.used && e.token == token as usize) {
        *e = SecurityTokenIdentity::EMPTY;
        STATUS_SUCCESS
    } else { 0xC000_0008u32 as i32 }
}

fn token_contains_sid(token: *mut c_void, sid: &[u8]) -> bool {
    if token.is_null() || sid.len() < 8 { return false; }
    let _guard = SECURITY_TOKEN_LOCK.lock();
    let entries = unsafe { &*core::ptr::addr_of!(SECURITY_TOKEN_IDENTITIES) };
    entries.iter().find(|e| e.used && e.token == token as usize).is_some_and(|e| {
        (0..e.count as usize).any(|i| {
            let n = e.lengths[i] as usize;
            n == sid.len() && e.sids[i][..n] == *sid
        }) || (0..e.capability_count as usize).any(|i| {
            let n = e.capability_lengths[i] as usize;
            n == sid.len() && e.capability_sids[i][..n] == *sid
        })
    })
}
fn token_has_restricting_sids(token: *mut c_void) -> bool {
    if token.is_null() { return false; }
    let _guard = SECURITY_TOKEN_LOCK.lock();
    let entries = unsafe { &*core::ptr::addr_of!(SECURITY_TOKEN_IDENTITIES) };
    entries.iter().find(|e| e.used && e.token == token as usize)
        .is_some_and(|e| e.restricted_count != 0 || e.capability_count != 0)
}
fn token_contains_restricting_sid(token: *mut c_void, sid: &[u8]) -> bool {
    if token.is_null() || sid.len() < 8 { return false; }
    let _guard = SECURITY_TOKEN_LOCK.lock();
    let entries = unsafe { &*core::ptr::addr_of!(SECURITY_TOKEN_IDENTITIES) };
    entries.iter().find(|e| e.used && e.token == token as usize).is_some_and(|e| {
        (0..e.restricted_count as usize).any(|i| {
            let n = e.restricted_lengths[i] as usize;
            n == sid.len() && e.restricted_sids[i][..n] == *sid
        }) || (0..e.capability_count as usize).any(|i| {
            let n = e.capability_lengths[i] as usize;
            n == sid.len() && e.capability_sids[i][..n] == *sid
        })
    })
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SeGenericMapping {
    pub generic_read: u32,
    pub generic_write: u32,
    pub generic_execute: u32,
    pub generic_all: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SeSecuritySubjectContext {
    pub client_token: *mut c_void,
    pub impersonation_level: u32,
    pub primary_token: *mut c_void,
    pub process_audit_id: *mut c_void,
}
/// Capture the current process primary token and current thread impersonation token
/// into a kernel subject-context record, holding references until release.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeCaptureSubjectContext(subject_context: *mut SeSecuritySubjectContext) {
    if subject_context.is_null() { return; }
    let pid = crate::thread::current_process_id();
    let primary = crate::process::reference_primary_token(pid);
    let client = crate::thread::reference_current_impersonation_token();
    let level = if client.is_null() { 0 } else { unsafe { SeTokenImpersonationLevel(client) } };
    let context = SeSecuritySubjectContext {
        client_token: client,
        impersonation_level: level,
        primary_token: primary,
        process_audit_id: core::ptr::null_mut(),
    };
    unsafe { core::ptr::write_unaligned(subject_context, context); }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeReleaseSubjectContext(subject_context: *mut SeSecuritySubjectContext) {
    if subject_context.is_null() { return; }
    let context = unsafe { core::ptr::read_unaligned(subject_context) };
    if !context.client_token.is_null() {
        let _ = unsafe { se_internal_DereferenceToken(context.client_token) };
    }
    if !context.primary_token.is_null() {
        let _ = unsafe { se_internal_DereferenceToken(context.primary_token) };
    }
    unsafe {
        core::ptr::write_unaligned(subject_context, SeSecuritySubjectContext {
            client_token: core::ptr::null_mut(), impersonation_level: 0,
            primary_token: core::ptr::null_mut(), process_audit_id: core::ptr::null_mut(),
        });
    }
}

fn map_generic_access(mask: u32, mapping: &SeGenericMapping) -> u32 {
    let mut out = mask;
    if out & 0x8000_0000 != 0 { out = (out & !0x8000_0000) | mapping.generic_read; }
    if out & 0x4000_0000 != 0 { out = (out & !0x4000_0000) | mapping.generic_write; }
    if out & 0x2000_0000 != 0 { out = (out & !0x2000_0000) | mapping.generic_execute; }
    if out & 0x1000_0000 != 0 { out = (out & !0x1000_0000) | mapping.generic_all; }
    out
}
fn is_everyone_sid(sid: &[u8]) -> bool {
    sid.len() == 12 && sid[0] == 1 && sid[1] == 1
        && sid[2..7].iter().all(|v| *v == 0) && sid[7] == 1
        && sid[8..12] == [0, 0, 0, 0]
}
fn sid_matches_subject(token: *mut c_void, sid: &[u8], restricting_pass: bool) -> bool {
    if restricting_pass { token_contains_restricting_sid(token, sid) }
    else { is_everyone_sid(sid) || token_contains_sid(token, sid) }
}
fn descriptor_component<'a>(bytes: &'a [u8], field: usize, acl: bool) -> Option<&'a [u8]> {
    let off = u32le(bytes, field)? as usize;
    if off == 0 { return None; }
    if off < 20 || off >= bytes.len() { return None; }
    let n = if acl { bounded_acl_size(&bytes[off..])? } else { bounded_sid_size(&bytes[off..])? };
    bytes.get(off..off.checked_add(n)?)
}
fn evaluate_dacl(token: *mut c_void, sd: &[u8], requested: u32, owner_rights: u32, restricting_pass: bool) -> Option<u32> {
    if sd.len() < 20 || sd[0] != 1 { return None; }
    let control = u16le(sd, 2)?;
    if control & 0x8000 == 0 { return None; }
    let dacl_present = control & 0x0004 != 0;
    if !dacl_present { return Some(u32::MAX); } // NULL/absent DACL grants all access.
    let dacl_off = u32le(sd, 16)? as usize;
    if dacl_off == 0 { return Some(u32::MAX); }
    let acl = descriptor_component(sd, 16, true)?;
    let acl_size = u16le(acl, 2)? as usize;
    let ace_count = u16le(acl, 4)? as usize;
    let mut cursor = 8usize;
    let mut remaining = requested & !owner_rights;
    let mut granted = owner_rights;
    for _ in 0..ace_count {
        let header = acl.get(cursor..cursor.checked_add(4)?)?;
        let ace_type = header[0];
        let ace_size = u16le(header, 2)? as usize;
        if ace_size < 8 || cursor.checked_add(ace_size)? > acl_size { return None; }
        let ace = acl.get(cursor..cursor.checked_add(ace_size)?)?;
        if matches!(ace_type, 0 | 1 | 5 | 6 | 9 | 10 | 11 | 12) {
            let mask = u32le(ace, 4)?;
            let object_ace = matches!(ace_type, 5 | 6 | 11 | 12);
            let callback_ace = matches!(ace_type, 9 | 10 | 11 | 12);
            let mut object_flags = 0u32;
            let mut sid_offset = if object_ace { 12usize } else { 8usize };
            if object_ace {
                let flags = u32le(ace, 8)?;
                object_flags = flags;
                if flags & !3 != 0 { return None; }
                if flags & 1 != 0 { sid_offset = sid_offset.checked_add(16)?; }
                if flags & 2 != 0 { sid_offset = sid_offset.checked_add(16)?; }
            }
            let sid = ace.get(sid_offset..)?;
            let sid_n = bounded_sid_size(sid)?;
            let sid = &sid[..sid_n];
            if sid_matches_subject(token, sid, restricting_pass) {
                if object_ace && object_flags != 0 { return None; }
                if callback_ace && ace.len() > sid_offset + sid_n { return None; }
                if matches!(ace_type, 1 | 6 | 10 | 12) && (mask & remaining) != 0 { return Some(granted); }
                if matches!(ace_type, 0 | 5 | 9 | 11) {
                    let take = mask & remaining;
                    granted |= take;
                    remaining &= !take;
                    if remaining == 0 { return Some(granted); }
                }
            }
        }
        cursor = cursor.checked_add(ace_size)?;
    }
    Some(granted)
}

/// Windows SeAccessCheck ABI. Supports bounded self-relative descriptors, generic
/// mapping, owner READ_CONTROL/WRITE_DAC, and basic allow/deny ACEs for registered
/// token SIDs plus Everyone. It intentionally fails closed for unknown tokens,
/// malformed descriptors, unsupported ACE forms, and absent token identity metadata.
const SECURITY_AUDIT_CAPACITY: usize = 64;
#[derive(Clone, Copy)]
struct SecurityAuditEvent {
    sequence: u64,
    token: usize,
    desired: u32,
    granted: u32,
    status: i32,
    ace_type: u8,
}
impl SecurityAuditEvent {
    const EMPTY: Self = Self { sequence: 0, token: 0, desired: 0, granted: 0, status: 0, ace_type: 0 };
}
static SECURITY_AUDIT_LOCK: TokenMetaLock = TokenMetaLock::new();
static mut SECURITY_AUDIT_EVENTS: [SecurityAuditEvent; SECURITY_AUDIT_CAPACITY] =
    [SecurityAuditEvent::EMPTY; SECURITY_AUDIT_CAPACITY];
static mut SECURITY_AUDIT_NEXT: u64 = 1;

fn record_security_audit(sd: &[u8], token: *mut c_void, desired: u32, granted: u32, success: bool) {
    if sd.len() < 20 || sd[0] != 1 { return; }
    let Some(control) = u16le(sd, 2) else { return; };
    if control & 0x0010 == 0 { return; }
    let Some(acl) = descriptor_component(sd, 12, true) else { return; };
    let Some(ace_count) = u16le(acl, 4).map(|v| v as usize) else { return; };
    let acl_size = u16le(acl, 2).unwrap_or(0) as usize;
    let mut cursor = 8usize;
    for _ in 0..ace_count {
        let Some(header) = acl.get(cursor..cursor.saturating_add(4)) else { return; };
        let kind = header[0];
        let flags = header[1];
        let ace_size = u16le(header, 2).unwrap_or(0) as usize;
        if ace_size < 8 || cursor.checked_add(ace_size).is_none_or(|end| end > acl_size) { return; }
        let Some(ace) = acl.get(cursor..cursor + ace_size) else { return; };
        if matches!(kind, 2 | 3 | 7 | 8 | 13 | 14 | 15 | 16) {
            let object_ace = matches!(kind, 7 | 8 | 15 | 16);
            let callback_ace = matches!(kind, 13 | 14 | 15 | 16);
            let mut sid_offset = if object_ace { 12usize } else { 8usize };
            let mut object_flags = 0u32;
            if object_ace {
                object_flags = u32le(ace, 8).unwrap_or(u32::MAX);
                if object_flags & !3 != 0 { cursor += ace_size; continue; }
                if object_flags & 1 != 0 { sid_offset = match sid_offset.checked_add(16) { Some(v) => v, None => return }; }
                if object_flags & 2 != 0 { sid_offset = match sid_offset.checked_add(16) { Some(v) => v, None => return }; }
            }
            if object_flags == 0 || !object_ace {
                if let Some(sid_data) = ace.get(sid_offset..) {
                    if let Some(sid_len) = bounded_sid_size(sid_data) {
                        let audit_flag = if success { 0x40 } else { 0x80 };
                        let mask = u32le(ace, 4).unwrap_or(0);
                        let has_callback_data = callback_ace && ace.len() > sid_offset + sid_len;
                        if !has_callback_data && flags & audit_flag != 0 && mask & desired != 0
                            && sid_matches_subject(token, &sid_data[..sid_len], false) {
                            let _guard = SECURITY_AUDIT_LOCK.lock();
                            let events = unsafe { &mut *core::ptr::addr_of_mut!(SECURITY_AUDIT_EVENTS) };
                            let next = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(SECURITY_AUDIT_NEXT)) };
                            events[(next as usize - 1) % SECURITY_AUDIT_CAPACITY] = SecurityAuditEvent {
                                sequence: next, token: token as usize, desired, granted,
                                status: if success { STATUS_SUCCESS } else { 0xC000_0022u32 as i32 },
                                ace_type: kind,
                            };
                            unsafe { core::ptr::write_volatile(core::ptr::addr_of_mut!(SECURITY_AUDIT_NEXT), next.wrapping_add(1).max(1)); }
                        }
                    }
                }
            }
        }
        cursor += ace_size;
    }
}

pub(crate) fn se_internal_ReadSecurityAuditEvent(sequence: u64, output: *mut u8, output_length: usize) -> i32 {
    if output.is_null() || output_length < core::mem::size_of::<SecurityAuditEvent>() || sequence == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    let _guard = SECURITY_AUDIT_LOCK.lock();
    let events = unsafe { &*core::ptr::addr_of!(SECURITY_AUDIT_EVENTS) };
    let event = events.iter().find(|e| e.sequence == sequence).copied();
    let Some(event) = event else { return 0xC000_0034u32 as i32; };
    unsafe { core::ptr::copy_nonoverlapping((&event as *const SecurityAuditEvent).cast::<u8>(), output, core::mem::size_of::<SecurityAuditEvent>()); }
    STATUS_SUCCESS
}

static SUBJECT_CONTEXT_LOCK: AtomicBool = AtomicBool::new(false);
fn acquire_subject_context_lock() {
    while SUBJECT_CONTEXT_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
fn release_subject_context_lock() {
    SUBJECT_CONTEXT_LOCK.store(false, Ordering::Release);
}

/// Windows SeLockSubjectContext ABI. RdotOS currently serializes subject-context
/// operations through one kernel lock until per-token/process locks are available.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeLockSubjectContext(subject_context: *mut SeSecuritySubjectContext) {
    if subject_context.is_null() { return; }
    acquire_subject_context_lock();
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeUnlockSubjectContext(subject_context: *mut SeSecuritySubjectContext) {
    if subject_context.is_null() { return; }
    release_subject_context_lock();
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SeAccessCheck(
    security_descriptor: *const c_void,
    subject_context: *mut SeSecuritySubjectContext,
    subject_context_locked: *mut u8,
    desired_access: u32,
    previously_granted_access: u32,
    privileges: *mut *mut c_void,
    generic_mapping: *const SeGenericMapping,
    access_mode: i8,
    granted_access: *mut u32,
    access_status: *mut i32,
) -> u8 {
    const STATUS_ACCESS_DENIED: i32 = 0xC000_0022u32 as i32;
    if !privileges.is_null() { unsafe { core::ptr::write_unaligned(privileges, core::ptr::null_mut()); } }
    if !granted_access.is_null() { unsafe { core::ptr::write_unaligned(granted_access, 0); } }
    if !access_status.is_null() { unsafe { core::ptr::write_unaligned(access_status, STATUS_ACCESS_DENIED); } }
    if security_descriptor.is_null() || subject_context.is_null() || subject_context_locked.is_null()
        || generic_mapping.is_null() || granted_access.is_null() || access_status.is_null() || !(0..=1).contains(&access_mode) {
        return 0;
    }

    let already_locked = unsafe { core::ptr::read_unaligned(subject_context_locked) } != 0;
    if !already_locked {
        acquire_subject_context_lock();
        unsafe { core::ptr::write_unaligned(subject_context_locked, 1); }
    }
    let mut audit_token = core::ptr::null_mut();
    let mut audit_desired = desired_access;
    let mut audit_granted = 0u32;
    let success = (|| -> bool {
        let context = unsafe { core::ptr::read_unaligned(subject_context) };
        let token = if !context.client_token.is_null() {
            if context.impersonation_level < 2 { return false; }
            context.client_token
        } else { context.primary_token };
        if token.is_null() || token_meta(token).is_none() { return false; }
        audit_token = token;
        let mapping = unsafe { core::ptr::read_unaligned(generic_mapping) };
        let desired = map_generic_access(desired_access, &mapping);
        audit_desired = desired;
        let previous = map_generic_access(previously_granted_access, &mapping);
        // SeAccessCheck consumes a captured kernel security descriptor. User-mode
        // callers must use the capture/probe path before passing this pointer.
        let sd = unsafe { core::slice::from_raw_parts(security_descriptor.cast::<u8>(), 4096) };
        if unsafe { SeValidSecurityDescriptor(security_descriptor) } == 0 { return false; }
        let owner = match descriptor_component(sd, 4, false) { Some(s) => s, None => &[] };
        let owner_rights = if !owner.is_empty() && sid_matches_subject(token, owner, false) {
            0x0002_0000 | 0x0004_0000
        } else { 0 };
        let max_allowed = desired & 0x0200_0000 != 0;
        let requested = if max_allowed { 0x000F_FFFF } else { desired & !0x0200_0000 };
        let meta = token_meta(token).ok_or(()).ok();
        let allowed = match evaluate_dacl(token, sd, requested, owner_rights, false) {
            Some(v) => v, None => return false,
        };
        let mut effective_allowed = allowed;
        if let Some(m) = meta {
            // Windows privilege-assisted grants for the special security-system and
            // ownership rights. Privilege IDs follow the NT LUID low-part mapping.
            if m.privileges & (1u64 << 8) != 0 { effective_allowed |= 0x0100_0000; }
            if m.privileges & (1u64 << 9) != 0 { effective_allowed |= 0x0008_0000; }
            let write_access = requested & (0x0001_0000 | 0x0002_0000 | 0x0004_0000 | 0x0008_0000 | 0x0000_0116) != 0;
            if m.restricted || m.app_container || (m.write_restricted && write_access) {
                let restricted = match evaluate_dacl(token, sd, requested, 0, true) {
                    Some(v) => v, None => return false,
                };
                effective_allowed &= restricted;
            }
            if (m.restricted || m.app_container) && !token_has_restricting_sids(token) { return false; }
            if m.write_restricted && write_access && !token_has_restricting_sids(token) { return false; }
        }
        let explicit = desired & !0x0200_0000;
        if explicit & !(effective_allowed | previous) != 0 { return false; }
        let grant = if max_allowed { (effective_allowed | previous) & !0x0200_0000 } else { explicit | previous };
        audit_granted = grant;
        unsafe {
            core::ptr::write_unaligned(granted_access, grant);
            core::ptr::write_unaligned(access_status, STATUS_SUCCESS);
        }
        true
    })();
    if !audit_token.is_null() && unsafe { SeValidSecurityDescriptor(security_descriptor) } != 0 {
        let sd = unsafe { core::slice::from_raw_parts(security_descriptor.cast::<u8>(), 4096) };
        record_security_audit(sd, audit_token, audit_desired, audit_granted, success);
    }
    if !already_locked {
        unsafe { core::ptr::write_unaligned(subject_context_locked, 0); }
        release_subject_context_lock();
    }
    success as u8
}

#[cfg(test)]
mod se_access_check_tests {
    use super::*;
    #[test]
    fn registered_token_sid_controls_basic_dacl_access() {
        let mut token_storage = 0u64;
        let token = (&mut token_storage as *mut u64).cast::<c_void>();
        let sid = [1u8, 1, 0, 0, 0, 0, 0, 5, 21, 0, 0, 0];
        let ptrs = [sid.as_ptr()];
        let lens = [sid.len() as u8];
        assert_eq!(unsafe { se_internal_RegisterTokenMetadata(token, 1, 0, 0, 0, 0, 1) }, STATUS_SUCCESS);
        assert_eq!(unsafe { se_internal_RegisterTokenSids(token, ptrs.as_ptr(), lens.as_ptr(), 1) }, STATUS_SUCCESS);
        // Self-relative SD: DACL_PRESENT, one ACCESS_ALLOWED_ACE for mask 0x1.
        let mut sd = [0u8; 4096];
        sd[0] = 1;
        sd[2..4].copy_from_slice(&0x8004u16.to_le_bytes());
        sd[16..20].copy_from_slice(&20u32.to_le_bytes());
        sd[20..22].copy_from_slice(&2u16.to_le_bytes());
        sd[22..24].copy_from_slice(&28u16.to_le_bytes());
        sd[24..26].copy_from_slice(&1u16.to_le_bytes());
        sd[28] = 0; sd[30..32].copy_from_slice(&20u16.to_le_bytes());
        sd[32..36].copy_from_slice(&1u32.to_le_bytes());
        sd[36..48].copy_from_slice(&sid);
        let context = SeSecuritySubjectContext { client_token: core::ptr::null_mut(),
            impersonation_level: 0, primary_token: token, process_audit_id: core::ptr::null_mut() };
        let mapping = SeGenericMapping { generic_read: 1, generic_write: 2, generic_execute: 4, generic_all: 7 };
        let mut granted = 0u32;
        let mut status = 0i32;
        let mut privileges: *mut c_void = core::ptr::null_mut();
        let mut locked = 1u8;
        assert_eq!(unsafe { SeAccessCheck(sd.as_ptr().cast(), (&context as *const _).cast_mut(), &mut locked,
            1, 0, &mut privileges, &mapping, 1, &mut granted, &mut status) }, 1);
        assert_eq!(granted, 1);
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(unsafe { se_internal_UnregisterTokenSids(token) }, STATUS_SUCCESS);
        assert_eq!(unsafe { se_internal_UnregisterTokenMetadata(token) }, STATUS_SUCCESS);
    }
}


/// Windows SePrivilegeCheck ABI, using the registered RdotOS token privilege bitmap.
/// LUIDs outside the bounded 0..64 privilege namespace fail closed. RequiredPrivileges
/// is limited to 64 entries because the Windows ABI does not carry an array length.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn SePrivilegeCheck(
    required_privileges: *const c_void,
    subject_context: *mut SeSecuritySubjectContext,
    access_mode: i8,
) -> u8 {
    if !(0..=1).contains(&access_mode) || required_privileges.is_null() || subject_context.is_null() { return 0; }
    if access_mode == 0 { return 1; }
    let context = unsafe { core::ptr::read_unaligned(subject_context) };
    let token = if !context.client_token.is_null() {
        if context.impersonation_level < 2 { return 0; }
        context.client_token
    } else { context.primary_token };
    let Some(meta) = token_meta(token) else { return 0; };
    let set = required_privileges.cast::<u8>();
    let count = unsafe { core::ptr::read_unaligned(set.cast::<u32>()) } as usize;
    let control = unsafe { core::ptr::read_unaligned(set.add(4).cast::<u32>()) };
    if count > 64 { return 0; }
    if count == 0 { return 1; }
    let mut found = 0usize;
    for i in 0..count {
        let Some(offset) = 8usize.checked_add(i.checked_mul(12).unwrap_or(usize::MAX)) else { return 0; };
        if offset == usize::MAX { return 0; }
        let low = unsafe { core::ptr::read_unaligned(set.add(offset).cast::<u32>()) };
        let high = unsafe { core::ptr::read_unaligned(set.add(offset + 4).cast::<u32>()) };
        let attributes = unsafe { core::ptr::read_unaligned(set.add(offset + 8).cast::<u32>()) };
        if high == 0 && low < 64 && attributes & 2 != 0 && meta.privileges & (1u64 << low) != 0 { found += 1; }
        else if control & 1 != 0 { return 0; }
    }
    if control & 1 != 0 { (found == count) as u8 } else { (found != 0) as u8 }
}

#[cfg(test)]
mod se_privilege_check_tests {
    use super::*;
    #[test]
    fn privilege_check_obeys_all_necessary_and_token_bitmap() {
        let mut token_storage = 0u64;
        let token = (&mut token_storage as *mut u64).cast::<c_void>();
        assert_eq!(unsafe { se_internal_RegisterTokenMetadata(token, 1, 0, 0, (1 << 3) | (1 << 7), 0, 1) }, STATUS_SUCCESS);
        let context = SeSecuritySubjectContext { client_token: core::ptr::null_mut(),
            impersonation_level: 0, primary_token: token, process_audit_id: core::ptr::null_mut() };
        let mut set = [0u8; 32];
        set[0..4].copy_from_slice(&2u32.to_le_bytes());
        set[4..8].copy_from_slice(&1u32.to_le_bytes());
        set[8..12].copy_from_slice(&3u32.to_le_bytes());
        set[16..20].copy_from_slice(&2u32.to_le_bytes());
        set[20..24].copy_from_slice(&7u32.to_le_bytes());
        set[28..32].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(unsafe { SePrivilegeCheck(set.as_ptr().cast(), (&context as *const _).cast_mut(), 1) }, 1);
        set[20..24].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(unsafe { SePrivilegeCheck(set.as_ptr().cast(), (&context as *const _).cast_mut(), 1) }, 0);
        set[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(unsafe { SePrivilegeCheck(set.as_ptr().cast(), (&context as *const _).cast_mut(), 1) }, 1);
        assert_eq!(unsafe { se_internal_UnregisterTokenMetadata(token) }, STATUS_SUCCESS);
    }
}

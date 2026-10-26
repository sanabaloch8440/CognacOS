//! ALPC advanced resources: bounded section-view descriptors, security contexts,
//! and completion-list queues. This layer owns metadata/lifetimes only; the VM
//! mapping operation and user-handle publication must be performed by the
//! syscall/Object Manager adapters before exposing these records to user mode.
use core::sync::atomic::{AtomicBool, Ordering};

pub const STATUS_SUCCESS: u32 = 0;
pub const STATUS_INVALID_PARAMETER: u32 = 0xC000_000D;
pub const STATUS_INVALID_HANDLE: u32 = 0xC000_0008;
pub const STATUS_ACCESS_DENIED: u32 = 0xC000_0022;
pub const STATUS_INSUFFICIENT_RESOURCES: u32 = 0xC000_009A;
pub const STATUS_BUFFER_TOO_SMALL: u32 = 0xC000_0023;
pub const STATUS_PORT_DISCONNECTED: u32 = 0xC000_0037;
const MAX_VIEWS: usize = 32;
const MAX_SECURITY_CONTEXTS: usize = 32;
const MAX_COMPLETION_LISTS: usize = 8;
const MAX_COMPLETIONS: usize = 64;
const PAGE_SIZE: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionView {
    pub id: u32,
    pub owner_pid: u64,
    pub port_id: u32,
    pub section_object: u64,
    pub base: u64,
    pub size: u64,
    pub active: bool,
}
impl SectionView { const EMPTY: Self = Self { id: 0, owner_pid: 0, port_id: 0, section_object: 0, base: 0, size: 0, active: false }; }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecurityContext {
    pub id: u32,
    pub owner_pid: u64,
    pub peer_pid: u64,
    pub desired_access: u32,
    pub impersonation_level: u8,
    pub revoked: bool,
    pub active: bool,
}
impl SecurityContext { const EMPTY: Self = Self { id: 0, owner_pid: 0, peer_pid: 0, desired_access: 0, impersonation_level: 0, revoked: false, active: false }; }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub sequence: u64,
    pub message_id: u64,
    pub context: u64,
    pub status: u32,
    pub information: u64,
}
impl Completion { const EMPTY: Self = Self { sequence: 0, message_id: 0, context: 0, status: 0, information: 0 }; }

#[derive(Clone, Copy)]
struct CompletionList {
    active: bool,
    id: u32,
    owner_pid: u64,
    port_id: u32,
    concurrency: u32,
    next_sequence: u64,
    head: usize,
    len: usize,
    entries: [Completion; MAX_COMPLETIONS],
}
impl CompletionList {
    const EMPTY: Self = Self { active: false, id: 0, owner_pid: 0, port_id: 0, concurrency: 1, next_sequence: 1, head: 0, len: 0, entries: [Completion::EMPTY; MAX_COMPLETIONS] };
}
struct State {
    views: [SectionView; MAX_VIEWS],
    contexts: [SecurityContext; MAX_SECURITY_CONTEXTS],
    lists: [CompletionList; MAX_COMPLETION_LISTS],
    next_id: u32,
}
impl State {
    const fn new() -> Self { Self { views: [SectionView::EMPTY; MAX_VIEWS], contexts: [SecurityContext::EMPTY; MAX_SECURITY_CONTEXTS], lists: [CompletionList::EMPTY; MAX_COMPLETION_LISTS], next_id: 1 } }
}
struct Shared { lock: AtomicBool, state: core::cell::UnsafeCell<State> }
unsafe impl Sync for Shared {}
static STATE: Shared = Shared { lock: AtomicBool::new(false), state: core::cell::UnsafeCell::new(State::new()) };
struct Guard;
impl Guard {
    fn acquire() -> Self {
        while STATE.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); }
        Self
    }
    fn state(&mut self) -> &mut State { unsafe { &mut *STATE.state.get() } }
}
impl Drop for Guard { fn drop(&mut self) { STATE.lock.store(false, Ordering::Release); } }
fn alloc_id(s: &mut State) -> u32 { let id = s.next_id.max(1); s.next_id = id.wrapping_add(1).max(1); id }

/// Register an already mapped view. This does not map pages; the MM/syscall
/// adapter must validate the backing section and create the mapping first.
pub fn register_section_view(owner_pid: u64, port_id: u32, section_object: u64, base: u64, size: u64) -> Result<u32, u32> {
    if owner_pid == 0 || port_id == 0 || section_object == 0 || base == 0 || size == 0
        || base % PAGE_SIZE != 0 || size % PAGE_SIZE != 0 || base.checked_add(size).is_none()
        || base.checked_add(size).is_some_and(|end| end > 0x0000_8000_0000_0000) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let mut g = Guard::acquire();
    let s = g.state();
    if s.views.iter().any(|v| v.active && v.owner_pid == owner_pid && v.base < base + size && base < v.base + v.size) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let slot = s.views.iter().position(|v| !v.active).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let id = alloc_id(s);
    s.views[slot] = SectionView { id, owner_pid, port_id, section_object, base, size, active: true };
    Ok(id)
}
pub fn query_section_view(id: u32, owner_pid: u64) -> Result<SectionView, u32> {
    let g = Guard::acquire();
    let s = unsafe { &*STATE.state.get() };
    let v = s.views.iter().find(|v| v.active && v.id == id).copied().ok_or(STATUS_INVALID_HANDLE)?;
    if v.owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    Ok(v)
}
/// Remove view metadata after the VM adapter has successfully unmapped it.
pub fn unregister_section_view(id: u32, owner_pid: u64) -> Result<SectionView, u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let i = s.views.iter().position(|v| v.active && v.id == id).ok_or(STATUS_INVALID_HANDLE)?;
    if s.views[i].owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    let old = s.views[i]; s.views[i] = SectionView::EMPTY; Ok(old)
}
pub fn create_security_context(owner_pid: u64, peer_pid: u64, desired_access: u32, impersonation_level: u8) -> Result<u32, u32> {
    // SecurityImpersonation (2) is the highest level accepted by this core;
    // identification/anonymous are permitted, delegation is not.
    if owner_pid == 0 || peer_pid == 0 || desired_access == 0 || impersonation_level > 2 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let mut g = Guard::acquire();
    let s = g.state();
    let i = s.contexts.iter().position(|c| !c.active).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let id = alloc_id(s);
    s.contexts[i] = SecurityContext { id, owner_pid, peer_pid, desired_access, impersonation_level, revoked: false, active: true };
    Ok(id)
}
pub fn query_security_context(id: u32, owner_pid: u64) -> Result<SecurityContext, u32> {
    let g = Guard::acquire();
    let s = unsafe { &*STATE.state.get() };
    let c = s.contexts.iter().find(|c| c.active && c.id == id).copied().ok_or(STATUS_INVALID_HANDLE)?;
    if c.owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    if c.revoked { return Err(STATUS_PORT_DISCONNECTED); }
    Ok(c)
}
pub fn revoke_security_context(id: u32, owner_pid: u64) -> Result<(), u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let c = s.contexts.iter_mut().find(|c| c.active && c.id == id).ok_or(STATUS_INVALID_HANDLE)?;
    if c.owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    c.revoked = true; Ok(())
}
pub fn delete_security_context(id: u32, owner_pid: u64) -> Result<(), u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let i = s.contexts.iter().position(|c| c.active && c.id == id).ok_or(STATUS_INVALID_HANDLE)?;
    if s.contexts[i].owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    s.contexts[i] = SecurityContext::EMPTY; Ok(())
}
pub fn register_completion_list(owner_pid: u64, port_id: u32, concurrency: u32) -> Result<u32, u32> {
    if owner_pid == 0 || port_id == 0 || concurrency == 0 || concurrency > MAX_COMPLETIONS as u32 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let mut g = Guard::acquire();
    let s = g.state();
    if s.lists.iter().any(|l| l.active && l.owner_pid == owner_pid && l.port_id == port_id) { return Err(STATUS_INVALID_PARAMETER); }
    let i = s.lists.iter().position(|l| !l.active).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let id = alloc_id(s);
    let mut list = CompletionList::EMPTY; list.active = true; list.id = id; list.owner_pid = owner_pid; list.port_id = port_id; list.concurrency = concurrency;
    s.lists[i] = list; Ok(id)
}
pub fn post_completion(list_id: u32, owner_pid: u64, message_id: u64, context: u64, status: u32, information: u64) -> Result<u64, u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let l = s.lists.iter_mut().find(|l| l.active && l.id == list_id).ok_or(STATUS_INVALID_HANDLE)?;
    if l.owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    if l.len == MAX_COMPLETIONS { return Err(STATUS_BUFFER_TOO_SMALL); }
    let sequence = l.next_sequence; l.next_sequence = l.next_sequence.wrapping_add(1).max(1);
    let at = (l.head + l.len) % MAX_COMPLETIONS;
    l.entries[at] = Completion { sequence, message_id, context, status, information };
    l.len += 1; Ok(sequence)
}
pub fn dequeue_completion(list_id: u32, owner_pid: u64) -> Result<Completion, u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let l = s.lists.iter_mut().find(|l| l.active && l.id == list_id).ok_or(STATUS_INVALID_HANDLE)?;
    if l.owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    if l.len == 0 { return Err(STATUS_PORT_DISCONNECTED); }
    let item = l.entries[l.head]; l.entries[l.head] = Completion::EMPTY;
    l.head = (l.head + 1) % MAX_COMPLETIONS; l.len -= 1; Ok(item)
}
pub fn completion_count(list_id: u32, owner_pid: u64) -> Result<usize, u32> {
    let g = Guard::acquire();
    let s = unsafe { &*STATE.state.get() };
    let l = s.lists.iter().find(|l| l.active && l.port_id == list_id).ok_or(STATUS_INVALID_HANDLE)?;
    if l.owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    Ok(l.len)
}
pub fn unregister_completion_list(list_id: u32, owner_pid: u64) -> Result<(), u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let i = s.lists.iter().position(|l| l.active && l.port_id == list_id).ok_or(STATUS_INVALID_HANDLE)?;
    if s.lists[i].owner_pid != owner_pid { return Err(STATUS_ACCESS_DENIED); }
    s.lists[i] = CompletionList::EMPTY; Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn view_ranges_are_aligned_and_owner_checked() {
        let id = register_section_view(7001, 41, 99, 0x10000, 0x2000).unwrap();
        assert_eq!(query_section_view(id, 7002), Err(STATUS_ACCESS_DENIED));
        assert_eq!(query_section_view(id, 7001).unwrap().size, 0x2000);
        assert_eq!(register_section_view(7001, 41, 99, 0x11000, 0x1000), Err(STATUS_INVALID_PARAMETER));
        unregister_section_view(id, 7001).unwrap();
    }
    #[test] fn security_contexts_can_be_revoked_and_deleted() {
        let id = create_security_context(7003, 7004, 3, 2).unwrap();
        assert_eq!(query_security_context(id, 7003).unwrap().peer_pid, 7004);
        assert_eq!(revoke_security_context(id, 7003), Ok(()));
        assert_eq!(query_security_context(id, 7003), Err(STATUS_PORT_DISCONNECTED));
        assert_eq!(query_security_context(id, 7004), Err(STATUS_ACCESS_DENIED));
        delete_security_context(id, 7003).unwrap();
    }
    #[test] fn completion_list_is_fifo_and_bounded() {
        let id = register_completion_list(7005, 42, 2).unwrap();
        post_completion(id, 7005, 100, 9, STATUS_SUCCESS, 16).unwrap();
        post_completion(id, 7005, 101, 10, STATUS_ACCESS_DENIED, 0).unwrap();
        assert_eq!(completion_count(id, 7005), Ok(2));
        assert_eq!(dequeue_completion(id, 7005).unwrap().message_id, 100);
        assert_eq!(dequeue_completion(id, 7005).unwrap().message_id, 101);
        assert_eq!(dequeue_completion(id, 7005), Err(STATUS_PORT_DISCONNECTED));
        unregister_completion_list(id, 7005).unwrap();
    }
}

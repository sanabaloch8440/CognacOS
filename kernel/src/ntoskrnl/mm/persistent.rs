//! Persistent-memory validation, token lifetime, and cache-line persistence.
//! A token can only be created for a range explicitly marked EFI_MEMORY_NV by
//! the firmware map. This module does not infer persistence from ordinary RAM.
use core::sync::atomic::{AtomicBool, Ordering};

const PAGE: u64 = 4096;
const MAX_TOKENS: usize = 16;
const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;
const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;
const STATUS_NO_MEMORY: i32 = 0xC000_0017u32 as i32;

#[derive(Clone, Copy)]
struct Token { live: bool, generation: u32, base: u64, length: u64, references: u32 }
impl Token { const EMPTY: Self = Self { live:false, generation:1, base:0, length:0, references:0 }; }
struct Spin(AtomicBool);
impl Spin {
    const fn new() -> Self { Self(AtomicBool::new(false)) }
    fn lock(&self) -> Guard<'_> {
        while self.0.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); }
        Guard(&self.0)
    }
}
struct Guard<'a>(&'a AtomicBool);
impl Drop for Guard<'_> { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }
static LOCK: Spin = Spin::new();
static mut TOKENS: [Token; MAX_TOKENS] = [Token::EMPTY; MAX_TOKENS];

fn decode(handle: u64) -> Option<(usize,u32)> {
    let low = (handle & 0xff) as usize; let generation = (handle >> 8) as u32;
    if low == 0 || low > MAX_TOKENS || generation == 0 { None } else { Some((low-1,generation)) }
}
fn range_is_nv(base: u64, length: u64) -> bool {
    if length == 0 || base.checked_add(length).is_none() { return false; }
    let end = base + length;
    let mut page = base & !(PAGE-1);
    while page < end {
        if !super::physical::is_nonvolatile_physical_address(page) { return false; }
        let Some(next) = page.checked_add(PAGE) else { return false; };
        page = next;
    }
    true
}

/// Create a generation-tagged token for a firmware-declared persistent physical range.
pub fn get_token(physical_base: u64, length: u64, out: &mut u64) -> i32 {
    if physical_base & (PAGE-1) != 0 || !range_is_nv(physical_base,length) { return STATUS_INVALID_PARAMETER; }
    let _guard = LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKENS) };
    let Some(slot) = entries.iter().position(|t| !t.live) else { return STATUS_NO_MEMORY; };
    let generation = entries[slot].generation.max(1);
    entries[slot] = Token { live:true, generation, base:physical_base, length, references:1 };
    *out = ((generation as u64) << 8) | (slot as u64 + 1);
    STATUS_SUCCESS
}
pub fn reference(handle: u64) -> i32 {
    let Some((slot,generation)) = decode(handle) else { return STATUS_INVALID_HANDLE; };
    let _guard = LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKENS) };
    let Some(token) = entries.get_mut(slot).filter(|t| t.live && t.generation == generation) else { return STATUS_INVALID_HANDLE; };
    let Some(next) = token.references.checked_add(1) else { return STATUS_NO_MEMORY; };
    token.references = next; STATUS_SUCCESS
}
pub fn free_token(handle: u64) -> i32 {
    let Some((slot,generation)) = decode(handle) else { return STATUS_INVALID_HANDLE; };
    let _guard = LOCK.lock();
    let entries = unsafe { &mut *core::ptr::addr_of_mut!(TOKENS) };
    let Some(token) = entries.get_mut(slot).filter(|t| t.live && t.generation == generation) else { return STATUS_INVALID_HANDLE; };
    if token.references == 0 { return STATUS_INVALID_PARAMETER; }
    token.references -= 1;
    if token.references == 0 {
        let next_generation = token.generation.wrapping_add(1).max(1);
        entries[slot] = Token { generation:next_generation, ..Token::EMPTY };
    }
    STATUS_SUCCESS
}
fn token_range(handle: u64, offset: u64, length: u64) -> Result<u64,i32> {
    let Some((slot,generation)) = decode(handle) else { return Err(STATUS_INVALID_HANDLE); };
    let _guard = LOCK.lock();
    let entries = unsafe { &*core::ptr::addr_of!(TOKENS) };
    let token = entries.get(slot).filter(|t| t.live && t.generation == generation).ok_or(STATUS_INVALID_HANDLE)?;
    let end = offset.checked_add(length).ok_or(STATUS_INVALID_PARAMETER)?;
    if length == 0 || end > token.length { return Err(STATUS_INVALID_PARAMETER); }
    token.base.checked_add(offset).ok_or(STATUS_INVALID_PARAMETER)
}
fn clflush_supported() -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf = core::arch::x86_64::__cpuid(1);
        if leaf.edx & (1 << 19) == 0 { return None; }
        let line = (((leaf.ebx >> 8) & 0xff) * 8) as usize;
        if line < 16 || !line.is_power_of_two() { None } else { Some(line) }
    }
    #[cfg(not(target_arch = "x86_64"))]
    { None }
}
/// Persist a token-relative range using CLFLUSH and (unless no_drain) SFENCE.
/// The firmware memory map must still identify every covered page as EFI_MEMORY_NV.
pub fn flush(handle: u64, offset: u64, length: u64, no_drain: bool) -> i32 {
    let physical = match token_range(handle,offset,length) { Ok(v)=>v, Err(s)=>return s };
    if !range_is_nv(physical,length) { return STATUS_INVALID_PARAMETER; }
    let Some(line_size) = clflush_supported() else { return STATUS_NOT_SUPPORTED; };
    let Some(pointer) = super::physical::direct_map_address(physical) else { return STATUS_INVALID_PARAMETER; };
    let Some(end) = pointer.checked_add(length) else { return STATUS_INVALID_PARAMETER; };
    let mut line = pointer & !((line_size as u64)-1);
    while line < end {
        unsafe { core::arch::asm!("clflush [{}]", in(reg) line, options(nostack, preserves_flags)); }
        line = match line.checked_add(line_size as u64) { Some(v)=>v, None=>return STATUS_INVALID_PARAMETER };
    }
    if !no_drain { unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)); } }
    STATUS_SUCCESS
}
pub fn write(handle: u64, offset: u64, source: &[u8], persist: bool) -> i32 {
    let physical = match token_range(handle,offset,source.len() as u64) { Ok(v)=>v, Err(s)=>return s };
    if !range_is_nv(physical,source.len() as u64) { return STATUS_INVALID_PARAMETER; }
    let Some(pointer) = super::physical::direct_map_address(physical) else { return STATUS_INVALID_PARAMETER; };
    unsafe { core::ptr::copy_nonoverlapping(source.as_ptr(), pointer as *mut u8, source.len()); }
    if persist { flush(handle,offset,source.len() as u64,false) } else { STATUS_SUCCESS }
}
pub fn fill(handle: u64, offset: u64, length: u64, value: u8, persist: bool) -> i32 {
    let physical = match token_range(handle,offset,length) { Ok(v)=>v, Err(s)=>return s };
    if !range_is_nv(physical,length) { return STATUS_INVALID_PARAMETER; }
    let Some(pointer) = super::physical::direct_map_address(physical) else { return STATUS_INVALID_PARAMETER; };
    unsafe { core::ptr::write_bytes(pointer as *mut u8, value, length as usize); }
    if persist { flush(handle,offset,length,false) } else { STATUS_SUCCESS }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_persistent_tokens_fail_closed() {
        assert_eq!(reference(0), STATUS_INVALID_HANDLE);
        assert_eq!(free_token(0), STATUS_INVALID_HANDLE);
        assert_eq!(flush(0,0,1,false), STATUS_INVALID_HANDLE);
    }
}

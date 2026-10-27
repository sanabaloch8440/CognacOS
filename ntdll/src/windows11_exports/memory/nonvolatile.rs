//! Nonvolatile memory, memory-copy, and secure-memory-cache exports.
#![allow(non_snake_case, clippy::missing_safety_doc)]
use core::{ffi::c_void, ptr, sync::atomic::{AtomicBool, Ordering}};
use super::virtual_memory::{self, PVOID, SIZE_T, ULONG, NTSTATUS};
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;

#[no_mangle]
pub unsafe extern "system" fn RtlCopyMemoryNonTemporal(destination: PVOID, source: *const c_void, length: SIZE_T) {
    if length == 0 || destination.cast_const() == source { return; }
    unsafe { ptr::copy_nonoverlapping(source.cast::<u8>(), destination.cast::<u8>(), length); }
}
#[no_mangle]
pub unsafe extern "system" fn RtlFillMemoryNonTemporal(destination: PVOID, length: SIZE_T, fill: u8) {
    if length == 0 { return; }
    unsafe { ptr::write_bytes(destination.cast::<u8>(), fill, length); }
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNonVolatileToken(base: PVOID, length: SIZE_T, token_out: *mut PVOID) -> NTSTATUS {
    if base.is_null() || length == 0 || token_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { virtual_memory::cognacos_rtl_get_nonvolatile_token(base, length, token_out) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlFreeNonVolatileToken(token: PVOID) -> NTSTATUS {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { virtual_memory::cognacos_rtl_free_nonvolatile_token(token) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlFillNonVolatileMemory(token: PVOID, destination: PVOID, value: u8, length: SIZE_T, flags: ULONG) -> NTSTATUS {
    if token.is_null() || destination.is_null() || length == 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { virtual_memory::cognacos_rtl_fill_nonvolatile_memory(token, destination, length, value, flags) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlWriteNonVolatileMemory(token: PVOID, destination: PVOID, source: *const c_void, length: SIZE_T, flags: ULONG) -> NTSTATUS {
    if token.is_null() || destination.is_null() || source.is_null() || length == 0 || flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { virtual_memory::cognacos_rtl_write_nonvolatile_memory(token, destination, source, length, flags) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemory(token: PVOID, buffer: PVOID, length: SIZE_T, flags: ULONG) -> NTSTATUS {
    if token.is_null() || buffer.is_null() || length == 0 || flags & !1 != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { virtual_memory::cognacos_rtl_flush_nonvolatile_memory(token, buffer, length, flags) }
}
#[repr(C)]
pub struct RTL_NV_MEMORY_RANGE { pub base_address: PVOID, pub length: SIZE_T }
#[no_mangle]
pub unsafe extern "system" fn RtlFlushNonVolatileMemoryRanges(token: PVOID, count: ULONG, ranges: *const RTL_NV_MEMORY_RANGE, flags: ULONG) -> NTSTATUS {
    if token.is_null() || (count != 0 && ranges.is_null()) || flags & !1 != 0 || count > 1024 { return STATUS_INVALID_PARAMETER; }
    for index in 0..count as usize {
        let range = unsafe { ptr::read_unaligned(ranges.add(index)) };
        let status = unsafe { RtlFlushNonVolatileMemory(token, range.base_address, range.length, flags) };
        if status < 0 { return status; }
    }
    STATUS_SUCCESS
}
const SECURE_CACHE_CALLBACKS: usize = 16;
static SECURE_CACHE_CALLBACK_LOCK: AtomicBool = AtomicBool::new(false);
static mut SECURE_CACHE_CALLBACK_TABLE: [usize; SECURE_CACHE_CALLBACKS] = [0; SECURE_CACHE_CALLBACKS];
fn secure_cache_lock() { while SECURE_CACHE_CALLBACK_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); } }
fn secure_cache_unlock() { SECURE_CACHE_CALLBACK_LOCK.store(false, Ordering::Release); }
pub(super) fn notify_secure_cache_callbacks(base: PVOID, length: SIZE_T) {
    let mut callbacks = [0usize; SECURE_CACHE_CALLBACKS];
    secure_cache_lock();
    unsafe { callbacks.copy_from_slice(&*core::ptr::addr_of!(SECURE_CACHE_CALLBACK_TABLE)); }
    secure_cache_unlock();
    for address in callbacks.into_iter().filter(|p| *p != 0) {
        let callback: unsafe extern "system" fn(PVOID, SIZE_T) = unsafe { core::mem::transmute(address) };
        unsafe { callback(base, length); }
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlFlushSecureMemoryCache(base: PVOID, length: SIZE_T) -> u8 {
    if length == 0 { return 1; }
    if base.is_null() || (base as usize).checked_add(length).is_none() { return 0; }
    // This operation is the cache-invalidation notification hook; it does not
    // claim to pin pages itself. The MM secure-range contract is enforced by Nt VM.
    notify_secure_cache_callbacks(base, length);
    1
}
#[no_mangle]
pub unsafe extern "system" fn RtlRegisterSecureMemoryCacheCallback(callback: PVOID) -> NTSTATUS {
    if callback.is_null() { return STATUS_INVALID_PARAMETER; }
    let address = callback as usize;
    secure_cache_lock();
    let table = unsafe { &mut *core::ptr::addr_of_mut!(SECURE_CACHE_CALLBACK_TABLE) };
    if table.contains(&address) { secure_cache_unlock(); return STATUS_SUCCESS; }
    let Some(slot) = table.iter_mut().find(|p| **p == 0) else { secure_cache_unlock(); return STATUS_NO_MEMORY; };
    *slot = address;
    secure_cache_unlock();
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlDeregisterSecureMemoryCacheCallback(callback: PVOID) -> NTSTATUS {
    if callback.is_null() { return STATUS_INVALID_PARAMETER; }
    let address = callback as usize;
    secure_cache_lock();
    let table = unsafe { &mut *core::ptr::addr_of_mut!(SECURE_CACHE_CALLBACK_TABLE) };
    let Some(slot) = table.iter_mut().find(|p| **p == address) else { secure_cache_unlock(); return STATUS_INVALID_PARAMETER; };
    *slot = 0;
    secure_cache_unlock();
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlCopyMappedMemory(destination: PVOID, source: PVOID, length: SIZE_T) -> NTSTATUS {
    if length != 0 && (destination.is_null() || source.is_null()) { return STATUS_INVALID_PARAMETER; }
    if length != 0 { unsafe { ptr::copy(source.cast::<u8>(), destination.cast::<u8>(), length); } }
    STATUS_SUCCESS
}

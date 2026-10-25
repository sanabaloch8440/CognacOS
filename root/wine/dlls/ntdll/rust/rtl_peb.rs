//! Native x64 PEB/TEB access and RTL critical-section primitives.
//!
//! ABI references: Windows x64 TEB (GS-relative offsets), PEB, and
//! RTL_CRITICAL_SECTION. The native PE currently implements the same exports
//! in cognac_native.c; this file is the Rust source port for the NTDLL Rust
//! migration. Do not link this module alongside the C definitions.
//!
//! The lock is recursive and uses the ABI's LockCount/RecursionCount/
//! OwningThread fields. Contenders yield after bounded spinning. It does not
//! yet implement the Windows event-backed waiter wake-up policy, so it is a
//! bootstrap synchronization primitive, not a full replacement for the
//! Windows critical-section algorithm under heavy contention.

#![allow(non_camel_case_types, non_snake_case)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

pub type PVOID = *mut c_void;
pub type LONG = i32;
pub type NTSTATUS = i32;
pub type HANDLE = *mut c_void;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_NOT_OWNER: NTSTATUS = 0xC000_005Au32 as i32;

#[repr(C)]
pub struct RTL_CRITICAL_SECTION {
    pub DebugInfo: *mut c_void,
    pub LockCount: LONG,
    pub RecursionCount: LONG,
    pub OwningThread: HANDLE,
    pub LockSemaphore: HANDLE,
    pub SpinCount: usize,
}

extern "system" {
    fn NtYieldExecution() -> NTSTATUS;
}

/// Read the current thread's TEB from the Windows x64 GS segment.
///
/// # Safety
/// The process must have installed a valid Windows-layout TEB in user GS.
#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentTeb() -> PVOID {
    let teb: PVOID;
    unsafe { core::arch::asm!("mov {}, gs:[0x30]", out(reg) teb, options(nostack, preserves_flags, readonly)) };
    teb
}

extern "system" { fn RtlGetCurrentPeb() -> PVOID; }

/// Acquire a Windows-layout recursive critical section.
///
/// The caller must supply a correctly initialized RTL_CRITICAL_SECTION whose
/// LockCount is -1 while unowned. This routine does not initialize objects.
#[no_mangle]
pub unsafe extern "system" fn RtlEnterCriticalSection(cs: *mut RTL_CRITICAL_SECTION) -> NTSTATUS {
    if cs.is_null() { return STATUS_INVALID_PARAMETER; }
    let tid = unsafe { current_thread_id() };
    if tid.is_null() { return STATUS_INVALID_PARAMETER; }
    let owner = unsafe { &*(core::ptr::addr_of!((*cs).OwningThread) as *const AtomicPtr<c_void>) };
    if owner.load(Ordering::Acquire) == tid {
        let recursion = unsafe { &*(core::ptr::addr_of!((*cs).RecursionCount) as *const AtomicI32) };
        recursion.fetch_add(1, Ordering::Relaxed);
        return STATUS_SUCCESS;
    }

    let lock = unsafe { &*(core::ptr::addr_of!((*cs).LockCount) as *const AtomicI32) };
    let recursion = unsafe { &*(core::ptr::addr_of!((*cs).RecursionCount) as *const AtomicI32) };
    let mut spins = 0usize;
    loop {
        if lock.compare_exchange(-1, 0, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            recursion.store(1, Ordering::Relaxed);
            owner.store(tid, Ordering::Release);
            return STATUS_SUCCESS;
        }
        #[cfg(target_arch = "x86_64")]
        unsafe { core::arch::asm!("pause", options(nomem, nostack, preserves_flags)) };
        spins += 1;
        if spins >= 4096 {
            spins = 0;
            let _ = unsafe { NtYieldExecution() };
        }
    }
}

/// Release a critical section held by the current thread.
#[no_mangle]
pub unsafe extern "system" fn RtlLeaveCriticalSection(cs: *mut RTL_CRITICAL_SECTION) -> NTSTATUS {
    if cs.is_null() { return STATUS_INVALID_PARAMETER; }
    let tid = unsafe { current_thread_id() };
    let owner = unsafe { &*(core::ptr::addr_of!((*cs).OwningThread) as *const AtomicPtr<c_void>) };
    if owner.load(Ordering::Acquire) != tid { return STATUS_NOT_OWNER; }

    let recursion = unsafe { &*(core::ptr::addr_of!((*cs).RecursionCount) as *const AtomicI32) };
    let remaining = recursion.fetch_sub(1, Ordering::Relaxed) - 1;
    if remaining > 0 { return STATUS_SUCCESS; }
    if remaining < 0 {
        recursion.store(0, Ordering::Relaxed);
        return STATUS_INVALID_PARAMETER;
    }

    let lock = unsafe { &*(core::ptr::addr_of!((*cs).LockCount) as *const AtomicI32) };
    owner.store(core::ptr::null_mut(), Ordering::Release);
    lock.store(-1, Ordering::Release);
    STATUS_SUCCESS
}

/// Acquire PEB.FastPebLock when the process runtime has initialized it.
///
/// Early bootstrap may legitimately have no PEB or no FastPebLock yet.
#[no_mangle]
pub unsafe extern "system" fn RtlAcquirePebLock() {
    let peb = unsafe { RtlGetCurrentPeb() };
    if peb.is_null() { return; }
    let lock = unsafe { core::ptr::read_unaligned(peb.cast::<u8>().add(0x38).cast::<*mut RTL_CRITICAL_SECTION>()) };
    if !lock.is_null() { let _ = unsafe { RtlEnterCriticalSection(lock) }; }
}

/// Release PEB.FastPebLock if initialized.
#[no_mangle]
pub unsafe extern "system" fn RtlReleasePebLock() {
    let peb = unsafe { RtlGetCurrentPeb() };
    if peb.is_null() { return; }
    let lock = unsafe { core::ptr::read_unaligned(peb.cast::<u8>().add(0x38).cast::<*mut RTL_CRITICAL_SECTION>()) };
    if !lock.is_null() { let _ = unsafe { RtlLeaveCriticalSection(lock) }; }
}

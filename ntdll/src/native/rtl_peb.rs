//! Native x64 PEB/TEB access and RTL critical-section primitives.
//!
//! ABI references: Windows x64 TEB (GS-relative offsets), PEB, and
//! RTL_CRITICAL_SECTION. The native PE currently implements the same exports
//! in cognac_native.c; this file is the Rust source port for the NTDLL Rust
//! migration. Do not link this module alongside the C definitions.
//!
//! The lock is recursive and uses the ABI's LockCount/RecursionCount/
//! OwningThread fields. Contenders use a lazily-created synchronization event
//! stored in LockSemaphore and wait/signal through native NT dispatcher services.

#![allow(non_camel_case_types, non_snake_case)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

pub use crate::nt_abi::types::{HANDLE, LONG, PVOID};
pub use crate::nt_abi::ntstatus::{NTSTATUS, STATUS_INVALID_PARAMETER, STATUS_NOT_OWNER, STATUS_SUCCESS};
const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;


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

extern "system" {
    fn NtCreateEvent(event_handle: *mut HANDLE, desired_access: u32, object_attributes: *const c_void, event_type: u32, initial_state: u32) -> NTSTATUS;
    fn NtSetEvent(event_handle: HANDLE, previous_state: *mut LONG) -> NTSTATUS;
    fn NtWaitForSingleObject(handle: HANDLE, alertable: u8, timeout: *const i64) -> NTSTATUS;
    fn NtClose(handle: HANDLE) -> NTSTATUS;
}

const EVENT_MODIFY_STATE: u32 = 0x0002;
const SYNCHRONIZE: u32 = 0x0010_0000;
const SYNCHRONIZATION_EVENT: u32 = 1;
const SPIN_COUNT_MASK: usize = 0x00FF_FFFF;

#[inline]
unsafe fn ensure_lock_semaphore(cs: *mut RTL_CRITICAL_SECTION) -> Result<HANDLE, NTSTATUS> {
    let semaphore = unsafe { &*(core::ptr::addr_of!((*cs).LockSemaphore) as *const AtomicPtr<c_void>) };
    let existing = semaphore.load(Ordering::Acquire);
    if !existing.is_null() { return Ok(existing); }

    let mut created: HANDLE = core::ptr::null_mut();
    let status = unsafe {
        NtCreateEvent(&mut created, EVENT_MODIFY_STATE | SYNCHRONIZE, core::ptr::null(), SYNCHRONIZATION_EVENT, 0)
    };
    if status < 0 { return Err(status); }
    if created.is_null() { return Err(STATUS_INVALID_PARAMETER); }

    match semaphore.compare_exchange(core::ptr::null_mut(), created, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            // Close the race where the owner released before LockSemaphore was
            // published. The waiter has already incremented LockCount, so an
            // unsignalled event here would otherwise lose the handoff.
            let owner = unsafe { &*(core::ptr::addr_of!((*cs).OwningThread) as *const AtomicPtr<c_void>) };
            let lock = unsafe { &*(core::ptr::addr_of!((*cs).LockCount) as *const AtomicI32) };
            if owner.load(Ordering::Acquire).is_null() && lock.load(Ordering::Acquire) >= 0 {
                let _ = unsafe { NtSetEvent(created, core::ptr::null_mut()) };
            }
            Ok(created)
        }
        Err(winner) => {
            let _ = unsafe { NtClose(created) };
            Ok(winner)
        }
    }
}
#[inline]
unsafe fn current_thread_id() -> HANDLE {
    let tid: HANDLE;
    unsafe { core::arch::asm!("mov {}, gs:[0x48]", out(reg) tid, options(nostack, preserves_flags, readonly)) };
    tid
}

/// Read the current thread's TEB from the Windows x64 GS segment.
///
/// # Safety
/// The process must have installed a valid Windows-layout TEB in user GS.
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
    let lock = unsafe { &*(core::ptr::addr_of!((*cs).LockCount) as *const AtomicI32) };
    let recursion = unsafe { &*(core::ptr::addr_of!((*cs).RecursionCount) as *const AtomicI32) };

    if owner.load(Ordering::Acquire) == tid {
        lock.fetch_add(1, Ordering::Relaxed);
        recursion.fetch_add(1, Ordering::Relaxed);
        return STATUS_SUCCESS;
    }

    let previous = lock.fetch_add(1, Ordering::Acquire);
    if previous == -1 {
        while owner.compare_exchange(core::ptr::null_mut(), tid, Ordering::AcqRel, Ordering::Acquire).is_err() {
            #[cfg(target_arch = "x86_64")]
            unsafe { core::arch::asm!("pause", options(nomem, nostack, preserves_flags)) };
            let _ = unsafe { NtYieldExecution() };
        }
        recursion.store(1, Ordering::Relaxed);
        return STATUS_SUCCESS;
    }

    let spin_limit = unsafe { (*cs).SpinCount & SPIN_COUNT_MASK };
    for _ in 0..spin_limit {
        if owner.load(Ordering::Acquire).is_null() { break; }
        #[cfg(target_arch = "x86_64")]
        unsafe { core::arch::asm!("pause", options(nomem, nostack, preserves_flags)) };
    }

    loop {
        let event = match unsafe { ensure_lock_semaphore(cs) } {
            Ok(handle) => handle,
            Err(status) if status == STATUS_NOT_IMPLEMENTED => {
                let _ = unsafe { NtYieldExecution() };
                continue;
            }
            Err(status) => return status,
        };
        let status = unsafe { NtWaitForSingleObject(event, 0, core::ptr::null()) };
        if status == STATUS_SUCCESS {
            if owner.compare_exchange(core::ptr::null_mut(), tid, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                recursion.store(1, Ordering::Relaxed);
                return STATUS_SUCCESS;
            }
        } else if status == STATUS_NOT_IMPLEMENTED {
            let _ = unsafe { NtYieldExecution() };
        } else {
            return status;
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
    if remaining > 0 {
        let lock = unsafe { &*(core::ptr::addr_of!((*cs).LockCount) as *const AtomicI32) };
        lock.fetch_sub(1, Ordering::Release);
        return STATUS_SUCCESS;
    }
    if remaining < 0 {
        recursion.store(0, Ordering::Relaxed);
        return STATUS_INVALID_PARAMETER;
    }

    let lock = unsafe { &*(core::ptr::addr_of!((*cs).LockCount) as *const AtomicI32) };
    let new_count = lock.fetch_sub(1, Ordering::AcqRel) - 1;
    owner.store(core::ptr::null_mut(), Ordering::Release);
    if new_count >= 0 {
        let semaphore = unsafe { &*(core::ptr::addr_of!((*cs).LockSemaphore) as *const AtomicPtr<c_void>) };
        let event = semaphore.load(Ordering::Acquire);
        if !event.is_null() {
            let status = unsafe { NtSetEvent(event, core::ptr::null_mut()) };
            if status < 0 { return status; }
        }
    }
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

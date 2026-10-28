//! Native x64 PEB/TEB access and RTL critical-section primitives.
//!
//! ABI references: Windows x64 TEB (GS-relative offsets), PEB, and
//! RTL_CRITICAL_SECTION. The native PE currently implements the same exports
//! in cognac_native.c; this file is the Rust source port for the NTDLL Rust
//! migration. Do not link this module alongside the C definitions.
//!
//! The lock is recursive and uses the ABI's LockCount/RecursionCount/
//! OwningThread fields. Contenders use CognacOS's private address-wait/wake
//! service after bounded spinning. This is not Windows' private event-backed
//! implementation, and blocking requires a scheduler-owned thread; bootstrap
//! contexts fall back to yielding/spinning rather than claiming to have waited.

#![allow(non_camel_case_types, non_snake_case)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

pub use crate::nt_abi::types::{HANDLE, LONG, PVOID};
pub use crate::nt_abi::ntstatus::{NTSTATUS, STATUS_INVALID_PARAMETER, STATUS_NOT_OWNER, STATUS_SUCCESS};


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

#[inline]
unsafe fn cognac_wait_on_lock(address:*mut LONG,expected:LONG)->NTSTATUS{
    #[cfg(all(target_os="windows",target_arch="x86_64"))]
    {
        let status:u64;
        unsafe{
            core::arch::asm!(
                "mov r10, rcx",
                "syscall",
                inlateout("rax") 0x152u64 => status,
                inlateout("rcx") address as u64 => _,
                in("rdx") expected as u32 as u64,
                in("r8") 4u64,
                in("r9") u64::MAX,
                lateout("r10") _,
                lateout("r11") _,
                options(nostack)
            );
        }
        return status as u32 as i32;
    }
    #[cfg(not(all(target_os="windows",target_arch="x86_64")))]
    { let _=(address,expected); STATUS_NOT_IMPLEMENTED }
}

#[inline]
unsafe fn cognac_wake_lock(address:*mut LONG){
    #[cfg(all(target_os="windows",target_arch="x86_64"))]
    {
        let status:u64;
        unsafe{
            core::arch::asm!(
                "mov r10, rcx",
                "syscall",
                inlateout("rax") 0x153u64 => status,
                inlateout("rcx") address as u64 => _,
                in("rdx") 0u64,
                in("r8") 0u64,
                in("r9") 0u64,
                lateout("r10") _,
                lateout("r11") _,
                options(nostack)
            );
        }
        let _=status;
    }
    #[cfg(not(all(target_os="windows",target_arch="x86_64")))]
    { let _=address; }
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
    if owner.load(Ordering::Acquire) == tid {
        let recursion = unsafe { &*(core::ptr::addr_of!((*cs).RecursionCount) as *const AtomicI32) };
        recursion.fetch_add(1, Ordering::Relaxed);
        return STATUS_SUCCESS;
    }

    let lock = unsafe { &*(core::ptr::addr_of!((*cs).LockCount) as *const AtomicI32) };
    let recursion = unsafe { &*(core::ptr::addr_of!((*cs).RecursionCount) as *const AtomicI32) };

    // RTL_CRITICAL_SECTION.SpinCount is a caller-configured spin budget, not a
    // fixed implementation constant. Respect the Windows x64 24-bit range;
    // zero means proceed to the blocking path immediately.
    let spin_limit = unsafe { (*cs).SpinCount.min(0x00FF_FFFF) };
    let mut spins = 0usize;
    loop {
        if lock.compare_exchange(-1, 0, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            recursion.store(1, Ordering::Relaxed);
            owner.store(tid, Ordering::Release);
            return STATUS_SUCCESS;
        }

        #[cfg(target_arch = "x86_64")]
        if spins < spin_limit {
            unsafe { core::arch::asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
        if spins < spin_limit {
            spins += 1;
            continue;
        }

        // The kernel service rechecks LockCount while registering the waiter,
        // so an unlock racing with this call is not lost. Early bootstrap may
        // still lack a scheduler-owned thread; in that case explicitly yield
        // and retry rather than treating STATUS_NOT_IMPLEMENTED as a completed
        // wait. A zero spin count reaches this path on the first contention.
        let observed = lock.load(Ordering::Acquire);
        if observed != -1 {
            let status = unsafe {
                cognac_wait_on_lock(core::ptr::addr_of_mut!((*cs).LockCount), observed)
            };
            if status == STATUS_NOT_IMPLEMENTED {
                let _ = unsafe { NtYieldExecution() };
            }
        } else {
            let _ = unsafe { NtYieldExecution() };
        }
        spins = 0;
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
    unsafe{cognac_wake_lock(core::ptr::addr_of_mut!((*cs).LockCount));}
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

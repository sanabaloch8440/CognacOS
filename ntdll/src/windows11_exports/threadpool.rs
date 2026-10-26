//! CognacOS native thread-pool substrate for NTDLL exports.
//! Work objects are heap-backed and synchronized. Without a native worker-thread
//! scheduler, posted callbacks are queued and drained by TpWaitForWork on the caller.
//! This is an explicit cooperative fallback, not full Windows thread-pool behavior.
#![allow(non_snake_case, clippy::missing_safety_doc, dead_code)]
use core::{ffi::c_void, ptr, sync::atomic::{AtomicBool, Ordering}};
type PVOID=*mut c_void; type NTSTATUS=i32;
const STATUS_SUCCESS:NTSTATUS=0; const STATUS_INVALID_PARAMETER:NTSTATUS=0xC000000Du32 as i32; const STATUS_NO_MEMORY:NTSTATUS=0xC0000017u32 as i32; const STATUS_NOT_IMPLEMENTED:NTSTATUS=0xC0000002u32 as i32;
const HEAP_ZERO_MEMORY:u32=8;
use crate::rtl_heap::RtlGetProcessHeap;
extern "system" { fn RtlAllocateHeap(heap:PVOID,flags:u32,size:usize)->PVOID; fn RtlFreeHeap(heap:PVOID,flags:u32,mem:PVOID)->u8; }
pub type PTP_WORK_CALLBACK=Option<unsafe extern "system" fn(instance:PVOID, context:PVOID, work:PVOID)>;
#[repr(C)] struct Work { magic:u64, callback:PTP_WORK_CALLBACK, context:PVOID, pending:bool, running:bool, cancelled:bool, pool:PVOID }
#[repr(C)] struct Pool { magic:u64, max_threads:u32, min_threads:u32 }
const WORK_MAGIC:u64=0x434F_474E_5450_574B; const POOL_MAGIC:u64=0x434F_474E_5450_504C;
static WORK_LOCK:AtomicBool=AtomicBool::new(false); struct Guard; impl Guard{fn lock()->Self{while WORK_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}Self}} impl Drop for Guard{fn drop(&mut self){WORK_LOCK.store(false,Ordering::Release);}}
unsafe fn alloc(size:usize)->PVOID{let h=unsafe{RtlGetProcessHeap()};if h.is_null(){ptr::null_mut()}else{unsafe{RtlAllocateHeap(h,HEAP_ZERO_MEMORY,size)}}}
unsafe fn free(p:PVOID){if !p.is_null(){unsafe{let _=RtlFreeHeap(RtlGetProcessHeap(),0,p);}}}
#[no_mangle] pub unsafe extern "system" fn TpAllocPool(out:*mut PVOID,_reserved:PVOID)->NTSTATUS{if out.is_null(){return STATUS_INVALID_PARAMETER;}let p=unsafe{alloc(core::mem::size_of::<Pool>())} as *mut Pool;if p.is_null(){return STATUS_NO_MEMORY;}unsafe{(*p).magic=POOL_MAGIC;(*p).max_threads=0;(*p).min_threads=0;out.write(p.cast());}STATUS_SUCCESS}
#[no_mangle] pub unsafe extern "system" fn TpReleasePool(pool:PVOID){if pool.is_null(){return;}let p=pool.cast::<Pool>();if unsafe{(*p).magic}==POOL_MAGIC{unsafe{(*p).magic=0;free(pool);}}}
#[no_mangle] pub unsafe extern "system" fn TpAllocWork(out:*mut PVOID,callback:PTP_WORK_CALLBACK,context:PVOID,environment:PVOID)->NTSTATUS{if out.is_null()||callback.is_none(){return STATUS_INVALID_PARAMETER;}let w=unsafe{alloc(core::mem::size_of::<Work>())} as *mut Work;if w.is_null(){return STATUS_NO_MEMORY;}unsafe{(*w).magic=WORK_MAGIC;(*w).callback=callback;(*w).context=context;(*w).pending=false;(*w).running=false;(*w).cancelled=false;(*w).pool=environment;out.write(w.cast());}STATUS_SUCCESS}
#[no_mangle] pub unsafe extern "system" fn TpPostWork(work:PVOID){if work.is_null(){return;}let _g=Guard::lock();let w=work.cast::<Work>();if unsafe{(*w).magic}==WORK_MAGIC&&!unsafe{(*w).cancelled}{unsafe{(*w).pending=true;}}}
#[no_mangle] pub unsafe extern "system" fn TpWaitForWork(work:PVOID, cancel_pending:u8){if work.is_null(){return;}let w=work.cast::<Work>();let callback;let context;{let _g=Guard::lock();if unsafe{(*w).magic}!=WORK_MAGIC{return;}if cancel_pending!=0{unsafe{(*w).pending=false;(*w).cancelled=true;}return;}if !unsafe{(*w).pending}||unsafe{(*w).running}{return;}unsafe{(*w).pending=false;(*w).running=true;}callback=unsafe{(*w).callback};context=unsafe{(*w).context};}if let Some(cb)=callback{unsafe{cb(ptr::null_mut(),context,work);}}{let _g=Guard::lock();if unsafe{(*w).magic}==WORK_MAGIC{unsafe{(*w).running=false;}}}}
#[no_mangle] pub unsafe extern "system" fn TpReleaseWork(work:PVOID){if work.is_null(){return;}let w=work.cast::<Work>();let _g=Guard::lock();if unsafe{(*w).magic}==WORK_MAGIC&&!unsafe{(*w).running}{unsafe{(*w).magic=0;free(work);}}}
#[no_mangle]
pub unsafe extern "system" fn TpAllocAlpcCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpAllocAlpcCompletionEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpAllocCleanupGroup() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpAllocIoCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpAllocJobNotification() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpAllocTimer() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpAllocWait() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpAlpcRegisterCompletionList() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpAlpcUnregisterCompletionList() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackDetectedUnrecoverableError() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackIndependent() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackLeaveCriticalSectionOnCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackMayRunLong() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackReleaseMutexOnCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackReleaseSemaphoreOnCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackSendAlpcMessageOnCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackSendPendingAlpcMessage() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackSetEventOnCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCallbackUnloadDllOnCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCancelAsyncIoOperation() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCaptureCaller() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpCheckTerminateWorker() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpDbgDumpHeapUsage() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpDbgSetLogRoutine() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpDisablePoolCallbackChecks() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpDisassociateCallback() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpIsTimerSet() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpQueryPoolStackInformation() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpReleaseAlpcCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpReleaseCleanupGroup() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpReleaseCleanupGroupMembers() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpReleaseIoCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpReleaseJobNotification() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpReleaseTimer() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpReleaseWait() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetDefaultPoolMaxThreads() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetDefaultPoolStackInformation() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolMaxThreads() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolMaxThreadsSoftLimit() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolMinThreads() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolStackInformation() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolThreadBasePriority() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolThreadCpuSets() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolWorkerThreadIdleTimeout() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetTimer() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetTimerEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetWait() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSetWaitEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpSimpleTryPost() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpStartAsyncIoOperation() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpTimerOutstandingCallbackCount() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpTrimPools() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpWaitForAlpcCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpWaitForIoCompletion() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpWaitForJobNotification() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpWaitForTimer() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpWaitForWait() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpWorkOnBehalfClearTicket() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn TpWorkOnBehalfSetTicket() -> i32 { STATUS_NOT_IMPLEMENTED }

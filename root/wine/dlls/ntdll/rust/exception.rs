//! Source-first Rust port of the portable parts of Wine ntdll/exception.c.
//!
//! This file implements vectored exception/continue handler registration and
//! the unhandled-exception filter. Machine-context manipulation, SEH dispatch,
//! signal integration, and XSAVE/XSTATE routines remain architecture/kernel
//! integration work and are not falsely emulated here.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

type ULONG = u32;
type LONG = i32;
type ULONG_PTR = usize;
type ULONG64 = u64;
type PVOID = *mut c_void;
type NTSTATUS = i32;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const EXCEPTION_CONTINUE_EXECUTION: LONG = -1;
const EXCEPTION_CONTINUE_SEARCH: LONG = 0;
const MAX_VECTORED_HANDLERS: usize = 128;

#[repr(C)]
pub struct EXCEPTION_RECORD {
    pub ExceptionCode: ULONG,
    pub ExceptionFlags: ULONG,
    pub ExceptionRecord: *mut EXCEPTION_RECORD,
    pub ExceptionAddress: PVOID,
    pub NumberParameters: ULONG,
    pub ExceptionInformation: [ULONG_PTR; 15],
}

#[repr(C)]
pub struct EXCEPTION_POINTERS {
    pub ExceptionRecord: *mut EXCEPTION_RECORD,
    pub ContextRecord: PVOID,
}

pub type PVECTORED_EXCEPTION_HANDLER = Option<unsafe extern "system" fn(*mut EXCEPTION_POINTERS) -> LONG>;
pub type PRTL_EXCEPTION_FILTER = Option<unsafe extern "system" fn(*mut EXCEPTION_POINTERS) -> LONG>;

#[derive(Clone, Copy)]
struct HandlerSlot {
    function: PVECTORED_EXCEPTION_HANDLER,
    first: bool,
    sequence: u64,
    kind: u8, // 1 = exception, 2 = continue
}

impl HandlerSlot {
    const EMPTY: Self = Self { function: None, first: false, sequence: 0, kind: 0 };
}

static LOCK: AtomicBool = AtomicBool::new(false);
static mut HANDLERS: [HandlerSlot; MAX_VECTORED_HANDLERS] = [HandlerSlot::EMPTY; MAX_VECTORED_HANDLERS];
static mut NEXT_SEQUENCE: u64 = 1;
static mut UNHANDLED_FILTER: PRTL_EXCEPTION_FILTER = None;

struct Guard;
impl Guard {
    fn acquire() -> Self {
        while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) { LOCK.store(false, Ordering::Release); }
}

unsafe fn add_handler(first: ULONG, function: PVECTORED_EXCEPTION_HANDLER, kind: u8) -> PVOID {
    let Some(_) = function else { return core::ptr::null_mut(); };
    let _guard = Guard::acquire();
    let slots = core::ptr::addr_of_mut!(HANDLERS).cast::<HandlerSlot>();
    for index in 0..MAX_VECTORED_HANDLERS {
        let slot = &mut *slots.add(index);
        if slot.kind == 0 {
            let sequence = NEXT_SEQUENCE;
            NEXT_SEQUENCE = NEXT_SEQUENCE.wrapping_add(1).max(1);
            *slot = HandlerSlot { function, first: first != 0, sequence, kind };
            // The opaque handle is the stable slot address, never the callback.
            return slot as *mut HandlerSlot as PVOID;
        }
    }
    core::ptr::null_mut()
}

unsafe fn remove_handler(handle: PVOID, kind: u8) -> ULONG {
    if handle.is_null() { return 0; }
    let _guard = Guard::acquire();
    let slots = core::ptr::addr_of_mut!(HANDLERS).cast::<HandlerSlot>();
    for index in 0..MAX_VECTORED_HANDLERS {
        let slot = &mut *slots.add(index);
        if (slot as *mut HandlerSlot as PVOID) == handle && slot.kind == kind && slot.kind != 0 {
            *slot = HandlerSlot::EMPTY;
            return 1;
        }
    }
    0
}

unsafe fn snapshot_handlers(kind: u8) -> [HandlerSlot; MAX_VECTORED_HANDLERS] {
    let _guard = Guard::acquire();
    let slots = core::ptr::addr_of!(HANDLERS).cast::<HandlerSlot>();
    let mut result = [HandlerSlot::EMPTY; MAX_VECTORED_HANDLERS];
    let mut count = 0usize;
    for index in 0..MAX_VECTORED_HANDLERS {
        let slot = *slots.add(index);
        if slot.kind == kind && slot.function.is_some() {
            result[count] = slot;
            count += 1;
        }
    }
    // Wine ordering: first handlers are newest-first; ordinary handlers are oldest-first.
    for i in 1..count {
        let current = result[i];
        let mut j = i;
        while j > 0 {
            let previous = result[j - 1];
            let current_precedes = if current.first != previous.first {
                current.first
            } else if current.first {
                current.sequence > previous.sequence
            } else {
                current.sequence < previous.sequence
            };
            if !current_precedes { break; }
            result[j] = previous;
            j -= 1;
        }
        result[j] = current;
    }
    result
}

unsafe fn call_handlers(pointers: *mut EXCEPTION_POINTERS, kind: u8) -> LONG {
    let snapshot = snapshot_handlers(kind);
    for slot in snapshot.iter() {
        if let Some(function) = slot.function {
            let result = function(pointers);
            if result == EXCEPTION_CONTINUE_EXECUTION { return result; }
        }
    }
    EXCEPTION_CONTINUE_SEARCH
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddVectoredExceptionHandler(
    first: ULONG, function: PVECTORED_EXCEPTION_HANDLER,
) -> PVOID {
    add_handler(first, function, 1)
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemoveVectoredExceptionHandler(handle: PVOID) -> ULONG {
    remove_handler(handle, 1)
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddVectoredContinueHandler(
    first: ULONG, function: PVECTORED_EXCEPTION_HANDLER,
) -> PVOID {
    add_handler(first, function, 2)
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemoveVectoredContinueHandler(handle: PVOID) -> ULONG {
    remove_handler(handle, 2)
}

/// CognacOS helper for the dispatcher to run registered exception callbacks.
/// It does not itself resume a saved machine context.
#[no_mangle]
pub unsafe extern "system" fn CognacCallVectoredExceptionHandlers(
    record: *mut EXCEPTION_RECORD, context: PVOID,
) -> LONG {
    if record.is_null() { return EXCEPTION_CONTINUE_SEARCH; }
    let mut pointers = EXCEPTION_POINTERS { ExceptionRecord: record, ContextRecord: context };
    call_handlers(&mut pointers, 1)
}

/// CognacOS helper for the dispatcher to run registered continuation callbacks.
#[no_mangle]
pub unsafe extern "system" fn CognacCallVectoredContinueHandlers(
    record: *mut EXCEPTION_RECORD, context: PVOID,
) -> LONG {
    if record.is_null() { return EXCEPTION_CONTINUE_SEARCH; }
    let mut pointers = EXCEPTION_POINTERS { ExceptionRecord: record, ContextRecord: context };
    call_handlers(&mut pointers, 2)
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetUnhandledExceptionFilter(filter: PRTL_EXCEPTION_FILTER) {
    let _guard = Guard::acquire();
    UNHANDLED_FILTER = filter;
}

#[no_mangle]
pub unsafe extern "system" fn CognacCallUnhandledExceptionFilter(
    pointers: *mut EXCEPTION_POINTERS,
) -> LONG {
    if pointers.is_null() { return EXCEPTION_CONTINUE_SEARCH; }
    let filter = {
        let _guard = Guard::acquire();
        UNHANDLED_FILTER
    };
    match filter {
        Some(callback) => callback(pointers),
        None => EXCEPTION_CONTINUE_SEARCH,
    }
}

/// XSTATE configuration copied from the kernel's boot-time CPU policy.
/// NTDLL must receive this from the native process bootstrap/shared-data
/// initializer before exposing CONTEXT_XSTATE to applications.
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicU32;

static NTDLL_XSTATE_ENABLED: AtomicU64 = AtomicU64::new(0);
static NTDLL_XSTATE_SUPPORTED: AtomicU64 = AtomicU64::new(0);
static NTDLL_XSTATE_AREA_SIZE: AtomicU32 = AtomicU32::new(0);
static NTDLL_XSTATE_READY: AtomicBool = AtomicBool::new(false);
static NTDLL_XSTATE_OFFSETS: [AtomicU32; 64] =
    [const { AtomicU32::new(0) }; 64];
static NTDLL_XSTATE_SIZES: [AtomicU32; 64] =
    [const { AtomicU32::new(0) }; 64];

/// Publish the configuration computed by ntoskrnl to NTDLL during native
/// process initialization. Offsets are CPUID.(0xD, feature).EBX values in the
/// standard (non-compacted) XSAVE layout; sizes are CPUID leaf 0xD subleaf EAX.
#[no_mangle]
pub unsafe extern "system" fn CognacNtdllPublishXStateConfiguration(
    supported: ULONG64,
    enabled: ULONG64,
    area_size: ULONG,
    offsets: *const ULONG,
    sizes: *const ULONG,
) -> NTSTATUS {
    if enabled == 0 || enabled & !supported != 0 || area_size < 576
        || offsets.is_null() || sizes.is_null()
    {
        return STATUS_INVALID_PARAMETER;
    }
    // The legacy x87/SSE components are mandatory in the standard format.
    if enabled & 0x3 != 0x3 { return STATUS_INVALID_PARAMETER; }
    NTDLL_XSTATE_READY.store(false, Ordering::Release);
    for i in 0..64 {
        NTDLL_XSTATE_OFFSETS[i].store(*offsets.add(i), Ordering::Relaxed);
        NTDLL_XSTATE_SIZES[i].store(*sizes.add(i), Ordering::Relaxed);
    }
    NTDLL_XSTATE_SUPPORTED.store(supported, Ordering::Relaxed);
    NTDLL_XSTATE_ENABLED.store(enabled, Ordering::Relaxed);
    NTDLL_XSTATE_AREA_SIZE.store(area_size, Ordering::Relaxed);
    NTDLL_XSTATE_READY.store(true, Ordering::Release);
    STATUS_SUCCESS
}

/// Return enabled extended processor feature bits, limited to the caller's mask.
#[no_mangle]
pub extern "system" fn RtlGetEnabledExtendedFeatures(mask: ULONG64) -> ULONG64 {
    if !NTDLL_XSTATE_READY.load(Ordering::Acquire) { return 0; }
    NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & mask
}

/// Return the size of the requested CONTEXT allocation. XSTATE is reported only
/// after the kernel configuration has been published; this prevents NTDLL from
/// advertising AVX/other state that the scheduler cannot preserve.
#[no_mangle]
pub unsafe extern "system" fn RtlGetExtendedContextLength(
    context_flags: ULONG, length: *mut ULONG,
) -> NTSTATUS {
    RtlGetExtendedContextLength2(context_flags, length, ULONG64::MAX)
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetExtendedContextLength2(
    context_flags: ULONG, length: *mut ULONG, compaction_mask: ULONG64,
) -> NTSTATUS {
    if length.is_null() { return STATUS_INVALID_PARAMETER; }
    *length = 0;
    let architecture = context_flags & 0x0011_0000;
    let supported = match architecture {
        0x0010_0000 => (0xd800_0000 | 0x0010_005f, 0x4d0u32),
        0x0001_0000 => (0xd800_0000 | 0x0001_007f, 0x2ccu32),
        _ => return STATUS_INVALID_PARAMETER,
    };
    if context_flags & !supported.0 != 0 { return STATUS_INVALID_PARAMETER; }
    if context_flags & 0x40 == 0 {
        *length = supported.1;
        return STATUS_SUCCESS;
    }
    if !NTDLL_XSTATE_READY.load(Ordering::Acquire) {
        return 0xC000_00BBu32 as i32; // STATUS_NOT_SUPPORTED
    }
    let enabled = NTDLL_XSTATE_ENABLED.load(Ordering::Acquire);
    if enabled == 0 || (compaction_mask != ULONG64::MAX && compaction_mask & !enabled != 0) {
        return STATUS_INVALID_PARAMETER;
    }
    // CONTEXT_EX is three RTL_CONTEXT_CHUNK records (24 bytes). The XSAVE
    // area size already includes the 512-byte legacy region and 64-byte header.
    let extended = (supported.1 as usize)
        .checked_add(24)
        .and_then(|n| n.checked_add(NTDLL_XSTATE_AREA_SIZE.load(Ordering::Acquire) as usize))
        .and_then(|n| n.checked_add(63))
        .map(|n| n & !63);
    match extended {
        Some(n) if n <= ULONG::MAX as usize => { *length = n as ULONG; STATUS_SUCCESS }
        _ => STATUS_INVALID_PARAMETER,
    }
}

/// Query one component's standard XSAVE-area offset and size. This CognacOS
/// helper is used by the CONTEXT_EX implementation as it is completed.
#[no_mangle]
pub unsafe extern "system" fn CognacRtlQueryExtendedFeature(
    feature_id: ULONG, offset: *mut ULONG, size: *mut ULONG,
) -> NTSTATUS {
    if offset.is_null() || size.is_null() || feature_id >= 64 {
        return STATUS_INVALID_PARAMETER;
    }
    if !NTDLL_XSTATE_READY.load(Ordering::Acquire) {
        return 0xC000_00BBu32 as i32;
    }
    let bit = 1u64 << feature_id;
    if NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & bit == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    *offset = NTDLL_XSTATE_OFFSETS[feature_id as usize].load(Ordering::Acquire);
    *size = NTDLL_XSTATE_SIZES[feature_id as usize].load(Ordering::Acquire);
    STATUS_SUCCESS
}

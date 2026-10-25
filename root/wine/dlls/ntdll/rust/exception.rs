//! Source-first Rust port of the portable parts of Wine ntdll/exception.c.
//!
//! This file implements vectored exception/continue handler registration and
//! the unhandled-exception filter plus CONTEXT_EX/XSTATE sizing, initialization,
//! feature lookup, masks, and context copying. Full SEH dispatch and hardware
//! exception/unwind integration remain separate kernel work.

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
static NTDLL_XSTATE_COMPACTION_ENABLED: AtomicBool = AtomicBool::new(false);
static NTDLL_XSTATE_READY: AtomicBool = AtomicBool::new(false);
static NTDLL_XSTATE_OFFSETS: [AtomicU32; 64] =
    [const { AtomicU32::new(0) }; 64];
static NTDLL_XSTATE_SIZES: [AtomicU32; 64] =
    [const { AtomicU32::new(0) }; 64];
const KUSER_SHARED_DATA_X64: usize = 0x0000_0000_7ffe_0000;
const KUSER_XSTATE_OFFSET: usize = 0x3d8;
#[repr(C)]
#[derive(Clone, Copy)]
struct WindowsXStateFeature { offset: u32, size: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct WindowsXStateConfiguration {
    enabled_features: u64,
    enabled_volatile_features: u64,
    size: u32,
    control_flags: u32,
    features: [WindowsXStateFeature; 64],
    enabled_supervisor_features: u64,
    aligned_features: u64,
    all_feature_size: u32,
    all_features: [u32; 64],
    enabled_user_visible_supervisor_features: u64,
    extended_feature_disable_features: u64,
    all_non_large_feature_size: u32,
    spare: u32,
}
/// Lazy NTDLL bootstrap from the read-only page installed by
/// process::initialize. This keeps feature reporting tied to the kernel's
/// actual boot policy instead of trusting local CPUID alone.
fn ensure_ntdll_xstate_config() -> bool {
    if NTDLL_XSTATE_READY.load(Ordering::Acquire) { return true; }
    // KUSER_SHARED_DATA is mapped read-only at 0x7ffe0000. XState uses the
    // Windows 11 24H2 x64 offset 0x3d8 and its exact 0x348-byte layout.
    let shared = (KUSER_SHARED_DATA_X64 + KUSER_XSTATE_OFFSET) as *const WindowsXStateConfiguration;
    let config = unsafe { shared.read_unaligned() };
    let enabled = config.enabled_features;
    if enabled & 3 != 3 || config.size < 576 { return false; }
    let mut offsets = [0u32; 64];
    let mut sizes = [0u32; 64];
    for i in 0..64 {
        offsets[i] = config.features[i].offset;
        sizes[i] = config.features[i].size;
    }
    unsafe {
        CognacNtdllPublishXStateConfiguration(
            enabled,
            enabled,
            config.size,
            offsets.as_ptr(),
            sizes.as_ptr(),
        ) == STATUS_SUCCESS
    }
}


/// Publish the configuration computed by ntoskrnl to NTDLL during native
/// process initialization. Offsets are CPUID.(0xD, feature).EBX values in the
/// standard (non-compacted) XSAVE layout; sizes are CPUID leaf 0xD subleaf EAX.
#[no_mangle]
pub unsafe extern "system" fn CognacNtdllPublishXStateConfiguration(
    supported: ULONG64,
    enabled: ULONG64,
    area_size: ULONG,
    compaction_enabled: ULONG,
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
        NTDLL_XSTATE_OFFSETS[i].store(unsafe { *offsets.add(i) }, Ordering::Relaxed);
        NTDLL_XSTATE_SIZES[i].store(unsafe { *sizes.add(i) }, Ordering::Relaxed);
    }
    NTDLL_XSTATE_SUPPORTED.store(supported, Ordering::Relaxed);
    NTDLL_XSTATE_ENABLED.store(enabled, Ordering::Relaxed);
    NTDLL_XSTATE_AREA_SIZE.store(area_size, Ordering::Relaxed);
    NTDLL_XSTATE_COMPACTION_ENABLED.store(compaction_enabled != 0, Ordering::Relaxed);
    NTDLL_XSTATE_READY.store(true, Ordering::Release);
    STATUS_SUCCESS
}

/// Return enabled extended processor feature bits, limited to the caller's mask.
#[no_mangle]
pub extern "system" fn RtlGetEnabledExtendedFeatures(mask: ULONG64) -> ULONG64 {
    if !ensure_ntdll_xstate_config() { return 0; }
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

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RTL_CONTEXT_CHUNK { pub Offset: i32, pub Length: ULONG }
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CONTEXT_EX { pub All: RTL_CONTEXT_CHUNK, pub Legacy: RTL_CONTEXT_CHUNK, pub XState: RTL_CONTEXT_CHUNK }

const CONTEXT_AMD64: ULONG = 0x0010_0000;
const CONTEXT_I386: ULONG = 0x0001_0000;
const CONTEXT_XSTATE: ULONG = 0x40;
const CONTEXT_EX_SIZE: usize = 24;
const AMD64_CONTEXT_SIZE: usize = 0x4d0;
const I386_CONTEXT_SIZE: usize = 0x2cc;
const STATUS_NOT_SUPPORTED: NTSTATUS = 0xC000_00BBu32 as i32;
const STATUS_BUFFER_OVERFLOW: NTSTATUS = 0x8000_0005u32 as i32;

fn context_parameters(flags: ULONG) -> Option<(usize, usize)> {
    let arch = flags & 0x0011_0000;
    let allowed = match arch {
        CONTEXT_AMD64 => 0xd800_0000 | CONTEXT_AMD64 | 0x5f,
        CONTEXT_I386 => 0xd800_0000 | CONTEXT_I386 | 0x7f,
        _ => return None,
    };
    if flags & !allowed != 0 { return None; }
    Some(if arch == CONTEXT_AMD64 { (AMD64_CONTEXT_SIZE, 0x30) } else { (I386_CONTEXT_SIZE, 0) })
}
fn compacted_feature_offset(mask: ULONG64, feature: ULONG) -> Option<usize> {
    let mut offset = 64usize;
    for id in 2..=feature {
        if mask & (1u64 << id) == 0 { continue; }
        let size = NTDLL_XSTATE_SIZES[id as usize].load(Ordering::Acquire) as usize;
        if size == 0 { return None; }
        let leaf = unsafe { core::arch::x86_64::__cpuid_count(0x0d, id) };
        if leaf.ecx & 2 != 0 { offset = offset.checked_add(63)? & !63; }
        if id == feature { return Some(offset); }
        offset = offset.checked_add(size)?;
    }
    None
}
fn ntdll_xstate_payload_size(mask: ULONG64) -> Option<usize> {
    if !ensure_ntdll_xstate_config() { return None; }
    let selected = mask & NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & !3u64;
    if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
        let mut end = 64usize;
        for feature in 2..64 {
            if selected & (1u64 << feature) == 0 { continue; }
            let size = NTDLL_XSTATE_SIZES[feature].load(Ordering::Acquire) as usize;
            if size == 0 { return None; }
            let leaf = unsafe { core::arch::x86_64::__cpuid_count(0x0d, feature as u32) };
            if leaf.ecx & 2 != 0 { end = end.checked_add(63)? & !63; }
            end = end.checked_add(size)?;
        }
        return Some(end);
    }
    let mut end = 64usize;
    for feature in 2..64 {
        if selected & (1u64 << feature) == 0 { continue; }
        let offset = NTDLL_XSTATE_OFFSETS[feature].load(Ordering::Acquire) as usize;
        let size = NTDLL_XSTATE_SIZES[feature].load(Ordering::Acquire) as usize;
        if size == 0 || offset < 512 { return None; }
        end = end.max(offset.checked_add(size)?.checked_sub(512)?);
    }
    Some(end)
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetExtendedContextLength2(flags: ULONG, length: *mut ULONG, mask: ULONG64) -> NTSTATUS {
    if length.is_null() { return STATUS_INVALID_PARAMETER; }
    *length = 0;
    let Some((context_size, _)) = context_parameters(flags) else { return STATUS_INVALID_PARAMETER; };
    let extra = if flags & CONTEXT_XSTATE == 0 {
        CONTEXT_EX_SIZE + 15
    } else {
        if !ensure_ntdll_xstate_config() { return STATUS_NOT_SUPPORTED; }
        let Some(payload) = ntdll_xstate_payload_size(mask) else { return STATUS_INVALID_PARAMETER; };
        CONTEXT_EX_SIZE + 63 + payload
    };
    match context_size.checked_add(extra) {
        Some(n) if n <= ULONG::MAX as usize => { *length = n as ULONG; STATUS_SUCCESS }
        _ => STATUS_INVALID_PARAMETER,
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeExtendedContext2(context: PVOID, flags: ULONG, out: *mut *mut CONTEXT_EX, mask: ULONG64) -> NTSTATUS {
    if context.is_null() || out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some((size, flags_offset)) = context_parameters(flags) else { return STATUS_INVALID_PARAMETER; };
    if flags & CONTEXT_XSTATE != 0 && !ensure_ntdll_xstate_config() { return STATUS_NOT_SUPPORTED; }
    let aligned = match (context as usize).checked_add(15) { Some(v) => v & !15, None => return STATUS_INVALID_PARAMETER };
    let base = aligned as *mut u8;
    unsafe { base.add(flags_offset).cast::<ULONG>().write_unaligned(flags); }
    let ex_ptr = unsafe { base.add(size).cast::<CONTEXT_EX>() };
    let mut ex = CONTEXT_EX::default();
    ex.All.Offset = -(size as i32);
    ex.Legacy.Offset = -(size as i32);
    ex.Legacy.Length = size as ULONG;
    if flags & CONTEXT_XSTATE == 0 {
        ex.XState.Offset = 25;
        ex.All.Length = (size + CONTEXT_EX_SIZE) as ULONG;
        unsafe { ex_ptr.write_unaligned(ex); *out = ex_ptr; }
        return STATUS_SUCCESS;
    }
    let Some(payload) = ntdll_xstate_payload_size(mask) else { return STATUS_INVALID_PARAMETER; };
    let ex_addr = ex_ptr as usize;
    let xstate_addr = match ex_addr.checked_add(CONTEXT_EX_SIZE + 63) { Some(v) => v & !63, None => return STATUS_INVALID_PARAMETER };
    let offset = xstate_addr - ex_addr;
    let all_len = match size.checked_add(offset).and_then(|v| v.checked_add(payload)) {
        Some(v) if v <= ULONG::MAX as usize => v, _ => return STATUS_INVALID_PARAMETER,
    };
    ex.XState.Offset = offset as i32;
    ex.XState.Length = payload as ULONG;
    ex.All.Length = all_len as ULONG;
    unsafe {
        core::ptr::write_bytes(xstate_addr as *mut u8, 0, payload);
        if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
            let selected = mask & NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & !3u64;
            let compaction_mask = (1u64 << 63) | selected;
            (xstate_addr as *mut u8).add(8).cast::<ULONG64>().write_unaligned(compaction_mask);
        }
        ex_ptr.write_unaligned(ex);
        *out = ex_ptr;
    }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlInitializeExtendedContext(context: PVOID, flags: ULONG, out: *mut *mut CONTEXT_EX) -> NTSTATUS {
    RtlInitializeExtendedContext2(context, flags, out, ULONG64::MAX)
}
#[no_mangle]
pub unsafe extern "system" fn RtlLocateLegacyContext(ex: *mut CONTEXT_EX, length: *mut ULONG) -> PVOID {
    if ex.is_null() { return core::ptr::null_mut(); }
    let chunk = unsafe { core::ptr::addr_of!((*ex).Legacy).read_unaligned() };
    if !length.is_null() { unsafe { length.write_unaligned(chunk.Length); } }
    unsafe { ex.cast::<u8>().offset(chunk.Offset as isize).cast() }
}
#[no_mangle]
pub unsafe extern "system" fn RtlLocateExtendedFeature(ex: *mut CONTEXT_EX, feature: ULONG, length: *mut ULONG) -> PVOID {
    if ex.is_null() || !(2..64).contains(&feature) || !ensure_ntdll_xstate_config() { return core::ptr::null_mut(); }
    let chunk = unsafe { core::ptr::addr_of!((*ex).XState).read_unaligned() };
    if NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & (1u64 << feature) == 0 { return core::ptr::null_mut(); }
    let size = NTDLL_XSTATE_SIZES[feature as usize].load(Ordering::Acquire) as usize;
    if size == 0 { return core::ptr::null_mut(); }
    let xstate = unsafe { ex.cast::<u8>().add(chunk.Offset as usize) };
    let offset = if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
        let compaction_mask = unsafe { xstate.add(8).cast::<ULONG64>().read_unaligned() };
        if compaction_mask & (1u64 << 63) == 0 || compaction_mask & (1u64 << feature) == 0 {
            return core::ptr::null_mut();
        }
        let Some(offset) = compacted_feature_offset(compaction_mask & !(1u64 << 63), feature) else {
            return core::ptr::null_mut();
        };
        offset
    } else {
        let standard_offset = NTDLL_XSTATE_OFFSETS[feature as usize].load(Ordering::Acquire) as usize;
        if standard_offset < 512 { return core::ptr::null_mut(); }
        standard_offset - 512
    };
    if offset.checked_add(size).map_or(true, |n| n > chunk.Length as usize) { return core::ptr::null_mut(); }
    if !length.is_null() { unsafe { length.write_unaligned(size as ULONG); } }
    unsafe { xstate.add(offset).cast() }
}
#[no_mangle]
pub unsafe extern "system" fn RtlLocateExtendedFeature2(ex: *mut CONTEXT_EX, feature: ULONG, config: PVOID, length: *mut ULONG) -> PVOID {
    if config.is_null() { return core::ptr::null_mut(); }
    RtlLocateExtendedFeature(ex, feature, length)
}
#[no_mangle]
pub unsafe extern "system" fn RtlSetExtendedFeaturesMask(ex: *mut CONTEXT_EX, mask: ULONG64) {
    if ex.is_null() { return; }
    let chunk = unsafe { core::ptr::addr_of!((*ex).XState).read_unaligned() };
    if chunk.Length < 8 { return; }
    let mut enabled = RtlGetEnabledExtendedFeatures(mask) & !3u64;
    let xstate = unsafe { ex.cast::<u8>().add(chunk.Offset as usize) };
    if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
        let compaction_mask = unsafe { xstate.add(8).cast::<ULONG64>().read_unaligned() };
        enabled &= compaction_mask & !(1u64 << 63);
    }
    unsafe { xstate.cast::<ULONG64>().write_unaligned(enabled); }
}
#[no_mangle]
pub unsafe extern "system" fn RtlGetExtendedFeaturesMask(ex: *mut CONTEXT_EX) -> ULONG64 {
    if ex.is_null() { return 0; }
    let chunk = unsafe { core::ptr::addr_of!((*ex).XState).read_unaligned() };
    if chunk.Length < 8 { return 0; }
    unsafe { ex.cast::<u8>().add(chunk.Offset as usize).cast::<ULONG64>().read_unaligned() & !3u64 }
}
fn copy_amd64_context_ranges(dst: *mut u8, src: *const u8, flags: ULONG) {
    // Windows x64 CONTEXT field boundaries from the native context copy map.
    const RANGES: [(usize, ULONG); 12] = [
        (0x38, 0x1), (0x3a, 0x4), (0x42, 0x1), (0x48, 0x10),
        (0x78, 0x2), (0x98, 0x1), (0xa0, 0x2), (0xf8, 0x1),
        (0x100, 0x8), (0x2a0, 0), (0x4b0, 0x10), (0x4d0, 0),
    ];
    let mut start = 0usize;
    for (boundary, group) in RANGES {
        if group & flags != 0 {
            if start == 0 { start = boundary; }
        } else if start != 0 {
            unsafe { core::ptr::copy(src.add(start), dst.add(start), boundary - start); }
            start = 0;
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyExtendedContext(dst: *mut CONTEXT_EX, flags: ULONG, src: *const CONTEXT_EX) -> NTSTATUS {
    if dst.is_null() || src.is_null() { return STATUS_INVALID_PARAMETER; }
    if flags & CONTEXT_XSTATE != 0 && !ensure_ntdll_xstate_config() { return STATUS_NOT_SUPPORTED; }
    let s = unsafe { src.read_unaligned() };
    let d = unsafe { dst.read_unaligned() };
    if s.Legacy.Length > d.Legacy.Length { return STATUS_BUFFER_OVERFLOW; }
    if flags & CONTEXT_XSTATE != 0 && d.XState.Length < s.XState.Length { return STATUS_BUFFER_OVERFLOW; }
    // CONTEXT_EX objects can have different alignment padding, so copy the
    // legacy context and XSTATE payload separately instead of copying All as
    // one block and accidentally shifting the destination's aligned area.
    let source_legacy = unsafe { src.cast::<u8>().offset(s.Legacy.Offset as isize) };
    let dest_legacy = unsafe { dst.cast::<u8>().offset(d.Legacy.Offset as isize) };
    if flags & 0x0011_0000 == CONTEXT_AMD64 {
        copy_amd64_context_ranges(dest_legacy, source_legacy, flags);
    } else {
        // The x86 compatibility layout is retained as a conservative full
        // legacy-block copy until its per-field range map is ported.
        unsafe { core::ptr::copy(source_legacy, dest_legacy, s.Legacy.Length as usize); }
    }
    if flags & CONTEXT_XSTATE != 0 && s.XState.Length != 0 {
        let source_xstate = unsafe { src.cast::<u8>().add(s.XState.Offset as usize) };
        let dest_xstate = unsafe { dst.cast::<u8>().add(d.XState.Offset as usize) };
        unsafe { core::ptr::copy(source_xstate, dest_xstate, s.XState.Length as usize); }
    }
    STATUS_SUCCESS
}

/// Query one enabled component's standard XSAVE-area offset and size.
#[no_mangle]
pub unsafe extern "system" fn CognacRtlQueryExtendedFeature(
    feature_id: ULONG, offset: *mut ULONG, size: *mut ULONG,
) -> NTSTATUS {
    if offset.is_null() || size.is_null() || feature_id >= 64 {
        return STATUS_INVALID_PARAMETER;
    }
    if !ensure_ntdll_xstate_config() {
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


#[cfg(test)]
mod xstate_context_layout_tests {
    use super::*;
    #[test]
    fn context_extension_layout_matches_x64_chunk_contract() {
        assert_eq!(core::mem::size_of::<RTL_CONTEXT_CHUNK>(), 8);
        assert_eq!(core::mem::size_of::<CONTEXT_EX>(), 24);
    }
    #[test]
    fn native_context_sizes_are_x64_and_x86_sized() {
        assert_eq!(context_parameters(CONTEXT_AMD64 | 0x1f).unwrap().0, AMD64_CONTEXT_SIZE);
        assert_eq!(context_parameters(CONTEXT_I386 | 0x7f).unwrap().0, I386_CONTEXT_SIZE);
    }
    #[test]
    fn published_configuration_fits_one_user_page() {
        assert!(core::mem::size_of::<CognacXStateSharedConfiguration>() <= 4096);
    }
}

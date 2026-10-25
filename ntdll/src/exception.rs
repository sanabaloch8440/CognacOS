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
static NTDLL_XSTATE_ALIGNED_MASK: AtomicU64 = AtomicU64::new(0);
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
    max_sve_vector_length: u16,
    spare1: u16,
}
const _: [(); 0x348] = [(); core::mem::size_of::<WindowsXStateConfiguration>()];
const _: [(); 0x18] = [(); core::mem::offset_of!(WindowsXStateConfiguration, features)];
const _: [(); 0x330] = [(); core::mem::offset_of!(WindowsXStateConfiguration, enabled_user_visible_supervisor_features)];
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
    NTDLL_XSTATE_ALIGNED_MASK.store(config.aligned_features, Ordering::Release);
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
            (config.control_flags >> 1) & 1,
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
const CONTEXT_EX_ALIGNED_SIZE: usize = 32;
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
        if NTDLL_XSTATE_ALIGNED_MASK.load(Ordering::Acquire) & (1u64 << id) != 0 {
            offset = offset.checked_add(63)? & !63;
        }
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
            if NTDLL_XSTATE_ALIGNED_MASK.load(Ordering::Acquire) & (1u64 << feature) != 0 {
                end = end.checked_add(63)? & !63;
            }
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
        CONTEXT_EX_ALIGNED_SIZE + 63 + payload
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
        ex.XState.Offset = CONTEXT_EX_ALIGNED_SIZE as i32;
        ex.All.Length = (size + CONTEXT_EX_SIZE) as ULONG;
        unsafe { ex_ptr.write_unaligned(ex); *out = ex_ptr; }
        return STATUS_SUCCESS;
    }
    let Some(payload) = ntdll_xstate_payload_size(mask) else { return STATUS_INVALID_PARAMETER; };
    let ex_addr = ex_ptr as usize;
    let xstate_addr = match ex_addr.checked_add(CONTEXT_EX_ALIGNED_SIZE + 63) { Some(v) => v & !63, None => return STATUS_INVALID_PARAMETER };
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
    if !ensure_ntdll_xstate_config() { return core::ptr::null_mut(); }
    let config = (KUSER_SHARED_DATA_X64 + KUSER_XSTATE_OFFSET) as PVOID;
    RtlLocateExtendedFeature2(ex, feature, config, length)
}

#[no_mangle]
pub unsafe extern "system" fn RtlLocateExtendedFeature2(ex: *mut CONTEXT_EX, feature: ULONG, config: PVOID, length: *mut ULONG) -> PVOID {
    if !length.is_null() { unsafe { length.write_unaligned(0); } }
    if ex.is_null() || config.is_null() || !(2..64).contains(&feature) { return core::ptr::null_mut(); }
    // The WDK API accepts a caller-supplied XSTATE_CONFIGURATION. Do not
    // require pointer identity with KUSER_SHARED_DATA; use the provided layout.
    let cfg = unsafe { (config as *const WindowsXStateConfiguration).read_unaligned() };
    let feature_bit = 1u64 << feature;
    let component = cfg.features[feature as usize];
    if cfg.enabled_features & feature_bit == 0 || component.size == 0 {
        return core::ptr::null_mut();
    }
    if !length.is_null() { unsafe { length.write_unaligned(component.size); } }
    let chunk = unsafe { core::ptr::addr_of!((*ex).XState).read_unaligned() };
    if chunk.Offset < 0 || chunk.Length < 64 { return core::ptr::null_mut(); }
    let xstate = unsafe { ex.cast::<u8>().add(chunk.Offset as usize) };
    let offset = if cfg.control_flags & 2 != 0 {
        let compacted = unsafe { xstate.add(8).cast::<ULONG64>().read_unaligned() };
        if compacted & (1u64 << 63) == 0 || compacted & feature_bit == 0 {
            return core::ptr::null_mut();
        }
        let compacted_features = compacted & !(1u64 << 63);
        let mut off = 64usize;
        for id in 2..feature {
            let bit = 1u64 << id;
            if compacted_features & bit == 0 { continue; }
            let size = cfg.features[id as usize].size as usize;
            if size == 0 { return core::ptr::null_mut(); }
            off = match off.checked_add(size) { Some(v) => v, None => return core::ptr::null_mut() };
            if cfg.aligned_features & (1u64 << (id + 1)) != 0 {
                off = match off.checked_add(63) { Some(v) => v & !63, None => return core::ptr::null_mut() };
            }
        }
        off
    } else {
        if cfg.enabled_features & feature_bit == 0 || component.offset < 512 {
            return core::ptr::null_mut();
        }
        (component.offset - 512) as usize
    };
    let size = component.size as usize;
    if offset.checked_add(size).map_or(true, |end| end > chunk.Length as usize) {
        return core::ptr::null_mut();
    }
    unsafe { xstate.add(offset).cast() }
}
#[no_mangle]
pub unsafe extern "system" fn RtlSetExtendedFeaturesMask(ex: *mut CONTEXT_EX, mask: ULONG64) {
    if ex.is_null() { return; }
    let chunk = unsafe { core::ptr::addr_of!((*ex).XState).read_unaligned() };
    if chunk.Offset < 0 || chunk.Length < 8 { return; }
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
    if chunk.Offset < 0 || chunk.Length < 8 { return 0; }
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

fn copy_context_ranges(dst: *mut u8, src: *const u8, flags: ULONG, arch: ULONG) {
    const AMD64: [(usize, ULONG); 12] = [
        (0x38, 0x1), (0x3a, 0x4), (0x42, 0x1), (0x48, 0x10),
        (0x78, 0x2), (0x98, 0x1), (0xa0, 0x2), (0xf8, 0x1),
        (0x100, 0x8), (0x2a0, 0), (0x4b0, 0x10), (0x4d0, 0),
    ];
    const I386: [(usize, ULONG); 8] = [
        (0x04, 0x10), (0x1c, 0x08), (0x8c, 0x04), (0x9c, 0x02),
        (0xb4, 0x01), (0xcc, 0x20), (0x1ec, 0), (0x2cc, 0),
    ];
    let flags_offset = if arch == CONTEXT_AMD64 { 0x30 } else { 0 };
    unsafe {
        let field = dst.add(flags_offset).cast::<ULONG>();
        field.write_unaligned(field.read_unaligned() | flags);
    }
    let mut start = 0usize;
    if arch == CONTEXT_AMD64 {
        for (boundary, group) in AMD64 {
            if group & flags != 0 {
                if start == 0 { start = boundary; }
            } else if start != 0 {
                unsafe { core::ptr::copy(src.add(start), dst.add(start), boundary - start); }
                start = 0;
            }
        }
    } else {
        for (boundary, group) in I386 {
            if group & flags != 0 {
                if start == 0 { start = boundary; }
            } else if start != 0 {
                unsafe { core::ptr::copy(src.add(start), dst.add(start), boundary - start); }
                start = 0;
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyExtendedContext(dst: *mut CONTEXT_EX, flags: ULONG, src: *const CONTEXT_EX) -> NTSTATUS {
    let Some((_, _)) = context_parameters(flags) else { return STATUS_INVALID_PARAMETER; };
    if dst.is_null() || src.is_null() { return STATUS_INVALID_PARAMETER; }
    if flags & CONTEXT_XSTATE != 0 && !ensure_ntdll_xstate_config() { return STATUS_NOT_SUPPORTED; }
    let s = unsafe { src.read_unaligned() };
    let d = unsafe { dst.read_unaligned() };
    if s.Legacy.Length > d.Legacy.Length { return STATUS_BUFFER_OVERFLOW; }
    let source_legacy = unsafe { src.cast::<u8>().offset(s.Legacy.Offset as isize) };
    let dest_legacy = unsafe { dst.cast::<u8>().offset(d.Legacy.Offset as isize) };
    copy_context_ranges(dest_legacy, source_legacy, flags, flags & 0x0011_0000);
    if flags & CONTEXT_XSTATE == 0 { return STATUS_SUCCESS; }
    if s.XState.Offset < 0 || d.XState.Offset < 0 || s.XState.Length < 64 || d.XState.Length < 64 {
        return STATUS_BUFFER_OVERFLOW;
    }
    let source_xstate = unsafe { src.cast::<u8>().add(s.XState.Offset as usize) };
    let dest_xstate = unsafe { dst.cast::<u8>().add(d.XState.Offset as usize) };
    let source_mask = unsafe { source_xstate.cast::<ULONG64>().read_unaligned() };
    let source_compaction = unsafe { source_xstate.add(8).cast::<ULONG64>().read_unaligned() };
    let enabled = NTDLL_XSTATE_ENABLED.load(Ordering::Acquire);
    let selected = source_mask & enabled & !3u64;
    unsafe {
        core::ptr::write_bytes(dest_xstate, 0, 64);
        dest_xstate.cast::<ULONG64>().write_unaligned(selected);
        let compacted = NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire);
        dest_xstate.add(8).cast::<ULONG64>().write_unaligned(
            if compacted { (1u64 << 63) | (source_compaction & enabled & !3u64) } else { 0 }
        );
    }
    for feature in 2..64 {
        let bit = 1u64 << feature;
        if selected & bit == 0 { continue; }
        let size = NTDLL_XSTATE_SIZES[feature].load(Ordering::Acquire) as usize;
        if size == 0 { continue; }
        let offset = if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
            if source_compaction & (1u64 << 63) == 0 || source_compaction & bit == 0 { continue; }
            let mut off = 64usize;
            for previous in 2..feature {
                if source_compaction & (1u64 << previous) == 0 { continue; }
                let previous_size = NTDLL_XSTATE_SIZES[previous].load(Ordering::Acquire) as usize;
                if previous_size == 0 { return STATUS_INVALID_PARAMETER; }
                off = match off.checked_add(previous_size) { Some(v) => v, None => return STATUS_BUFFER_OVERFLOW };
                if NTDLL_XSTATE_ALIGNED_MASK.load(Ordering::Acquire) & (1u64 << (previous + 1)) != 0 {
                    off = match off.checked_add(63) { Some(v) => v & !63, None => return STATUS_BUFFER_OVERFLOW };
                }
            }
            off
        } else {
            let standard = NTDLL_XSTATE_OFFSETS[feature].load(Ordering::Acquire) as usize;
            if standard < 512 { continue; }
            standard - 512
        };
        if offset.checked_add(size).map_or(true, |end| end > s.XState.Length as usize || end > d.XState.Length as usize) {
            return STATUS_BUFFER_OVERFLOW;
        }
        unsafe { core::ptr::copy(source_xstate.add(offset), dest_xstate.add(offset), size); }
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyContext(dst: PVOID, flags: ULONG, src: PVOID) -> NTSTATUS {
    if dst.is_null() || src.is_null() { return STATUS_INVALID_PARAMETER; }
    let arch = flags & (CONTEXT_AMD64 | CONTEXT_I386);
    let (size, flags_offset) = match context_parameters(flags) {
        Some(v) => v,
        None => return STATUS_INVALID_PARAMETER,
    };
    let dflags = unsafe { dst.cast::<u8>().add(flags_offset).cast::<ULONG>().read_unaligned() };
    let sflags = unsafe { src.cast::<u8>().add(flags_offset).cast::<ULONG>().read_unaligned() };
    if dflags & (CONTEXT_AMD64 | CONTEXT_I386) != arch || sflags & (CONTEXT_AMD64 | CONTEXT_I386) != arch {
        return STATUS_INVALID_PARAMETER;
    }
    let copy_flags = flags & sflags;
    if copy_flags & !dflags & CONTEXT_XSTATE != 0 { return STATUS_BUFFER_OVERFLOW; }
    if copy_flags & CONTEXT_XSTATE != 0 {
        return RtlCopyExtendedContext(
            unsafe { dst.cast::<u8>().add(size).cast::<CONTEXT_EX>() },
            copy_flags,
            unsafe { src.cast::<u8>().add(size).cast::<CONTEXT_EX>() },
        );
    }
    copy_context_ranges(dst.cast(), src.cast(), copy_flags, arch);
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
    fn extended_context_length_matches_amd64_non_xstate_layout() {
        let mut length = 0;
        let status = unsafe { RtlGetExtendedContextLength2(CONTEXT_AMD64 | 0x1f, &mut length, 0) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(length as usize, AMD64_CONTEXT_SIZE + CONTEXT_EX_SIZE + 15);
    }
    #[test]
    fn extended_context_length_rejects_invalid_architecture_and_null_output() {
        let mut length = 99;
        assert_eq!(unsafe { RtlGetExtendedContextLength2(0, &mut length, 0) }, STATUS_INVALID_PARAMETER);
        assert_eq!(length, 0);
        assert_eq!(unsafe { RtlGetExtendedContextLength2(CONTEXT_AMD64, core::ptr::null_mut(), 0) }, STATUS_INVALID_PARAMETER);
    }
    #[test]
    fn locate_feature2_honors_custom_standard_configuration() {
        let mut raw = [0u64; 96];
        let ex = raw.as_mut_ptr().cast::<CONTEXT_EX>();
        unsafe {
            core::ptr::addr_of_mut!((*ex).XState).write_unaligned(RTL_CONTEXT_CHUNK { Offset: 24, Length: 320 });
        }
        let mut cfg = WindowsXStateConfiguration {
            enabled_features: 1 << 2, enabled_volatile_features: 0, size: 832, control_flags: 0,
            features: [WindowsXStateFeature { offset: 0, size: 0 }; 64],
            enabled_supervisor_features: 0, aligned_features: 0, all_feature_size: 832,
            all_features: [0; 64], enabled_user_visible_supervisor_features: 0,
            extended_feature_disable_features: 0, all_non_large_feature_size: 832, max_sve_vector_length: 0, spare1: 0,
        };
        cfg.features[2] = WindowsXStateFeature { offset: 576, size: 256 };
        let mut length = 0;
        let found = unsafe { RtlLocateExtendedFeature2(ex, 2, (&mut cfg as *mut WindowsXStateConfiguration).cast(), &mut length) };
        assert_eq!(length, 256);
        assert_eq!(found as usize, ex as usize + 24 + 64);
    }
    #[test]
    fn locate_feature2_honors_custom_compacted_configuration() {
        let mut raw = [0u64; 96];
        let ex = raw.as_mut_ptr().cast::<CONTEXT_EX>();
        unsafe {
            core::ptr::addr_of_mut!((*ex).XState).write_unaligned(RTL_CONTEXT_CHUNK { Offset: 24, Length: 320 });
            let xstate = ex.cast::<u8>().add(24);
            xstate.add(8).cast::<ULONG64>().write_unaligned((1u64 << 63) | (1 << 2));
        }
        let mut cfg = WindowsXStateConfiguration {
            enabled_features: 1 << 2, enabled_volatile_features: 0, size: 832, control_flags: 2,
            features: [WindowsXStateFeature { offset: 0, size: 0 }; 64],
            enabled_supervisor_features: 0, aligned_features: 0, all_feature_size: 832,
            all_features: [0; 64], enabled_user_visible_supervisor_features: 0,
            extended_feature_disable_features: 0, all_non_large_feature_size: 832, max_sve_vector_length: 0, spare1: 0,
        };
        cfg.features[2] = WindowsXStateFeature { offset: 576, size: 256 };
        let mut length = 0;
        let found = unsafe { RtlLocateExtendedFeature2(ex, 2, (&mut cfg as *mut WindowsXStateConfiguration).cast(), &mut length) };
        assert_eq!(length, 256);
        assert_eq!(found as usize, ex as usize + 24 + 64);
    }
    #[test]
    fn locate_feature2_rejects_a_truncated_xstate_chunk() {
        let mut raw = [0u64; 16];
        let ex = raw.as_mut_ptr().cast::<CONTEXT_EX>();
        unsafe {
            core::ptr::addr_of_mut!((*ex).XState).write_unaligned(RTL_CONTEXT_CHUNK { Offset: 24, Length: 64 });
        }
        let mut cfg = WindowsXStateConfiguration {
            enabled_features: 1 << 2, enabled_volatile_features: 0, size: 832, control_flags: 0,
            features: [WindowsXStateFeature { offset: 0, size: 0 }; 64],
            enabled_supervisor_features: 0, aligned_features: 0, all_feature_size: 832,
            all_features: [0; 64], enabled_user_visible_supervisor_features: 0,
            extended_feature_disable_features: 0, all_non_large_feature_size: 832,
            max_sve_vector_length: 0, spare1: 0,
        };
        cfg.features[2] = WindowsXStateFeature { offset: 576, size: 256 };
        let mut length = 0;
        let found = unsafe { RtlLocateExtendedFeature2(ex, 2, (&mut cfg as *mut WindowsXStateConfiguration).cast(), &mut length) };
        assert!(found.is_null());
        assert_eq!(length, 256);
    }
    #[test]
    fn locate_feature2_null_inputs_clear_output_length() {
        let mut length = 99;
        let found = unsafe { RtlLocateExtendedFeature2(core::ptr::null_mut(), 2, core::ptr::null_mut(), &mut length) };
        assert!(found.is_null());
        assert_eq!(length, 0);
    }
    #[test]
    fn partial_context_copy_updates_only_requested_control_group() {
        let mut source = [0u8; AMD64_CONTEXT_SIZE];
        let mut destination = [0xA5u8; AMD64_CONTEXT_SIZE];
        unsafe {
            source.as_mut_ptr().add(0x30).cast::<ULONG>().write_unaligned(CONTEXT_AMD64 | 1);
            destination.as_mut_ptr().add(0x30).cast::<ULONG>().write_unaligned(CONTEXT_AMD64);
            source.as_mut_ptr().add(0x38).write(0x11);
            source.as_mut_ptr().add(0x42).write(0x22);
            destination.as_mut_ptr().add(0x38).write(0x33);
            destination.as_mut_ptr().add(0x42).write(0x44);
        }
        let status = unsafe { RtlCopyContext(destination.as_mut_ptr().cast(), CONTEXT_AMD64 | 1, source.as_mut_ptr().cast()) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(destination[0x38], 0x11);
        assert_eq!(destination[0x42], 0x22);
        assert_eq!(destination[0x100], 0xA5);
    }
    #[test]
    fn published_configuration_fits_one_user_page() {
        assert_eq!(core::mem::size_of::<WindowsXStateConfiguration>(), 0x348);
        assert!(KUSER_XSTATE_OFFSET + core::mem::size_of::<WindowsXStateConfiguration>() <= 4096);
    }
}

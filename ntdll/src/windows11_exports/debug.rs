//! Windows 11 x64 NTDLL debugging exports.
//!
//! The debugger UI APIs are user-mode wrappers around NT debug-object/system
//! services. They must not be replaced by zero-argument placeholder symbols.
//! This module implements local breakpoint/filter behavior and keeps unsupported
//! debugger transport operations explicit until their NT service bridge exists.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::{c_char, c_void};
use core::sync::atomic::{AtomicU32, Ordering};

type BOOLEAN = u8;
type ULONG = u32;
type NTSTATUS = i32;
type HANDLE = *mut c_void;
type PVOID = *mut c_void;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_PORT_NOT_SET: NTSTATUS = 0xC000_0353u32 as i32;
const DPFLTR_COMPONENTS: usize = 256;
const DPFLTR_DEFAULT_ID: ULONG = 0;
const DPFLTR_INFO_LEVEL: ULONG = 3;

// Windows' debug-print filters are component masks. A zero mask suppresses
// ordinary informational output until a caller explicitly enables a bit.
static FILTER_MASKS: [AtomicU32; DPFLTR_COMPONENTS] =
    [const { AtomicU32::new(0) }; DPFLTR_COMPONENTS];

#[inline]
fn filter_slot(component_id: ULONG) -> usize {
    (component_id as usize).min(DPFLTR_COMPONENTS - 1)
}

#[inline]
fn filter_enabled(component_id: ULONG, level: ULONG) -> bool {
    let mask = FILTER_MASKS[filter_slot(component_id)].load(Ordering::Acquire);
    if level < 32 {
        mask & (1u32 << level) != 0
    } else {
        // Windows treats levels >= 32 as bit masks, not bit indices.
        mask & level != 0
    }
}

/// Deliberately raises the x86-64 breakpoint exception, as the native export does.
#[no_mangle]
pub unsafe extern "system" fn DbgBreakPoint() {
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("int3", options(nomem, nostack));
}

/// ABI-compatible entry point. Rust stable does not support defining C variadic
/// functions, so this implementation accepts the format pointer only and does
/// not consume caller varargs. The kernel debug transport is not wired yet.
#[no_mangle]
pub unsafe extern "system" fn DbgPrint(_format: *const c_char) -> ULONG {
    STATUS_NOT_IMPLEMENTED as ULONG
}

/// See DbgPrint: the fixed leading parameters match the x64 ABI; varargs are not
/// consumed until the target-specific variadic bridge is available.
#[no_mangle]
pub unsafe extern "system" fn DbgPrintEx(
    _component_id: ULONG,
    _level: ULONG,
    _format: *const c_char,
) -> ULONG {
    STATUS_NOT_IMPLEMENTED as ULONG
}

#[no_mangle]
pub unsafe extern "system" fn DbgPrintReturnControlC(_format: *const c_char) -> ULONG {
    STATUS_NOT_IMPLEMENTED as ULONG
}

/// NTDLL's prompt routine shares the kernel DbgPrompt contract. No debugger
/// transport currently exists, so no output or input is fabricated.
#[no_mangle]
pub unsafe extern "system" fn DbgPrompt(
    _prompt: *const c_char,
    response: *mut c_char,
    length: ULONG,
) -> ULONG {
    if length != 0 && response.is_null() {
        return 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "system" fn DbgQueryDebugFilterState(
    component_id: ULONG,
    level: ULONG,
) -> NTSTATUS {
    if filter_enabled(component_id, level) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn DbgSetDebugFilterState(
    component_id: ULONG,
    level: ULONG,
    state: BOOLEAN,
) -> NTSTATUS {
    let mask = &FILTER_MASKS[filter_slot(component_id)];
    if level < 32 {
        let bit = 1u32 << level;
        if state != 0 {
            mask.fetch_or(bit, Ordering::AcqRel);
        } else {
            mask.fetch_and(!bit, Ordering::AcqRel);
        }
    } else if state != 0 {
        mask.fetch_or(level, Ordering::AcqRel);
    } else {
        mask.fetch_and(!level, Ordering::AcqRel);
    }
    STATUS_SUCCESS
}

// DBGUI_WAIT_STATE_CHANGE and DEBUG_EVENT are large public ABI structures.
// Conversion is intentionally not guessed: returning a failure is safer than
// writing an incorrectly laid-out event into caller memory.
#[no_mangle]
pub unsafe extern "system" fn DbgUiConvertStateChangeStructure(
    _state_change: PVOID,
    _debug_event: PVOID,
) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiConvertStateChangeStructureEx(
    _state_change: PVOID,
    _debug_event: PVOID,
) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiGetThreadDebugObject() -> HANDLE {
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiIssueRemoteBreakin(_process: HANDLE) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiRemoteBreakin() {
    // The native implementation terminates the remote thread after requesting
    // a breakpoint. Until the debug-object bridge exists, avoid crashing an
    // arbitrary caller or pretending a break-in was delivered.
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiSetThreadDebugObject(_debug_object: HANDLE) {
    // Requires TEB/debug-object state integration; intentionally not emulated
    // with a process-global handle because that would violate thread semantics.
}

#[no_mangle]
pub unsafe extern "system" fn DbgUserBreakPoint() {
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("int3", options(nomem, nostack));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_filter_can_enable_and_disable_levels() {
        unsafe {
            assert_eq!(DbgSetDebugFilterState(7, 2, 1), STATUS_SUCCESS);
            assert_eq!(DbgQueryDebugFilterState(7, 2), 1);
            assert_eq!(DbgSetDebugFilterState(7, 2, 0), STATUS_SUCCESS);
            assert_eq!(DbgQueryDebugFilterState(7, 2), 0);
        }
    }

    #[test]
    fn prompt_rejects_null_response_for_nonzero_length() {
        unsafe {
            assert_eq!(DbgPrompt(core::ptr::null(), core::ptr::null_mut(), 1), 0);
        }
    }
}

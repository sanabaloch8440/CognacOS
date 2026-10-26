//! Application-compatibility switch-branch exports.
//!
//! Signatures follow the publicly documented reverse-engineered prototypes.
//! The scenario table layout/selection semantics are not yet implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;

type ULONG = u32;
type PVOID = *mut c_void;
type FARPROC = Option<unsafe extern "system" fn() -> isize>;

#[no_mangle]
pub unsafe extern "system" fn SbSelectProcedure(
    signature: ULONG,
    version: ULONG,
    scenario_info: *const c_void,
    scenario_entry: ULONG,
) -> FARPROC {
    let _ = (signature, version, scenario_info, scenario_entry);
    None
}

#[no_mangle]
pub unsafe extern "system" fn SbExecuteProcedure(
    signature: ULONG,
    version: ULONG,
    scenario_info: *const c_void,
    scenario_entry: ULONG,
    context: PVOID,
) -> PVOID {
    let _ = (signature, version, scenario_info, scenario_entry, context);
    core::ptr::null_mut()
}

//! First set of NT kernel ABI services backed by real RdotOS HAL state.
//! Only routines implemented here are eligible for callable import resolution.
use core::sync::atomic::Ordering;

const ALL_PROCESSOR_GROUPS: u16 = 0xffff;

/// RdotOS has not yet implemented interrupt-priority IRQLs; external interrupts
/// remain disabled during this stage, so execution is at PASSIVE_LEVEL.
pub unsafe extern "win64" fn ke_get_current_irql() -> u8 { 0 }

/// Return the active logical processor count for group 0 or all groups.
/// RdotOS currently models one processor group, with at most 64 logical CPUs.
pub unsafe extern "win64" fn ke_query_active_processor_count_ex(group: u16) -> u32 {
    let count = crate::hal::online_cpu_count() as u32;
    if group == 0 || group == ALL_PROCESSOR_GROUPS { count } else { 0 }
}

pub unsafe extern "win64" fn ke_query_active_processor_count() -> u32 {
    unsafe { ke_query_active_processor_count_ex(0) }
}

pub unsafe extern "win64" fn ke_query_active_group_count() -> u16 {
    if crate::hal::online_cpu_count() == 0 { 0 } else { 1 }
}

/// Windows PROCESSOR_NUMBER: group, group-relative number, reserved byte.
#[repr(C)]
pub struct ProcessorNumber { pub group: u16, pub number: u8, pub reserved: u8 }

pub unsafe extern "win64" fn ke_get_current_processor_number_ex(out: *mut ProcessorNumber) {
    if out.is_null() { return; }
    let Some(cpu) = crate::hal::current_cpu_local() else { return; };
    let logical = cpu.logical_id.load(Ordering::Acquire);
    // Only group zero exists at present. Do not fabricate a number if the HAL
    // reports a slot outside the supported group width.
    if logical >= 64 { return; }
    unsafe { out.write(ProcessorNumber { group: 0, number: logical as u8, reserved: 0 }); }
}

pub unsafe extern "win64" fn ke_get_current_processor_index() -> u32 {
    crate::hal::current_cpu_local().map(|cpu| cpu.logical_id.load(Ordering::Acquire)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_constants_match_nt_contract() { assert_eq!(ALL_PROCESSOR_GROUPS, 0xffff); }
}

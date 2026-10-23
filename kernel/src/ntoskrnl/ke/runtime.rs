//! Callable NT kernel executive (Ke) routines with ABI-compatible signatures.
use core::sync::atomic::{AtomicU32, Ordering};

const ALL_PROCESSOR_GROUPS: u16 = 0xffff;
const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;

/// Windows PROCESSOR_NUMBER layout: group, group-relative number, reserved.
#[repr(C)]
pub struct ProcessorNumber { pub group: u16, pub number: u8, pub reserved: u8 }

/// Minimal x64 DISPATCHER_HEADER prefix shared by KEVENT/KSEMAPHORE.
#[repr(C)]
struct DispatcherHeader {
    kind: u8,
    absolute: u8,
    size: u8,
    inserted: u8,
    signal_state: i32,
    wait_list_flink: u64,
    wait_list_blink: u64,
}

const EVENT_NOTIFICATION: u8 = 0;
const EVENT_SYNCHRONIZATION: u8 = 1;
const EVENT_SIZE: u8 = 4; // 24-byte header in 32-bit units.

/// IRQL is PASSIVE_LEVEL until the interrupt-priority layer is brought online.
pub unsafe extern "win64" fn ke_get_current_irql() -> u8 { 0 }

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
pub unsafe extern "win64" fn ke_query_maximum_processor_count_ex(group: u16) -> u32 {
    if group == 0 || group == ALL_PROCESSOR_GROUPS { crate::hal::MAX_CPUS as u32 } else { 0 }
}
pub unsafe extern "win64" fn ke_query_maximum_processor_count() -> u32 {
    crate::hal::MAX_CPUS as u32
}
pub unsafe extern "win64" fn ke_get_current_processor_number_ex(out: *mut ProcessorNumber) {
    if out.is_null() { return; }
    let Some(cpu) = crate::hal::current_cpu_local() else { return; };
    let logical = cpu.logical_id.load(Ordering::Acquire);
    if logical < 64 {
        unsafe { out.write(ProcessorNumber { group: 0, number: logical as u8, reserved: 0 }); }
    }
}
pub unsafe extern "win64" fn ke_get_current_processor_index() -> u32 {
    crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Acquire))
        .unwrap_or(0)
}
pub unsafe extern "win64" fn ke_get_current_node_number() -> u32 { 0 }
pub unsafe extern "win64" fn ke_get_recommended_shared_data_alignment() -> u32 { 64 }

/// Initialize an NT spin-lock word. Actual IRQL raising/spin ownership is not
/// advertised until the scheduler/interrupt-priority layer exists.
pub unsafe extern "win64" fn ke_initialize_spin_lock(lock: *mut u64) {
    if !lock.is_null() { unsafe { core::ptr::write_volatile(lock, 0); } }
}
pub unsafe extern "win64" fn ke_test_spin_lock(lock: *const u64) -> u8 {
    if lock.is_null() { return 0; }
    (unsafe { core::ptr::read_volatile(lock) } == 0) as u8
}
pub unsafe extern "win64" fn ke_flush_write_buffer() {
    core::sync::atomic::fence(Ordering::SeqCst);
    #[cfg(target_arch = "x86_64")]
    unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)); }
}

/// Initialize notification (type 0) or synchronization (type 1) events.
pub unsafe extern "win64" fn ke_initialize_event(event: *mut DispatcherHeader, event_type: i32, state: u8) {
    if event.is_null() || !(event_type == 0 || event_type == 1) { return; }
    unsafe {
        event.write(DispatcherHeader {
            kind: if event_type == 0 { EVENT_NOTIFICATION } else { EVENT_SYNCHRONIZATION },
            absolute: 0, size: EVENT_SIZE, inserted: 0,
            signal_state: if state != 0 { 1 } else { 0 },
            wait_list_flink: event as u64 + core::mem::size_of::<DispatcherHeader>() as u64 - 16,
            wait_list_blink: event as u64 + core::mem::size_of::<DispatcherHeader>() as u64 - 16,
        });
    }
}
pub unsafe extern "win64" fn ke_clear_event(event: *mut DispatcherHeader) -> i32 {
    if event.is_null() { return 0; }
    unsafe { core::ptr::replace(&mut (*event).signal_state, 0) }
}
pub unsafe extern "win64" fn ke_reset_event(event: *mut DispatcherHeader) -> i32 {
    unsafe { ke_clear_event(event) }
}
pub unsafe extern "win64" fn ke_read_state_event(event: *const DispatcherHeader) -> i32 {
    if event.is_null() { return 0; }
    unsafe { core::ptr::read_volatile(&(*event).signal_state) }
}
pub unsafe extern "win64" fn ke_set_event(event: *mut DispatcherHeader, _increment: i32, _wait: u8) -> i32 {
    if event.is_null() { return 0; }
    unsafe { core::ptr::replace(&mut (*event).signal_state, 1) }
}

/// Semaphore header followed by the NT limit field.
#[repr(C)]
struct SemaphoreObject { header: DispatcherHeader, limit: i32 }
pub unsafe extern "win64" fn ke_initialize_semaphore(semaphore: *mut SemaphoreObject, count: i32, limit: i32) -> i32 {
    if semaphore.is_null() || count < 0 || limit <= 0 || count > limit { return STATUS_INVALID_PARAMETER; }
    unsafe {
        semaphore.write(SemaphoreObject {
            header: DispatcherHeader {
                kind: 5, absolute: 0, size: 5, inserted: 0, signal_state: count,
                wait_list_flink: semaphore as u64 + 8,
                wait_list_blink: semaphore as u64 + 8,
            },
            limit,
        });
    }
    STATUS_SUCCESS
}
pub unsafe extern "win64" fn ke_read_state_semaphore(semaphore: *const SemaphoreObject) -> i32 {
    if semaphore.is_null() { return 0; }
    unsafe { core::ptr::read_volatile(&(*semaphore).header.signal_state) }
}
pub unsafe extern "win64" fn ke_release_semaphore(semaphore: *mut SemaphoreObject, increment: i32, adjustment: i32, _wait: u8) -> i32 {
    if semaphore.is_null() || adjustment <= 0 { return STATUS_INVALID_PARAMETER; }
    let header = unsafe { &mut (*semaphore).header };
    let old = unsafe { core::ptr::read_volatile(&header.signal_state) };
    let limit = unsafe { (*semaphore).limit };
    let Some(next) = old.checked_add(adjustment) else { return STATUS_INVALID_PARAMETER; };
    if next > limit { return STATUS_INVALID_PARAMETER; }
    unsafe { core::ptr::write_volatile(&mut header.signal_state, next); }
    let _ = increment; // priority boosts are not supported without a scheduler.
    old
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dispatcher_header_layout_matches_x64_nt() {
        assert_eq!(core::mem::size_of::<DispatcherHeader>(), 24);
        assert_eq!(core::mem::size_of::<SemaphoreObject>(), 32);
    }
    #[test]
    fn event_type_values_match_nt() {
        assert_eq!(EVENT_NOTIFICATION, 0);
        assert_eq!(EVENT_SYNCHRONIZATION, 1);
    }
}

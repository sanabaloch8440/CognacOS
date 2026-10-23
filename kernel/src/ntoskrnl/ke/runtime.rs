//! Callable NT kernel executive (Ke) routines with ABI-compatible signatures.
use core::sync::atomic::Ordering;

const ALL_PROCESSOR_GROUPS: u16 = 0xffff;
const PASSIVE_LEVEL: u8 = 0;
const DISPATCH_LEVEL: u8 = 2;
const IRQL_STACK_DEPTH: usize = 16;
static DISPATCHER_OBJECT_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
static CURRENT_IRQL: [core::sync::atomic::AtomicU8; 64] =
    [const { core::sync::atomic::AtomicU8::new(PASSIVE_LEVEL) }; 64];
static IRQL_DEPTH: [core::sync::atomic::AtomicU8; 64] =
    [const { core::sync::atomic::AtomicU8::new(0) }; 64];
static IRQL_FLAGS: [core::sync::atomic::AtomicU64; 64 * IRQL_STACK_DEPTH] =
    [const { core::sync::atomic::AtomicU64::new(0) }; 64 * IRQL_STACK_DEPTH];

#[inline]
fn cpu_slot() -> usize {
    crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Relaxed) as usize)
        .filter(|id| *id < 64)
        .unwrap_or(0)
}
#[inline]
unsafe fn read_rflags() -> u64 {
    let flags: u64;
    unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags)); }
    flags
}
#[inline]
unsafe fn irq_disable() { unsafe { core::arch::asm!("cli", options(nomem, nostack)); } }
#[inline]
unsafe fn irq_restore(flags: u64) {
    if flags & (1 << 9) != 0 {
        unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    }
}
fn push_irq_state(flags: u64) {
    let cpu = cpu_slot();
    let depth = IRQL_DEPTH[cpu].load(Ordering::Relaxed) as usize;
    if depth < IRQL_STACK_DEPTH {
        IRQL_FLAGS[cpu * IRQL_STACK_DEPTH + depth].store(flags, Ordering::Relaxed);
        IRQL_DEPTH[cpu].store((depth + 1) as u8, Ordering::Release);
    }
}
fn pop_irq_state() -> u64 {
    let cpu = cpu_slot();
    let depth = IRQL_DEPTH[cpu].load(Ordering::Acquire) as usize;
    if depth == 0 { return 0; }
    IRQL_DEPTH[cpu].store((depth - 1) as u8, Ordering::Release);
    IRQL_FLAGS[cpu * IRQL_STACK_DEPTH + depth - 1].load(Ordering::Relaxed)
}
#[inline]
fn atomic_lock_word(lock: *mut u64) -> Option<&'static core::sync::atomic::AtomicU64> {
    if lock.is_null() || (lock as usize) % core::mem::align_of::<u64>() != 0 { return None; }
    Some(unsafe { &*(lock.cast::<core::sync::atomic::AtomicU64>()) })
}


/// Windows PROCESSOR_NUMBER layout: group, group-relative number, reserved.
#[repr(C)]
pub struct ProcessorNumber { pub group: u16, pub number: u8, pub reserved: u8 }

/// Minimal x64 DISPATCHER_HEADER prefix shared by KEVENT/KSEMAPHORE.
#[repr(C)]
pub struct DispatcherHeader {
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
const EVENT_SIZE: u8 = 6; // sizeof(KEVENT) / sizeof(ULONG).

/// IRQL is PASSIVE_LEVEL until the interrupt-priority layer is brought online.
pub unsafe extern "win64" fn ke_get_current_irql() -> u8 { CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire) }

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

/// Initialize a Windows x64 KSPIN_LOCK word.
pub unsafe extern "win64" fn ke_initialize_spin_lock(lock: *mut u64) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
}
pub unsafe extern "win64" fn ke_test_spin_lock(lock: *const u64) -> u8 {
    if lock.is_null() || (lock as usize) % core::mem::align_of::<u64>() != 0 { return 0; }
    let word = unsafe { &*(lock.cast::<core::sync::atomic::AtomicU64>()) };
    (word.load(Ordering::Relaxed) == 0) as u8
}

/// Raise to the kernel's conservative DISPATCH_LEVEL model. Until an APIC TPR
/// policy is installed, local interrupts are masked rather than priority-filtered.
pub unsafe extern "win64" fn ke_raise_irql_to_dpc_level() -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(DISPATCH_LEVEL, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(DISPATCH_LEVEL) };
    old
}
pub unsafe extern "win64" fn ke_lower_irql(new_irql: u8) {
    let cpu = cpu_slot();
    CURRENT_IRQL[cpu].store(new_irql, Ordering::Release);
    let _ = unsafe { crate::apic::set_task_priority(new_irql) };
    let flags = pop_irq_state();
    unsafe { irq_restore(flags); }
}

/// KeAcquireSpinLock: disable local interrupts, raise logical IRQL, then acquire
/// atomically so an interrupt handler cannot recursively deadlock this CPU.
pub unsafe extern "win64" fn ke_acquire_spin_lock(lock: *mut u64, old_irql: *mut u8) {
    if old_irql.is_null() { return; }
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(DISPATCH_LEVEL, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(DISPATCH_LEVEL) };
    unsafe { old_irql.write(old); }
    let Some(word) = atomic_lock_word(lock) else {
        CURRENT_IRQL[cpu_slot()].store(old, Ordering::Release);
        let _ = unsafe { crate::apic::set_task_priority(old) };
        let saved = pop_irq_state(); unsafe { irq_restore(saved); }
        return;
    };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
/// Raise to SYNCH_LEVEL. Local interrupts are conservatively masked because
/// this kernel does not yet program APIC task-priority registers.
pub unsafe extern "win64" fn ke_raise_irql_to_synch_level() -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(12, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(12) };
    old
}
/// Acquire a spin lock at SYNCH_LEVEL and return the prior IRQL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_raise_to_synch(lock: *mut u64) -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(12, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(12) };
    let Some(word) = atomic_lock_word(lock) else {
        CURRENT_IRQL[cpu_slot()].store(old, Ordering::Release);
        let saved = pop_irq_state(); unsafe { irq_restore(saved); }
        return old;
    };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    old
}
/// Acquire a spin lock while raising to DISPATCH_LEVEL; returns the prior IRQL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_raise_to_dpc(lock: *mut u64) -> u8 {
    let mut old_irql = PASSIVE_LEVEL;
    unsafe { ke_acquire_spin_lock(lock, &mut old_irql as *mut u8); }
    old_irql
}
pub unsafe extern "win64" fn ke_release_spin_lock(lock: *mut u64, old_irql: u8) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
    unsafe { ke_lower_irql(old_irql); }
}
pub unsafe extern "win64" fn ke_acquire_spin_lock_at_dpc_level(lock: *mut u64) {
    let Some(word) = atomic_lock_word(lock) else { return; };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
pub unsafe extern "win64" fn ke_release_spin_lock_from_dpc_level(lock: *mut u64) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
}
pub unsafe extern "win64" fn ke_try_to_acquire_spin_lock_at_dpc_level(lock: *mut u64) -> u8 {
    match atomic_lock_word(lock) {
        Some(word) => word.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_ok() as u8,
        None => 0,
    }
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
            wait_list_flink: event as u64 + 8,
            wait_list_blink: event as u64 + 8,
        });
    }
}
pub unsafe extern "win64" fn ke_clear_event(event: *mut DispatcherHeader) -> i32 {
    if event.is_null() { return 0; }
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    unsafe { (&*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>()).swap(0, Ordering::AcqRel) }
}
pub unsafe extern "win64" fn ke_reset_event(event: *mut DispatcherHeader) -> i32 {
    unsafe { ke_clear_event(event) }
}
pub unsafe extern "win64" fn ke_read_state_event(event: *const DispatcherHeader) -> i32 {
    if event.is_null() { return 0; }
    unsafe { (&*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire) }
}
pub unsafe extern "win64" fn ke_set_event(event: *mut DispatcherHeader, _increment: i32, _wait: u8) -> i32 {
    if event.is_null() { return 0; }
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    let state = unsafe { &*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>() };
    let old = state.swap(1, Ordering::AcqRel);
    complete_ready_wait_all_groups();
    if unsafe { (*event).kind } == EVENT_SYNCHRONIZATION {
        if state.load(Ordering::Acquire) > 0 {
            if let Some(thread_id) = crate::dispatcher::wake_one(event as u64) {
                let status = crate::dispatcher::complete_wait_group(thread_id, event as u64, STATUS_SUCCESS, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(STATUS_SUCCESS);
                let _ = crate::thread::complete_wait(thread_id, status);
                if status == STATUS_SUCCESS {
                    let _ = state.compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire);
                }
            }
        }
    } else {
        let mut ids = [0u64; crate::dispatcher::MAX_WAITERS];
        let count = crate::dispatcher::wake_all(event as u64, &mut ids);
        for thread_id in ids[..count].iter().copied() {
            let status = crate::dispatcher::complete_wait_group(thread_id, event as u64, STATUS_SUCCESS, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(STATUS_SUCCESS);
            let _ = crate::thread::complete_wait(thread_id, status);
        }
    }
    old
}

/// Semaphore header followed by the NT limit field.
#[repr(C)]
pub struct SemaphoreObject { header: DispatcherHeader, limit: i32 }
pub unsafe extern "win64" fn ke_initialize_semaphore(semaphore: *mut SemaphoreObject, count: i32, limit: i32) {
    if semaphore.is_null() || count < 0 || limit <= 0 || count > limit { return; }
    unsafe {
        semaphore.write(SemaphoreObject {
            header: DispatcherHeader {
                kind: 5, absolute: 0, size: 8, inserted: 0, signal_state: count,
                wait_list_flink: semaphore as u64 + 8,
                wait_list_blink: semaphore as u64 + 8,
            },
            limit,
        });
    }
}
pub unsafe extern "win64" fn ke_read_state_semaphore(semaphore: *const SemaphoreObject) -> i32 {
    if semaphore.is_null() { return 0; }
    unsafe { (&*core::ptr::addr_of!((*semaphore).header.signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire) }
}
pub unsafe extern "win64" fn ke_release_semaphore(semaphore: *mut SemaphoreObject, increment: i32, adjustment: i32, _wait: u8) -> i32 {
    if semaphore.is_null() || adjustment <= 0 { return 0; }
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    let state = unsafe { &*core::ptr::addr_of!((*semaphore).header.signal_state).cast::<core::sync::atomic::AtomicI32>() };
    let limit = unsafe { (*semaphore).limit };
    let old = state.load(Ordering::Acquire);
    let Some(next) = old.checked_add(adjustment) else { return 0; };
    if next > limit { return 0; }
    state.store(next, Ordering::Release);
    complete_ready_wait_all_groups();
    // Each waiter consumes one available count before becoming runnable.
    loop {
        if state.load(Ordering::Acquire) <= 0 { break; }
        let Some(thread_id) = crate::dispatcher::wake_one(semaphore as u64) else { break; };
        state.fetch_sub(1, Ordering::AcqRel);
        let status = crate::dispatcher::complete_wait_group(thread_id, semaphore as u64, STATUS_SUCCESS, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(STATUS_SUCCESS);
        if status == STATUS_TIMEOUT { state.fetch_add(1, Ordering::AcqRel); }
        let _ = crate::thread::complete_wait(thread_id, status);
    }
    let _ = increment; // priority boosts require a scheduler priority model.
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


const STATUS_SUCCESS: i32 = 0;
const STATUS_ALERTED: i32 = 0x0000_0101;
const STATUS_TIMEOUT: i32 = 0x0000_0102;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002_u32 as i32;

/// Consume a signalled event/semaphore state using an atomic compare/exchange.
unsafe fn try_consume_dispatcher_object_locked(object: *mut DispatcherHeader) -> bool {
    if object.is_null() { return false; }
    let state = unsafe { &*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>() };
    match unsafe { (*object).kind } {
        EVENT_NOTIFICATION => state.load(Ordering::Acquire) > 0,
        EVENT_SYNCHRONIZATION | 5 => {
            let old = state.load(Ordering::Acquire);
            if old <= 0 { return false; }
            state.store(old - 1, Ordering::Release);
            true
        }
        _ => false,
    }
}
unsafe fn try_consume_dispatcher_object(object: *mut DispatcherHeader) -> bool {
    let _guard = DISPATCHER_OBJECT_LOCK.lock();
    unsafe { try_consume_dispatcher_object_locked(object) }
}

fn wait_all_object_signalled(object_id:u64)->bool{
    let object=object_id as *mut DispatcherHeader;
    if object.is_null(){return false}
    let kind=unsafe{(*object).kind};
    if !matches!(kind,EVENT_NOTIFICATION|EVENT_SYNCHRONIZATION|5){return false}
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)>0}
}
fn consume_wait_all_object(object_id:u64){
    let object=object_id as *mut DispatcherHeader;
    if object.is_null()||unsafe{(*object).kind}==EVENT_NOTIFICATION{return}
    let state=unsafe{&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()};
    state.store(state.load(Ordering::Acquire)-1,Ordering::Release);
}
fn complete_ready_wait_all_groups(){
    let mut ids=[0u64;crate::dispatcher::MAX_WAIT_GROUPS];
    let count=crate::dispatcher::complete_ready_wait_all(&mut ids,wait_all_object_signalled,consume_wait_all_object);
    for id in ids[..count].iter().copied(){let _=crate::thread::complete_wait(id,STATUS_SUCCESS);}
}

/// Block the current RdotOS kernel thread on a dispatcher object. This requires
/// a thread created by the kernel thread runtime; unsupported caller contexts
/// return STATUS_NOT_IMPLEMENTED instead of pretending that a wait occurred.
pub unsafe extern "win64" fn ke_wait_for_single_object(
    object: *mut DispatcherHeader,
    _wait_reason: i32,
    _wait_mode: i32,
    alertable: u8,
    timeout: *const i64,
) -> i32 {
    if object.is_null() { return STATUS_INVALID_PARAMETER; }
    if alertable != 0 { return STATUS_NOT_IMPLEMENTED; } // APC delivery is not yet implemented.
    let Some(thread_id) = crate::thread::current_thread_id() else { return STATUS_NOT_IMPLEMENTED; };
    if unsafe { try_consume_dispatcher_object(object) } { return STATUS_SUCCESS; }
    let deadline = if timeout.is_null() { 0 } else {
        let interval = unsafe { timeout.read_unaligned() };
        if interval == 0 { return STATUS_TIMEOUT; }
        if interval > 0 { return STATUS_NOT_IMPLEMENTED; }
        if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
        match crate::apic::relative_timeout_deadline(interval) {
            Some(value) => value,
            None => return STATUS_NOT_IMPLEMENTED,
        }
    };
    let token = match crate::dispatcher::enqueue(object as u64, thread_id, deadline) {
        Ok(value) => value,
        Err(_) => return STATUS_NOT_IMPLEMENTED,
    };
    if unsafe { try_consume_dispatcher_object(object) } {
        let _ = crate::dispatcher::cancel(token);
        let _ = crate::thread::clear_wake_pending();
        return STATUS_SUCCESS;
    }
    if crate::thread::block_current().is_err() {
        let _ = crate::dispatcher::cancel(token);
        return STATUS_NOT_IMPLEMENTED;
    }
    crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS)
}

/// Multiple-object waits are rejected explicitly until shared wait-block groups
/// can provide atomic wait-all reservation and safe cancellation of wait-any peers.
pub unsafe extern "win64" fn ke_wait_for_multiple_objects(
    count: u32,
    objects: *const *mut DispatcherHeader,
    wait_type: i32,
    wait_reason: i32,
    wait_mode: i32,
    alertable: u8,
    timeout: *const i64,
    _wait_block_array: *mut core::ffi::c_void,
) -> i32 {
    if count == 0 || count > 64 || objects.is_null() || !(wait_type == 0 || wait_type == 1) {
        return STATUS_INVALID_PARAMETER;
    }
    let _ = (wait_reason, wait_mode);
    if alertable != 0 { return STATUS_NOT_IMPLEMENTED; }
    let object_list = unsafe { core::slice::from_raw_parts(objects, count as usize) };
    if object_list.iter().any(|object| object.is_null()) { return STATUS_INVALID_PARAMETER; }
    for i in 0..object_list.len() {
        let kind = unsafe { (*object_list[i]).kind };
        if !matches!(kind, EVENT_NOTIFICATION | EVENT_SYNCHRONIZATION | 5) {
            return STATUS_NOT_IMPLEMENTED;
        }
        if object_list[..i].iter().any(|previous| *previous == object_list[i]) {
            return STATUS_INVALID_PARAMETER;
        }
    }
    let current_id = crate::thread::current_thread_id();
    let guard = DISPATCHER_OBJECT_LOCK.lock();
    if wait_type == 1 {
        for (index, object) in object_list.iter().copied().enumerate() {
            if unsafe { try_consume_dispatcher_object_locked(object) } {
                return index as i32;
            }
        }
    } else {
        let all_signalled = object_list.iter().copied().all(|object| {
            unsafe { (&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire) > 0 }
        });
        if all_signalled {
            for object in object_list.iter().copied() { consume_wait_all_object(object as u64); }
            return STATUS_SUCCESS;
        }
    }
    if !timeout.is_null() && unsafe { timeout.read_unaligned() } == 0 { return STATUS_TIMEOUT; }
    let Some(thread_id) = current_id else { return STATUS_NOT_IMPLEMENTED; };
    let deadline = if timeout.is_null() { 0 } else {
        let interval = unsafe { timeout.read_unaligned() };
        if interval > 0 || !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
        match crate::apic::relative_timeout_deadline(interval) {
            Some(value) => value,
            None => return STATUS_NOT_IMPLEMENTED,
        }
    };
    let mut object_ids = [0u64; 64];
    for (index, object) in object_list.iter().copied().enumerate() { object_ids[index] = object as u64; }

    if wait_type == 0 {
        let timeout_token = if deadline != 0 {
            match crate::dispatcher::enqueue(crate::dispatcher::wait_all_timeout_object(thread_id), thread_id, deadline) {
                Ok(token) => Some(token),
                Err(_) => return STATUS_NOT_IMPLEMENTED,
            }
        } else { None };
        if crate::dispatcher::register_wait_all(thread_id, &object_ids[..count as usize], timeout_token, deadline).is_err() {
            if let Some(token) = timeout_token { let _ = crate::dispatcher::cancel(token); }
            return STATUS_NOT_IMPLEMENTED;
        }
        drop(guard);
        if crate::thread::block_current().is_err() {
            crate::dispatcher::clear_wait_group(thread_id);
            if let Some(token) = timeout_token { let _ = crate::dispatcher::cancel(token); }
            return STATUS_NOT_IMPLEMENTED;
        }
        return crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
    }

    let mut tokens = [0u64; 64];
    let mut registered = 0usize;
    for object in object_ids[..count as usize].iter().copied() {
        match crate::dispatcher::enqueue(object, thread_id, deadline) {
            Ok(token) => { tokens[registered] = token; registered += 1; }
            Err(_) => {
                for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
                return STATUS_NOT_IMPLEMENTED;
            }
        }
    }
    if crate::dispatcher::register_wait_any(thread_id, &object_ids[..count as usize], &tokens[..registered], deadline).is_err() {
        for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
        return STATUS_NOT_IMPLEMENTED;
    }
    drop(guard);
    if crate::thread::block_current().is_err() {
        crate::dispatcher::clear_wait_group(thread_id);
        for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
        return STATUS_NOT_IMPLEMENTED;
    }
    crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS)
}

/// Never busy-loops when the thread runtime or calibrated delay source is absent.
pub unsafe extern "win64" fn ke_delay_execution_thread(
    wait_mode: i32,
    alertable: u8,
    interval: *const i64,
) -> i32 {
    if interval.is_null() { return STATUS_INVALID_PARAMETER; }
    if alertable != 0 { return STATUS_NOT_IMPLEMENTED; }
    let value = unsafe { interval.read_unaligned() };
    if value == 0 { return STATUS_SUCCESS; }
    if value > 0 { return STATUS_NOT_IMPLEMENTED; }
    let _ = wait_mode;
    let Some(thread_id) = crate::thread::current_thread_id() else { return STATUS_NOT_IMPLEMENTED; };
    if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
    let deadline = match crate::apic::relative_timeout_deadline(value) {
        Some(value) => value,
        None => return STATUS_NOT_IMPLEMENTED,
    };
    let token = match crate::dispatcher::enqueue(crate::dispatcher::delay_object(thread_id), thread_id, deadline) {
        Ok(value) => value,
        Err(_) => return STATUS_NOT_IMPLEMENTED,
    };
    if crate::thread::block_current().is_err() {
        let _ = crate::dispatcher::cancel(token);
        return STATUS_NOT_IMPLEMENTED;
    }
    crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS)
}

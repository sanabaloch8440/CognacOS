// events and semaphore dispatcher objects.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

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
pub unsafe extern "win64" fn ke_set_event(event: *mut DispatcherHeader, increment: i32, wait: u8) -> i32 {
    if event.is_null() { return 0; }
    // WDK requires Wait=TRUE to be followed immediately by KeWaitXxx. Keep
    // local preemption deferred and raise the modeled IRQL until that wait
    // consumes the handoff; the wait routine restores the caller's old IRQL.
    if wait != 0 { begin_ke_set_event_wait_handoff(); }
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    let state = unsafe { &*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>() };
    let old = state.swap(1, Ordering::AcqRel);
    complete_ready_wait_all_groups();
    if unsafe { (*event).kind } == EVENT_SYNCHRONIZATION {
        if state.load(Ordering::Acquire) > 0 {
            if let Some(waiter) = crate::dispatcher::wake_one_timed(event as u64) {
                let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
                let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
                let _ = crate::thread::complete_wait(waiter.thread, status);
                 if status == STATUS_SUCCESS { boost_thread_priority(waiter.thread, increment); }
                if status == STATUS_SUCCESS {
                    let _ = state.compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire);
                }
            }
        }
    } else {
        let mut waiters = [crate::dispatcher::WokenWaiter { thread: 0, timed_out: false }; crate::dispatcher::MAX_WAITERS];
        let count = crate::dispatcher::wake_all_timed(event as u64, &mut waiters);
        for waiter in waiters[..count].iter().copied() {
            let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
            let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
            let _ = crate::thread::complete_wait(waiter.thread, status);
                 if status == STATUS_SUCCESS { boost_thread_priority(waiter.thread, increment); }
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
        let Some(waiter) = crate::dispatcher::wake_one_timed(semaphore as u64) else { break; };
        let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
        if !waiter.timed_out { state.fetch_sub(1, Ordering::AcqRel); }
        let status = crate::dispatcher::complete_wait_group(waiter.thread, semaphore as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
        if status == STATUS_TIMEOUT && !waiter.timed_out { state.fetch_add(1, Ordering::AcqRel); }
        let _ = crate::thread::complete_wait(waiter.thread, status);
                 if status == STATUS_SUCCESS { boost_thread_priority(waiter.thread, increment); }
    }

    old
}


static INTERRUPT_TIME_ORIGIN: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

#[repr(C)]
pub struct GroupAffinity {
    pub mask: u64,
    pub group: u16,
    pub reserved: [u16; 3],
}

fn interrupt_time_100ns() -> u64 {
    let Some(hz) = crate::hal::query_performance_frequency_hz().filter(|hz| *hz != 0) else { return 0; };
    let now = crate::hal::query_performance_counter();
    let origin = match INTERRUPT_TIME_ORIGIN.compare_exchange(0, now.max(1), Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => now.max(1),
        Err(existing) => existing,
    };
    ((now.saturating_sub(origin) as u128) * 10_000_000u128 / hz as u128)
        .min(u64::MAX as u128) as u64
}


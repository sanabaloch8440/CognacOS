// APC state, event pulse, waits, and thread affinity.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

// The BSP/bootstrap path can call APC-region helpers before a schedulable
// thread exists. These counters are used only for that no-thread context.
static BOOTSTRAP_CRITICAL_DEPTH:[core::sync::atomic::AtomicU32;64]=[const{core::sync::atomic::AtomicU32::new(0)};64];
static BOOTSTRAP_GUARDED_DEPTH:[core::sync::atomic::AtomicU32;64]=[const{core::sync::atomic::AtomicU32::new(0)};64];
/// Critical and guarded region nesting lives in the current thread record.
/// Bootstrap/no-thread contexts cannot retain per-thread APC state and therefore
/// do not manufacture a process-global substitute.
pub unsafe extern "win64" fn ke_enter_critical_region() {
    if !crate::thread::adjust_apc_region(false, true) { BOOTSTRAP_CRITICAL_DEPTH[cpu_slot()].fetch_add(1,Ordering::AcqRel); }
}
pub unsafe extern "win64" fn ke_leave_critical_region() {
    if !crate::thread::adjust_apc_region(false, false) { let depth=&BOOTSTRAP_CRITICAL_DEPTH[cpu_slot()];let _=depth.fetch_update(Ordering::AcqRel,Ordering::Acquire,|v|Some(v.saturating_sub(1))); }
}
pub unsafe extern "win64" fn ke_enter_guarded_region() {
    if !crate::thread::adjust_apc_region(true, true) { BOOTSTRAP_GUARDED_DEPTH[cpu_slot()].fetch_add(1,Ordering::AcqRel); }
}
pub unsafe extern "win64" fn ke_leave_guarded_region() {
    if !crate::thread::adjust_apc_region(true, false) { let depth=&BOOTSTRAP_GUARDED_DEPTH[cpu_slot()];let _=depth.fetch_update(Ordering::AcqRel,Ordering::Acquire,|v|Some(v.saturating_sub(1))); }
}
pub unsafe extern "win64" fn ke_are_apcs_disabled() -> u8 {
    let (critical, _) = crate::thread::apc_region_depths();
    let critical=if crate::thread::current_thread_id().is_some(){critical as u32}else{BOOTSTRAP_CRITICAL_DEPTH[cpu_slot()].load(Ordering::Acquire)};
    (CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire) >= 1 || critical != 0) as u8
}
pub unsafe extern "win64" fn ke_are_all_apcs_disabled() -> u8 {
    let (critical, guarded) = crate::thread::apc_region_depths();
    let (critical,guarded)=if crate::thread::current_thread_id().is_some(){(critical as u32,guarded as u32)}else{(BOOTSTRAP_CRITICAL_DEPTH[cpu_slot()].load(Ordering::Acquire),BOOTSTRAP_GUARDED_DEPTH[cpu_slot()].load(Ordering::Acquire))};
    (CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire) >= 1 || critical != 0 || guarded != 0) as u8
}
/// BOOLEAN KeAlertThread(PKTHREAD Thread, KPROCESSOR_MODE AlertMode).
/// This implements thread alert state, not kernel/user APC object delivery.
pub unsafe extern "win64" fn ke_alert_thread(thread:*mut core::ffi::c_void,_alert_mode:i32)->u8{
    let Some(id)=crate::thread::thread_id_from_object(thread) else{return 0;};
    crate::thread::set_alert_pending(id).unwrap_or(false) as u8
}
/// BOOLEAN KeTestAlertThread(KPROCESSOR_MODE AlertMode).
pub unsafe extern "win64" fn ke_test_alert_thread(_alert_mode:i32)->u8{
    crate::thread::take_alert_pending() as u8
}

/// LONG KePulseEvent(PKEVENT, KPRIORITY, BOOLEAN).
pub unsafe extern "win64" fn ke_pulse_event(
    event: *mut DispatcherHeader, increment: i32, wait: u8,
) -> i32 {
    if event.is_null() { return 0; }
    // Serialize the entire pulse against concurrent state changes.
    let _object_guard = DISPATCHER_OBJECT_LOCK.lock();
    let state = unsafe { &*core::ptr::addr_of!((*event).signal_state).cast::<core::sync::atomic::AtomicI32>() };
    let previous = state.swap(1, Ordering::AcqRel);
    complete_ready_wait_all_groups();
    match unsafe { (*event).kind } {
        EVENT_NOTIFICATION => {
            let mut waiters = [crate::dispatcher::WokenWaiter { thread: 0, timed_out: false }; crate::dispatcher::MAX_WAITERS];
            let count = crate::dispatcher::wake_all_timed(event as u64, &mut waiters);
            for waiter in waiters[..count].iter().copied() {
                let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
                let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
                let _ = crate::thread::complete_wait(waiter.thread, status);
            }
        }
        EVENT_SYNCHRONIZATION => {
            if let Some(waiter) = crate::dispatcher::wake_one_timed(event as u64) {
                let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
                let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
                let _ = crate::thread::complete_wait(waiter.thread, status);
            }
        }
        _ => {}
    }
    state.store(0, Ordering::Release);
    let _ = (increment, wait); // Priority boosts and atomic Wait handoff remain unsupported.
    previous
}

/// NTSTATUS KeQueryNodeActiveAffinity2(USHORT, PGROUP_AFFINITY, USHORT, PUSHORT).
/// The current topology is one node represented by one affinity group.
pub unsafe extern "win64" fn ke_query_node_active_affinity2(
    node: u16, affinities: *mut GroupAffinity, capacity: u16, required: *mut u16,
) -> i32 {
    if !required.is_null() { unsafe { required.write(if node == 0 { 1 } else { 0 }); } }
    if node != 0 { return STATUS_INVALID_PARAMETER; }
    if capacity < 1 || affinities.is_null() { return 0xC000_0023u32 as i32; }
    unsafe {
        affinities.write(GroupAffinity {
            mask: ke_query_group_affinity(0), group: 0, reserved: [0; 3],
        });
    }
    STATUS_SUCCESS
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
    #[test]
    fn critical_and_guarded_region_nesting_is_balanced() {
        let cpu = cpu_slot();
        let prior_irql = CURRENT_IRQL[cpu].load(Ordering::Acquire);
        unsafe {
            ke_enter_critical_region();
            assert_eq!(ke_are_apcs_disabled(), 1);
            ke_enter_critical_region();
            ke_leave_critical_region();
            assert_eq!(ke_are_apcs_disabled(), 1);
            ke_leave_critical_region();
            assert_eq!(ke_are_apcs_disabled(), (prior_irql >= 1) as u8);

            ke_enter_guarded_region();
            assert_eq!(ke_are_all_apcs_disabled(), 1);
            ke_enter_guarded_region();
            ke_leave_guarded_region();
            assert_eq!(ke_are_all_apcs_disabled(), 1);
            ke_leave_guarded_region();
            assert_eq!(ke_are_all_apcs_disabled(), (prior_irql >= 1) as u8);
        }
    }

    #[test]
    fn current_processor_number_return_matches_output() {
        let mut number = ProcessorNumber { group: u16::MAX, number: u8::MAX, reserved: u8::MAX };
        let index = unsafe { ke_get_current_processor_number_ex(&mut number) };
        assert_eq!(number.group, 0);
        assert_eq!(number.number as u32, index);
        assert_eq!(number.reserved, 0);
        assert_eq!(unsafe { ke_get_current_processor_number_ex(core::ptr::null_mut()) }, index);
    }

    #[test]
    fn executive_queue_fifo_lifo_and_rundown() {
        let mut queue=core::mem::MaybeUninit::<ExecutiveQueue>::uninit();
        unsafe{ke_initialize_queue(queue.as_mut_ptr(),2);}
        let queue=unsafe{&mut *queue.as_mut_ptr()};
        let mut first=ListEntry{flink:core::ptr::null_mut(),blink:core::ptr::null_mut()};
        let mut second=ListEntry{flink:core::ptr::null_mut(),blink:core::ptr::null_mut()};
        assert_eq!(unsafe{ke_insert_queue(queue,&mut first)},0);
        assert_eq!(unsafe{ke_insert_head_queue(queue,&mut second)},1);
        assert_eq!(queue.header.signal_state,2);
        let no_wait=0i64;
        assert!(core::ptr::eq(unsafe{ke_remove_queue(queue,0,&no_wait)},&mut second));
        assert!(core::ptr::eq(unsafe{ke_remove_queue(queue,0,&no_wait)},&mut first));
        assert_eq!(queue.header.signal_state,0);
        assert!(unsafe{ke_rundown_queue(queue)}.is_null());
    }

    static DPC_TEST_COUNT:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
    unsafe extern "win64" fn dpc_test_routine(_: *mut core::ffi::c_void,_: *mut core::ffi::c_void,_: *mut core::ffi::c_void,_: *mut core::ffi::c_void){
        DPC_TEST_COUNT.fetch_add(1,Ordering::AcqRel);
    }
    #[test]
    fn dpc_queue_rejects_duplicates_and_runs_once(){
        let mut object=0u64;
        let dpc=(&mut object as *mut u64).cast::<core::ffi::c_void>();
        DPC_TEST_COUNT.store(0,Ordering::Release);
        let cpu=cpu_slot();let prior=CURRENT_IRQL[cpu].swap(DISPATCH_LEVEL,Ordering::AcqRel);
        unsafe{ke_initialize_dpc(dpc,Some(dpc_test_routine),core::ptr::null_mut());}
        assert_eq!(unsafe{ke_insert_queue_dpc(dpc,core::ptr::null_mut(),core::ptr::null_mut())},1);
        assert_eq!(unsafe{ke_insert_queue_dpc(dpc,core::ptr::null_mut(),core::ptr::null_mut())},0);
        CURRENT_IRQL[cpu].store(prior,Ordering::Release);
        unsafe{ke_flush_queued_dpcs();}
        assert_eq!(DPC_TEST_COUNT.load(Ordering::Acquire),1);
        assert_eq!(unsafe{ke_remove_queue_dpc(dpc)},0);
    }

    #[test]
    fn timer_object_expires_and_signals(){
        let mut storage=core::mem::MaybeUninit::<DispatcherHeader>::uninit();
        let timer=storage.as_mut_ptr();
        unsafe{ke_initialize_timer(timer);}
        assert_eq!(unsafe{ke_read_state_timer(timer)},0);
        assert_eq!(unsafe{ke_set_timer(timer,0,core::ptr::null_mut())},0);
        timer_tick(crate::apic::monotonic_ticks());
        assert_eq!(unsafe{ke_read_state_timer(timer)},1);
        assert_eq!(unsafe{ke_cancel_timer(timer)},0);
    }

    #[test]
    fn mutant_initial_state_is_signalled(){
        let mut storage=core::mem::MaybeUninit::<DispatcherHeader>::uninit();
        let mutant=storage.as_mut_ptr();
        unsafe{ke_initialize_mutant(mutant,0);}
        assert_eq!(unsafe{ke_read_state_mutant(mutant)},1);
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
        EVENT_NOTIFICATION | TIMER_NOTIFICATION => state.load(Ordering::Acquire) > 0,
        MUTANT_OBJECT => try_acquire_mutant(object),
        EVENT_SYNCHRONIZATION | 5 | TIMER_SYNCHRONIZATION | QUEUE_OBJECT => {
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
    if !matches!(kind,EVENT_NOTIFICATION|EVENT_SYNCHRONIZATION|5|TIMER_NOTIFICATION|TIMER_SYNCHRONIZATION){return false}
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)>0}
}
fn consume_wait_all_object(object_id:u64){
    let object=object_id as *mut DispatcherHeader;
    if object.is_null()||matches!(unsafe{(*object).kind},EVENT_NOTIFICATION|TIMER_NOTIFICATION){return}
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
    if !matches!(unsafe { (*object).kind }, EVENT_NOTIFICATION | EVENT_SYNCHRONIZATION | 5 | TIMER_NOTIFICATION | TIMER_SYNCHRONIZATION | QUEUE_OBJECT | MUTANT_OBJECT) {
        return STATUS_NOT_IMPLEMENTED;
    }
    let Some(thread_id) = crate::thread::current_thread_id() else { return STATUS_NOT_IMPLEMENTED; };
    if alertable != 0 && crate::thread::take_alert_pending() { return STATUS_ALERTED; }
    if unsafe{(*object).kind}==MUTANT_OBJECT{
        if let Some(status)=try_acquire_mutant_status(object){return status}
    }else if unsafe { try_consume_dispatcher_object(object) } { return STATUS_SUCCESS; }
    let deadline = if timeout.is_null() { 0 } else {
        let interval = unsafe { timeout.read_unaligned() };
        if interval == 0 { return STATUS_TIMEOUT; }
        if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
        match timer_deadline(interval) {
            Some(value) => value,
            None => return STATUS_NOT_IMPLEMENTED,
        }
    };
    let _ = crate::thread::set_wait_alertable(alertable != 0);
    let token = match crate::dispatcher::enqueue(object as u64, thread_id, deadline) {
        Ok(value) => value,
        Err(_) => { let _=crate::thread::set_wait_alertable(false); return STATUS_NOT_IMPLEMENTED; },
    };
    if alertable != 0 && crate::thread::take_alert_pending() {
        let _=crate::dispatcher::cancel(token);let _=crate::thread::set_wait_alertable(false);return STATUS_ALERTED;
    }
    let acquired=if unsafe{(*object).kind}==MUTANT_OBJECT{
        try_acquire_mutant_status(object)
    }else if unsafe{try_consume_dispatcher_object(object)}{Some(STATUS_SUCCESS)}else{None};
    if let Some(status)=acquired{
        let _ = crate::dispatcher::cancel(token);
        let _ = crate::thread::clear_wake_pending();
        let _=crate::thread::set_wait_alertable(false);
        return status;
    }
    if crate::thread::block_current().is_err() {
        let _ = crate::dispatcher::cancel(token);
        let _=crate::thread::set_wait_alertable(false);
        return STATUS_NOT_IMPLEMENTED;
    }
    let status=crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
    let _=crate::thread::set_wait_alertable(false);
    status
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
    if alertable != 0 && crate::thread::take_alert_pending() { return STATUS_ALERTED; }
    let object_list = unsafe { core::slice::from_raw_parts(objects, count as usize) };
    if object_list.iter().any(|object| object.is_null()) { return STATUS_INVALID_PARAMETER; }
    for i in 0..object_list.len() {
        let kind = unsafe { (*object_list[i]).kind };
        if !matches!(kind, EVENT_NOTIFICATION | EVENT_SYNCHRONIZATION | 5 | TIMER_NOTIFICATION | TIMER_SYNCHRONIZATION) {
            return STATUS_NOT_IMPLEMENTED;
        }
        // Mutants need an abandoned-status result and queues need entry transfer.
        // Keep both out of the generic multi-wait path until their per-object
        // reservations can be committed atomically for wait-any and wait-all.
        if matches!(kind, MUTANT_OBJECT | QUEUE_OBJECT) {
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
        if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
        match timer_deadline(interval) {
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
        let _=crate::thread::set_wait_alertable(alertable!=0);
        if crate::dispatcher::register_wait_all(thread_id, &object_ids[..count as usize], timeout_token, deadline).is_err() {
            if let Some(token) = timeout_token { let _ = crate::dispatcher::cancel(token); }
            let _=crate::thread::set_wait_alertable(false);return STATUS_NOT_IMPLEMENTED;
        }
        drop(guard);
        if alertable!=0 && crate::thread::take_alert_pending(){
            crate::dispatcher::clear_wait_group(thread_id);if let Some(token)=timeout_token{let _=crate::dispatcher::cancel(token);}
            let _=crate::thread::set_wait_alertable(false);return STATUS_ALERTED;
        }
        if crate::thread::block_current().is_err() {
            crate::dispatcher::clear_wait_group(thread_id);
            if let Some(token) = timeout_token { let _ = crate::dispatcher::cancel(token); }
            let _=crate::thread::set_wait_alertable(false);return STATUS_NOT_IMPLEMENTED;
        }
        let status=crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
        let _=crate::thread::set_wait_alertable(false);
        return status;
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
    let _=crate::thread::set_wait_alertable(alertable!=0);
    if crate::dispatcher::register_wait_any(thread_id, &object_ids[..count as usize], &tokens[..registered], deadline).is_err() {
        for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
        let _=crate::thread::set_wait_alertable(false);return STATUS_NOT_IMPLEMENTED;
    }
    drop(guard);
    if alertable!=0 && crate::thread::take_alert_pending(){
        crate::dispatcher::clear_wait_group(thread_id);for token in tokens[..registered].iter().copied(){let _=crate::dispatcher::cancel(token);}
        let _=crate::thread::set_wait_alertable(false);return STATUS_ALERTED;
    }
    if crate::thread::block_current().is_err() {
        crate::dispatcher::clear_wait_group(thread_id);
        for token in tokens[..registered].iter().copied() { let _ = crate::dispatcher::cancel(token); }
        let _=crate::thread::set_wait_alertable(false);return STATUS_NOT_IMPLEMENTED;
    }
    let status=crate::thread::take_wait_status().unwrap_or(STATUS_SUCCESS);
    let _=crate::thread::set_wait_alertable(false);
    status
}




/// Processor-group affinity is limited to group 0 until multi-group topology
/// exists. The cooperative scheduler refuses masks that would migrate a live
/// context away from its owning CPU.
pub unsafe extern "win64" fn ke_set_affinity_thread(thread:*mut core::ffi::c_void,affinity:u64)->u64{
 let Some(id)=priority_id(thread) else{return 0};
 crate::thread::set_thread_affinity(id,affinity).unwrap_or_else(||crate::thread::thread_affinity(id).unwrap_or(0))
}
const MAX_AFFINITY_RESTORE:usize=128;
#[derive(Clone,Copy)]struct AffinityRestore{id:u64,mask:u64}
impl AffinityRestore{const EMPTY:Self=Self{id:0,mask:0};}
struct AffinityRestoreTable{records:[AffinityRestore;MAX_AFFINITY_RESTORE]}
impl AffinityRestoreTable{const fn new()->Self{Self{records:[AffinityRestore::EMPTY;MAX_AFFINITY_RESTORE]}}}
static AFFINITY_RESTORE:crate::hal::IrqSpinLock<AffinityRestoreTable>=crate::hal::IrqSpinLock::new(AffinityRestoreTable::new());
pub unsafe extern "win64" fn ke_set_system_group_affinity_thread(thread:*mut core::ffi::c_void,new_affinity:*const GroupAffinity,previous_affinity:*mut GroupAffinity){
 if new_affinity.is_null(){return}
 let Some(id)=priority_id(thread) else{return};
 let requested=unsafe{new_affinity.read_unaligned()};
 if requested.group!=0||requested.mask==0{return}
 let Some(old)=crate::thread::set_thread_affinity(id,requested.mask) else{return};
 if !previous_affinity.is_null(){unsafe{previous_affinity.write_unaligned(GroupAffinity{mask:old,group:0,reserved:[0;3]});}}
 let mut table=AFFINITY_RESTORE.lock();
 let slot=table.records.iter().position(|r|r.id==id).or_else(||table.records.iter().position(|r|r.id==0));
 if let Some(slot)=slot{table.records[slot]=AffinityRestore{id,mask:old};}
}
pub unsafe extern "win64" fn ke_revert_to_user_group_affinity_thread(thread:*mut core::ffi::c_void){
 let Some(id)=priority_id(thread) else{return};
 let old={let mut table=AFFINITY_RESTORE.lock();let Some(slot)=table.records.iter().position(|r|r.id==id) else{return};let mask=table.records[slot].mask;table.records[slot]=AffinityRestore::EMPTY;mask};
 if old!=0{let _=crate::thread::set_thread_affinity(id,old);}
}


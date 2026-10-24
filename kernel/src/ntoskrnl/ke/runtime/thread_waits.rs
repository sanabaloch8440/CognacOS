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


// ---- APC object metadata and kernel-mode delivery ----------------------------
// KAPC is opaque driver-owned storage here. Side metadata avoids assuming a
// private Windows KAPC layout. User-mode normal routines are deliberately left
// queued until an actual user-return/trap-frame delivery path exists.
type ApcNormalRoutine = unsafe extern "win64" fn(*mut core::ffi::c_void,*mut core::ffi::c_void,*mut core::ffi::c_void);
type ApcKernelRoutine = unsafe extern "win64" fn(*mut core::ffi::c_void,*mut usize,*mut *mut core::ffi::c_void,*mut *mut core::ffi::c_void,*mut *mut core::ffi::c_void);
type ApcRundownRoutine = unsafe extern "win64" fn(*mut core::ffi::c_void);
#[derive(Clone,Copy)]
struct ApcRecord {
 object:usize, thread_id:u64, kernel_routine:usize, rundown_routine:usize,
 normal_routine:usize, normal_context:usize, arg1:usize, arg2:usize,
 environment:i32, mode:i8, inserted:bool, delivering:bool, kernel_routine_done:bool,
}
impl ApcRecord { const EMPTY:Self=Self{object:0,thread_id:0,kernel_routine:0,rundown_routine:0,normal_routine:0,normal_context:0,arg1:0,arg2:0,environment:0,mode:0,inserted:false,delivering:false,kernel_routine_done:false}; }
struct ApcTable { records:[ApcRecord;128] }
impl ApcTable { const fn new()->Self{Self{records:[ApcRecord::EMPTY;128]}} }
static APC_TABLE:crate::hal::IrqSpinLock<ApcTable>=crate::hal::IrqSpinLock::new(ApcTable::new());
static APC_DELIVERING:[core::sync::atomic::AtomicBool;64]=[const{core::sync::atomic::AtomicBool::new(false)};64];

/// VOID KeInitializeApc(PKAPC, PKTHREAD, KAPC_ENVIRONMENT, PKKERNEL_ROUTINE,
/// PKRUNDOWN_ROUTINE, PKNORMAL_ROUTINE, KPROCESSOR_MODE, PVOID).
pub unsafe extern "win64" fn ke_initialize_apc(
 apc:*mut core::ffi::c_void, thread:*mut core::ffi::c_void, environment:i32,
 kernel_routine:Option<ApcKernelRoutine>, rundown_routine:Option<ApcRundownRoutine>,
 normal_routine:Option<ApcNormalRoutine>, mode:i8, normal_context:*mut core::ffi::c_void,
){
 if apc.is_null(){return}
 let Some(thread_id)=crate::thread::thread_id_from_object(thread) else{return};
 let mut table=APC_TABLE.lock();
 let existing=table.records.iter().position(|r|r.object==apc as usize);
 if existing.is_some_and(|i|table.records[i].inserted||table.records[i].delivering){return}
 let slot=existing.or_else(||table.records.iter().position(|r|r.object==0));
 if let Some(slot)=slot{
  table.records[slot]=ApcRecord{object:apc as usize,thread_id,
   kernel_routine:kernel_routine.map(|f|f as usize).unwrap_or(0),
   rundown_routine:rundown_routine.map(|f|f as usize).unwrap_or(0),
   normal_routine:normal_routine.map(|f|f as usize).unwrap_or(0),
   normal_context:normal_context as usize,arg1:0,arg2:0,environment,mode,inserted:false,
   delivering:false,kernel_routine_done:false};
 }
}

/// BOOLEAN KeInsertQueueApc(PKAPC, PVOID, PVOID, KPRIORITY).
pub unsafe extern "win64" fn ke_insert_queue_apc(
 apc:*mut core::ffi::c_void,arg1:*mut core::ffi::c_void,arg2:*mut core::ffi::c_void,increment:i32,
)->u8{
 if apc.is_null(){return 0}
 let thread_id={
  let mut table=APC_TABLE.lock();
  let Some(record)=table.records.iter_mut().find(|r|r.object==apc as usize) else{return 0};
  if record.inserted||record.delivering{return 0}
  record.arg1=arg1 as usize;record.arg2=arg2 as usize;record.inserted=true;record.thread_id
 };
 boost_thread_priority(thread_id,increment);
 let is_user_apc={
  let table=APC_TABLE.lock();
  table.records.iter().find(|r|r.object==apc as usize).map(|r|r.mode!=0&&r.normal_routine!=0).unwrap_or(false)
 };
 if is_user_apc && crate::thread::is_alertable_wait(thread_id) {
  let _=crate::thread::set_user_apc_delivery_pending(thread_id,true);
  let _=crate::dispatcher::cancel_thread_waits(thread_id);
  crate::dispatcher::clear_wait_group(thread_id);
  let _=crate::thread::complete_wait(thread_id,STATUS_USER_APC);
 } else if crate::thread::current_thread_id()==Some(thread_id)
  && CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire)==PASSIVE_LEVEL
  && unsafe{ke_are_apcs_disabled()}==0 { ke_deliver_pending_kernel_apcs(); }
 1
}

/// BOOLEAN KeRemoveQueueApc(PKAPC).
pub unsafe extern "win64" fn ke_remove_queue_apc(apc:*mut core::ffi::c_void)->u8{
 if apc.is_null(){return 0}
 let mut table=APC_TABLE.lock();
 let Some(record)=table.records.iter_mut().find(|r|r.object==apc as usize) else{return 0};
 let was=record.inserted;record.inserted=false;was as u8
}

const STATUS_USER_APC:i32=0x0000_00C0;
fn has_pending_user_apc(thread_id:u64)->bool{
 let table=APC_TABLE.lock();
 table.records.iter().any(|r|r.inserted&&r.thread_id==thread_id&&r.mode!=0&&r.normal_routine!=0)
}
/// Take one user APC for the current thread. Its kernel routine runs on the
/// kernel stack immediately before the syscall's user-return frame is rewritten.
pub fn take_pending_user_apc()->Option<(usize,usize,usize,usize,usize)>{
 let thread_id=crate::thread::current_thread_id()?;
 let record={
  let mut table=APC_TABLE.lock();
  let slot=table.records.iter().position(|r|r.inserted&&r.thread_id==thread_id&&r.mode!=0&&r.normal_routine!=0)?;
  let record=table.records[slot];table.records[slot].inserted=false;
  table.records[slot].delivering=true;table.records[slot].kernel_routine_done=true;record
 };
 let mut normal=record.normal_routine;
 let mut context=record.normal_context as *mut core::ffi::c_void;
 let mut arg1=record.arg1 as *mut core::ffi::c_void;
 let mut arg2=record.arg2 as *mut core::ffi::c_void;
 if !record.kernel_routine_done&&record.kernel_routine!=0{
  let routine:ApcKernelRoutine=unsafe{core::mem::transmute(record.kernel_routine)};
  unsafe{routine(record.object as *mut _,&mut normal,&mut context,&mut arg1,&mut arg2);}
 }
 Some((record.object,normal,context as usize,arg1 as usize,arg2 as usize))
}
/// Requeue an APC whose user-return frame could not be built. The kernel
/// routine is marked complete so retrying the user return does not invoke it twice.
pub fn requeue_pending_user_apc(object:usize,normal:usize,context:usize,arg1:usize,arg2:usize)->bool{
 let thread_id=crate::thread::current_thread_id().unwrap_or(0);
 let mut table=APC_TABLE.lock();
 let Some(record)=table.records.iter_mut().find(|r|r.object==object&&r.delivering&&!r.inserted) else{return false};
 record.normal_routine=normal;record.normal_context=context;record.arg1=arg1;record.arg2=arg2;
 record.thread_id=thread_id;record.inserted=true;record.delivering=false;true
}
pub fn complete_user_apc_delivery(object:usize){
 let mut table=APC_TABLE.lock();
 if let Some(record)=table.records.iter_mut().find(|r|r.object==object&&r.delivering&&!r.inserted){*record=ApcRecord::EMPTY;}
}

/// Invoke queued APC rundown routines when a thread exits; never call drivers
/// while holding the APC side-table lock.
pub fn ke_rundown_apcs_for_thread(thread_id:u64){
 let mut rundown=[(0usize,0usize);128];let mut count=0usize;
 {
  let mut table=APC_TABLE.lock();
  for record in table.records.iter_mut(){
   if record.object!=0&&record.thread_id==thread_id {
    if record.inserted&&record.rundown_routine!=0&&count<rundown.len(){
     rundown[count]=(record.object,record.rundown_routine);count+=1;
    }
    *record=ApcRecord::EMPTY;
   }
  }
 }
 for (object,routine) in rundown[..count].iter().copied(){
  let callback:ApcRundownRoutine=unsafe{core::mem::transmute(routine)};
  unsafe{callback(object as *mut _);}
 }
}

/// Deliver queued kernel APCs only at a PASSIVE_LEVEL safe point with APCs
/// enabled. User APC normal routines are deferred until an alertable wait marks delivery pending.
pub fn ke_deliver_pending_kernel_apcs(){
 let cpu=cpu_slot();
 if CURRENT_IRQL[cpu].load(Ordering::Acquire)!=PASSIVE_LEVEL
  || unsafe{ke_are_apcs_disabled()}!=0
  || APC_DELIVERING[cpu].swap(true,Ordering::AcqRel){return}
 let Some(thread_id)=crate::thread::current_thread_id() else{
  APC_DELIVERING[cpu].store(false,Ordering::Release);return
 };
 for _ in 0..128 {
  let record={
   let mut table=APC_TABLE.lock();
   let Some(slot)=table.records.iter().position(|r|r.inserted&&r.thread_id==thread_id&&r.mode==0) else{break};
   let record=table.records[slot];table.records[slot].inserted=false;
   table.records[slot].delivering=true;table.records[slot].kernel_routine_done=true;record
  };
  let mut normal=record.normal_routine;
  let mut context=record.normal_context as *mut core::ffi::c_void;
  let mut arg1=record.arg1 as *mut core::ffi::c_void;
  let mut arg2=record.arg2 as *mut core::ffi::c_void;
  if record.kernel_routine!=0{
   let routine:ApcKernelRoutine=unsafe{core::mem::transmute(record.kernel_routine)};
   unsafe{routine(record.object as *mut _,&mut normal,&mut context,&mut arg1,&mut arg2);}
  }
  if normal!=0{
   let routine:ApcNormalRoutine=unsafe{core::mem::transmute(normal)};
   unsafe{routine(context,arg1,arg2);}
  }
  let mut table=APC_TABLE.lock();
  if let Some(slot)=table.records.iter().position(|r|r.object==record.object&&r.delivering&&!r.inserted){table.records[slot]=ApcRecord::EMPTY;}
 }
 APC_DELIVERING[cpu].store(false,Ordering::Release);
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
                if status == STATUS_SUCCESS { boost_thread_priority(waiter.thread, increment); }
            }
        }
        EVENT_SYNCHRONIZATION => {
            if let Some(waiter) = crate::dispatcher::wake_one_timed(event as u64) {
                let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
                let status = crate::dispatcher::complete_wait_group(waiter.thread, event as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT).unwrap_or(signal_status);
                let _ = crate::thread::complete_wait(waiter.thread, status);
                if status == STATUS_SUCCESS { boost_thread_priority(waiter.thread, increment); }
            }
        }
        _ => {}
    }
    state.store(0, Ordering::Release);
    let _ = wait; // The cooperative scheduler has no atomic set-and-wait handoff yet.
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

static mut MULTIWAIT_TEST_MUTANT_A:DispatcherHeader=DispatcherHeader{kind:MUTANT_OBJECT,absolute:0,size:14,inserted:0,signal_state:1,wait_list_flink:0,wait_list_blink:0};
static mut MULTIWAIT_BLOCKING_TEST_MUTANT:DispatcherHeader=DispatcherHeader{kind:MUTANT_OBJECT,absolute:0,size:14,inserted:0,signal_state:1,wait_list_flink:0,wait_list_blink:0};
static mut MULTIWAIT_BLOCKING_TEST_STACK:[u8;8192]=[0;8192];
static MULTIWAIT_BLOCKING_TEST_STARTED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static MULTIWAIT_BLOCKING_TEST_STATUS:core::sync::atomic::AtomicI32=core::sync::atomic::AtomicI32::new(STATUS_NOT_IMPLEMENTED);
extern "C" fn multiwait_blocking_mutant_waiter(_argument:*mut core::ffi::c_void){
    MULTIWAIT_BLOCKING_TEST_STARTED.store(true,Ordering::Release);
    let mutant=unsafe{core::ptr::addr_of_mut!(MULTIWAIT_BLOCKING_TEST_MUTANT)};
    let objects=[mutant];
    let status=unsafe{ke_wait_for_multiple_objects(1,objects.as_ptr(),0,0,0,0,core::ptr::null(),core::ptr::null_mut())};
    MULTIWAIT_BLOCKING_TEST_STATUS.store(status,Ordering::Release);
    if status==STATUS_SUCCESS{let _=unsafe{ke_release_mutant(mutant,0,0,0)};}
}
static mut MULTIWAIT_TEST_MUTANT_B:DispatcherHeader=DispatcherHeader{kind:MUTANT_OBJECT,absolute:0,size:14,inserted:0,signal_state:1,wait_list_flink:0,wait_list_blink:0};
static mut MULTIWAIT_TEST_MUTANT_C:DispatcherHeader=DispatcherHeader{kind:MUTANT_OBJECT,absolute:0,size:14,inserted:0,signal_state:1,wait_list_flink:0,wait_list_blink:0};
static mut MULTIWAIT_TEST_QUEUE:core::mem::MaybeUninit<ExecutiveQueue>=core::mem::MaybeUninit::uninit();
static mut MULTIWAIT_TEST_QUEUE_ENTRY:ListEntry=ListEntry{flink:core::ptr::null_mut(),blink:core::ptr::null_mut()};

/// Exercise the real kernel wait path from a scheduled thread: abandoned
/// wait-any mutant status, atomic immediate wait-all acquisition, and queue
/// entry transfer to the waiting thread.
pub(crate) fn dispatcher_extended_self_test()->bool{
    if crate::thread::current_thread_id().is_none(){return false}
    let a=unsafe{core::ptr::addr_of_mut!(MULTIWAIT_TEST_MUTANT_A)};
    let b=unsafe{core::ptr::addr_of_mut!(MULTIWAIT_TEST_MUTANT_B)};
    let c=unsafe{core::ptr::addr_of_mut!(MULTIWAIT_TEST_MUTANT_C)};
    unsafe{ke_initialize_mutant(a,0);ke_initialize_mutant(b,0);ke_initialize_mutant(c,0);}
    {
        let mut table=MUTANTS.lock();
        let Some(record)=table.records.iter_mut().find(|r|r.object==a as usize) else{return false};
        record.abandoned=true;
    }
    let one=[a];
    let status=unsafe{ke_wait_for_multiple_objects(1,one.as_ptr(),1,0,0,0,core::ptr::null(),core::ptr::null_mut())};
    if status!=STATUS_ABANDONED_WAIT_0{return false}
    let _=unsafe{ke_release_mutant(a,0,0,0)};
    let both=[b,c];
    if unsafe{ke_wait_for_multiple_objects(2,both.as_ptr(),0,0,0,0,core::ptr::null(),core::ptr::null_mut())}!=STATUS_SUCCESS{return false}
    let _=unsafe{ke_release_mutant(b,0,0,0)};
    let _=unsafe{ke_release_mutant(c,0,0,0)};
    if unsafe{ke_read_state_mutant(b)}!=1||unsafe{ke_read_state_mutant(c)}!=1{return false}

    let blocking_mutant=unsafe{core::ptr::addr_of_mut!(MULTIWAIT_BLOCKING_TEST_MUTANT)};
    unsafe{ke_initialize_mutant(blocking_mutant,1);}
    MULTIWAIT_BLOCKING_TEST_STARTED.store(false,Ordering::Release);
    MULTIWAIT_BLOCKING_TEST_STATUS.store(STATUS_NOT_IMPLEMENTED,Ordering::Release);
    let cpu=match crate::thread::current_thread_id(){Some(_)=>crate::hal::current_cpu_local().map(|l|l.logical_id.load(Ordering::Acquire) as usize).unwrap_or(0),None=>return false};
    let stack=unsafe{core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(MULTIWAIT_BLOCKING_TEST_STACK).cast::<u8>(),8192)};
    let waiter=match crate::thread::spawn(stack,1u64<<cpu,multiwait_blocking_mutant_waiter,core::ptr::null_mut()){Ok(id)=>id,Err(_)=>return false};
    let mut spins=0usize;
    while spins<10000&&crate::thread::state(waiter)!=Some(crate::thread::ThreadState::Blocked){
        let _=crate::thread::yield_current();spins+=1;
    }
    if crate::thread::state(waiter)!=Some(crate::thread::ThreadState::Blocked){
        let _=unsafe{ke_release_mutant(blocking_mutant,0,0,0)};return false
    }
    let _=unsafe{ke_release_mutant(blocking_mutant,0,0,0)};
    spins=0;
    while spins<10000&&crate::thread::state(waiter)!=Some(crate::thread::ThreadState::Exited){
        let _=crate::thread::yield_current();spins+=1;
    }
    if crate::thread::state(waiter)!=Some(crate::thread::ThreadState::Exited)
        ||MULTIWAIT_BLOCKING_TEST_STATUS.load(Ordering::Acquire)!=STATUS_SUCCESS{return false}

    let queue=unsafe{core::ptr::addr_of_mut!(MULTIWAIT_TEST_QUEUE).cast::<ExecutiveQueue>()};
    unsafe{ke_initialize_queue(queue,1);}
    let entry=unsafe{core::ptr::addr_of_mut!(MULTIWAIT_TEST_QUEUE_ENTRY)};
    let _=unsafe{ke_insert_queue(queue,entry)};
    let queue_object=queue.cast::<DispatcherHeader>();
    let one_queue=[queue_object];
    if unsafe{ke_wait_for_multiple_objects(1,one_queue.as_ptr(),1,0,0,0,core::ptr::null(),core::ptr::null_mut())}!=0{return false}
    crate::thread::take_queue_entry()==Some(entry as u64)
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
        timer_tick(crate::hal::query_performance_counter());
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
unsafe fn try_take_queue_entry_locked(object:*mut DispatcherHeader,thread_id:u64)->bool{
    if object.is_null()||unsafe{(*object).kind}!=QUEUE_OBJECT{return false}
    let queue=object.cast::<ExecutiveQueue>();
    let head=unsafe{core::ptr::addr_of_mut!((*queue).entry_list_head)};
    let entry=unsafe{list_remove_head(head)};
    if entry.is_null(){return false}
    unsafe{
        (&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>())
            .fetch_sub(1,Ordering::AcqRel);
    }
    let _=crate::thread::set_queue_entry(thread_id,entry as u64);
    true
}

fn wait_all_object_signalled(thread_id:u64,object_id:u64)->bool{
    let object=object_id as *mut DispatcherHeader;
    if object.is_null(){return false}
    let kind=unsafe{(*object).kind};
    if kind==MUTANT_OBJECT{
        let table=MUTANTS.lock();
        return table.records.iter().find(|r|r.object==object as usize)
            .is_some_and(|r|r.owner==0||r.owner==thread_id);
    }
    if !matches!(kind,EVENT_NOTIFICATION|EVENT_SYNCHRONIZATION|5|TIMER_NOTIFICATION|TIMER_SYNCHRONIZATION){return false}
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)>0}
}
fn consume_wait_all_object(thread_id:u64,object_id:u64)->i32{
    let object=object_id as *mut DispatcherHeader;
    if object.is_null(){return STATUS_INVALID_PARAMETER}
    if unsafe{(*object).kind}==MUTANT_OBJECT{
        let mut table=MUTANTS.lock();
        let Some(record)=table.records.iter_mut().find(|r|r.object==object as usize) else{return STATUS_INVALID_PARAMETER};
        if record.owner==thread_id{record.recursion=record.recursion.saturating_add(1);}
        else{record.owner=thread_id;record.recursion=1;}
        let abandoned=record.abandoned;record.abandoned=false;
        unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1-record.recursion as i32,Ordering::Release);}
        return if abandoned{0x80}else{STATUS_SUCCESS};
    }
    // The caller holds DISPATCHER_OBJECT_LOCK. Notification objects remain
    // signalled; synchronization events and semaphores are consumed once.
    let _=unsafe{try_consume_dispatcher_object_locked(object)};
    STATUS_SUCCESS
}
fn complete_ready_wait_all_groups(){
    let mut ready=[crate::dispatcher::WaitCompletion::EMPTY;crate::dispatcher::MAX_WAIT_GROUPS];
    let count=crate::dispatcher::complete_ready_wait_all(&mut ready,wait_all_object_signalled,consume_wait_all_object);
    for completion in ready[..count].iter().copied(){let _=crate::thread::complete_wait(completion.thread,completion.status);}
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
    if alertable != 0 && has_pending_user_apc(thread_id) { let _=crate::thread::set_user_apc_delivery_pending(thread_id,true); return STATUS_USER_APC; }
    if unsafe{(*object).kind}==MUTANT_OBJECT{
        if let Some(status)=try_acquire_mutant_status(object){return status}
    }else if unsafe { try_consume_dispatcher_object(object) } { return STATUS_SUCCESS; }
    let deadline = if timeout.is_null() { 0 } else {
        let interval = unsafe { timeout.read_unaligned() };
        if interval == 0 { return STATUS_TIMEOUT; }
        if !crate::hal::local_clock_event_active() { return STATUS_NOT_IMPLEMENTED; }
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
    if alertable != 0 && has_pending_user_apc(thread_id) {
        let _=crate::thread::set_user_apc_delivery_pending(thread_id,true);
        let _=crate::dispatcher::cancel(token);let _=crate::thread::clear_wake_pending();let _=crate::thread::set_wait_alertable(false);
        return STATUS_USER_APC;
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
    if alertable != 0 { if let Some(id)=crate::thread::current_thread_id() { if has_pending_user_apc(id) { let _=crate::thread::set_user_apc_delivery_pending(id,true); return STATUS_USER_APC; } } }
    let object_list = unsafe { core::slice::from_raw_parts(objects, count as usize) };
    if object_list.iter().any(|object| object.is_null()) { return STATUS_INVALID_PARAMETER; }
    for i in 0..object_list.len() {
        let kind = unsafe { (*object_list[i]).kind };
        if !matches!(kind, EVENT_NOTIFICATION | EVENT_SYNCHRONIZATION | 5 | TIMER_NOTIFICATION | TIMER_SYNCHRONIZATION | MUTANT_OBJECT | QUEUE_OBJECT) {
            return STATUS_NOT_IMPLEMENTED;
        }
        // Queue entries can be transferred atomically for wait-any. Wait-all
        // cannot reserve a queue entry alongside other objects in this ABI yet.
        if kind==QUEUE_OBJECT && wait_type==0{return STATUS_NOT_IMPLEMENTED}
        if object_list[..i].iter().any(|previous| *previous == object_list[i]) {
            return STATUS_INVALID_PARAMETER;
        }
    }
    let current_id = crate::thread::current_thread_id();
    let guard = DISPATCHER_OBJECT_LOCK.lock();
    if wait_type == 1 {
        for (index, object) in object_list.iter().copied().enumerate() {
            let kind=unsafe{(*object).kind};
            if kind==MUTANT_OBJECT {
                if let Some(status)=try_acquire_mutant_status(object){
                    return if status==STATUS_ABANDONED_WAIT_0{STATUS_ABANDONED_WAIT_0+index as i32}else{index as i32};
                }
            } else if kind==QUEUE_OBJECT {
                if current_id.is_some_and(|id|unsafe{try_take_queue_entry_locked(object,id)}) {return index as i32;}
            } else if unsafe { try_consume_dispatcher_object_locked(object) } {
                return index as i32;
            }
        }
    } else {
        // Mutants are available when unowned or recursively owned by this
        // thread. Queue objects are excluded above because entry transfer is
        // consumptive and cannot participate in wait-all reservation.
        let mut mutants=MUTANTS.lock();
        let all_signalled = current_id.is_some_and(|id|object_list.iter().copied().all(|object|{
            if unsafe{(*object).kind}==MUTANT_OBJECT{
                mutants.records.iter().find(|r|r.object==object as usize).is_some_and(|r|r.owner==0||r.owner==id)
            }else{unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)>0}}
        }));
        if all_signalled {
            let id=current_id.unwrap_or(0);
            let mut abandoned_index=None;
            for (index,object) in object_list.iter().copied().enumerate(){
                if unsafe{(*object).kind}==MUTANT_OBJECT{
                    if let Some(record)=mutants.records.iter_mut().find(|r|r.object==object as usize){
                        if record.owner==id{record.recursion=record.recursion.saturating_add(1);}
                        else{record.owner=id;record.recursion=1;}
                        if record.abandoned{if abandoned_index.is_none(){abandoned_index=Some(index);}record.abandoned=false;}
                        unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1-record.recursion as i32,Ordering::Release);}
                    }
                }else{consume_wait_all_object(object as u64);}
            }
            return abandoned_index.map_or(STATUS_SUCCESS,|index|STATUS_ABANDONED_WAIT_0+index as i32);
        }
        drop(mutants);
    }
    if !timeout.is_null() && unsafe { timeout.read_unaligned() } == 0 { return STATUS_TIMEOUT; }
    let Some(thread_id) = current_id else { return STATUS_NOT_IMPLEMENTED; };
    let deadline = if timeout.is_null() { 0 } else {
        let interval = unsafe { timeout.read_unaligned() };
        if !crate::hal::local_clock_event_active() { return STATUS_NOT_IMPLEMENTED; }
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
        if alertable!=0 && has_pending_user_apc(thread_id){
            let _=crate::thread::set_user_apc_delivery_pending(thread_id,true);crate::dispatcher::clear_wait_group(thread_id);
            if let Some(token)=timeout_token{let _=crate::dispatcher::cancel(token);}
            let _=crate::thread::clear_wake_pending();let _=crate::thread::set_wait_alertable(false);return STATUS_USER_APC;
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
    if alertable!=0 && has_pending_user_apc(thread_id){
        let _=crate::thread::set_user_apc_delivery_pending(thread_id,true);crate::dispatcher::clear_wait_group(thread_id);
        for token in tokens[..registered].iter().copied(){let _=crate::dispatcher::cancel(token);}
        let _=crate::thread::clear_wake_pending();let _=crate::thread::set_wait_alertable(false);return STATUS_USER_APC;
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


// interrupt synchronization, scheduler, delay, and guarded mutex helpers.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

/// Interrupt-object synchronization side table. Until HAL interrupt objects expose
/// their real SynchronizeIrql and ISR lock, this uses a conservative DPC-level
/// critical section; it serializes RdotOS callers but is not a Windows DIRQL clone.
const MAX_INTERRUPT_LOCKS:usize=128;
struct InterruptLockSlot{object:core::sync::atomic::AtomicUsize,word:core::sync::atomic::AtomicU64}
impl InterruptLockSlot{const fn new()->Self{Self{object:core::sync::atomic::AtomicUsize::new(0),word:core::sync::atomic::AtomicU64::new(0)}}}
static INTERRUPT_LOCKS:[InterruptLockSlot;MAX_INTERRUPT_LOCKS]=[const{InterruptLockSlot::new()};MAX_INTERRUPT_LOCKS];
static INTERRUPT_LOCK_REGISTRY:crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
fn interrupt_lock_slot(interrupt:*mut core::ffi::c_void)->Option<usize>{
 if interrupt.is_null(){return None}
 let _guard=INTERRUPT_LOCK_REGISTRY.lock();
 if let Some(i)=INTERRUPT_LOCKS.iter().position(|s|s.object.load(Ordering::Acquire)==interrupt as usize){return Some(i)}
 let i=INTERRUPT_LOCKS.iter().position(|s|s.object.load(Ordering::Acquire)==0)?;
 INTERRUPT_LOCKS[i].object.store(interrupt as usize,Ordering::Release);Some(i)
}
fn acquire_interrupt_lock(interrupt:*mut core::ffi::c_void)->Option<u8>{
 let i=interrupt_lock_slot(interrupt)?;let old=unsafe{ke_raise_irql_to_dpc_level()};let word=&INTERRUPT_LOCKS[i].word;
 while word.compare_exchange_weak(0,1,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}
 Some(old)
}
pub unsafe extern "win64" fn ke_acquire_interrupt_spin_lock(interrupt:*mut core::ffi::c_void)->u8{
 acquire_interrupt_lock(interrupt).unwrap_or_else(||unsafe{ke_get_current_irql()})
}
pub unsafe extern "win64" fn ke_release_interrupt_spin_lock(interrupt:*mut core::ffi::c_void,old_irql:u8){
 if let Some(i)=interrupt_lock_slot(interrupt){INTERRUPT_LOCKS[i].word.store(0,Ordering::Release);unsafe{ke_lower_irql(old_irql);}}
}
pub type SynchronizeRoutine=unsafe extern "win64" fn(*mut core::ffi::c_void)->u8;
pub unsafe extern "win64" fn ke_synchronize_execution(interrupt:*mut core::ffi::c_void,routine:Option<SynchronizeRoutine>,context:*mut core::ffi::c_void)->u8{
 let Some(routine)=routine else{return 0};
 let Some(old)=acquire_interrupt_lock(interrupt) else{return 0};
 let result=unsafe{routine(context)};
 unsafe{ke_release_interrupt_spin_lock(interrupt,old);}
 result
}

/// Precise monotonic time; suspend/resume bias is not modelled yet.
pub unsafe extern "win64" fn ke_query_unbiased_interrupt_time_precise(qpc_timestamp:*mut u64)->u64{
 if !qpc_timestamp.is_null(){unsafe{qpc_timestamp.write(crate::apic::monotonic_ticks());}}
 interrupt_time_100ns()
}
/// Clock resolution in 100-ns units. Until dynamic clock programming is wired,
/// all three values report the configured 1-ms quantum.
pub unsafe extern "win64" fn ke_get_clock_timer_resolution(maximum:*mut u32,minimum:*mut u32,current:*mut u32){
 for p in [maximum,minimum,current]{if !p.is_null(){unsafe{p.write(10_000);}}}
}
pub unsafe extern "win64" fn ke_get_next_clock_tick_duration()->u32{
 let hz=crate::apic::tsc_frequency_hz().unwrap_or(0);if hz==0{return 0}
 let now=crate::apic::monotonic_ticks();
 let earliest={let table=TIMERS.lock();table.records.iter().filter(|r|r.active).map(|r|r.due).min()};
 let Some(deadline)=earliest else{return 10_000};
 ((deadline.saturating_sub(now) as u128*10_000_000u128/hz as u128).min(u32::MAX as u128)) as u32
}
/// x86-64 coherent-DMA fallback: enforce ordering; non-coherent cache maintenance
/// requires a platform DMA coherency operation and is not claimed here.
pub unsafe extern "win64" fn ke_flush_io_buffers(_mdl:*mut core::ffi::c_void,read_operation:u8,_dma_operation:u8){
 if read_operation!=0{core::sync::atomic::fence(Ordering::SeqCst);}
 unsafe{core::arch::asm!("mfence",options(nostack,preserves_flags));}
}
static BUGCHECK_CODE:core::sync::atomic::AtomicU32=core::sync::atomic::AtomicU32::new(0);
static BUGCHECK_PARAMETERS:[core::sync::atomic::AtomicU64;4]=[const{core::sync::atomic::AtomicU64::new(0)};4];
pub unsafe extern "win64" fn ke_bug_check(code:u32)->!{unsafe{ke_bug_check_ex(code,0,0,0,0)}}
pub unsafe extern "win64" fn ke_bug_check_ex(code:u32,p1:usize,p2:usize,p3:usize,p4:usize)->!{
 BUGCHECK_PARAMETERS[0].store(p1 as u64,Ordering::Release);BUGCHECK_PARAMETERS[1].store(p2 as u64,Ordering::Release);
 BUGCHECK_PARAMETERS[2].store(p3 as u64,Ordering::Release);BUGCHECK_PARAMETERS[3].store(p4 as u64,Ordering::Release);
 BUGCHECK_CODE.store(code,Ordering::Release);
 loop{unsafe{core::arch::asm!("cli","hlt",options(nomem,nostack));}}
}


/// Limited scheduler priority metadata, keyed by opaque RdotOS thread tokens.
/// Valid Windows priorities are 0..31; quantum boosts and scheduler reordering
/// are not yet represented by the cooperative scheduler.
const MAX_THREAD_PRIORITIES:usize=128;
#[derive(Clone,Copy)] struct ThreadPriorityRecord{id:u64,base:i8,current:i8}
impl ThreadPriorityRecord{const EMPTY:Self=Self{id:0,base:8,current:8};}
struct ThreadPriorityTable{records:[ThreadPriorityRecord;MAX_THREAD_PRIORITIES]}
impl ThreadPriorityTable{const fn new()->Self{Self{records:[ThreadPriorityRecord::EMPTY;MAX_THREAD_PRIORITIES]}}}
static THREAD_PRIORITIES:crate::hal::IrqSpinLock<ThreadPriorityTable>=crate::hal::IrqSpinLock::new(ThreadPriorityTable::new());
fn priority_id(thread:*mut core::ffi::c_void)->Option<u64>{if thread.is_null(){crate::thread::current_thread_id()}else{crate::thread::thread_id_from_object(thread)}}
pub unsafe extern "win64" fn ke_query_priority_thread(thread:*mut core::ffi::c_void)->i8{
 let Some(id)=priority_id(thread) else{return 8};let table=THREAD_PRIORITIES.lock();
 table.records.iter().find(|r|r.id==id).map(|r|r.current).unwrap_or(8)
}
pub unsafe extern "win64" fn ke_set_priority_thread(thread:*mut core::ffi::c_void,priority:i8)->i8{
 let Some(id)=priority_id(thread) else{return 8};
 if !(0..=31).contains(&priority){return unsafe{ke_query_priority_thread(thread)}}
 let old={
  let mut table=THREAD_PRIORITIES.lock();
  let Some(slot)=table.records.iter().position(|r|r.id==id).or_else(||table.records.iter().position(|r|r.id==0)) else{return 8};
  let old=if table.records[slot].id==id{table.records[slot].current}else{8};
  let base=if table.records[slot].id==id{table.records[slot].base}else{priority};
  table.records[slot]=ThreadPriorityRecord{id,base,current:priority};old
 };
 let _=crate::thread::set_scheduler_priority(id,(priority as u8/4).min(7));
 old
}
/// Apply a dispatcher wake boost to dynamic priority and scheduler metadata.
fn boost_thread_priority(id:u64,increment:i32){
 if increment<=0{return}
 let current={
  let mut table=THREAD_PRIORITIES.lock();
  let slot=table.records.iter().position(|r|r.id==id).or_else(||table.records.iter().position(|r|r.id==0));
  if let Some(slot)=slot{
   let record=table.records[slot];
   let base=if record.id==id{record.base}else{8};
   let old=if record.id==id{record.current}else{8};
   let current=((old as i32)+increment).clamp(0,31) as i8;
   table.records[slot]=ThreadPriorityRecord{id,base,current};
   Some(current)
  }else{None}
 };
 if let Some(current)=current{let _=crate::thread::set_scheduler_priority(id,(current as u8/4).min(7));}
}
pub unsafe extern "win64" fn ke_set_base_priority_thread(thread:*mut core::ffi::c_void,increment:i32)->i32{
 let Some(id)=priority_id(thread) else{return 0};
 let (old,base)={
  let mut table=THREAD_PRIORITIES.lock();
  let Some(slot)=table.records.iter().position(|r|r.id==id) else{return 0};
  let old=table.records[slot].base as i32;
  let base=(old+increment).clamp(0,31) as i8;
  table.records[slot].base=base;table.records[slot].current=base;
  (old,base)
 };
 let _=crate::thread::set_scheduler_priority(id,(base as u8/4).min(7));
 old
}

/// Never busy-loops when the thread runtime or calibrated delay source is absent.
pub unsafe extern "win64" fn ke_delay_execution_thread(
    wait_mode: i32,
    alertable: u8,
    interval: *const i64,
) -> i32 {
    if interval.is_null() { return STATUS_INVALID_PARAMETER; }
    let value = unsafe { interval.read_unaligned() };
    if value == 0 { return STATUS_SUCCESS; }
    let Some(thread_id) = crate::thread::current_thread_id() else { return STATUS_NOT_IMPLEMENTED; };
    if alertable != 0 && crate::thread::take_alert_pending() { return STATUS_ALERTED; }
    if !crate::apic::local_timer_active() { return STATUS_NOT_IMPLEMENTED; }
    let deadline = match timer_deadline(value) {
        Some(value) => value,
        None => return STATUS_NOT_IMPLEMENTED,
    };
    let _ = wait_mode;
    let _ = crate::thread::set_wait_alertable(alertable != 0);
    let token = match crate::dispatcher::enqueue(crate::dispatcher::delay_object(thread_id), thread_id, deadline) {
        Ok(value) => value,
        Err(_) => { let _=crate::thread::set_wait_alertable(false); return STATUS_NOT_IMPLEMENTED; },
    };
    if alertable != 0 && crate::thread::take_alert_pending() {
        let _=crate::dispatcher::cancel(token);let _=crate::thread::set_wait_alertable(false);return STATUS_ALERTED;
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

pub unsafe extern "win64" fn ke_initialize_guarded_mutex(object:*mut DispatcherHeader){unsafe{initialize_mutant(object,0)}}
pub unsafe extern "win64" fn ke_acquire_guarded_mutex(object:*mut DispatcherHeader){
    unsafe{ke_enter_guarded_region();}
    let status=unsafe{ke_wait_for_single_object(object,0,0,0,core::ptr::null())};
    if status!=STATUS_SUCCESS{unsafe{ke_leave_guarded_region();}}
}
pub unsafe extern "win64" fn ke_release_guarded_mutex(object:*mut DispatcherHeader){
    let _=release_mutant(object,false);unsafe{ke_leave_guarded_region();}
}
pub unsafe extern "win64" fn ke_try_to_acquire_guarded_mutex(object:*mut DispatcherHeader)->u8{
    unsafe{ke_enter_guarded_region();}
    if unsafe{try_consume_dispatcher_object(object)}{1}else{unsafe{ke_leave_guarded_region();}0}
}
pub unsafe extern "win64" fn ke_acquire_guarded_mutex_unsafe(object:*mut DispatcherHeader){
    let _=unsafe{ke_wait_for_single_object(object,0,0,0,core::ptr::null())};
}
pub unsafe extern "win64" fn ke_release_guarded_mutex_unsafe(object:*mut DispatcherHeader){
    let _=release_mutant(object,false);
}



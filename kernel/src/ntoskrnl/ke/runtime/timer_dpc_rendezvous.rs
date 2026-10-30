// timer, DPC, and cross-processor rendezvous routines.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

// ---- Timer and DPC executive ------------------------------------------------
// Driver-owned timer/DPC object storage is opaque to this implementation. The
// side tables track only lifecycle metadata, avoiding assumptions about private
// Windows LIST_ENTRY/KDPC internals while preserving the public DDI signatures.
const MAX_TIMER_OBJECTS: usize = 128;
const MAX_DPC_OBJECTS: usize = 128;
const TIMER_NOTIFICATION: u8 = 8;
const TIMER_SYNCHRONIZATION: u8 = 9;
pub type DpcRoutine = unsafe extern "win64" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void);
pub type Timer2Routine = unsafe extern "win64" fn(*mut core::ffi::c_void, *mut core::ffi::c_void);

/// One metadata table owns both classic KTIMER and KTIMER2 deadlines. The
/// `timer2` discriminator changes expiry delivery, not the scheduling engine.
#[derive(Clone, Copy)]
struct TimerRecord {
    object: usize,
    active: bool,
    due: u64,
    period: u64,
    dpc: usize,
    callback: usize,
    context: usize,
    tolerable_delay: u32,
    timer2: bool,
    callback_running: bool,
    generation: u64,
}
impl TimerRecord {
    const EMPTY: Self = Self {
        object: 0, active: false, due: 0, period: 0, dpc: 0,
        callback: 0, context: 0, tolerable_delay: 0, timer2: false,
        callback_running: false, generation: 0,
    };
}
struct TimerTable { records: [TimerRecord; MAX_TIMER_OBJECTS] }
impl TimerTable {
    const fn new() -> Self { Self { records: [TimerRecord::EMPTY; MAX_TIMER_OBJECTS] } }
}
static TIMERS: crate::hal::IrqSpinLock<TimerTable> = crate::hal::IrqSpinLock::new(TimerTable::new());
#[derive(Clone, Copy)]
struct DpcRecord { object: usize, routine: usize, context: usize, arg1: usize, arg2: usize, target_cpu: usize, importance:u8, threaded: bool, active: bool, queued: bool, running: bool }
impl DpcRecord { const EMPTY: Self = Self { object:0,routine:0,context:0,arg1:0,arg2:0,target_cpu:0,importance:1,threaded:false,active:false,queued:false,running:false }; }
struct DpcTable { records:[DpcRecord;MAX_DPC_OBJECTS] }
impl DpcTable { const fn new()->Self { Self { records:[DpcRecord::EMPTY;MAX_DPC_OBJECTS] } } }
static DPCS: crate::hal::IrqSpinLock<DpcTable> = crate::hal::IrqSpinLock::new(DpcTable::new());
static DPC_IN_FLIGHT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static DPC_DISPATCHING: [core::sync::atomic::AtomicBool; 64] =
    [const { core::sync::atomic::AtomicBool::new(false) };64];
static THREADED_DPC_DISPATCHING: [core::sync::atomic::AtomicBool; 64] =
    [const { core::sync::atomic::AtomicBool::new(false) };64];

// ---- Cross-processor call rendezvous -----------------------------------------
// A dedicated IPI vector isolates Ke broadcasts from scheduler notifications.
// The request is armed only after every target IPI has been sent, so early
// recipients wait in the interrupt handler rather than seeing partial state.
const BROADCAST_IDLE:u8=0;
const BROADCAST_PREPARING:u8=1;
const BROADCAST_ARMED:u8=2;
const BROADCAST_ABORTED:u8=3;
const BROADCAST_IPI_CALL:u8=1;
const BROADCAST_DPC_CALL:u8=2;
const IPI_LEVEL:u8=12;
static BROADCAST_LOCK:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static BROADCAST_STATE:core::sync::atomic::AtomicU8=core::sync::atomic::AtomicU8::new(BROADCAST_IDLE);
static BROADCAST_MODE:core::sync::atomic::AtomicU8=core::sync::atomic::AtomicU8::new(0);
static BROADCAST_EPOCH:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BROADCAST_LAST_EPOCH:[core::sync::atomic::AtomicU64;64]=[const{core::sync::atomic::AtomicU64::new(0)};64];
static BROADCAST_TARGETS:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BROADCAST_SENT_MASK:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BROADCAST_ABORT_ACK_MASK:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BROADCAST_DISABLED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static BROADCAST_EXPECTED:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static BROADCAST_COMPLETED:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static BROADCAST_CALLBACK:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static BROADCAST_CONTEXT:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static DPC_SYNC_ARRIVED:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static DPC_DONE_ARRIVED:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
#[repr(C,align(16))]
struct BroadcastDpcStorage([u64;8]);
static BROADCAST_DPC:BroadcastDpcStorage=BroadcastDpcStorage([0;8]);

pub type IpiGenericRoutine=unsafe extern "win64" fn(usize)->usize;

static BROADCAST_TEST_COUNTS:[core::sync::atomic::AtomicU32;64]=[const{core::sync::atomic::AtomicU32::new(0)};64];
static BROADCAST_TEST_SYNC_OK:core::sync::atomic::AtomicU32=core::sync::atomic::AtomicU32::new(0);
static BROADCAST_TEST_DONE_RETURNED:core::sync::atomic::AtomicU32=core::sync::atomic::AtomicU32::new(0);
unsafe extern "win64" fn broadcast_test_ipi(_context:usize)->usize{
    let cpu=cpu_slot();
    if cpu<64{(BROADCAST_TEST_COUNTS[cpu].fetch_add(1,Ordering::AcqRel)+1) as usize}else{0}
}
unsafe extern "win64" fn broadcast_test_dpc(
    _dpc:*mut core::ffi::c_void,_context:*mut core::ffi::c_void,
    done:*mut core::ffi::c_void,sync:*mut core::ffi::c_void,
){
    let cpu=cpu_slot();
    if cpu<64{BROADCAST_TEST_COUNTS[cpu].fetch_add(1,Ordering::AcqRel);}
    let synchronized=unsafe{ke_signal_call_dpc_synchronize(sync)};
    if synchronized!=0{
        BROADCAST_TEST_SYNC_OK.fetch_add(1,Ordering::AcqRel);
        unsafe{ke_signal_call_dpc_done(done);}
        BROADCAST_TEST_DONE_RETURNED.fetch_add(1,Ordering::AcqRel);
    }
}
/// Exercise both broadcast paths on the actual online CPU set. This is called
/// only after the HAL IPI path is enabled and the boot-time SMP stress is ready.
pub fn broadcast_rendezvous_self_test()->bool{
    let Some(mask)=online_broadcast_mask() else{return false};
    for cpu in 0..64{BROADCAST_TEST_COUNTS[cpu].store(0,Ordering::Release);}
    let result=unsafe{ke_ipi_generic_call(Some(broadcast_test_ipi),0)};
    if result==0{return false}
    for cpu in 0..64{
        if mask&(1u64<<cpu)!=0&&BROADCAST_TEST_COUNTS[cpu].load(Ordering::Acquire)!=1{return false}
    }
    for cpu in 0..64{BROADCAST_TEST_COUNTS[cpu].store(0,Ordering::Release);}
    BROADCAST_TEST_SYNC_OK.store(0,Ordering::Release);
    BROADCAST_TEST_DONE_RETURNED.store(0,Ordering::Release);
    unsafe{ke_generic_call_dpc(Some(broadcast_test_dpc),core::ptr::null_mut());}
    let expected=mask.count_ones();
    if BROADCAST_TEST_SYNC_OK.load(Ordering::Acquire)!=expected||BROADCAST_TEST_DONE_RETURNED.load(Ordering::Acquire)!=expected{return false}
    for cpu in 0..64{
        if mask&(1u64<<cpu)!=0&&BROADCAST_TEST_COUNTS[cpu].load(Ordering::Acquire)!=1{return false}
    }
    true
}


fn acquire_broadcast_lock(){
    while BROADCAST_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){
        core::hint::spin_loop();
    }
}
fn release_broadcast_lock(){BROADCAST_LOCK.store(false,Ordering::Release);}

fn online_broadcast_mask()->Option<u64>{
    let mut mask=0u64;
    for cpu in 0..crate::hal::MAX_CPUS.min(64){
        if crate::hal::cpu_state(cpu).ok()==Some(crate::hal::CPU_ONLINE){
            let local=crate::hal::cpu_local(cpu).ok()?;
            if cpu!=cpu_slot() && local.apic_id.load(Ordering::Acquire)>u8::MAX as u32{return None}
            mask|=1u64<<cpu;
        }
    }
    if mask==0 { mask=1u64<<cpu_slot(); }
    Some(mask)
}

/// Send one rendezvous IPI to each online processor other than the caller.
/// Early recipients wait in rdotos_ke_broadcast_ipi_handler until armed.
fn begin_broadcast(mode:u8,callback:usize,context:usize)->Option<(u64,u64)>{
    acquire_broadcast_lock();
    if BROADCAST_DISABLED.load(Ordering::Acquire){release_broadcast_lock();return None;}
    if crate::hal::online_cpu_count()>1&&!crate::smp::ipi_shootdowns_enabled(){
        release_broadcast_lock();return None;
    }
    let Some(mask)=online_broadcast_mask() else{release_broadcast_lock();return None};
    let current=cpu_slot();
    let epoch=BROADCAST_EPOCH.fetch_add(1,Ordering::AcqRel).wrapping_add(1).max(1);
    BROADCAST_MODE.store(mode,Ordering::Relaxed);
    BROADCAST_CALLBACK.store(callback,Ordering::Relaxed);
    BROADCAST_CONTEXT.store(context,Ordering::Relaxed);
    BROADCAST_TARGETS.store(mask,Ordering::Relaxed);
    BROADCAST_EXPECTED.store(mask.count_ones() as usize,Ordering::Relaxed);
    BROADCAST_COMPLETED.store(0,Ordering::Relaxed);
    BROADCAST_SENT_MASK.store(0,Ordering::Relaxed);
    BROADCAST_ABORT_ACK_MASK.store(0,Ordering::Relaxed);
    for cpu in 0..crate::hal::MAX_CPUS.min(64){if mask&(1u64<<cpu)!=0{BROADCAST_LAST_EPOCH[cpu].store(epoch.wrapping_sub(1),Ordering::Release);}}
    DPC_SYNC_ARRIVED.store(0,Ordering::Relaxed);
    DPC_DONE_ARRIVED.store(0,Ordering::Relaxed);
    BROADCAST_STATE.store(BROADCAST_PREPARING,Ordering::Release);
    for cpu in 0..crate::hal::MAX_CPUS.min(64){
        if cpu==current||mask&(1u64<<cpu)==0{continue}
        BROADCAST_SENT_MASK.fetch_or(1u64<<cpu,Ordering::AcqRel);
        if !crate::smp::request_ke_broadcast(cpu){
            BROADCAST_STATE.store(BROADCAST_ABORTED,Ordering::Release);
            let sent=BROADCAST_SENT_MASK.load(Ordering::Acquire);
            let mut spins=0usize;
            while BROADCAST_ABORT_ACK_MASK.load(Ordering::Acquire)&sent!=sent && spins<10_000_000{
                spins+=1;core::hint::spin_loop();
            }
            if BROADCAST_ABORT_ACK_MASK.load(Ordering::Acquire)&sent!=sent{
                // A late IPI must never be allowed to run a later broadcast.
                BROADCAST_DISABLED.store(true,Ordering::Release);
            }
            BROADCAST_STATE.store(BROADCAST_IDLE,Ordering::Release);
            release_broadcast_lock();return None;
        }
    }
    // The dedicated vector cannot be consumed by ordinary reschedule IPIs.
    BROADCAST_STATE.store(BROADCAST_ARMED,Ordering::Release);
    Some((mask,epoch))
}

fn execute_broadcast_callback(cpu:usize)->usize{
    let mode=BROADCAST_MODE.load(Ordering::Acquire);
    let callback=BROADCAST_CALLBACK.load(Ordering::Acquire);
    if callback==0{return 0}
    if mode==BROADCAST_IPI_CALL{
        let old=CURRENT_IRQL[cpu].swap(IPI_LEVEL,Ordering::AcqRel);
        let function:IpiGenericRoutine=unsafe{core::mem::transmute(callback)};
        let context=BROADCAST_CONTEXT.load(Ordering::Acquire);
        let result=unsafe{function(context)};
        CURRENT_IRQL[cpu].store(old,Ordering::Release);
        result
    }else if mode==BROADCAST_DPC_CALL{
        let old=CURRENT_IRQL[cpu].swap(DISPATCH_LEVEL,Ordering::AcqRel);
        let was_dispatching=DPC_DISPATCHING[cpu].swap(true,Ordering::AcqRel);
        let function:DpcRoutine=unsafe{core::mem::transmute(callback)};
        let context=BROADCAST_CONTEXT.load(Ordering::Acquire);
        let done_arg=core::ptr::addr_of!(DPC_DONE_ARRIVED) as *mut core::ffi::c_void;
        let sync_arg=core::ptr::addr_of!(DPC_SYNC_ARRIVED) as *mut core::ffi::c_void;
        unsafe{
            function(
                core::ptr::addr_of!(BROADCAST_DPC) as *mut core::ffi::c_void,
                context as *mut core::ffi::c_void,
                done_arg,
                sync_arg,
            );
        }
        DPC_DISPATCHING[cpu].store(was_dispatching,Ordering::Release);
        CURRENT_IRQL[cpu].store(old,Ordering::Release);
        0
    }else{0}
}

/// Called by the dedicated Ke broadcast-vector handler on the receiving processor. It deliberately
/// runs without a Rust lock held: callbacks may enter ordinary Ke routines.
pub fn handle_broadcast_ipi(){
    let cpu=cpu_slot();
    if cpu>=64{return}
    let mut state=BROADCAST_STATE.load(Ordering::Acquire);
    if state==BROADCAST_IDLE{return}
    if BROADCAST_TARGETS.load(Ordering::Acquire)&(1u64<<cpu)==0{return}
    if state==BROADCAST_PREPARING{
        while BROADCAST_STATE.load(Ordering::Acquire)==BROADCAST_PREPARING{core::hint::spin_loop();}
        state=BROADCAST_STATE.load(Ordering::Acquire);
    }
    if state==BROADCAST_ABORTED{
        if BROADCAST_SENT_MASK.load(Ordering::Acquire)&(1u64<<cpu)!=0{
            BROADCAST_ABORT_ACK_MASK.fetch_or(1u64<<cpu,Ordering::AcqRel);
        }
        return
    }
    if state!=BROADCAST_ARMED{return}
    let epoch=BROADCAST_EPOCH.load(Ordering::Acquire);
    if BROADCAST_LAST_EPOCH[cpu].compare_exchange(epoch.wrapping_sub(1),epoch,Ordering::AcqRel,Ordering::Acquire).is_err(){return}
    let _=execute_broadcast_callback(cpu);
    BROADCAST_COMPLETED.fetch_add(1,Ordering::AcqRel);
}

/// VOID KeIpiGenericCall(PKIPI_BROADCAST_WORKER Function, ULONG_PTR Context).
/// The return is the current processor's callback result; completion is not
/// reported until every online processor in the current HAL topology returns.
pub unsafe extern "win64" fn ke_ipi_generic_call(function:Option<IpiGenericRoutine>,context:usize)->usize{
    let Some(function)=function else{return 0};
    let Some((mask,_epoch))=begin_broadcast(BROADCAST_IPI_CALL,function as usize,context) else{return 0};
    let current=cpu_slot();
    BROADCAST_LAST_EPOCH[current].store(_epoch,Ordering::Release);
    let local_result=execute_broadcast_callback(current);
    BROADCAST_COMPLETED.fetch_add(1,Ordering::AcqRel);
    let expected=mask.count_ones() as usize;
    while BROADCAST_COMPLETED.load(Ordering::Acquire)<expected{core::hint::spin_loop();}
    BROADCAST_STATE.store(BROADCAST_IDLE,Ordering::Release);
    release_broadcast_lock();
    local_result
}

/// VOID KeGenericCallDpc(PKDEFERRED_ROUTINE Routine, PVOID Context).
/// One callback runs on every online CPU. The callback's SystemArgument1 and
/// SystemArgument2 point at the shared Done and Synchronize barriers.
pub unsafe extern "win64" fn ke_generic_call_dpc(routine:Option<DpcRoutine>,context:*mut core::ffi::c_void){
    let Some(routine)=routine else{return};
    let Some((mask,_epoch))=begin_broadcast(BROADCAST_DPC_CALL,routine as usize,context as usize) else{return};
    let current=cpu_slot();
    BROADCAST_LAST_EPOCH[current].store(_epoch,Ordering::Release);
    let _=execute_broadcast_callback(current);
    BROADCAST_COMPLETED.fetch_add(1,Ordering::AcqRel);
    let expected=mask.count_ones() as usize;
    while BROADCAST_COMPLETED.load(Ordering::Acquire)<expected{core::hint::spin_loop();}
    BROADCAST_STATE.store(BROADCAST_IDLE,Ordering::Release);
    release_broadcast_lock();
}

/// BOOLEAN KeSignalCallDpcSynchronize(PVOID SystemArgument2).
/// Each participant signals the rendezvous and waits for all participants.
pub unsafe extern "win64" fn ke_signal_call_dpc_synchronize(argument:*mut core::ffi::c_void)->u8{
    if argument!=core::ptr::addr_of!(DPC_SYNC_ARRIVED) as *mut core::ffi::c_void{return 0}
    DPC_SYNC_ARRIVED.fetch_add(1,Ordering::AcqRel);
    let expected=BROADCAST_EXPECTED.load(Ordering::Acquire);
    while DPC_SYNC_ARRIVED.load(Ordering::Acquire)<expected{core::hint::spin_loop();}
    1
}

/// VOID KeSignalCallDpcDone(PVOID SystemArgument1).
/// The barrier releases only after every callback has signaled completion.
pub unsafe extern "win64" fn ke_signal_call_dpc_done(argument:*mut core::ffi::c_void){
    if argument!=core::ptr::addr_of!(DPC_DONE_ARRIVED) as *mut core::ffi::c_void{return}
    DPC_DONE_ARRIVED.fetch_add(1,Ordering::AcqRel);
    let expected=BROADCAST_EXPECTED.load(Ordering::Acquire);
    while DPC_DONE_ARRIVED.load(Ordering::Acquire)<expected{core::hint::spin_loop();}
}


fn dpc_slot(object:usize)->Option<usize>{
    let table=DPCS.lock();
    table.records.iter().position(|r|r.active&&r.object==object)
}
fn timer_deadline(due:i64)->Option<u64>{
    if due==0{return Some(crate::hal::query_performance_counter())}
    if due<0{return crate::hal::relative_timeout_deadline(due)}
    let now=precise_system_time_100ns()? as i64;
    let delta=(due-now).max(0) as u64;
    let hz=crate::hal::query_performance_frequency_hz()?;
    let ticks=((delta as u128).checked_mul(hz as u128)?/10_000_000u128).min(u64::MAX as u128) as u64;
    crate::hal::query_performance_counter().checked_add(ticks)
}
unsafe fn initialize_timer(timer:*mut DispatcherHeader, timer_type:i32){
    if timer.is_null()||!(timer_type==0||timer_type==1){return}
    let kind=if timer_type==0{TIMER_NOTIFICATION}else{TIMER_SYNCHRONIZATION};
    unsafe{timer.write(DispatcherHeader{kind,absolute:0,size:16,inserted:0,signal_state:0,wait_list_flink:timer as u64+8,wait_list_blink:timer as u64+8});}
    let mut table=TIMERS.lock();
    let slot=table.records.iter().position(|r|r.object==timer as usize)
        .or_else(||table.records.iter().position(|r|r.object==0));
    if let Some(i)=slot {
        // The void initializer cannot report failure. Do not replace metadata
        // while a KTIMER2 callback using the same opaque address is in flight.
        if table.records[i].object == timer as usize && table.records[i].callback_running { return; }
        // Preserve monotonic identity across reinitialization so an old expiry
        // snapshot cannot become valid again after the same address is reused.
        let generation=next_timer_generation(table.records[i].generation);
        table.records[i]=TimerRecord{object:timer as usize,generation,..TimerRecord::EMPTY};
    }
}
pub unsafe extern "win64" fn ke_initialize_timer(timer:*mut DispatcherHeader){
    unsafe{initialize_timer(timer,0)}
}
pub unsafe extern "win64" fn ke_initialize_timer_ex(timer:*mut DispatcherHeader,timer_type:i32){
    unsafe{initialize_timer(timer,timer_type)}
}
pub unsafe extern "win64" fn ke_read_state_timer(timer:*const DispatcherHeader)->i32{
    if timer.is_null(){return 0}
    unsafe{(&*core::ptr::addr_of!((*timer).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)}
}
/// BOOLEAN KeSetTimer(PKTIMER Timer, LARGE_INTEGER DueTime, PKDPC Dpc).
/// LARGE_INTEGER is passed by value in the x64 Windows ABI.
pub unsafe extern "win64" fn ke_set_timer(timer:*mut DispatcherHeader,due_time:i64,dpc:*mut core::ffi::c_void)->u8{
    unsafe{ke_set_timer_ex(timer,due_time,0,dpc)}
}
/// BOOLEAN KeSetTimerEx(PKTIMER Timer, LARGE_INTEGER DueTime, LONG Period, PKDPC Dpc).
pub unsafe extern "win64" fn ke_set_timer_ex(timer:*mut DispatcherHeader,due_time:i64,period_ms:i32,dpc:*mut core::ffi::c_void)->u8{
    if timer.is_null()||!matches!(unsafe{(*timer).kind},TIMER_NOTIFICATION|TIMER_SYNCHRONIZATION){return 0}
    let value=due_time;
    let Some(deadline)=timer_deadline(value) else{return 0};
    let hz=crate::hal::query_performance_frequency_hz().unwrap_or(0);
    let period=if period_ms<=0||hz==0{0}else{((hz as u128)*(period_ms as u32 as u128)/1000).min(u64::MAX as u128) as u64};
    let (was_active,_old_dpc)={
        let mut table=TIMERS.lock();
        let Some(record)=table.records.iter_mut().find(|r|r.object==timer as usize && !r.timer2) else{return 0};
        let old=(record.active,record.dpc);
        record.active=true;record.due=deadline;record.period=period;record.dpc=dpc as usize;record.generation=next_timer_generation(record.generation);
        // Keep timer mutation and removal serialized with timer_tick's
        // timer-lock -> DPC-lock enqueue path, so an old cancellation cannot
        // accidentally remove a newly queued expiry for the rearmed timer.
        if old.1!=0 { let _=unsafe{ke_remove_queue_dpc(old.1 as *mut core::ffi::c_void)}; }
        old
    };
    unsafe{(&*core::ptr::addr_of!((*timer).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
    was_active as u8
}
pub unsafe extern "win64" fn ke_cancel_timer(timer:*mut DispatcherHeader)->u8{
    if timer.is_null(){return 0}
    let was_active={
        let mut table=TIMERS.lock();
        let Some(record)=table.records.iter_mut().find(|r|r.object==timer as usize && !r.timer2) else{return 0};
        let old=(record.active,record.dpc);record.active=false;record.dpc=0;
        // Invalidate an expiry snapshot even if the one-shot timer has already
        // transitioned inactive but its DPC has not yet been enqueued.
        record.generation=next_timer_generation(record.generation);
        // Serialize removal with rearm and expiry enqueue; no DPC callback is
        // dispatched by KeRemoveQueueDpc itself.
        if old.1!=0 { let _=unsafe{ke_remove_queue_dpc(old.1 as *mut core::ffi::c_void)}; }
        old.0
    };
    was_active as u8
}
pub unsafe extern "win64" fn ke_initialize_dpc(dpc:*mut core::ffi::c_void,routine:Option<DpcRoutine>,context:*mut core::ffi::c_void){
    if dpc.is_null(){return}
    let mut table=DPCS.lock();
    let slot=table.records.iter().position(|r|r.object==dpc as usize)
        .or_else(||table.records.iter().position(|r|!r.active));
    if let Some(i)=slot{
        // Do not recycle caller-owned KDPC metadata while its previous callback
        // is queued or executing. The public initializer has no failure return,
        // so an unsafe reinitialization attempt is ignored until rundown ends.
        let existing=table.records[i];
        if existing.object==dpc as usize && (existing.queued || existing.running) { return; }
        table.records[i]=DpcRecord{object:dpc as usize,routine:routine.map(|f|f as usize).unwrap_or(0),context:context as usize,arg1:0,arg2:0,target_cpu:cpu_slot(),importance:1,threaded:false,active:true,queued:false,running:false};
    }
}
/// Threaded DPCs share the public callback ABI; execution remains at DISPATCH_LEVEL
/// until the scheduler grows a dedicated PASSIVE_LEVEL worker queue.
pub unsafe extern "win64" fn ke_initialize_threaded_dpc(dpc:*mut core::ffi::c_void,routine:Option<DpcRoutine>,context:*mut core::ffi::c_void){
    unsafe{ke_initialize_dpc(dpc,routine,context);}
    if let Some(index)=dpc_slot(dpc as usize){DPCS.lock().records[index].threaded=true;}
}
pub unsafe extern "win64" fn ke_set_target_processor_dpc(dpc:*mut core::ffi::c_void,processor:u8){
    if dpc.is_null()||processor as usize>=crate::hal::MAX_CPUS||crate::hal::cpu_state(processor as usize).ok()!=Some(crate::hal::CPU_ONLINE){return}
    let mut table=DPCS.lock();
    if let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize){record.target_cpu=processor as usize;}
}
pub unsafe extern "win64" fn ke_set_target_processor_dpc_ex(dpc:*mut core::ffi::c_void,processor:*const ProcessorNumber){
    if processor.is_null(){return}
    let number=unsafe{&*processor};
    if number.group!=0{return}
    unsafe{ke_set_target_processor_dpc(dpc,number.number);}
}
pub unsafe extern "win64" fn ke_set_importance_dpc(dpc:*mut core::ffi::c_void,importance:i32){
    if dpc.is_null(){return}
    let mut table=DPCS.lock();
    if let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize){
        record.importance=importance.clamp(0,2) as u8;
    }
}
pub unsafe extern "win64" fn ke_is_executing_dpc()->u8{
    DPC_DISPATCHING[cpu_slot()].load(Ordering::Acquire) as u8
}
pub unsafe extern "win64" fn ke_insert_queue_dpc(dpc:*mut core::ffi::c_void,arg1:*mut core::ffi::c_void,arg2:*mut core::ffi::c_void)->u8{
    if dpc.is_null(){return 0}
    let mut table=DPCS.lock();
    let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize) else{return 0};
    if record.queued{return 0}
    record.arg1=arg1 as usize;record.arg2=arg2 as usize;record.queued=true;
    let target=record.target_cpu;
    let threaded=record.threaded;
    drop(table);
    let current=cpu_slot();
    if threaded {
        let woken=crate::thread::wake_threaded_dpc_worker(target);
        if !woken && target==current && CURRENT_IRQL[current].load(Ordering::Acquire)==PASSIVE_LEVEL {
            dispatch_threaded_dpcs();
        } else if target!=current && !woken {
            let _=crate::smp::request_reschedule(target);
        }
    } else if target!=current {
        let _=crate::smp::request_reschedule(target);
    } else if CURRENT_IRQL[current].load(Ordering::Acquire)<DISPATCH_LEVEL {
        dispatch_queued_dpcs();
    }
    1
}
/// Queue a DPC without dispatching it immediately. Timer expiry uses this
/// while holding the timer-generation lock so cancellation can either invalidate
/// the expiry before enqueue or remove the already-enqueued DPC afterward.
fn queue_dpc_deferred(dpc:*mut core::ffi::c_void,arg1:*mut core::ffi::c_void,arg2:*mut core::ffi::c_void)->Option<usize>{
    if dpc.is_null(){return None}
    let mut table=DPCS.lock();
    let record=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize)?;
    if record.queued{return None}
    record.arg1=arg1 as usize;record.arg2=arg2 as usize;record.queued=true;
    Some(record.target_cpu)
}

/// Queue a device DPC from interrupt context without executing it on the ISR
/// stack. The external-interrupt dispatcher drains the local queue only after
/// the callback has returned and the local APIC has been acknowledged.
pub fn queue_dpc_from_interrupt(dpc:*mut core::ffi::c_void)->bool {
    if dpc.is_null(){return false}
    let cpu=cpu_slot();
    let mut table=DPCS.lock();
    let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize) else{return false};
    if record.queued||record.running{return false}
    record.target_cpu=cpu;record.arg1=0;record.arg2=0;record.queued=true;true
}


pub unsafe extern "win64" fn ke_remove_queue_dpc(dpc:*mut core::ffi::c_void)->u8{
    if dpc.is_null(){return 0}
    let mut table=DPCS.lock();
    let Some(record)=table.records.iter_mut().find(|r|r.active&&r.object==dpc as usize) else{return 0};
    let was=record.queued;record.queued=false;was as u8
}
/// Drain this CPU's queue while the caller owns DPC_DISPATCHING and has
/// raised the logical IRQL to DISPATCH_LEVEL. Returns false only when the
/// bounded callback budget was exhausted and work remains.
fn drain_local_dpc_queue(cpu:usize)->bool{
    for _ in 0..(MAX_DPC_OBJECTS*16) {
        let callback={
            let mut table=DPCS.lock();
            let Some(record)=table.records.iter_mut().find(|r|r.active&&r.queued&&r.target_cpu==cpu&&!r.threaded) else {return true};
            record.queued=false;
            if record.routine==0 {None} else {
                // Mark the object before dropping the table lock so cancel,
                // reinitialize, and rundown observe the executing callback.
                record.running=true;
                DPC_IN_FLIGHT.fetch_add(1,Ordering::AcqRel);
                Some((record.routine,record.object,record.context,record.arg1,record.arg2))
            }
        };
        if let Some((routine,object,context,arg1,arg2))=callback {
            let function:DpcRoutine=unsafe{core::mem::transmute(routine)};
            watchdog_dpc_begin(routine);
            unsafe{function(object as *mut _,context as *mut _,arg1 as *mut _,arg2 as *mut _);}
            watchdog_dpc_end();
            {
                let mut table=DPCS.lock();
                if let Some(record)=table.records.iter_mut().find(|r|r.object==object && r.running) {
                    record.running=false;
                }
            }
            DPC_IN_FLIGHT.fetch_sub(1,Ordering::AcqRel);
        }
    }
    let table=DPCS.lock();
    !table.records.iter().any(|r|r.active&&r.queued&&r.target_cpu==cpu)
}
pub fn dispatch_queued_dpcs(){
    let cpu=cpu_slot();
    if cpu>=64||CURRENT_IRQL[cpu].load(Ordering::Acquire)>DISPATCH_LEVEL{return}
    if DPC_DISPATCHING[cpu].swap(true,Ordering::AcqRel){return}
    let prior=CURRENT_IRQL[cpu].swap(DISPATCH_LEVEL,Ordering::AcqRel);
    let _=drain_local_dpc_queue(cpu);
    CURRENT_IRQL[cpu].store(prior,Ordering::Release);
    DPC_DISPATCHING[cpu].store(false,Ordering::Release);
}

/// Run threaded DPC callbacks only from a PASSIVE_LEVEL scheduler safe point.
/// The callback is claimed under the table lock; removal/reinitialization and
/// global flush observe its in-flight lifetime until it returns.
pub fn dispatch_threaded_dpcs(){
    let cpu=cpu_slot();
    if cpu>=64||CURRENT_IRQL[cpu].load(Ordering::Acquire)!=PASSIVE_LEVEL
        ||DPC_DISPATCHING[cpu].load(Ordering::Acquire)
        ||THREADED_DPC_DISPATCHING[cpu].swap(true,Ordering::AcqRel){return}
    for _ in 0..(MAX_DPC_OBJECTS*16){
        let callback={
            let mut table=DPCS.lock();
            let Some(record)=table.records.iter_mut().find(|r|r.active&&r.queued&&r.target_cpu==cpu&&r.threaded) else{break};
            record.queued=false;
            if record.routine==0{None}else{
                record.running=true;
                DPC_IN_FLIGHT.fetch_add(1,Ordering::AcqRel);
                Some((record.routine,record.object,record.context,record.arg1,record.arg2))
            }
        };
        if let Some((routine,object,context,arg1,arg2))=callback{
            let function:DpcRoutine=unsafe{core::mem::transmute(routine)};
            watchdog_dpc_begin(routine);
            unsafe{function(object as *mut _,context as *mut _,arg1 as *mut _,arg2 as *mut _);}
            watchdog_dpc_end();
            let mut table=DPCS.lock();
            if let Some(record)=table.records.iter_mut().find(|r|r.object==object&&r.running){record.running=false;}
            drop(table);
            DPC_IN_FLIGHT.fetch_sub(1,Ordering::AcqRel);
        }
    }
    THREADED_DPC_DISPATCHING[cpu].store(false,Ordering::Release);
}

#[repr(C,align(16))]
struct ThreadedDpcTestStorage([u64;8]);
static THREADED_DPC_TEST_OBJECT:ThreadedDpcTestStorage=ThreadedDpcTestStorage([0;8]);
static THREADED_DPC_TEST_COUNT:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
unsafe extern "win64" fn threaded_dpc_test_routine(
    _dpc:*mut core::ffi::c_void,_context:*mut core::ffi::c_void,
    _arg1:*mut core::ffi::c_void,_arg2:*mut core::ffi::c_void,
){THREADED_DPC_TEST_COUNT.fetch_add(1,Ordering::AcqRel);}

/// Boot-time contract check: ordinary DPC draining must not execute a threaded
/// callback, while a PASSIVE_LEVEL drain must execute it exactly once.
pub(crate) fn threaded_dpc_self_test()->bool{
    let object=core::ptr::addr_of!(THREADED_DPC_TEST_OBJECT).cast_mut().cast::<core::ffi::c_void>();
    THREADED_DPC_TEST_COUNT.store(0,Ordering::Release);
    unsafe{ke_initialize_threaded_dpc(object,Some(threaded_dpc_test_routine),core::ptr::null_mut());}
    let cpu=cpu_slot();let prior=CURRENT_IRQL[cpu].swap(DISPATCH_LEVEL,Ordering::AcqRel);
    let inserted=unsafe{ke_insert_queue_dpc(object,core::ptr::null_mut(),core::ptr::null_mut())}!=0;
    dispatch_queued_dpcs();
    CURRENT_IRQL[cpu].store(prior,Ordering::Release);
    if !inserted||THREADED_DPC_TEST_COUNT.load(Ordering::Acquire)!=0{return false}
    dispatch_threaded_dpcs();
    THREADED_DPC_TEST_COUNT.load(Ordering::Acquire)==1
        && DPC_IN_FLIGHT.load(Ordering::Acquire)==0
}

#[repr(C,align(16))]
struct DpcRundownTestStorage([u64;8]);
static DPC_RUNDOWN_TEST_OBJECT:DpcRundownTestStorage=DpcRundownTestStorage([0;8]);
static DPC_RUNDOWN_TEST_COUNT:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
unsafe extern "win64" fn dpc_rundown_test_routine(
    dpc:*mut core::ffi::c_void,_context:*mut core::ffi::c_void,
    _arg1:*mut core::ffi::c_void,_arg2:*mut core::ffi::c_void,
){
    DPC_RUNDOWN_TEST_COUNT.fetch_add(1,Ordering::AcqRel);
    // Reinitialization during the callback must be ignored until rundown clears.
    unsafe{ke_initialize_dpc(dpc,Some(dpc_rundown_test_routine),core::ptr::null_mut());}
}

#[repr(C,align(16))]
struct Timer2RundownTestStorage([u64;8]);
static TIMER2_RUNDOWN_TEST_OBJECT:Timer2RundownTestStorage=Timer2RundownTestStorage([0;8]);
static TIMER2_RUNDOWN_TEST_COUNT:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
unsafe extern "win64" fn timer2_rundown_test_routine(
    timer:*mut core::ffi::c_void,_context:*mut core::ffi::c_void,
){
    TIMER2_RUNDOWN_TEST_COUNT.fetch_add(1,Ordering::AcqRel);
    unsafe{
        let _=ke_cancel_timer2(timer);
        // This initializer must not replace the callback/context while running.
        ke_initialize_timer2(timer,Some(timer2_rundown_test_routine),core::ptr::null_mut());
    }
}

/// Stress-facing boot regression for queued cancellation, callback rundown,
/// reinitialization while running, and timer-generation invalidation.
pub(crate) fn timer_dpc_rundown_self_test()->bool{
    let dpc=core::ptr::addr_of!(DPC_RUNDOWN_TEST_OBJECT).cast_mut().cast::<core::ffi::c_void>();
    DPC_RUNDOWN_TEST_COUNT.store(0,Ordering::Release);
    unsafe{ke_initialize_dpc(dpc,Some(dpc_rundown_test_routine),core::ptr::null_mut());}
    let cpu=cpu_slot();let prior=CURRENT_IRQL[cpu].swap(DISPATCH_LEVEL,Ordering::AcqRel);
    for iteration in 0..64{
        if unsafe{ke_insert_queue_dpc(dpc,core::ptr::null_mut(),core::ptr::null_mut())}==0
            ||unsafe{ke_remove_queue_dpc(dpc)}==0{
            CURRENT_IRQL[cpu].store(prior,Ordering::Release);return false;
        }
        if unsafe{ke_insert_queue_dpc(dpc,core::ptr::null_mut(),core::ptr::null_mut())}==0{
            CURRENT_IRQL[cpu].store(prior,Ordering::Release);return false;
        }
        CURRENT_IRQL[cpu].store(prior,Ordering::Release);
        dispatch_queued_dpcs();
        if DPC_RUNDOWN_TEST_COUNT.load(Ordering::Acquire)!=(iteration+1)
            ||DPC_IN_FLIGHT.load(Ordering::Acquire)!=0{return false}
        if iteration!=63{CURRENT_IRQL[cpu].store(DISPATCH_LEVEL,Ordering::Release);}
    }
    let timer=unsafe{core::ptr::addr_of_mut!(TIMER_DPC_CANCEL_TEST_TIMER)};
    unsafe{ke_initialize_timer_ex(timer,0);}
    let _=unsafe{ke_set_timer(timer,0,core::ptr::null_mut())};
    if unsafe{ke_cancel_timer(timer)}==0{return false}
    timer_tick(crate::hal::query_performance_counter().saturating_add(1));
    if unsafe{ke_read_state_timer(timer)}!=0{return false}

    let timer2=core::ptr::addr_of!(TIMER2_RUNDOWN_TEST_OBJECT).cast_mut().cast::<core::ffi::c_void>();
    TIMER2_RUNDOWN_TEST_COUNT.store(0,Ordering::Release);
    unsafe{ke_initialize_timer2(timer2,Some(timer2_rundown_test_routine),core::ptr::null_mut());}
    let _=unsafe{ke_set_timer2(timer2,0,0,0)};
    timer_tick(crate::hal::query_performance_counter().saturating_add(1));
    let table=TIMERS.lock();
    let record=table.records.iter().find(|r|r.object==timer2 as usize&&r.timer2);
    TIMER2_RUNDOWN_TEST_COUNT.load(Ordering::Acquire)==1
        && record.is_some_and(|r|!r.callback_running&&!r.active&&r.callback==timer2_rundown_test_routine as usize)
        && DPC_IN_FLIGHT.load(Ordering::Acquire)==0
}

static mut TIMER_DPC_CANCEL_TEST_TIMER:DispatcherHeader=DispatcherHeader{
    kind:TIMER_NOTIFICATION,absolute:0,size:6,inserted:0,signal_state:0,
    wait_list_flink:0,wait_list_blink:0,
};
unsafe extern "win64" fn flush_dpc_rendezvous(
    _dpc:*mut core::ffi::c_void,_context:*mut core::ffi::c_void,
    done:*mut core::ffi::c_void,sync:*mut core::ffi::c_void,
){
    if unsafe{ke_signal_call_dpc_synchronize(sync)}==0{return}
    let cpu=cpu_slot();
    if cpu<64 {
        // KeGenericCallDpc already owns this CPU's dispatch guard and is running
        // at DISPATCH_LEVEL; drain directly rather than recursively dispatching.
        let _=drain_local_dpc_queue(cpu);
    }
    unsafe{ke_signal_call_dpc_done(done);}
}
pub unsafe extern "win64" fn ke_flush_queued_dpcs(){
    let current=cpu_slot();
    // A synchronous flush cannot safely wait while running at DISPATCH_LEVEL.
    if CURRENT_IRQL[current].load(Ordering::Acquire)>1{return}
    let online=crate::hal::online_cpu_count();
    if online<=1 {
        dispatch_queued_dpcs();
        dispatch_threaded_dpcs();
        while DPC_IN_FLIGHT.load(Ordering::Acquire)!=0 {core::hint::spin_loop();}
        return
    }
    // A synchronous generic-DPC rendezvous drains every CPU in the online
    // topology. Repeat because callbacks can queue follow-on DPCs while a
    // different processor is draining its local queue. Never report success
    // while any queue, callback, or remote dispatcher remains active. Like the
    // Windows contract, this can wait indefinitely if an online CPU stops
    // participating; returning early would silently violate completion.
    loop {
        unsafe{ke_generic_call_dpc(Some(flush_dpc_rendezvous),core::ptr::null_mut());}
        dispatch_queued_dpcs();
        dispatch_threaded_dpcs();
        let pending={
            let table=DPCS.lock();
            table.records.iter().any(|r|r.active&&r.queued)
        }||DPC_IN_FLIGHT.load(Ordering::Acquire)!=0
          ||DPC_DISPATCHING.iter().enumerate().any(|(cpu,state)|cpu!=current&&state.load(Ordering::Acquire));
        if !pending{return}
        core::hint::spin_loop();
    }
}

/// Initializes an opaque KTIMER2 in the same deadline table used by KTIMER.
pub unsafe extern "win64" fn ke_initialize_timer2(
    timer: *mut core::ffi::c_void,
    callback: Option<Timer2Routine>,
    context: *mut core::ffi::c_void,
) {
    if timer.is_null() { return; }
    let mut table = TIMERS.lock();
    let slot = table.records.iter().position(|r| r.object == timer as usize)
        .or_else(|| table.records.iter().position(|r| r.object == 0));
    if let Some(i) = slot {
        // A callback may be using this opaque object/context. The initializer
        // has no status return, so leave the live record intact until rundown.
        if table.records[i].object == timer as usize && table.records[i].callback_running { return; }
        let generation = next_timer_generation(table.records[i].generation);
        table.records[i] = TimerRecord {
            object: timer as usize,
            callback: callback.map(|f| f as usize).unwrap_or(0),
            context: context as usize,
            timer2: true,
            generation,
            ..TimerRecord::EMPTY
        };
    }
}
pub unsafe extern "win64" fn ke_set_timer2(
    timer: *mut core::ffi::c_void,
    due_time: i64,
    period_ms: u32,
    tolerable_delay_ms: u32,
) -> u8 {
    if timer.is_null() { return 0; }
    let Some(deadline) = timer_deadline(due_time) else { return 0; };
    let hz = crate::hal::query_performance_frequency_hz().unwrap_or(0);
    let period = if period_ms == 0 || hz == 0 { 0 }
        else { ((hz as u128) * (period_ms as u128) / 1000).min(u64::MAX as u128) as u64 };
    let mut table = TIMERS.lock();
    let Some(record) = table.records.iter_mut().find(|r| r.object == timer as usize && r.timer2) else { return 0; };
    let was_active = record.active;
    record.active = true;
    record.due = deadline;
    record.period = period;
    record.tolerable_delay = tolerable_delay_ms;
    record.generation = next_timer_generation(record.generation);
    was_active as u8
}
pub unsafe extern "win64" fn ke_cancel_timer2(timer: *mut core::ffi::c_void) -> u8 {
    if timer.is_null() { return 0; }
    let mut table = TIMERS.lock();
    let Some(record) = table.records.iter_mut().find(|r| r.object == timer as usize && r.timer2) else { return 0; };
    let was_active = record.active;
    record.active = false;
    record.generation = next_timer_generation(record.generation);
    was_active as u8
}

/// One scheduler services classic KTIMER waits/DPCs and KTIMER2 callbacks.
/// Expiry snapshots are copied under the single timer lock and delivered after
/// releasing it; cancellation/rearm invalidates snapshots by generation.
pub fn timer_tick(now: u64) {
    let mut expired = [TimerRecord::EMPTY; MAX_TIMER_OBJECTS];
    let mut count = 0usize;
    {
        let mut table = TIMERS.lock();
        for record in table.records.iter_mut() {
            if !record.active || record.due > now { continue; }
            if count < expired.len() { expired[count] = *record; count += 1; }
            if record.period != 0 {
                record.due = record.due.saturating_add(record.period);
                if record.due <= now { record.due = now.saturating_add(record.period); }
            } else {
                record.active = false;
            }
        }
    }

    for record in expired[..count].iter().copied() {
        // A cancellation, rearm, or reinitialization after snapshot invalidates
        // delivery for both timer styles, including already-inactive one-shots.
        let still_current = {
            let table = TIMERS.lock();
            table.records.iter().any(|r|
                timer_snapshot_is_current(record.object, record.generation, r.object, r.generation)
                    && r.timer2 == record.timer2
            )
        };
        if !still_current { continue; }

        if record.timer2 {
            if record.callback == 0 { continue; }
            // Claim callback rundown under the same lock used by cancellation
            // and reinitialization. Once claimed, cancellation invalidates future
            // snapshots but treats this callback as already in flight.
            let claimed = {
                let mut table = TIMERS.lock();
                if let Some(live) = table.records.iter_mut().find(|r|
                    r.object == record.object && r.timer2
                        && r.generation == record.generation
                        && !r.callback_running
                ) {
                    live.callback_running = true;
                    true
                } else { false }
            };
            if !claimed { continue; }
            let callback: Timer2Routine = unsafe { core::mem::transmute(record.callback) };
            let cpu = cpu_slot();
            let prior_irql = CURRENT_IRQL[cpu].swap(DISPATCH_LEVEL, Ordering::AcqRel);
            unsafe {
                callback(record.object as *mut core::ffi::c_void,
                         record.context as *mut core::ffi::c_void);
            }
            CURRENT_IRQL[cpu].store(prior_irql, Ordering::Release);
            // Clear rundown by object identity even if the callback cancelled
            // itself and advanced the generation while it was running.
            let mut table = TIMERS.lock();
            if let Some(live) = table.records.iter_mut().find(|r|
                r.object == record.object && r.timer2
            ) {
                live.callback_running = false;
            }
            continue;
        }

        let timer = record.object as *mut DispatcherHeader;
        if timer.is_null() { continue; }
        unsafe {
            (&*core::ptr::addr_of!((*timer).signal_state).cast::<core::sync::atomic::AtomicI32>())
                .store(1, Ordering::Release);
        }
        let _guard = DISPATCHER_OBJECT_LOCK.lock();
        complete_ready_wait_all_groups();
        let mut waiters = [crate::dispatcher::WokenWaiter { thread: 0, timed_out: false }; crate::dispatcher::MAX_WAITERS];
        let woke = if unsafe { (*timer).kind } == TIMER_NOTIFICATION {
            crate::dispatcher::wake_all_timed(timer as u64, &mut waiters)
        } else if let Some(waiter) = crate::dispatcher::wake_one_timed(timer as u64) {
            waiters[0] = waiter; 1
        } else { 0 };
        let consumed = woke != 0 && waiters[..woke].iter().any(|waiter| !waiter.timed_out);
        if consumed && unsafe { (*timer).kind } == TIMER_SYNCHRONIZATION {
            unsafe {
                (&*core::ptr::addr_of!((*timer).signal_state).cast::<core::sync::atomic::AtomicI32>())
                    .store(0, Ordering::Release);
            }
        }
        for waiter in waiters[..woke].iter().copied() {
            let signal_status = if waiter.timed_out { STATUS_TIMEOUT } else { STATUS_SUCCESS };
            let status = crate::dispatcher::complete_wait_group(waiter.thread, timer as u64, signal_status, STATUS_SUCCESS, STATUS_TIMEOUT)
                .unwrap_or(signal_status);
            let _ = crate::thread::complete_wait(waiter.thread, status);
        }
        drop(_guard);

        if record.dpc != 0 {
            // Keep timer -> DPC lock order consistent with set/cancel.
            let target = {
                let table = TIMERS.lock();
                if table.records.iter().any(|r|
                    timer_snapshot_is_current(record.object, record.generation, r.object, r.generation)
                        && !r.timer2
                ) {
                    queue_dpc_deferred(record.dpc as *mut _, core::ptr::null_mut(), core::ptr::null_mut())
                } else { None }
            };
            if let Some(target) = target {
                if target != cpu_slot() { let _ = crate::smp::request_reschedule(target); }
            }
        }
    }
    dispatch_queued_dpcs();
}

/// Result of the Ki processor-start phase. A successful AP count means APs
/// reached the online state; it does not imply Windows binary compatibility.
pub(crate) struct ProcessorStartupReport {
    pub online_aps: usize,
    pub startup_ok: bool,
    pub ipi_enabled: bool,
    pub interrupt_stress_passed: bool,
    pub tsc_synced: bool,
    pub worst_tsc_offset: u64,
    pub scheduler_stress_passed: bool,
    pub vm_stress_passed: bool,
}

/// KeInitializeExecutive-style early executive initialization. The hardware
/// startup path remains in Ki routines and architecture/HAL modules.
pub(crate) fn ke_initialize_executive() {
    crate::scheduler::initialize();
    crate::thread::initialize();
}

/// KiInitializeProcessor performs the software-visible checks that can be
/// verified after the CPU's private GDT/TSS/IDT and GS-local state are installed.
pub(crate) fn ki_initialize_processor(logical_id: usize) -> bool {
    if logical_id >= crate::hal::MAX_CPUS
        || crate::hal::cpu_state(logical_id).is_err()
    {
        return false;
    }
    // This routine is called on the target logical processor during CPU
    // bring-up. Program that CPU's XCR0 before it is allowed to run threads.
    let local_xstate_ready = unsafe { ki_initialize_xstate_current_cpu() };
    if ki_xstate_enabled() && (!local_xstate_ready
        || !unsafe { ki_xstate_self_test_current_cpu() }
        || !unsafe { ki_xstate_nested_save_restore_self_test() }) {
        // A CPU that cannot preserve the system-wide enabled state mask must
        // not be admitted to the scheduler.
        return false;
    }
    crate::ntoskrnl::ke::arch::per_cpu_tss_self_test(logical_id)
}

/// KiStartAllProcessors coordinates ACPI topology discovery, AP startup,
/// per-CPU IPI stress and cross-core TSC diagnostics. BSP remains usable if
/// firmware topology or AP startup is unavailable.
pub(crate) fn ki_start_all_processors(
    pml4_physical: u64,
    trampoline_physical: u64,
    rsdp_address: u64,
    bsp_apic_id: u32,
) -> ProcessorStartupReport {
    let mut report = ProcessorStartupReport {
        online_aps: 0,
        startup_ok: false,
        ipi_enabled: false,
        interrupt_stress_passed: false,
        tsc_synced: false,
        worst_tsc_offset: 0,
        scheduler_stress_passed: false,
        vm_stress_passed: false,
    };
    if rsdp_address != 0 && trampoline_physical != 0 {
        if let Ok(madt) = unsafe { crate::acpi::discover_madt(rsdp_address) } {
            if let Ok((online, ipi_enabled)) = unsafe {
                crate::smp::start_discovered_aps_with_ipi(
                    pml4_physical, trampoline_physical, &madt, bsp_apic_id,
                )
            } {
                report.online_aps = online;
                report.startup_ok = true;
                report.ipi_enabled = ipi_enabled;
                report.interrupt_stress_passed = ipi_enabled
                    && crate::smp::interrupt_setup_stress_test(64);
                let (synced, offset) = crate::hal::query_cross_core_clock_status();
                report.tsc_synced = synced;
                report.worst_tsc_offset = offset;
            }
        }
    }
    report.scheduler_stress_passed =
        crate::thread::smp_scheduler_stress_test(crate::hal::online_cpu_count());
    report.vm_stress_passed = crate::kernel_vm::self_test();
    report
}

/// KiInitializeKernel is the fail-closed validation gate used after the
/// processor-start phase and before later boot-time self-tests continue.
pub(crate) fn ki_initialize_kernel(report: &ProcessorStartupReport) -> bool {
    report.scheduler_stress_passed
        && report.vm_stress_passed
        && threaded_dpc_self_test()
        && timer_dpc_rundown_self_test()
}

/// Minimal Ke processor-query routines backed by the live HAL processor table.
pub unsafe extern "win64" fn ke_get_current_processor_number() -> u8 {
    unsafe { ke_get_current_processor_index().min(u8::MAX as u32) as u8 }
}
pub unsafe extern "win64" fn ke_query_active_processors() -> u64 {
    let mut mask = 0u64;
    for cpu in 0..crate::hal::MAX_CPUS.min(64) {
        if crate::hal::cpu_state(cpu).ok() == Some(crate::hal::CPU_ONLINE) {
            mask |= 1u64 << cpu;
        }
    }
    mask
}
pub unsafe extern "win64" fn ke_query_time_increment() -> u32 {
    // NT's default 100-ns time increment is not exposed until the timer source
    // is calibrated; return zero rather than advertise a fabricated tick rate.
    if crate::hal::local_clock_event_active() { 156_250 } else { 0 }
}
pub unsafe extern "win64" fn ke_should_yield_processor() -> u8 {
    crate::thread::has_ready_threads() as u8
}

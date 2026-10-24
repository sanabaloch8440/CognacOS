//! Real kernel-thread contexts and a bounded cooperative ready queue.
//! Stacks are supplied by the caller and must remain mapped for the thread lifetime.
use core::cell::UnsafeCell;
use core::ffi::c_void;
use crate::hal::IrqSpinLock;

pub const MAX_THREADS: usize = 64;
pub type ThreadEntry = extern "C" fn(*mut c_void);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState { Empty, Ready, Running, Blocked, Exited }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadError { InvalidStack, InvalidAffinity, Full, NoCurrentThread, InvalidState, InvalidCpu }
#[derive(Clone, Copy)]
struct ThreadRecord { id:u64, scheduler_id:u64, state:ThreadState, affinity:u64, context_rsp:*mut u64, stack_top:u64, entry:Option<ThreadEntry>, argument:*mut c_void, wake_pending:bool, wait_status:i32, critical_region_depth:u16, guarded_region_depth:u16, alert_pending:bool, alertable_wait:bool, queue_entry:u64 }
impl ThreadRecord { const EMPTY:Self=Self{id:0,scheduler_id:0,state:ThreadState::Empty,affinity:1,context_rsp:core::ptr::null_mut(),stack_top:0,entry:None,argument:core::ptr::null_mut(),wake_pending:false,wait_status:0,critical_region_depth:0,guarded_region_depth:0,alert_pending:false,alertable_wait:false,queue_entry:0}; }
unsafe impl Send for ThreadRecord {}
struct ThreadTable { records:[ThreadRecord;MAX_THREADS], next_id:u64 }
impl ThreadTable { const fn new()->Self { Self{records:[ThreadRecord::EMPTY;MAX_THREADS],next_id:1} } }
unsafe impl Send for ThreadTable {}
static THREADS:IrqSpinLock<ThreadTable>=IrqSpinLock::new(ThreadTable::new());
struct IdleContext(UnsafeCell<*mut u64>);
unsafe impl Sync for IdleContext {}
static IDLE_CONTEXTS:[IdleContext;crate::hal::MAX_CPUS]=[const { IdleContext(UnsafeCell::new(core::ptr::null_mut())) };crate::hal::MAX_CPUS];
static IDLE_STACK_TOPS:[core::sync::atomic::AtomicU64;crate::hal::MAX_CPUS]=[const { core::sync::atomic::AtomicU64::new(0) };crate::hal::MAX_CPUS];
static STRESS_LOCK:IrqSpinLock<u64>=IrqSpinLock::new(0);
static STRESS_COMPLETED:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static STRESS_CPU_MASK:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static STRESS_FAILURES:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static mut STRESS_STACKS:[[u8;8192];8]=[[0;8192];8];

fn cpu_index()->Result<usize,ThreadError>{
    if let Some(local)=crate::hal::current_cpu_local(){
        let logical=local.logical_id.load(core::sync::atomic::Ordering::Acquire) as usize;
        if logical<crate::hal::MAX_CPUS{return Ok(logical)}
    }
    // The BSP may execute before a GS-based CPU-local accessor is fully usable;
    // fall back to the firmware-registered APIC ID, never to an arbitrary CPU slot.
    let apic_id=(unsafe{core::arch::x86_64::__cpuid(1)}.ebx>>24)&0xff;
    if let Some(index)=crate::hal::cpu_index_for_apic(apic_id).filter(|i|*i<crate::hal::MAX_CPUS){return Ok(index)}
    // The runtime deliberately starts only the BSP. Until AP startup is complete,
    // a single-online-CPU machine has an unambiguous scheduler slot.
    if crate::hal::online_cpu_count()==1 { return Ok(0); }
    Err(ThreadError::InvalidCpu)
}
extern "C" fn thread_trampoline()->! {
    let entry_and_arg={let cpu=match cpu_index(){Ok(v)=>v,Err(_)=>loop{core::hint::spin_loop()}};let id=crate::hal::cpu_local(cpu).map(|c|c.current_thread.load(core::sync::atomic::Ordering::Acquire)).unwrap_or(0);let table=THREADS.lock();table.records.iter().find(|t|t.id==id&&t.state==ThreadState::Running).and_then(|t|t.entry.map(|entry|(entry,t.argument)))};
    if let Some((entry,arg))=entry_and_arg { entry(arg); }
    exit_current()
}

/// Create a runnable kernel thread on a caller-owned, permanently mapped stack.
/// The supplied entry must return normally; its return transitions the thread to Exited.
pub fn spawn(stack:&'static mut [u8],affinity:u64,entry:ThreadEntry,argument:*mut c_void)->Result<u64,ThreadError>{
    if affinity==0{return Err(ThreadError::InvalidAffinity)}
    if stack.len()<256{return Err(ThreadError::InvalidStack)}
    let top=(stack.as_mut_ptr() as usize+stack.len())&!0xfusize;
    if top<stack.as_ptr() as usize+128{return Err(ThreadError::InvalidStack)}
    // Keep the synthetic frame 16-byte aligned before restore; RET enters the
    // trampoline with the x86-64 function-entry RSP alignment (RSP % 16 == 8).
    let frame=(top-64) as *mut u64;
    unsafe { for i in 0..6 { frame.add(i).write(0); } frame.add(6).write(thread_trampoline as usize as u64); }
    let mut table=THREADS.lock();
    let slot=table.records.iter().position(|t|t.state==ThreadState::Empty).ok_or(ThreadError::Full)?;
    let id=table.next_id;
    let mut eligible=[0usize;crate::hal::MAX_CPUS];
    let mut eligible_count=0usize;
    for cpu in 0..crate::hal::MAX_CPUS {
        if affinity&(1u64<<cpu)!=0
            && crate::hal::cpu_state(cpu).ok()==Some(crate::hal::CPU_ONLINE) {
            eligible[eligible_count]=cpu;
            eligible_count+=1;
        }
    }
    if eligible_count==0{return Err(ThreadError::InvalidAffinity);}
    // Until migration-safe context publication/per-CPU run queues exist, bind
    // each context to one allowed online CPU. Round-robin placement distributes
    // all-CPU-affinity tasks without allowing another CPU to resume an unsaved RSP.
    let owner_cpu=eligible[(id.wrapping_sub(1) as usize)%eligible_count];
    let owner_affinity=1u64<<owner_cpu;
    table.next_id=table.next_id.wrapping_add(1).max(1);
    let scheduler_id=crate::scheduler::create_task(
        4, owner_affinity, top as u64, thread_trampoline as usize as u64,
    ).map_err(|_|ThreadError::Full)?;
    table.records[slot]=ThreadRecord{id,scheduler_id,state:ThreadState::Ready,affinity:owner_affinity,context_rsp:frame,stack_top:top as u64,entry:Some(entry),argument,wake_pending:false,wait_status:0,critical_region_depth:0,guarded_region_depth:0,alert_pending:false,alertable_wait:false,queue_entry:0};
    drop(table);
    notify_affinity(owner_affinity);
    Ok(id)
}

/// Select the highest-ID-independent first runnable thread and switch stacks.
/// If the ready queue is empty, a blocked/yielding thread switches back to its saved idle context.
fn reschedule(block:bool)->Result<(),ThreadError>{
    let cpu=cpu_index()?;
    let local=crate::hal::cpu_local(cpu).map_err(|_|ThreadError::InvalidCpu)?;
    let mut observed_rsp:u64;
    unsafe{core::arch::asm!("mov {}, rsp",out(reg) observed_rsp,options(nomem,nostack,preserves_flags));}
    let _=IDLE_STACK_TOPS[cpu].compare_exchange(0,observed_rsp,core::sync::atomic::Ordering::AcqRel,core::sync::atomic::Ordering::Acquire);
    let mut current_id=local.current_thread.load(core::sync::atomic::Ordering::Acquire);
    let idle_rsp_ptr=IDLE_CONTEXTS[cpu].0.get();
    let (old_rsp_ptr,current_scheduler_id)={
        let mut table=THREADS.lock();
        if current_id!=0&&!table.records.iter().any(|t|t.id==current_id&&t.state==ThreadState::Running){
            current_id=0;
            local.current_thread.store(0,core::sync::atomic::Ordering::Release);
        }
        if current_id==0 {
            (idle_rsp_ptr,0)
        }else{
            let current=table.records.iter_mut().find(|t|t.id==current_id&&t.state==ThreadState::Running).ok_or(ThreadError::NoCurrentThread)?;
            if block&&current.wake_pending{current.wake_pending=false;return Ok(())}
            let scheduler_id=current.scheduler_id;
            current.state=if block{ThreadState::Blocked}else{ThreadState::Ready};
            if block {
                crate::scheduler::block_task(scheduler_id).map_err(|_|ThreadError::InvalidState)?;
            }else{
                crate::scheduler::yield_current(cpu).map_err(|_|ThreadError::InvalidState)?;
            }
            (&mut current.context_rsp as *mut *mut u64,scheduler_id)
        }
    };
    let selected=if current_scheduler_id==0 {
        crate::scheduler::choose_next(cpu)
    }else{
        crate::scheduler::choose_next_excluding(cpu,current_scheduler_id)
    }.map_err(|_|ThreadError::InvalidState)?;
    let next_index={
        let table=THREADS.lock();
        selected.and_then(|task|table.records.iter().position(|t|
            t.scheduler_id==task.id&&t.state==ThreadState::Ready&&t.affinity&(1u64<<cpu)!=0))
    };
    if let Some(index)=next_index {
        let next_rsp={
            let mut table=THREADS.lock();
            let next=&mut table.records[index];
            next.state=ThreadState::Running;
            local.current_thread.store(next.id,core::sync::atomic::Ordering::Release);
            let _=crate::arch::set_kernel_stack(cpu,next.stack_top);
            next.context_rsp
        };
        unsafe{crate::arch::context_switch(old_rsp_ptr,next_rsp)};
        Ok(())
    }else if current_id!=0&&!block {
        crate::scheduler::resume_task(cpu,current_scheduler_id).map_err(|_|ThreadError::InvalidState)?;
        let mut table=THREADS.lock();
        if let Some(current)=table.records.iter_mut().find(|t|t.id==current_id){
            current.state=ThreadState::Running;
        }
        Ok(())
    }else if current_id!=0 {
        let idle=unsafe{*idle_rsp_ptr};
        if idle.is_null(){
            let mut table=THREADS.lock();
            if let Some(current)=table.records.iter_mut().find(|t|t.id==current_id){
                current.state=ThreadState::Running;
            }
            return Err(ThreadError::InvalidState);
        }
        local.current_thread.store(0,core::sync::atomic::Ordering::Release);
        let idle_top=IDLE_STACK_TOPS[cpu].load(core::sync::atomic::Ordering::Acquire);
        if idle_top!=0{let _=crate::arch::set_kernel_stack(cpu,idle_top);}
        unsafe{crate::arch::context_switch(old_rsp_ptr,idle)};
        Ok(())
    }else{
        Ok(())
    }
}

pub fn yield_current()->Result<(),ThreadError>{crate::ntoskrnl::ke::runtime::dispatch_queued_dpcs();crate::ntoskrnl::ke::runtime::ke_deliver_pending_kernel_apcs();reschedule(false)}
/// Block the current thread. A wake racing before this call is remembered and cancels the block.
pub fn block_current()->Result<(),ThreadError>{reschedule(true)?;crate::ntoskrnl::ke::runtime::ke_deliver_pending_kernel_apcs();Ok(())}
/// Wake a blocked thread. If it is still running, remember the wake to close enqueue/block races.
pub fn wake(id:u64)->Result<(),ThreadError>{
    let mut table=THREADS.lock();
    let t=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty).ok_or(ThreadError::InvalidState)?;
    let affinity=t.affinity;
    match t.state{
        ThreadState::Blocked=>{
            crate::scheduler::wake_task(t.scheduler_id).map_err(|_|ThreadError::InvalidState)?;
            t.state=ThreadState::Ready;
        },
        ThreadState::Running=>t.wake_pending=true,
        ThreadState::Ready=>{},
        ThreadState::Exited|ThreadState::Empty=>return Err(ThreadError::InvalidState),
    }
    drop(table);
    notify_affinity(affinity);
    Ok(())
}
fn notify_affinity(affinity:u64) {
    for cpu in 0..crate::hal::MAX_CPUS {
        if affinity & (1u64 << cpu) != 0
            && crate::hal::cpu_state(cpu).ok() == Some(crate::hal::CPU_ONLINE) {
            let _ = crate::smp::request_reschedule(cpu);
        }
    }
}
fn exit_current()->! {
    let cpu=match cpu_index(){Ok(v)=>v,Err(_)=>loop{core::hint::spin_loop()}};
    let local=match crate::hal::cpu_local(cpu){Ok(v)=>v,Err(_)=>loop{core::hint::spin_loop()}};
    let id=local.current_thread.load(core::sync::atomic::Ordering::Acquire);
    crate::ntoskrnl::ke::runtime::thread_exit_cleanup(id);
    let idle_rsp_ptr=IDLE_CONTEXTS[cpu].0.get();
    let (old_rsp_ptr,new_rsp)={
        let mut table=THREADS.lock();
        let current=match table.records.iter_mut().find(|t|t.id==id&&t.state==ThreadState::Running){Some(t)=>t,None=>loop{core::hint::spin_loop()}};
        current.state=ThreadState::Exited;
        current.entry=None;
        current.argument=core::ptr::null_mut();
        let scheduler_id=current.scheduler_id;
        let old_rsp_ptr=&mut current.context_rsp as *mut *mut u64;
        if crate::scheduler::exit_task(scheduler_id).is_err(){loop{core::hint::spin_loop()}}
        let selected=match crate::scheduler::choose_next(cpu){Ok(task)=>task,Err(_)=>loop{core::hint::spin_loop()}};
        if let Some(task)=selected {
            let next=match table.records.iter_mut().find(|t|t.scheduler_id==task.id&&t.state==ThreadState::Ready){
                Some(t)=>t,None=>loop{core::hint::spin_loop()}
            };
            next.state=ThreadState::Running;
            local.current_thread.store(next.id,core::sync::atomic::Ordering::Release);
            let next_rsp=next.context_rsp;
            let next_stack_top=next.stack_top;
            let _=crate::arch::set_kernel_stack(cpu,next_stack_top);
            (old_rsp_ptr,next_rsp)
        }else{
            local.current_thread.store(0,core::sync::atomic::Ordering::Release);
            let idle=unsafe{*idle_rsp_ptr};
            if idle.is_null(){loop{core::hint::spin_loop()}}
            let idle_top=IDLE_STACK_TOPS[cpu].load(core::sync::atomic::Ordering::Acquire);
            if idle_top!=0{let _=crate::arch::set_kernel_stack(cpu,idle_top);}
            (old_rsp_ptr,idle)
        }
    };
    unsafe{crate::arch::context_switch(old_rsp_ptr,new_rsp)};
    loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}
}
/// Return the address of the current live thread record as an opaque KTHREAD token.
/// The record table is static, so the address remains stable until the slot is reused.
pub fn current_thread_object() -> *mut c_void {
    let Some(id) = current_thread_id() else { return core::ptr::null_mut(); };
    let mut table = THREADS.lock();
    table.records.iter_mut().find(|t| t.id == id && t.state != ThreadState::Empty)
        .map(|record| (record as *mut ThreadRecord).cast::<c_void>())
        .unwrap_or(core::ptr::null_mut())
}

/// Update scheduler/thread affinity metadata without migrating a live saved
/// context. The scheduler rejects masks that would strand a running thread.
pub fn set_thread_affinity(id:u64,mask:u64)->Option<u64>{
    if id==0||mask==0{return None}
    let (scheduler_id,old)={
        let table=THREADS.lock();
        let t=table.records.iter().find(|t|t.id==id&&t.state!=ThreadState::Empty)?;
        (t.scheduler_id,t.affinity)
    };
    if crate::scheduler::set_affinity(scheduler_id,mask).is_err(){return None}
    let affinity={
        let mut table=THREADS.lock();
        let t=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty)?;
        t.affinity=mask;t.affinity
    };
    notify_affinity(affinity);Some(old)
}
pub fn thread_affinity(id:u64)->Option<u64>{
    let table=THREADS.lock();
    table.records.iter().find(|t|t.id==id&&t.state!=ThreadState::Empty).map(|t|t.affinity)
}
pub fn current_thread_id()->Option<u64>{let cpu=cpu_index().ok()?;let id=crate::hal::cpu_local(cpu).ok()?.current_thread.load(core::sync::atomic::Ordering::Acquire);(id!=0).then_some(id)}
pub fn state(id:u64)->Option<ThreadState>{THREADS.lock().records.iter().find(|t|t.id==id).map(|t|t.state)}
/// Per-thread APC disable nesting. Critical regions suppress normal APCs;
/// guarded regions suppress both normal and special APCs.
pub fn apc_region_depths()->(u16,u16){
    let Some(id)=current_thread_id() else{return (0,0)};
    let table=THREADS.lock();
    table.records.iter().find(|t|t.id==id&&t.state!=ThreadState::Empty)
        .map(|t|(t.critical_region_depth,t.guarded_region_depth)).unwrap_or((0,0))
}
pub fn adjust_apc_region(guarded:bool,enter:bool)->bool{
    let Some(id)=current_thread_id() else{return false};
    let mut table=THREADS.lock();
    let Some(t)=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty) else{return false};
    let depth=if guarded{&mut t.guarded_region_depth}else{&mut t.critical_region_depth};
    if enter { *depth=depth.saturating_add(1); } else { *depth=depth.saturating_sub(1); }
    true
}
pub fn thread_id_from_object(object:*mut c_void)->Option<u64>{
    if object.is_null(){return None}
    let table=THREADS.lock();
    table.records.iter().find(|t|core::ptr::eq((*t as *const ThreadRecord).cast::<c_void>(),object)&&t.state!=ThreadState::Empty).map(|t|t.id)
}
pub fn set_wait_alertable(alertable:bool)->bool{
    let Some(id)=current_thread_id() else{return false};
    let mut table=THREADS.lock();
    if let Some(t)=table.records.iter_mut().find(|t|t.id==id&&t.state==ThreadState::Running){t.alertable_wait=alertable;true}else{false}
}
pub fn set_alert_pending(id:u64)->Option<bool>{
    let mut table=THREADS.lock();
    let t=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty)?;
    let prior=t.alert_pending;t.alert_pending=true;let affinity=t.affinity;
    if t.alertable_wait && t.state==ThreadState::Blocked {
        let _=crate::dispatcher::cancel_thread_waits(id);
        t.wait_status=0x0000_0101;
        let _=crate::scheduler::wake_task(t.scheduler_id);
        t.state=ThreadState::Ready;
    }
    drop(table);notify_affinity(affinity);Some(prior)
}
pub fn take_alert_pending()->bool{
    let Some(id)=current_thread_id() else{return false};
    let mut table=THREADS.lock();
    let Some(t)=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty) else{return false};
    let pending=t.alert_pending;t.alert_pending=false;pending
}
pub fn alert_pending(id:u64)->bool{
    THREADS.lock().records.iter().find(|t|t.id==id&&t.state!=ThreadState::Empty).is_some_and(|t|t.alert_pending)
}
pub fn set_queue_entry(id:u64,entry:u64)->bool{
    let mut table=THREADS.lock();
    if let Some(t)=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty){t.queue_entry=entry;true}else{false}
}
pub fn take_queue_entry()->Option<u64>{
    let id=current_thread_id()?;
    let mut table=THREADS.lock();
    let t=table.records.iter_mut().find(|t|t.id==id&&t.state==ThreadState::Running)?;
    let entry=t.queue_entry;t.queue_entry=0;(entry!=0).then_some(entry)
}


/// Complete a wait before making its thread runnable. Running-thread completion
/// is remembered so a signal between queue insertion and blocking cannot be lost.
pub fn complete_wait(id:u64,status:i32)->Result<(),ThreadError>{
    let mut table=THREADS.lock();
    let t=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty).ok_or(ThreadError::InvalidState)?;
    t.wait_status=status;
    let affinity=t.affinity;
    match t.state {
        ThreadState::Blocked=>{
            crate::scheduler::wake_task(t.scheduler_id).map_err(|_|ThreadError::InvalidState)?;
            t.state=ThreadState::Ready;
        },
        ThreadState::Running=>t.wake_pending=true,
        ThreadState::Ready=>{},
        ThreadState::Exited|ThreadState::Empty=>return Err(ThreadError::InvalidState),
    }
    drop(table);
    notify_affinity(affinity);
    Ok(())
}
pub fn take_wait_status()->Option<i32>{
    let id=current_thread_id()?;
    let mut table=THREADS.lock();
    let t=table.records.iter_mut().find(|t|t.id==id&&t.state==ThreadState::Running)?;
    let status=t.wait_status;t.wait_status=0;Some(status)
}

/// Clear a wake consumed by the waiter's post-enqueue state recheck.
pub fn clear_wake_pending()->Result<(),ThreadError>{
    let id=current_thread_id().ok_or(ThreadError::NoCurrentThread)?;
    let mut table=THREADS.lock();
    let t=table.records.iter_mut().find(|t|t.id==id&&t.state==ThreadState::Running).ok_or(ThreadError::InvalidState)?;
    t.wake_pending=false;t.wait_status=0;Ok(())
}

/// Publish the actual top of a CPU's permanent idle/bootstrap stack. This
/// value is used both for TSS.RSP0 and when a blocked/exiting thread returns to idle.
pub fn set_idle_stack_top(cpu:usize,stack_top:u64)->bool{
    if cpu>=crate::hal::MAX_CPUS||stack_top==0||stack_top&0xf!=0{return false;}
    IDLE_STACK_TOPS[cpu].store(stack_top,core::sync::atomic::Ordering::Release);
    crate::arch::set_kernel_stack(cpu,stack_top)
}

/// Initialize the runtime's per-CPU current-thread slots. The bootstrap context
/// is the idle context until the first explicit thread switch.
pub fn initialize(){
    for cpu in 0..crate::hal::MAX_CPUS {
        if let Ok(local)=crate::hal::cpu_local(cpu) {
            local.current_thread.store(0,core::sync::atomic::Ordering::Release);
        }
        unsafe { *IDLE_CONTEXTS[cpu].0.get()=core::ptr::null_mut(); }
        IDLE_STACK_TOPS[cpu].store(0,core::sync::atomic::Ordering::Release);
    }
}

fn smp_stress_io_dispatch(irp:&mut crate::io::Irp)->crate::io::IoStatus{
    irp.information=irp.length;
    crate::io::IoStatus::Success
}

extern "C" fn lock_stress_worker(argument:*mut c_void) {
    let device_id=argument as usize as u32;
    if let Ok(cpu)=cpu_index() {
        STRESS_CPU_MASK.fetch_or(1u64<<cpu,core::sync::atomic::Ordering::AcqRel);
    }
    for round in 0..256 {
        {
            let mut guard=STRESS_LOCK.lock();
            *guard=(*guard).wrapping_add(1);
        }
        if round%8==0 {
            let irp=crate::io::submit(device_id,crate::io::IRP_MJ_READ,0,32);
            match irp {
                Ok(id)=>{
                    let valid=crate::io::dispatch_one(id)==Ok(crate::io::IoStatus::Success)
                        && matches!(crate::io::query_irp(id),Some(r) if r.completed&&!r.in_flight&&r.status==crate::io::IoStatus::Success)
                        && crate::io::release_irp(id).is_ok();
                    if !valid{STRESS_FAILURES.fetch_add(1,core::sync::atomic::Ordering::AcqRel);}
                },
                Err(_)=>{STRESS_FAILURES.fetch_add(1,core::sync::atomic::Ordering::AcqRel);}
            }
            match crate::object::create(crate::object::ObjectType::Event,0x1f) {
                Ok(object)=>{
                    if crate::object::close(object.handle).is_err(){
                        STRESS_FAILURES.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
                    }
                },
                Err(_)=>{STRESS_FAILURES.fetch_add(1,core::sync::atomic::Ordering::AcqRel);}
            }
        }
        let _=yield_current();
    }
    STRESS_COMPLETED.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
}

/// Exercise contended IRQ-save locking, cross-CPU wake IPIs, context switching,
/// and thread-record teardown. Intended for the post-SMP boot self-test.
pub fn smp_scheduler_stress_test(online_cpus:usize)->bool {
    if online_cpus==0 || online_cpus>crate::hal::MAX_CPUS { return false; }
    *STRESS_LOCK.lock()=0;
    STRESS_COMPLETED.store(0,core::sync::atomic::Ordering::Release);
    STRESS_CPU_MASK.store(0,core::sync::atomic::Ordering::Release);
    STRESS_FAILURES.store(0,core::sync::atomic::Ordering::Release);
    let mut dispatch:[Option<crate::io::DispatchFn>;16]=[None;16];
    dispatch[crate::io::IRP_MJ_READ as usize]=Some(smp_stress_io_dispatch);
    let device_id=match crate::io::register_device(b"\\Device\\SmpStress",dispatch){
        Ok(id)=>id,
        Err(_)=>return false,
    };
    let mut cpu_ids=[0usize;8];
    let mut cpu_count=0usize;
    for cpu in 0..crate::hal::MAX_CPUS {
        if crate::hal::cpu_state(cpu).ok()==Some(crate::hal::CPU_ONLINE) {
            if cpu_count<cpu_ids.len(){cpu_ids[cpu_count]=cpu;cpu_count+=1;}
        }
    }
    if cpu_count==0 || crate::hal::online_cpu_count()!=online_cpus { return false; }
    let smp_active=crate::smp::ipi_shootdowns_enabled();
    let worker_cpu_count=if smp_active{cpu_count}else{1};
    let mut ids=[0u64;8];
    for slot in 0..8 {
        let stack: &'static mut [u8] = unsafe { &mut *core::ptr::addr_of_mut!(STRESS_STACKS[slot]) };
        let target_cpu=if smp_active{cpu_ids[slot%worker_cpu_count]}else{0};
        let affinity=1u64<<target_cpu;
        match spawn(stack,affinity,lock_stress_worker,device_id as usize as *mut c_void) {
            Ok(id)=>ids[slot]=id,
            Err(_)=>return false,
        }
    }
    let mut spins=0usize;
    while spins<100_000_000
        && (STRESS_COMPLETED.load(core::sync::atomic::Ordering::Acquire)<ids.len()
            || ids.iter().any(|id|state(*id)!=Some(ThreadState::Exited))) {
        let _=yield_current();
        core::hint::spin_loop();
        spins+=1;
    }
    let scheduler_teardown_ok=ids.iter().all(|id|{
        let scheduler_id={
            let table=THREADS.lock();
            table.records.iter().find(|t|t.id==*id).map(|t|t.scheduler_id)
        };
        scheduler_id.and_then(|sid|crate::scheduler::task_snapshot(sid).ok())
            .is_some_and(|task|task.state==crate::scheduler::TaskState::Exited)
    });
    let expected_cpu_count=if smp_active{core::cmp::min(online_cpus,8)}else{1};
    STRESS_COMPLETED.load(core::sync::atomic::Ordering::Acquire)==ids.len()
        && *STRESS_LOCK.lock()==8*256
        && STRESS_FAILURES.load(core::sync::atomic::Ordering::Acquire)==0
        && STRESS_CPU_MASK.load(core::sync::atomic::Ordering::Acquire).count_ones() as usize>=expected_cpu_count
        && ids.iter().all(|id|state(*id)==Some(ThreadState::Exited))
        && scheduler_teardown_ok
}

/// Fast scheduler predicate for the interruptible idle loop.
pub fn has_ready_threads()->bool{
    let cpu=cpu_index().unwrap_or(0);
    let bit=1u64.checked_shl(cpu as u32).unwrap_or(0);
    THREADS.lock().records.iter().any(|t|t.state==ThreadState::Ready&&t.affinity&bit!=0)
}

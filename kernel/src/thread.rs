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
struct ThreadRecord { id:u64, state:ThreadState, affinity:u64, context_rsp:*mut u64, entry:Option<ThreadEntry>, argument:*mut c_void, wake_pending:bool, wait_status:i32 }
impl ThreadRecord { const EMPTY:Self=Self{id:0,state:ThreadState::Empty,affinity:1,context_rsp:core::ptr::null_mut(),entry:None,argument:core::ptr::null_mut(),wake_pending:false,wait_status:0}; }
unsafe impl Send for ThreadRecord {}
struct ThreadTable { records:[ThreadRecord;MAX_THREADS], next_id:u64 }
impl ThreadTable { const fn new()->Self { Self{records:[ThreadRecord::EMPTY;MAX_THREADS],next_id:1} } }
unsafe impl Send for ThreadTable {}
static THREADS:IrqSpinLock<ThreadTable>=IrqSpinLock::new(ThreadTable::new());
struct IdleContext(UnsafeCell<*mut u64>);
unsafe impl Sync for IdleContext {}
static IDLE_CONTEXTS:[IdleContext;crate::hal::MAX_CPUS]=[const { IdleContext(UnsafeCell::new(core::ptr::null_mut())) };crate::hal::MAX_CPUS];

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
    let id=table.next_id;table.next_id=table.next_id.wrapping_add(1).max(1);
    table.records[slot]=ThreadRecord{id,state:ThreadState::Ready,affinity,context_rsp:frame,entry:Some(entry),argument,wake_pending:false,wait_status:0};
    Ok(id)
}

/// Select the highest-ID-independent first runnable thread and switch stacks.
/// If the ready queue is empty, a blocked/yielding thread switches back to its saved idle context.
fn reschedule(block:bool)->Result<(),ThreadError>{
    let cpu=cpu_index()?;
    let local=crate::hal::cpu_local(cpu).map_err(|_|ThreadError::InvalidCpu)?;
    let mut current_id=local.current_thread.load(core::sync::atomic::Ordering::Acquire);
    let idle_rsp_ptr=IDLE_CONTEXTS[cpu].0.get();
    let (old_rsp_ptr,new_rsp)= {
        let mut table=THREADS.lock();
        // The bootstrapping execution context is not a TCB. If a stale/non-runtime
        // current-thread value exists, adopt the saved idle context rather than
        // refusing the first real thread dispatch.
        if current_id!=0 && !table.records.iter().any(|t|t.id==current_id&&t.state==ThreadState::Running) {
            current_id=0;
            local.current_thread.store(0,core::sync::atomic::Ordering::Release);
        }
        let old_rsp_ptr=if current_id==0 {idle_rsp_ptr} else {
            let current=table.records.iter_mut().find(|t|t.id==current_id&&t.state==ThreadState::Running).ok_or(ThreadError::NoCurrentThread)?;
            if block&&current.wake_pending {current.wake_pending=false;return Ok(())}
            current.state=if block{ThreadState::Blocked}else{ThreadState::Ready};
            &mut current.context_rsp as *mut *mut u64
        };
        let bit=1u64.checked_shl(cpu as u32).unwrap_or(0);
        let next=table.records.iter().position(|t|t.state==ThreadState::Ready&&t.affinity&bit!=0&&(current_id==0||t.id!=current_id));
        if let Some(index)=next {
            let next_record=&mut table.records[index];next_record.state=ThreadState::Running;
            local.current_thread.store(next_record.id,core::sync::atomic::Ordering::Release);
            (old_rsp_ptr,next_record.context_rsp)
        } else if current_id!=0 && !block {
            if let Some(current)=table.records.iter_mut().find(|t|t.id==current_id){current.state=ThreadState::Running;}
            return Ok(());
        } else if current_id!=0 {
            let idle=unsafe{*idle_rsp_ptr};
            if idle.is_null() {
                if let Some(current)=table.records.iter_mut().find(|t|t.id==current_id){current.state=ThreadState::Running;}
                return Err(ThreadError::InvalidState);
            }
            local.current_thread.store(0,core::sync::atomic::Ordering::Release);
            (old_rsp_ptr,idle)
        } else { return Ok(()) }
    };
    unsafe{crate::arch::context_switch(old_rsp_ptr,new_rsp)};
    Ok(())
}

pub fn yield_current()->Result<(),ThreadError>{reschedule(false)}
/// Block the current thread. A wake racing before this call is remembered and cancels the block.
pub fn block_current()->Result<(),ThreadError>{reschedule(true)}
/// Wake a blocked thread. If it is still running, remember the wake to close enqueue/block races.
pub fn wake(id:u64)->Result<(),ThreadError>{let mut table=THREADS.lock();let t=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty).ok_or(ThreadError::InvalidState)?;match t.state{ThreadState::Blocked=>{t.state=ThreadState::Ready;Ok(())},ThreadState::Running=>{t.wake_pending=true;Ok(())},ThreadState::Ready=>Ok(()),ThreadState::Exited|ThreadState::Empty=>Err(ThreadError::InvalidState)}}
fn exit_current()->! {
    let cpu=match cpu_index(){Ok(v)=>v,Err(_)=>loop{core::hint::spin_loop()}};
    let local=match crate::hal::cpu_local(cpu){Ok(v)=>v,Err(_)=>loop{core::hint::spin_loop()}};
    let id=local.current_thread.load(core::sync::atomic::Ordering::Acquire);
    let idle_rsp_ptr=IDLE_CONTEXTS[cpu].0.get();
    let (old_rsp_ptr,new_rsp)={
        let mut table=THREADS.lock();
        let current=match table.records.iter_mut().find(|t|t.id==id&&t.state==ThreadState::Running){Some(t)=>t,None=>loop{core::hint::spin_loop()}};
        current.state=ThreadState::Exited;current.entry=None;current.argument=core::ptr::null_mut();
        let old_rsp_ptr=&mut current.context_rsp as *mut *mut u64;
        let bit=1u64.checked_shl(cpu as u32).unwrap_or(0);
        if let Some(index)=table.records.iter().position(|t|t.state==ThreadState::Ready&&t.affinity&bit!=0){
            let next=&mut table.records[index];next.state=ThreadState::Running;
            local.current_thread.store(next.id,core::sync::atomic::Ordering::Release);
            (old_rsp_ptr,next.context_rsp)
        }else{
            local.current_thread.store(0,core::sync::atomic::Ordering::Release);
            let idle=unsafe{*idle_rsp_ptr};
            if idle.is_null(){loop{core::hint::spin_loop()}}
            (old_rsp_ptr,idle)
        }
    };
    unsafe{crate::arch::context_switch(old_rsp_ptr,new_rsp)};
    loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}
}
pub fn current_thread_id()->Option<u64>{let cpu=cpu_index().ok()?;let id=crate::hal::cpu_local(cpu).ok()?.current_thread.load(core::sync::atomic::Ordering::Acquire);(id!=0).then_some(id)}
pub fn state(id:u64)->Option<ThreadState>{THREADS.lock().records.iter().find(|t|t.id==id).map(|t|t.state)}

/// Complete a wait before making its thread runnable. Running-thread completion
/// is remembered so a signal between queue insertion and blocking cannot be lost.
pub fn complete_wait(id:u64,status:i32)->Result<(),ThreadError>{
    let mut table=THREADS.lock();
    let t=table.records.iter_mut().find(|t|t.id==id&&t.state!=ThreadState::Empty).ok_or(ThreadError::InvalidState)?;
    t.wait_status=status;
    match t.state {
        ThreadState::Blocked=>{t.state=ThreadState::Ready;Ok(())},
        ThreadState::Running=>{t.wake_pending=true;Ok(())},
        ThreadState::Ready=>Ok(()),
        ThreadState::Exited|ThreadState::Empty=>Err(ThreadError::InvalidState),
    }
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

/// Initialize the runtime's per-CPU current-thread slots. The bootstrap context
/// is the idle context until the first explicit thread switch.
pub fn initialize(){
    for cpu in 0..crate::hal::MAX_CPUS {
        if let Ok(local)=crate::hal::cpu_local(cpu) {
            local.current_thread.store(0,core::sync::atomic::Ordering::Release);
        }
        unsafe { *IDLE_CONTEXTS[cpu].0.get()=core::ptr::null_mut(); }
    }
}

/// Fast scheduler predicate for the interruptible idle loop.
pub fn has_ready_threads()->bool{
    let cpu=cpu_index().unwrap_or(0);
    let bit=1u64.checked_shl(cpu as u32).unwrap_or(0);
    THREADS.lock().records.iter().any(|t|t.state==ThreadState::Ready&&t.affinity&bit!=0)
}

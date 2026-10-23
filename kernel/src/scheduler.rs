//! Bounded scheduler core: task metadata, priorities, and CPU affinity.
//! Stack switching/preemption and AP startup must be integrated before this is a
//! preemptive SMP scheduler.
use core::sync::atomic::{AtomicBool, Ordering};
pub const MAX_TASKS: usize = 128;
pub const MAX_CPUS: usize = 64;
pub const PRIORITY_LEVELS: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState { Empty, Ready, Running, Blocked, Exited }
#[derive(Clone, Copy, Debug)]
pub struct Task { pub id:u64, pub state:TaskState, pub priority:u8, pub affinity:u64, pub stack_top:u64, pub instruction_pointer:u64 }
impl Task { const EMPTY:Self=Self{id:0,state:TaskState::Empty,priority:0,affinity:1,stack_top:0,instruction_pointer:0}; }
struct SpinLock(AtomicBool);
impl SpinLock {
 const fn new()->Self{Self(AtomicBool::new(false))}
 fn lock(&self){while self.0.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}}
 fn unlock(&self){self.0.store(false,Ordering::Release);}
}
struct SchedulerState { tasks:[Task;MAX_TASKS], next_id:u64, current:[u64;MAX_CPUS], initialized:bool }
struct Shared { lock:SpinLock, state:core::cell::UnsafeCell<SchedulerState> }
unsafe impl Sync for Shared {}
static SCHED:Shared=Shared{lock:SpinLock::new(),state:core::cell::UnsafeCell::new(SchedulerState{tasks:[Task::EMPTY;MAX_TASKS],next_id:1,current:[0;MAX_CPUS],initialized:false})};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SchedulerError { NotInitialized, Full, InvalidPriority, InvalidAffinity, UnknownTask, InvalidCpu }
pub fn initialize(){SCHED.lock.lock();unsafe{(*SCHED.state.get()).initialized=true;}SCHED.lock.unlock();}
pub fn create_task(priority:u8,affinity:u64,stack_top:u64,instruction_pointer:u64)->Result<u64,SchedulerError>{
 if priority as usize>=PRIORITY_LEVELS{return Err(SchedulerError::InvalidPriority);}
 if affinity==0{return Err(SchedulerError::InvalidAffinity);}
 SCHED.lock.lock();let s=unsafe{&mut *SCHED.state.get()};
 if !s.initialized {SCHED.lock.unlock();return Err(SchedulerError::NotInitialized);}
 let Some(index)=s.tasks.iter().position(|t|t.state==TaskState::Empty) else {SCHED.lock.unlock();return Err(SchedulerError::Full);};
 let id=s.next_id;s.next_id=s.next_id.wrapping_add(1).max(1);
 s.tasks[index]=Task{id,state:TaskState::Ready,priority,affinity,stack_top,instruction_pointer};SCHED.lock.unlock();Ok(id)
}
/// Select the highest-priority ready task eligible on the supplied logical CPU.
/// Caller owns context switching and must invoke this with interrupts disabled.
pub fn choose_next(cpu:usize)->Result<Option<Task>,SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 SCHED.lock.lock();let s=unsafe{&mut *SCHED.state.get()};
 if !s.initialized{SCHED.lock.unlock();return Err(SchedulerError::NotInitialized);}
 let bit=1u64<<cpu;let mut best:Option<usize>=None;
 for (i,t) in s.tasks.iter().enumerate(){if t.state==TaskState::Ready&&t.affinity&bit!=0&&best.map_or(true,|b|t.priority>s.tasks[b].priority){best=Some(i);}}
 let result=if let Some(i)=best {s.tasks[i].state=TaskState::Running;s.current[cpu]=s.tasks[i].id;Some(s.tasks[i])}else{None};
 SCHED.lock.unlock();Ok(result)
}
pub fn yield_current(cpu:usize)->Result<(),SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 SCHED.lock.lock();let s=unsafe{&mut *SCHED.state.get()};let id=s.current[cpu];
 let Some(index)=s.tasks.iter().position(|t|t.id==id&&t.state==TaskState::Running)else{SCHED.lock.unlock();return Err(SchedulerError::UnknownTask);};
 s.tasks[index].state=TaskState::Ready;s.current[cpu]=0;SCHED.lock.unlock();Ok(())
}
pub fn set_affinity(id:u64,mask:u64)->Result<(),SchedulerError>{
 if mask==0{return Err(SchedulerError::InvalidAffinity);}
 SCHED.lock.lock();let s=unsafe{&mut *SCHED.state.get()};
 let Some(index)=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty)else{SCHED.lock.unlock();return Err(SchedulerError::UnknownTask);};
 s.tasks[index].affinity=mask;SCHED.lock.unlock();Ok(())
}

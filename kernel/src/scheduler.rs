//! SMP-safe scheduler metadata core. This module does not claim real context
//! switching until the architecture switch frame and timer preemption are wired.
use crate::hal::IrqSpinLock;
pub const MAX_TASKS: usize = 128;
pub const MAX_CPUS: usize = 64;
pub const PRIORITY_LEVELS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState { Empty, Ready, Running, Blocked, Exited }
#[derive(Clone, Copy, Debug)]
pub struct Task {
    pub id:u64, pub state:TaskState, pub priority:u8, pub affinity:u64,
    pub stack_top:u64, pub instruction_pointer:u64,
}
impl Task { const EMPTY:Self=Self{id:0,state:TaskState::Empty,priority:0,affinity:1,stack_top:0,instruction_pointer:0}; }
struct SchedulerState { tasks:[Task;MAX_TASKS], next_id:u64, current:[u64;MAX_CPUS], initialized:bool }
impl SchedulerState {
 const fn new()->Self { Self{tasks:[Task::EMPTY;MAX_TASKS],next_id:1,current:[0;MAX_CPUS],initialized:false} }
}
static SCHED:IrqSpinLock<SchedulerState>=IrqSpinLock::new(SchedulerState::new());
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SchedulerError { NotInitialized, Full, InvalidPriority, InvalidAffinity, UnknownTask, InvalidCpu, CpuBusy, AffinityConflict }

pub fn initialize(){ SCHED.lock().initialized=true; }
pub fn create_task(priority:u8,affinity:u64,stack_top:u64,instruction_pointer:u64)->Result<u64,SchedulerError>{
 if priority as usize>=PRIORITY_LEVELS{return Err(SchedulerError::InvalidPriority);}
 if affinity==0{return Err(SchedulerError::InvalidAffinity);}
 let mut s=SCHED.lock();
 if !s.initialized{return Err(SchedulerError::NotInitialized);}
 let index=s.tasks.iter().position(|t|t.state==TaskState::Empty).ok_or(SchedulerError::Full)?;
 let id=s.next_id;s.next_id=s.next_id.wrapping_add(1).max(1);
 s.tasks[index]=Task{id,state:TaskState::Ready,priority,affinity,stack_top,instruction_pointer};
 Ok(id)
}
/// Select at most one task per CPU. Caller must still perform the architecture
/// context switch; the lock only protects scheduler metadata.
pub fn choose_next(cpu:usize)->Result<Option<Task>,SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 let mut s=SCHED.lock();
 if !s.initialized{return Err(SchedulerError::NotInitialized);}
 if s.current[cpu]!=0{return Err(SchedulerError::CpuBusy);}
 let bit=1u64<<cpu;
 let mut best:Option<usize>=None;
 for (i,t) in s.tasks.iter().enumerate(){
  if t.state==TaskState::Ready&&t.affinity&bit!=0&&best.map_or(true,|b|t.priority>s.tasks[b].priority){best=Some(i);}
 }
 if let Some(i)=best {s.tasks[i].state=TaskState::Running;s.current[cpu]=s.tasks[i].id;Ok(Some(s.tasks[i]))}else{Ok(None)}
}
pub fn yield_current(cpu:usize)->Result<(),SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 let mut s=SCHED.lock();let id=s.current[cpu];
 let index=s.tasks.iter().position(|t|t.id==id&&t.state==TaskState::Running).ok_or(SchedulerError::UnknownTask)?;
 s.tasks[index].state=TaskState::Ready;s.current[cpu]=0;Ok(())
}
pub fn set_affinity(id:u64,mask:u64)->Result<(),SchedulerError>{
 if mask==0{return Err(SchedulerError::InvalidAffinity);}
 let mut s=SCHED.lock();
 let index=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty).ok_or(SchedulerError::UnknownTask)?;
 if s.tasks[index].state==TaskState::Running {
  let owner=s.current.iter().position(|current|*current==id).ok_or(SchedulerError::UnknownTask)?;
  if mask&(1u64<<owner)==0{return Err(SchedulerError::AffinityConflict);}
 }
 s.tasks[index].affinity=mask;Ok(())
}
pub fn task_snapshot(id:u64)->Result<Task,SchedulerError>{
 let s=SCHED.lock();
 s.tasks.iter().find(|t|t.id==id&&t.state!=TaskState::Empty).copied().ok_or(SchedulerError::UnknownTask)
}

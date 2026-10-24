//! SMP-aware scheduler state integrated with the cooperative kernel-thread runtime.
//! Context switches use the architecture stack-switch routine; timer preemption remains pending.
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
pub enum SchedulerError { NotInitialized, Full, InvalidPriority, InvalidAffinity, UnknownTask, InvalidCpu, CpuBusy, AffinityConflict, InvalidState }

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
 choose_next_excluding(cpu,0)
}
/// Select a ready task except the caller's current task, allowing cooperative
/// round-robin behavior without accidentally switching to the same saved frame.
pub fn choose_next_excluding(cpu:usize,exclude_id:u64)->Result<Option<Task>,SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 let mut s=SCHED.lock();
 if !s.initialized{return Err(SchedulerError::NotInitialized);}
 if s.current[cpu]!=0{return Err(SchedulerError::CpuBusy);}
 let bit=1u64<<cpu;
 let mut best:Option<usize>=None;
 for (i,t) in s.tasks.iter().enumerate(){
  if t.state==TaskState::Ready&&t.id!=exclude_id&&t.affinity&bit!=0&&best.map_or(true,|b|t.priority>s.tasks[b].priority){best=Some(i);}
 }
 if let Some(i)=best {s.tasks[i].state=TaskState::Running;s.current[cpu]=s.tasks[i].id;Ok(Some(s.tasks[i]))}else{Ok(None)}
}
/// Restore a just-yielded task when no peer is runnable on this CPU.
pub fn resume_task(cpu:usize,id:u64)->Result<(),SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 let mut s=SCHED.lock();
 if !s.initialized{return Err(SchedulerError::NotInitialized);}
 if s.current[cpu]!=0{return Err(SchedulerError::CpuBusy);}
 let index=s.tasks.iter().position(|t|t.id==id&&t.state==TaskState::Ready&&t.affinity&(1u64<<cpu)!=0).ok_or(SchedulerError::UnknownTask)?;
 s.tasks[index].state=TaskState::Running;s.current[cpu]=id;Ok(())
}
/// Retire a completed task and release its CPU ownership slot.
pub fn exit_task(id:u64)->Result<(),SchedulerError>{
 let mut s=SCHED.lock();
 let index=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty).ok_or(SchedulerError::UnknownTask)?;
 if s.tasks[index].state==TaskState::Running {
  let cpu=s.current.iter().position(|current|*current==id).ok_or(SchedulerError::UnknownTask)?;
  s.current[cpu]=0;
 }
 s.tasks[index].state=TaskState::Exited;Ok(())
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
/// Change a live task's scheduling priority and return its previous level.
pub fn set_task_priority(id:u64,priority:u8)->Result<u8,SchedulerError>{
 if priority as usize>=PRIORITY_LEVELS{return Err(SchedulerError::InvalidPriority)}
 let mut s=SCHED.lock();
 let index=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty).ok_or(SchedulerError::UnknownTask)?;
 let old=s.tasks[index].priority;
 s.tasks[index].priority=priority;
 Ok(old)
}
pub fn task_snapshot(id:u64)->Result<Task,SchedulerError>{
 let s=SCHED.lock();
 s.tasks.iter().find(|t|t.id==id&&t.state!=TaskState::Empty).copied().ok_or(SchedulerError::UnknownTask)
}


/// Mark a task blocked and detach it from its CPU's current-task slot.
/// This is scheduler metadata only; a caller must switch to another saved context
/// before using this from a running thread.
pub fn block_task(id: u64) -> Result<(), SchedulerError> {
    let mut s = SCHED.lock();
    let index = s.tasks.iter().position(|t| t.id == id && t.state != TaskState::Empty)
        .ok_or(SchedulerError::UnknownTask)?;
    match s.tasks[index].state {
        TaskState::Ready => s.tasks[index].state = TaskState::Blocked,
        TaskState::Running => {
            let cpu = s.current.iter().position(|current| *current == id)
                .ok_or(SchedulerError::UnknownTask)?;
            s.current[cpu] = 0;
            s.tasks[index].state = TaskState::Blocked;
        }
        TaskState::Blocked | TaskState::Exited | TaskState::Empty => {
            return Err(SchedulerError::InvalidState);
        }
    }
    Ok(())
}

/// Make a blocked task eligible for selection by choose_next().
pub fn wake_task(id: u64) -> Result<(), SchedulerError> {
    let mut s = SCHED.lock();
    let index = s.tasks.iter().position(|t| t.id == id && t.state != TaskState::Empty)
        .ok_or(SchedulerError::UnknownTask)?;
    if s.tasks[index].state != TaskState::Blocked {
        return Err(SchedulerError::InvalidState);
    }
    s.tasks[index].state = TaskState::Ready;
    Ok(())
}

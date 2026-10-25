//! SMP scheduler with bounded per-CPU ready queues and affinity-aware stealing.
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
#[derive(Clone, Copy)]
struct RunQueue { ids:[u64;MAX_TASKS], len:usize }
impl RunQueue {
 const EMPTY:Self=Self{ids:[0;MAX_TASKS],len:0};
 fn remove(&mut self,id:u64)->bool{
  let Some(i)=self.ids[..self.len].iter().position(|v|*v==id) else{return false};
  self.len-=1;self.ids[i]=self.ids[self.len];self.ids[self.len]=0;true
 }
 fn push(&mut self,id:u64)->bool{
  if self.len==MAX_TASKS||self.ids[..self.len].contains(&id){return false}
  self.ids[self.len]=id;self.len+=1;true
 }
}
struct SchedulerState { tasks:[Task;MAX_TASKS], queues:[RunQueue;MAX_CPUS], next_id:u64, current:[u64;MAX_CPUS], initialized:bool }
impl SchedulerState {
 const fn new()->Self { Self{tasks:[Task::EMPTY;MAX_TASKS],queues:[RunQueue::EMPTY;MAX_CPUS],next_id:1,current:[0;MAX_CPUS],initialized:false} }
}
static SCHED:IrqSpinLock<SchedulerState>=IrqSpinLock::new(SchedulerState::new());
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SchedulerError { NotInitialized, Full, InvalidPriority, InvalidAffinity, UnknownTask, InvalidCpu, CpuBusy, AffinityConflict, InvalidState }

pub fn initialize(){ SCHED.lock().initialized=true; }
fn eligible_mask(mask:u64)->u64{
 let mut eligible=0;
 for cpu in 0..MAX_CPUS { if mask&(1u64<<cpu)!=0 && crate::hal::cpu_state(cpu).ok()==Some(crate::hal::CPU_ONLINE){eligible|=1u64<<cpu;} }
 eligible
}
fn least_loaded_cpu(s:&SchedulerState,mask:u64)->Option<usize>{
 (0..MAX_CPUS).filter(|cpu|mask&(1u64<<cpu)!=0)
  .min_by_key(|cpu|(s.queues[*cpu].len,if s.current[*cpu]==0{0usize}else{1usize},*cpu))
}
fn enqueue_ready(s:&mut SchedulerState,index:usize,preferred:Option<usize>)->Result<(),SchedulerError>{
 let task=s.tasks[index];
 let mask=eligible_mask(task.affinity);
 let cpu=preferred.filter(|c|*c<MAX_CPUS&&mask&(1u64<<c)!=0)
  .or_else(||least_loaded_cpu(s,mask)).ok_or(SchedulerError::InvalidAffinity)?;
 if !s.queues[cpu].push(task.id){return Err(SchedulerError::Full)}
 Ok(())
}
fn remove_from_queues(s:&mut SchedulerState,id:u64){for queue in s.queues.iter_mut(){queue.remove(id);}}
fn pop_best_from_queue(s:&mut SchedulerState,cpu:usize,exclude_id:u64)->Option<Task>{
 let queue=&s.queues[cpu];
 let mut best:Option<(usize,u8)>=None;
 for (pos,id) in queue.ids[..queue.len].iter().enumerate(){
  if *id==exclude_id{continue}
  let Some(task)=s.tasks.iter().find(|t|t.id==*id&&t.state==TaskState::Ready&&t.affinity&(1u64<<cpu)!=0) else{continue};
  if best.map_or(true,|(_,p)|task.priority>p){best=Some((pos,task.priority));}
 }
 let (pos,_)=best?;
 let id=s.queues[cpu].ids[pos];
 s.queues[cpu].remove(id);
 let index=s.tasks.iter().position(|t|t.id==id&&t.state==TaskState::Ready)?;
 s.tasks[index].state=TaskState::Running;s.current[cpu]=id;Some(s.tasks[index])
}
pub fn create_task(priority:u8,affinity:u64,stack_top:u64,instruction_pointer:u64)->Result<u64,SchedulerError>{
 if priority as usize>=PRIORITY_LEVELS{return Err(SchedulerError::InvalidPriority);}
 let affinity=eligible_mask(affinity);
 if affinity==0{return Err(SchedulerError::InvalidAffinity);}
 let mut s=SCHED.lock();
 if !s.initialized{return Err(SchedulerError::NotInitialized);}
 let index=s.tasks.iter().position(|t|t.state==TaskState::Empty||t.state==TaskState::Exited).ok_or(SchedulerError::Full)?;
 let id=s.next_id;s.next_id=s.next_id.wrapping_add(1).max(1);
 s.tasks[index]=Task{id,state:TaskState::Ready,priority,affinity,stack_top,instruction_pointer};
 if let Err(e)=enqueue_ready(&mut s,index,None){s.tasks[index]=Task::EMPTY;return Err(e)}
 Ok(id)
}
/// Pick locally first, then steal the highest-priority affinity-compatible task
/// from another queue. The scheduler lock makes a task's ownership transfer atomic.
pub fn choose_next(cpu:usize)->Result<Option<Task>,SchedulerError>{choose_next_excluding(cpu,0)}
pub fn choose_next_excluding(cpu:usize,exclude_id:u64)->Result<Option<Task>,SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 let mut s=SCHED.lock();
 if !s.initialized{return Err(SchedulerError::NotInitialized);}
 if s.current[cpu]!=0{return Err(SchedulerError::CpuBusy);}
 if let Some(task)=pop_best_from_queue(&mut s,cpu,exclude_id){return Ok(Some(task))}
 for source in 0..MAX_CPUS {
  if source==cpu{continue}
  let Some((pos,id))=s.queues[source].ids[..s.queues[source].len].iter().enumerate()
   .filter(|(_,id)|**id!=exclude_id)
   .filter_map(|(pos,id)|s.tasks.iter().find(|t|t.id==*id&&t.state==TaskState::Ready&&t.affinity&(1u64<<cpu)!=0).map(|t|(pos,t.id,t.priority)))
   .max_by_key(|(_,_,priority)|*priority).map(|(pos,id,_)|(pos,id)) else{continue};
  let _=pos;s.queues[source].remove(id);
  let index=s.tasks.iter().position(|t|t.id==id&&t.state==TaskState::Ready).ok_or(SchedulerError::UnknownTask)?;
  s.tasks[index].state=TaskState::Running;s.current[cpu]=id;return Ok(Some(s.tasks[index]))
 }
 Ok(None)
}
pub fn resume_task(cpu:usize,id:u64)->Result<(),SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 let mut s=SCHED.lock();
 if !s.initialized{return Err(SchedulerError::NotInitialized);}
 if s.current[cpu]!=0{return Err(SchedulerError::CpuBusy);}
 let index=s.tasks.iter().position(|t|t.id==id&&t.state==TaskState::Ready&&t.affinity&(1u64<<cpu)!=0).ok_or(SchedulerError::UnknownTask)?;
 remove_from_queues(&mut s,id);s.tasks[index].state=TaskState::Running;s.current[cpu]=id;Ok(())
}
pub fn exit_task(id:u64)->Result<(),SchedulerError>{
 let mut s=SCHED.lock();
 let index=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty).ok_or(SchedulerError::UnknownTask)?;
 if s.tasks[index].state==TaskState::Running {
  let cpu=s.current.iter().position(|current|*current==id).ok_or(SchedulerError::UnknownTask)?;s.current[cpu]=0;
 }
 remove_from_queues(&mut s,id);s.tasks[index].state=TaskState::Exited;Ok(())
}
/// Detach the current task without publishing it to any run queue. The
/// architecture switch path must save its RSP before publish_ready_task is called.
pub fn yield_current(cpu:usize)->Result<(),SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 let mut s=SCHED.lock();let id=s.current[cpu];
 let index=s.tasks.iter().position(|t|t.id==id&&t.state==TaskState::Running).ok_or(SchedulerError::UnknownTask)?;
 s.current[cpu]=0;s.tasks[index].state=TaskState::Ready;Ok(())
}
/// Publish a switched-out task only after its saved context is fully written.
pub fn publish_ready_task(id:u64,cpu:usize)->Result<(),SchedulerError>{
 if cpu>=MAX_CPUS{return Err(SchedulerError::InvalidCpu);}
 let mut s=SCHED.lock();
 let index=s.tasks.iter().position(|t|t.id==id&&t.state==TaskState::Ready).ok_or(SchedulerError::UnknownTask)?;
 if s.queues.iter().any(|q|q.ids[..q.len].contains(&id)){return Ok(())}
 enqueue_ready(&mut s,index,Some(cpu))
}
pub fn set_affinity(id:u64,mask:u64)->Result<(),SchedulerError>{
 let mask=eligible_mask(mask);if mask==0{return Err(SchedulerError::InvalidAffinity);}
 let mut s=SCHED.lock();
 let index=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty).ok_or(SchedulerError::UnknownTask)?;
 if s.tasks[index].state==TaskState::Running {
  let owner=s.current.iter().position(|current|*current==id).ok_or(SchedulerError::UnknownTask)?;
  if mask&(1u64<<owner)==0{return Err(SchedulerError::AffinityConflict);}
 }
 let ready=s.tasks[index].state==TaskState::Ready;
 if ready{remove_from_queues(&mut s,id);}
 s.tasks[index].affinity=mask;
 if ready{enqueue_ready(&mut s,index,None)?;}
 Ok(())
}
pub fn set_task_priority(id:u64,priority:u8)->Result<u8,SchedulerError>{
 if priority as usize>=PRIORITY_LEVELS{return Err(SchedulerError::InvalidPriority)}
 let mut s=SCHED.lock();
 let index=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty).ok_or(SchedulerError::UnknownTask)?;
 let old=s.tasks[index].priority;s.tasks[index].priority=priority;Ok(old)
}
pub fn task_snapshot(id:u64)->Result<Task,SchedulerError>{
 let s=SCHED.lock();s.tasks.iter().find(|t|t.id==id&&t.state!=TaskState::Empty).copied().ok_or(SchedulerError::UnknownTask)
}
pub fn has_runnable_for_cpu(cpu:usize,exclude_id:u64)->bool{
 if cpu>=MAX_CPUS{return false}
 let s=SCHED.lock();
 (0..MAX_CPUS).any(|source|s.queues[source].ids[..s.queues[source].len].iter().any(|id|
  *id!=exclude_id&&s.tasks.iter().any(|t|t.id==*id&&t.state==TaskState::Ready&&t.affinity&(1u64<<cpu)!=0)))
}
pub fn block_task(id:u64)->Result<(),SchedulerError>{
 let mut s=SCHED.lock();let index=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty).ok_or(SchedulerError::UnknownTask)?;
 match s.tasks[index].state{
  TaskState::Ready=>{remove_from_queues(&mut s,id);s.tasks[index].state=TaskState::Blocked;}
  TaskState::Running=>{let cpu=s.current.iter().position(|current|*current==id).ok_or(SchedulerError::UnknownTask)?;s.current[cpu]=0;s.tasks[index].state=TaskState::Blocked;}
  _=>return Err(SchedulerError::InvalidState),
 }Ok(())
}
pub fn wake_task(id:u64)->Result<(),SchedulerError>{
 let mut s=SCHED.lock();let index=s.tasks.iter().position(|t|t.id==id&&t.state!=TaskState::Empty).ok_or(SchedulerError::UnknownTask)?;
 if s.tasks[index].state!=TaskState::Blocked{return Err(SchedulerError::InvalidState)}
 s.tasks[index].state=TaskState::Ready;enqueue_ready(&mut s,index,None)
}

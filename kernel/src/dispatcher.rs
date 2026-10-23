//! Bounded dispatcher wait queue core. This records waits but does not suspend a CPU.
use crate::hal::IrqSpinLock;
pub const MAX_WAITERS: usize = 256;
pub const MAX_WAIT_GROUPS: usize = 64;
pub const MAX_GROUP_OBJECTS: usize = 64;
pub type WaitToken = u64;
pub const DELAY_OBJECT_PREFIX: u64 = 0xffff_ffff_0000_0000;
#[derive(Clone, Copy)]
pub struct ExpiredWait { pub thread:u64, pub object:u64 }
pub fn delay_object(thread:u64)->u64 { DELAY_OBJECT_PREFIX | (thread & 0xffff_ffff) }
pub fn is_delay_object(object:u64)->bool { object & 0xffff_ffff_0000_0000 == DELAY_OBJECT_PREFIX }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WaitState { Free=0, Queued=1, Signalled=2, TimedOut=3, Cancelled=4 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitError { InvalidObject, InvalidThread, Full, StaleToken }
#[derive(Clone, Copy)]
struct Record { object:u64, thread:u64, deadline:u64, generation:u32, state:WaitState }
impl Record { const EMPTY:Self=Self{object:0,thread:0,deadline:0,generation:0,state:WaitState::Free}; }
struct State { records:[Record;MAX_WAITERS], next_generation:u32 }
impl State { const fn new()->Self { Self{records:[Record::EMPTY;MAX_WAITERS],next_generation:1} } }
static QUEUES:IrqSpinLock<State>=IrqSpinLock::new(State::new());
fn token(i:usize,g:u32)->u64 { ((g as u64)<<32)|(i as u64+1) }
fn decode(t:u64)->Option<(usize,u32)> { let low=t as u32;let g=(t>>32) as u32;if low==0||g==0{return None;}let i=low as usize-1;(i<MAX_WAITERS).then_some((i,g)) }
/// Deadline zero means infinite; other deadlines are absolute monotonic ticks.
pub fn enqueue(object:u64,thread:u64,deadline:u64)->Result<WaitToken,WaitError>{if object==0{return Err(WaitError::InvalidObject)}if thread==0{return Err(WaitError::InvalidThread)}let mut s=QUEUES.lock();let i=s.records.iter().position(|r|r.state==WaitState::Free).ok_or(WaitError::Full)?;let g=s.next_generation.max(1);s.next_generation=g.wrapping_add(1).max(1);s.records[i]=Record{object,thread,deadline,generation:g,state:WaitState::Queued};Ok(token(i,g))}
/// Cancel a pending wait; stale tokens cannot cancel a reused slot.
pub fn cancel(t:u64)->Result<u64,WaitError>{let(i,g)=decode(t).ok_or(WaitError::StaleToken)?;let mut s=QUEUES.lock();let r=&mut s.records[i];if r.generation!=g||r.state!=WaitState::Queued{return Err(WaitError::StaleToken)}let id=r.thread;*r=Record::EMPTY;Ok(id)}
/// Wake the oldest queued waiter for an object; caller must ready that thread.
pub fn wake_one(object:u64)->Option<u64>{if object==0{return None}let mut s=QUEUES.lock();let i=s.records.iter().enumerate().filter(|(_,r)|r.state==WaitState::Queued&&r.object==object).min_by_key(|(_,r)|r.generation).map(|(i,_)|i)?;let id=s.records[i].thread;s.records[i]=Record::EMPTY;Some(id)}
/// Wake waiters into a caller-provided buffer. Returns the number written.
pub fn wake_all(object:u64,out:&mut[u64])->usize{if object==0||out.is_empty(){return 0}let mut s=QUEUES.lock();let mut n=0;while n<out.len(){let next=s.records.iter().enumerate().filter(|(_,r)|r.state==WaitState::Queued&&r.object==object).min_by_key(|(_,r)|r.generation).map(|(i,_)|i);let Some(i)=next else{break};out[n]=s.records[i].thread;n+=1;s.records[i]=Record::EMPTY;}n}
/// Expire waits whose finite absolute deadline is <= now; caller readies threads.
pub fn expire(now:u64,out:&mut[u64])->usize{if out.is_empty(){return 0}let mut s=QUEUES.lock();let mut n=0;for r in s.records.iter_mut(){if n==out.len(){break}if r.state==WaitState::Queued&&r.deadline!=0&&r.deadline<=now{out[n]=r.thread;n+=1;*r=Record::EMPTY}}n}
pub fn pending_count()->usize{QUEUES.lock().records.iter().filter(|r|r.state==WaitState::Queued).count()}
#[cfg(test)] mod tests { use super::*; #[test] fn fifo_and_cancel(){let a=enqueue(0x100,0x11,0).unwrap();let b=enqueue(0x100,0x22,0).unwrap();assert_eq!(wake_one(0x100),Some(0x11));assert_eq!(cancel(b),Ok(0x22));assert_eq!(cancel(a),Err(WaitError::StaleToken));} #[test] fn deadline(){let _=enqueue(0x200,0x31,100).unwrap();let _=enqueue(0x200,0x32,0).unwrap();let mut out=[0;4];assert_eq!(expire(99,&mut out),0);assert_eq!(expire(100,&mut out),1);assert_eq!(out[0],0x31);assert_eq!(wake_one(0x200),Some(0x32));} }

/// Expire waits while preserving the object key so delay timers can distinguish
/// successful delays from dispatcher timeouts.
pub fn expire_detailed(now:u64,out:&mut[ExpiredWait])->usize{
    if out.is_empty(){return 0}
    let mut s=QUEUES.lock();let mut n=0;
    for r in s.records.iter_mut(){
        if n==out.len(){break}
        if r.state==WaitState::Queued&&r.deadline!=0&&r.deadline<=now{
            out[n]=ExpiredWait{thread:r.thread,object:r.object};n+=1;*r=Record::EMPTY;
        }
    }
    n
}

#[derive(Clone, Copy)]
struct WaitGroup { active:bool, thread:u64, count:usize, objects:[u64;MAX_GROUP_OBJECTS], tokens:[WaitToken;MAX_GROUP_OBJECTS] }
impl WaitGroup { const EMPTY:Self=Self{active:false,thread:0,count:0,objects:[0;MAX_GROUP_OBJECTS],tokens:[0;MAX_GROUP_OBJECTS]}; }
struct GroupTable { groups:[WaitGroup;MAX_WAIT_GROUPS] }
impl GroupTable { const fn new()->Self{Self{groups:[WaitGroup::EMPTY;MAX_WAIT_GROUPS]}} }
static GROUPS:IrqSpinLock<GroupTable>=IrqSpinLock::new(GroupTable::new());

/// Register a wait-any group after all per-object queue entries have been inserted.
/// The caller must serialize object signal transitions during this registration.
pub fn register_wait_any(thread:u64,objects:&[u64],tokens:&[WaitToken])->Result<(),WaitError>{
    if thread==0{return Err(WaitError::InvalidThread)}
    if objects.is_empty()||objects.len()>MAX_GROUP_OBJECTS||objects.len()!=tokens.len(){return Err(WaitError::InvalidObject)}
    let mut groups=GROUPS.lock();
    let group=groups.groups.iter_mut().find(|g|!g.active).ok_or(WaitError::Full)?;
    group.active=true;group.thread=thread;group.count=objects.len();
    for i in 0..objects.len(){group.objects[i]=objects[i];group.tokens[i]=tokens[i];}
    Ok(())
}

/// Complete a registered wait-any group when one object signals or its deadline expires.
/// Returns the final NTSTATUS and cancels all sibling registrations. A stale dequeued
/// token is harmless because cancellation checks queue-slot generations.
pub fn complete_wait_group(thread:u64,object:u64,status:i32,success_status:i32,timeout_status:i32)->Option<i32>{
    let (result,tokens,count)={
        let mut groups=GROUPS.lock();
        let group=groups.groups.iter_mut().find(|g|g.active&&g.thread==thread)?;
        let result=if status==timeout_status {timeout_status} else {
            let index=group.objects[..group.count].iter().position(|candidate|*candidate==object)?;
            success_status+index as i32
        };
        let tokens=group.tokens;let count=group.count;
        *group=WaitGroup::EMPTY;
        (result,tokens,count)
    };
    for token in tokens[..count].iter().copied(){let _=cancel(token);}
    Some(result)
}

/// Remove a wait group after a scheduler/context-switch failure.
pub fn clear_wait_group(thread:u64){
    let mut groups=GROUPS.lock();
    if let Some(group)=groups.groups.iter_mut().find(|g|g.active&&g.thread==thread){*group=WaitGroup::EMPTY;}
}

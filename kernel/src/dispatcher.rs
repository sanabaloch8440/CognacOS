//! Bounded dispatcher wait queue core. This records waits but does not suspend a CPU.
use crate::hal::IrqSpinLock;
pub const MAX_WAITERS: usize = 256;
pub const MAX_WAIT_GROUPS: usize = 64;
pub const MAX_GROUP_OBJECTS: usize = 64;
pub type WaitToken = u64;
#[derive(Clone, Copy)]
pub struct WokenWaiter { pub thread:u64, pub timed_out:bool }
pub const DELAY_OBJECT_PREFIX: u64 = 0xffff_ffff_0000_0000;
pub const WAIT_ALL_TIMEOUT_PREFIX: u64 = 0xffff_fffe_0000_0000;
pub fn wait_all_timeout_object(thread:u64)->u64 { WAIT_ALL_TIMEOUT_PREFIX | (thread & 0xffff_ffff) }
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
pub fn wake_one_timed(object:u64)->Option<WokenWaiter>{
    if object==0{return None}
    let mut s=QUEUES.lock();
    let now=crate::apic::monotonic_ticks();
    let i=s.records.iter().enumerate().filter(|(_,r)|r.state==WaitState::Queued&&r.object==object).min_by_key(|(_,r)|r.generation).map(|(i,_)|i)?;
    let record=s.records[i];s.records[i]=Record::EMPTY;
    Some(WokenWaiter{thread:record.thread,timed_out:record.deadline!=0&&record.deadline<=now})
}
pub fn wake_one(object:u64)->Option<u64>{wake_one_timed(object).map(|w|w.thread)}
/// Wake waiters into a caller-provided buffer. Returns the number written.
pub fn wake_all_timed(object:u64,out:&mut[WokenWaiter])->usize{
    if object==0||out.is_empty(){return 0}
    let mut s=QUEUES.lock();let now=crate::apic::monotonic_ticks();let mut n=0;
    while n<out.len(){
        let next=s.records.iter().enumerate().filter(|(_,r)|r.state==WaitState::Queued&&r.object==object).min_by_key(|(_,r)|r.generation).map(|(i,_)|i);
        let Some(i)=next else{break};let record=s.records[i];s.records[i]=Record::EMPTY;
        out[n]=WokenWaiter{thread:record.thread,timed_out:record.deadline!=0&&record.deadline<=now};n+=1;
    }n
}
pub fn wake_all(object:u64,out:&mut[u64])->usize{
    if out.is_empty(){return 0}
    let mut timed=[WokenWaiter{thread:0,timed_out:false};MAX_WAITERS];
    let limit=out.len().min(timed.len());let n=wake_all_timed(object,&mut timed[..limit]);
    for i in 0..n{out[i]=timed[i].thread;}n
}
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
struct WaitGroup { active:bool, wait_all:bool, thread:u64, count:usize, token_count:usize, deadline:u64, objects:[u64;MAX_GROUP_OBJECTS], tokens:[WaitToken;MAX_GROUP_OBJECTS] }
impl WaitGroup { const EMPTY:Self=Self{active:false,wait_all:false,thread:0,count:0,token_count:0,deadline:0,objects:[0;MAX_GROUP_OBJECTS],tokens:[0;MAX_GROUP_OBJECTS]}; }
struct GroupTable { groups:[WaitGroup;MAX_WAIT_GROUPS] }
impl GroupTable { const fn new()->Self{Self{groups:[WaitGroup::EMPTY;MAX_WAIT_GROUPS]}} }
static GROUPS:IrqSpinLock<GroupTable>=IrqSpinLock::new(GroupTable::new());

/// Register a wait-any group after all per-object queue entries have been inserted.
/// The caller must serialize object signal transitions during this registration.
pub fn register_wait_any(thread:u64,objects:&[u64],tokens:&[WaitToken],deadline:u64)->Result<(),WaitError>{
    if thread==0{return Err(WaitError::InvalidThread)}
    if objects.is_empty()||objects.len()>MAX_GROUP_OBJECTS||objects.len()!=tokens.len(){return Err(WaitError::InvalidObject)}
    let mut groups=GROUPS.lock();
    let group=groups.groups.iter_mut().find(|g|!g.active).ok_or(WaitError::Full)?;
    group.active=true;group.wait_all=false;group.thread=thread;group.count=objects.len();group.token_count=tokens.len();group.deadline=deadline;
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
        let timed_out=status==timeout_status || (group.deadline!=0 && crate::apic::monotonic_ticks()>=group.deadline);
        let result=if timed_out {timeout_status} else {
            let index=group.objects[..group.count].iter().position(|candidate|*candidate==object)?;
            success_status+index as i32
        };
        if group.wait_all && status != timeout_status { return None; }
        let tokens=group.tokens;let count=group.token_count;
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

/// Register a wait-all group. Object signal transitions are serialized by the
/// caller while the group is installed, so no signal can slip between check and enqueue.
pub fn register_wait_all(thread:u64,objects:&[u64],timeout_token:Option<WaitToken>,deadline:u64)->Result<(),WaitError>{
    if thread==0{return Err(WaitError::InvalidThread)}
    if objects.is_empty()||objects.len()>MAX_GROUP_OBJECTS{return Err(WaitError::InvalidObject)}
    let mut groups=GROUPS.lock();
    let group=groups.groups.iter_mut().find(|g|!g.active).ok_or(WaitError::Full)?;
    group.active=true;group.wait_all=true;group.thread=thread;group.count=objects.len();group.deadline=deadline;
    group.token_count=if let Some(token)=timeout_token {group.tokens[0]=token;1}else{0};
    for i in 0..objects.len(){group.objects[i]=objects[i];}
    Ok(())
}

/// Atomically complete all wait-all groups whose objects are currently signalled.
/// Caller holds the global dispatcher-object state lock. Notification events remain
/// signalled; synchronization events and semaphores are consumed exactly once.
pub fn complete_ready_wait_all(
    out_threads:&mut[u64],
    is_signalled:fn(u64)->bool,
    consume:fn(u64),
)->usize{
    if out_threads.is_empty(){return 0}
    let mut cancel_tokens=[0u64;MAX_WAIT_GROUPS];let mut cancel_count=0usize;
    let mut count=0usize;
    {
        let mut groups=GROUPS.lock();
        for group in groups.groups.iter_mut(){
            if count==out_threads.len(){break}
            if !group.active||!group.wait_all{continue}
            if group.deadline!=0&&crate::apic::monotonic_ticks()>=group.deadline{continue}
            if !group.objects[..group.count].iter().copied().all(is_signalled){continue}
            for object in group.objects[..group.count].iter().copied(){consume(object);}
            out_threads[count]=group.thread;count+=1;
            if group.token_count>0&&cancel_count<cancel_tokens.len(){cancel_tokens[cancel_count]=group.tokens[0];cancel_count+=1;}
            *group=WaitGroup::EMPTY;
        }
    }
    for token in cancel_tokens[..cancel_count].iter().copied(){let _=cancel(token);}
    count
}


static SELFTEST_SIGNAL_A: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1);
static SELFTEST_SIGNAL_B: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(1);
fn selftest_is_signalled(object:u64)->bool{
    match object {
        0x301=>SELFTEST_SIGNAL_A.load(core::sync::atomic::Ordering::Acquire)>0,
        0x302=>SELFTEST_SIGNAL_B.load(core::sync::atomic::Ordering::Acquire)>0,
        _=>false,
    }
}
fn selftest_consume(object:u64){
    match object {
        0x301=>{SELFTEST_SIGNAL_A.fetch_sub(1,core::sync::atomic::Ordering::AcqRel);},
        0x302=>{SELFTEST_SIGNAL_B.fetch_sub(1,core::sync::atomic::Ordering::AcqRel);},
        _=>{},
    }
}

/// Boot-time validation for FIFO order, generation-safe cancellation, grouped
/// wait-any peer removal, wait-all signal consumption, and finite deadline expiry.
pub fn self_test()->bool{
    if pending_count()!=0{return false}
    let first=match enqueue(0x101,0x11,0){Ok(t)=>t,Err(_)=>return false};
    let second=match enqueue(0x101,0x22,0){Ok(t)=>t,Err(_)=>return false};
    if wake_one(0x101)!=Some(0x11)||cancel(second)!=Ok(0x22)||cancel(first)!=Err(WaitError::StaleToken){return false}

    let a=match enqueue(0x201,0x33,0){Ok(t)=>t,Err(_)=>return false};
    let b=match enqueue(0x202,0x33,0){Ok(t)=>t,Err(_)=>return false};
    if register_wait_any(0x33,&[0x201,0x202],&[a,b],0).is_err(){return false}
    if wake_one(0x202)!=Some(0x33){return false}
    if complete_wait_group(0x33,0x202,0,0,0x102)!=Some(1)||pending_count()!=0{return false}

    SELFTEST_SIGNAL_A.store(1,core::sync::atomic::Ordering::Release);
    SELFTEST_SIGNAL_B.store(1,core::sync::atomic::Ordering::Release);
    if register_wait_all(0x44,&[0x301,0x302],None,0).is_err(){return false}
    let mut ready=[0u64;4];
    let count=complete_ready_wait_all(&mut ready,selftest_is_signalled,selftest_consume);
    if count!=1||ready[0]!=0x44||SELFTEST_SIGNAL_A.load(core::sync::atomic::Ordering::Acquire)!=0||SELFTEST_SIGNAL_B.load(core::sync::atomic::Ordering::Acquire)!=0{return false}

    let deadline=crate::apic::monotonic_ticks();
    if enqueue(0x401,0x55,deadline).is_err(){return false}
    let mut expired=[ExpiredWait{thread:0,object:0};4];
    let count=expire_detailed(crate::apic::monotonic_ticks(),&mut expired);
    count==1&&expired[0].thread==0x55&&expired[0].object==0x401&&pending_count()==0
}

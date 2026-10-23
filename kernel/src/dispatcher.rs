//! Bounded dispatcher wait queue core. This records waits but does not suspend a CPU.
use crate::hal::IrqSpinLock;
pub const MAX_WAITERS: usize = 256;
pub type WaitToken = u64;
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
pub fn wake_one(object:u64)->Option<u64>{if object==0{return None}let mut s=QUEUES.lock();let i=s.records.iter().position(|r|r.state==WaitState::Queued&&r.object==object)?;let id=s.records[i].thread;s.records[i]=Record::EMPTY;Some(id)}
/// Wake waiters into a caller-provided buffer. Returns the number written.
pub fn wake_all(object:u64,out:&mut[u64])->usize{if object==0||out.is_empty(){return 0}let mut s=QUEUES.lock();let mut n=0;for r in s.records.iter_mut(){if n==out.len(){break}if r.state==WaitState::Queued&&r.object==object{out[n]=r.thread;n+=1;*r=Record::EMPTY}}n}
/// Expire waits whose finite absolute deadline is <= now; caller readies threads.
pub fn expire(now:u64,out:&mut[u64])->usize{if out.is_empty(){return 0}let mut s=QUEUES.lock();let mut n=0;for r in s.records.iter_mut(){if n==out.len(){break}if r.state==WaitState::Queued&&r.deadline!=0&&r.deadline<=now{out[n]=r.thread;n+=1;*r=Record::EMPTY}}n}
pub fn pending_count()->usize{QUEUES.lock().records.iter().filter(|r|r.state==WaitState::Queued).count()}
#[cfg(test)] mod tests { use super::*; #[test] fn fifo_and_cancel(){let a=enqueue(0x100,0x11,0).unwrap();let b=enqueue(0x100,0x22,0).unwrap();assert_eq!(wake_one(0x100),Some(0x11));assert_eq!(cancel(b),Ok(0x22));assert_eq!(cancel(a),Err(WaitError::StaleToken));} #[test] fn deadline(){let _=enqueue(0x200,0x31,100).unwrap();let _=enqueue(0x200,0x32,0).unwrap();let mut out=[0;4];assert_eq!(expire(99,&mut out),0);assert_eq!(expire(100,&mut out),1);assert_eq!(out[0],0x31);assert_eq!(wake_one(0x200),Some(0x32));} }

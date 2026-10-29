//! Hardware-independent keyboard class core: bounded packet queue and exclusive read ownership.
//! This is a CognacOS implementation of the class/port contract, not Microsoft's binary.
#![no_std]
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)] extern crate std;

pub const QUEUE_CAPACITY: usize = 256;
pub const KEY_BREAK: u16 = 0x0001;
pub const KEY_E0: u16 = 0x0002;
pub const KEY_E1: u16 = 0x0004;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct KeyboardInputData {
    pub unit_id: u16,
    pub make_code: u16,
    pub flags: u16,
    pub reserved: u16,
    pub extra_information: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassError { QueueFull, NotConnected, AlreadyConnected, AlreadyOpen, NotOpen, InvalidBuffer }

/// Fixed-capacity queue suitable for nonpaged storage. Callers serialize access at DISPATCH_LEVEL
/// or with the kernel's interrupt-safe queue lock; this primitive itself never allocates/blocks.
pub struct InputQueue {
    entries: [KeyboardInputData; QUEUE_CAPACITY],
    head: usize,
    len: usize,
    connected: bool,
    opened: bool,
    dropped: u64,
}
impl InputQueue {
    pub const fn new() -> Self {
        Self { entries: [KeyboardInputData { unit_id: 0, make_code: 0, flags: 0, reserved: 0, extra_information: 0 }; QUEUE_CAPACITY], head: 0, len: 0, connected: false, opened: false, dropped: 0 }
    }
    pub fn connect(&mut self) -> Result<(), ClassError> {
        if self.connected { return Err(ClassError::AlreadyConnected); }
        self.connected = true; Ok(())
    }
    pub fn disconnect(&mut self) { self.connected = false; self.opened = false; self.head = 0; self.len = 0; }
    pub fn open_exclusive(&mut self) -> Result<(), ClassError> {
        if !self.connected { return Err(ClassError::NotConnected); }
        if self.opened { return Err(ClassError::AlreadyOpen); }
        self.opened = true; Ok(())
    }
    pub fn close(&mut self) { self.opened = false; }
    pub fn push(&mut self, packet: KeyboardInputData) -> Result<(), ClassError> {
        if !self.connected { return Err(ClassError::NotConnected); }
        if self.len == QUEUE_CAPACITY { self.dropped = self.dropped.saturating_add(1); return Err(ClassError::QueueFull); }
        let tail = (self.head + self.len) % QUEUE_CAPACITY;
        self.entries[tail] = packet; self.len += 1; Ok(())
    }
    pub fn read(&mut self, out: &mut [KeyboardInputData]) -> Result<usize, ClassError> {
        if !self.opened { return Err(ClassError::NotOpen); }
        if out.is_empty() { return Err(ClassError::InvalidBuffer); }
        let n = out.len().min(self.len);
        for slot in out.iter_mut().take(n) { *slot = self.entries[self.head]; self.head = (self.head + 1) % QUEUE_CAPACITY; self.len -= 1; }
        Ok(n)
    }
    pub const fn queued(&self) -> usize { self.len }
    pub const fn dropped(&self) -> u64 { self.dropped }
}
impl Default for InputQueue { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn exclusive_open_and_packet_read() {
  let mut q=InputQueue::new(); assert_eq!(q.open_exclusive(),Err(ClassError::NotConnected));
  q.connect().unwrap(); q.open_exclusive().unwrap(); assert_eq!(q.open_exclusive(),Err(ClassError::AlreadyOpen));
  q.push(KeyboardInputData{make_code:0x1e,..Default::default()}).unwrap();
  let mut out=[KeyboardInputData::default();2]; assert_eq!(q.read(&mut out),Ok(1)); assert_eq!(out[0].make_code,0x1e);
 }
 #[test] fn queue_is_bounded_and_counts_drops() {
  let mut q=InputQueue::new(); q.connect().unwrap();
  for i in 0..QUEUE_CAPACITY { q.push(KeyboardInputData{make_code:i as u16,..Default::default()}).unwrap(); }
  assert_eq!(q.push(KeyboardInputData::default()),Err(ClassError::QueueFull)); assert_eq!(q.dropped(),1);
 }
}

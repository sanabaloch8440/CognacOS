//! Bounded, allocation-free kernel event trace for post-failure diagnosis.
//!
//! Records are best-effort: writers never allocate or wait on a global lock.
//! A slot's published sequence is checked before and after reads so incomplete
//! or overwritten records are never reported as valid evidence.
use core::sync::atomic::{AtomicU64, Ordering};

pub const CAPACITY: usize = 64;
pub const EVENT_EXCEPTION: u32 = 1;
pub const EVENT_PANIC: u32 = 2;
pub const EVENT_PAGE_FAULT: u32 = 3;
pub const EVENT_VM_FAILURE: u32 = 4;
const BUSY: u64 = 1 << 63;

struct Slot {
    published: AtomicU64,
    event: AtomicU64,
    arg0: AtomicU64,
    arg1: AtomicU64,
    instruction_pointer: AtomicU64,
}
impl Slot {
    const fn new() -> Self {
        Self {
            published: AtomicU64::new(0),
            event: AtomicU64::new(0),
            arg0: AtomicU64::new(0),
            arg1: AtomicU64::new(0),
            instruction_pointer: AtomicU64::new(0),
        }
    }
}

static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static SLOTS: [Slot; CAPACITY] = [const { Slot::new() }; CAPACITY];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventRecord {
    pub sequence: u64,
    pub event: u32,
    pub arg0: u64,
    pub arg1: u64,
    pub instruction_pointer: u64,
}

/// Publish a diagnostic event without heap allocation or a global lock.
/// arg0 and arg1 are event-specific; rip is zero if unavailable.
pub fn record(event: u32, arg0: u64, arg1: u64, rip: u64) {
    let sequence = NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    if sequence == 0 || sequence >= BUSY { return; }
    let slot = &SLOTS[sequence as usize % CAPACITY];
    // Claim the slot so a delayed writer cannot overwrite a newer record.
    let mut observed = slot.published.load(Ordering::Acquire);
    loop {
        if observed & BUSY != 0 { return; }
        if observed >= sequence { return; }
        match slot.published.compare_exchange_weak(
            observed, BUSY | sequence, Ordering::AcqRel, Ordering::Acquire
        ) {
            Ok(_) => break,
            Err(actual) => observed = actual,
        }
    }
    slot.event.store(event as u64, Ordering::Relaxed);
    slot.arg0.store(arg0, Ordering::Relaxed);
    slot.arg1.store(arg1, Ordering::Relaxed);
    slot.instruction_pointer.store(rip, Ordering::Relaxed);
    slot.published.store(sequence, Ordering::Release);
}

/// Read a record only if its slot still contains the requested sequence.
pub fn read(sequence: u64) -> Option<EventRecord> {
    if sequence == 0 { return None; }
    let slot = &SLOTS[sequence as usize % CAPACITY];
    if slot.published.load(Ordering::Acquire) != sequence { return None; }
    let event = slot.event.load(Ordering::Relaxed) as u32;
    let arg0 = slot.arg0.load(Ordering::Relaxed);
    let arg1 = slot.arg1.load(Ordering::Relaxed);
    let instruction_pointer = slot.instruction_pointer.load(Ordering::Relaxed);
    if slot.published.load(Ordering::Acquire) != sequence { return None; }
    Some(EventRecord { sequence, event, arg0, arg1, instruction_pointer })
}

pub fn latest_sequence() -> u64 {
    NEXT_SEQUENCE.load(Ordering::Acquire).saturating_sub(1)
}

/// Print recent trace records to the early serial console during fatal handling.
pub fn dump_serial(limit: usize) {
    let latest = latest_sequence();
    let count = limit.min(CAPACITY).min(latest as usize);
    let first = latest.saturating_sub(count as u64).saturating_add(1);
    unsafe { crate::serial_write(b"\r\nRecent kernel trace (seq event arg0 arg1 RIP):\r\n"); }
    for sequence in first..=latest {
        if let Some(record) = read(sequence) {
            unsafe {
                crate::serial_write(b"TRACE ");
                crate::serial_write_hex(record.sequence);
                crate::serial_write(b" ");
                crate::serial_write_hex(record.event as u64);
                crate::serial_write(b" ");
                crate::serial_write_hex(record.arg0);
                crate::serial_write(b" ");
                crate::serial_write_hex(record.arg1);
                crate::serial_write(b" ");
                crate::serial_write_hex(record.instruction_pointer);
                crate::serial_write(b"\r\n");
            }
        }
    }
}

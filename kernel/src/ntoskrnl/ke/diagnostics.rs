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

/// Decoded architectural x86-64 page-fault error-code fields.
/// This is pure bit decoding: safe to use in the allocation-free exception path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFaultInfo {
    /// P=1: protection violation; P=0: translation not present.
    pub protection_violation: bool,
    /// W/R=1 means the access attempted a write.
    pub write: bool,
    /// U/S=1 means the access originated at CPL3.
    pub user: bool,
    /// RSVD=1 means a reserved paging-structure bit was set.
    pub reserved_bit: bool,
    /// I/D=1 means the fault was caused by instruction fetch.
    pub instruction_fetch: bool,
    /// PK=1 means a protection-key check caused the fault.
    pub protection_key: bool,
    /// SS=1 means the fault involved a shadow-stack access.
    pub shadow_stack: bool,
    /// SGX=1 means the fault is associated with SGX.
    pub sgx: bool,
}

pub const fn decode_page_fault_error(error: u64) -> PageFaultInfo {
    PageFaultInfo {
        protection_violation: error & (1 << 0) != 0,
        write: error & (1 << 1) != 0,
        user: error & (1 << 2) != 0,
        reserved_bit: error & (1 << 3) != 0,
        instruction_fetch: error & (1 << 4) != 0,
        protection_key: error & (1 << 5) != 0,
        shadow_stack: error & (1 << 6) != 0,
        sgx: error & (1 << 15) != 0,
    }
}

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
#[cfg(not(test))]
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


#[cfg(test)]
mod tests {
    use super::*;
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn page_fault_error_code_decodes_access_and_fault_reason_bits() {
        let not_present_read = decode_page_fault_error(0);
        assert!(!not_present_read.protection_violation);
        assert!(!not_present_read.write);
        assert!(!not_present_read.user);
        assert!(!not_present_read.reserved_bit);
        assert!(!not_present_read.instruction_fetch);

        let user_write_protection = decode_page_fault_error(0b00111);
        assert!(user_write_protection.protection_violation);
        assert!(user_write_protection.write);
        assert!(user_write_protection.user);

        let reserved_instruction = decode_page_fault_error((1 << 3) | (1 << 4));
        assert!(reserved_instruction.reserved_bit);
        assert!(reserved_instruction.instruction_fetch);

        let extended = decode_page_fault_error((1 << 5) | (1 << 6) | (1 << 15));
        assert!(extended.protection_key);
        assert!(extended.shadow_stack);
        assert!(extended.sgx);
    }

    #[test]
    fn trace_records_publish_and_old_slots_are_rejected_after_wraparound() {
        let _guard = TEST_LOCK.lock().unwrap();
        assert!(read(0).is_none());
        let before = latest_sequence();
        record(EVENT_EXCEPTION, 0x21, 0x22, 0x1234_5678);
        let first = before + 1;
        assert_eq!(latest_sequence(), first);
        assert_eq!(read(first), Some(EventRecord {
            sequence: first,
            event: EVENT_EXCEPTION,
            arg0: 0x21,
            arg1: 0x22,
            instruction_pointer: 0x1234_5678,
        }));

        for index in 0..=CAPACITY {
            record(EVENT_VM_FAILURE, index as u64, !index as u64, 0xfeed_0000 + index as u64);
        }
        let latest = latest_sequence();
        assert!(latest >= first + CAPACITY as u64 + 1);
        assert!(read(first).is_none(), "overwritten sequence must not be returned");
        let last = read(latest).expect("latest published event must be readable");
        assert_eq!(last.sequence, latest);
        assert_eq!(last.event, EVENT_VM_FAILURE);
        assert_eq!(last.arg0, CAPACITY as u64);
        assert_eq!(last.arg1, !(CAPACITY as u64));
        assert_eq!(last.instruction_pointer, 0xfeed_0000 + CAPACITY as u64);
    }

    #[test]
    fn concurrent_writers_never_publish_torn_event_payloads() {
        let _guard = TEST_LOCK.lock().unwrap();
        const WORKERS: usize = 8;
        const EVENTS_PER_WORKER: usize = 1000;
        let workers: std::vec::Vec<_> = (0..WORKERS).map(|worker| {
            std::thread::spawn(move || {
                for index in 0..EVENTS_PER_WORKER {
                    let payload = ((worker as u64) << 32) | index as u64;
                    record(EVENT_EXCEPTION, payload, !payload, payload ^ 0xa5a5_5a5a);
                }
            })
        }).collect();
        for worker in workers { worker.join().expect("trace writer thread panicked"); }

        let latest = latest_sequence();
        let first = latest.saturating_sub(CAPACITY as u64 - 1).max(1);
        let mut valid_records = 0usize;
        for sequence in first..=latest {
            if let Some(event) = read(sequence) {
                assert_eq!(event.sequence, sequence);
                assert_eq!(event.event, EVENT_EXCEPTION);
                assert_eq!(event.arg1, !event.arg0, "payload fields must come from one writer");
                assert_eq!(event.instruction_pointer, event.arg0 ^ 0xa5a5_5a5a);
                valid_records += 1;
            }
        }
        assert!(valid_records > 0, "stress test should publish readable records");
    }
}

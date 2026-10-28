//! NTOSKRNL Debug Subsystem: bounded structured tracing and dynamic component filters.
//!
//! This is the kernel-side equivalent of a small tracefs/dynamic-debug core:
//! events are allocation-free, best-effort, sequence-stamped, and readable after
//! a failure. It is not a live KD transport and never dereferences event payloads.
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub const TRACE_CAPACITY: usize = 128;
pub const COMPONENT_COUNT: usize = 256;

pub const LEVEL_ERROR: u32 = 0;
pub const LEVEL_WARNING: u32 = 1;
pub const LEVEL_INFO: u32 = 2;
pub const LEVEL_TRACE: u32 = 3;

pub const COMPONENT_DEFAULT: u32 = 0;
pub const COMPONENT_IO: u32 = 1;
pub const COMPONENT_DRIVER: u32 = 2;
pub const COMPONENT_MEMORY: u32 = 3;
pub const COMPONENT_SCHEDULER: u32 = 4;
pub const COMPONENT_EXCEPTION: u32 = 5;

pub const EVENT_DEBUG_PRINT: u32 = 1;
pub const EVENT_IO_CALL_REJECTED: u32 = 0x1001;
pub const EVENT_DRIVER_LOAD: u32 = 0x2001;
pub const EVENT_DRIVER_DISPATCH: u32 = 0x2002;
pub const EVENT_EXCEPTION: u32 = 0x3001;
pub const EVENT_PAGE_FAULT: u32 = 0x3002;
pub const EVENT_KERNEL_PANIC: u32 = 0x3003;

const BUSY: u64 = 1 << 63;
const DEFAULT_MASK: u32 = (1 << LEVEL_ERROR) | (1 << LEVEL_WARNING);

struct Slot {
    published: AtomicU64,
    component: AtomicU32,
    level: AtomicU32,
    event: AtomicU32,
    _reserved: AtomicU32,
    arg0: AtomicU64,
    arg1: AtomicU64,
    rip: AtomicU64,
}
impl Slot {
    const fn new() -> Self {
        Self {
            published: AtomicU64::new(0),
            component: AtomicU32::new(0),
            level: AtomicU32::new(0),
            event: AtomicU32::new(0),
            _reserved: AtomicU32::new(0),
            arg0: AtomicU64::new(0),
            arg1: AtomicU64::new(0),
            rip: AtomicU64::new(0),
        }
    }
}

static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static SLOTS: [Slot; TRACE_CAPACITY] = [const { Slot::new() }; TRACE_CAPACITY];
static FILTERS: [AtomicU32; COMPONENT_COUNT] =
    [const { AtomicU32::new(DEFAULT_MASK) }; COMPONENT_COUNT];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceRecord {
    pub sequence: u64,
    pub component: u32,
    pub level: u32,
    pub event: u32,
    pub arg0: u64,
    pub arg1: u64,
    pub instruction_pointer: u64,
}

#[inline]
fn component_slot(component: u32) -> usize {
    (component as usize).min(COMPONENT_COUNT - 1)
}

/// Set or clear one debug level for a component. Levels are bit positions 0..31.
pub fn set_filter(component: u32, level: u32, enabled: bool) -> bool {
    if level >= 32 { return false; }
    let mask = &FILTERS[component_slot(component)];
    let bit = 1u32 << level;
    if enabled { mask.fetch_or(bit, Ordering::AcqRel); }
    else { mask.fetch_and(!bit, Ordering::AcqRel); }
    true
}

pub fn query_filter(component: u32, level: u32) -> bool {
    if level >= 32 { return false; }
    FILTERS[component_slot(component)].load(Ordering::Acquire) & (1u32 << level) != 0
}

/// Record only when the component/level filter is enabled. No allocation or locks.
pub fn trace(component: u32, level: u32, event: u32, arg0: u64, arg1: u64, rip: u64) {
    if query_filter(component, level) {
        record_unfiltered(component, level, event, arg0, arg1, rip);
    }
}

/// Record a critical event regardless of filters (fatal paths and guard failures).
pub fn record_unfiltered(component: u32, level: u32, event: u32, arg0: u64, arg1: u64, rip: u64) {
    let sequence = NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    if sequence == 0 || sequence >= BUSY { return; }
    let slot = &SLOTS[sequence as usize % TRACE_CAPACITY];
    let mut observed = slot.published.load(Ordering::Acquire);
    loop {
        if observed & BUSY != 0 || observed >= sequence { return; }
        match slot.published.compare_exchange_weak(
            observed, BUSY | sequence, Ordering::AcqRel, Ordering::Acquire
        ) {
            Ok(_) => break,
            Err(actual) => observed = actual,
        }
    }
    slot.component.store(component, Ordering::Relaxed);
    slot.level.store(level, Ordering::Relaxed);
    slot.event.store(event, Ordering::Relaxed);
    slot.arg0.store(arg0, Ordering::Relaxed);
    slot.arg1.store(arg1, Ordering::Relaxed);
    slot.rip.store(rip, Ordering::Relaxed);
    slot.published.store(sequence, Ordering::Release);
}

pub fn read(sequence: u64) -> Option<TraceRecord> {
    if sequence == 0 { return None; }
    let slot = &SLOTS[sequence as usize % TRACE_CAPACITY];
    if slot.published.load(Ordering::Acquire) != sequence { return None; }
    let record = TraceRecord {
        sequence,
        component: slot.component.load(Ordering::Relaxed),
        level: slot.level.load(Ordering::Relaxed),
        event: slot.event.load(Ordering::Relaxed),
        arg0: slot.arg0.load(Ordering::Relaxed),
        arg1: slot.arg1.load(Ordering::Relaxed),
        instruction_pointer: slot.rip.load(Ordering::Relaxed),
    };
    if slot.published.load(Ordering::Acquire) != sequence { return None; }
    Some(record)
}

pub fn latest_sequence() -> u64 {
    NEXT_SEQUENCE.load(Ordering::Acquire).saturating_sub(1)
}

const PERSIST_MAGIC: &[u8; 8] = b"COGTRC01";
const PERSIST_HEADER: usize = 32;
const PERSIST_RECORD: usize = 48;
const PERSIST_BYTES: usize = PERSIST_HEADER + TRACE_CAPACITY * PERSIST_RECORD;
static PERSIST_BUSY: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static mut PERSIST_BUFFER: [u8; PERSIST_BYTES] = [0; PERSIST_BYTES];

fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 { crc = (crc >> 1) ^ (0xedb8_8320u32 & (0u32.wrapping_sub(crc & 1))); }
    }
    !crc
}
fn put_u32(buf: &mut [u8], at: usize, value: u32) { buf[at..at+4].copy_from_slice(&value.to_le_bytes()); }
fn put_u64(buf: &mut [u8], at: usize, value: u64) { buf[at..at+8].copy_from_slice(&value.to_le_bytes()); }
fn get_u32(buf: &[u8], at: usize) -> u32 { u32::from_le_bytes(buf[at..at+4].try_into().unwrap_or([0;4])) }
fn get_u64(buf: &[u8], at: usize) -> u64 { u64::from_le_bytes(buf[at..at+8].try_into().unwrap_or([0;8])) }

/// Persist the current bounded trace snapshot to the explicitly reserved GPT
/// debug partition. Returns false if storage/partition is unavailable.
pub fn persist_to_storage() -> bool {
    if PERSIST_BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { return false; }
    let result = unsafe {
        let ptr = core::ptr::addr_of_mut!(PERSIST_BUFFER).cast::<u8>();
        core::ptr::write_bytes(ptr, 0, PERSIST_BYTES);
        let buf = core::slice::from_raw_parts_mut(ptr, PERSIST_BYTES);
        buf[..8].copy_from_slice(PERSIST_MAGIC);
        put_u32(buf, 8, 1);
        let latest = latest_sequence();
        let count = (latest.min(TRACE_CAPACITY as u64)) as usize;
        put_u32(buf, 12, count as u32);
        put_u64(buf, 16, latest);
        let first = latest.saturating_sub(count as u64).saturating_add(1);
        let mut written = 0usize;
        if count != 0 {
            for seq in first..=latest {
                if let Some(record) = read(seq) {
                    let at = PERSIST_HEADER + written * PERSIST_RECORD;
                    put_u64(buf, at, record.sequence);
                    put_u32(buf, at+8, record.component);
                    put_u32(buf, at+12, record.level);
                    put_u32(buf, at+16, record.event);
                    put_u32(buf, at+20, 0);
                    put_u64(buf, at+24, record.arg0);
                    put_u64(buf, at+32, record.arg1);
                    put_u64(buf, at+40, record.instruction_pointer);
                    written += 1;
                }
            }
        }
        put_u32(buf, 12, written as u32);
        let crc = checksum(&buf[PERSIST_HEADER..PERSIST_HEADER + written * PERSIST_RECORD]);
        put_u32(buf, 24, crc);
        put_u32(buf, 28, 0);
        crate::ata_disk::debug_trace_store_write(0, &buf[..PERSIST_HEADER + written * PERSIST_RECORD])
    };
    PERSIST_BUSY.store(false, Ordering::Release);
    result
}

/// Restore validated records from the reserved GPT debug partition. Records
/// are replayed into the volatile ring with fresh sequence numbers.
pub fn restore_from_storage() -> bool {
    if PERSIST_BUSY.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { return false; }
    let result = unsafe {
        let ptr = core::ptr::addr_of_mut!(PERSIST_BUFFER).cast::<u8>();
        let buf = core::slice::from_raw_parts_mut(ptr, PERSIST_BYTES);
        if !crate::ata_disk::debug_trace_store_read(0, &mut buf[..PERSIST_HEADER]) ||
            &buf[..8] != PERSIST_MAGIC || get_u32(buf, 8) != 1 {
            false
        } else {
            let count = get_u32(buf, 12) as usize;
            let size = PERSIST_HEADER.saturating_add(count.saturating_mul(PERSIST_RECORD));
            if count > TRACE_CAPACITY || size > PERSIST_BYTES ||
                !crate::ata_disk::debug_trace_store_read(0, &mut buf[..size]) ||
                checksum(&buf[PERSIST_HEADER..size]) != get_u32(buf, 24) {
                false
            } else {
                for index in 0..count {
                    let at = PERSIST_HEADER + index * PERSIST_RECORD;
                    record_unfiltered(get_u32(buf, at+8), get_u32(buf, at+12), get_u32(buf, at+16),
                        get_u64(buf, at+24), get_u64(buf, at+32), get_u64(buf, at+40));
                }
                true
            }
        }
    };
    PERSIST_BUSY.store(false, Ordering::Release);
    result
}

/// Dump recent structured events to the early UART. Safe for fatal paths; bounded.
#[cfg(not(test))]
pub fn dump_serial(limit: usize) {
    let latest = latest_sequence();
    let count = limit.min(TRACE_CAPACITY).min(latest as usize);
    if count == 0 { return; }
    let first = latest.saturating_sub(count as u64).saturating_add(1);
    unsafe { crate::serial_write(b"\\r\\nNTOSKRNL DEBUG TRACE: seq component level event arg0 arg1 RIP\\r\\n"); }
    for sequence in first..=latest {
        if let Some(r) = read(sequence) {
            unsafe {
                crate::serial_write(b"DBG ");
                crate::serial_write_hex(r.sequence);
                crate::serial_write(b" ");
                crate::serial_write_hex(r.component as u64);
                crate::serial_write(b" ");
                crate::serial_write_hex(r.level as u64);
                crate::serial_write(b" ");
                crate::serial_write_hex(r.event as u64);
                crate::serial_write(b" ");
                crate::serial_write_hex(r.arg0);
                crate::serial_write(b" ");
                crate::serial_write_hex(r.arg1);
                crate::serial_write(b" ");
                crate::serial_write_hex(r.instruction_pointer);
                crate::serial_write(b"\\r\\n");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn component_filters_toggle_independent_levels() {
        let _guard = TEST_LOCK.lock().unwrap();
        assert!(query_filter(COMPONENT_IO, LEVEL_ERROR));
        assert!(!query_filter(COMPONENT_IO, LEVEL_TRACE));
        assert!(set_filter(COMPONENT_IO, LEVEL_TRACE, true));
        assert!(query_filter(COMPONENT_IO, LEVEL_TRACE));
        assert!(set_filter(COMPONENT_IO, LEVEL_TRACE, false));
        assert!(!query_filter(COMPONENT_IO, LEVEL_TRACE));
        assert!(!set_filter(COMPONENT_IO, 32, true));
    }

    #[test]
    fn structured_records_round_trip_and_expired_sequences_are_rejected() {
        let _guard = TEST_LOCK.lock().unwrap();
        let before = latest_sequence();
        record_unfiltered(COMPONENT_IO, LEVEL_ERROR, EVENT_IO_CALL_REJECTED, 0x1234, 0x5678, 0xfeed);
        let sequence = before + 1;
        assert_eq!(read(sequence), Some(TraceRecord {
            sequence, component: COMPONENT_IO, level: LEVEL_ERROR,
            event: EVENT_IO_CALL_REJECTED, arg0: 0x1234, arg1: 0x5678,
            instruction_pointer: 0xfeed,
        }));
        for i in 0..=TRACE_CAPACITY {
            record_unfiltered(COMPONENT_DRIVER, LEVEL_INFO, EVENT_DRIVER_DISPATCH, i as u64, !i as u64, 0);
        }
        assert!(read(sequence).is_none(), "overwritten records must not masquerade as current data");
    }

    #[test]
    fn filtered_events_do_not_consume_ring_slots() {
        let _guard = TEST_LOCK.lock().unwrap();
        set_filter(COMPONENT_SCHEDULER, LEVEL_TRACE, false);
        let before = latest_sequence();
        trace(COMPONENT_SCHEDULER, LEVEL_TRACE, 0x4001, 1, 2, 3);
        assert_eq!(latest_sequence(), before);
        set_filter(COMPONENT_SCHEDULER, LEVEL_TRACE, true);
        trace(COMPONENT_SCHEDULER, LEVEL_TRACE, 0x4001, 1, 2, 3);
        assert_eq!(latest_sequence(), before + 1);
        set_filter(COMPONENT_SCHEDULER, LEVEL_TRACE, false);
    }
}

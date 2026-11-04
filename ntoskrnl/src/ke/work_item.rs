//! Bounded PASSIVE_LEVEL executive work queue.
//!
//! Producers may enqueue from ISR/DPC-safe contexts. Only the dedicated
//! kernel worker invokes callbacks, and shutdown closes admission, discards
//! queued callbacks, and waits for an active callback to return before the
//! caller may release its context.
use core::ffi::c_void;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::hal::IrqSpinLock;

pub const MAX_WORK_ITEMS: usize = 64;
pub type WorkRoutine = extern "C" fn(u64);

#[derive(Clone, Copy)]
struct WorkRecord { routine: Option<WorkRoutine>, context: u64 }
impl WorkRecord { const EMPTY: Self = Self { routine: None, context: 0 }; }

struct QueueState {
    entries: [WorkRecord; MAX_WORK_ITEMS],
    head: usize,
    tail: usize,
    count: usize,
    active: usize,
    active_context: u64,
    closed_contexts: [u64; 16],
    accepting: bool,
}
impl QueueState {
    const fn new() -> Self {
        Self { entries: [WorkRecord::EMPTY; MAX_WORK_ITEMS], head: 0, tail: 0,
            count: 0, active: 0, active_context: 0, closed_contexts: [0; 16], accepting: false }
    }
    fn push(&mut self, item: WorkRecord) -> bool {
        if !self.accepting || self.count == MAX_WORK_ITEMS
            || self.closed_contexts.contains(&item.context) { return false; }
        self.entries[self.tail] = item;
        self.tail = (self.tail + 1) % MAX_WORK_ITEMS;
        self.count += 1;
        true
    }
    fn pop(&mut self) -> Option<WorkRecord> {
        if self.count == 0 { return None; }
        let item = self.entries[self.head];
        self.entries[self.head] = WorkRecord::EMPTY;
        self.head = (self.head + 1) % MAX_WORK_ITEMS;
        self.count -= 1;
        self.active += 1;
        self.active_context = item.context;
        Some(item)
    }
}
static QUEUE: IrqSpinLock<QueueState> = IrqSpinLock::new(QueueState::new());
static WORKER_ID: AtomicU64 = AtomicU64::new(0);
static mut WORKER_STACK: [u8; 16 * 1024] = [0; 16 * 1024];

extern "C" fn worker_entry(_argument: *mut c_void) {
    loop {
        let item = {
            let mut state = QUEUE.lock();
            match state.pop() {
                Some(item) => Some(item),
                None if !state.accepting => return,
                None => None,
            }
        };
        if let Some(item) = item {
            if let Some(routine) = item.routine { routine(item.context); }
            let mut state = QUEUE.lock();
            state.active = state.active.saturating_sub(1);
            if state.active == 0 { state.active_context = 0; }
            continue;
        }
        // The queue insertion and thread wake are ordered so a producer racing
        // this block sets the thread's wake-pending bit instead of losing work.
        let _ = crate::thread::block_current();
    }
}

/// Start the single PASSIVE_LEVEL worker after scheduler/thread initialization.
/// The static stack and worker record remain resident until kernel shutdown.
pub fn initialize() -> bool {
    if WORKER_ID.load(Ordering::Acquire) != 0 {
        return true;
    }
    {
        let mut state = QUEUE.lock();
        if state.active != 0 || state.count != 0 { return false; }
        state.accepting = true;
    }
    let stack = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(WORKER_STACK).cast::<u8>(), 16 * 1024)
    };
    match crate::thread::spawn(stack, 1, worker_entry, core::ptr::null_mut()) {
        Ok(id) => {
            WORKER_ID.store(id, Ordering::Release);
            true
        }
        Err(_) => {
            QUEUE.lock().accepting = false;
            false
        }
    }
}

/// Queue one callback for execution by the PASSIVE_LEVEL worker.
/// Returns false if the queue is not initialized, is rundown, or is full.
pub fn queue(routine: WorkRoutine, context: u64) -> bool {
    let id = WORKER_ID.load(Ordering::Acquire);
    if id == 0 { return false; }
    if !QUEUE.lock().push(WorkRecord { routine: Some(routine), context }) { return false; }
    crate::thread::wake(id).is_ok()
}

/// Close queue admission, cancel records that have not started, wake the worker,
/// and wait until the active callback and worker thread have exited. Must run
/// at PASSIVE_LEVEL and must not be called by the worker itself.
pub fn rundown() -> bool {
    let id = WORKER_ID.load(Ordering::Acquire);
    if id == 0 { return true; }
    if crate::thread::current_thread_id() == Some(id) { return false; }
    {
        let mut state = QUEUE.lock();
        state.accepting = false;
        state.entries = [WorkRecord::EMPTY; MAX_WORK_ITEMS];
        state.head = 0;
        state.tail = 0;
        state.count = 0;
    }
    let _ = crate::thread::wake(id);
    let mut spins = 0usize;
    while spins < 10_000_000 {
        let active = QUEUE.lock().active;
        if active == 0 && crate::thread::state(id) == Some(crate::thread::ThreadState::Exited) {
            WORKER_ID.store(0, Ordering::Release);
            return true;
        }
        core::hint::spin_loop();
        spins += 1;
    }
    false
}

/// Queue adapter suitable for a DPC callback. The callback itself performs no
/// AML execution and only publishes a bounded record to the passive worker.
pub extern "C" fn queue_from_dpc(context: u64, routine: WorkRoutine) -> bool {
    queue(routine, context)
}

/// Permanently close one driver's work-item admission, cancel its queued items,
/// and wait until its currently executing callback returns. The callback context
/// may be reclaimed only after this function succeeds.
pub extern "C" fn rundown_context(context: u64) -> bool {
    if context == 0 || crate::thread::current_thread_id() == Some(WORKER_ID.load(Ordering::Acquire)) {
        return false;
    }
    {
        let mut state = QUEUE.lock();
        if !state.closed_contexts.contains(&context) {
            let Some(slot) = state.closed_contexts.iter_mut().find(|slot| **slot == 0) else { return false; };
            *slot = context;
        }
        let mut retained = [WorkRecord::EMPTY; MAX_WORK_ITEMS];
        let mut retained_count = 0usize;
        for offset in 0..state.count {
            let index = (state.head + offset) % MAX_WORK_ITEMS;
            let item = state.entries[index];
            if item.context != context {
                retained[retained_count] = item;
                retained_count += 1;
            }
        }
        state.entries = retained;
        state.head = 0;
        state.tail = retained_count % MAX_WORK_ITEMS;
        state.count = retained_count;
    }
    let id = WORKER_ID.load(Ordering::Acquire);
    if id != 0 { let _ = crate::thread::wake(id); }
    let mut spins = 0usize;
    while QUEUE.lock().active_context == context && spins < 10_000_000 {
        core::hint::spin_loop();
        spins += 1;
    }
    QUEUE.lock().active_context != context
}

/// Reopen a context slot before registering a newly initialized ACPI instance.
pub extern "C" fn allow_context(context: u64) -> bool {
    if context == 0 { return false; }
    let mut state = QUEUE.lock();
    if let Some(slot) = state.closed_contexts.iter_mut().find(|slot| **slot == context) {
        *slot = 0;
    }
    state.accepting
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_is_bounded_and_rejects_after_rundown() {
        let mut state = QueueState::new();
        state.accepting = true;
        let noop: WorkRoutine = |_| {};
        for index in 0..MAX_WORK_ITEMS {
            assert!(state.push(WorkRecord { routine: Some(noop), context: index as u64 }));
        }
        assert!(!state.push(WorkRecord { routine: Some(noop), context: 0 }));
        assert_eq!(state.count, MAX_WORK_ITEMS);
        state.accepting = false;
        assert!(!state.push(WorkRecord { routine: Some(noop), context: 1 }));
    }
    #[test]
    fn queue_pop_tracks_active_work_and_fifo_order() {
        let mut state = QueueState::new();
        state.accepting = true;
        let noop: WorkRoutine = |_| {};
        assert!(state.push(WorkRecord { routine: Some(noop), context: 10 }));
        assert!(state.push(WorkRecord { routine: Some(noop), context: 20 }));
        assert_eq!(state.pop().map(|x| x.context), Some(10));
        assert_eq!(state.active, 1);
        assert_eq!(state.pop().map(|x| x.context), Some(20));
        assert_eq!(state.count, 0);
        assert_eq!(state.active, 2);
    }
}

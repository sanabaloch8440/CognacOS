//! Deterministic host-side concurrency model for the WDM rundown contract.
//! This complements (but does not replace) the kernel's WDM tests and QEMU SMP
//! tests. Fault points deliberately exercise cleanup paths after callbacks start.
use std::sync::{
    atomic::{AtomicU8, AtomicUsize, Ordering},
    Arc, Barrier, Condvar, Mutex,
};
use std::thread;
use std::time::Duration;

#[derive(Default)]
struct Counts {
    accepting: bool,
    outstanding: usize,
    active_callbacks: usize,
    unload_started: bool,
    unload_calls: usize,
    completed: usize,
    cancelled: usize,
    torn_down: bool,
}
struct Device {
    counts: Mutex<Counts>,
    drained: Condvar,
}
impl Device {
    fn new() -> Self {
        Self { counts: Mutex::new(Counts { accepting: true, ..Counts::default() }), drained: Condvar::new() }
    }
    fn begin_irp(&self) -> bool {
        let mut s = self.counts.lock().unwrap();
        if !s.accepting { return false; }
        s.outstanding += 1;
        true
    }
    fn enter_callback(&self) {
        self.counts.lock().unwrap().active_callbacks += 1;
    }
    fn leave_callback(&self) {
        let mut s = self.counts.lock().unwrap();
        assert!(s.active_callbacks > 0);
        s.active_callbacks -= 1;
        if s.active_callbacks == 0 && s.outstanding == 0 { self.drained.notify_all(); }
    }
    fn complete(&self, irp: &Irp, inject_fault: bool) {
        if irp.completion.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_err() { return; }
        self.enter_callback();
        // Simulate a completion callback fault. The completion owner must still
        // publish a terminal state and release the outstanding-IRP reference.
        let callback_result = if inject_fault { Err(()) } else { Ok(()) };
        let mut s = self.counts.lock().unwrap();
        s.completed += 1;
        assert!(s.outstanding > 0);
        s.outstanding -= 1;
        irp.completion.store(2, Ordering::Release);
        if callback_result.is_err() { irp.faulted.store(true, Ordering::Release); }
        s.active_callbacks -= 1;
        if s.active_callbacks == 0 && s.outstanding == 0 { self.drained.notify_all(); }
    }
    fn cancel(&self, irp: &Irp) {
        irp.cancel_requested.store(true, Ordering::Release);
        if irp.cancel_callback.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_err() { return; }
        self.enter_callback();
        irp.cancel_callbacks.fetch_add(1, Ordering::AcqRel);
        // Cancellation can lose the completion race; it must never complete
        // the packet a second time or retain a device reference indefinitely.
        self.leave_callback();
    }
    fn unload_and_drain(&self) {
        let mut s = self.counts.lock().unwrap();
        s.accepting = false;
        s.unload_started = true;
        while s.outstanding != 0 || s.active_callbacks != 0 {
            s = self.drained.wait(s).unwrap();
        }
        if !s.torn_down {
            s.torn_down = true;
            s.unload_calls += 1;
        }
    }
}
struct Irp {
    completion: AtomicU8, // 0 live, 1 completing, 2 completed
    cancel_callback: AtomicU8,
    cancel_requested: std::sync::atomic::AtomicBool,
    cancel_callbacks: AtomicUsize,
    faulted: std::sync::atomic::AtomicBool,
}
impl Irp {
    fn new() -> Self {
        Self {
            completion: AtomicU8::new(0),
            cancel_callback: AtomicU8::new(0),
            cancel_requested: std::sync::atomic::AtomicBool::new(false),
            cancel_callbacks: AtomicUsize::new(0),
            faulted: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[test]
fn cancel_completion_and_unload_race_matrix() {
    for iteration in 0..512 {
        let device = Arc::new(Device::new());
        let irp = Arc::new(Irp::new());
        assert!(device.begin_irp());
        let gate = Arc::new(Barrier::new(4));
        let cdev = device.clone();
        let cirp = irp.clone();
        let cg = gate.clone();
        let cancel = thread::spawn(move || { cg.wait(); cdev.cancel(&cirp); });
        let ddev = device.clone();
        let dirp = irp.clone();
        let dg = gate.clone();
        let complete = thread::spawn(move || { dg.wait(); ddev.complete(&dirp, iteration % 7 == 0); });
        let udev = device.clone();
        let ug = gate.clone();
        let unload = thread::spawn(move || { ug.wait(); udev.unload_and_drain(); });
        gate.wait();
        cancel.join().unwrap();
        complete.join().unwrap();
        unload.join().unwrap();
        let s = device.counts.lock().unwrap();
        assert_eq!(s.outstanding, 0, "iteration {iteration}: IRP leaked");
        assert_eq!(s.active_callbacks, 0, "iteration {iteration}: callback leaked");
        assert_eq!(s.completed, 1, "iteration {iteration}: completion was not exactly once");
        assert_eq!(s.unload_calls, 1, "iteration {iteration}: unload callback count");
        assert!(s.torn_down);
        assert_eq!(irp.completion.load(Ordering::Acquire), 2);
        assert!(irp.cancel_callbacks.load(Ordering::Acquire) <= 1);
        if iteration % 7 == 0 { assert!(irp.faulted.load(Ordering::Acquire)); }
    }
}

#[test]
fn unload_barrier_rejects_new_work_and_drains_all_irps() {
    let device = Arc::new(Device::new());
    let irps: Vec<_> = (0..16).map(|_| { assert!(device.begin_irp()); Arc::new(Irp::new()) }).collect();
    let d = device.clone();
    let unload = thread::spawn(move || d.unload_and_drain());
    // Give the teardown thread an opportunity to close admission; no sleeps are
    // used to decide correctness, only to make this interleaving reproducible.
    for _ in 0..1000 {
        if device.counts.lock().unwrap().unload_started { break; }
        thread::yield_now();
    }
    assert!(!device.begin_irp(), "new IRP admitted after unload barrier");
    for irp in &irps { device.complete(irp, false); }
    unload.join().unwrap();
    let s = device.counts.lock().unwrap();
    assert_eq!(s.outstanding, 0);
    assert_eq!(s.unload_calls, 1);
    assert!(s.torn_down);
}

#[test]
fn callbacks_racing_teardown_are_drained_even_on_injected_fault() {
    let device = Arc::new(Device::new());
    let irp = Arc::new(Irp::new());
    assert!(device.begin_irp());
    device.enter_callback();
    let d = device.clone();
    let unload = thread::spawn(move || d.unload_and_drain());
    for _ in 0..1000 {
        if device.counts.lock().unwrap().unload_started { break; }
        thread::yield_now();
    }
    // Fault injection happens inside a callback that already holds rundown.
    {
        let mut s = device.counts.lock().unwrap();
        assert!(s.active_callbacks > 0);
        assert_eq!(s.unload_calls, 0);
        s.outstanding -= 1;
        s.active_callbacks -= 1;
        irp.faulted.store(true, Ordering::Release);
        device.drained.notify_all();
    }
    unload.join().unwrap();
    let s = device.counts.lock().unwrap();
    assert_eq!(s.outstanding, 0);
    assert_eq!(s.active_callbacks, 0);
    assert_eq!(s.unload_calls, 1);
    assert!(irp.faulted.load(Ordering::Acquire));
}

#[test]
fn completion_claim_is_single_winner_under_contention() {
    for _ in 0..1024 {
        let irp = Arc::new(Irp::new());
        let gate = Arc::new(Barrier::new(9));
        let mut threads = Vec::new();
        for _ in 0..8 {
            let i = irp.clone();
            let g = gate.clone();
            threads.push(thread::spawn(move || {
                g.wait();
                i.completion.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok()
            }));
        }
        gate.wait();
        let winners = threads.into_iter().filter(|t| t.join().unwrap()).count();
        assert_eq!(winners, 1);
        irp.completion.store(2, Ordering::Release);
        assert_eq!(irp.completion.load(Ordering::Acquire), 2);
    }
    thread::sleep(Duration::from_millis(1));
}

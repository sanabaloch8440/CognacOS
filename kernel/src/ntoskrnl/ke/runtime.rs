//! Callable NT kernel executive (Ke) routines with ABI-compatible signatures.
use core::sync::atomic::Ordering;

const ALL_PROCESSOR_GROUPS: u16 = 0xffff;
const PASSIVE_LEVEL: u8 = 0;
const DISPATCH_LEVEL: u8 = 2;
const IRQL_STACK_DEPTH: usize = 16;
static DISPATCHER_OBJECT_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
static CURRENT_IRQL: [core::sync::atomic::AtomicU8; 64] =
    [const { core::sync::atomic::AtomicU8::new(PASSIVE_LEVEL) }; 64];
static IRQL_DEPTH: [core::sync::atomic::AtomicU8; 64] =
    [const { core::sync::atomic::AtomicU8::new(0) }; 64];
static IRQL_FLAGS: [core::sync::atomic::AtomicU64; 64 * IRQL_STACK_DEPTH] =
    [const { core::sync::atomic::AtomicU64::new(0) }; 64 * IRQL_STACK_DEPTH];

#[inline]
fn cpu_slot() -> usize {
    crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Relaxed) as usize)
        .filter(|id| *id < 64)
        .unwrap_or(0)
}
#[inline]
unsafe fn read_rflags() -> u64 {
    let flags: u64;
    unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags)); }
    flags
}
#[inline]
unsafe fn irq_disable() { unsafe { core::arch::asm!("cli", options(nomem, nostack)); } }
#[inline]
unsafe fn irq_restore(flags: u64) {
    if flags & (1 << 9) != 0 {
        unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    }
}
fn push_irq_state(flags: u64) {
    let cpu = cpu_slot();
    let depth = IRQL_DEPTH[cpu].load(Ordering::Relaxed) as usize;
    if depth < IRQL_STACK_DEPTH {
        IRQL_FLAGS[cpu * IRQL_STACK_DEPTH + depth].store(flags, Ordering::Relaxed);
        IRQL_DEPTH[cpu].store((depth + 1) as u8, Ordering::Release);
    }
}
fn pop_irq_state() -> u64 {
    let cpu = cpu_slot();
    let depth = IRQL_DEPTH[cpu].load(Ordering::Acquire) as usize;
    if depth == 0 { return 0; }
    IRQL_DEPTH[cpu].store((depth - 1) as u8, Ordering::Release);
    IRQL_FLAGS[cpu * IRQL_STACK_DEPTH + depth - 1].load(Ordering::Relaxed)
}
#[inline]
fn atomic_lock_word(lock: *mut u64) -> Option<&'static core::sync::atomic::AtomicU64> {
    if lock.is_null() || (lock as usize) % core::mem::align_of::<u64>() != 0 { return None; }
    Some(unsafe { &*(lock.cast::<core::sync::atomic::AtomicU64>()) })
}


/// Windows PROCESSOR_NUMBER layout: group, group-relative number, reserved.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessorNumber { pub group: u16, pub number: u8, pub reserved: u8 }

/// Minimal x64 DISPATCHER_HEADER prefix shared by KEVENT/KSEMAPHORE.
#[repr(C)]
pub struct DispatcherHeader {
    kind: u8,
    absolute: u8,
    size: u8,
    inserted: u8,
    signal_state: i32,
    wait_list_flink: u64,
    wait_list_blink: u64,
}

const EVENT_NOTIFICATION: u8 = 0;
const EVENT_SYNCHRONIZATION: u8 = 1;
const MUTANT_OBJECT: u8 = 2;
const QUEUE_OBJECT: u8 = 4;
const EVENT_SIZE: u8 = 6; // sizeof(KEVENT) / sizeof(ULONG).


// Keep the pure generation predicate separately testable by the host CI runner.
include!("timer_generation.rs");

// Routine groups are kept as source fragments in one module scope for now. This
// preserves shared private state and ABI while making functional areas navigable.
include!("runtime/processor.rs");
include!("runtime/dispatcher_objects.rs");
include!("runtime/time.rs");
include!("runtime/thread_waits.rs");
include!("runtime/interrupts_scheduler.rs");
include!("runtime/device_queue.rs");
include!("runtime/executive_queue.rs");
include!("runtime/spinlocks.rs");
include!("runtime/mutex.rs");
include!("runtime/affinity.rs");
include!("runtime/timer_dpc_rendezvous.rs");

include!("runtime/process_attach.rs");
#[path = "runtime/xstate.rs"] mod xstate;
pub use xstate::*;


#[cfg(test)]
mod abi_layout_tests {
    use super::*;

    #[test]
    fn public_x64_dispatcher_prefix_layouts_are_stable() {
        assert_eq!(core::mem::size_of::<ProcessorNumber>(), 4);
        assert_eq!(core::mem::offset_of!(ProcessorNumber, group), 0);
        assert_eq!(core::mem::offset_of!(ProcessorNumber, number), 2);
        assert_eq!(core::mem::offset_of!(ProcessorNumber, reserved), 3);

        assert_eq!(core::mem::size_of::<DispatcherHeader>(), 24);
        assert_eq!(core::mem::offset_of!(DispatcherHeader, kind), 0);
        assert_eq!(core::mem::offset_of!(DispatcherHeader, signal_state), 4);
        assert_eq!(core::mem::offset_of!(DispatcherHeader, wait_list_flink), 8);
        assert_eq!(core::mem::offset_of!(DispatcherHeader, wait_list_blink), 16);
        assert_eq!(core::mem::size_of::<SemaphoreObject>(), 32);
        assert_eq!(core::mem::offset_of!(SemaphoreObject, limit), 24);
        assert_eq!(core::mem::size_of::<GroupAffinity>(), 16);
        assert_eq!(core::mem::offset_of!(GroupAffinity, group), 8);
    }

    #[test]
    fn processor_change_notification_abi_is_stable() {
        assert_eq!(core::mem::size_of::<ProcessorChangeNotifyContext>(), 16);
        assert_eq!(core::mem::offset_of!(ProcessorChangeNotifyContext, processor_number), 0);
        assert_eq!(core::mem::offset_of!(ProcessorChangeNotifyContext, change_type), 4);
        assert_eq!(core::mem::offset_of!(ProcessorChangeNotifyContext, status), 8);
        assert_eq!(core::mem::offset_of!(ProcessorChangeNotifyContext, reserved), 12);
    }
}

include!("runtime/nmi_callbacks.rs");

// Ke-owned clock/interrupt dispatch formerly split out as Ki.
include!("runtime/interrupt_dispatch.rs");

// Per-processor tick accounting, DPC timing, and clock watchdog detection.
include!("runtime/watchdog.rs");

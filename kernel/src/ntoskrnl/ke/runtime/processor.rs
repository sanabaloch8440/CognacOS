// processor and IRQL routines.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

/// IRQL is PASSIVE_LEVEL until the interrupt-priority layer is brought online.
pub unsafe extern "win64" fn ke_get_current_irql() -> u8 { CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire) }

pub unsafe extern "win64" fn ke_query_active_processor_count_ex(group: u16) -> u32 {
    let count = crate::hal::online_cpu_count() as u32;
    if group == 0 || group == ALL_PROCESSOR_GROUPS { count } else { 0 }
}
pub unsafe extern "win64" fn ke_query_active_processor_count(active_processors:*mut u64) -> u32 {
    let mask=unsafe{ke_query_group_affinity(0)};
    if !active_processors.is_null(){unsafe{active_processors.write(mask);}}
    mask.count_ones()
}
pub unsafe extern "win64" fn ke_query_active_group_count() -> u16 {
    if crate::hal::online_cpu_count() == 0 { 0 } else { 1 }
}
pub unsafe extern "win64" fn ke_query_maximum_processor_count_ex(group: u16) -> u32 {
    if group == 0 || group == ALL_PROCESSOR_GROUPS { crate::hal::MAX_CPUS as u32 } else { 0 }
}
pub unsafe extern "win64" fn ke_query_maximum_processor_count() -> u32 {
    crate::hal::MAX_CPUS as u32
}
/// ULONG KeGetCurrentProcessorNumberEx(PPROCESSOR_NUMBER ProcNumber).
/// The output pointer is optional; the return is always the systemwide index.
pub unsafe extern "win64" fn ke_get_current_processor_number_ex(out: *mut ProcessorNumber) -> u32 {
    let logical = crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Acquire))
        .unwrap_or(0);
    if !out.is_null() && logical < 64 {
        unsafe { out.write(ProcessorNumber { group: 0, number: logical as u8, reserved: 0 }); }
    }
    logical
}
pub unsafe extern "win64" fn ke_get_current_processor_index() -> u32 {
    crate::hal::current_cpu_local()
        .map(|cpu| cpu.logical_id.load(Ordering::Acquire))
        .unwrap_or(0)
}
pub unsafe extern "win64" fn ke_get_current_node_number() -> u32 { 0 }
pub unsafe extern "win64" fn ke_get_recommended_shared_data_alignment() -> u32 { 64 }

/// Initialize a Windows x64 KSPIN_LOCK word.
pub unsafe extern "win64" fn ke_initialize_spin_lock(lock: *mut u64) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
}
pub unsafe extern "win64" fn ke_test_spin_lock(lock: *const u64) -> u8 {
    if lock.is_null() || (lock as usize) % core::mem::align_of::<u64>() != 0 { return 0; }
    let word = unsafe { &*(lock.cast::<core::sync::atomic::AtomicU64>()) };
    (word.load(Ordering::Relaxed) == 0) as u8
}

/// Raise to the kernel's conservative DISPATCH_LEVEL model. Until an APIC TPR
/// policy is installed, local interrupts are masked rather than priority-filtered.
pub unsafe extern "win64" fn ke_raise_irql_to_dpc_level() -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(DISPATCH_LEVEL, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(DISPATCH_LEVEL) };
    old
}
pub unsafe extern "win64" fn ke_lower_irql(new_irql: u8) {
    let cpu = cpu_slot();
    CURRENT_IRQL[cpu].store(new_irql, Ordering::Release);
    let _ = unsafe { crate::apic::set_task_priority(new_irql) };
    let flags = pop_irq_state();
    unsafe { irq_restore(flags); }
    if new_irql < DISPATCH_LEVEL { dispatch_queued_dpcs(); }
    if new_irql == PASSIVE_LEVEL { ke_deliver_pending_kernel_apcs(); }
}

/// KeAcquireSpinLock: disable local interrupts, raise logical IRQL, then acquire
/// atomically so an interrupt handler cannot recursively deadlock this CPU.
pub unsafe extern "win64" fn ke_acquire_spin_lock(lock: *mut u64, old_irql: *mut u8) {
    if old_irql.is_null() { return; }
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(DISPATCH_LEVEL, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(DISPATCH_LEVEL) };
    unsafe { old_irql.write(old); }
    let Some(word) = atomic_lock_word(lock) else {
        CURRENT_IRQL[cpu_slot()].store(old, Ordering::Release);
        let _ = unsafe { crate::apic::set_task_priority(old) };
        let saved = pop_irq_state(); unsafe { irq_restore(saved); }
        return;
    };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
/// Raise to SYNCH_LEVEL. Local interrupts are conservatively masked because
/// this kernel does not yet program APIC task-priority registers.
pub unsafe extern "win64" fn ke_raise_irql_to_synch_level() -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(12, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(12) };
    old
}
/// Acquire a spin lock at SYNCH_LEVEL and return the prior IRQL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_raise_to_synch(lock: *mut u64) -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(12, Ordering::AcqRel);
    let _ = unsafe { crate::apic::set_task_priority(12) };
    let Some(word) = atomic_lock_word(lock) else {
        CURRENT_IRQL[cpu_slot()].store(old, Ordering::Release);
        let saved = pop_irq_state(); unsafe { irq_restore(saved); }
        return old;
    };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    old
}
/// Acquire a spin lock while raising to DISPATCH_LEVEL; returns the prior IRQL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_raise_to_dpc(lock: *mut u64) -> u8 {
    let mut old_irql = PASSIVE_LEVEL;
    unsafe { ke_acquire_spin_lock(lock, &mut old_irql as *mut u8); }
    old_irql
}
pub unsafe extern "win64" fn ke_release_spin_lock(lock: *mut u64, old_irql: u8) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
    unsafe { ke_lower_irql(old_irql); }
}
pub unsafe extern "win64" fn ke_acquire_spin_lock_at_dpc_level(lock: *mut u64) {
    let Some(word) = atomic_lock_word(lock) else { return; };
    while word.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
pub unsafe extern "win64" fn ke_release_spin_lock_from_dpc_level(lock: *mut u64) {
    if let Some(word) = atomic_lock_word(lock) { word.store(0, Ordering::Release); }
}
pub unsafe extern "win64" fn ke_try_to_acquire_spin_lock_at_dpc_level(lock: *mut u64) -> u8 {
    match atomic_lock_word(lock) {
        Some(word) => word.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_ok() as u8,
        None => 0,
    }
}
/// Query the architectural monotonic TSC using the Windows x64 LARGE_INTEGER ABI.
/// Frequency is reported in ticks per second and is only advertised when the
/// platform exposes a defensible TSC frequency.
pub unsafe extern "win64" fn ke_query_performance_counter(frequency: *mut i64) -> i64 {
    if let Some(hz) = crate::apic::tsc_frequency_hz() {
        if !frequency.is_null() {
            unsafe { frequency.write(hz.min(i64::MAX as u64) as i64); }
        }
    } else if !frequency.is_null() {
        unsafe { frequency.write(0); }
    }
    crate::apic::monotonic_ticks().min(i64::MAX as u64) as i64
}

/// Serialize prior writes before returning to a caller that requires the
/// legacy KeFlushWriteBuffer ordering guarantee.
pub unsafe extern "win64" fn ke_flush_write_buffer() {
    core::sync::atomic::fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)); }
}

/// Busy-wait for a bounded microsecond interval using the calibrated/stable TSC.
/// If no usable frequency is known, fail closed by returning without pretending
/// that a guessed delay was accurate; HAL import resolution hides this export.
pub unsafe extern "win64" fn ke_stall_execution_processor(microseconds: u32) {
    if microseconds == 0 { return; }
    let Some(hz) = crate::apic::tsc_frequency_hz() else { return; };
    let ticks = ((hz as u128) * (microseconds as u128) + 999_999) / 1_000_000;
    let Ok(ticks) = u64::try_from(ticks) else { return; };
    let start = crate::apic::monotonic_ticks();
    while crate::apic::monotonic_ticks().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
}


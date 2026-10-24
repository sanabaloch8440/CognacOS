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
    let _ = unsafe { crate::hal::set_interrupt_priority(DISPATCH_LEVEL) };
    old
}
pub unsafe extern "win64" fn ke_lower_irql(new_irql: u8) {
    let cpu = cpu_slot();
    CURRENT_IRQL[cpu].store(new_irql, Ordering::Release);
    let _ = unsafe { crate::hal::set_interrupt_priority(new_irql) };
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
    let _ = unsafe { crate::hal::set_interrupt_priority(DISPATCH_LEVEL) };
    unsafe { old_irql.write(old); }
    let Some(word) = atomic_lock_word(lock) else {
        CURRENT_IRQL[cpu_slot()].store(old, Ordering::Release);
        let _ = unsafe { crate::hal::set_interrupt_priority(old) };
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
    let _ = unsafe { crate::hal::set_interrupt_priority(12) };
    old
}
/// Acquire a spin lock at SYNCH_LEVEL and return the prior IRQL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_raise_to_synch(lock: *mut u64) -> u8 {
    let flags = unsafe { read_rflags() };
    unsafe { irq_disable(); }
    push_irq_state(flags);
    let old = CURRENT_IRQL[cpu_slot()].swap(12, Ordering::AcqRel);
    let _ = unsafe { crate::hal::set_interrupt_priority(12) };
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
    if let Some(hz) = crate::hal::query_performance_frequency_hz() {
        if !frequency.is_null() {
            unsafe { frequency.write(hz.min(i64::MAX as u64) as i64); }
        }
    } else if !frequency.is_null() {
        unsafe { frequency.write(0); }
    }
    crate::hal::query_performance_counter().min(i64::MAX as u64) as i64
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
    let Some(hz) = crate::hal::query_performance_frequency_hz() else { return; };
    let ticks = ((hz as u128) * (microseconds as u128) + 999_999) / 1_000_000;
    let Ok(ticks) = u64::try_from(ticks) else { return; };
    let start = crate::hal::query_performance_counter();
    while crate::hal::query_performance_counter().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
}



// Windows LOGICAL_PROCESSOR_RELATIONSHIP::RelationProcessorCore = 0.
// The current HAL models one logical processor per core and one processor group.
const STATUS_INFO_LENGTH_MISMATCH:i32=0xC000_0004u32 as i32;
const STATUS_NOT_SUPPORTED:i32=0xC000_00BBu32 as i32;
const STATUS_INVALID_PARAMETER_NT:i32=0xC000_000Du32 as i32;
const PROCESSOR_CORE_RECORD_SIZE:usize=48;
unsafe fn write_processor_core_record(output:*mut u8,cpu:usize){
 unsafe{
  core::ptr::write_bytes(output,0,PROCESSOR_CORE_RECORD_SIZE);
  output.cast::<u32>().write_unaligned(0); // RelationProcessorCore
  output.add(4).cast::<u32>().write_unaligned(PROCESSOR_CORE_RECORD_SIZE as u32);
  output.add(30).cast::<u16>().write_unaligned(1); // GroupCount
  output.add(32).cast::<u64>().write_unaligned(1u64<<cpu); // GROUP_AFFINITY.Mask
  output.add(40).cast::<u16>().write_unaligned(0); // Group
 }
}

/// NTSTATUS KeQueryLogicalProcessorRelationship(PPROCESSOR_NUMBER, relationship,
/// PSYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, PULONG Length).
pub unsafe extern "win64" fn ke_query_logical_processor_relationship(
 processor_number:*const ProcessorNumber,relationship:i32,buffer:*mut core::ffi::c_void,length:*mut u32,
)->i32{
 if length.is_null(){return STATUS_INVALID_PARAMETER_NT}
 if relationship!=0{return STATUS_NOT_SUPPORTED}
 let input_length=unsafe{length.read_unaligned()} as usize;
 let mut cpus=[0usize;crate::hal::MAX_CPUS];let mut count=0usize;
 if processor_number.is_null(){
  for cpu in 0..crate::hal::MAX_CPUS{
   if crate::hal::cpu_state(cpu).ok()==Some(crate::hal::CPU_ONLINE){cpus[count]=cpu;count+=1;}
  }
 }else{
  let number=unsafe{processor_number.read_unaligned()};
  let cpu=number.number as usize;
  if number.group!=0||cpu>=crate::hal::MAX_CPUS||crate::hal::cpu_state(cpu).ok()!=Some(crate::hal::CPU_ONLINE){
   unsafe{length.write_unaligned(PROCESSOR_CORE_RECORD_SIZE as u32);}
   return STATUS_INVALID_PARAMETER_NT;
  }
  cpus[0]=cpu;count=1;
 }
 let required=count.saturating_mul(PROCESSOR_CORE_RECORD_SIZE);
 unsafe{length.write_unaligned(required.min(u32::MAX as usize) as u32);}
 if required==0{return STATUS_INVALID_PARAMETER_NT}
 if buffer.is_null()||input_length<required{return STATUS_INFO_LENGTH_MISMATCH}
 for (index,cpu) in cpus[..count].iter().copied().enumerate(){
  unsafe{write_processor_core_record((buffer as *mut u8).add(index*PROCESSOR_CORE_RECORD_SIZE),cpu);}
 }
 0
}

// Processor-change callbacks use side metadata: callers' callback handles are
// stable kernel pointers, while notification storage is valid only for the call.
pub const KE_PROCESSOR_CHANGE_ADD_EXISTING:u32=0x1;
pub const KE_PROCESSOR_CHANGE_ADD_COMPLETE:u32=1;
pub const KE_PROCESSOR_CHANGE_REMOVE_COMPLETE:u32=2;
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ProcessorChangeNotifyContext {
    pub processor_number:ProcessorNumber,
    pub change_type:u32,
    pub status:i32,
    pub reserved:u32,
}
pub type ProcessorChangeRoutine=unsafe extern "win64" fn(*mut core::ffi::c_void,*mut ProcessorChangeNotifyContext,u32)->i32;
const MAX_PROCESSOR_CHANGE_CALLBACKS:usize=64;
#[derive(Clone,Copy)]
struct ProcessorCallbackRecord { active:bool, routine:usize, context:usize, flags:u32 }
impl ProcessorCallbackRecord { const EMPTY:Self=Self{active:false,routine:0,context:0,flags:0}; }
struct ProcessorCallbackTable { records:[ProcessorCallbackRecord;MAX_PROCESSOR_CHANGE_CALLBACKS] }
impl ProcessorCallbackTable { const fn new()->Self{Self{records:[ProcessorCallbackRecord::EMPTY;MAX_PROCESSOR_CHANGE_CALLBACKS]}} }
static PROCESSOR_CALLBACKS:crate::hal::IrqSpinLock<ProcessorCallbackTable>=crate::hal::IrqSpinLock::new(ProcessorCallbackTable::new());
static PROCESSOR_CALLBACK_IN_FLIGHT:[core::sync::atomic::AtomicUsize;MAX_PROCESSOR_CHANGE_CALLBACKS]=
 [const{core::sync::atomic::AtomicUsize::new(0)};MAX_PROCESSOR_CHANGE_CALLBACKS];

/// Register for processor add/remove lifecycle notifications.
pub unsafe extern "win64" fn ke_register_processor_change_callback(
 callback:Option<ProcessorChangeRoutine>,context:*mut core::ffi::c_void,flags:u32,
)->*mut core::ffi::c_void{
 let Some(callback)=callback else{return core::ptr::null_mut()};
 if flags & !KE_PROCESSOR_CHANGE_ADD_EXISTING != 0{return core::ptr::null_mut()}
 // Invert the dependency: Ke registers an observer with HAL; HAL never calls
 // into ntoskrnl by name or imports the executive module.
 if crate::hal::register_processor_change_notifier(notify_processor_change).is_err() {
  return core::ptr::null_mut();
 }
 let slot={
  let mut table=PROCESSOR_CALLBACKS.lock();
  let Some(slot)=table.records.iter().enumerate().position(|(i,r)|!r.active && PROCESSOR_CALLBACK_IN_FLIGHT[i].load(Ordering::Acquire)==0) else{return core::ptr::null_mut()};
  table.records[slot]=ProcessorCallbackRecord{active:true,routine:callback as usize,context:context as usize,flags};
  slot
 };
 let handle=(slot+1) as *mut core::ffi::c_void;
 if flags & KE_PROCESSOR_CHANGE_ADD_EXISTING != 0 {
  for cpu in 0..crate::hal::MAX_CPUS {
   if crate::hal::cpu_state(cpu).ok()==Some(crate::hal::CPU_ONLINE){notify_processor_callback(slot,cpu,KE_PROCESSOR_CHANGE_ADD_COMPLETE);}
  }
 }
 handle
}

/// Unregister and wait for callbacks already in flight to finish.
pub unsafe extern "win64" fn ke_deregister_processor_change_callback(handle:*mut core::ffi::c_void){
 if handle.is_null(){return}
 let raw=handle as usize;
 if raw==0||raw>MAX_PROCESSOR_CHANGE_CALLBACKS{return}
 let slot=raw-1;
 {
  let mut table=PROCESSOR_CALLBACKS.lock();
  if !table.records[slot].active{return}
  table.records[slot]=ProcessorCallbackRecord::EMPTY;
 }
 while PROCESSOR_CALLBACK_IN_FLIGHT[slot].load(Ordering::Acquire)!=0 {core::hint::spin_loop();}
}

/// Called by HAL after a processor lifecycle transition has been published.
fn notify_processor_callback(slot:usize,cpu:usize,change_type:u32){
 let record={
  let table=PROCESSOR_CALLBACKS.lock();
  let Some(record)=table.records.get(slot).copied().filter(|r|r.active) else{return};
  PROCESSOR_CALLBACK_IN_FLIGHT[slot].fetch_add(1,Ordering::AcqRel);
  record
 };
 let Ok(state)=crate::hal::cpu_state(cpu) else{PROCESSOR_CALLBACK_IN_FLIGHT[slot].fetch_sub(1,Ordering::AcqRel);return};
 if cpu>u8::MAX as usize{PROCESSOR_CALLBACK_IN_FLIGHT[slot].fetch_sub(1,Ordering::AcqRel);return}
 let mut notification=ProcessorChangeNotifyContext{processor_number:ProcessorNumber{group:0,number:cpu as u8,reserved:0},change_type,status:0,reserved:state as u32};
 let routine:ProcessorChangeRoutine=unsafe{core::mem::transmute(record.routine)};
 unsafe{routine(record.context as *mut _,&mut notification,change_type);}
 PROCESSOR_CALLBACK_IN_FLIGHT[slot].fetch_sub(1,Ordering::AcqRel);
}
pub fn notify_processor_change(cpu:usize,change_type:u32){
 let Ok(state)=crate::hal::cpu_state(cpu) else{return};
 if cpu>u8::MAX as usize{return}
 let snapshot={
  let table=PROCESSOR_CALLBACKS.lock();
  let mut copy=[ProcessorCallbackRecord::EMPTY;MAX_PROCESSOR_CHANGE_CALLBACKS];
  for (i,r) in table.records.iter().enumerate(){
   if r.active {
    PROCESSOR_CALLBACK_IN_FLIGHT[i].fetch_add(1,Ordering::AcqRel);
    copy[i]=*r;
   }
  }
  copy
 };
 let mut notification=ProcessorChangeNotifyContext{processor_number:ProcessorNumber{group:0,number:cpu as u8,reserved:0},change_type,status:0,reserved:state as u32};
 for (i,record) in snapshot.iter().enumerate(){
  if !record.active{continue}
  let routine:ProcessorChangeRoutine=unsafe{core::mem::transmute(record.routine)};
  unsafe{routine(record.context as *mut _,&mut notification,change_type);}
  PROCESSOR_CALLBACK_IN_FLIGHT[i].fetch_sub(1,Ordering::AcqRel);
 }
}


#[cfg(test)]
mod processor_callback_tests {
    use super::*;
    static CALLS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
    unsafe extern "win64" fn callback(_: *mut core::ffi::c_void, context:*mut ProcessorChangeNotifyContext, change:u32)->i32 {
        if !context.is_null() && unsafe{(*context).change_type}==change {
            CALLS.fetch_add(1,Ordering::AcqRel);
        }
        0
    }
    #[test]
    fn processor_callback_registration_and_deregistration() {
        CALLS.store(0,Ordering::Release);
        let handle=unsafe{ke_register_processor_change_callback(Some(callback),core::ptr::null_mut(),0)};
        assert!(!handle.is_null());
        notify_processor_change(0,KE_PROCESSOR_CHANGE_ADD_COMPLETE);
        assert_eq!(CALLS.load(Ordering::Acquire),1);
        unsafe{ke_deregister_processor_change_callback(handle);}
        notify_processor_change(0,KE_PROCESSOR_CHANGE_ADD_COMPLETE);
        assert_eq!(CALLS.load(Ordering::Acquire),1);
    }
}


#[cfg(test)]
mod logical_processor_layout_tests {
    use super::*;
    #[test]
    fn single_core_relationship_record_has_expected_x64_offsets() {
        let mut bytes=[0u8;PROCESSOR_CORE_RECORD_SIZE];
        unsafe{write_processor_core_record(bytes.as_mut_ptr(),3);}
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()),0);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()),48);
        assert_eq!(u16::from_le_bytes(bytes[30..32].try_into().unwrap()),1);
        assert_eq!(u64::from_le_bytes(bytes[32..40].try_into().unwrap()),8);
        assert_eq!(u16::from_le_bytes(bytes[40..42].try_into().unwrap()),0);
    }
}

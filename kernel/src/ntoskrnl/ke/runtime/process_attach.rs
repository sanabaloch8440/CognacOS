// Process attach/detach: switch CR3 to a referenced, registered process root.
// KAPC_STATE storage is treated as an opaque caller-owned key; Windows private
// bytes are not read or written. Per-thread side metadata supports nesting.
use core::sync::atomic::Ordering;

const MAX_PROCESS_ATTACHES: usize = 128;
#[derive(Clone, Copy)]
struct AttachRecord {
    active: bool,
    thread: u64,
    state_key: usize,
    previous_root: u64,
    target_root: u64,
    process: usize,
    sequence: u64,
}
impl AttachRecord {
    const EMPTY: Self = Self { active:false, thread:0, state_key:0, previous_root:0, target_root:0, process:0, sequence:0 };
}
struct AttachTable { records:[AttachRecord;MAX_PROCESS_ATTACHES], next_sequence:u64 }
impl AttachTable { const fn new()->Self { Self { records:[AttachRecord::EMPTY;MAX_PROCESS_ATTACHES], next_sequence:1 } } }
static ATTACHES: crate::hal::IrqSpinLock<AttachTable> = crate::hal::IrqSpinLock::new(AttachTable::new());

fn attach(process:*mut core::ffi::c_void, state_key:usize)->bool {
    if process.is_null() || CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire)!=PASSIVE_LEVEL { return false; }
    let Some(thread)=crate::thread::current_thread_id() else { return false; };
    let Some(target_root)=crate::process::reference_process_root(process) else { return false; };
    let previous_root=crate::vm::active_root_physical();
    if previous_root==0 || target_root==0 {
        unsafe { crate::process::obf_dereference_process_object(process); }
        return false;
    }
    {
        let mut table=ATTACHES.lock();
        let Some(slot)=table.records.iter().position(|r|!r.active) else {
            drop(table);
            unsafe { crate::process::obf_dereference_process_object(process); }
            return false;
        };
        let sequence=table.next_sequence;
        table.next_sequence=table.next_sequence.wrapping_add(1).max(1);
        table.records[slot]=AttachRecord{active:true,thread,state_key,previous_root,target_root,process:process as usize,sequence};
    }
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) target_root, options(nostack, preserves_flags)); }
    true
}

fn detach(state_key:Option<usize>)->bool {
    if CURRENT_IRQL[cpu_slot()].load(Ordering::Acquire)!=PASSIVE_LEVEL { return false; }
    let Some(thread)=crate::thread::current_thread_id() else { return false; };
    let record={
        let mut table=ATTACHES.lock();
        let selected=table.records.iter().enumerate()
            .filter(|(_,r)|r.active && r.thread==thread && state_key.map_or(true,|key|r.state_key==key))
            .max_by_key(|(_,r)|r.sequence).map(|(i,_)|i);
        let Some(slot)=selected else { return false; };
        let record=table.records[slot];
        // Enforce LIFO address-space restoration for this thread. A mismatched
        // KAPC_STATE must not silently restore an unrelated nested attachment.
        if table.records.iter().any(|r|r.active && r.thread==thread && r.sequence>record.sequence) { return false; }
        table.records[slot]=AttachRecord::EMPTY;
        record
    };
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) record.previous_root, options(nostack, preserves_flags)); }
    unsafe { crate::process::obf_dereference_process_object(record.process as *mut core::ffi::c_void); }
    true
}

/// VOID KeStackAttachProcess(PRKPROCESS Process, PRKAPC_STATE ApcState).
pub unsafe extern "win64" fn ke_stack_attach_process(process:*mut core::ffi::c_void,apc_state:*mut core::ffi::c_void) {
    if apc_state.is_null() { return; }
    let _=attach(process,apc_state as usize);
}
/// VOID KeUnstackDetachProcess(PRKAPC_STATE ApcState).
pub unsafe extern "win64" fn ke_unstack_detach_process(apc_state:*mut core::ffi::c_void) {
    if apc_state.is_null() { return; }
    let _=detach(Some(apc_state as usize));
}
/// Legacy VOID KeAttachProcess(PRKPROCESS Process).
pub unsafe extern "win64" fn ke_attach_process(process:*mut core::ffi::c_void) {
    let _=attach(process,0);
}
/// Legacy VOID KeDetachProcess(VOID).
pub unsafe extern "win64" fn ke_detach_process() { let _=detach(Some(0)); }

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn side_metadata_has_no_windows_private_layout_dependency() {
        assert_eq!(core::mem::size_of::<AttachRecord>(), 48);
        assert_eq!(core::mem::align_of::<AttachRecord>(), 8);
    }
}

//! Minimal per-process Windows x64 PEB/TEB and TLS storage.
//! Loader lists, process parameters, and full NT object semantics remain future work.
use crate::vm::{AddressSpace, VmError};
const PAGE: u64 = 4096;
const USER_START: u64 = 0x0000_0080_0000_0000;
pub const PEB_ADDRESS: u64 = 0x0000_0080_1000_0000;
pub const TEB_ADDRESS: u64 = 0x0000_0080_2000_0000;
pub const TLS_VECTOR_ADDRESS: u64 = 0x0000_0080_3000_0000;
pub const LDR_ADDRESS: u64 = 0x0000_0080_4000_0000;
pub const LDR_ENTRY_ADDRESS: u64 = 0x0000_0080_5000_0000;
pub const IMAGE_NAME_ADDRESS: u64 = 0x0000_0080_6000_0000;
pub const PROCESS_PARAMS_ADDRESS: u64 = 0x0000_0080_7000_0000;
static NEXT_PID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(4);
const MAX_PROCESSES: usize = 64;
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState { Created, Running, Exited, Reaped }
#[derive(Clone, Copy, Debug)]
pub struct ProcessSnapshot { pub process_id: u64, pub state: ProcessState, pub exit_code: u64 }

/// Stable kernel-resident process object used by NT process-pointer APIs.
/// This is RdotOS's internal EPROCESS-compatible anchor, not the private
/// binary layout of a particular Windows build.
const EPROCESS_MAGIC: u64 = 0x5250_4f43_4553_5331;
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct EProcess {
    magic: u64,
    pub process_id: u64,
    state: ProcessState,
    used: bool,
    pub address_space_root: u64,
    references: u32,
    _reserved: u32,
    exit_code: u64,
}
impl EProcess {
    const EMPTY: Self = Self { magic: 0, process_id: 0, state: ProcessState::Reaped,
        used: false, address_space_root: 0, references: 0, _reserved: 0, exit_code: 0 };
}
struct ProcessRegistry([EProcess; MAX_PROCESSES]);
static TABLE: crate::hal::IrqSpinLock<ProcessRegistry> =
    crate::hal::IrqSpinLock::new(ProcessRegistry([EProcess::EMPTY; MAX_PROCESSES]));

fn register_process(pid: u64, address_space_root: u64) -> Result<(), ProcessError> {
    if address_space_root == 0 { return Err(ProcessError::InvalidLayout); }
    let mut registry = TABLE.lock();
    let Some(slot) = registry.0.iter_mut().find(|p| !p.used) else { return Err(ProcessError::TooManyProcesses); };
    *slot = EProcess { magic: EPROCESS_MAGIC, process_id: pid, state: ProcessState::Created,
        used: true, address_space_root, references: 0, _reserved: 0, exit_code: 0 };
    Ok(())
}

/// Return the stable process-object address registered for a process ID.
/// Look up an EPROCESS-compatible object by PID and return it referenced.
/// The caller must eventually release the reference with ObfDereferenceObject.
pub fn eprocess_for_pid(pid: u64) -> Option<*mut EProcess> {
    let mut registry = TABLE.lock();
    let index = registry.0.iter().position(|p| p.used && p.magic == EPROCESS_MAGIC && p.process_id == pid)?;
    let next = registry.0[index].references.checked_add(1)?;
    registry.0[index].references = next;
    Some(core::ptr::addr_of!(registry.0[index]) as *mut EProcess)
}

/// Add an object reference for a registered process. Used by NT process lookup
/// and generic object reference entry points.
pub fn reference_eprocess(pointer: *mut core::ffi::c_void) -> bool {
    let mut registry = TABLE.lock();
    let Some(p) = registry.0.iter_mut().find(|p|
        core::ptr::addr_of!(**p) as *mut EProcess as *mut core::ffi::c_void == pointer
            && p.used && p.magic == EPROCESS_MAGIC) else { return false; };
    let Some(next) = p.references.checked_add(1) else { return false; };
    p.references = next;
    true
}
pub fn dereference_eprocess(pointer: *mut core::ffi::c_void) -> bool {
    let mut registry = TABLE.lock();
    let Some(p) = registry.0.iter_mut().find(|p|
        core::ptr::addr_of!(**p) as *mut EProcess as *mut core::ffi::c_void == pointer
            && p.used && p.magic == EPROCESS_MAGIC) else { return false; };
    if p.references == 0 { return false; }
    p.references -= 1;
    true
}

/// Windows x64 PsLookupProcessByProcessId subset. Returned objects carry a
/// reference that must be released with ObfDereferenceObject.
pub unsafe extern "win64" fn ps_lookup_process_by_process_id(
    process_id: *mut core::ffi::c_void,
    process_out: *mut *mut core::ffi::c_void,
) -> i32 {
    const STATUS_SUCCESS: i32 = 0;
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_FOUND: i32 = 0xC000_0225u32 as i32;
    let output = process_out as u64;
    let Some(output_end) = output.checked_add(7) else { return STATUS_INVALID_PARAMETER; };
    if process_out.is_null() || output < 0xffff_8000_0000_0000
        || crate::kernel_vm::is_pageable_address(output)
        || crate::kernel_vm::is_pageable_address(output_end)
        || unsafe { crate::ntoskrnl::mm::runtime::mm_is_non_paged_system_address_valid(process_out.cast()) } == 0
        || unsafe { crate::ntoskrnl::mm::runtime::mm_is_non_paged_system_address_valid(output_end as *const core::ffi::c_void) } == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { process_out.write(core::ptr::null_mut()); }
    let pid = process_id as u64;
    let mut registry = TABLE.lock();
    let Some(p) = registry.0.iter_mut().find(|p|
        p.used && p.magic == EPROCESS_MAGIC && p.process_id == pid
            && p.state != ProcessState::Reaped) else { return STATUS_NOT_FOUND; };
    let Some(next) = p.references.checked_add(1) else { return STATUS_INVALID_PARAMETER; };
    p.references = next;
    let object = p as *mut EProcess as *mut core::ffi::c_void;
    unsafe { process_out.write(object); }
    STATUS_SUCCESS
}

/// The current object-manager subset can reference/dereference registered
/// EPROCESS anchors. Other object types remain owned by their own subsystems.
pub unsafe extern "win64" fn obf_reference_process_object(object: *mut core::ffi::c_void) {
    let _ = reference_eprocess(object);
}
pub unsafe extern "win64" fn obf_dereference_process_object(object: *mut core::ffi::c_void) {
    let _ = dereference_eprocess(object);
}
pub fn mark_running(pid: u64) -> bool {
    let mut registry = TABLE.lock();
    if let Some(p) = registry.0.iter_mut().find(|p| p.used && p.process_id == pid && p.state == ProcessState::Created) {
        p.state = ProcessState::Running; true
    } else { false }
}
pub fn mark_exited(pid: u64, code: u64) -> bool {
    let mut registry = TABLE.lock();
    if let Some(p) = registry.0.iter_mut().find(|p| p.used && p.process_id == pid && p.state == ProcessState::Running) {
        p.state = ProcessState::Exited; p.exit_code = code; true
    } else { false }
}
pub fn query(pid: u64) -> Option<ProcessSnapshot> {
    let registry = TABLE.lock();
    registry.0.iter().find(|p| p.used && p.magic == EPROCESS_MAGIC && p.process_id == pid)
        .map(|p| ProcessSnapshot { process_id: p.process_id, state: p.state, exit_code: p.exit_code })
}
pub fn reap(pid: u64) -> Option<u64> {
    let mut registry = TABLE.lock();
    let p = registry.0.iter_mut().find(|p| p.used && p.process_id == pid && p.state == ProcessState::Exited)?;
    // A process object cannot be recycled while its address-space root is still registered.
    if p.references != 0 || p.address_space_root != 0 { return None; }
    let code = p.exit_code;
    p.state = ProcessState::Reaped;
    p.used = false;
    p.address_space_root = 0;
    p.magic = 0;
    Some(code)
}

/// Called by AddressSpace::destroy while the global address-space lifetime lock
/// is held. Invalidates the root before its page tables can be reclaimed.
pub(crate) fn detach_address_space(root: u64) {
    let mut registry = TABLE.lock();
    for p in registry.0.iter_mut().filter(|p| p.used && p.address_space_root == root) {
        p.address_space_root = 0;
    }
}


pub fn reference_process_for_root(root: u64) -> Option<*mut core::ffi::c_void> {
    if root == 0 { return None; }
    let mut registry = TABLE.lock();
    let p = registry.0.iter_mut().find(|p| p.used && p.magic == EPROCESS_MAGIC
        && p.address_space_root == root
        && (p.state == ProcessState::Running || p.state == ProcessState::Created))?;
    // Activating a registered root is the first reliable evidence that its
    // process has begun execution; older launch paths did not set Running.
    if p.state == ProcessState::Created { p.state = ProcessState::Running; }
    p.references = p.references.checked_add(1)?;
    Some(p as *mut EProcess as *mut core::ffi::c_void)
}
/// Reference a registered process object and snapshot its live address-space root.
/// The returned reference must be released with ObfDereferenceObject.
pub fn reference_process_root(pointer:*mut core::ffi::c_void)->Option<u64>{
    if pointer.is_null(){return None}
    let mut registry=TABLE.lock();
    let process=registry.0.iter_mut().find(|p|
        core::ptr::addr_of!(**p) as *mut EProcess as *mut core::ffi::c_void==pointer
        && p.used && p.magic==EPROCESS_MAGIC
        && (p.state==ProcessState::Running||p.state==ProcessState::Created)
        && p.address_space_root!=0)?;
    process.references=process.references.checked_add(1)?;
    Some(process.address_space_root)
}
pub fn process_matches_root(pointer: *mut core::ffi::c_void, root: u64) -> bool {
    let registry = TABLE.lock();
    registry.0.iter().any(|p| p.used && p.magic == EPROCESS_MAGIC
        && (p as *const EProcess as *mut core::ffi::c_void) == pointer
        && p.address_space_root == root && p.state == ProcessState::Running)
}

/// Execute a cross-process operation with both registered process objects
/// referenced and both address spaces protected against concurrent mutation or
/// destruction. Lock order is always address-space lifetime lock, then registry.
pub fn with_registered_process_roots<R>(
    source: *mut core::ffi::c_void,
    target: *mut core::ffi::c_void,
    operation: impl FnOnce(u64, u64) -> R,
) -> Option<R> {
    let _address_spaces = crate::vm::ADDRESS_SPACE_LIFETIME_LOCK.lock();
    let (source_index, target_index, source_root, target_root) = {
        let mut registry = TABLE.lock();
        let find = |pointer: *mut core::ffi::c_void, entries: &[EProcess]| -> Option<usize> {
            entries.iter().position(|p| {
                (p as *const EProcess as *mut core::ffi::c_void) == pointer
                    && p.used && p.magic == EPROCESS_MAGIC
                    && p.state != ProcessState::Exited && p.state != ProcessState::Reaped
                    && p.address_space_root != 0
            })
        };
        let source_index = find(source, &registry.0)?;
        let target_index = find(target, &registry.0)?;
        if source_index == target_index {
            if registry.0[source_index].references > u32::MAX - 2 { return None; }
        } else if registry.0[source_index].references == u32::MAX
            || registry.0[target_index].references == u32::MAX { return None; }
        registry.0[source_index].references += 1;
        registry.0[target_index].references += 1;
        let roots = (source_index, target_index,
            registry.0[source_index].address_space_root, registry.0[target_index].address_space_root);
        roots
    };
    let result = operation(source_root, target_root);
    {
        let mut registry = TABLE.lock();
        if registry.0[source_index].references != 0 { registry.0[source_index].references -= 1; }
        if registry.0[target_index].references != 0 { registry.0[target_index].references -= 1; }
    }
    Some(result)
}
#[derive(Clone, Copy, Debug)]
pub enum ProcessError { Vm(VmError), OutOfMemory, InvalidLayout, TooManyProcesses }
impl From<VmError> for ProcessError { fn from(e: VmError) -> Self { Self::Vm(e) } }
#[derive(Clone, Copy, Debug)]
pub struct ProcessEnvironment { pub process_id: u64, pub peb: u64, pub teb: u64, pub tls_vector: u64 }
fn map_zeroed(space: &mut AddressSpace, base: u64, pages: usize) -> Result<(), ProcessError> {
    for i in 0..pages {
        let physical = unsafe { crate::memory::allocate_page() }.map_err(|_| ProcessError::OutOfMemory)?;
        unsafe { space.map_user_page(base + i as u64 * PAGE, physical, true, false)?; }
    }
    Ok(())
}
fn put_u64(space: &AddressSpace, address: u64, value: u64) -> Result<(), ProcessError> {
    unsafe { space.copy_to_user(address, &value.to_le_bytes())?; } Ok(())
}
fn put_u8(space: &AddressSpace, address: u64, value: u8) -> Result<(), ProcessError> {
    unsafe { space.copy_to_user(address, &[value])?; } Ok(())
}
fn put_u16(space: &AddressSpace, address: u64, value: u16) -> Result<(), ProcessError> {
    unsafe { space.copy_to_user(address, &value.to_le_bytes())?; } Ok(())
}
fn put_u32(space: &AddressSpace, address: u64, value: u32) -> Result<(), ProcessError> {
    unsafe { space.copy_to_user(address, &value.to_le_bytes())?; } Ok(())
}
/// Initialize stable x64 PEB/TEB fields in a fresh address space.
pub unsafe fn initialize(space: &mut AddressSpace, image_base: u64, image_entry: u64, image_size: u32, stack_base: u64, stack_limit: u64)
    -> Result<ProcessEnvironment, ProcessError> {
    if image_base == 0 || stack_base <= stack_limit || stack_limit < USER_START {
        return Err(ProcessError::InvalidLayout);
    }
    map_zeroed(space, PEB_ADDRESS, 1)?;
    map_zeroed(space, TEB_ADDRESS, 2)?;
    map_zeroed(space, TLS_VECTOR_ADDRESS, 1)?;
    map_zeroed(space, LDR_ADDRESS, 1)?;
    map_zeroed(space, LDR_ENTRY_ADDRESS, 2)?;
    map_zeroed(space, IMAGE_NAME_ADDRESS, 2)?;
    map_zeroed(space, PROCESS_PARAMS_ADDRESS, 1)?;
    let pid = NEXT_PID.fetch_add(4, core::sync::atomic::Ordering::Relaxed);
    // PEB x64: BeingDebugged, ImageBaseAddress, Ldr, ProcessParameters.
    put_u8(space, PEB_ADDRESS + 2, 0)?;
    put_u64(space, PEB_ADDRESS + 0x10, image_base)?;
    put_u64(space, PEB_ADDRESS + 0x18, LDR_ADDRESS)?;
    put_u64(space, PEB_ADDRESS + 0x20, PROCESS_PARAMS_ADDRESS)?;

    // Build PEB_LDR_DATA circular lists for the main image and every DLL
    // already loaded into this process address space.
    put_u32(space, LDR_ADDRESS, 0x58)?;
    put_u8(space, LDR_ADDRESS + 4, 1)?;
    let load_head=LDR_ADDRESS+0x10; let memory_head=LDR_ADDRESS+0x20; let init_head=LDR_ADDRESS+0x30;
    let mut dlls=[crate::winabi::ModuleInfo::EMPTY;31];
    let dll_count=crate::winabi::module_infos(&mut dlls);
    let module_count=dll_count+1;
    for i in 0..module_count {
        let entry=LDR_ENTRY_ADDRESS+i as u64*0x100;
        let name_address=IMAGE_NAME_ADDRESS+i as u64*0x100;
        let (module_name,module_len,module_base,module_entry,module_size)=if i==0 {
            (None,10usize,image_base,image_entry,image_size)
        } else {
            let m=dlls[i-1];
            (Some(m),m.len,m.base,m.entrypoint,m.size)
        };
        const MAIN_NAME:[u16;10]=[0x52,0x64,0x6f,0x74,0x4f,0x53,0x2e,0x65,0x78,0x65];
        for j in 0..module_len {
            let ch=if let Some(m)=module_name { m.name[j] as u16 } else { MAIN_NAME[j] };
            put_u16(space,name_address+j as u64*2,ch)?;
        }
        let bytes=(module_len*2) as u16;
        put_u16(space,entry+0x48,bytes)?; put_u16(space,entry+0x4a,bytes+2)?;
        put_u64(space,entry+0x50,name_address)?;
        put_u16(space,entry+0x58,bytes)?; put_u16(space,entry+0x5a,bytes+2)?;
        put_u64(space,entry+0x60,name_address)?;
        put_u64(space,entry+0x30,module_base)?;
        put_u64(space,entry+0x38,module_entry)?;
        put_u32(space,entry+0x40,module_size)?;

        let prev=if i==0 {0} else {LDR_ENTRY_ADDRESS+(i as u64-1)*0x100};
        let next=if i+1==module_count {0} else {LDR_ENTRY_ADDRESS+(i as u64+1)*0x100};
        put_u64(space,entry,if i+1==module_count {load_head}else{next})?;
        put_u64(space,entry+8,if i==0 {load_head}else{prev})?;
        put_u64(space,entry+0x10,if i+1==module_count {memory_head}else{next+0x10})?;
        put_u64(space,entry+0x18,if i==0 {memory_head}else{prev+0x10})?;
        put_u64(space,entry+0x20,if i+1==module_count {init_head}else{next+0x20})?;
        put_u64(space,entry+0x28,if i==0 {init_head}else{prev+0x20})?;
    }
    let last=LDR_ENTRY_ADDRESS+(module_count as u64-1)*0x100;
    put_u64(space,load_head,LDR_ENTRY_ADDRESS)?; put_u64(space,load_head+8,last)?;
    put_u64(space,memory_head,LDR_ENTRY_ADDRESS+0x10)?; put_u64(space,memory_head+8,last+0x10)?;
    put_u64(space,init_head,LDR_ENTRY_ADDRESS+0x20)?; put_u64(space,init_head+8,last+0x20)?;

    // RTL_USER_PROCESS_PARAMETERS prefix and Unicode image/command-line strings.
    put_u32(space, PROCESS_PARAMS_ADDRESS, 0x448)?;
    put_u32(space, PROCESS_PARAMS_ADDRESS+4, 0x448)?;
    put_u32(space, PROCESS_PARAMS_ADDRESS+8, 1)?;
    put_u16(space, PROCESS_PARAMS_ADDRESS+0x60, 20)?;
    put_u16(space, PROCESS_PARAMS_ADDRESS+0x62, 22)?;
    put_u64(space, PROCESS_PARAMS_ADDRESS+0x68, IMAGE_NAME_ADDRESS)?;
    put_u16(space, PROCESS_PARAMS_ADDRESS+0x70, 20)?;
    put_u16(space, PROCESS_PARAMS_ADDRESS+0x72, 22)?;
    put_u64(space, PROCESS_PARAMS_ADDRESS+0x78, IMAGE_NAME_ADDRESS)?;
    // NT_TIB-compatible TEB prefix; GS base must point here in user mode.
    put_u64(space, TEB_ADDRESS + 0x08, stack_base)?;
    put_u64(space, TEB_ADDRESS + 0x10, stack_limit)?;
    put_u64(space, TEB_ADDRESS + 0x30, TEB_ADDRESS)?;
    put_u64(space, TEB_ADDRESS + 0x40, pid)?;
    put_u64(space, TEB_ADDRESS + 0x48, 1)?;
    put_u64(space, TEB_ADDRESS + 0x58, TLS_VECTOR_ADDRESS)?;
    put_u64(space, TEB_ADDRESS + 0x60, PEB_ADDRESS)?;
    // The x64 TLS slots begin at offset 0x1480, so TEB occupies two pages.
    put_u64(space, TEB_ADDRESS + 0x1480, 0)?;
    register_process(pid, space.root_physical())?;
    Ok(ProcessEnvironment { process_id: pid, peb: PEB_ADDRESS, teb: TEB_ADDRESS, tls_vector: TLS_VECTOR_ADDRESS })
}

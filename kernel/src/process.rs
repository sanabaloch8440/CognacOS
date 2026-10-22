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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState { Created, Running, Exited, Reaped }
#[derive(Clone, Copy, Debug)]
pub struct ProcessSnapshot { pub process_id: u64, pub state: ProcessState, pub exit_code: u64 }
#[derive(Clone, Copy)]
struct ProcessRecord { process_id: u64, state: ProcessState, exit_code: u64, used: bool }
impl ProcessRecord { const EMPTY: Self = Self { process_id: 0, state: ProcessState::Created, exit_code: 0, used: false }; }
struct ProcessTable(core::cell::UnsafeCell<[ProcessRecord; MAX_PROCESSES]>);
unsafe impl Sync for ProcessTable {}
static TABLE: ProcessTable = ProcessTable(core::cell::UnsafeCell::new([ProcessRecord::EMPTY; MAX_PROCESSES]));
fn register_process(pid: u64) -> Result<(), ProcessError> {
    let table = unsafe { &mut *TABLE.0.get() };
    let Some(slot) = table.iter_mut().find(|r| !r.used) else { return Err(ProcessError::TooManyProcesses); };
    *slot = ProcessRecord { process_id: pid, state: ProcessState::Created, exit_code: 0, used: true };
    Ok(())
}
pub fn mark_running(pid: u64) -> bool {
    let table = unsafe { &mut *TABLE.0.get() };
    if let Some(r) = table.iter_mut().find(|r| r.used && r.process_id == pid && r.state == ProcessState::Created) {
        r.state = ProcessState::Running; true
    } else { false }
}
pub fn mark_exited(pid: u64, code: u64) -> bool {
    let table = unsafe { &mut *TABLE.0.get() };
    if let Some(r) = table.iter_mut().find(|r| r.used && r.process_id == pid && r.state == ProcessState::Running) {
        r.state = ProcessState::Exited; r.exit_code = code; true
    } else { false }
}
pub fn query(pid: u64) -> Option<ProcessSnapshot> {
    let table = unsafe { &*TABLE.0.get() };
    table.iter().find(|r| r.used && r.process_id == pid)
        .map(|r| ProcessSnapshot { process_id: r.process_id, state: r.state, exit_code: r.exit_code })
}
pub fn reap(pid: u64) -> Option<u64> {
    let table = unsafe { &mut *TABLE.0.get() };
    let r = table.iter_mut().find(|r| r.used && r.process_id == pid && r.state == ProcessState::Exited)?;
    let code = r.exit_code; r.state = ProcessState::Reaped; r.used = false; Some(code)
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
            (Some(m),m.len,m.base,0,m.size)
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
    register_process(pid)?;
    Ok(ProcessEnvironment { process_id: pid, peb: PEB_ADDRESS, teb: TEB_ADDRESS, tls_vector: TLS_VECTOR_ADDRESS })
}

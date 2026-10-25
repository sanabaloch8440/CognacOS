//! Initial RdotOS x64 ring-3 syscall ABI; intentionally distinct from NT syscalls.
use core::arch::global_asm;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::ntoskrnl::mm::address_space::AddressSpace;
pub const SYS_EXIT:u64=0;
pub const SYS_GETPID:u64=1;
pub const SYS_WRITE:u64=2;
pub const SYS_YIELD:u64=3;
pub const EXIT_SENTINEL:u64=u64::MAX;
const ERR_BAD_ADDRESS:u64=(-14i64) as u64;
const ERR_INVALID:u64=(-22i64) as u64;
const MAX_WRITE:usize=512;
static CURRENT_PID:AtomicU64=AtomicU64::new(0);
static CALL_COUNT:AtomicU64=AtomicU64::new(0);
static LAST_EXIT:AtomicU64=AtomicU64::new(u64::MAX);
struct ActiveSpace(core::cell::UnsafeCell<*const AddressSpace>);
unsafe impl Sync for ActiveSpace {}
static ACTIVE_SPACE:ActiveSpace=ActiveSpace(core::cell::UnsafeCell::new(core::ptr::null()));
global_asm!(r#"
    .section .text
    .global rdotos_syscall_entry
    .type rdotos_syscall_entry, @function
rdotos_syscall_entry:
    cli
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rbx, rsp
    mov rdi, rsp
    and rsp, -16
    call rdotos_syscall_dispatch
    mov rsp, rbx
    cmp rax, -1
    je rdotos_process_exit_stub
    mov qword ptr [rsp + 112], rax
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq
"#);
unsafe extern "C" { fn rdotos_syscall_entry(); }
pub unsafe fn initialize() {
    unsafe { crate::ntoskrnl::ke::arch::install_user_gate(0x81, rdotos_syscall_entry as usize as u64); }
    unsafe { initialize_native_nt_syscalls(); }
}
pub fn set_current_process(pid:u64,space:*const AddressSpace) {
    LAST_EXIT.store(u64::MAX,Ordering::Release);
    CURRENT_PID.store(pid,Ordering::Release);
    let _ = crate::process::mark_running(pid);
    unsafe { *ACTIVE_SPACE.0.get()=space; }
}
pub fn clear_current_process() {
    CURRENT_PID.store(0,Ordering::Release);
    unsafe { *ACTIVE_SPACE.0.get()=core::ptr::null(); }
}
pub fn dispatch_count()->u64 { CALL_COUNT.load(Ordering::Acquire) }
pub fn last_exit_code()->u64 { LAST_EXIT.load(Ordering::Acquire) }
pub fn dispatch(number:u64,args:[u64;6])->u64 {
    CALL_COUNT.fetch_add(1,Ordering::AcqRel);
    match number {
        SYS_EXIT => { let pid=CURRENT_PID.load(Ordering::Acquire); LAST_EXIT.store(args[0],Ordering::Release); let _=crate::process::mark_exited(pid,args[0]); EXIT_SENTINEL }
        SYS_GETPID => CURRENT_PID.load(Ordering::Acquire),
        SYS_WRITE => {
            let length=core::cmp::min(args[2] as usize,MAX_WRITE);
            if length==0{return 0;}
            let pointer=unsafe{*ACTIVE_SPACE.0.get()};
            if pointer.is_null(){return ERR_BAD_ADDRESS;}
            let space=unsafe{&*pointer};
            let mut buffer=[0u8;MAX_WRITE];
            if unsafe{space.copy_from_user(args[1],&mut buffer[..length])}.is_err(){return ERR_BAD_ADDRESS;}
            unsafe{crate::serial_write(&buffer[..length]);}
            length as u64
        }
        SYS_YIELD=>0, // cooperative placeholder until the scheduler can hand off a thread
        _=>ERR_INVALID,
    }
}

/*
 * CognacOS native NT syscall ABI. NTDLL enters through DPL3 interrupt 0x82;
 * this is an explicit CognacOS transition (not the Windows SYSCALL/MSR ABI).
 * IDs are deliberately separate from Windows-build-specific service numbers.
 */
pub const NT_SYSCALL_CLOSE: u64 = 0x100;
pub const NT_SYSCALL_YIELD_EXECUTION: u64 = 0x101;
pub const NT_SYSCALL_QUERY_INFORMATION_PROCESS: u64 = 0x102;
pub const NT_SYSCALL_QUERY_INFORMATION_THREAD: u64 = 0x103;
pub const NT_SYSCALL_QUERY_SYSTEM_INFORMATION: u64 = 0x104;
pub const NT_MAX_ARGUMENTS: usize = 17;
const NT_STATUS_SUCCESS: u64 = 0;
const NT_STATUS_NOT_IMPLEMENTED: u64 = 0xC000_0002;
const NT_STATUS_INVALID_INFO_CLASS: u64 = 0xC000_0003;
const NT_STATUS_INFO_LENGTH_MISMATCH: u64 = 0xC000_0004;
const NT_STATUS_INVALID_HANDLE: u64 = 0xC000_0008;
const NT_STATUS_INVALID_PARAMETER: u64 = 0xC000_000D;
const NT_STATUS_ACCESS_VIOLATION: u64 = 0xC000_0005;
const NT_CURRENT_PROCESS: u64 = u64::MAX;
const NT_CURRENT_THREAD: u64 = u64::MAX - 1;
const PROCESS_BASIC_INFORMATION_CLASS: u64 = 0;
const THREAD_BASIC_INFORMATION_CLASS: u64 = 0;
const SYSTEM_BASIC_INFORMATION_CLASS: u64 = 0;

global_asm!(r#"
    .section .text
    .global cognacos_nt_syscall_entry
    .type cognacos_nt_syscall_entry, @function
cognacos_nt_syscall_entry:
    cli
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rbx, rsp
    mov rdi, rsp
    and rsp, -16
    call cognacos_nt_syscall_dispatch
    mov rsp, rbx
    mov qword ptr [rsp + 112], rax
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq
"#);
unsafe extern "C" { fn cognacos_nt_syscall_entry(); }

pub unsafe fn initialize_native_nt_syscalls() {
    unsafe { crate::ntoskrnl::ke::arch::install_user_gate(0x82, cognacos_nt_syscall_entry as usize as u64); }
}

fn copy_nt_output(address: u64, bytes: &[u8]) -> Result<(), u64> {
    if bytes.is_empty() { return Ok(()); }
    let pointer = unsafe { *ACTIVE_SPACE.0.get() };
    if pointer.is_null() { return Err(NT_STATUS_ACCESS_VIOLATION); }
    let space = unsafe { &*pointer };
    unsafe { space.copy_to_user(address, bytes) }.map_err(|_| NT_STATUS_ACCESS_VIOLATION)
}

fn write_return_length(address: u64, length: u32) -> Result<(), u64> {
    if address == 0 { return Ok(()); }
    copy_nt_output(address, &length.to_le_bytes())
}

fn nt_query_process(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != NT_CURRENT_PROCESS { return NT_STATUS_INVALID_HANDLE; }
    if args[1] != PROCESS_BASIC_INFORMATION_CLASS { return NT_STATUS_INVALID_INFO_CLASS; }
    const SIZE: usize = 48;
    if args[3] < SIZE as u64 { let _ = write_return_length(args[4], SIZE as u32); return NT_STATUS_INFO_LENGTH_MISMATCH; }
    let pid = CURRENT_PID.load(Ordering::Acquire);
    let mut info = [0u8; SIZE];
    info[0..4].copy_from_slice(&0i32.to_le_bytes()); // ExitStatus
    info[8..16].copy_from_slice(&(crate::process::PEB_ADDRESS).to_le_bytes());
    info[16..24].copy_from_slice(&1u64.to_le_bytes()); // minimal single-processor affinity
    info[24..28].copy_from_slice(&8i32.to_le_bytes()); // normal base priority
    info[32..40].copy_from_slice(&pid.to_le_bytes());
    info[40..48].copy_from_slice(&0u64.to_le_bytes()); // inherited PID not tracked yet
    if let Err(status) = write_return_length(args[4], SIZE as u32) { return status; }
    if let Err(status) = copy_nt_output(args[2], &info) { return status; }
    NT_STATUS_SUCCESS
}

fn nt_query_thread(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != NT_CURRENT_THREAD { return NT_STATUS_INVALID_HANDLE; }
    if args[1] != THREAD_BASIC_INFORMATION_CLASS { return NT_STATUS_INVALID_INFO_CLASS; }
    const SIZE: usize = 48;
    if args[3] < SIZE as u64 { let _ = write_return_length(args[4], SIZE as u32); return NT_STATUS_INFO_LENGTH_MISMATCH; }
    let mut info = [0u8; SIZE];
    info[0..4].copy_from_slice(&0i32.to_le_bytes()); // ExitStatus
    info[8..16].copy_from_slice(&crate::process::TEB_ADDRESS.to_le_bytes());
    info[16..24].copy_from_slice(&CURRENT_PID.load(Ordering::Acquire).to_le_bytes());
    info[24..32].copy_from_slice(&(crate::ntoskrnl::ke::thread::current_thread_id().unwrap_or(0)).to_le_bytes());
    info[32..40].copy_from_slice(&1u64.to_le_bytes()); // affinity
    info[40..44].copy_from_slice(&8i32.to_le_bytes()); // priority
    info[44..48].copy_from_slice(&8i32.to_le_bytes()); // base priority
    if let Err(status) = write_return_length(args[4], SIZE as u32) { return status; }
    if let Err(status) = copy_nt_output(args[2], &info) { return status; }
    NT_STATUS_SUCCESS
}

fn nt_query_system(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] != SYSTEM_BASIC_INFORMATION_CLASS { return NT_STATUS_INVALID_INFO_CLASS; }
    const SIZE: usize = 64;
    if args[2] < SIZE as u64 { let _ = write_return_length(args[3], SIZE as u32); return NT_STATUS_INFO_LENGTH_MISMATCH; }
    let mut info = [0u8; SIZE];
    info[0..4].copy_from_slice(&0u32.to_le_bytes()); // Reserved
    info[4..8].copy_from_slice(&10000u32.to_le_bytes()); // TimerResolution (100 ns units)
    info[8..12].copy_from_slice(&4096u32.to_le_bytes()); // PageSize
    info[12..16].copy_from_slice(&0u32.to_le_bytes()); // physical page count not yet exported
    info[16..20].copy_from_slice(&0u32.to_le_bytes()); // LowestPhysicalPageNumber
    info[20..24].copy_from_slice(&0u32.to_le_bytes()); // HighestPhysicalPageNumber not yet exported
    info[24..28].copy_from_slice(&65536u32.to_le_bytes()); // AllocationGranularity
    info[32..40].copy_from_slice(&0x0000_0000_0001_0000u64.to_le_bytes()); // MinimumUserModeAddress
    info[40..48].copy_from_slice(&0x0000_8000_0000_0000u64.to_le_bytes()); // MaximumUserModeAddress
    info[48..56].copy_from_slice(&1u64.to_le_bytes()); // minimal single-processor affinity
    info[56] = 1; // NumberOfProcessors
    if let Err(status) = write_return_length(args[3], SIZE as u32) { return status; }
    if let Err(status) = copy_nt_output(args[1], &info) { return status; }
    NT_STATUS_SUCCESS
}

fn dispatch_native_nt(number: u64, args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    match number {
        NT_SYSCALL_CLOSE => {
            let status = unsafe { crate::ntoskrnl::ob::runtime::ObCloseHandle(args[0] as *mut core::ffi::c_void, 1) };
            status as u32 as u64
        }
        NT_SYSCALL_YIELD_EXECUTION => {
            let _ = crate::ntoskrnl::ke::thread::yield_current();
            NT_STATUS_SUCCESS
        }
        NT_SYSCALL_QUERY_INFORMATION_PROCESS => nt_query_process(args),
        NT_SYSCALL_QUERY_INFORMATION_THREAD => nt_query_thread(args),
        NT_SYSCALL_QUERY_SYSTEM_INFORMATION => nt_query_system(args),
        _ => NT_STATUS_NOT_IMPLEMENTED,
    }
}

/// Decode the actual x64 syscall frame and copy stack arguments from the caller's
/// user address space before dispatch. User-controlled pointers are never directly
/// dereferenced by the service handlers.
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_nt_syscall_dispatch(frame: *mut u64) -> u64 {
    if frame.is_null() { return NT_STATUS_ACCESS_VIOLATION; }
    CALL_COUNT.fetch_add(1, Ordering::AcqRel);
    unsafe {
        // Saved register frame: r15,r14,r13,r12,r11,r10,r9,r8,rbp,rdi,rsi,rdx,rcx,rbx,rax.
        let f = core::slice::from_raw_parts(frame, 20);
        let number = f[14];
        let user_rsp = f[18];
        let mut args = [0u64; NT_MAX_ARGUMENTS];
        args[0] = f[5];  // R10 = arg1
        args[1] = f[11]; // RDX = arg2
        args[2] = f[7];  // R8 = arg3
        args[3] = f[6];  // R9 = arg4
        let count = match number {
            NT_SYSCALL_CLOSE => 1,
            NT_SYSCALL_YIELD_EXECUTION => 0,
            NT_SYSCALL_QUERY_INFORMATION_PROCESS | NT_SYSCALL_QUERY_INFORMATION_THREAD => 5,
            NT_SYSCALL_QUERY_SYSTEM_INFORMATION => 4,
            _ => return NT_STATUS_NOT_IMPLEMENTED,
        };
        if count > 4 {
            let extra = count - 4;
            let mut raw = [0u8; (NT_MAX_ARGUMENTS - 4) * 8];
            let pointer = ACTIVE_SPACE.0.get().read();
            if pointer.is_null() { return NT_STATUS_ACCESS_VIOLATION; }
            if (*pointer).copy_from_user(user_rsp.saturating_add(40), &mut raw[..extra * 8]).is_err() {
                return NT_STATUS_ACCESS_VIOLATION;
            }
            for index in 0..extra {
                args[index + 4] = u64::from_le_bytes(raw[index * 8..index * 8 + 8].try_into().unwrap());
            }
        }
        dispatch_native_nt(number, &args)
    }
}

/// Boot/QEMU regression entry for the native dispatcher. Exercises the same
/// service decoder and checked user-copy helpers with a real test address space.
pub fn native_nt_dispatch_self_test(space: &AddressSpace, user_base: u64) -> bool {
    let previous_pid = CURRENT_PID.swap(4, Ordering::AcqRel);
    let previous_space = unsafe { *ACTIVE_SPACE.0.get() };
    unsafe { *ACTIVE_SPACE.0.get() = space as *const AddressSpace; }
    let mut stack5 = [0u8; 8];
    stack5.copy_from_slice(&(user_base + 0x280).to_le_bytes());
    let five_stack_written = unsafe { space.copy_to_user(user_base + 0x300, &stack5) }.is_ok();
    // Construct the same saved-register/IRET frame the real vector-0x82 entry passes.
    let mut frame = [0u64; 20];
    frame[14] = NT_SYSCALL_QUERY_INFORMATION_PROCESS;
    frame[18] = user_base + 0x2d8;
    frame[5] = NT_CURRENT_PROCESS;
    frame[11] = PROCESS_BASIC_INFORMATION_CLASS;
    frame[7] = user_base + 0x200;
    frame[6] = 48;
    let five_arg_ok = five_stack_written
        && unsafe { cognacos_nt_syscall_dispatch(frame.as_mut_ptr()) } == NT_STATUS_SUCCESS;
    let mut output = [0u8; 4];
    let return_len_ok = unsafe { space.copy_from_user(user_base + 0x280, &mut output) }.is_ok()
        && u32::from_le_bytes(output) == 48;
    frame[7] = 0x0000_7fff_ffff_f000;
    let invalid_buffer_ok = unsafe { cognacos_nt_syscall_dispatch(frame.as_mut_ptr()) } == NT_STATUS_ACCESS_VIOLATION;
    frame[14] = 0xdead;
    let unsupported_ok = unsafe { cognacos_nt_syscall_dispatch(frame.as_mut_ptr()) } == NT_STATUS_NOT_IMPLEMENTED;
    // Decode all 13 stack arguments (arguments 5..17) from user memory.
    let mut stack = [0u8; (NT_MAX_ARGUMENTS - 4) * 8];
    for (i, chunk) in stack.chunks_exact_mut(8).enumerate() {
        chunk.copy_from_slice(&((i as u64) + 5).to_le_bytes());
    }
    let stack_ok = unsafe { space.copy_to_user(user_base + 0x300, &stack) }.is_ok()
        && decode_nt_stack_arguments(space, user_base + 0x2d8, 0xdead, [1,2,3,4], NT_MAX_ARGUMENTS) == NT_STATUS_NOT_IMPLEMENTED;
    unsafe { *ACTIVE_SPACE.0.get() = previous_space; }
    CURRENT_PID.store(previous_pid, Ordering::Release);
    five_arg_ok && return_len_ok && invalid_buffer_ok && unsupported_ok && stack_ok
}

fn decode_nt_stack_arguments(space: &AddressSpace, user_rsp: u64, number: u64, first: [u64; 4], count: usize) -> u64 {
    if count > NT_MAX_ARGUMENTS || count < 4 { return NT_STATUS_INVALID_PARAMETER; }
    let extra = count - 4;
    let mut raw = [0u8; (NT_MAX_ARGUMENTS - 4) * 8];
    if unsafe { space.copy_from_user(user_rsp + 40, &mut raw[..extra * 8]) }.is_err() {
        return NT_STATUS_ACCESS_VIOLATION;
    }
    let mut args = [0u64; NT_MAX_ARGUMENTS];
    args[..4].copy_from_slice(&first);
    for index in 0..extra {
        args[index + 4] = u64::from_le_bytes(raw[index*8..index*8+8].try_into().unwrap());
    }
    dispatch_native_nt(number, &args)
}

/// Prepare a user-mode APC by turning the interrupted return context into
/// a normal x64 call frame. The interrupted RIP is placed at user RSP-8 as the
/// routine's return address; RDX/R8 carry the two APC arguments.
pub fn prepare_user_apc_return(frame:*mut u64)->bool {
    if frame.is_null(){return false}
    unsafe {
        // Saved registers occupy [0..15], followed by the five-word IRET frame.
        if (*frame.add(16) & 3) != 3 { return false; }
        let original_rip=*frame.add(15);
        let original_rsp=*frame.add(18);
        if original_rsp<8 || original_rsp>=0x0000_8000_0000_0000 { return false; }
        let space_ptr=*ACTIVE_SPACE.0.get();
        if space_ptr.is_null(){return false}
        let space=&*space_ptr;
        if !crate::thread::take_user_apc_delivery_pending(){return false;}
        let Some((object,normal,context,arg1,arg2))=crate::ntoskrnl::ke::runtime::take_pending_user_apc() else{return false};
        if normal==0 {
            crate::ntoskrnl::ke::runtime::complete_user_apc_delivery(object);
            return false;
        }
        if !space.is_user_executable(normal as u64) {
            if crate::ntoskrnl::ke::runtime::requeue_pending_user_apc(object,normal,context,arg1,arg2) {
                let _=crate::thread::set_user_apc_delivery_pending(crate::thread::current_thread_id().unwrap_or(0),true);
            }
            return false;
        }
        let return_slot=original_rsp-8;
        if space.copy_to_user(return_slot,&original_rip.to_ne_bytes()).is_err(){
            if crate::ntoskrnl::ke::runtime::requeue_pending_user_apc(object,normal,context,arg1,arg2) {
                let _=crate::thread::set_user_apc_delivery_pending(crate::thread::current_thread_id().unwrap_or(0),true);
            }
            return false;
        }
        *frame.add(15)=normal as u64;
        *frame.add(18)=return_slot;
        *frame.add(12)=context as u64; // RCX = NormalContext
        *frame.add(11)=arg1 as u64;     // RDX = SystemArgument1
        *frame.add(7)=arg2 as u64;     // R8  = SystemArgument2
        crate::ntoskrnl::ke::runtime::complete_user_apc_delivery(object);
        true
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn rdotos_syscall_dispatch(frame:*mut u64)->u64 {
    if frame.is_null(){return ERR_BAD_ADDRESS;}
    let result=unsafe {
        // Saved stack: r15,r14,r13,r12,r11,r10,r9,r8,rbp,rdi,rsi,rdx,rcx,rbx,rax.
        let f=core::slice::from_raw_parts(frame,15);
        dispatch(f[14],[f[9],f[10],f[11],f[5],f[7],f[6]])
    };
    if result!=EXIT_SENTINEL {
        crate::ntoskrnl::ke::runtime::ke_deliver_pending_kernel_apcs();
        let _=prepare_user_apc_return(frame);
    }
    result
}

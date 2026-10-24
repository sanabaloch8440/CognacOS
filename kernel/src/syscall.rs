//! Initial RdotOS x64 ring-3 syscall ABI; intentionally distinct from NT syscalls.
use core::arch::global_asm;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::vm::AddressSpace;
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
    unsafe { crate::arch::install_user_gate(0x81, rdotos_syscall_entry as usize as u64); }
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
#[unsafe(no_mangle)]
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
        let _=object;
        if normal==0 || !space.is_user_executable(normal as u64) { return false; }
        let return_slot=original_rsp-8;
        if space.copy_to_user(return_slot,&original_rip.to_ne_bytes()).is_err(){return false}
        *frame.add(15)=normal as u64;
        *frame.add(18)=return_slot;
        *frame.add(12)=context as u64; // RCX = NormalContext
        *frame.add(11)=arg1 as u64;     // RDX = SystemArgument1
        *frame.add(7)=arg2 as u64;     // R8  = SystemArgument2
        true
    }
}
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

//! Windows x64 NT syscall dispatch plus the separate process-exit gate.
use core::arch::global_asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
// Bounded boot trace prevents serial floods while preserving the last early NT calls.
static NT_SYSCALL_TRACE_COUNT: AtomicU64 = AtomicU64::new(0);
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
    # Save user XSTATE before any Rust syscall dispatch can touch SIMD state.
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
    mov rdi, rsp
    and rsp, -16
    call rdotos_syscall_dispatch
    mov rsp, rbx
    cmp rax, -1
    je rdotos_process_exit_stub
    mov qword ptr [rsp + 112], rax
    # Restore the current thread's user state after dispatch/scheduling and
    # immediately before restoring registers and returning with IRETQ.
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lrdotos_syscall_no_xstate_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lrdotos_syscall_no_xstate_restore:
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
pub fn current_process_id() -> u64 { CURRENT_PID.load(Ordering::Acquire) }
/// Checked copy helpers for native syscall adapters. They never dereference a
/// caller-controlled address directly in kernel mode.
pub fn copy_current_user_from(address:u64, output:&mut [u8])->bool {
    let pointer=unsafe{*ACTIVE_SPACE.0.get()};
    !pointer.is_null() && unsafe{(&*pointer).copy_from_user(address,output)}.is_ok()
}
pub fn copy_current_user_to(address:u64, input:&[u8])->bool {
    let pointer=unsafe{*ACTIVE_SPACE.0.get()};
    !pointer.is_null() && unsafe{(&*pointer).copy_to_user(address,input)}.is_ok()
}
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
 * Internal kernel dispatch IDs for native NT services. User-mode NTDLL enters
 * only through the architectural Windows x64 SYSCALL/MSR ABI; exported service
 * numbers are discovered from this Rust NTDLL's PE stubs and translated here
 * to stable internal handlers.
 */
pub const NT_SYSCALL_CLOSE: u64 = 0x100;
pub const NT_SYSCALL_YIELD_EXECUTION: u64 = 0x101;
pub const NT_SYSCALL_QUERY_INFORMATION_PROCESS: u64 = 0x102;
pub const NT_SYSCALL_OPEN_PROCESS: u64 = 0x150;
pub const NT_SYSCALL_GET_NEXT_PROCESS: u64 = 0x151;
pub const NT_SYSCALL_QUERY_INFORMATION_THREAD: u64 = 0x103;
pub const NT_SYSCALL_QUERY_SYSTEM_INFORMATION: u64 = 0x104;
pub const NT_SYSCALL_OPEN_KEY: u64 = 0x105;
pub const NT_SYSCALL_QUERY_VALUE_KEY: u64 = 0x106;
pub const NT_SYSCALL_OPEN_FILE: u64 = 0x107;
pub const NT_SYSCALL_CREATE_KEY: u64 = 0x145;
pub const NT_SYSCALL_SET_VALUE_KEY: u64 = 0x146;
pub const NT_SYSCALL_DELETE_VALUE_KEY: u64 = 0x147;
pub const NT_SYSCALL_DELETE_KEY: u64 = 0x148;
pub const NT_SYSCALL_FLUSH_KEY: u64 = 0x149;
pub const NT_SYSCALL_ENUMERATE_KEY: u64 = 0x14a;
pub const NT_SYSCALL_ENUMERATE_VALUE_KEY: u64 = 0x14b;
pub const NT_SYSCALL_QUERY_KEY: u64 = 0x14c;
pub const NT_SYSCALL_READ_FILE: u64 = 0x108;
pub const NT_SYSCALL_ALLOCATE_VIRTUAL_MEMORY: u64 = 0x109;
pub const NT_SYSCALL_FREE_VIRTUAL_MEMORY: u64 = 0x10a;
pub const NT_SYSCALL_PROTECT_VIRTUAL_MEMORY: u64 = 0x10b;
pub const NT_SYSCALL_QUERY_VIRTUAL_MEMORY: u64 = 0x10c;
pub const NT_SYSCALL_READ_VIRTUAL_MEMORY: u64 = 0x10d;
pub const NT_SYSCALL_WRITE_VIRTUAL_MEMORY: u64 = 0x10e;
pub const NT_SYSCALL_LOCK_VIRTUAL_MEMORY: u64 = 0x10f;
pub const NT_SYSCALL_UNLOCK_VIRTUAL_MEMORY: u64 = 0x110;
pub const NT_SYSCALL_GET_NONVOLATILE_TOKEN: u64 = 0x111;
pub const NT_SYSCALL_FREE_NONVOLATILE_TOKEN: u64 = 0x112;
pub const NT_SYSCALL_FLUSH_NONVOLATILE_MEMORY: u64 = 0x113;
pub const NT_SYSCALL_WRITE_NONVOLATILE_MEMORY: u64 = 0x114;
pub const NT_SYSCALL_FILL_NONVOLATILE_MEMORY: u64 = 0x115;
pub const NT_SYSCALL_CHECK_TOKEN_MEMBERSHIP: u64 = 0x116;
pub const NT_SYSCALL_CREATE_SECTION: u64 = 0x117;
pub const NT_SYSCALL_DEREFERENCE_SECTION: u64 = 0x118;
pub const NT_SYSCALL_SECTION_COPY: u64 = 0x119;
pub const NT_SYSCALL_REFERENCE_SECTION: u64 = 0x11a;
pub const NT_SYSCALL_ALPC_CREATE_PORT:u64=0x11b;
pub const NT_SYSCALL_ALPC_DISCONNECT_PORT:u64=0x11c;
pub const NT_SYSCALL_ALPC_CONNECT_PORT:u64=0x11d;
pub const NT_SYSCALL_ALPC_ACCEPT_CONNECT_PORT:u64=0x11e;
pub const NT_SYSCALL_ALPC_SEND_WAIT_RECEIVE_PORT:u64=0x11f;
pub const NT_SYSCALL_ALPC_CREATE_PORT_SECTION:u64=0x120;
pub const NT_SYSCALL_ALPC_DELETE_PORT_SECTION:u64=0x121;
pub const NT_SYSCALL_ALPC_CREATE_SECTION_VIEW:u64=0x122;
pub const NT_SYSCALL_ALPC_DELETE_SECTION_VIEW:u64=0x123;
pub const NT_SYSCALL_ALPC_CREATE_SECURITY_CONTEXT:u64=0x124;
pub const NT_SYSCALL_ALPC_DELETE_SECURITY_CONTEXT:u64=0x125;
pub const NT_SYSCALL_ALPC_REVOKE_SECURITY_CONTEXT:u64=0x126;
pub const NT_SYSCALL_ALPC_CREATE_RESOURCE_RESERVE:u64=0x127;
pub const NT_SYSCALL_ALPC_DELETE_RESOURCE_RESERVE:u64=0x128;
pub const NT_SYSCALL_ALPC_QUERY_INFORMATION:u64=0x129;
pub const NT_SYSCALL_ALPC_SET_INFORMATION:u64=0x12a;
pub const NT_SYSCALL_ALPC_QUERY_INFORMATION_MESSAGE:u64=0x12b;
pub const NT_SYSCALL_ALPC_CANCEL_MESSAGE:u64=0x12c;
pub const NT_SYSCALL_ALPC_OPEN_SENDER_PROCESS:u64=0x12d;
pub const NT_SYSCALL_ALPC_OPEN_SENDER_THREAD:u64=0x12e;
pub const NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_OF_PORT:u64=0x12f;
pub const NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_CONTAINER_OF_PORT:u64=0x130;
pub const NT_SYSCALL_ALPC_CONNECT_PORT_EX:u64=0x131;
pub const NT_SYSCALL_NT_CREATE_SECTION:u64=0x132;
pub const NT_SYSCALL_ETW_REGISTER:u64=0x133;
pub const NT_SYSCALL_ETW_ENABLE_STATE:u64=0x134;
pub const NT_SYSCALL_ETW_WRITE_EVENT:u64=0x135;
pub const NT_SYSCALL_ETW_READ_EVENT:u64=0x136;
pub const NT_SYSCALL_ETW_CREATE_SESSION:u64=0x138;
pub const NT_SYSCALL_ETW_ENABLE_SESSION:u64=0x139;
pub const NT_SYSCALL_ETW_STOP_SESSION:u64=0x13a;
pub const NT_SYSCALL_ETW_UNREGISTER:u64=0x13b;
pub const NT_SYSCALL_ETW_CREATE_NAMED_SESSION:u64=0x13c;
pub const NT_SYSCALL_ETW_QUERY_EVENT_ENABLED:u64=0x13d;
pub const NT_SYSCALL_REGISTER_FUNCTION_TABLE:u64=0x13e;
pub const NT_SYSCALL_DELETE_FUNCTION_TABLE:u64=0x13f;
pub const NT_SYSCALL_LOOKUP_FUNCTION_ENTRY:u64=0x140;
pub const NT_SYSCALL_NT_CONTINUE:u64=0x141;
pub const NT_SYSCALL_NT_RAISE_EXCEPTION:u64=0x142;
pub const NT_SYSCALL_REGISTER_EXCEPTION_DISPATCHER:u64=0x143;
pub const NT_SYSCALL_DEBUG_EVENT: u64 = 0x144;
/// CognacOS private user-address wait/wake services used by NTDLL's user-mode
/// synchronization primitives. These are not Windows-exported NT syscall IDs.
pub const NT_SYSCALL_WAIT_ON_ADDRESS: u64 = 0x152;
pub const NT_SYSCALL_WAKE_ADDRESS: u64 = 0x153;
pub const NT_SYSCALL_WAKE_ALL_ADDRESS: u64 = 0x154;
pub const NT_SYSCALL_CREATE_EVENT: u64 = 0x155;
pub const NT_SYSCALL_SET_EVENT: u64 = 0x156;
pub const NT_SYSCALL_RESET_EVENT: u64 = 0x157;
pub const NT_SYSCALL_WAIT_FOR_SINGLE_OBJECT: u64 = 0x158;
pub const NT_SYSCALL_OPEN_EVENT: u64 = 0x159;

#[derive(Clone, Copy)]
struct UserAddressWaiter { active: bool, process: u64, address: u64, thread: u64, deadline: u64 }
impl UserAddressWaiter { const EMPTY: Self = Self { active:false, process:0, address:0, thread:0, deadline:0 }; }
static USER_ADDRESS_WAITERS: crate::hal::IrqSpinLock<[UserAddressWaiter; 128]> =
    crate::hal::IrqSpinLock::new([UserAddressWaiter::EMPTY; 128]);

fn wait_on_user_address(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    let address=args[0];
    if address==0 || !(args[2]==4||args[2]==8) { return NT_STATUS_INVALID_PARAMETER; }
    if args[3]==0 { return 0x0000_0102; } // STATUS_TIMEOUT
    let Some(thread)=crate::thread::current_thread_id() else { return NT_STATUS_NOT_IMPLEMENTED; };
    let process=current_process_id();
    if process==0 { return NT_STATUS_NOT_IMPLEMENTED; }
    let deadline=if args[3]==u64::MAX {0}else{
        let Some(hz)=crate::hal::query_performance_frequency_hz().filter(|v|*v!=0) else{return NT_STATUS_NOT_IMPLEMENTED;};
        let ticks=((args[3] as u128)*(hz as u128)+999)/1000;
        match crate::hal::query_performance_counter().checked_add(ticks.min(u64::MAX as u128) as u64){Some(v)=>v.max(1),None=>return NT_STATUS_NOT_IMPLEMENTED}
    };
    let mut current=[0u8;8];
    let mut waiters=USER_ADDRESS_WAITERS.lock();
    if !copy_current_user_from(address,&mut current[..args[2] as usize]) { return NT_STATUS_ACCESS_VIOLATION; }
    let observed=if args[2]==4 {u32::from_le_bytes(current[..4].try_into().unwrap()) as u64}else{u64::from_le_bytes(current)};
    if observed!=args[1] { return NT_STATUS_SUCCESS; }
    let Some(slot)=waiters.iter().position(|w|!w.active) else { return 0xC000_009A; };
    // Registration and the value recheck share the same lock as wake. A wake
    // cannot pass between the value check and waiter publication unnoticed.
    waiters[slot]=UserAddressWaiter{active:true,process,address,thread,deadline};
    drop(waiters);
    if crate::thread::block_current().is_err() {
        let mut waiters=USER_ADDRESS_WAITERS.lock();
        if let Some(slot)=waiters.iter().position(|w|w.active&&w.process==process&&w.address==address&&w.thread==thread) {
            waiters[slot]=UserAddressWaiter::EMPTY;
        }
        return NT_STATUS_NOT_IMPLEMENTED;
    }
    let mut waiters=USER_ADDRESS_WAITERS.lock();
    if let Some(slot)=waiters.iter().position(|w|w.active&&w.process==process&&w.address==address&&w.thread==thread) {
        waiters[slot]=UserAddressWaiter::EMPTY;
    }
    crate::thread::take_wait_status().unwrap_or(NT_STATUS_SUCCESS as i32) as u32 as u64
}

/// Expire finite NTDLL address waits from the kernel clock interrupt path.
pub(crate) fn expire_user_address_waits(now:u64) {
    let mut expired=[0u64;128];
    let mut count=0usize;
    {
        let mut waiters=USER_ADDRESS_WAITERS.lock();
        for waiter in waiters.iter_mut() {
            if waiter.active&&waiter.deadline!=0&&waiter.deadline<=now&&count<expired.len(){
                expired[count]=waiter.thread;count+=1;*waiter=UserAddressWaiter::EMPTY;
            }
        }
    }
    for thread in expired[..count].iter().copied(){let _=crate::thread::complete_wait(thread,0x0000_0102);}
}

fn wake_user_address(args: &[u64; NT_MAX_ARGUMENTS], all: bool) -> u64 {
    let address=args[0];
    if address==0 { return NT_STATUS_INVALID_PARAMETER; }
    let process=current_process_id();
    let mut wake=[0u64;128];
    let mut count=0usize;
    {
        let mut waiters=USER_ADDRESS_WAITERS.lock();
        for waiter in waiters.iter_mut() {
            if waiter.active&&waiter.process==process&&waiter.address==address {
                if count<wake.len(){wake[count]=waiter.thread;count+=1;}
                *waiter=UserAddressWaiter::EMPTY;
                if !all { break; }
            }
        }
    }
    for thread in wake[..count].iter().copied() {
        let _=crate::thread::complete_wait(thread,0);
    }
    NT_STATUS_SUCCESS
}


pub const NT_MAX_ARGUMENTS: usize = 17;
const NT_STATUS_SUCCESS: u64 = 0;
const NT_STATUS_NOT_IMPLEMENTED: u64 = 0xC000_0002;
const NT_STATUS_INVALID_INFO_CLASS: u64 = 0xC000_0003;
const NT_STATUS_INFO_LENGTH_MISMATCH: u64 = 0xC000_0004;
const NT_STATUS_INVALID_HANDLE: u64 = 0xC000_0008;
const NT_STATUS_INVALID_PARAMETER: u64 = 0xC000_000D;
const NT_STATUS_ACCESS_VIOLATION: u64 = 0xC000_0005;
const ETW_PROVIDER_CAPACITY: usize = 64;
const ETW_EVENT_CAPACITY: usize = 128;
const ETW_PAYLOAD_CAPACITY: usize = 256;
static ETW_KERNEL_LOCK: AtomicBool = AtomicBool::new(false);
static ETW_KERNEL_NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy)]
struct KernelEtwSubscription { enabled: bool, level: u8, _pad: [u8;6], any: u64, all: u64 }
impl KernelEtwSubscription { const EMPTY: Self = Self { enabled:false, level:0, _pad:[0;6], any:0, all:0 }; }
#[derive(Clone, Copy)]
struct KernelEtwProvider { active: bool, handle: u64, owner: u64, guid: [u8;16], subscriptions: [KernelEtwSubscription;16] }
impl KernelEtwProvider { const EMPTY: Self = Self { active:false, handle:0, owner:0, guid:[0;16], subscriptions:[KernelEtwSubscription::EMPTY;16] }; }
#[derive(Clone, Copy)]
struct KernelEtwSession { active: bool, handle: u64, owner: u64, name_len: u8, _pad: [u8;7], name: [u16;64] }
impl KernelEtwSession { const EMPTY: Self = Self { active:false, handle:0, owner:0, name_len:0, _pad:[0;7], name:[0;64] }; }
#[derive(Clone, Copy)]
struct KernelEtwEvent { active: bool, _pad:[u8;5], session_mask:u16, provider: u64, pid:u64, tid:u64, descriptor:[u8;16], activity:[u8;16], related_activity:[u8;16], length:u16, _pad2:[u8;6], payload:[u8;ETW_PAYLOAD_CAPACITY] }
impl KernelEtwEvent { const EMPTY: Self = Self { active:false,_pad:[0;5],session_mask:0,provider:0,pid:0,tid:0,descriptor:[0;16],activity:[0;16],related_activity:[0;16],length:0,_pad2:[0;6],payload:[0;ETW_PAYLOAD_CAPACITY] }; }
static mut ETW_KERNEL_PROVIDERS:[KernelEtwProvider;ETW_PROVIDER_CAPACITY]=[KernelEtwProvider::EMPTY;ETW_PROVIDER_CAPACITY];
static mut ETW_KERNEL_SESSIONS:[KernelEtwSession;16]=[KernelEtwSession::EMPTY;16];
static mut ETW_KERNEL_EVENTS:[KernelEtwEvent;ETW_EVENT_CAPACITY]=[KernelEtwEvent::EMPTY;ETW_EVENT_CAPACITY];
static mut ETW_KERNEL_HEAD:usize=0;
static mut ETW_KERNEL_LEN:usize=0;
struct KernelEtwGuard;
impl KernelEtwGuard { fn lock()->Self { while ETW_KERNEL_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();} Self } }
impl Drop for KernelEtwGuard { fn drop(&mut self){ETW_KERNEL_LOCK.store(false,Ordering::Release);} }
fn etw_register(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 if args[0]==0 || args[1]==0 { return NT_STATUS_INVALID_PARAMETER; }
 let mut guid=[0u8;16]; if let Err(s)=copy_nt_input(args[0],&mut guid){return s;}
 let pid=current_process_id(); if pid==0{return NT_STATUS_ACCESS_VIOLATION;}
 let _g=KernelEtwGuard::lock();
 unsafe {
  let table=core::ptr::addr_of_mut!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();
  if let Some(i)=(0..ETW_PROVIDER_CAPACITY).find(|&i|!(*table.add(i)).active) {
   let handle=ETW_KERNEL_NEXT.fetch_add(1,Ordering::Relaxed).max(1);
   *table.add(i)=KernelEtwProvider{active:true,handle,owner:pid,guid,subscriptions:[KernelEtwSubscription::EMPTY;16]};
   if let Err(s)=copy_nt_output(args[1],&handle.to_le_bytes()){*table.add(i)=KernelEtwProvider::EMPTY;return s;}
   return NT_STATUS_SUCCESS;
  }
 }
 0xC000009Au64
}
fn etw_unregister(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 let handle=args[0];let pid=current_process_id();let _g=KernelEtwGuard::lock();
 unsafe {
  let providers=core::ptr::addr_of_mut!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();
  let Some(i)=(0..ETW_PROVIDER_CAPACITY).find(|&i|{let p=&*providers.add(i);p.active&&p.handle==handle&&p.owner==pid})else{return NT_STATUS_INVALID_HANDLE;};
  *providers.add(i)=KernelEtwProvider::EMPTY;
  let events=core::ptr::addr_of_mut!(ETW_KERNEL_EVENTS).cast::<KernelEtwEvent>();
  let old_head=ETW_KERNEL_HEAD;let old_len=ETW_KERNEL_LEN;let mut kept=0usize;
  for off in 0..old_len {let src=(old_head+off)%ETW_EVENT_CAPACITY;let e=*events.add(src);if !e.active||e.provider==handle{*events.add(src)=KernelEtwEvent::EMPTY;continue;}let dst=(old_head+kept)%ETW_EVENT_CAPACITY;if dst!=src{*events.add(dst)=e;*events.add(src)=KernelEtwEvent::EMPTY;}kept+=1;}
  ETW_KERNEL_LEN=kept;if kept==0{ETW_KERNEL_HEAD=0;}
 }
 NT_STATUS_SUCCESS
}
fn etw_enable_state(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 if args[0]==0 || args[1]==0{return NT_STATUS_INVALID_PARAMETER;}
 let handle=args[0]; let mut state=[0u8;24];
 let _g=KernelEtwGuard::lock();
 unsafe {
  let table=core::ptr::addr_of!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();
  let Some(i)=(0..ETW_PROVIDER_CAPACITY).find(|&i|{let p=&*table.add(i);p.active&&p.handle==handle&&p.owner==current_process_id()}) else{return NT_STATUS_INVALID_HANDLE;};
  let p=&*table.add(i);let mut enabled=false;let mut level=0u8;let mut any=0u64;let mut all=0u64;for sub in p.subscriptions.iter(){if sub.enabled{enabled=true;level=level.max(sub.level);any|=sub.any;all|=sub.all;}}state[0]=enabled as u8;state[1]=level;state[8..16].copy_from_slice(&any.to_le_bytes());state[16..24].copy_from_slice(&all.to_le_bytes());
 }
 copy_nt_output(args[1],&state).map(|_|NT_STATUS_SUCCESS).unwrap_or_else(|s|s)
}
fn etw_query_event_enabled(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 if args[1]==0{return NT_STATUS_INVALID_PARAMETER;}
 let handle=args[0];let mut descriptor=[0u8;16];
 if let Err(status)=copy_nt_input(args[1],&mut descriptor){return status;}
 let level=descriptor[4];let keyword=u64::from_le_bytes(descriptor[8..16].try_into().unwrap());
 let pid=current_process_id();let _g=KernelEtwGuard::lock();
 unsafe {
  let providers=core::ptr::addr_of!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();
  let Some(pi)=(0..ETW_PROVIDER_CAPACITY).find(|&i|{let p=&*providers.add(i);p.active&&p.handle==handle&&p.owner==pid})else{return NT_STATUS_INVALID_HANDLE;};
  let sessions=core::ptr::addr_of!(ETW_KERNEL_SESSIONS).cast::<KernelEtwSession>();
  let p=&*providers.add(pi);
  for si in 0..16 {
   let sub=&p.subscriptions[si];let session=&*sessions.add(si);
   if !sub.enabled||!session.active{continue;}
   if sub.level!=0&&level!=0&&level>sub.level{continue;}
   if keyword!=0&&sub.any!=0&&sub.any&keyword==0{continue;}
   if keyword!=0&&sub.all!=0&&keyword&sub.all!=sub.all{continue;}
   return 1;
  }
 }
 0
}

fn etw_write_event(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 if args[1]==0 || args[2]==0 || args[3]>ETW_PAYLOAD_CAPACITY as u64{return NT_STATUS_INVALID_PARAMETER;}
 let handle=args[0]; let mut wire=[0u8;48]; let mut descriptor=[0u8;16]; let mut payload=[0u8;ETW_PAYLOAD_CAPACITY];
 if let Err(s)=copy_nt_input(args[1],&mut wire){return s;}
 descriptor.copy_from_slice(&wire[..16]);
 let n=args[3] as usize;if n>0 {if let Err(s)=copy_nt_input(args[2],&mut payload[..n]){return s;}}
 let pid=current_process_id();let tid=crate::ntoskrnl::ke::thread::current_thread_id().unwrap_or(0);
 let _g=KernelEtwGuard::lock();
 unsafe {
  let table=core::ptr::addr_of!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();
  let Some(i)=(0..ETW_PROVIDER_CAPACITY).find(|&i|{let p=&*table.add(i);p.active&&p.handle==handle&&p.owner==pid}) else{return NT_STATUS_INVALID_HANDLE;};
  let p=&*table.add(i);let event_level=descriptor[4];let keyword=u64::from_le_bytes(descriptor[8..16].try_into().unwrap());let sessions=core::ptr::addr_of!(ETW_KERNEL_SESSIONS).cast::<KernelEtwSession>();let mut session_mask=0u16;
  for si in 0..16 {let sub=&p.subscriptions[si];let session=&*sessions.add(si);if !sub.enabled||!session.active{continue;}if sub.level!=0&&event_level!=0&&event_level>sub.level{continue;}if keyword!=0&&sub.any!=0&&sub.any&keyword==0{continue;}if keyword!=0&&sub.all!=0&&keyword&sub.all!=sub.all{continue;}session_mask|=1u16<<si;}
  if session_mask==0{return NT_STATUS_SUCCESS;}
  let events=core::ptr::addr_of_mut!(ETW_KERNEL_EVENTS).cast::<KernelEtwEvent>();
  if ETW_KERNEL_LEN==ETW_EVENT_CAPACITY {(*events.add(ETW_KERNEL_HEAD)).active=false;ETW_KERNEL_HEAD=(ETW_KERNEL_HEAD+1)%ETW_EVENT_CAPACITY;ETW_KERNEL_LEN-=1;}
  let slot=(ETW_KERNEL_HEAD+ETW_KERNEL_LEN)%ETW_EVENT_CAPACITY;
  let mut activity=[0u8;16];activity.copy_from_slice(&wire[16..32]);
  let mut related_activity=[0u8;16];related_activity.copy_from_slice(&wire[32..48]);
  *events.add(slot)=KernelEtwEvent{active:true,_pad:[0;5],session_mask,provider:handle,pid,tid,descriptor,activity,related_activity,length:n as u16,_pad2:[0;6],payload};
  ETW_KERNEL_LEN+=1;
 }
 NT_STATUS_SUCCESS
}
fn etw_read_event(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 if args[0]==0 || args[1]==0 || args[2]>ETW_PAYLOAD_CAPACITY as u64{return NT_STATUS_INVALID_PARAMETER;}
 let pid=current_process_id();
 let mut record=[0u8;80];let mut payload=[0u8;ETW_PAYLOAD_CAPACITY];let mut n=0usize;
 {
  let _g=KernelEtwGuard::lock();
  unsafe {
   if ETW_KERNEL_LEN==0{return 0x8000001Au64;}
   let sessions=core::ptr::addr_of!(ETW_KERNEL_SESSIONS).cast::<KernelEtwSession>();
   let session_slot=if args[4]==0{None}else{(0..16).find(|&i|{let s=&*sessions.add(i);s.active&&s.handle==args[4]&&s.owner==pid})};
   let providers=core::ptr::addr_of!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();
   let events=core::ptr::addr_of_mut!(ETW_KERNEL_EVENTS).cast::<KernelEtwEvent>();
   let mut selected=None;
   for off in 0..ETW_KERNEL_LEN {
    let idx=(ETW_KERNEL_HEAD+off)%ETW_EVENT_CAPACITY;let e=&*events.add(idx);
    let owner=(0..ETW_PROVIDER_CAPACITY).find_map(|i|{let p=&*providers.add(i);if p.active&&p.handle==e.provider{Some(p.owner)}else{None}});
    if owner==Some(pid)||session_slot.is_some_and(|si|e.session_mask&(1u16<<si)!=0){selected=Some(off);break;}
   }
   let Some(offset)=selected else{return 0x8000001Au64;};
   let idx=(ETW_KERNEL_HEAD+offset)%ETW_EVENT_CAPACITY;let e=*events.add(idx);
   record[0..8].copy_from_slice(&e.provider.to_le_bytes());record[8..16].copy_from_slice(&e.pid.to_le_bytes());record[16..24].copy_from_slice(&e.tid.to_le_bytes());record[24..26].copy_from_slice(&e.length.to_le_bytes());record[32..48].copy_from_slice(&e.descriptor);record[48..64].copy_from_slice(&e.activity);record[64..80].copy_from_slice(&e.related_activity);
   n=e.length as usize;payload[..n].copy_from_slice(&e.payload[..n]);
   for off in offset..(ETW_KERNEL_LEN-1){let dst=(ETW_KERNEL_HEAD+off)%ETW_EVENT_CAPACITY;let src=(ETW_KERNEL_HEAD+off+1)%ETW_EVENT_CAPACITY;*events.add(dst)=*events.add(src);}
   let last=(ETW_KERNEL_HEAD+ETW_KERNEL_LEN-1)%ETW_EVENT_CAPACITY;*events.add(last)=KernelEtwEvent::EMPTY;ETW_KERNEL_LEN-=1;if ETW_KERNEL_LEN==0{ETW_KERNEL_HEAD=0;}
  }
 }
 if let Err(s)=copy_nt_output(args[0],&record){return s;}
 if n>0 {if let Err(s)=copy_nt_output(args[1],&payload[..core::cmp::min(n,args[2] as usize)]){return s;}}
 if args[3]!=0 {if let Err(s)=copy_nt_output(args[3],&(n as u32).to_le_bytes()){return s;}}
 NT_STATUS_SUCCESS
}

fn etw_create_session(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 if args[1]==0{return NT_STATUS_INVALID_PARAMETER;}
 let pid=current_process_id();
 let token=crate::process::reference_primary_token(pid);
 if token.is_null(){return 0xC0000022;}
 let privileged=unsafe{crate::ntoskrnl::se::runtime::se_internal_TokenIsAdminAndUnrestricted(token)}!=0
  && unsafe{crate::ntoskrnl::se::runtime::se_internal_TokenHasPrivilege(token,11)}!=0;
 let _=unsafe{crate::ntoskrnl::se::runtime::se_internal_DereferenceToken(token)};
 if !privileged{return 0xC0000022;}
 let _g=KernelEtwGuard::lock();
 unsafe {
  let table=core::ptr::addr_of_mut!(ETW_KERNEL_SESSIONS).cast::<KernelEtwSession>();
  let Some(i)=(0..16).find(|&i|!(*table.add(i)).active)else{return 0xC000009A;};
  let handle=ETW_KERNEL_NEXT.fetch_add(1,Ordering::Relaxed).max(1);
  *table.add(i)=KernelEtwSession{active:true,handle,owner:pid,name_len:0,_pad:[0;7],name:[0;64]};
  if let Err(s)=copy_nt_output(args[1],&handle.to_le_bytes()){*table.add(i)=KernelEtwSession::EMPTY;return s;}
 }
 NT_STATUS_SUCCESS
}
fn etw_session_name_equal(left:&[u16],right:&[u16])->bool {
 if left.len()!=right.len(){return false;}
 left.iter().zip(right.iter()).all(|(a,b)|{
  let fold=|ch:u16|if ch>=65&&ch<=90{ch+32}else{ch};
  fold(*a)==fold(*b)
 })
}
fn etw_create_named_session(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 let name_ptr=args[0];let name_len=args[1] as usize;let out=args[2];
 if out==0||name_len==0||name_len>63||name_ptr==0{return NT_STATUS_INVALID_PARAMETER;}
 let mut name=[0u16;64];let mut bytes=[0u8;126];
 if let Err(status)=copy_nt_input(name_ptr,&mut bytes[..name_len*2]){return status;}
 for i in 0..name_len{name[i]=u16::from_le_bytes([bytes[i*2],bytes[i*2+1]]);if name[i]==0{return NT_STATUS_INVALID_PARAMETER;}}
 let pid=current_process_id();let token=crate::process::reference_primary_token(pid);
 if token.is_null(){return 0xC0000022;}
 let privileged=unsafe{crate::ntoskrnl::se::runtime::se_internal_TokenIsAdminAndUnrestricted(token)}!=0
  && unsafe{crate::ntoskrnl::se::runtime::se_internal_TokenHasPrivilege(token,11)}!=0;
 let _=unsafe{crate::ntoskrnl::se::runtime::se_internal_DereferenceToken(token)};
 if !privileged{return 0xC0000022;}
 let _g=KernelEtwGuard::lock();
 unsafe {
  let table=core::ptr::addr_of_mut!(ETW_KERNEL_SESSIONS).cast::<KernelEtwSession>();
  if (0..16).any(|i|{let s=&*table.add(i);s.active&&etw_session_name_equal(&s.name[..s.name_len as usize],&name[..name_len])}){return 0xC0000035;}
  let Some(i)=(0..16).find(|&i|!(*table.add(i)).active)else{return 0xC000009A;};
  let handle=ETW_KERNEL_NEXT.fetch_add(1,Ordering::Relaxed).max(1);
  *table.add(i)=KernelEtwSession{active:true,handle,owner:pid,name_len:name_len as u8,_pad:[0;7],name};
  if let Err(status)=copy_nt_output(out,&handle.to_le_bytes()){*table.add(i)=KernelEtwSession::EMPTY;return status;}
 }
 NT_STATUS_SUCCESS
}
fn etw_enable_session(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 if args[1]==0||args[2]>1||args[3]>u8::MAX as u64{return NT_STATUS_INVALID_PARAMETER;}
 let handle=args[0];let mut guid=[0u8;16];if let Err(s)=copy_nt_input(args[1],&mut guid){return s;}
 let pid=current_process_id();let enabled=args[2]!=0;let level=args[3] as u8;let any=args[4];let all=args[5];
 let _g=KernelEtwGuard::lock();
 unsafe {
  let sessions=core::ptr::addr_of_mut!(ETW_KERNEL_SESSIONS).cast::<KernelEtwSession>();
  let Some(si)=(0..16).find(|&i|{let s=&*sessions.add(i);s.active&&s.handle==handle&&s.owner==pid})else{return 0xC0000022;};
  let providers=core::ptr::addr_of_mut!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();let mut found=false;
  for i in 0..ETW_PROVIDER_CAPACITY {let p=&*providers.add(i);if p.active&&p.guid==guid{found=true;}}
  if !found{return 0xC0000225;}
  for i in 0..ETW_PROVIDER_CAPACITY {let p=&mut *providers.add(i);if p.active&&p.guid==guid{p.subscriptions[si]=if enabled{KernelEtwSubscription{enabled:true,level,_pad:[0;6],any,all}}else{KernelEtwSubscription::EMPTY};}}
  NT_STATUS_SUCCESS
 }
}
fn etw_stop_session(args:&[u64;NT_MAX_ARGUMENTS])->u64 {
 let handle=args[0];let pid=current_process_id();let _g=KernelEtwGuard::lock();
 unsafe {
  let sessions=core::ptr::addr_of_mut!(ETW_KERNEL_SESSIONS).cast::<KernelEtwSession>();
  let Some(si)=(0..16).find(|&i|{let s=&*sessions.add(i);s.active&&s.handle==handle&&s.owner==pid})else{return 0xC0000022;};
  let providers=core::ptr::addr_of_mut!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();
  for i in 0..ETW_PROVIDER_CAPACITY {let p=&mut *providers.add(i);if p.active{p.subscriptions[si]=KernelEtwSubscription::EMPTY;}}
  let events=core::ptr::addr_of_mut!(ETW_KERNEL_EVENTS).cast::<KernelEtwEvent>();let old_head=ETW_KERNEL_HEAD;let old_len=ETW_KERNEL_LEN;let mut kept=0usize;
  for off in 0..old_len {let src=(old_head+off)%ETW_EVENT_CAPACITY;let mut e=*events.add(src);e.session_mask&=!(1u16<<si);if e.session_mask==0{*events.add(src)=KernelEtwEvent::EMPTY;continue;}let dst=(old_head+kept)%ETW_EVENT_CAPACITY;if dst!=src{*events.add(dst)=e;*events.add(src)=KernelEtwEvent::EMPTY;}else{*events.add(dst)=e;}kept+=1;}
  ETW_KERNEL_LEN=kept;if kept==0{ETW_KERNEL_HEAD=0;}
  *sessions.add(si)=KernelEtwSession::EMPTY;
 }
 NT_STATUS_SUCCESS
}
pub(crate) fn etw_process_exit_rundown(pid:u64) {
 let _g=KernelEtwGuard::lock();
 unsafe {
  let providers=core::ptr::addr_of_mut!(ETW_KERNEL_PROVIDERS).cast::<KernelEtwProvider>();
  let sessions=core::ptr::addr_of_mut!(ETW_KERNEL_SESSIONS).cast::<KernelEtwSession>();
  let mut removed_session_mask=0u16;
  for i in 0..16 {
   let session=&mut *sessions.add(i);
   if session.active&&session.owner==pid {
    removed_session_mask|=1u16<<i;
    for j in 0..ETW_PROVIDER_CAPACITY {let provider=&mut *providers.add(j);if provider.active{provider.subscriptions[i]=KernelEtwSubscription::EMPTY;}}
    *session=KernelEtwSession::EMPTY;
   }
  }
  for i in 0..ETW_PROVIDER_CAPACITY {let provider=&mut *providers.add(i);if provider.active&&provider.owner==pid{*provider=KernelEtwProvider::EMPTY;}}
  let events=core::ptr::addr_of_mut!(ETW_KERNEL_EVENTS).cast::<KernelEtwEvent>();
  let old_head=ETW_KERNEL_HEAD;let old_len=ETW_KERNEL_LEN;let mut kept=0usize;
  for off in 0..old_len {
   let src=(old_head+off)%ETW_EVENT_CAPACITY;let mut event=*events.add(src);
   if !event.active||event.pid==pid { *events.add(src)=KernelEtwEvent::EMPTY; continue; }
   event.session_mask&=!removed_session_mask;
   if event.session_mask==0 { *events.add(src)=KernelEtwEvent::EMPTY; continue; }
   let dst=(old_head+kept)%ETW_EVENT_CAPACITY;
   if dst!=src {*events.add(dst)=event;*events.add(src)=KernelEtwEvent::EMPTY;}else{*events.add(dst)=event;}
   kept+=1;
  }
  ETW_KERNEL_LEN=kept;if kept==0{ETW_KERNEL_HEAD=0;}
 }
}

const NT_CURRENT_PROCESS: u64 = u64::MAX;
const NT_CURRENT_THREAD: u64 = u64::MAX - 1;
// Service IDs extracted from the exact AMD64 NTDLL reference image recorded in
// root/wine/dlls/ntdll/LOAD_ISSUES.md. These ordinals are build-specific.
pub const WIN_NT_CLOSE: u64 = 0x0f;
pub const WIN_NT_QUERY_INFORMATION_PROCESS: u64 = 0x19;
pub const WIN_NT_QUERY_INFORMATION_THREAD: u64 = 0x25;
pub const WIN_NT_QUERY_SYSTEM_INFORMATION: u64 = 0x36;
pub const WIN_NT_YIELD_EXECUTION: u64 = 0x46;
// Service IDs are discovered from the staged PE's own exported syscall stubs.
const SERVICE_UNAVAILABLE: u64 = u64::MAX;
static WINDOWS_SERVICE_TABLE_ENABLED: AtomicBool = AtomicBool::new(false);
static WINDOWS_SERVICE_NUMBERS: [AtomicU64; 30] =
    [const { AtomicU64::new(SERVICE_UNAVAILABLE) }; 30];
const SERVICE_NT_CLOSE: usize = 0;
const SERVICE_NT_YIELD_EXECUTION: usize = 1;
const SERVICE_NT_QUERY_INFORMATION_PROCESS: usize = 2;
const SERVICE_NT_QUERY_INFORMATION_THREAD: usize = 3;
const SERVICE_NT_QUERY_SYSTEM_INFORMATION: usize = 4;
const SERVICE_NT_OPEN_KEY: usize = 5;
const SERVICE_NT_QUERY_VALUE_KEY: usize = 6;
const SERVICE_NT_OPEN_FILE: usize = 7;
const SERVICE_NT_READ_FILE: usize = 8;
const SERVICE_NT_ALLOCATE_VIRTUAL_MEMORY: usize = 9;
const SERVICE_NT_FREE_VIRTUAL_MEMORY: usize = 10;
const SERVICE_NT_PROTECT_VIRTUAL_MEMORY: usize = 11;
const SERVICE_NT_QUERY_VIRTUAL_MEMORY: usize = 12;
const SERVICE_NT_READ_VIRTUAL_MEMORY: usize = 13;
const SERVICE_NT_WRITE_VIRTUAL_MEMORY: usize = 14;
const SERVICE_NT_CREATE_KEY: usize = 15;
const SERVICE_NT_SET_VALUE_KEY: usize = 16;
const SERVICE_NT_DELETE_VALUE_KEY: usize = 17;
const SERVICE_NT_DELETE_KEY: usize = 18;
const SERVICE_NT_FLUSH_KEY: usize = 19;
const SERVICE_NT_ENUMERATE_KEY: usize = 20;
const SERVICE_NT_ENUMERATE_VALUE_KEY: usize = 21;
const SERVICE_NT_QUERY_KEY: usize = 22;
const SERVICE_NT_OPEN_PROCESS: usize = 23;
const SERVICE_NT_GET_NEXT_PROCESS: usize = 24;
const SERVICE_NT_CREATE_EVENT: usize = 25;
const SERVICE_NT_SET_EVENT: usize = 26;
const SERVICE_NT_RESET_EVENT: usize = 27;
const SERVICE_NT_WAIT_FOR_SINGLE_OBJECT: usize = 28;
const SERVICE_NT_OPEN_EVENT: usize = 29;

fn image_u16(image: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(image.get(at..at.checked_add(2)?)?.try_into().ok()?))
}
fn image_u32(image: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(image.get(at..at.checked_add(4)?)?.try_into().ok()?))
}
fn image_rva_offset(image: &[u8], section_table: usize, section_count: usize,
                    headers_size: usize, rva: u32, required: usize) -> Option<usize> {
    let rva = rva as usize;
    if rva < headers_size && rva.checked_add(required)? <= image.len() { return Some(rva); }
    for index in 0..section_count {
        let at = section_table.checked_add(index.checked_mul(40)?)?;
        let virtual_size = image_u32(image, at.checked_add(8)?)? as usize;
        let virtual_address = image_u32(image, at.checked_add(12)?)? as usize;
        let raw_size = image_u32(image, at.checked_add(16)?)? as usize;
        let raw_offset = image_u32(image, at.checked_add(20)?)? as usize;
        let span = core::cmp::max(virtual_size, raw_size);
        if rva >= virtual_address && rva.checked_add(required)? <= virtual_address.checked_add(span)? {
            let delta = rva.checked_sub(virtual_address)?;
            if delta.checked_add(required)? > raw_size { return None; }
            let offset = raw_offset.checked_add(delta)?;
            if offset.checked_add(required)? <= image.len() { return Some(offset); }
            return None;
        }
    }
    None
}
fn extract_windows_service_numbers(image: &[u8]) -> Option<[u64; 30]> {
    if image.get(0..2)? != b"MZ" { return None; }
    let pe = image_u32(image, 0x3c)? as usize;
    if image.get(pe..pe.checked_add(4)?)? != b"PE\0\0" { return None; }
    let coff = pe.checked_add(4)?;
    let section_count = image_u16(image, coff.checked_add(2)?)? as usize;
    let optional_size = image_u16(image, coff.checked_add(16)?)? as usize;
    let optional = pe.checked_add(24)?;
    if image_u16(image, coff)? != 0x8664 || image_u16(image, optional)? != 0x20b
        || section_count == 0 || section_count > 96 { return None; }
    let headers_size = image_u32(image, optional.checked_add(60)?)? as usize;
    let section_table = optional.checked_add(optional_size)?;
    if section_table.checked_add(section_count.checked_mul(40)?)? > image.len() { return None; }
    let export_rva = image_u32(image, optional.checked_add(112)?)?;
    let export_size = image_u32(image, optional.checked_add(116)?)?;
    if export_rva == 0 || export_size < 40 { return None; }
    let export = image_rva_offset(image, section_table, section_count, headers_size, export_rva, 40)?;
    let function_count = image_u32(image, export.checked_add(20)?)? as usize;
    let name_count = image_u32(image, export.checked_add(24)?)? as usize;
    let functions_rva = image_u32(image, export.checked_add(28)?)?;
    let names_rva = image_u32(image, export.checked_add(32)?)?;
    let ordinals_rva = image_u32(image, export.checked_add(36)?)?;
    if function_count == 0 || function_count > 100_000 || name_count > 100_000 { return None; }
    let functions = image_rva_offset(image, section_table, section_count, headers_size, functions_rva, function_count.checked_mul(4)?)?;
    let names = image_rva_offset(image, section_table, section_count, headers_size, names_rva, name_count.checked_mul(4)?)?;
    let ordinals = image_rva_offset(image, section_table, section_count, headers_size, ordinals_rva, name_count.checked_mul(2)?)?;
    // Rust NTDLL is the sole native user/kernel boundary. Require every
    // syscall export used by the current kernel, including registry mutations.
    // A partial table is a boot error, not a reason to switch ABI paths.
    let required_names: [&[u8]; 30] = [
        b"NtClose", b"NtYieldExecution", b"NtQueryInformationProcess",
        b"NtQueryInformationThread", b"NtQuerySystemInformation", b"NtOpenKey",
        b"NtQueryValueKey", b"NtOpenFile", b"NtReadFile",
        b"NtAllocateVirtualMemory", b"NtFreeVirtualMemory", b"NtProtectVirtualMemory",
        b"NtQueryVirtualMemory", b"NtReadVirtualMemory", b"NtWriteVirtualMemory",
        b"NtCreateKey", b"NtSetValueKey", b"NtDeleteValueKey", b"NtDeleteKey", b"NtFlushKey",
        b"NtEnumerateKey", b"NtEnumerateValueKey", b"NtQueryKey",
        b"NtOpenProcess", b"NtGetNextProcess",
        b"NtCreateEvent", b"NtSetEvent", b"NtResetEvent", b"NtWaitForSingleObject", b"NtOpenEvent",
    ];
    let mut discovered = [SERVICE_UNAVAILABLE; 30];
    for index in 0..name_count {
        let name_rva = image_u32(image, names.checked_add(index.checked_mul(4)?)?)?;
        let name_at = image_rva_offset(image, section_table, section_count, headers_size, name_rva, 1)?;
        let name_end_rel = image.get(name_at..)?.iter().position(|byte| *byte == 0)?;
        let name = &image[name_at..name_at.checked_add(name_end_rel)?];
        let Some(slot) = required_names.iter().position(|required| *required == name) else { continue; };
        let ordinal = image_u16(image, ordinals.checked_add(index.checked_mul(2)?)?)? as usize;
        if ordinal >= function_count { return None; }
        let function_rva = image_u32(image, functions.checked_add(ordinal.checked_mul(4)?)?)?;
        if function_rva >= export_rva && function_rva < export_rva.saturating_add(export_size) { continue; }
        let Some(stub) = image_rva_offset(image, section_table, section_count, headers_size, function_rva, 16) else { continue; };
        let register_move = image.get(stub..stub.checked_add(3)?)?;
        let has_r10_rcx = register_move == [0x4c, 0x8b, 0xd1]
            || register_move == [0x49, 0x89, 0xca];
        if !has_r10_rcx || image.get(stub.checked_add(3)?)? != &0xb8 { continue; }
        let service = image_u32(image, stub.checked_add(4)?)?;
        let window_end = core::cmp::min(image.len(), stub.saturating_add(32));
        if !image.get(stub.checked_add(8)?..window_end)?.windows(2).any(|op| op == [0x0f, 0x05]) { continue; }
        discovered[slot] = service as u64;
    }
    if discovered[..required_names.len()].iter().any(|service| *service == SERVICE_UNAVAILABLE) { return None; }
    Some(discovered)
}
pub fn configure_windows_service_table_from_image(image: &[u8]) -> bool {
    let Some(discovered) = extract_windows_service_numbers(image) else { return false; };
    for (slot, service) in discovered.iter().enumerate() {
        WINDOWS_SERVICE_NUMBERS[slot].store(*service, Ordering::Release);
    }
    WINDOWS_SERVICE_TABLE_ENABLED.store(true, Ordering::Release);
    true
}
pub fn windows_service_number(name: &[u8]) -> Option<u32> {
    let slot = match name {
        b"NtClose" => SERVICE_NT_CLOSE,
        b"NtYieldExecution" => SERVICE_NT_YIELD_EXECUTION,
        b"NtQueryInformationProcess" => SERVICE_NT_QUERY_INFORMATION_PROCESS,
        b"NtQueryInformationThread" => SERVICE_NT_QUERY_INFORMATION_THREAD,
        b"NtQuerySystemInformation" => SERVICE_NT_QUERY_SYSTEM_INFORMATION,
        b"NtOpenKey" => SERVICE_NT_OPEN_KEY,
        b"NtQueryValueKey" => SERVICE_NT_QUERY_VALUE_KEY,
        b"NtOpenFile" => SERVICE_NT_OPEN_FILE,
        b"NtReadFile" => SERVICE_NT_READ_FILE,
        b"NtAllocateVirtualMemory" => SERVICE_NT_ALLOCATE_VIRTUAL_MEMORY,
        b"NtFreeVirtualMemory" => SERVICE_NT_FREE_VIRTUAL_MEMORY,
        b"NtProtectVirtualMemory" => SERVICE_NT_PROTECT_VIRTUAL_MEMORY,
        b"NtQueryVirtualMemory" => SERVICE_NT_QUERY_VIRTUAL_MEMORY,
        b"NtReadVirtualMemory" => SERVICE_NT_READ_VIRTUAL_MEMORY,
        b"NtWriteVirtualMemory" => SERVICE_NT_WRITE_VIRTUAL_MEMORY,
        b"NtCreateKey" => SERVICE_NT_CREATE_KEY,
        b"NtSetValueKey" => SERVICE_NT_SET_VALUE_KEY,
        b"NtDeleteValueKey" => SERVICE_NT_DELETE_VALUE_KEY,
        b"NtDeleteKey" => SERVICE_NT_DELETE_KEY,
        b"NtFlushKey" => SERVICE_NT_FLUSH_KEY,
        b"NtEnumerateKey" => SERVICE_NT_ENUMERATE_KEY,
        b"NtEnumerateValueKey" => SERVICE_NT_ENUMERATE_VALUE_KEY,
        b"NtQueryKey" => SERVICE_NT_QUERY_KEY,
        b"NtOpenProcess" => SERVICE_NT_OPEN_PROCESS,
        b"NtGetNextProcess" => SERVICE_NT_GET_NEXT_PROCESS,
        b"NtCreateEvent" => SERVICE_NT_CREATE_EVENT,
        b"NtSetEvent" => SERVICE_NT_SET_EVENT,
        b"NtResetEvent" => SERVICE_NT_RESET_EVENT,
        b"NtWaitForSingleObject" => SERVICE_NT_WAIT_FOR_SINGLE_OBJECT,
        b"NtOpenEvent" => SERVICE_NT_OPEN_EVENT,
        _ => return None,
    };
    let value = WINDOWS_SERVICE_NUMBERS[slot].load(Ordering::Acquire);
    (value != SERVICE_UNAVAILABLE && value <= u32::MAX as u64).then_some(value as u32)
}
const PROCESS_BASIC_INFORMATION_CLASS: u64 = 0;
const THREAD_BASIC_INFORMATION_CLASS: u64 = 0;
const SYSTEM_BASIC_INFORMATION_CLASS: u64 = 0;



#[repr(C, align(16))]
struct WindowsSyscallCpuLocal {
    user_rsp: u64,
    kernel_rsp: u64,
    scratch_rax: u64,
    scratch_rdx: u64,
    stack_page: u64,
    user_gs_base: u64,
}
impl WindowsSyscallCpuLocal {
    const fn new() -> Self {
        Self { user_rsp: 0, kernel_rsp: 0, scratch_rax: 0, scratch_rdx: 0, stack_page: 0, user_gs_base: 0 }
    }
}
static mut WINDOWS_SYSCALL_CPUS: [WindowsSyscallCpuLocal; crate::hal::MAX_CPUS] =
    [const { WindowsSyscallCpuLocal::new() }; crate::hal::MAX_CPUS];

global_asm!(r#"
    .section .text
    .global cognacos_windows_syscall_entry
    .type cognacos_windows_syscall_entry, @function
cognacos_windows_syscall_entry:
    # SYSCALL does not switch RSP. FMASK has already disabled interrupts.
    swapgs
    # After SWAPGS, IA32_KERNEL_GS_BASE holds the caller's TEB base. Save it
    # per CPU and restore it explicitly before returning, even if nested kernel
    # code touched GS MSRs while servicing an NT call.
    push rax
    push rdx
    push rcx
    mov ecx, 0xC0000102
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov qword ptr gs:[40], rax
    pop rcx
    pop rdx
    pop rax
    # Temporary marker s confirms that LSTAR and SWAPGS were reached.
    push rax
    push rdx
    mov dx, 0x3f8
    mov al, 's'
    out dx, al
    pop rdx
    pop rax
    # Check the active GS base high word (H=kernel address, L=user address).
    push rax
    push rdx
    push rcx
    mov ecx, 0xC0000101
    rdmsr
    cmp edx, 0xffffffff
    jne .Lwindows_gs_low
    mov al, 'H'
    jmp .Lwindows_gs_marker
.Lwindows_gs_low:
    mov al, 'L'
.Lwindows_gs_marker:
    mov dx, 0x3f8
    out dx, al
    pop rcx
    pop rdx
    pop rax
    cld
    # GS:[0] saves user RSP; GS:[8] is this CPU's private kernel stack top.
    mov qword ptr gs:[0], rsp
    # Save volatile values in the per-CPU local area to trace GS access safely.
    mov qword ptr gs:[16], rax
    mov qword ptr gs:[24], rdx
    mov dx, 0x3f8
    mov al, 'g'
    out dx, al
    mov rax, qword ptr gs:[16]
    mov rdx, qword ptr gs:[24]
    # Check that the saved kernel stack top is a high-half canonical pointer.
    mov qword ptr gs:[16], rax
    mov qword ptr gs:[24], rdx
    mov rax, qword ptr gs:[8]
    mov rdx, rax
    shr rdx, 48
    cmp dx, 0xffff
    jne .Lwindows_stack_bad
    test eax, 0xfff
    jne .Lwindows_stack_bad
    mov al, 'T'
    jmp .Lwindows_stack_marker
.Lwindows_stack_bad:
    mov al, 'X'
.Lwindows_stack_marker:
    mov dx, 0x3f8
    out dx, al
    mov rax, qword ptr gs:[16]
    mov rdx, qword ptr gs:[24]
    mov rsp, qword ptr gs:[8]
    # Temporary marker 'k' proves the per-CPU GS stack switch succeeded.
    push rax
    push rdx
    mov dx, 0x3f8
    mov al, 'k'
    out dx, al
    pop rdx
    pop rax
    # Synthesize the privilege-return frame consumed by IRETQ.
    # IRETQ frame order from top: RIP, CS, RFLAGS, RSP, SS.
    push 0x1b
    push qword ptr gs:[0]
    push r11
    push 0x23
    push rcx
    # Match the 15-register frame consumed by cognacos_nt_syscall_dispatch.
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
    # Temporary serial markers isolate SYSCALL entry, dispatch, and return.
    mov dx, 0x3f8
    mov al, 'S'
    out dx, al
    mov rbx, rsp
    # Save the interrupted user XSTATE before entering Rust or any scheduler path.
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
    mov rdi, rsp
    and rsp, -16
    call cognacos_windows_syscall_dispatch
    mov rsp, rbx
    mov qword ptr [rsp + 112], rax
    # Dispatch may have switched threads; restore the current thread's XSTATE
    # immediately before restoring its registers and returning to ring 3.
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lwindows_syscall_no_xstate_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lwindows_syscall_no_xstate_restore:
    mov dx, 0x3f8
    mov al, 'D'
    out dx, al
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
    # Preserve the exact user TEB GS base across the syscall boundary.
    push rax
    push rdx
    push rcx
    mov rax, qword ptr gs:[40]
    mov rdx, rax
    shr rdx, 32
    mov ecx, 0xC0000102
    wrmsr
    pop rcx
    pop rdx
    pop rax
    swapgs
    iretq
"#);
unsafe extern "C" { fn cognacos_windows_syscall_entry(); }

/// Install the architectural Windows x64 SYSCALL entry on the current CPU.
/// Uses a private stack per CPU, preserves the user TEB's GS base via SWAPGS,
/// and returns through IRETQ rather than trusting SYSRET on unvalidated state.
pub unsafe fn initialize_windows_syscall_abi(cpu: usize) -> Result<(), ()> {
    if cpu >= crate::hal::MAX_CPUS { return Err(()); }
    let local = unsafe { core::ptr::addr_of_mut!(WINDOWS_SYSCALL_CPUS[cpu]) };
    unsafe {
        if (*local).stack_page != 0 {
            let stack_base = crate::ntoskrnl::mm::physical::direct_map_address((*local).stack_page).ok_or(())?;
            (*local).kernel_rsp = stack_base + 4096;
        }
    }
    unsafe {
        write_msr(0xC000_0101, 0); // IA32_GS_BASE: kernel has no active GS segment base
        write_msr(0xC000_0102, local as u64); // IA32_KERNEL_GS_BASE: swapgs target
        let efer = read_msr(0xC000_0080);
        write_msr(0xC000_0080, efer | 1); // IA32_EFER.SCE
        write_msr(0xC000_0081, (0x08u64) << 32); // IA32_STAR: ring-0 CS/SS selectors
        write_msr(0xC000_0082, cognacos_windows_syscall_entry as usize as u64); // IA32_LSTAR
        write_msr(0xC000_0084, 0x0000_0000_0004_7700); // IF, TF, DF, AC, NT, IOPL masked
        // Fail during boot instead of letting a CPL3 SYSCALL fault silently.
        if read_msr(0xC000_0080) & 1 == 0 {
            crate::serial_write(b"FATAL: IA32_EFER.SCE did not latch.\r\n");
            return Err(());
        }
        if read_msr(0xC000_0081) != 0x0000_0008_0000_0000 {
            crate::serial_write(b"FATAL: IA32_STAR selector setup did not latch.\r\n");
            return Err(());
        }
        if read_msr(0xC000_0082) != cognacos_windows_syscall_entry as usize as u64 {
            crate::serial_write(b"FATAL: IA32_LSTAR entry address did not latch.\r\n");
            return Err(());
        }
        if read_msr(0xC000_0084) != 0x0000_0000_0004_7700 {
            crate::serial_write(b"FATAL: IA32_FMASK setup did not latch.\r\n");
            return Err(());
        }
        if read_msr(0xC000_0101) != 0 {
            crate::serial_write(b"FATAL: IA32_GS_BASE was not reset.\r\n");
            return Err(());
        }
        if read_msr(0xC000_0102) != local as u64 {
            crate::serial_write(b"FATAL: IA32_KERNEL_GS_BASE did not latch.\r\n");
            return Err(());
        }
    }
    Ok(())
}
/// Allocate a per-CPU syscall stack after the physical allocator is initialized.
/// The early identity map is shared as supervisor-only memory by every process CR3.
pub unsafe fn prepare_windows_syscall_stack(cpu: usize) -> Result<(), ()> {
    if cpu >= crate::hal::MAX_CPUS { return Err(()); }
    let local = unsafe { core::ptr::addr_of_mut!(WINDOWS_SYSCALL_CPUS[cpu]) };
    unsafe {
        if (*local).stack_page == 0 {
            let page = crate::ntoskrnl::mm::physical::allocate_data_page().map_err(|_| ())?;
            if page == 0 { return Err(()); }
            (*local).stack_page = page;
        }
        let stack_base = crate::ntoskrnl::mm::physical::direct_map_address((*local).stack_page).ok_or(())?;
        (*local).kernel_rsp = stack_base + 4096;
        if (*local).kernel_rsp == 0 { return Err(()); }
    }
    Ok(())
}

unsafe fn read_msr(index: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe { core::arch::asm!("rdmsr", in("ecx") index, out("eax") low, out("edx") high, options(nostack, preserves_flags)); }
    ((high as u64) << 32) | low as u64
}
unsafe fn write_msr(index: u32, value: u64) {
    unsafe { core::arch::asm!("wrmsr", in("ecx") index, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nostack, preserves_flags)); }
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

fn nt_open_process(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] == 0 || args[2] == 0 || args[3] == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let mut cid = [0u8; 16];
    if let Err(status) = copy_nt_input(args[3], &mut cid) { return status; }
    let pid = u64::from_le_bytes(cid[..8].try_into().unwrap());
    if pid == 0 || pid == u64::MAX { return 0xC000_000Bu64; }
    let mut oa = [0u8; 48];
    if let Err(status) = copy_nt_input(args[2], &mut oa) { return status; }
    if u32::from_le_bytes(oa[..4].try_into().unwrap()) < 48
        || u64::from_le_bytes(oa[16..24].try_into().unwrap()) != 0
        || u64::from_le_bytes(oa[32..40].try_into().unwrap()) != 0
        || u64::from_le_bytes(oa[40..48].try_into().unwrap()) != 0 {
        return 0xC000_0030u64;
    }
    const VALID_PROCESS_ACCESS: u32 = 0x001F_FFFF | 0x0010_0000 | 0x0002_0000 | 0x0001_0000;
    let mut desired = args[1] as u32;
    if desired & !VALID_PROCESS_ACCESS != 0 { return NT_STATUS_INVALID_PARAMETER; }
    if desired & 0x0400 != 0 { desired |= 0x1000; }
    let Some(process) = crate::process::eprocess_for_pid(pid) else { return 0xC000_000Bu64; };
    let mut handle = core::ptr::null_mut();
    let status = unsafe { crate::ntoskrnl::ob::runtime::ObOpenObjectByPointer(
        process.cast(), u32::from_le_bytes(oa[12..16].try_into().unwrap()),
        core::ptr::null_mut(), desired, core::ptr::null_mut(), 1, &mut handle) };
    let _ = crate::process::dereference_eprocess(process.cast());
    if status != 0 { return status as u32 as u64; }
    if let Err(status) = copy_nt_output(args[0], &(handle as usize as u64).to_le_bytes()) {
        let _ = unsafe { crate::ntoskrnl::ob::runtime::ObCloseHandle(handle, 1) };
        return status;
    }
    NT_STATUS_SUCCESS
}

fn nt_get_next_process(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[4] == 0 || args[3] != 0 { return NT_STATUS_INVALID_PARAMETER; }
    let mut after_pid = 0u64;
    if args[0] != 0 {
        let mut previous = core::ptr::null_mut();
        let status = unsafe { crate::ntoskrnl::ob::runtime::ObReferenceObjectByHandle(
            args[0] as *mut core::ffi::c_void, 0, core::ptr::null_mut(), 1,
            &mut previous, core::ptr::null_mut()) };
        if status != 0 { return status as u32 as u64; }
        let snapshot = crate::process::snapshot_referenced(previous);
        let _ = crate::process::dereference_eprocess(previous);
        let Some(snapshot) = snapshot else { return NT_STATUS_INVALID_HANDLE; };
        after_pid = snapshot.process_id;
    }
    let Some(process) = crate::process::next_live_process(after_pid) else {
        let _ = copy_nt_output(args[4], &0u64.to_le_bytes());
        return 0x8000_001Au64;
    };
    let mut handle = core::ptr::null_mut();
    let mut desired = args[1] as u32;
    if desired & 0x0400 != 0 { desired |= 0x1000; }
    let status = unsafe { crate::ntoskrnl::ob::runtime::ObOpenObjectByPointer(
        process.cast(), args[2] as u32, core::ptr::null_mut(), desired,
        core::ptr::null_mut(), 1, &mut handle) };
    let _ = crate::process::dereference_eprocess(process.cast());
    if status != 0 { return status as u32 as u64; }
    if let Err(status) = copy_nt_output(args[4], &(handle as usize as u64).to_le_bytes()) {
        let _ = unsafe { crate::ntoskrnl::ob::runtime::ObCloseHandle(handle, 1) };
        return status;
    }
    NT_STATUS_SUCCESS
}

fn nt_query_process(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[1] != PROCESS_BASIC_INFORMATION_CLASS { return NT_STATUS_INVALID_INFO_CLASS; }
    const SIZE: usize = 48;
    if args[3] < SIZE as u64 { let _ = write_return_length(args[4], SIZE as u32); return NT_STATUS_INFO_LENGTH_MISMATCH; }
    let (pid, exit_status, referenced) = if args[0] == NT_CURRENT_PROCESS {
        (CURRENT_PID.load(Ordering::Acquire), 0u64, core::ptr::null_mut())
    } else {
        let mut object = core::ptr::null_mut();
        let status = unsafe { crate::ntoskrnl::ob::runtime::ObReferenceObjectByHandle(
            args[0] as *mut core::ffi::c_void, 0x1000, core::ptr::null_mut(), 1,
            &mut object, core::ptr::null_mut()) };
        if status != 0 { return status as u32 as u64; }
        let Some(snapshot) = crate::process::snapshot_referenced(object) else {
            let _ = crate::process::dereference_eprocess(object);
            return NT_STATUS_INVALID_HANDLE;
        };
        (snapshot.process_id, snapshot.exit_code, object)
    };
    let mut info = [0u8; SIZE];
    info[0..4].copy_from_slice(&(exit_status as u32).to_le_bytes());
    info[8..16].copy_from_slice(&(crate::process::PEB_ADDRESS).to_le_bytes());
    info[16..24].copy_from_slice(&1u64.to_le_bytes());
    info[24..28].copy_from_slice(&8i32.to_le_bytes());
    info[32..40].copy_from_slice(&pid.to_le_bytes());
    info[40..48].copy_from_slice(&0u64.to_le_bytes());
    if !referenced.is_null() { let _ = crate::process::dereference_eprocess(referenced); }
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
    let thread_id = crate::ntoskrnl::ke::thread::current_thread_id().unwrap_or(0);
    let affinity = crate::ntoskrnl::ke::thread::thread_affinity(thread_id).unwrap_or_else(|| {
        crate::hal::current_cpu_local()
            .map(|cpu| 1u64.checked_shl(cpu.logical_id.load(Ordering::Acquire) as u32).unwrap_or(0))
            .unwrap_or(0)
    });
    info[32..40].copy_from_slice(&affinity.to_le_bytes()); // scheduler-owned thread affinity
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
    // Report the processors that the HAL has actually brought online. The
    // scheduler and affinity APIs use this same logical-CPU numbering; never
    // advertise CPU 0 alone when APs are online, and never count offline slots.
    let mut active_affinity = 0u64;
    for cpu in 0..crate::hal::MAX_CPUS.min(64) {
        if crate::hal::cpu_state(cpu).ok() == Some(crate::hal::CPU_ONLINE) {
            active_affinity |= 1u64 << cpu;
        }
    }
    info[48..56].copy_from_slice(&active_affinity.to_le_bytes());
    info[56] = active_affinity.count_ones().min(u8::MAX as u32) as u8;
    if let Err(status) = write_return_length(args[3], SIZE as u32) { return status; }
    if let Err(status) = copy_nt_output(args[1], &info) { return status; }
    NT_STATUS_SUCCESS
}

const NT_STATUS_OBJECT_NAME_NOT_FOUND: u64 = 0xC000_0034;
const NT_REGISTRY_NAME_UNITS: usize = 96;
const NT_REGISTRY_QUERY_MAX: usize = 1024;

fn copy_nt_input(address: u64, bytes: &mut [u8]) -> Result<(), u64> {
    if bytes.is_empty() { return Ok(()); }
    let pointer = unsafe { *ACTIVE_SPACE.0.get() };
    if pointer.is_null() { return Err(NT_STATUS_ACCESS_VIOLATION); }
    unsafe { (&*pointer).copy_from_user(address, bytes) }.map_err(|_| NT_STATUS_ACCESS_VIOLATION)
}

fn nt_open_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    // x64 OBJECT_ATTRIBUTES is 48 bytes; UNICODE_STRING is 16 bytes.
    let mut oa = [0u8; 48];
    if args[2] == 0 || args[0] == 0 { return NT_STATUS_INVALID_PARAMETER; }
    if let Err(status) = copy_nt_input(args[2], &mut oa) { return status; }
    let oa_len = u32::from_le_bytes(oa[0..4].try_into().unwrap());
    if oa_len < 48 { return NT_STATUS_INVALID_PARAMETER; }
    let root = u64::from_le_bytes(oa[8..16].try_into().unwrap());
    let us_address = u64::from_le_bytes(oa[16..24].try_into().unwrap());
    if us_address == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let mut us = [0u8; 16];
    if let Err(status) = copy_nt_input(us_address, &mut us) { return status; }
    let length = u16::from_le_bytes([us[0], us[1]]) as usize;
    let maximum = u16::from_le_bytes([us[2], us[3]]) as usize;
    let buffer_address = u64::from_le_bytes(us[8..16].try_into().unwrap());
    if length % 2 != 0 || length > maximum || length / 2 > NT_REGISTRY_NAME_UNITS
        || (length != 0 && buffer_address == 0) { return NT_STATUS_INVALID_PARAMETER; }
    let mut raw = [0u8; NT_REGISTRY_NAME_UNITS * 2];
    if length != 0 {
        if let Err(status) = copy_nt_input(buffer_address, &mut raw[..length]) { return status; }
    }
    let mut path = [0u16; NT_REGISTRY_NAME_UNITS];
    for i in 0..length / 2 { path[i] = u16::from_le_bytes([raw[i*2], raw[i*2+1]]); }
    let name = crate::ntoskrnl::cm::runtime::UnicodeString {
        length: length as u16, maximum_length: maximum as u16, buffer: path.as_ptr()
    };
    let attributes = crate::ntoskrnl::cm::runtime::ObjectAttributes {
        length: oa_len, root_directory: root as *mut core::ffi::c_void,
        object_name: &name, attributes: u32::from_le_bytes(oa[24..28].try_into().unwrap()),
        security_descriptor: u64::from_le_bytes(oa[32..40].try_into().unwrap()) as *mut core::ffi::c_void,
        security_qos: u64::from_le_bytes(oa[40..48].try_into().unwrap()) as *mut core::ffi::c_void,
    };
    let mut handle = core::ptr::null_mut();
    let status = unsafe { crate::ntoskrnl::cm::runtime::ZwOpenKey(&mut handle, args[1] as u32, &attributes) };
    if status != 0 { return status as u32 as u64; }
    let bytes = (handle as usize as u64).to_le_bytes();
    if let Err(status) = copy_nt_output(args[0], &bytes) {
        let _ = unsafe { crate::ntoskrnl::ob::runtime::ObCloseHandle(handle, 1) };
        return status;
    }
    NT_STATUS_SUCCESS
}


fn nt_open_file(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] == 0 || args[2] == 0 || args[3] == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let mut oa = [0u8; 48];
    if let Err(status) = copy_nt_input(args[2], &mut oa) { return status; }
    let oa_len = u32::from_le_bytes(oa[0..4].try_into().unwrap());
    if oa_len < 48 || u64::from_le_bytes(oa[8..16].try_into().unwrap()) != 0 { return NT_STATUS_INVALID_PARAMETER; }
    let us_address = u64::from_le_bytes(oa[16..24].try_into().unwrap());
    if us_address == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let mut us = [0u8; 16];
    if let Err(status) = copy_nt_input(us_address, &mut us) { return status; }
    let length = u16::from_le_bytes([us[0], us[1]]) as usize;
    let maximum = u16::from_le_bytes([us[2], us[3]]) as usize;
    let buffer = u64::from_le_bytes(us[8..16].try_into().unwrap());
    if length == 0 || length & 1 != 0 || length > maximum || length / 2 > 260 || buffer == 0 {
        return NT_STATUS_INVALID_PARAMETER;
    }
    let mut raw = [0u8; 520];
    if let Err(status) = copy_nt_input(buffer, &mut raw[..length]) { return status; }
    let mut path = [0u8; 260];
    for i in 0..length / 2 {
        let unit = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
        if unit > 0x7f { return NT_STATUS_OBJECT_NAME_NOT_FOUND; }
        path[i] = unit as u8;
    }
    let handle = match crate::file_services::open_for_process(current_process_id(), &path[..length / 2]) {
        Ok(handle) => handle, Err(status) => return status as u32 as u64,
    };
    let mut iosb = [0u8; 16];
    iosb[0..4].copy_from_slice(&0i32.to_le_bytes());
    if let Err(status) = copy_nt_output(args[3], &iosb) {
        let _ = crate::file_services::close_for_process(current_process_id(), handle); return status;
    }
    if let Err(status) = copy_nt_output(args[0], &(handle as u64).to_le_bytes()) {
        let _ = crate::file_services::close_for_process(current_process_id(), handle); return status;
    }
    NT_STATUS_SUCCESS
}

fn nt_read_file(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] == 0 || args[5] == 0 || args[6] > 4096 || args[4] == 0 {
        return NT_STATUS_INVALID_PARAMETER;
    }
    if args[1] != 0 || args[2] != 0 || args[3] != 0 { return NT_STATUS_INVALID_PARAMETER; }
    let length = args[6] as usize;
    let mut buffer = [0u8; 4096];
    let handle = args[0] as usize;
    let owner = current_process_id();
    let current_offset = if args[7] == 0 {
        match crate::file_services::current_offset_for_process(owner, handle) {
            Ok(offset) => Some(offset), Err(status) => return status as u32 as u64,
        }
    } else {
        None
    };
    let offset = if let Some(offset) = current_offset {
        offset
    } else {
        let mut offset_bytes = [0u8; 8];
        if let Err(status) = copy_nt_input(args[7], &mut offset_bytes) { return status; }
        let offset = i64::from_le_bytes(offset_bytes);
        if offset < 0 { return NT_STATUS_INVALID_PARAMETER; }
        offset as usize
    };
    let count = match crate::file_services::read_at_for_process(owner, handle, offset, &mut buffer[..length]) {
        Ok(n) => n, Err(status) => return status as u32 as u64,
    };
    if let Err(status) = copy_nt_output(args[5], &buffer[..count]) { return status; }
    if let Some(expected) = current_offset {
        if let Err(status) = crate::file_services::advance_current_for_process(owner, handle, expected, count) {
            return status as u32 as u64;
        }
    }
    let mut iosb = [0u8; 16];
    iosb[0..4].copy_from_slice(&(if count == 0 { 0xC000_0011u32 as i32 } else { 0 }).to_le_bytes());
    iosb[8..16].copy_from_slice(&(count as u64).to_le_bytes());
    if let Err(status) = copy_nt_output(args[4], &iosb) { return status; }
    if count == 0 { 0xC000_0011u32 as u64 } else { NT_STATUS_SUCCESS }
}


fn nt_create_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] == 0 || args[2] == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let mut oa = [0u8; 48];
    if let Err(status) = copy_nt_input(args[2], &mut oa) { return status; }
    let oa_len = u32::from_le_bytes(oa[0..4].try_into().unwrap());
    if oa_len < 48 { return NT_STATUS_INVALID_PARAMETER; }
    let root = u64::from_le_bytes(oa[8..16].try_into().unwrap());
    let us_address = u64::from_le_bytes(oa[16..24].try_into().unwrap());
    if us_address == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let mut us = [0u8; 16];
    if let Err(status) = copy_nt_input(us_address, &mut us) { return status; }
    let length = u16::from_le_bytes([us[0], us[1]]) as usize;
    let maximum = u16::from_le_bytes([us[2], us[3]]) as usize;
    let buffer_address = u64::from_le_bytes(us[8..16].try_into().unwrap());
    if length % 2 != 0 || length > maximum || length / 2 > NT_REGISTRY_NAME_UNITS
        || (length != 0 && buffer_address == 0) { return NT_STATUS_INVALID_PARAMETER; }
    let mut raw = [0u8; NT_REGISTRY_NAME_UNITS * 2];
    if length != 0 { if let Err(status) = copy_nt_input(buffer_address, &mut raw[..length]) { return status; } }
    let mut path = [0u16; NT_REGISTRY_NAME_UNITS];
    for i in 0..length / 2 { path[i] = u16::from_le_bytes([raw[i*2], raw[i*2+1]]); }
    let name = crate::ntoskrnl::cm::runtime::UnicodeString { length:length as u16, maximum_length:maximum as u16, buffer:path.as_ptr() };
    let attributes = crate::ntoskrnl::cm::runtime::ObjectAttributes {
        length:oa_len, root_directory:root as *mut core::ffi::c_void, object_name:&name,
        attributes:u32::from_le_bytes(oa[24..28].try_into().unwrap()),
        security_descriptor:u64::from_le_bytes(oa[32..40].try_into().unwrap()) as *mut core::ffi::c_void,
        security_qos:u64::from_le_bytes(oa[40..48].try_into().unwrap()) as *mut core::ffi::c_void,
    };
    let mut handle = core::ptr::null_mut();
    let mut disposition = 0u32;
    let status = unsafe { crate::ntoskrnl::cm::runtime::ZwCreateKey(&mut handle,args[1] as u32,&attributes,args[3] as u32,core::ptr::null(),args[5] as u32,&mut disposition) };
    if status != 0 { return status as u32 as u64; }
    if let Err(status) = copy_nt_output(args[0], &(handle as usize as u64).to_le_bytes()) {
        let _ = unsafe { crate::ntoskrnl::ob::runtime::ObCloseHandle(handle,1) }; return status;
    }
    if args[6] != 0 {
        if let Err(status) = write_return_length(args[6],disposition) {
            let _ = unsafe { crate::ntoskrnl::ob::runtime::ObCloseHandle(handle,1) }; return status;
        }
    }
    NT_STATUS_SUCCESS
}
fn nt_set_value_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0]==0 || args[1]==0 || args[5]>256 || (args[5]!=0 && args[4]==0) { return NT_STATUS_INVALID_PARAMETER; }
    let mut us=[0u8;16]; if let Err(status)=copy_nt_input(args[1],&mut us){return status;}
    let length=u16::from_le_bytes([us[0],us[1]]) as usize; let maximum=u16::from_le_bytes([us[2],us[3]]) as usize;
    let address=u64::from_le_bytes(us[8..16].try_into().unwrap());
    if length%2!=0 || length>maximum || length/2>NT_REGISTRY_NAME_UNITS || (length!=0 && address==0){return NT_STATUS_INVALID_PARAMETER;}
    let mut name_bytes=[0u8;NT_REGISTRY_NAME_UNITS*2];
    if length!=0 {if let Err(status)=copy_nt_input(address,&mut name_bytes[..length]){return status;}}
    let mut units=[0u16;NT_REGISTRY_NAME_UNITS]; for i in 0..length/2{units[i]=u16::from_le_bytes([name_bytes[i*2],name_bytes[i*2+1]]);}
    let name=crate::ntoskrnl::cm::runtime::UnicodeString{length:length as u16,maximum_length:maximum as u16,buffer:units.as_ptr()};
    let mut data=[0u8;256];let n=args[5] as usize;
    if n!=0 {if let Err(status)=copy_nt_input(args[4],&mut data[..n]){return status;}}
    unsafe {crate::ntoskrnl::cm::runtime::ZwSetValueKey(args[0] as *mut core::ffi::c_void,&name,args[2] as u32,args[3] as u32,data.as_ptr().cast(),n as u32) as u32 as u64}
}
fn nt_delete_value_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0]==0 || args[1]==0{return NT_STATUS_INVALID_PARAMETER;}
    let mut us=[0u8;16];if let Err(status)=copy_nt_input(args[1],&mut us){return status;}
    let length=u16::from_le_bytes([us[0],us[1]]) as usize;let maximum=u16::from_le_bytes([us[2],us[3]]) as usize;
    let address=u64::from_le_bytes(us[8..16].try_into().unwrap());
    if length%2!=0||length>maximum||length/2>NT_REGISTRY_NAME_UNITS||(length!=0&&address==0){return NT_STATUS_INVALID_PARAMETER;}
    let mut bytes=[0u8;NT_REGISTRY_NAME_UNITS*2];if length!=0{if let Err(status)=copy_nt_input(address,&mut bytes[..length]){return status;}}
    let mut units=[0u16;NT_REGISTRY_NAME_UNITS];for i in 0..length/2{units[i]=u16::from_le_bytes([bytes[i*2],bytes[i*2+1]]);}
    let name=crate::ntoskrnl::cm::runtime::UnicodeString{length:length as u16,maximum_length:maximum as u16,buffer:units.as_ptr()};
    unsafe{crate::ntoskrnl::cm::runtime::ZwDeleteValueKey(args[0] as *mut core::ffi::c_void,&name) as u32 as u64}
}
fn nt_delete_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0]==0{return NT_STATUS_INVALID_PARAMETER;}
    unsafe{crate::ntoskrnl::cm::runtime::ZwDeleteKey(args[0] as *mut core::ffi::c_void) as u32 as u64}
}
fn nt_registry_info(args: &[u64; NT_MAX_ARGUMENTS], operation: u8) -> u64 {
    let is_query = operation == 2;
    let handle = args[0];
    let index_or_class = if is_query { args[1] } else { args[1] };
    let class = if is_query { args[1] } else { args[2] };
    let output_arg = if is_query { 2 } else { 3 };
    let length_arg = if is_query { 3 } else { 4 };
    let result_arg = if is_query { 4 } else { 5 };
    if handle == 0 || (args[output_arg] == 0 && args[length_arg] != 0) {
        return NT_STATUS_INVALID_PARAMETER;
    }
    let capacity = core::cmp::min(args[length_arg] as usize, NT_REGISTRY_QUERY_MAX);
    let mut output = [0u8; NT_REGISTRY_QUERY_MAX];
    let mut required = 0u32;
    let status = unsafe {
        match operation {
            0 => crate::ntoskrnl::cm::runtime::ZwEnumerateKey(
                handle as *mut core::ffi::c_void, index_or_class as u32, class as u32,
                output.as_mut_ptr().cast(), capacity as u32, &mut required),
            1 => crate::ntoskrnl::cm::runtime::ZwEnumerateValueKey(
                handle as *mut core::ffi::c_void, index_or_class as u32, class as u32,
                output.as_mut_ptr().cast(), capacity as u32, &mut required),
            _ => crate::ntoskrnl::cm::runtime::ZwQueryKey(
                handle as *mut core::ffi::c_void, class as u32,
                output.as_mut_ptr().cast(), capacity as u32, &mut required),
        }
    };
    if result_arg < NT_MAX_ARGUMENTS && args[result_arg] != 0 {
        if let Err(fault) = write_return_length(args[result_arg], required) { return fault; }
    }
    if status == 0 {
        let count = core::cmp::min(required as usize, capacity);
        if let Err(fault) = copy_nt_output(args[output_arg], &output[..count]) { return fault; }
    }
    status as u32 as u64
}
fn nt_enumerate_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 { nt_registry_info(args, 0) }
fn nt_enumerate_value_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 { nt_registry_info(args, 1) }
fn nt_query_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 { nt_registry_info(args, 2) }

fn nt_flush_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0]==0{return NT_STATUS_INVALID_PARAMETER;}
    unsafe{crate::ntoskrnl::cm::runtime::ZwFlushKey(args[0] as *mut core::ffi::c_void) as u32 as u64}
}

fn nt_query_value_key(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] == 0 || args[1] == 0 || (args[3] == 0 && args[4] != 0) { return NT_STATUS_INVALID_PARAMETER; }
    let mut us = [0u8; 16];
    if let Err(status) = copy_nt_input(args[1], &mut us) { return status; }
    let length = u16::from_le_bytes([us[0], us[1]]) as usize;
    let maximum = u16::from_le_bytes([us[2], us[3]]) as usize;
    let buffer_address = u64::from_le_bytes(us[8..16].try_into().unwrap());
    if length % 2 != 0 || length > maximum || length / 2 > NT_REGISTRY_NAME_UNITS
        || (length != 0 && buffer_address == 0) { return NT_STATUS_INVALID_PARAMETER; }
    let mut raw = [0u8; NT_REGISTRY_NAME_UNITS * 2];
    if length != 0 {
        if let Err(status) = copy_nt_input(buffer_address, &mut raw[..length]) { return status; }
    }
    let mut name_buffer = [0u16; NT_REGISTRY_NAME_UNITS];
    for i in 0..length / 2 { name_buffer[i] = u16::from_le_bytes([raw[i*2], raw[i*2+1]]); }
    let name = crate::ntoskrnl::cm::runtime::UnicodeString {
        length: length as u16, maximum_length: maximum as u16, buffer: name_buffer.as_ptr()
    };
    let mut result = [0u8; NT_REGISTRY_QUERY_MAX];
    let mut required = 0u32;
    let capacity = core::cmp::min(args[4] as usize, result.len());
    let status = unsafe {
        crate::ntoskrnl::cm::runtime::ZwQueryValueKey(
            args[0] as *mut core::ffi::c_void, &name, args[2] as u32,
            result.as_mut_ptr().cast(), capacity as u32, &mut required
        )
    };
    if args[5] != 0 {
        if let Err(fault) = write_return_length(args[5], required) { return fault; }
    }
    if status == 0 {
        let copy_len = core::cmp::min(required as usize, capacity);
        if let Err(fault) = copy_nt_output(args[3], &result[..copy_len]) { return fault; }
    }
    status as u32 as u64
}


const AMD64_CONTEXT_SIZE: usize = 0x4d0;
const AMD64_EXCEPTION_RECORD_SIZE: usize = 0x98;
#[repr(align(16))]
struct AlignedAmd64Context([u8; AMD64_CONTEXT_SIZE]);
const NT_STATUS_BAD_STACK: u64 = 0xC000_0028;
const NT_STATUS_INVALID_DISPOSITION: u64 = 0xC000_0026;

fn valid_user_code_pointer(value: u64) -> bool {
    value >= 0x1000 && value < crate::ntoskrnl::mm::address_space::USER_END
}

fn context_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn context_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// Apply a validated Windows AMD64 CONTEXT to the normalized INT 0x83
/// return frame. Indices are the exact register/IRET layout built by the gate.
unsafe fn restore_context_to_syscall_frame(frame: *mut u64, bytes: &[u8]) -> u64 {
    if bytes.len() != AMD64_CONTEXT_SIZE { return NT_STATUS_INVALID_PARAMETER; }
    let flags = context_u32(bytes, 0x30);
    if flags & 0x0010_0000 == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let rip = context_u64(bytes, 0xf8);
    let rsp = context_u64(bytes, 0x98);
    if !valid_user_code_pointer(rip) || !valid_user_code_pointer(rsp) || rsp & 7 != 0 {
        return NT_STATUS_BAD_STACK;
    }
    let cs = u16::from_le_bytes(bytes[0x38..0x3a].try_into().unwrap()) as u64;
    let ss = u16::from_le_bytes(bytes[0x42..0x44].try_into().unwrap()) as u64;
    if cs & 3 != 3 || ss & 3 != 3 { return NT_STATUS_INVALID_PARAMETER; }
    let mut rflags = context_u32(bytes, 0x44) as u64;
    // Never restore IOPL, NT, VM, or reserved bits from a user-provided context.
    rflags &= 0x0000_0000_003f_7fd7;
    rflags &= !((3u64 << 12) | (1u64 << 14) | (1u64 << 17));
    rflags |= 0x202; // reserved bit 1 and interrupt enable
    let f = core::slice::from_raw_parts_mut(frame, 20);
    f[0] = context_u64(bytes, 0xf0); // R15
    f[1] = context_u64(bytes, 0xe8); // R14
    f[2] = context_u64(bytes, 0xe0); // R13
    f[3] = context_u64(bytes, 0xd8); // R12
    f[4] = context_u64(bytes, 0xd0); // R11
    f[5] = context_u64(bytes, 0xc8); // R10
    f[6] = context_u64(bytes, 0xc0); // R9
    f[7] = context_u64(bytes, 0xb8); // R8
    f[8] = context_u64(bytes, 0xa0); // RBP
    f[9] = context_u64(bytes, 0xb0); // RDI
    f[10] = context_u64(bytes, 0xa8); // RSI
    f[11] = context_u64(bytes, 0x88); // RDX
    f[12] = context_u64(bytes, 0x80); // RCX
    f[13] = context_u64(bytes, 0x90); // RBX
    f[14] = context_u64(bytes, 0x78); // RAX
    f[15] = rip;
    f[16] = cs;
    f[17] = rflags;
    f[18] = rsp;
    f[19] = ss;
    // Restore the Windows FXSAVE area only after every fallible user-memory
    // operation has completed. The syscall return stub performs no SIMD work.
    if flags & 0x0010_0008 == 0x0010_0008 {
        // Reject reserved MXCSR bits before FXRSTOR so malformed CONTEXT data
        // cannot turn a user syscall into a supervisor #GP fault.
        if context_u32(bytes, 0x118) & !0x0000_ffbf != 0 {
            return NT_STATUS_INVALID_PARAMETER;
        }
        let fp_state = unsafe { bytes.as_ptr().add(0x100) };
        // Keep the scheduler's saved user xstate coherent with the CONTEXT we
        // are about to IRET into; otherwise the next context switch could undo
        // NtContinue's restored FP/SSE registers.
        let saved_xstate = crate::ntoskrnl::ke::thread::cognacos_current_user_xstate_ptr() as *mut u8;
        if !saved_xstate.is_null() {
            unsafe {
                core::ptr::copy_nonoverlapping(fp_state, saved_xstate, 512);
                core::ptr::write_bytes(saved_xstate.add(512), 0, 64);
                core::ptr::write_unaligned(saved_xstate.add(512).cast::<u64>(), 3);
            }
        }
        unsafe { core::arch::asm!("fxrstor64 [{}]", in(reg) fp_state, options(nostack)); }
    }
    NT_STATUS_SUCCESS
}

/// NtContinue is implemented by replacing the syscall's saved user return
/// frame. Returning success means IRETQ will resume at CONTEXT.Rip/Rsp.
unsafe fn nt_continue_from_user(frame: *mut u64, context_address: u64) -> u64 {
    let mut bytes = AlignedAmd64Context([0u8; AMD64_CONTEXT_SIZE]);
    if !copy_current_user_from(context_address, &mut bytes.0) { return NT_STATUS_ACCESS_VIOLATION; }
    restore_context_to_syscall_frame(frame, &bytes.0)
}

/// Build a first-chance user exception-dispatch entry frame. The record/context
/// are copied to a bounded region of the current user stack before changing RIP.
unsafe fn route_user_exception(frame: *mut u64, args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    let record_address = args[0];
    let context_address = args[1];
    let first_chance = args[2] != 0;
    let dispatcher = args[3];
    let mut record = [0u8; AMD64_EXCEPTION_RECORD_SIZE];
    let mut context = [0u8; AMD64_CONTEXT_SIZE];
    if !copy_current_user_from(record_address, &mut record)
        || !copy_current_user_from(context_address, &mut context) {
        return NT_STATUS_ACCESS_VIOLATION;
    }
    if !first_chance {
        let code = u32::from_le_bytes(record[0..4].try_into().unwrap()) as u64;
        let pid = current_process_id();
        if pid != 0 { crate::ntoskrnl::ke::thread::terminate_current_process_from_fault(pid, code); }
        return NT_STATUS_INVALID_DISPOSITION;
    }
    if !valid_user_code_pointer(dispatcher) { return NT_STATUS_INVALID_PARAMETER; }
    // The supplied context must describe a plausible user-mode frame.
    if context_u32(&context, 0x30) & 0x0010_0000 == 0
        || !valid_user_code_pointer(context_u64(&context, 0xf8))
        || !valid_user_code_pointer(context_u64(&context, 0x98)) {
        return NT_STATUS_INVALID_PARAMETER;
    }
    let f = core::slice::from_raw_parts(frame, 20);
    let old_rsp = f[18];
    let record_offset = 0x28usize;
    let context_offset = (record_offset + AMD64_EXCEPTION_RECORD_SIZE + 15) & !15;
    let total = context_offset + AMD64_CONTEXT_SIZE + 16;
    let Some(raw_start) = old_rsp.checked_sub(total as u64) else { return NT_STATUS_BAD_STACK; };
    let new_rsp = (raw_start & !15) + 8;
    if new_rsp < 0x1000 || new_rsp >= old_rsp || !valid_user_code_pointer(new_rsp) {
        return NT_STATUS_BAD_STACK;
    }
    let record_user = new_rsp + record_offset as u64;
    let context_user = new_rsp + context_offset as u64;
    let mut return_address = [0u8; 8]; // dispatcher must not return to arbitrary code
    if !copy_current_user_to(new_rsp, &return_address)
        || !copy_current_user_to(record_user, &record)
        || !copy_current_user_to(context_user, &context) {
        return NT_STATUS_ACCESS_VIOLATION;
    }
    let f = core::slice::from_raw_parts_mut(frame, 20);
    f[0] = 0; f[1] = 0; f[2] = 0; f[3] = 0; f[4] = 0;
    f[5] = 0; f[6] = 0; f[7] = 0; f[8] = 0; f[9] = 0;
    f[10] = 0; f[11] = context_user; f[12] = record_user;
    f[13] = 0; f[14] = 0;
    f[15] = dispatcher;
    f[17] = 0x202;
    f[18] = new_rsp;
    NT_STATUS_SUCCESS
}


const EXCEPTION_DISPATCHER_CAPACITY: usize = 128;
#[derive(Clone, Copy)]
struct ExceptionDispatcherSlot { owner: u64, address: u64 }
impl ExceptionDispatcherSlot { const EMPTY: Self = Self { owner: 0, address: 0 }; }
static EXCEPTION_DISPATCHER_LOCK: AtomicBool = AtomicBool::new(false);
static mut EXCEPTION_DISPATCHERS: [ExceptionDispatcherSlot; EXCEPTION_DISPATCHER_CAPACITY] =
    [ExceptionDispatcherSlot::EMPTY; EXCEPTION_DISPATCHER_CAPACITY];
struct ExceptionDispatcherGuard;
impl ExceptionDispatcherGuard {
    fn acquire() -> Self {
        while EXCEPTION_DISPATCHER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for ExceptionDispatcherGuard {
    fn drop(&mut self) { EXCEPTION_DISPATCHER_LOCK.store(false, Ordering::Release); }
}
fn register_exception_dispatcher(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    let owner = current_process_id();
    let address = args[0];
    if owner == 0 || !valid_user_code_pointer(address) { return NT_STATUS_INVALID_PARAMETER; }
    let _guard = ExceptionDispatcherGuard::acquire();
    let slots = unsafe { &mut *core::ptr::addr_of_mut!(EXCEPTION_DISPATCHERS) };
    if let Some(slot) = slots.iter_mut().find(|slot| slot.owner == owner) {
        slot.address = address;
        return NT_STATUS_SUCCESS;
    }
    let Some(slot) = slots.iter_mut().find(|slot| slot.owner == 0) else { return 0xC000_009A; };
    *slot = ExceptionDispatcherSlot { owner, address };
    NT_STATUS_SUCCESS
}
pub fn exception_dispatcher_for(owner: u64) -> u64 {
    let _guard = ExceptionDispatcherGuard::acquire();
    let slots = unsafe { &*core::ptr::addr_of!(EXCEPTION_DISPATCHERS) };
    slots.iter().find(|slot| slot.owner == owner).map(|slot| slot.address).unwrap_or(0)
}
pub fn exception_dispatcher_rundown(owner: u64) {
    let _guard = ExceptionDispatcherGuard::acquire();
    let slots = unsafe { &mut *core::ptr::addr_of_mut!(EXCEPTION_DISPATCHERS) };
    for slot in slots.iter_mut() { if slot.owner == owner { *slot = ExceptionDispatcherSlot::EMPTY; } }
}
static NTDLL_DEBUG_EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

const EVENT_MODIFY_STATE: u32 = 0x0002;
const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;
const EVENT_TYPE_NAME: [u16; 5] = [b'E' as u16, b'v' as u16, b'e' as u16, b'n' as u16, b't' as u16];
/// Construct a self-relative security descriptor from the effective token's
/// TOKEN_DEFAULT_DACL. Impersonation token takes precedence over the process
/// primary token. The queried ACL and descriptor are kernel-owned stack copies
/// and are consumed synchronously by Object Manager registration.
fn native_event_default_security_descriptor() -> Result<([u8; 256], usize), u64> {
    let mut token = crate::thread::reference_current_impersonation_token();
    if token.is_null() {
        token = crate::process::reference_primary_token(crate::thread::current_process_id());
    }
    if token.is_null() { return Err(0xC000_0078); } // STATUS_NO_TOKEN
    let mut acl = [0u8; 236];
    let mut acl_len = 0usize;
    let status = unsafe {
        crate::ntoskrnl::se::runtime::se_internal_TokenQueryDefaultDacl(
            token, acl.as_mut_ptr(), acl.len(), &mut acl_len,
        )
    };
    let _ = unsafe { crate::ntoskrnl::se::runtime::se_internal_DereferenceToken(token) };
    if status < 0 { return Err(status as u32 as u64); }
    if acl_len < 8 || acl_len > acl.len() { return Err(0xC000_000D); }
    let acl_size = u16::from_le_bytes([acl[2], acl[3]]) as usize;
    if acl_size != acl_len { return Err(0xC000_000D); }
    let mut sd = [0u8; 256];
    sd[0] = 1; // SECURITY_DESCRIPTOR_REVISION
    sd[2..4].copy_from_slice(&0x8004u16.to_le_bytes()); // SELF_RELATIVE | DACL_PRESENT
    sd[16..20].copy_from_slice(&20u32.to_le_bytes());
    sd[20..20 + acl_len].copy_from_slice(&acl[..acl_len]);
    Ok((sd, 20 + acl_len))
}

struct NativeEventSlot {
    used: AtomicBool,
    header: core::cell::UnsafeCell<core::mem::MaybeUninit<crate::ntoskrnl::ke::runtime::DispatcherHeader>>,
}
impl NativeEventSlot {
    const fn new() -> Self {
        Self { used: AtomicBool::new(false), header: core::cell::UnsafeCell::new(core::mem::MaybeUninit::uninit()) }
    }
    fn header_ptr(&self) -> *mut crate::ntoskrnl::ke::runtime::DispatcherHeader {
        unsafe { (*self.header.get()).as_mut_ptr() }
    }
}
unsafe impl Sync for NativeEventSlot {}
static NATIVE_EVENT_SLOTS: [NativeEventSlot; 64] = [const { NativeEventSlot::new() }; 64];

unsafe extern "win64" fn native_event_finalizer(body: *mut core::ffi::c_void) {
    for slot in &NATIVE_EVENT_SLOTS {
        if slot.header_ptr().cast::<core::ffi::c_void>() == body {
            slot.used.store(false, Ordering::Release);
            return;
        }
    }
}

fn native_event_type_id() -> Result<u16, i32> {
    crate::ntoskrnl::ob::runtime::ensure_type(&EVENT_TYPE_NAME, 1, 2, 4, 7, u32::MAX)
}


/// Parse the x64 OBJECT_ATTRIBUTES/UNICODE_STRING supplied by a user process.
/// Names are copied into kernel memory before namespace operations. Relative
/// names are resolved against a directory handle; security descriptors are
/// rejected until user-mode descriptor capture/validation is implemented.
fn native_event_object_attributes(address: u64, owner: usize, reject_security_descriptor: bool) -> Result<([u16; 128], usize, u32), i32> {
    let mut name = [0u16; 128];
    if address == 0 { return Ok((name, 0, 0)); }
    let mut oa = [0u8; 48];
    copy_nt_input(address, &mut oa).map_err(|s| s as u32 as i32)?;
    let length = u32::from_le_bytes(oa[0..4].try_into().unwrap());
    if length < 48 { return Err(NT_STATUS_INVALID_PARAMETER as u32 as i32); }
    let root = u64::from_le_bytes(oa[8..16].try_into().unwrap());
    let us_address = u64::from_le_bytes(oa[16..24].try_into().unwrap());
    let attributes = u32::from_le_bytes(oa[24..28].try_into().unwrap());
    let security_descriptor = u64::from_le_bytes(oa[32..40].try_into().unwrap());
    if reject_security_descriptor && security_descriptor != 0 { return Err(0xC000_00BBu32 as i32); }
    if us_address == 0 { return Ok((name, 0, attributes)); }
    let mut us = [0u8; 16];
    copy_nt_input(us_address, &mut us).map_err(|s| s as u32 as i32)?;
    let bytes = u16::from_le_bytes(us[0..2].try_into().unwrap()) as usize;
    let maximum = u16::from_le_bytes(us[2..4].try_into().unwrap()) as usize;
    let buffer = u64::from_le_bytes(us[8..16].try_into().unwrap());
    if bytes == 0 || bytes & 1 != 0 || bytes > maximum || bytes / 2 > name.len() || buffer == 0 {
        return Err(NT_STATUS_INVALID_PARAMETER as u32 as i32);
    }
    let units = bytes / 2;
    let mut raw = [0u8; 256];
    copy_nt_input(buffer, &mut raw[..bytes]).map_err(|s| s as u32 as i32)?;
    for i in 0..units {
        name[i] = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
        if name[i] == 0 { return Err(NT_STATUS_INVALID_PARAMETER as u32 as i32); }
    }
    if name[0] == b'\\' as u16 { return Ok((name, units, attributes)); }
    if root == 0 { return Err(0xC000_003Bu32 as i32); } // STATUS_OBJECT_PATH_SYNTAX_BAD
    let directory_type_name = [b'D' as u16,b'i' as u16,b'r' as u16,b'e' as u16,b'c' as u16,b't' as u16,b'o' as u16,b'r' as u16,b'y' as u16];
    let directory_type = crate::ntoskrnl::ob::runtime::ensure_type(&directory_type_name, 1, 2, 4, 7, u32::MAX)?;
    let body = crate::ntoskrnl::ob::runtime::reference_by_handle(root as *mut core::ffi::c_void, 0, directory_type, owner, false)?;
    let mut prefix = [0u16; 128];
    let prefix_len = crate::ntoskrnl::ob::runtime::object_name(body, &mut prefix)?;
    let _ = crate::ntoskrnl::ob::runtime::dereference_body(body);
    if prefix_len == 0 || prefix_len + 1 + units > name.len() { return Err(0xC000_003Bu32 as i32); }
    let mut full = [0u16; 128];
    full[..prefix_len].copy_from_slice(&prefix[..prefix_len]);
    let mut total = prefix_len;
    if full[total - 1] != b'\\' as u16 { full[total] = b'\\' as u16; total += 1; }
    full[total..total + units].copy_from_slice(&name[..units]);
    total += units;
    Ok((full, total, attributes))
}

fn nt_open_event(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] == 0 || args[2] == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let owner = crate::ntoskrnl::ob::runtime::current_owner();
    let (name, name_len, attributes) = match native_event_object_attributes(args[2], owner, false) {
        Ok(value) => value,
        Err(status) => return status as u32 as u64,
    };
    if name_len == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let event_type = match native_event_type_id() { Ok(value) => value, Err(status) => return status as u32 as u64 };
    match crate::ntoskrnl::ob::runtime::reference_by_name(&name[..name_len], event_type, args[1] as u32, false) {
        Err(status) => return status as u32 as u64,
        Ok(body) => { let _ = crate::ntoskrnl::ob::runtime::dereference_body(body); }
    }
    let handle = match crate::ntoskrnl::ob::runtime::open_by_name(&name[..name_len], args[1] as u32, owner, false, attributes) {
        Ok(handle) => handle,
        Err(status) => return status as u32 as u64,
    };
    if let Err(status) = copy_nt_output(args[0], &(handle as usize as u64).to_le_bytes()) {
        let _ = crate::ntoskrnl::ob::runtime::close_handle(handle, owner);
        return status;
    }
    NT_STATUS_SUCCESS
}

fn nt_create_event(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] == 0 || args[3] > 1 || args[4] > 1 {
        return NT_STATUS_INVALID_PARAMETER;
    }
    let owner = crate::ntoskrnl::ob::runtime::current_owner();
    let (name, name_len, attributes) = match native_event_object_attributes(args[2], owner, true) {
        Ok(value) => value,
        Err(status) => return status as u32 as u64,
    };
    let named = name_len != 0;
    if named {
        let event_type = match native_event_type_id() { Ok(value) => value, Err(status) => return status as u32 as u64 };
        match crate::ntoskrnl::ob::runtime::reference_by_name(&name[..name_len], event_type, args[1] as u32, false) {
            Ok(body) => {
                let _ = crate::ntoskrnl::ob::runtime::dereference_body(body);
                if attributes & 0x80 == 0 { return 0xC000_0035; } // STATUS_OBJECT_NAME_COLLISION
                return match crate::ntoskrnl::ob::runtime::open_by_name(&name[..name_len], args[1] as u32, owner, false, attributes) {
                    Ok(handle) => { let raw = handle as usize as u64; if let Err(status) = copy_nt_output(args[0], &raw.to_le_bytes()) { let _ = crate::ntoskrnl::ob::runtime::close_handle(handle, owner); return status; } 0x4000_0000 },
                    Err(status) => status as u32 as u64,
                };
            }
            Err(status) if status as u32 as u64 == 0xC000_0034 => {} // STATUS_OBJECT_NAME_NOT_FOUND
            Err(status) if status as u32 as u64 == 0xC000_0024 && attributes & 0x80 != 0 => return status as u32 as u64,
            Err(status) if status as u32 as u64 == 0xC000_0024 => return 0xC000_0035,
            Err(status) => return status as u32 as u64,
        }
    }
    let Some(slot) = NATIVE_EVENT_SLOTS.iter().find(|slot|
        slot.used.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok()
    ) else { return 0xC000_009A; }; // STATUS_INSUFFICIENT_RESOURCES
    let header = slot.header_ptr();
    unsafe { crate::ntoskrnl::ke::runtime::ke_initialize_event(header, args[3] as i32, args[4] as u8); }
    let (security_descriptor, security_descriptor_len) = match native_event_default_security_descriptor() {
        Ok(value) => value,
        Err(status) => { slot.used.store(false, Ordering::Release); return status; }
    };
    let registered = unsafe {
        crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(
            header.cast(), &EVENT_TYPE_NAME, &name[..name_len], false,
            security_descriptor.as_ptr(), security_descriptor_len, native_event_finalizer,
        )
    };
    if let Err(status) = registered {
        slot.used.store(false, Ordering::Release);
        return status as u32 as u64;
    }
    let handle = match crate::ntoskrnl::ob::runtime::open_by_pointer(
        header.cast(), args[1] as u32, owner, !named, attributes,
    ) {
        Ok(handle) => handle,
        Err(status) => {
            let _ = crate::ntoskrnl::ob::runtime::unregister_object(header.cast());
            return status as u32 as u64;
        }
    };
    // Drop the registration's initial object reference; the returned handle now
    // owns the lifetime, and outstanding waits add their own temporary reference.
    let _ = crate::ntoskrnl::ob::runtime::dereference_body(header.cast());
    let raw_handle = handle as usize as u64;
    if let Err(status) = copy_nt_output(args[0], &raw_handle.to_le_bytes()) {
        let _ = crate::ntoskrnl::ob::runtime::close_handle(handle, owner);
        return status;
    }
    NT_STATUS_SUCCESS
}

fn nt_set_or_reset_event(args: &[u64; NT_MAX_ARGUMENTS], set: bool) -> u64 {
    if args[0] == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let event_type = match native_event_type_id() {
        Ok(value) => value,
        Err(status) => return status as u32 as u64,
    };
    let owner = crate::ntoskrnl::ob::runtime::current_owner();
    let body = match crate::ntoskrnl::ob::runtime::reference_by_handle(
        args[0] as *mut core::ffi::c_void, EVENT_MODIFY_STATE, event_type, owner, false,
    ) {
        Ok(body) => body,
        Err(status) => return status as u32 as u64,
    };
    let previous = unsafe {
        if set {
            crate::ntoskrnl::ke::runtime::ke_set_event(body.cast(), 0, 0)
        } else {
            crate::ntoskrnl::ke::runtime::ke_reset_event(body.cast())
        }
    };
    let _ = crate::ntoskrnl::ob::runtime::dereference_body(body);
    if args[1] != 0 {
        if let Err(status) = copy_nt_output(args[1], &previous.to_le_bytes()) { return status; }
    }
    NT_STATUS_SUCCESS
}
fn nt_set_event(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 { nt_set_or_reset_event(args, true) }
fn nt_reset_event(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 { nt_set_or_reset_event(args, false) }

fn nt_wait_for_single_object(args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    if args[0] == 0 { return NT_STATUS_INVALID_PARAMETER; }
    let event_type = match native_event_type_id() {
        Ok(value) => value,
        Err(status) => return status as u32 as u64,
    };
    let owner = crate::ntoskrnl::ob::runtime::current_owner();
    let body = match crate::ntoskrnl::ob::runtime::reference_by_handle(
        args[0] as *mut core::ffi::c_void, SYNCHRONIZE_ACCESS, event_type, owner, false,
    ) {
        Ok(body) => body,
        Err(status) => return status as u32 as u64,
    };
    let mut timeout_value = 0i64;
    let timeout_ptr = if args[2] == 0 {
        core::ptr::null()
    } else {
        let mut bytes = [0u8; 8];
        if let Err(status) = copy_nt_input(args[2], &mut bytes) {
            let _ = crate::ntoskrnl::ob::runtime::dereference_body(body);
            return status;
        }
        timeout_value = i64::from_le_bytes(bytes);
        &timeout_value as *const i64
    };
    let status = unsafe {
        crate::ntoskrnl::ke::runtime::ke_wait_for_single_object(
            body.cast(), 0, 0, (args[1] & 0xff) as u8, timeout_ptr,
        )
    };
    let _ = crate::ntoskrnl::ob::runtime::dereference_body(body);
    status as u32 as u64
}


fn dispatch_native_nt(number: u64, args: &[u64; NT_MAX_ARGUMENTS]) -> u64 {
    match number {
        NT_SYSCALL_WAIT_ON_ADDRESS => wait_on_user_address(args),
        NT_SYSCALL_CREATE_EVENT => nt_create_event(args),
        NT_SYSCALL_SET_EVENT => nt_set_event(args),
        NT_SYSCALL_RESET_EVENT => nt_reset_event(args),
        NT_SYSCALL_WAIT_FOR_SINGLE_OBJECT => nt_wait_for_single_object(args),
        NT_SYSCALL_OPEN_EVENT => nt_open_event(args),
        NT_SYSCALL_WAKE_ADDRESS => wake_user_address(args,false),
        NT_SYSCALL_WAKE_ALL_ADDRESS => wake_user_address(args,true),
        NT_SYSCALL_DEBUG_EVENT => {
            let sequence = NTDLL_DEBUG_EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
            if sequence < 256 {
                unsafe {
                crate::serial_write(b"[DBG][NTDLL][EVENT] seq=");
                crate::serial_write_hex(sequence);
                crate::serial_write(b" event=");
                crate::serial_write_hex(args[0]);
                crate::serial_write(b" arg0=");
                crate::serial_write_hex(args[1]);
                crate::serial_write(b" arg1=");
                crate::serial_write_hex(args[2]);
                crate::serial_write(b" pid=");
                crate::serial_write_hex(current_process_id());
                crate::serial_write(b"\r\n");
                }
            }
            NT_STATUS_SUCCESS
        }
        NT_SYSCALL_CLOSE => {
            if crate::file_services::close_for_process(current_process_id(), args[0] as usize).is_ok() { return NT_STATUS_SUCCESS; }
            let status = unsafe { crate::ntoskrnl::ob::runtime::ObCloseHandle(args[0] as *mut core::ffi::c_void, 1) };
            status as u32 as u64
        }
        NT_SYSCALL_YIELD_EXECUTION => {
            // Windows returns STATUS_NO_YIELD_PERFORMED when no schedulable
            // peer thread can run. The current ring-3 bootstrap is not attached
            // to a scheduler-owned ETHREAD, so do not report a false success.
            0x4000_0024
        }
        NT_SYSCALL_QUERY_INFORMATION_PROCESS => nt_query_process(args),
        NT_SYSCALL_OPEN_PROCESS => nt_open_process(args),
        NT_SYSCALL_GET_NEXT_PROCESS => nt_get_next_process(args),
        NT_SYSCALL_QUERY_INFORMATION_THREAD => nt_query_thread(args),
        NT_SYSCALL_QUERY_SYSTEM_INFORMATION => nt_query_system(args),
        NT_SYSCALL_ETW_REGISTER => etw_register(args),
        NT_SYSCALL_ETW_UNREGISTER => etw_unregister(args),
        NT_SYSCALL_ETW_ENABLE_STATE => etw_enable_state(args),
        NT_SYSCALL_ETW_QUERY_EVENT_ENABLED => etw_query_event_enabled(args),
        NT_SYSCALL_ETW_WRITE_EVENT => etw_write_event(args),
        NT_SYSCALL_ETW_READ_EVENT => etw_read_event(args),
        NT_SYSCALL_ETW_CREATE_SESSION => etw_create_session(args),
        NT_SYSCALL_ETW_ENABLE_SESSION => etw_enable_session(args),
        NT_SYSCALL_ETW_STOP_SESSION => etw_stop_session(args),
        NT_SYSCALL_ETW_CREATE_NAMED_SESSION => etw_create_named_session(args),
        NT_SYSCALL_OPEN_KEY => nt_open_key(args),
        NT_SYSCALL_QUERY_VALUE_KEY => nt_query_value_key(args),
        NT_SYSCALL_CREATE_KEY => nt_create_key(args),
        NT_SYSCALL_SET_VALUE_KEY => nt_set_value_key(args),
        NT_SYSCALL_DELETE_VALUE_KEY => nt_delete_value_key(args),
        NT_SYSCALL_DELETE_KEY => nt_delete_key(args),
        NT_SYSCALL_FLUSH_KEY => nt_flush_key(args),
        NT_SYSCALL_ENUMERATE_KEY => nt_enumerate_key(args),
        NT_SYSCALL_ENUMERATE_VALUE_KEY => nt_enumerate_value_key(args),
        NT_SYSCALL_QUERY_KEY => nt_query_key(args),
        NT_SYSCALL_OPEN_FILE => nt_open_file(args),
        NT_SYSCALL_READ_FILE => nt_read_file(args),
        NT_SYSCALL_ALLOCATE_VIRTUAL_MEMORY => crate::ntoskrnl::mm::native_vm::allocate(args),
        NT_SYSCALL_FREE_VIRTUAL_MEMORY => crate::ntoskrnl::mm::native_vm::free(args),
        NT_SYSCALL_PROTECT_VIRTUAL_MEMORY => crate::ntoskrnl::mm::native_vm::protect(args),
        NT_SYSCALL_QUERY_VIRTUAL_MEMORY => crate::ntoskrnl::mm::native_vm::query(args),
        NT_SYSCALL_READ_VIRTUAL_MEMORY => crate::ntoskrnl::mm::native_vm::read(args),
        NT_SYSCALL_WRITE_VIRTUAL_MEMORY => crate::ntoskrnl::mm::native_vm::write(args),
        NT_SYSCALL_LOCK_VIRTUAL_MEMORY => crate::ntoskrnl::mm::native_vm::lock(args),
        NT_SYSCALL_UNLOCK_VIRTUAL_MEMORY => crate::ntoskrnl::mm::native_vm::unlock(args),
        NT_SYSCALL_GET_NONVOLATILE_TOKEN => crate::ntoskrnl::mm::native_vm::get_nonvolatile_token(args),
        NT_SYSCALL_FREE_NONVOLATILE_TOKEN => crate::ntoskrnl::mm::native_vm::free_nonvolatile_token(args),
        NT_SYSCALL_FLUSH_NONVOLATILE_MEMORY => crate::ntoskrnl::mm::native_vm::flush_nonvolatile_memory(args),
        NT_SYSCALL_WRITE_NONVOLATILE_MEMORY => crate::ntoskrnl::mm::native_vm::write_nonvolatile_memory(args),
        NT_SYSCALL_FILL_NONVOLATILE_MEMORY => crate::ntoskrnl::mm::native_vm::fill_nonvolatile_memory(args),
        NT_SYSCALL_CHECK_TOKEN_MEMBERSHIP => crate::ntoskrnl::mm::native_vm::check_token_membership(args),
        NT_SYSCALL_CREATE_SECTION => crate::ntoskrnl::mm::native_vm::create_section(args),
        NT_SYSCALL_DEREFERENCE_SECTION => crate::ntoskrnl::mm::native_vm::dereference_section(args),
        NT_SYSCALL_SECTION_COPY => crate::ntoskrnl::mm::native_vm::section_copy(args),
        NT_SYSCALL_NT_CREATE_SECTION => crate::ntoskrnl::mm::section_handles::create_nt(args),
        NT_SYSCALL_REGISTER_FUNCTION_TABLE => crate::ntoskrnl::runtime_functions::register(args),
        NT_SYSCALL_DELETE_FUNCTION_TABLE => crate::ntoskrnl::runtime_functions::delete(args),
        NT_SYSCALL_LOOKUP_FUNCTION_ENTRY => crate::ntoskrnl::runtime_functions::lookup(args),
        NT_SYSCALL_REGISTER_EXCEPTION_DISPATCHER => register_exception_dispatcher(args),
        NT_SYSCALL_REFERENCE_SECTION => crate::ntoskrnl::mm::native_vm::reference_section(args),
        NT_SYSCALL_ALPC_CREATE_PORT..=NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_CONTAINER_OF_PORT | NT_SYSCALL_ALPC_CONNECT_PORT_EX => crate::ntoskrnl::alpc_native::dispatch(number,args),
        _ => NT_STATUS_NOT_IMPLEMENTED,
    }
}

/// Enable the reference service table only after verifying the staged NTDLL,
/// or temporarily while running the kernel's built-in ABI smoke fixture.
pub fn set_windows_service_table_enabled(enabled: bool) {
    WINDOWS_SERVICE_TABLE_ENABLED.store(enabled, Ordering::Release);
}

/// Compatibility wrapper retained for boot diagnostics. It now parses the
/// current image's syscall stubs instead of checking a fixed build fingerprint.
pub fn windows_service_table_matches(image: &[u8]) -> bool {
    configure_windows_service_table_from_image(image)
}

/// Windows x64 service-number entry. Numbers are build-specific; unknown or
/// unverified tables fail closed instead of dispatching a possibly wrong API.
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_windows_syscall_dispatch(frame: *mut u64) -> u64 {
    if frame.is_null() { return NT_STATUS_ACCESS_VIOLATION; }
    if CALL_COUNT.load(Ordering::Relaxed) < 16 {
        let user_gs_base: u64;
        unsafe { core::arch::asm!("mov {}, gs:[40]", out(reg) user_gs_base, options(nostack, preserves_flags)); }
        unsafe {
            crate::serial_write(b"[DBG][NTOSKRNL][SYSCALL] saved user GS base=");
            crate::serial_write_hex(user_gs_base);
            crate::serial_write(b"\r\n");
        }
    }
    if !WINDOWS_SERVICE_TABLE_ENABLED.load(Ordering::Acquire) {
        CALL_COUNT.fetch_add(1, Ordering::AcqRel);
        return NT_STATUS_NOT_IMPLEMENTED;
    }
    let service = unsafe { *frame.add(14) };
    let slot = (0..WINDOWS_SERVICE_NUMBERS.len())
        .find(|slot| WINDOWS_SERVICE_NUMBERS[*slot].load(Ordering::Acquire) == service);
    let native = match slot {
        Some(SERVICE_NT_CLOSE) => NT_SYSCALL_CLOSE,
        Some(SERVICE_NT_YIELD_EXECUTION) => NT_SYSCALL_YIELD_EXECUTION,
        Some(SERVICE_NT_QUERY_INFORMATION_PROCESS) => NT_SYSCALL_QUERY_INFORMATION_PROCESS,
        Some(SERVICE_NT_QUERY_INFORMATION_THREAD) => NT_SYSCALL_QUERY_INFORMATION_THREAD,
        Some(SERVICE_NT_QUERY_SYSTEM_INFORMATION) => NT_SYSCALL_QUERY_SYSTEM_INFORMATION,
        Some(SERVICE_NT_OPEN_KEY) => NT_SYSCALL_OPEN_KEY,
        Some(SERVICE_NT_QUERY_VALUE_KEY) => NT_SYSCALL_QUERY_VALUE_KEY,
        Some(SERVICE_NT_CREATE_KEY) => NT_SYSCALL_CREATE_KEY,
        Some(SERVICE_NT_SET_VALUE_KEY) => NT_SYSCALL_SET_VALUE_KEY,
        Some(SERVICE_NT_DELETE_VALUE_KEY) => NT_SYSCALL_DELETE_VALUE_KEY,
        Some(SERVICE_NT_DELETE_KEY) => NT_SYSCALL_DELETE_KEY,
        Some(SERVICE_NT_FLUSH_KEY) => NT_SYSCALL_FLUSH_KEY,
        Some(SERVICE_NT_ENUMERATE_KEY) => NT_SYSCALL_ENUMERATE_KEY,
        Some(SERVICE_NT_ENUMERATE_VALUE_KEY) => NT_SYSCALL_ENUMERATE_VALUE_KEY,
        Some(SERVICE_NT_QUERY_KEY) => NT_SYSCALL_QUERY_KEY,
        Some(SERVICE_NT_OPEN_PROCESS) => NT_SYSCALL_OPEN_PROCESS,
        Some(SERVICE_NT_GET_NEXT_PROCESS) => NT_SYSCALL_GET_NEXT_PROCESS,
        Some(SERVICE_NT_CREATE_EVENT) => NT_SYSCALL_CREATE_EVENT,
        Some(SERVICE_NT_SET_EVENT) => NT_SYSCALL_SET_EVENT,
        Some(SERVICE_NT_RESET_EVENT) => NT_SYSCALL_RESET_EVENT,
        Some(SERVICE_NT_WAIT_FOR_SINGLE_OBJECT) => NT_SYSCALL_WAIT_FOR_SINGLE_OBJECT,
        Some(SERVICE_NT_OPEN_EVENT) => NT_SYSCALL_OPEN_EVENT,
        Some(SERVICE_NT_OPEN_FILE) => NT_SYSCALL_OPEN_FILE,
        Some(SERVICE_NT_READ_FILE) => NT_SYSCALL_READ_FILE,
        Some(SERVICE_NT_ALLOCATE_VIRTUAL_MEMORY) => NT_SYSCALL_ALLOCATE_VIRTUAL_MEMORY,
        Some(SERVICE_NT_FREE_VIRTUAL_MEMORY) => NT_SYSCALL_FREE_VIRTUAL_MEMORY,
        Some(SERVICE_NT_PROTECT_VIRTUAL_MEMORY) => NT_SYSCALL_PROTECT_VIRTUAL_MEMORY,
        Some(SERVICE_NT_QUERY_VIRTUAL_MEMORY) => NT_SYSCALL_QUERY_VIRTUAL_MEMORY,
        Some(SERVICE_NT_READ_VIRTUAL_MEMORY) => NT_SYSCALL_READ_VIRTUAL_MEMORY,
        Some(SERVICE_NT_WRITE_VIRTUAL_MEMORY) => NT_SYSCALL_WRITE_VIRTUAL_MEMORY,
        // Internal CognacOS executive services use the same architectural
        // Windows x64 SYSCALL entry and register/stack ABI. These IDs are
        // kernel-private service selectors, not exported NTDLL names. Known
        // exported Nt* stubs are always resolved by the PE-discovered table above.
        None if (0x100..=0x1ff).contains(&service) => service,
        _ => {
            CALL_COUNT.fetch_add(1, Ordering::AcqRel);
            return NT_STATUS_NOT_IMPLEMENTED;
        }
    };
    unsafe { *frame.add(14) = native; }
    let status = cognacos_nt_syscall_dispatch(frame);
    unsafe { *frame.add(14) = service; }
    status
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
        let trace_index = NT_SYSCALL_TRACE_COUNT.fetch_add(1, Ordering::Relaxed);
        if trace_index < 128 {
            crate::serial_write(b"[DBG][NTDLL->NTOSKRNL][SYSCALL] enter seq=");
            crate::serial_write_hex(trace_index);
            crate::serial_write(b" service=");
            crate::serial_write_hex(number);
            crate::serial_write(b" rip=");
            crate::serial_write_hex(f[15]);
            crate::serial_write(b" rsp=");
            crate::serial_write_hex(user_rsp);
            crate::serial_write(b" rcx=");
            crate::serial_write_hex(f[12]);
            crate::serial_write(b" rdx=");
            crate::serial_write_hex(f[11]);
            crate::serial_write(b" r8=");
            crate::serial_write_hex(f[7]);
            crate::serial_write(b" r9=");
            crate::serial_write_hex(f[6]);
            crate::serial_write(b"\r\n");
        }
        let mut args = [0u64; NT_MAX_ARGUMENTS];
        args[0] = f[5];  // R10 = arg1
        args[1] = f[11]; // RDX = arg2
        args[2] = f[7];  // R8 = arg3
        args[3] = f[6];  // R9 = arg4
        let count = match number {
            NT_SYSCALL_CLOSE => 1,
            NT_SYSCALL_YIELD_EXECUTION => 0,
            NT_SYSCALL_QUERY_INFORMATION_PROCESS | NT_SYSCALL_QUERY_INFORMATION_THREAD | NT_SYSCALL_GET_NEXT_PROCESS => 5,
            NT_SYSCALL_OPEN_PROCESS => 4,
            NT_SYSCALL_QUERY_SYSTEM_INFORMATION => 4,
            NT_SYSCALL_ETW_REGISTER => 2,
            NT_SYSCALL_ETW_UNREGISTER => 1,
            NT_SYSCALL_ETW_ENABLE_STATE => 2,
            NT_SYSCALL_ETW_WRITE_EVENT => 4,
            NT_SYSCALL_ETW_READ_EVENT => 5,
            NT_SYSCALL_ETW_CREATE_SESSION => 2,
            NT_SYSCALL_ETW_ENABLE_SESSION => 6,
            NT_SYSCALL_ETW_STOP_SESSION => 1,
            NT_SYSCALL_OPEN_KEY => 3,
            NT_SYSCALL_QUERY_VALUE_KEY => 6,
            NT_SYSCALL_ENUMERATE_KEY | NT_SYSCALL_ENUMERATE_VALUE_KEY => 6,
            NT_SYSCALL_QUERY_KEY => 5,
            NT_SYSCALL_CREATE_KEY => 7,
            NT_SYSCALL_SET_VALUE_KEY => 6,
            NT_SYSCALL_DELETE_VALUE_KEY => 2,
            NT_SYSCALL_DELETE_KEY => 1,
            NT_SYSCALL_FLUSH_KEY => 1,
            NT_SYSCALL_OPEN_FILE => 6,
            NT_SYSCALL_READ_FILE => 9,
            NT_SYSCALL_ALLOCATE_VIRTUAL_MEMORY => 6,
            NT_SYSCALL_FREE_VIRTUAL_MEMORY => 4,
            NT_SYSCALL_PROTECT_VIRTUAL_MEMORY => 5,
            NT_SYSCALL_QUERY_VIRTUAL_MEMORY => 6,
            NT_SYSCALL_READ_VIRTUAL_MEMORY | NT_SYSCALL_WRITE_VIRTUAL_MEMORY => 5,
            NT_SYSCALL_LOCK_VIRTUAL_MEMORY | NT_SYSCALL_UNLOCK_VIRTUAL_MEMORY => 4,
            NT_SYSCALL_GET_NONVOLATILE_TOKEN => 3,
            NT_SYSCALL_FREE_NONVOLATILE_TOKEN => 1,
            NT_SYSCALL_FLUSH_NONVOLATILE_MEMORY => 4,
            NT_SYSCALL_WRITE_NONVOLATILE_MEMORY | NT_SYSCALL_FILL_NONVOLATILE_MEMORY => 5,
            NT_SYSCALL_CHECK_TOKEN_MEMBERSHIP => 4,
            NT_SYSCALL_CREATE_SECTION => 2,
            NT_SYSCALL_DEREFERENCE_SECTION => 1,
            NT_SYSCALL_SECTION_COPY => 5,
            NT_SYSCALL_REFERENCE_SECTION => 1,
            NT_SYSCALL_ALPC_CREATE_PORT=>3,
            NT_SYSCALL_ALPC_DISCONNECT_PORT=>2,
            NT_SYSCALL_ALPC_CONNECT_PORT=>11,
            NT_SYSCALL_ALPC_ACCEPT_CONNECT_PORT=>9,
            NT_SYSCALL_ALPC_SEND_WAIT_RECEIVE_PORT=>8,
            NT_SYSCALL_ALPC_CREATE_PORT_SECTION=>6,
            NT_SYSCALL_ALPC_DELETE_PORT_SECTION=>3,
            NT_SYSCALL_ALPC_CREATE_SECTION_VIEW=>3,
            NT_SYSCALL_ALPC_DELETE_SECTION_VIEW=>3,
            NT_SYSCALL_ALPC_CREATE_SECURITY_CONTEXT=>3,
            NT_SYSCALL_ALPC_DELETE_SECURITY_CONTEXT=>3,
            NT_SYSCALL_ALPC_REVOKE_SECURITY_CONTEXT=>3,
            NT_SYSCALL_ALPC_CREATE_RESOURCE_RESERVE=>4,
            NT_SYSCALL_ALPC_DELETE_RESOURCE_RESERVE=>3,
            NT_SYSCALL_ALPC_QUERY_INFORMATION=>5,
            NT_SYSCALL_ALPC_SET_INFORMATION=>4,
            NT_SYSCALL_ALPC_QUERY_INFORMATION_MESSAGE=>6,
            NT_SYSCALL_ALPC_CANCEL_MESSAGE=>3,
            NT_SYSCALL_ALPC_OPEN_SENDER_PROCESS=>6,
            NT_SYSCALL_ALPC_OPEN_SENDER_THREAD=>6,
            NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_OF_PORT=>3,
            NT_SYSCALL_ALPC_IMPERSONATE_CLIENT_CONTAINER_OF_PORT=>3,
            NT_SYSCALL_ALPC_CONNECT_PORT_EX=>11,
            NT_SYSCALL_REGISTER_FUNCTION_TABLE => 3,
            NT_SYSCALL_DELETE_FUNCTION_TABLE => 1,
            NT_SYSCALL_LOOKUP_FUNCTION_ENTRY => 2,
            NT_SYSCALL_NT_CONTINUE => 2,
            NT_SYSCALL_NT_RAISE_EXCEPTION => 4,
            NT_SYSCALL_REGISTER_EXCEPTION_DISPATCHER => 1,
            NT_SYSCALL_WAIT_ON_ADDRESS => 4,
            NT_SYSCALL_CREATE_EVENT => 5,
            NT_SYSCALL_SET_EVENT | NT_SYSCALL_RESET_EVENT => 2,
            NT_SYSCALL_WAIT_FOR_SINGLE_OBJECT => 3,
            NT_SYSCALL_WAKE_ADDRESS | NT_SYSCALL_WAKE_ALL_ADDRESS => 1,
            NT_SYSCALL_DEBUG_EVENT => 3,
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
        if number == NT_SYSCALL_NT_CONTINUE {
            let status = nt_continue_from_user(frame, args[0]);
            return if status == NT_STATUS_SUCCESS { u64::MAX } else { status };
        }
        if number == NT_SYSCALL_NT_RAISE_EXCEPTION {
            return route_user_exception(frame, &args);
        }
        if trace_index < 32 {
            crate::serial_write(b"[DBG][NTOSKRNL][SYSCALL] dispatch-native begin service=");
            crate::serial_write_hex(number);
            crate::serial_write(b"\r\n");
        }
        let result = dispatch_native_nt(number, &args);
        if trace_index < 32 {
            crate::serial_write(b"[DBG][NTOSKRNL][SYSCALL] dispatch-native end service=");
            crate::serial_write_hex(number);
            crate::serial_write(b" status=");
            crate::serial_write_hex(result);
            crate::serial_write(b"\r\n");
        }
        result
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
    // Registry syscalls must reject invalid user pointers through checked copies,
    // before attempting to interpret caller-controlled OBJECT_ATTRIBUTES or names.
    let mut registry_args = [0u64; NT_MAX_ARGUMENTS];
    registry_args[0] = user_base + 0x200;
    registry_args[2] = 0x0000_7fff_ffff_f000;
    let invalid_registry_open = nt_open_key(&registry_args) == NT_STATUS_ACCESS_VIOLATION;
    registry_args = [0u64; NT_MAX_ARGUMENTS];
    registry_args[0] = 1;
    registry_args[1] = 0x0000_7fff_ffff_f000;
    registry_args[2] = 2;
    registry_args[3] = user_base + 0x200;
    registry_args[4] = 64;
    let invalid_registry_query = nt_query_value_key(&registry_args) == NT_STATUS_ACCESS_VIOLATION;
    // Decode all 13 stack arguments (arguments 5..17) from user memory.
    let mut stack = [0u8; (NT_MAX_ARGUMENTS - 4) * 8];
    for (i, chunk) in stack.chunks_exact_mut(8).enumerate() {
        chunk.copy_from_slice(&((i as u64) + 5).to_le_bytes());
    }
    let stack_ok = unsafe { space.copy_to_user(user_base + 0x300, &stack) }.is_ok()
        && decode_nt_stack_arguments(space, user_base + 0x2d8, 0xdead, [1,2,3,4], NT_MAX_ARGUMENTS) == NT_STATUS_NOT_IMPLEMENTED;
    unsafe { *ACTIVE_SPACE.0.get() = previous_space; }
    CURRENT_PID.store(previous_pid, Ordering::Release);
    five_arg_ok && return_len_ok && invalid_buffer_ok && unsupported_ok
        && invalid_registry_open && invalid_registry_query && stack_ok
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

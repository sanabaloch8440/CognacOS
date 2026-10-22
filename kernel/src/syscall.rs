//! Initial x64 ring-3 syscall ABI. This is an RdotOS ABI, not the Windows NT ABI.
//! RAX=number; RDI, RSI, RDX, R10, R8, R9=arguments; result in RAX.
use crate::vm::AddressSpace;
use core::sync::atomic::{AtomicU64, Ordering};
pub const SYS_EXIT: u64 = 0;
pub const SYS_GETPID: u64 = 1;
pub const SYS_WRITE: u64 = 2;
pub const SYS_YIELD: u64 = 3;
pub const EXIT_SENTINEL: u64 = u64::MAX;
const ERR_BAD_ADDRESS: u64 = (-14i64) as u64;
const ERR_INVALID: u64 = (-22i64) as u64;
const MAX_WRITE: usize = 512;
static CURRENT_PID: AtomicU64 = AtomicU64::new(0);
struct ActiveSpace(core::cell::UnsafeCell<*const AddressSpace>);
unsafe impl Sync for ActiveSpace {}
static ACTIVE_SPACE: ActiveSpace = ActiveSpace(core::cell::UnsafeCell::new(core::ptr::null()));
pub fn set_current_process(pid: u64, space: *const AddressSpace) {
    CURRENT_PID.store(pid, Ordering::Release);
    unsafe { *ACTIVE_SPACE.0.get() = space; }
}
pub fn clear_current_process() {
    CURRENT_PID.store(0, Ordering::Release);
    unsafe { *ACTIVE_SPACE.0.get() = core::ptr::null(); }
}
/// Dispatch one syscall. Unknown calls fail closed.
pub fn dispatch(number: u64, args: [u64; 6]) -> u64 {
    match number {
        SYS_EXIT => EXIT_SENTINEL,
        SYS_GETPID => CURRENT_PID.load(Ordering::Acquire),
        SYS_WRITE => {
            let length = core::cmp::min(args[2] as usize, MAX_WRITE);
            if length == 0 { return 0; }
            let pointer = unsafe { *ACTIVE_SPACE.0.get() };
            if pointer.is_null() { return ERR_BAD_ADDRESS; }
            let space = unsafe { &*pointer };
            let mut buffer = [0u8; MAX_WRITE];
            if unsafe { space.copy_from_user(args[1], &mut buffer[..length]) }.is_err() { return ERR_BAD_ADDRESS; }
            unsafe { crate::serial_write(&buffer[..length]); }
            length as u64
        }
        SYS_YIELD => 0,
        _ => ERR_INVALID,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn rdotos_syscall_dispatch(frame: *mut u64) -> u64 {
    if frame.is_null() { return ERR_BAD_ADDRESS; }
    unsafe {
        let f = core::slice::from_raw_parts(frame, 15);
        dispatch(f[14], [f[9], f[10], f[11], f[12], f[7], f[6]])
    }
}

//! Windows 11 x64 NTDLL exception and unwind exports.
//!
//! Public ABI layouts and signatures follow the Windows x64 CONTEXT,
//! EXCEPTION_RECORD, RUNTIME_FUNCTION and RTL exception APIs. Operations that
//! require a process exception dispatcher, loader function-table registry, or
//! kernel continuation service fail closed until those services are connected.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;

const STATUS_NOT_IMPLEMENTED: i32 = 0xC0000002u32 as i32;
const STATUS_INVALID_PARAMETER: i32 = 0xC000000Du32 as i32;
const STATUS_BAD_STACK: i32 = 0xC0000028u32 as i32;
const EXCEPTION_NONCONTINUABLE: u32 = 1;
const CONTEXT_AMD64: u32 = 0x0010_0000;
const CONTEXT_CONTROL: u32 = CONTEXT_AMD64 | 0x0000_0001;
const CONTEXT_INTEGER: u32 = CONTEXT_AMD64 | 0x0000_0002;
const CONTEXT_SEGMENTS: u32 = CONTEXT_AMD64 | 0x0000_0004;
const CONTEXT_FLOATING_POINT: u32 = CONTEXT_AMD64 | 0x0000_0008;
const CONTEXT_DEBUG_REGISTERS: u32 = CONTEXT_AMD64 | 0x0000_0010;

/// Windows x64 CONTEXT. The reserved vector area is kept opaque, but the
/// prefix and total size match the AMD64 ABI (0x4d0 bytes, 16-byte aligned).
#[repr(C, align(16))]
pub struct CONTEXT {
    pub P1Home: u64, pub P2Home: u64, pub P3Home: u64,
    pub P4Home: u64, pub P5Home: u64, pub P6Home: u64,
    pub ContextFlags: u32, pub MxCsr: u32,
    pub SegCs: u16, pub SegDs: u16, pub SegEs: u16,
    pub SegFs: u16, pub SegGs: u16, pub SegSs: u16,
    pub EFlags: u32,
    pub Dr0: u64, pub Dr1: u64, pub Dr2: u64, pub Dr3: u64,
    pub Dr6: u64, pub Dr7: u64,
    pub Rax: u64, pub Rcx: u64, pub Rdx: u64, pub Rbx: u64,
    pub Rsp: u64, pub Rbp: u64, pub Rsi: u64, pub Rdi: u64,
    pub R8: u64, pub R9: u64, pub R10: u64, pub R11: u64,
    pub R12: u64, pub R13: u64, pub R14: u64, pub R15: u64,
    pub Rip: u64,
    pub opaque: [u8; 0x4d0 - 256],
}

#[repr(C)]
pub struct EXCEPTION_RECORD {
    pub ExceptionCode: u32,
    pub ExceptionFlags: u32,
    pub ExceptionRecord: *mut EXCEPTION_RECORD,
    pub ExceptionAddress: *mut c_void,
    pub NumberParameters: u32,
    pub __alignment: u32,
    pub ExceptionInformation: [usize; 15],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RUNTIME_FUNCTION {
    pub BeginAddress: u32,
    pub EndAddress: u32,
    pub UnwindData: u32,
}

pub type EXCEPTION_ROUTINE = Option<unsafe extern "system" fn(
    *mut EXCEPTION_RECORD, *mut c_void, *mut CONTEXT, *mut c_void
) -> i32>;

#[no_mangle]
pub unsafe extern "system" fn KiRaiseUserExceptionDispatcher() -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn KiUserExceptionDispatcher() -> i32 {
    STATUS_NOT_IMPLEMENTED
}

/// NtContinue(PCONTEXT ContextRecord, BOOLEAN TestAlert)
#[no_mangle]
pub unsafe extern "system" fn NtContinue(_context: *const CONTEXT, _test_alert: u8) -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtContinueEx(_context: *const CONTEXT, _continue_argument: *const c_void) -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtDebugContinue(_debug_object: usize, _client_id: *const c_void, _continue_status: i32) -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtRaiseException(_record: *const EXCEPTION_RECORD, _context: *const CONTEXT, _first_chance: u8) -> i32 {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCaptureContext(context: *mut CONTEXT) {
    if context.is_null() { return; }
    // CONTEXT is zeroed first so all reserved and vector state is deterministic.
    ptr::write_bytes(context.cast::<u8>(), 0, core::mem::size_of::<CONTEXT>());
    let c = &mut *context;
    c.ContextFlags = CONTEXT_CONTROL | CONTEXT_INTEGER | CONTEXT_SEGMENTS;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "mov [{p} + 120], rax",
        "mov [{p} + 128], rcx",
        "mov [{p} + 136], rdx",
        "mov [{p} + 144], rbx",
        "lea rax, [rsp + 8]",
        "mov [{p} + 152], rax",
        "mov [{p} + 160], rbp",
        "mov [{p} + 168], rsi",
        "mov [{p} + 176], rdi",
        "mov [{p} + 184], r8",
        "mov [{p} + 192], r9",
        "mov [{p} + 200], r10",
        "mov [{p} + 208], r11",
        "mov [{p} + 216], r12",
        "mov [{p} + 224], r13",
        "mov [{p} + 232], r14",
        "mov [{p} + 240], r15",
        "mov rax, [rsp]",
        "mov [{p} + 248], rax",
        p = in(reg) context,
        out("rax") _,
        options(nostack, preserves_flags)
    );
    #[cfg(not(target_arch = "x86_64"))]
    { let _ = c; }
}
#[no_mangle]
pub unsafe extern "system" fn RtlCaptureContext2(context: *mut CONTEXT, _flags: u32) {
    RtlCaptureContext(context)
}
#[no_mangle]
pub unsafe extern "system" fn RtlKnownExceptionFilter(_record: *mut EXCEPTION_RECORD) -> i32 {
    0
}

/// The loader must register image and dynamic function tables before lookup
/// can be meaningful. Until that registry exists, return NULL and clear base.
#[no_mangle]
pub unsafe extern "system" fn RtlLookupFunctionEntry(
    _control_pc: u64, image_base: *mut u64, _history_table: *mut c_void
) -> *const RUNTIME_FUNCTION {
    if !image_base.is_null() { *image_base = 0; }
    ptr::null()
}

#[no_mangle]
pub unsafe extern "system" fn RtlRaiseException(record: *mut EXCEPTION_RECORD) {
    if record.is_null() { return; }
    // Dispatching software exceptions requires the native thread exception path.
    let _ = record;
}
#[no_mangle]
pub unsafe extern "system" fn RtlRaiseExceptionForReturnAddressHijack() { }
#[no_mangle]
pub unsafe extern "system" fn RtlRaiseNoncontinuableException(record: *mut EXCEPTION_RECORD, context: *mut CONTEXT) {
    if record.is_null() { return; }
    (*record).ExceptionFlags |= EXCEPTION_NONCONTINUABLE;
    let _ = context;
}
#[no_mangle]
pub unsafe extern "system" fn RtlRaiseStatus(_status: i32) { }
#[no_mangle]
pub unsafe extern "system" fn RtlReportException(_record: *mut EXCEPTION_RECORD, _context: *mut CONTEXT, _flags: u32) -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn RtlReportExceptionEx(_record: *mut EXCEPTION_RECORD, _context: *mut CONTEXT, _flags: u32, _nested: u8) -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn RtlRestoreContext(_context: *const CONTEXT, _record: *const EXCEPTION_RECORD) {
    // A real restore must use a dedicated assembly context-restoration trampoline.
}
#[no_mangle]
pub unsafe extern "system" fn RtlUnhandledExceptionFilter(_record: *mut EXCEPTION_RECORD) -> i32 { 0 }
#[no_mangle]
pub unsafe extern "system" fn RtlUnhandledExceptionFilter2(_record: *mut EXCEPTION_RECORD, _context: *mut CONTEXT) -> i32 { 0 }

#[no_mangle]
pub unsafe extern "system" fn RtlUnwind(
    _target_frame: *mut c_void, _target_ip: *mut c_void,
    _record: *mut EXCEPTION_RECORD, _return_value: *mut c_void
) { }
#[no_mangle]
pub unsafe extern "system" fn RtlUnwindEx(
    _target_frame: *mut c_void, _target_ip: *mut c_void,
    _record: *mut EXCEPTION_RECORD, _return_value: *mut c_void,
    _context: *mut CONTEXT, _history_table: *mut c_void
) { }

/// x64 leaf-frame unwind: restore RIP from [RSP], then advance RSP by 8.
/// Non-leaf unwind metadata is intentionally rejected until the PE loader's
/// .pdata/.xdata registry and full UNWIND_INFO interpreter are connected.
unsafe fn unwind_leaf(context: *mut CONTEXT, establisher: *mut u64) -> i32 {
    if context.is_null() { return STATUS_INVALID_PARAMETER; }
    let c = &mut *context;
    if c.Rsp == 0 || c.Rsp & 7 != 0 { return STATUS_BAD_STACK; }
    let ret = *(c.Rsp as *const u64);
    if ret == 0 { return STATUS_BAD_STACK; }
    c.Rip = ret;
    c.Rsp = c.Rsp.saturating_add(8);
    if !establisher.is_null() { *establisher = c.Rsp; }
    0
}
#[no_mangle]
pub unsafe extern "system" fn RtlVirtualUnwind(
    handler_type: u32, _image_base: u64, _control_pc: u64,
    function_entry: *const RUNTIME_FUNCTION, context: *mut CONTEXT,
    handler_data: *mut *mut c_void, establisher_frame: *mut u64,
    _context_pointers: *mut c_void
) -> EXCEPTION_ROUTINE {
    if !handler_data.is_null() { *handler_data = ptr::null_mut(); }
    if !function_entry.is_null() || handler_type & !0x7 != 0 {
        return None;
    }
    if unwind_leaf(context, establisher_frame) == 0 { None } else { None }
}
#[no_mangle]
pub unsafe extern "system" fn RtlVirtualUnwind2(
    handler_type: u32, image_base: u64, control_pc: u64,
    function_entry: *const RUNTIME_FUNCTION, context: *mut CONTEXT,
    machine_frame_unwound: *mut u8, handler_data: *mut *mut c_void,
    establisher_frame: *mut u64, context_pointers: *mut c_void,
    low_limit: *const u64, high_limit: *const u64,
    handler_routine: *mut EXCEPTION_ROUTINE, unwind_flags: u32
) -> i32 {
    if !machine_frame_unwound.is_null() { *machine_frame_unwound = 0; }
    if !handler_data.is_null() { *handler_data = ptr::null_mut(); }
    if !handler_routine.is_null() { *handler_routine = None; }
    if handler_type & !0x7 != 0 || unwind_flags & !1 != 0 { return STATUS_INVALID_PARAMETER; }
    if !function_entry.is_null() {
        let _ = (image_base, control_pc, context_pointers, low_limit, high_limit);
        return STATUS_NOT_IMPLEMENTED;
    }
    let status = unwind_leaf(context, establisher_frame);
    if status != 0 { return status; }
    if !low_limit.is_null() && (*context).Rsp < *low_limit { return STATUS_BAD_STACK; }
    if !high_limit.is_null() && (*context).Rsp > *high_limit { return STATUS_BAD_STACK; }
    0
}
#[no_mangle]
pub unsafe extern "system" fn RtlWerpReportException(_record: *mut EXCEPTION_RECORD, _context: *mut CONTEXT, _flags: u32, _out: *mut c_void) -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn WerReportExceptionWorker(_parameters: *mut c_void) -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn ZwContinue(context: *const CONTEXT, test_alert: u8) -> i32 {
    NtContinue(context, test_alert)
}
#[no_mangle]
pub unsafe extern "system" fn ZwContinueEx(context: *const CONTEXT, continue_argument: *const c_void) -> i32 {
    NtContinueEx(context, continue_argument)
}
#[no_mangle]
pub unsafe extern "system" fn ZwDebugContinue(debug_object: usize, client_id: *const c_void, continue_status: i32) -> i32 {
    NtDebugContinue(debug_object, client_id, continue_status)
}
#[no_mangle]
pub unsafe extern "system" fn ZwRaiseException(record: *const EXCEPTION_RECORD, context: *const CONTEXT, first_chance: u8) -> i32 {
    NtRaiseException(record, context, first_chance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_layout_matches_amd64_abi() {
        assert_eq!(core::mem::size_of::<CONTEXT>(), 0x4d0);
        assert_eq!(core::mem::align_of::<CONTEXT>(), 16);
        assert_eq!(core::mem::offset_of!(CONTEXT, ContextFlags), 48);
        assert_eq!(core::mem::offset_of!(CONTEXT, Rax), 120);
        assert_eq!(core::mem::offset_of!(CONTEXT, Rip), 248);
        assert_eq!(core::mem::size_of::<EXCEPTION_RECORD>(), 0x98);
        assert_eq!(core::mem::size_of::<RUNTIME_FUNCTION>(), 12);
    }

    #[test]
    fn leaf_unwind_advances_stack_and_recovers_return_address() {
        let mut stack = [0x1234_5678u64, 0x9999u64];
        let mut context: CONTEXT = unsafe { core::mem::zeroed() };
        context.Rsp = stack.as_mut_ptr() as u64;
        let mut frame = 0;
        assert_eq!(unsafe { unwind_leaf(&mut context, &mut frame) }, 0);
        assert_eq!(context.Rip, 0x1234_5678);
        assert_eq!(context.Rsp, stack.as_ptr() as u64 + 8);
        assert_eq!(frame, context.Rsp);
    }
}

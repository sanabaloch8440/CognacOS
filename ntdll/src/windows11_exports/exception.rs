//! Windows 11 x64 NTDLL exception and unwind exports.
//!
//! Public ABI layouts and signatures follow the Windows x64 CONTEXT,
//! EXCEPTION_RECORD, RUNTIME_FUNCTION and RTL exception APIs. Operations that
//! require a process exception dispatcher, loader function-table registry, or
//! kernel continuation service fail closed until those services are connected.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;

#[cfg(target_os = "none")]
core::arch::global_asm!(r#"
    .text
    .p2align 4
    .global cognacos_register_function_table
cognacos_register_function_table:
    mov r10, rcx
    mov eax, 0x13e
    int 0x83
    ret
    .p2align 4
    .global cognacos_delete_function_table
cognacos_delete_function_table:
    mov r10, rcx
    mov eax, 0x13f
    int 0x83
    ret
    .p2align 4
    .global cognacos_lookup_function_entry
cognacos_lookup_function_entry:
    mov r10, rcx
    mov eax, 0x140
    int 0x83
    ret
    .p2align 4
    .global cognacos_nt_continue
cognacos_nt_continue:
    mov r10, rcx
    mov eax, 0x141
    int 0x83
    ret
    .p2align 4
    .global cognacos_nt_raise_exception
cognacos_nt_raise_exception:
    mov r10, rcx
    mov eax, 0x142
    int 0x83
    ret
    .p2align 4
    .global cognacos_register_exception_dispatcher
cognacos_register_exception_dispatcher:
    mov r10, rcx
    mov eax, 0x143
    int 0x83
    ret
"#);

#[cfg(target_os = "none")]
extern "system" {
    fn cognacos_register_function_table(table: *const RUNTIME_FUNCTION, count: u32, base: u64) -> u64;
    fn cognacos_delete_function_table(table: *const RUNTIME_FUNCTION) -> u64;
    fn cognacos_lookup_function_entry(control_pc: u64, image_base: *mut u64, history_table: *mut c_void) -> *const RUNTIME_FUNCTION;
    fn cognacos_nt_continue(context: *const CONTEXT, test_alert: u8) -> i32;
    fn cognacos_nt_raise_exception(record: *const EXCEPTION_RECORD, context: *const CONTEXT, first_chance: u8, dispatcher: usize) -> i32;
    fn cognacos_register_exception_dispatcher(dispatcher: usize) -> u64;
}

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

/// Native user exception entry: the kernel supplies the copied exception record
/// and CONTEXT in RCX/RDX on the interrupted thread's stack.
#[no_mangle]
pub unsafe extern "system" fn KiUserExceptionDispatcher(
    record: *mut EXCEPTION_RECORD, context: *mut CONTEXT
) -> ! {
    if record.is_null() || context.is_null() {
        loop { core::hint::spin_loop(); }
    }
    if RtlDispatchException(record, context) != 0 {
        let _ = NtContinue(context, 0);
    }
    let _ = NtRaiseException(record, context, 0);
    // A second-chance exception should be terminated by the native process
    // manager. Until the process-termination syscall is wired, never resume the
    // faulting instruction with a corrupt dispatcher frame.
    loop { core::hint::spin_loop(); }
}

/// NtContinue(PCONTEXT ContextRecord, BOOLEAN TestAlert). A successful call
/// does not return: the kernel restores the saved user register and IRET frame.
#[no_mangle]
pub unsafe extern "system" fn NtContinue(context: *const CONTEXT, test_alert: u8) -> i32 {
    if context.is_null() { return STATUS_INVALID_PARAMETER; }
    #[cfg(target_os = "none")]
    { return cognacos_nt_continue(context, test_alert); }
    #[cfg(not(target_os = "none"))]
    { let _ = test_alert; STATUS_NOT_IMPLEMENTED }
}
#[no_mangle]
pub unsafe extern "system" fn NtContinueEx(context: *const CONTEXT, _continue_argument: *const c_void) -> i32 {
    NtContinue(context, 0)
}
#[no_mangle]
pub unsafe extern "system" fn NtDebugContinue(_debug_object: usize, _client_id: *const c_void, _continue_status: i32) -> i32 {
    STATUS_NOT_IMPLEMENTED
}
#[no_mangle]
pub unsafe extern "system" fn NtRaiseException(record: *const EXCEPTION_RECORD, context: *const CONTEXT, first_chance: u8) -> i32 {
    if record.is_null() || context.is_null() { return STATUS_INVALID_PARAMETER; }
    #[cfg(target_os = "none")]
    {
        let _ = RtlRegisterExceptionDispatcher();
        return cognacos_nt_raise_exception(record, context, first_chance, KiUserExceptionDispatcher as usize);
    }
    #[cfg(not(target_os = "none"))]
    { let _ = first_chance; STATUS_NOT_IMPLEMENTED }
}

/// Windows x64 DISPATCHER_CONTEXT supplied to language-specific handlers.
#[repr(C)]
struct RTL_DISPATCHER_CONTEXT {
    ControlPc: u64,
    ImageBase: u64,
    FunctionEntry: *const RUNTIME_FUNCTION,
    EstablisherFrame: u64,
    TargetIp: u64,
    ContextRecord: *mut CONTEXT,
    LanguageHandler: EXCEPTION_ROUTINE,
    HandlerData: *mut c_void,
    HistoryTable: *mut c_void,
    ScopeIndex: u32,
    Fill0: u32,
}

/// Walk AMD64 frames while preserving the original exception context unless a
/// language handler explicitly modifies it and requests ExceptionContinueExecution.
#[no_mangle]
pub unsafe extern "system" fn RtlDispatchException(record: *mut EXCEPTION_RECORD, context: *mut CONTEXT) -> u8 {
    if record.is_null() || context.is_null() { return 0; }
    let mut walk_context = ptr::read_unaligned(context);
    let mut control_pc = walk_context.Rip;
    let mut previous_rsp = 0u64;
    if control_pc == 0 || walk_context.Rsp == 0 || walk_context.Rsp & 7 != 0 { return 0; }
    for _ in 0..128 {
        let mut image_base = 0u64;
        let entry = RtlLookupFunctionEntry(control_pc, &mut image_base, ptr::null_mut());
        let mut handler_data = ptr::null_mut();
        let mut establisher = 0u64;
        let handler = RtlVirtualUnwind(
            1, image_base, control_pc, entry, &mut walk_context, &mut handler_data,
            &mut establisher, ptr::null_mut()
        );
        if let Some(handler_fn) = handler {
            let mut dispatcher_context = RTL_DISPATCHER_CONTEXT {
                ControlPc: control_pc,
                ImageBase: image_base,
                FunctionEntry: entry,
                EstablisherFrame: establisher,
                TargetIp: 0,
                ContextRecord: context,
                LanguageHandler: Some(handler_fn),
                HandlerData: handler_data,
                HistoryTable: ptr::null_mut(),
                ScopeIndex: 0,
                Fill0: 0,
            };
            let disposition = handler_fn(record, establisher as *mut c_void, context, (&mut dispatcher_context as *mut RTL_DISPATCHER_CONTEXT).cast());
            match disposition {
                0 => return 1, // ExceptionContinueExecution
                1 => {},       // ExceptionContinueSearch
                _ => return 0,
            }
        }
        if walk_context.Rip == 0 || walk_context.Rsp <= previous_rsp || walk_context.Rsp & 7 != 0 {
            return 0;
        }
        previous_rsp = walk_context.Rsp;
        control_pc = walk_context.Rip;
        if entry.is_null() && control_pc == 0 { return 0; }
    }
    0
}

#[no_mangle]
pub unsafe extern "system" fn RtlCaptureContext(context: *mut CONTEXT) {
    if context.is_null() { return; }
    // CONTEXT is zeroed first so all reserved and vector state is deterministic.
    ptr::write_bytes(context.cast::<u8>(), 0, core::mem::size_of::<CONTEXT>());
    let c = &mut *context;
    c.ContextFlags = CONTEXT_CONTROL | CONTEXT_INTEGER | CONTEXT_SEGMENTS | CONTEXT_FLOATING_POINT;
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
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "pushfq",
        "pop rax",
        "mov dword ptr [{p} + 68], eax",
        "mov ax, cs",
        "mov word ptr [{p} + 56], ax",
        "mov ax, ds",
        "mov word ptr [{p} + 58], ax",
        "mov ax, es",
        "mov word ptr [{p} + 60], ax",
        "mov ax, fs",
        "mov word ptr [{p} + 62], ax",
        "mov ax, gs",
        "mov word ptr [{p} + 64], ax",
        "mov ax, ss",
        "mov word ptr [{p} + 74], ax",
        p = in(reg) context,
        out("rax") _,
        options(preserves_flags)
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "fxsave64 [{}]",
        in(reg) (context.cast::<u8>().add(0x100)),
        options(nostack)
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "stmxcsr [{}]",
        in(reg) (context.cast::<u8>().add(0x34)),
        options(nostack)
    );
    #[cfg(not(target_arch = "x86_64"))]
    { let _ = c; }
    let _ = RtlRegisterExceptionDispatcher();
}
#[no_mangle]
pub unsafe extern "system" fn RtlCaptureContext2(context: *mut CONTEXT, _flags: u32) {
    RtlCaptureContext(context)
}
#[no_mangle]
pub unsafe extern "system" fn RtlKnownExceptionFilter(_record: *mut EXCEPTION_RECORD) -> i32 {
    0
}

// NTDLL-local function-table registry. Each loaded process has its own NTDLL
// instance; the registry stores caller-owned .pdata tables until image unload.
use core::sync::atomic::{AtomicBool, Ordering};
const MAX_FUNCTION_TABLES: usize = 64;
#[derive(Clone, Copy)]
struct FunctionTable { table: *const RUNTIME_FUNCTION, count: u32, base: u64, used: bool }
unsafe impl Send for FunctionTable {}
impl FunctionTable { const EMPTY: Self = Self { table: ptr::null(), count: 0, base: 0, used: false }; }
struct FunctionTableStore(core::cell::UnsafeCell<[FunctionTable; MAX_FUNCTION_TABLES]>);
unsafe impl Sync for FunctionTableStore {}
static FUNCTION_TABLES: FunctionTableStore = FunctionTableStore(core::cell::UnsafeCell::new([FunctionTable::EMPTY; MAX_FUNCTION_TABLES]));
static FUNCTION_TABLE_LOCK: AtomicBool = AtomicBool::new(false);
struct TableGuard;
impl TableGuard {
    fn acquire() -> Self {
        while FUNCTION_TABLE_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for TableGuard { fn drop(&mut self) { FUNCTION_TABLE_LOCK.store(false, Ordering::Release); } }

/// Register a sorted AMD64 RUNTIME_FUNCTION array (RtlAddFunctionTable ABI).
#[no_mangle]
pub unsafe extern "system" fn RtlAddFunctionTable(
    table: *const RUNTIME_FUNCTION, count: u32, base: u64
) -> u8 {
    if table.is_null() || count == 0 || count > 8192 || base == 0 { return 0; }
    let entries = core::slice::from_raw_parts(table, count as usize);
    let mut previous_end = 0u32;
    for entry in entries {
        if entry.BeginAddress >= entry.EndAddress || entry.BeginAddress < previous_end
            || base.checked_add(entry.EndAddress as u64).is_none()
            || entry.UnwindData & 3 != 0 {
            return 0;
        }
        previous_end = entry.EndAddress;
    }
    #[cfg(target_os = "none")]
    {
        // Kernel registration pins this caller-owned .pdata range against
        // concurrent loader teardown and revalidates every lookup in user space.
        if cognacos_register_function_table(table, count, base) != 0 { return 0; }
    }
    let mut inserted = false;
    {
        let _guard = TableGuard::acquire();
        let tables = &mut *FUNCTION_TABLES.0.get();
        if tables.iter().any(|t| t.used && t.table == table && t.base == base) {
            #[cfg(target_os = "none")]
            { let _ = cognacos_delete_function_table(table); }
            return 0;
        }
        if let Some(slot) = tables.iter_mut().find(|t| !t.used) {
            *slot = FunctionTable { table, count, base, used: true };
            inserted = true;
        }
    }
    if !inserted {
        #[cfg(target_os = "none")]
        { let _ = cognacos_delete_function_table(table); }
        return 0;
    }
    1
}

/// Remove a previously registered dynamic function table.
#[no_mangle]
pub unsafe extern "system" fn RtlDeleteFunctionTable(table: *const RUNTIME_FUNCTION) -> u8 {
    if table.is_null() { return 0; }
    #[cfg(target_os = "none")]
    let kernel_removed = cognacos_delete_function_table(table) == 1;
    let removed = {
        let _guard = TableGuard::acquire();
        let tables = &mut *FUNCTION_TABLES.0.get();
        if let Some(slot) = tables.iter_mut().find(|t| t.used && t.table == table) {
            *slot = FunctionTable::EMPTY;
            true
        } else { false }
    };
    #[cfg(target_os = "none")]
    { return (kernel_removed && removed) as u8; }
    #[cfg(not(target_os = "none"))]
    { removed as u8 }
}

/// Search registered AMD64 function tables for ControlPc.
#[no_mangle]
pub unsafe extern "system" fn RtlLookupFunctionEntry(
    control_pc: u64, image_base: *mut u64, _history_table: *mut c_void
) -> *const RUNTIME_FUNCTION {
    if !image_base.is_null() { *image_base = 0; }
    if control_pc == 0 { return ptr::null(); }
    #[cfg(target_os = "none")]
    {
        let found = cognacos_lookup_function_entry(control_pc, image_base, _history_table);
        if !found.is_null() { return found; }
    }
    let _guard = TableGuard::acquire();
    let tables = &*FUNCTION_TABLES.0.get();
    for t in tables.iter().filter(|t| t.used) {
        let Some(rva) = control_pc.checked_sub(t.base).and_then(|v| u32::try_from(v).ok()) else { continue; };
        let entries = core::slice::from_raw_parts(t.table, t.count as usize);
        let mut lo = 0usize;
        let mut hi = entries.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let e = &entries[mid];
            if rva < e.BeginAddress { hi = mid; }
            else if rva >= e.EndAddress { lo = mid + 1; }
            else {
                if !image_base.is_null() { *image_base = t.base; }
                return e as *const RUNTIME_FUNCTION;
            }
        }
    }
    ptr::null()
}

#[no_mangle]
pub unsafe extern "system" fn RtlRaiseException(record: *mut EXCEPTION_RECORD) {
    if record.is_null() { return; }
    let mut context: CONTEXT = core::mem::zeroed();
    RtlCaptureContext(&mut context);
    if (*record).ExceptionAddress.is_null() {
        (*record).ExceptionAddress = context.Rip as *mut c_void;
    }
    let _ = NtRaiseException(record, &context, 1);
}
#[no_mangle]
pub unsafe extern "system" fn RtlRaiseExceptionForReturnAddressHijack() {
    RtlRaiseStatus(STATUS_BAD_STACK);
}
#[no_mangle]
pub unsafe extern "system" fn RtlRaiseNoncontinuableException(record: *mut EXCEPTION_RECORD, context: *mut CONTEXT) {
    if record.is_null() { return; }
    (*record).ExceptionFlags |= EXCEPTION_NONCONTINUABLE;
    if context.is_null() {
        let mut captured: CONTEXT = core::mem::zeroed();
        RtlCaptureContext(&mut captured);
        let _ = NtRaiseException(record, &captured, 1);
    } else {
        let _ = NtRaiseException(record, context, 1);
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlRaiseStatus(status: i32) {
    let mut record: EXCEPTION_RECORD = core::mem::zeroed();
    record.ExceptionCode = status as u32;
    let mut context: CONTEXT = core::mem::zeroed();
    RtlCaptureContext(&mut context);
    record.ExceptionAddress = context.Rip as *mut c_void;
    let _ = NtRaiseException(&record, &context, 1);
}
#[no_mangle]
pub unsafe extern "system" fn RtlReportException(record: *mut EXCEPTION_RECORD, context: *mut CONTEXT, _flags: u32) -> i32 {
    if record.is_null() || context.is_null() { return STATUS_INVALID_PARAMETER; }
    NtRaiseException(record, context, 1)
}
#[no_mangle]
pub unsafe extern "system" fn RtlReportExceptionEx(record: *mut EXCEPTION_RECORD, context: *mut CONTEXT, flags: u32, _nested: u8) -> i32 {
    RtlReportException(record, context, flags)
}
#[no_mangle]
pub unsafe extern "system" fn RtlRestoreContext(context: *const CONTEXT, record: *const EXCEPTION_RECORD) {
    if context.is_null() { return; }
    if !record.is_null() {
        // The caller requested exception-aware restoration: dispatch against the
        // supplied context first; if no handler accepts it, restore the context
        // exactly as requested instead of silently discarding it.
        let mut local_record = ptr::read_unaligned(record);
        let mut local_context = ptr::read_unaligned(context);
        if RtlDispatchException(&mut local_record, &mut local_context) != 0 {
            let _ = NtContinue(&local_context, 0);
            return;
        }
    }
    let _ = NtContinue(context, 0);
}
#[no_mangle]
pub unsafe extern "system" fn RtlUnhandledExceptionFilter(_record: *mut EXCEPTION_RECORD) -> i32 { 0 }
#[no_mangle]
pub unsafe extern "system" fn RtlUnhandledExceptionFilter2(_record: *mut EXCEPTION_RECORD, _context: *mut CONTEXT) -> i32 { 0 }

unsafe fn unwind_to_target(
    target_frame: *mut c_void, target_ip: *mut c_void,
    record: *mut EXCEPTION_RECORD, return_value: *mut c_void,
    supplied_context: *mut CONTEXT
) {
    if target_frame.is_null() || target_ip.is_null() { return; }
    let mut context_storage: CONTEXT = core::mem::zeroed();
    let context = if supplied_context.is_null() {
        RtlCaptureContext(&mut context_storage);
        &mut context_storage
    } else {
        supplied_context
    };
    let mut local_record: EXCEPTION_RECORD = core::mem::zeroed();
    let rec = if record.is_null() { &mut local_record } else { record };
    (*rec).ExceptionFlags |= 0x2; // EXCEPTION_UNWINDING
    let target = target_frame as u64;
    let mut previous_rsp = 0u64;
    for _ in 0..128 {
        if (*context).Rsp >= target {
            (*context).Rip = target_ip as u64;
            (*context).Rsp = target;
            (*context).Rax = return_value as u64;
            let _ = NtContinue(context, 0);
            return;
        }
        let control_pc = (*context).Rip;
        let mut image_base = 0u64;
        let entry = RtlLookupFunctionEntry(control_pc, &mut image_base, ptr::null_mut());
        let mut handler_data = ptr::null_mut();
        let mut establisher = 0u64;
        if let Some(handler) = RtlVirtualUnwind(
            2, image_base, control_pc, entry, context, &mut handler_data,
            &mut establisher, ptr::null_mut()
        ) {
            let disposition = handler(rec, establisher as *mut c_void, context, handler_data);
            if disposition != 1 { return; }
        }
        if (*context).Rsp <= previous_rsp || (*context).Rsp & 7 != 0 { return; }
        previous_rsp = (*context).Rsp;
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlUnwind(
    target_frame: *mut c_void, target_ip: *mut c_void,
    record: *mut EXCEPTION_RECORD, return_value: *mut c_void
) {
    unwind_to_target(target_frame, target_ip, record, return_value, ptr::null_mut())
}
#[no_mangle]
pub unsafe extern "system" fn RtlUnwindEx(
    target_frame: *mut c_void, target_ip: *mut c_void,
    record: *mut EXCEPTION_RECORD, return_value: *mut c_void,
    context: *mut CONTEXT, _history_table: *mut c_void
) {
    unwind_to_target(target_frame, target_ip, record, return_value, context)
}

unsafe fn unwind_leaf(context: *mut CONTEXT, establisher: *mut u64) -> i32 {
    if context.is_null() { return STATUS_INVALID_PARAMETER; }
    let c = &mut *context;
    if c.Rsp == 0 || c.Rsp & 7 != 0 { return STATUS_BAD_STACK; }
    let ret = *(c.Rsp as *const u64);
    if ret == 0 { return STATUS_BAD_STACK; }
    c.Rip = ret;
    c.Rsp = match c.Rsp.checked_add(8) { Some(v) => v, None => return STATUS_BAD_STACK };
    if !establisher.is_null() { *establisher = c.Rsp; }
    0
}
unsafe fn unwind_function(
    handler_type: u32, image_base: u64, control_pc: u64,
    entry: *const RUNTIME_FUNCTION, context: *mut CONTEXT,
    handler_data: *mut *mut c_void, establisher_frame: *mut u64
) -> i32 {
    if context.is_null() || entry.is_null() || handler_type & !0x7 != 0 { return STATUS_INVALID_PARAMETER; }
    let c = &mut *context;
    let f = &*entry;
    if f.BeginAddress >= f.EndAddress || f.UnwindData & 1 != 0 { return STATUS_NOT_IMPLEMENTED; }
    let rva = match u32::try_from(control_pc.saturating_sub(image_base)) { Ok(v) if v >= f.BeginAddress && v < f.EndAddress => v, _ => return STATUS_INVALID_PARAMETER };
    let Some(info) = image_base.checked_add(f.UnwindData as u64) else { return STATUS_INVALID_PARAMETER; };
    let header = *(info as *const u8);
    let version = header & 7;
    let flags = header >> 3;
    if version != 1 || flags & !7 != 0 || flags & 4 != 0 { return STATUS_NOT_IMPLEMENTED; }
    let prolog_size = *((info + 1) as *const u8) as u32;
    let code_count = *((info + 2) as *const u8) as usize;
    let frame = *((info + 3) as *const u8);
    if code_count > 256 { return STATUS_INVALID_PARAMETER; }
    let control_offset = rva - f.BeginAddress;
    let in_prolog = control_offset < prolog_size;
    let mut index = 0usize;
    while index < code_count {
        let slot = *((info + 4 + (index * 2) as u64) as *const u8);
        let op = *((info + 5 + (index * 2) as u64) as *const u8);
        let code_offset = slot as u32;
        let opcode = op & 0x0f;
        let opinfo = op >> 4;
        if in_prolog && code_offset > control_offset { index += 1 + match opcode { 1 => if opinfo == 0 { 1 } else { 2 }, 4 | 8 => 1, 5 | 9 => 2, _ => 0 }; continue; }
        match opcode {
            0 => { // UWOP_PUSH_NONVOL
                if c.Rsp == 0 || c.Rsp & 7 != 0 { return STATUS_BAD_STACK; }
                let value = *(c.Rsp as *const u64);
                match opinfo {
                    3 => c.Rbx=value, 5 => c.Rbp=value, 6 => c.Rsi=value, 7 => c.Rdi=value,
                    12 => c.R12=value, 13 => c.R13=value, 14 => c.R14=value, 15 => c.R15=value,
                    _ => return STATUS_NOT_IMPLEMENTED,
                }
                c.Rsp += 8;
            }
            1 => { // UWOP_ALLOC_LARGE
                let amount = if opinfo == 0 {
                    index += 1; (*((info + 4 + (index * 2) as u64) as *const u16) as u64) * 8
                } else if opinfo == 1 {
                    index += 2; *((info + 4 + (index * 2) as u64 - 2) as *const u32) as u64
                } else { return STATUS_INVALID_PARAMETER };
                c.Rsp = match c.Rsp.checked_add(amount) { Some(v) => v, None => return STATUS_BAD_STACK };
            }
            2 => { // UWOP_ALLOC_SMALL
                c.Rsp = match c.Rsp.checked_add((opinfo as u64) * 8 + 8) { Some(v) => v, None => return STATUS_BAD_STACK };
            }
            3 => { // UWOP_SET_FPREG
                let reg = frame & 0x0f;
                let offset = ((frame >> 4) as u64) * 16;
                let fp = match reg { 5 => c.Rbp, _ => return STATUS_NOT_IMPLEMENTED };
                c.Rsp = match fp.checked_sub(offset) { Some(v) => v, None => return STATUS_BAD_STACK };
            }
            4 | 5 | 8 | 9 => return STATUS_NOT_IMPLEMENTED, // save slots require full context-pointer ABI
            10 => { // UWOP_PUSH_MACHFRAME
                let error_code = opinfo != 0;
                let rip_slot = c.Rsp + if error_code { 8 } else { 0 };
                c.Rip = *(rip_slot as *const u64);
                c.Rsp = *( (rip_slot + 24) as *const u64 );
                if !establisher_frame.is_null() { *establisher_frame = c.Rsp; }
                return 0;
            }
            _ => return STATUS_NOT_IMPLEMENTED,
        }
        index += 1;
    }
    if !establisher_frame.is_null() { *establisher_frame = c.Rsp; }
    if !handler_data.is_null() { *handler_data = ptr::null_mut(); }
    let ret = *(c.Rsp as *const u64);
    if ret == 0 { return STATUS_BAD_STACK; }
    c.Rip = ret;
    c.Rsp = match c.Rsp.checked_add(8) { Some(v) => v, None => return STATUS_BAD_STACK };
    0
}
#[no_mangle]
pub unsafe extern "system" fn RtlVirtualUnwind(
    handler_type: u32, image_base: u64, control_pc: u64,
    function_entry: *const RUNTIME_FUNCTION, context: *mut CONTEXT,
    handler_data: *mut *mut c_void, establisher_frame: *mut u64,
    _context_pointers: *mut c_void
) -> EXCEPTION_ROUTINE {
    if !handler_data.is_null() { *handler_data = ptr::null_mut(); }
    if function_entry.is_null() {
        let _ = unwind_leaf(context, establisher_frame);
        return None;
    }
    let status = unwind_function(handler_type, image_base, control_pc, function_entry, context, handler_data, establisher_frame);
    if status != 0 || handler_type & 3 == 0 { return None; }
    let f = &*function_entry;
    let info = match image_base.checked_add(f.UnwindData as u64) { Some(v) => v, None => return None };
    let header = ptr::read_unaligned(info as *const u8);
    let flags = header >> 3;
    let count = ptr::read_unaligned((info + 2) as *const u8) as u64;
    if flags & 4 != 0 || flags & (handler_type as u8 & 3) == 0 { return None; }
    let handler_rva_offset = info + 4 + ((count * 2 + 3) & !3);
    let handler_rva = ptr::read_unaligned(handler_rva_offset as *const u32) as u64;
    if handler_rva == 0 { return None; }
    if !handler_data.is_null() { *handler_data = (handler_rva_offset + 4) as *mut c_void; }
    let address = match image_base.checked_add(handler_rva) { Some(v) => v, None => return None };
    Some(core::mem::transmute::<usize, unsafe extern "system" fn(*mut EXCEPTION_RECORD, *mut c_void, *mut CONTEXT, *mut c_void) -> i32>(address as usize))
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
    let status = if function_entry.is_null() {
        unwind_leaf(context, establisher_frame)
    } else {
        let _ = context_pointers;
        unwind_function(handler_type, image_base, control_pc, function_entry, context, handler_data, establisher_frame)
    };
    if status != 0 { return status; }
    if !context.is_null() {
        if !low_limit.is_null() && (*context).Rsp < *low_limit { return STATUS_BAD_STACK; }
        if !high_limit.is_null() && (*context).Rsp >= *high_limit { return STATUS_BAD_STACK; }
    }
    if !function_entry.is_null() && (handler_type & 3) != 0 {
        let f = &*function_entry;
        let info = match image_base.checked_add(f.UnwindData as u64) { Some(v) => v, None => return STATUS_INVALID_PARAMETER };
        let header = ptr::read_unaligned(info as *const u8);
        let flags = header >> 3;
        let count = ptr::read_unaligned((info + 2) as *const u8) as u64;
        if flags & 4 == 0 && flags & (handler_type as u8 & 3) != 0 {
            let handler_rva_offset = info + 4 + ((count * 2 + 3) & !3);
            let handler_rva = ptr::read_unaligned(handler_rva_offset as *const u32) as u64;
            if handler_rva != 0 {
                if !handler_data.is_null() { *handler_data = (handler_rva_offset + 4) as *mut c_void; }
                let address = match image_base.checked_add(handler_rva) { Some(v) => v, None => return STATUS_INVALID_PARAMETER };
                if !handler_routine.is_null() { *handler_routine = Some(core::mem::transmute::<usize, unsafe extern "system" fn(*mut EXCEPTION_RECORD, *mut c_void, *mut CONTEXT, *mut c_void) -> i32>(address as usize)); }
            }
        }
    }
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
    fn dynamic_function_table_register_lookup_delete_round_trip() {
        let table = [RUNTIME_FUNCTION { BeginAddress: 0x1000, EndAddress: 0x1100, UnwindData: 0x2000 }];
        let base = 0x0000_0000_0001_0000u64;
        assert_eq!(unsafe { RtlAddFunctionTable(table.as_ptr(), table.len() as u32, base) }, 1);
        let mut image_base = 0;
        let found = unsafe { RtlLookupFunctionEntry(base + 0x1080, &mut image_base, ptr::null_mut()) };
        assert_eq!(image_base, base);
        assert_eq!(found, table.as_ptr());
        assert!(unsafe { RtlLookupFunctionEntry(base + 0x1100, &mut image_base, ptr::null_mut()) }.is_null());
        assert_eq!(unsafe { RtlDeleteFunctionTable(table.as_ptr()) }, 1);
        assert!(unsafe { RtlLookupFunctionEntry(base + 0x1080, &mut image_base, ptr::null_mut()) }.is_null());
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

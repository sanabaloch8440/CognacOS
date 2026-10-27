//! Windows 11 x64 NTDLL exception and unwind exports.
//!
//! Windows x64 CONTEXT, EXCEPTION_RECORD, runtime-function unwind and native
//! exception-routing support. Dynamic tables and dispatch registration are
//! process-scoped kernel services; unsupported chained metadata and newer
//! extended XSTATE formats remain explicitly outside the current implementation.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[cfg(any(target_os = "none", target_os = "windows"))]
core::arch::global_asm!(r#"
    .text
    .p2align 4
    .global cognacos_register_function_table
cognacos_register_function_table:
    mov r10, rcx
    mov eax, 0x13e
    syscall
    ret
    .p2align 4
    .global cognacos_delete_function_table
cognacos_delete_function_table:
    mov r10, rcx
    mov eax, 0x13f
    syscall
    ret
    .p2align 4
    .global cognacos_lookup_function_entry
cognacos_lookup_function_entry:
    mov r10, rcx
    mov eax, 0x140
    syscall
    ret
    .p2align 4
    .global cognacos_nt_continue
cognacos_nt_continue:
    mov r10, rcx
    mov eax, 0x141
    syscall
    ret
    .p2align 4
    .global cognacos_nt_raise_exception
cognacos_nt_raise_exception:
    mov r10, rcx
    mov eax, 0x142
    syscall
    ret
    .p2align 4
    .global cognacos_register_exception_dispatcher
cognacos_register_exception_dispatcher:
    mov r10, rcx
    mov eax, 0x143
    syscall
    ret
"#);

#[cfg(any(target_os = "none", target_os = "windows"))]
extern "system" {
    fn cognacos_register_function_table(table: *const RUNTIME_FUNCTION, count: u32, base: u64) -> u64;
    fn cognacos_delete_function_table(table: *const RUNTIME_FUNCTION) -> u64;
    fn cognacos_lookup_function_entry(control_pc: u64, image_base: *mut u64, history_table: *mut c_void) -> *const RUNTIME_FUNCTION;
    fn cognacos_nt_continue(context: *const CONTEXT, test_alert: u8) -> i32;
    fn cognacos_nt_raise_exception(record: *const EXCEPTION_RECORD, context: *const CONTEXT, first_chance: u8, dispatcher: usize) -> i32;
    fn cognacos_register_exception_dispatcher(dispatcher: usize) -> u64;
}

/// Register the native first-chance exception entry with the kernel for this process.
/// Registration is process-scoped and is idempotently refreshed by loader entry points.
pub unsafe extern "system" fn RtlRegisterExceptionDispatcher() -> i32 {
    #[cfg(any(target_os = "none", target_os = "windows"))]
    {
        let status = unsafe { cognacos_register_exception_dispatcher(KiUserExceptionDispatcher as usize) };
        return status as u32 as i32;
    }
    #[cfg(not(any(target_os = "none", target_os = "windows")))]
    { STATUS_NOT_IMPLEMENTED }
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
    #[cfg(any(target_os = "none", target_os = "windows"))]
    { return cognacos_nt_continue(context, test_alert); }
    #[cfg(not(any(target_os = "none", target_os = "windows")))]
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
    #[cfg(any(target_os = "none", target_os = "windows"))]
    {
        let _ = RtlRegisterExceptionDispatcher();
        return cognacos_nt_raise_exception(record, context, first_chance, KiUserExceptionDispatcher as usize);
    }
    #[cfg(not(any(target_os = "none", target_os = "windows")))]
    { let _ = first_chance; STATUS_NOT_IMPLEMENTED }
}

#[repr(C)]
pub struct EXCEPTION_POINTERS {
    pub ExceptionRecord: *mut EXCEPTION_RECORD,
    pub ContextRecord: *mut CONTEXT,
}
pub type PVECTORED_EXCEPTION_HANDLER = Option<unsafe extern "system" fn(*mut EXCEPTION_POINTERS) -> i32>;
const MAX_VECTORED_EXCEPTION_HANDLERS: usize = 64;
#[derive(Clone, Copy)]
struct VectoredExceptionEntry { cookie: usize, handler: usize }
impl VectoredExceptionEntry { const EMPTY: Self = Self { cookie: 0, handler: 0 }; }
static VECTORED_EXCEPTION_LOCK: AtomicBool = AtomicBool::new(false);
static NEXT_VECTORED_EXCEPTION_COOKIE: AtomicUsize = AtomicUsize::new(1);
static mut VECTORED_EXCEPTION_HANDLERS: [VectoredExceptionEntry; MAX_VECTORED_EXCEPTION_HANDLERS] =
    [VectoredExceptionEntry::EMPTY; MAX_VECTORED_EXCEPTION_HANDLERS];
struct VectoredExceptionGuard;
impl VectoredExceptionGuard {
    fn acquire() -> Self {
        while VECTORED_EXCEPTION_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for VectoredExceptionGuard {
    fn drop(&mut self) { VECTORED_EXCEPTION_LOCK.store(false, Ordering::Release); }
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddVectoredExceptionHandler(first: u32, handler: PVECTORED_EXCEPTION_HANDLER) -> *mut c_void {
    let Some(handler) = handler else { return ptr::null_mut(); };
    let _guard = VectoredExceptionGuard::acquire();
    let entries = unsafe { &mut *ptr::addr_of_mut!(VECTORED_EXCEPTION_HANDLERS) };
    let count = entries.iter().take_while(|entry| entry.cookie != 0).count();
    if count == MAX_VECTORED_EXCEPTION_HANDLERS { return ptr::null_mut(); }
    let cookie = NEXT_VECTORED_EXCEPTION_COOKIE.fetch_add(1, Ordering::Relaxed).max(1);
    let new_entry = VectoredExceptionEntry { cookie, handler: handler as usize };
    let index = if first != 0 { 0 } else { count };
    if first != 0 {
        for i in (0..count).rev() { entries[i + 1] = entries[i]; }
    }
    entries[index] = new_entry;
    cookie as *mut c_void
}
#[no_mangle]
pub unsafe extern "system" fn RtlRemoveVectoredExceptionHandler(cookie: *mut c_void) -> u32 {
    let cookie = cookie as usize;
    if cookie == 0 { return 0; }
    let _guard = VectoredExceptionGuard::acquire();
    let entries = unsafe { &mut *ptr::addr_of_mut!(VECTORED_EXCEPTION_HANDLERS) };
    let Some(index) = entries.iter().position(|entry| entry.cookie == cookie) else { return 0; };
    for i in index..MAX_VECTORED_EXCEPTION_HANDLERS - 1 { entries[i] = entries[i + 1]; }
    entries[MAX_VECTORED_EXCEPTION_HANDLERS - 1] = VectoredExceptionEntry::EMPTY;
    1
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
pub unsafe extern "system" fn RtlDispatchException(record: *mut EXCEPTION_RECORD, context: *mut CONTEXT) -> u8 {
    if record.is_null() || context.is_null() { return 0; }
    // VEH callbacks run before frame-based language handlers. Copy the
    // bounded registry under lock, then invoke callbacks without holding it.
    let handlers = {
        let _guard = VectoredExceptionGuard::acquire();
        unsafe { *ptr::addr_of!(VECTORED_EXCEPTION_HANDLERS) }
    };
    let mut exception_pointers = EXCEPTION_POINTERS { ExceptionRecord: record, ContextRecord: context };
    for entry in handlers.iter().take_while(|entry| entry.cookie != 0) {
        let callback: unsafe extern "system" fn(*mut EXCEPTION_POINTERS) -> i32 =
            core::mem::transmute(entry.handler);
        if unsafe { callback(&mut exception_pointers) } == -1 { return 1; }
    }
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
        "mov word ptr [{p} + 66], ax",
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
    #[cfg(any(target_os = "none", target_os = "windows"))]
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
            #[cfg(any(target_os = "none", target_os = "windows"))]
            { let _ = cognacos_delete_function_table(table); }
            return 0;
        }
        if let Some(slot) = tables.iter_mut().find(|t| !t.used) {
            *slot = FunctionTable { table, count, base, used: true };
            inserted = true;
        }
    }
    if !inserted {
        #[cfg(any(target_os = "none", target_os = "windows"))]
        { let _ = cognacos_delete_function_table(table); }
        return 0;
    }
    1
}

/// Remove a previously registered dynamic function table.
#[no_mangle]
pub unsafe extern "system" fn RtlDeleteFunctionTable(table: *const RUNTIME_FUNCTION) -> u8 {
    if table.is_null() { return 0; }
    #[cfg(any(target_os = "none", target_os = "windows"))]
    let kernel_removed = cognacos_delete_function_table(table) == 1;
    let removed = {
        let _guard = TableGuard::acquire();
        let tables = &mut *FUNCTION_TABLES.0.get();
        if let Some(slot) = tables.iter_mut().find(|t| t.used && t.table == table) {
            *slot = FunctionTable::EMPTY;
            true
        } else { false }
    };
    #[cfg(any(target_os = "none", target_os = "windows"))]
    { return (kernel_removed && removed) as u8; }
    #[cfg(not(any(target_os = "none", target_os = "windows")))]
    { removed as u8 }
}

/// Search registered AMD64 function tables for ControlPc.
#[no_mangle]
pub unsafe extern "system" fn RtlLookupFunctionEntry(
    control_pc: u64, image_base: *mut u64, _history_table: *mut c_void
) -> *const RUNTIME_FUNCTION {
    if !image_base.is_null() { *image_base = 0; }
    if control_pc == 0 { return ptr::null(); }
    #[cfg(any(target_os = "none", target_os = "windows"))]
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
unsafe fn restore_nonvolatile_register(context: &mut CONTEXT, register: u8, value: u64) -> bool {
    match register {
        3 => context.Rbx = value, 5 => context.Rbp = value,
        6 => context.Rsi = value, 7 => context.Rdi = value,
        12 => context.R12 = value, 13 => context.R13 = value,
        14 => context.R14 = value, 15 => context.R15 = value,
        _ => return false,
    }
    true
}

unsafe fn restore_xmm_register(context: &mut CONTEXT, register: u8, address: u64) -> bool {
    if register > 15 { return false; }
    let Some(source) = usize::try_from(address).ok() else { return false; };
    let target = (context as *mut CONTEXT).cast::<u8>().wrapping_add(0x1a0 + register as usize * 16);
    let lo = ptr::read_unaligned(source as *const u64);
    let hi = ptr::read_unaligned((source + 8) as *const u64);
    ptr::write_unaligned(target.cast::<u64>(), lo);
    ptr::write_unaligned(target.add(8).cast::<u64>(), hi);
    true
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
            4 => { // UWOP_SAVE_NONVOL: scaled 16-bit stack offset
                index += 1;
                let offset = ptr::read_unaligned((info + 4 + (index * 2) as u64) as *const u16) as u64 * 8;
                let Some(address) = c.Rsp.checked_add(offset) else { return STATUS_BAD_STACK; };
                let value = ptr::read_unaligned(address as *const u64);
                if !restore_nonvolatile_register(c, opinfo, value) { return STATUS_INVALID_PARAMETER; }
            }
            5 => { // UWOP_SAVE_NONVOL_FAR: unscaled 32-bit stack offset
                index += 2;
                let offset = ptr::read_unaligned((info + 4 + (index * 2) as u64 - 2) as *const u32) as u64;
                let Some(address) = c.Rsp.checked_add(offset) else { return STATUS_BAD_STACK; };
                let value = ptr::read_unaligned(address as *const u64);
                if !restore_nonvolatile_register(c, opinfo, value) { return STATUS_INVALID_PARAMETER; }
            }
            8 => { // UWOP_SAVE_XMM128: scaled 16-bit stack offset
                index += 1;
                let offset = ptr::read_unaligned((info + 4 + (index * 2) as u64) as *const u16) as u64 * 16;
                let Some(address) = c.Rsp.checked_add(offset) else { return STATUS_BAD_STACK; };
                if !restore_xmm_register(c, opinfo, address) { return STATUS_INVALID_PARAMETER; }
            }
            9 => { // UWOP_SAVE_XMM128_FAR: unscaled 32-bit stack offset
                index += 2;
                let offset = ptr::read_unaligned((info + 4 + (index * 2) as u64 - 2) as *const u32) as u64;
                let Some(address) = c.Rsp.checked_add(offset) else { return STATUS_BAD_STACK; };
                if !restore_xmm_register(c, opinfo, address) { return STATUS_INVALID_PARAMETER; }
            }
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
        assert_eq!(core::mem::offset_of!(CONTEXT, SegSs), 66);
        assert_eq!(core::mem::offset_of!(CONTEXT, EFlags), 68);
        assert_eq!(core::mem::offset_of!(CONTEXT, Rax), 120);
        assert_eq!(core::mem::offset_of!(CONTEXT, Rip), 248);
        assert_eq!(core::mem::offset_of!(CONTEXT, opaque), 256);
        assert_eq!(core::mem::size_of::<EXCEPTION_RECORD>(), 0x98);
        assert_eq!(core::mem::size_of::<RUNTIME_FUNCTION>(), 12);
    }

    #[test]
    fn capture_context_contains_restorable_user_segments() {
        let mut context: CONTEXT = unsafe { core::mem::zeroed() };
        unsafe { RtlCaptureContext(&mut context); }
        assert_ne!(context.ContextFlags & CONTEXT_CONTROL, 0);
        assert_ne!(context.ContextFlags & CONTEXT_FLOATING_POINT, 0);
        assert_eq!(context.SegCs & 3, 3);
        assert_eq!(context.SegSs & 3, 3);
        assert_ne!(context.Rip, 0);
        assert_ne!(context.Rsp, 0);
    }

    unsafe extern "system" fn continue_search_handler(_pointers: *mut EXCEPTION_POINTERS) -> i32 { 0 }

    #[test]
    fn vectored_exception_handler_registration_is_removable() {
        let cookie = unsafe { RtlAddVectoredExceptionHandler(1, Some(continue_search_handler)) };
        assert!(!cookie.is_null());
        assert_eq!(unsafe { RtlRemoveVectoredExceptionHandler(cookie) }, 1);
        assert_eq!(unsafe { RtlRemoveVectoredExceptionHandler(cookie) }, 0);
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


// Kernel-to-user callback dispatcher. This entry must return through the
// NtCallbackReturn protocol, not as an ordinary Rust function. Until the
// callback table and return syscall are wired, fail-fast rather than returning
// to the kernel with an invalid callback frame.
#[no_mangle]
pub unsafe extern "system" fn KiUserCallbackDispatcher(
    _callback_index: u32,
    _argument: *mut c_void,
    _argument_length: u32,
) -> ! {
    #[cfg(target_os = "windows")]
    unsafe {
        core::arch::asm!("int 0x29", in("rcx") 7usize, options(noreturn, nostack));
    }
    #[cfg(not(target_os = "windows"))]
    loop { core::hint::spin_loop(); }
}


// Consolidated XSTATE, CONTEXT_EX, vectored-continue, and unhandled-filter
// implementation retained from the former root exception module. The outer
// exception module owns native dispatch, CONTEXT restoration, and AMD64 unwind.
mod xstate_context_compat {
//! Source-first Rust port of the portable parts of Wine ntdll/exception.c.
//!
//! This file implements vectored exception/continue handler registration and
//! the unhandled-exception filter plus CONTEXT_EX/XSTATE sizing, initialization,
//! feature lookup, masks, and context copying. Full SEH dispatch and hardware
//! exception/unwind integration remain separate kernel work.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

type ULONG = u32;
type LONG = i32;
type ULONG_PTR = usize;
type ULONG64 = u64;
type PVOID = *mut c_void;
type NTSTATUS = i32;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const EXCEPTION_CONTINUE_EXECUTION: LONG = -1;
const EXCEPTION_CONTINUE_SEARCH: LONG = 0;
const MAX_VECTORED_HANDLERS: usize = 128;

#[repr(C)]
pub struct EXCEPTION_RECORD {
    pub ExceptionCode: ULONG,
    pub ExceptionFlags: ULONG,
    pub ExceptionRecord: *mut EXCEPTION_RECORD,
    pub ExceptionAddress: PVOID,
    pub NumberParameters: ULONG,
    pub ExceptionInformation: [ULONG_PTR; 15],
}

#[repr(C)]
pub struct EXCEPTION_POINTERS {
    pub ExceptionRecord: *mut EXCEPTION_RECORD,
    pub ContextRecord: PVOID,
}

pub type PVECTORED_EXCEPTION_HANDLER = Option<unsafe extern "system" fn(*mut EXCEPTION_POINTERS) -> LONG>;
pub type PRTL_EXCEPTION_FILTER = Option<unsafe extern "system" fn(*mut EXCEPTION_POINTERS) -> LONG>;

#[derive(Clone, Copy)]
struct HandlerSlot {
    function: PVECTORED_EXCEPTION_HANDLER,
    first: bool,
    sequence: u64,
    kind: u8, // 1 = exception, 2 = continue
}

impl HandlerSlot {
    const EMPTY: Self = Self { function: None, first: false, sequence: 0, kind: 0 };
}

static LOCK: AtomicBool = AtomicBool::new(false);
static mut HANDLERS: [HandlerSlot; MAX_VECTORED_HANDLERS] = [HandlerSlot::EMPTY; MAX_VECTORED_HANDLERS];
static mut NEXT_SEQUENCE: u64 = 1;
static mut UNHANDLED_FILTER: PRTL_EXCEPTION_FILTER = None;

struct Guard;
impl Guard {
    fn acquire() -> Self {
        while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) { LOCK.store(false, Ordering::Release); }
}

unsafe fn add_handler(first: ULONG, function: PVECTORED_EXCEPTION_HANDLER, kind: u8) -> PVOID {
    let Some(_) = function else { return core::ptr::null_mut(); };
    let _guard = Guard::acquire();
    let slots = core::ptr::addr_of_mut!(HANDLERS).cast::<HandlerSlot>();
    for index in 0..MAX_VECTORED_HANDLERS {
        let slot = &mut *slots.add(index);
        if slot.kind == 0 {
            let sequence = NEXT_SEQUENCE;
            NEXT_SEQUENCE = NEXT_SEQUENCE.wrapping_add(1).max(1);
            *slot = HandlerSlot { function, first: first != 0, sequence, kind };
            // The opaque handle is the stable slot address, never the callback.
            return slot as *mut HandlerSlot as PVOID;
        }
    }
    core::ptr::null_mut()
}

unsafe fn remove_handler(handle: PVOID, kind: u8) -> ULONG {
    if handle.is_null() { return 0; }
    let _guard = Guard::acquire();
    let slots = core::ptr::addr_of_mut!(HANDLERS).cast::<HandlerSlot>();
    for index in 0..MAX_VECTORED_HANDLERS {
        let slot = &mut *slots.add(index);
        if (slot as *mut HandlerSlot as PVOID) == handle && slot.kind == kind && slot.kind != 0 {
            *slot = HandlerSlot::EMPTY;
            return 1;
        }
    }
    0
}

unsafe fn snapshot_handlers(kind: u8) -> [HandlerSlot; MAX_VECTORED_HANDLERS] {
    let _guard = Guard::acquire();
    let slots = core::ptr::addr_of!(HANDLERS).cast::<HandlerSlot>();
    let mut result = [HandlerSlot::EMPTY; MAX_VECTORED_HANDLERS];
    let mut count = 0usize;
    for index in 0..MAX_VECTORED_HANDLERS {
        let slot = *slots.add(index);
        if slot.kind == kind && slot.function.is_some() {
            result[count] = slot;
            count += 1;
        }
    }
    // Wine ordering: first handlers are newest-first; ordinary handlers are oldest-first.
    for i in 1..count {
        let current = result[i];
        let mut j = i;
        while j > 0 {
            let previous = result[j - 1];
            let current_precedes = if current.first != previous.first {
                current.first
            } else if current.first {
                current.sequence > previous.sequence
            } else {
                current.sequence < previous.sequence
            };
            if !current_precedes { break; }
            result[j] = previous;
            j -= 1;
        }
        result[j] = current;
    }
    result
}

unsafe fn call_handlers(pointers: *mut EXCEPTION_POINTERS, kind: u8) -> LONG {
    let snapshot = snapshot_handlers(kind);
    for slot in snapshot.iter() {
        if let Some(function) = slot.function {
            let result = function(pointers);
            if result == EXCEPTION_CONTINUE_EXECUTION { return result; }
        }
    }
    EXCEPTION_CONTINUE_SEARCH
}

pub unsafe extern "system" fn RtlAddVectoredExceptionHandler(
    first: ULONG, function: PVECTORED_EXCEPTION_HANDLER,
) -> PVOID {
    add_handler(first, function, 1)
}

pub unsafe extern "system" fn RtlRemoveVectoredExceptionHandler(handle: PVOID) -> ULONG {
    remove_handler(handle, 1)
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddVectoredContinueHandler(
    first: ULONG, function: PVECTORED_EXCEPTION_HANDLER,
) -> PVOID {
    add_handler(first, function, 2)
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemoveVectoredContinueHandler(handle: PVOID) -> ULONG {
    remove_handler(handle, 2)
}

/// CognacOS helper for the dispatcher to run registered exception callbacks.
/// It does not itself resume a saved machine context.
pub unsafe extern "system" fn CognacCallVectoredExceptionHandlers(
    record: *mut EXCEPTION_RECORD, context: PVOID,
) -> LONG {
    if record.is_null() { return EXCEPTION_CONTINUE_SEARCH; }
    let mut pointers = EXCEPTION_POINTERS { ExceptionRecord: record, ContextRecord: context };
    call_handlers(&mut pointers, 1)
}

/// CognacOS helper for the dispatcher to run registered continuation callbacks.
pub unsafe extern "system" fn CognacCallVectoredContinueHandlers(
    record: *mut EXCEPTION_RECORD, context: PVOID,
) -> LONG {
    if record.is_null() { return EXCEPTION_CONTINUE_SEARCH; }
    let mut pointers = EXCEPTION_POINTERS { ExceptionRecord: record, ContextRecord: context };
    call_handlers(&mut pointers, 2)
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetUnhandledExceptionFilter(filter: PRTL_EXCEPTION_FILTER) {
    let _guard = Guard::acquire();
    UNHANDLED_FILTER = filter;
}

pub unsafe extern "system" fn CognacCallUnhandledExceptionFilter(
    pointers: *mut EXCEPTION_POINTERS,
) -> LONG {
    if pointers.is_null() { return EXCEPTION_CONTINUE_SEARCH; }
    let filter = {
        let _guard = Guard::acquire();
        UNHANDLED_FILTER
    };
    match filter {
        Some(callback) => callback(pointers),
        None => EXCEPTION_CONTINUE_SEARCH,
    }
}

/// XSTATE configuration copied from the kernel's boot-time CPU policy.
/// NTDLL must receive this from the native process bootstrap/shared-data
/// initializer before exposing CONTEXT_XSTATE to applications.
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicU32;

static NTDLL_XSTATE_ENABLED: AtomicU64 = AtomicU64::new(0);
static NTDLL_XSTATE_SUPPORTED: AtomicU64 = AtomicU64::new(0);
static NTDLL_XSTATE_AREA_SIZE: AtomicU32 = AtomicU32::new(0);
static NTDLL_XSTATE_COMPACTION_ENABLED: AtomicBool = AtomicBool::new(false);
static NTDLL_XSTATE_ALIGNED_MASK: AtomicU64 = AtomicU64::new(0);
static NTDLL_XSTATE_READY: AtomicBool = AtomicBool::new(false);
static NTDLL_XSTATE_OFFSETS: [AtomicU32; 64] =
    [const { AtomicU32::new(0) }; 64];
static NTDLL_XSTATE_SIZES: [AtomicU32; 64] =
    [const { AtomicU32::new(0) }; 64];
const KUSER_SHARED_DATA_X64: usize = 0x0000_0000_7ffe_0000;
const KUSER_XSTATE_OFFSET: usize = 0x3d8;
#[repr(C)]
#[derive(Clone, Copy)]
struct WindowsXStateFeature { offset: u32, size: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct WindowsXStateConfiguration {
    enabled_features: u64,
    enabled_volatile_features: u64,
    size: u32,
    control_flags: u32,
    features: [WindowsXStateFeature; 64],
    enabled_supervisor_features: u64,
    aligned_features: u64,
    all_feature_size: u32,
    all_features: [u32; 64],
    enabled_user_visible_supervisor_features: u64,
    extended_feature_disable_features: u64,
    all_non_large_feature_size: u32,
    max_sve_vector_length: u16,
    spare1: u16,
}
const _: [(); 0x348] = [(); core::mem::size_of::<WindowsXStateConfiguration>()];
const _: [(); 0x18] = [(); core::mem::offset_of!(WindowsXStateConfiguration, features)];
const _: [(); 0x330] = [(); core::mem::offset_of!(WindowsXStateConfiguration, enabled_user_visible_supervisor_features)];
/// Lazy NTDLL bootstrap from the read-only page installed by
/// process::initialize. This keeps feature reporting tied to the kernel's
/// actual boot policy instead of trusting local CPUID alone.
fn ensure_ntdll_xstate_config() -> bool {
    if NTDLL_XSTATE_READY.load(Ordering::Acquire) { return true; }
    // KUSER_SHARED_DATA is mapped read-only at 0x7ffe0000. XState uses the
    // Windows 11 24H2 x64 offset 0x3d8 and its exact 0x348-byte layout.
    let shared = (KUSER_SHARED_DATA_X64 + KUSER_XSTATE_OFFSET) as *const WindowsXStateConfiguration;
    let config = unsafe { shared.read_unaligned() };
    let enabled = config.enabled_features;
    if enabled & 3 != 3 || config.size < 576 { return false; }
    NTDLL_XSTATE_ALIGNED_MASK.store(config.aligned_features, Ordering::Release);
    let mut offsets = [0u32; 64];
    let mut sizes = [0u32; 64];
    for i in 0..64 {
        offsets[i] = config.features[i].offset;
        sizes[i] = config.features[i].size;
    }
    unsafe {
        CognacNtdllPublishXStateConfiguration(
            enabled,
            enabled,
            config.size,
            (config.control_flags >> 1) & 1,
            offsets.as_ptr(),
            sizes.as_ptr(),
        ) == STATUS_SUCCESS
    }
}


/// Publish the configuration computed by ntoskrnl to NTDLL during native
/// process initialization. Offsets are CPUID.(0xD, feature).EBX values in the
/// standard (non-compacted) XSAVE layout; sizes are CPUID leaf 0xD subleaf EAX.
pub unsafe extern "system" fn CognacNtdllPublishXStateConfiguration(
    supported: ULONG64,
    enabled: ULONG64,
    area_size: ULONG,
    compaction_enabled: ULONG,
    offsets: *const ULONG,
    sizes: *const ULONG,
) -> NTSTATUS {
    if enabled == 0 || enabled & !supported != 0 || area_size < 576
        || offsets.is_null() || sizes.is_null()
    {
        return STATUS_INVALID_PARAMETER;
    }
    // The legacy x87/SSE components are mandatory in the standard format.
    if enabled & 0x3 != 0x3 { return STATUS_INVALID_PARAMETER; }
    NTDLL_XSTATE_READY.store(false, Ordering::Release);
    for i in 0..64 {
        NTDLL_XSTATE_OFFSETS[i].store(unsafe { *offsets.add(i) }, Ordering::Relaxed);
        NTDLL_XSTATE_SIZES[i].store(unsafe { *sizes.add(i) }, Ordering::Relaxed);
    }
    NTDLL_XSTATE_SUPPORTED.store(supported, Ordering::Relaxed);
    NTDLL_XSTATE_ENABLED.store(enabled, Ordering::Relaxed);
    NTDLL_XSTATE_AREA_SIZE.store(area_size, Ordering::Relaxed);
    NTDLL_XSTATE_COMPACTION_ENABLED.store(compaction_enabled != 0, Ordering::Relaxed);
    NTDLL_XSTATE_READY.store(true, Ordering::Release);
    STATUS_SUCCESS
}

/// Return enabled extended processor feature bits, limited to the caller's mask.
#[no_mangle]
pub extern "system" fn RtlGetEnabledExtendedFeatures(mask: ULONG64) -> ULONG64 {
    if !ensure_ntdll_xstate_config() { return 0; }
    NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & mask
}

/// Return the size of the requested CONTEXT allocation. XSTATE is reported only
/// after the kernel configuration has been published; this prevents NTDLL from
/// advertising AVX/other state that the scheduler cannot preserve.
#[no_mangle]
pub unsafe extern "system" fn RtlGetExtendedContextLength(
    context_flags: ULONG, length: *mut ULONG,
) -> NTSTATUS {
    RtlGetExtendedContextLength2(context_flags, length, ULONG64::MAX)
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RTL_CONTEXT_CHUNK { pub Offset: i32, pub Length: ULONG }
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CONTEXT_EX { pub All: RTL_CONTEXT_CHUNK, pub Legacy: RTL_CONTEXT_CHUNK, pub XState: RTL_CONTEXT_CHUNK }

const CONTEXT_AMD64: ULONG = 0x0010_0000;
const CONTEXT_I386: ULONG = 0x0001_0000;
const CONTEXT_XSTATE: ULONG = 0x40;
const CONTEXT_EX_SIZE: usize = 24;
const CONTEXT_EX_ALIGNED_SIZE: usize = 32;
const AMD64_CONTEXT_SIZE: usize = 0x4d0;
const I386_CONTEXT_SIZE: usize = 0x2cc;
const STATUS_NOT_SUPPORTED: NTSTATUS = 0xC000_00BBu32 as i32;
const STATUS_BUFFER_OVERFLOW: NTSTATUS = 0x8000_0005u32 as i32;

fn context_parameters(flags: ULONG) -> Option<(usize, usize)> {
    let arch = flags & 0x0011_0000;
    let allowed = match arch {
        CONTEXT_AMD64 => 0xd800_0000 | CONTEXT_AMD64 | 0x5f,
        CONTEXT_I386 => 0xd800_0000 | CONTEXT_I386 | 0x7f,
        _ => return None,
    };
    if flags & !allowed != 0 { return None; }
    Some(if arch == CONTEXT_AMD64 { (AMD64_CONTEXT_SIZE, 0x30) } else { (I386_CONTEXT_SIZE, 0) })
}
fn compacted_feature_offset(mask: ULONG64, feature: ULONG) -> Option<usize> {
    let mut offset = 64usize;
    for id in 2..=feature {
        if mask & (1u64 << id) == 0 { continue; }
        let size = NTDLL_XSTATE_SIZES[id as usize].load(Ordering::Acquire) as usize;
        if size == 0 { return None; }
        if NTDLL_XSTATE_ALIGNED_MASK.load(Ordering::Acquire) & (1u64 << id) != 0 {
            offset = offset.checked_add(63)? & !63;
        }
        if id == feature { return Some(offset); }
        offset = offset.checked_add(size)?;
    }
    None
}
fn ntdll_xstate_payload_size(mask: ULONG64) -> Option<usize> {
    if !ensure_ntdll_xstate_config() { return None; }
    let selected = mask & NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & !3u64;
    if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
        let mut end = 64usize;
        for feature in 2..64 {
            if selected & (1u64 << feature) == 0 { continue; }
            let size = NTDLL_XSTATE_SIZES[feature].load(Ordering::Acquire) as usize;
            if size == 0 { return None; }
            if NTDLL_XSTATE_ALIGNED_MASK.load(Ordering::Acquire) & (1u64 << feature) != 0 {
                end = end.checked_add(63)? & !63;
            }
            end = end.checked_add(size)?;
        }
        return Some(end);
    }
    let mut end = 64usize;
    for feature in 2..64 {
        if selected & (1u64 << feature) == 0 { continue; }
        let offset = NTDLL_XSTATE_OFFSETS[feature].load(Ordering::Acquire) as usize;
        let size = NTDLL_XSTATE_SIZES[feature].load(Ordering::Acquire) as usize;
        if size == 0 || offset < 512 { return None; }
        end = end.max(offset.checked_add(size)?.checked_sub(512)?);
    }
    Some(end)
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetExtendedContextLength2(flags: ULONG, length: *mut ULONG, mask: ULONG64) -> NTSTATUS {
    if length.is_null() { return STATUS_INVALID_PARAMETER; }
    *length = 0;
    let Some((context_size, _)) = context_parameters(flags) else { return STATUS_INVALID_PARAMETER; };
    let extra = if flags & CONTEXT_XSTATE == 0 {
        CONTEXT_EX_SIZE + 15
    } else {
        if !ensure_ntdll_xstate_config() { return STATUS_NOT_SUPPORTED; }
        let Some(payload) = ntdll_xstate_payload_size(mask) else { return STATUS_INVALID_PARAMETER; };
        CONTEXT_EX_ALIGNED_SIZE + 63 + payload
    };
    match context_size.checked_add(extra) {
        Some(n) if n <= ULONG::MAX as usize => { *length = n as ULONG; STATUS_SUCCESS }
        _ => STATUS_INVALID_PARAMETER,
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeExtendedContext2(context: PVOID, flags: ULONG, out: *mut *mut CONTEXT_EX, mask: ULONG64) -> NTSTATUS {
    if context.is_null() || out.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some((size, flags_offset)) = context_parameters(flags) else { return STATUS_INVALID_PARAMETER; };
    if flags & CONTEXT_XSTATE != 0 && !ensure_ntdll_xstate_config() { return STATUS_NOT_SUPPORTED; }
    let aligned = match (context as usize).checked_add(15) { Some(v) => v & !15, None => return STATUS_INVALID_PARAMETER };
    let base = aligned as *mut u8;
    unsafe { base.add(flags_offset).cast::<ULONG>().write_unaligned(flags); }
    let ex_ptr = unsafe { base.add(size).cast::<CONTEXT_EX>() };
    let mut ex = CONTEXT_EX::default();
    ex.All.Offset = -(size as i32);
    ex.Legacy.Offset = -(size as i32);
    ex.Legacy.Length = size as ULONG;
    if flags & CONTEXT_XSTATE == 0 {
        ex.XState.Offset = CONTEXT_EX_ALIGNED_SIZE as i32;
        ex.All.Length = (size + CONTEXT_EX_SIZE) as ULONG;
        unsafe { ex_ptr.write_unaligned(ex); *out = ex_ptr; }
        return STATUS_SUCCESS;
    }
    let Some(payload) = ntdll_xstate_payload_size(mask) else { return STATUS_INVALID_PARAMETER; };
    let ex_addr = ex_ptr as usize;
    let xstate_addr = match ex_addr.checked_add(CONTEXT_EX_ALIGNED_SIZE + 63) { Some(v) => v & !63, None => return STATUS_INVALID_PARAMETER };
    let offset = xstate_addr - ex_addr;
    let all_len = match size.checked_add(offset).and_then(|v| v.checked_add(payload)) {
        Some(v) if v <= ULONG::MAX as usize => v, _ => return STATUS_INVALID_PARAMETER,
    };
    ex.XState.Offset = offset as i32;
    ex.XState.Length = payload as ULONG;
    ex.All.Length = all_len as ULONG;
    unsafe {
        core::ptr::write_bytes(xstate_addr as *mut u8, 0, payload);
        if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
            let selected = mask & NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & !3u64;
            let compaction_mask = (1u64 << 63) | selected;
            (xstate_addr as *mut u8).add(8).cast::<ULONG64>().write_unaligned(compaction_mask);
        }
        ex_ptr.write_unaligned(ex);
        *out = ex_ptr;
    }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlInitializeExtendedContext(context: PVOID, flags: ULONG, out: *mut *mut CONTEXT_EX) -> NTSTATUS {
    RtlInitializeExtendedContext2(context, flags, out, ULONG64::MAX)
}
#[no_mangle]
pub unsafe extern "system" fn RtlLocateLegacyContext(ex: *mut CONTEXT_EX, length: *mut ULONG) -> PVOID {
    if ex.is_null() { return core::ptr::null_mut(); }
    let chunk = unsafe { core::ptr::addr_of!((*ex).Legacy).read_unaligned() };
    if !length.is_null() { unsafe { length.write_unaligned(chunk.Length); } }
    unsafe { ex.cast::<u8>().offset(chunk.Offset as isize).cast() }
}
#[no_mangle]
pub unsafe extern "system" fn RtlLocateExtendedFeature(ex: *mut CONTEXT_EX, feature: ULONG, length: *mut ULONG) -> PVOID {
    if !ensure_ntdll_xstate_config() { return core::ptr::null_mut(); }
    let config = (KUSER_SHARED_DATA_X64 + KUSER_XSTATE_OFFSET) as PVOID;
    RtlLocateExtendedFeature2(ex, feature, config, length)
}

#[no_mangle]
pub unsafe extern "system" fn RtlLocateExtendedFeature2(ex: *mut CONTEXT_EX, feature: ULONG, config: PVOID, length: *mut ULONG) -> PVOID {
    if !length.is_null() { unsafe { length.write_unaligned(0); } }
    if ex.is_null() || config.is_null() || !(2..64).contains(&feature) { return core::ptr::null_mut(); }
    // The WDK API accepts a caller-supplied XSTATE_CONFIGURATION. Do not
    // require pointer identity with KUSER_SHARED_DATA; use the provided layout.
    let cfg = unsafe { (config as *const WindowsXStateConfiguration).read_unaligned() };
    let feature_bit = 1u64 << feature;
    let component = cfg.features[feature as usize];
    if cfg.enabled_features & feature_bit == 0 || component.size == 0 {
        return core::ptr::null_mut();
    }
    if !length.is_null() { unsafe { length.write_unaligned(component.size); } }
    let chunk = unsafe { core::ptr::addr_of!((*ex).XState).read_unaligned() };
    if chunk.Offset < 0 || chunk.Length < 64 { return core::ptr::null_mut(); }
    let xstate = unsafe { ex.cast::<u8>().add(chunk.Offset as usize) };
    let offset = if cfg.control_flags & 2 != 0 {
        let compacted = unsafe { xstate.add(8).cast::<ULONG64>().read_unaligned() };
        if compacted & (1u64 << 63) == 0 || compacted & feature_bit == 0 {
            return core::ptr::null_mut();
        }
        let compacted_features = compacted & !(1u64 << 63);
        let mut off = 64usize;
        for id in 2..feature {
            let bit = 1u64 << id;
            if compacted_features & bit == 0 { continue; }
            let size = cfg.features[id as usize].size as usize;
            if size == 0 { return core::ptr::null_mut(); }
            off = match off.checked_add(size) { Some(v) => v, None => return core::ptr::null_mut() };
            if cfg.aligned_features & (1u64 << (id + 1)) != 0 {
                off = match off.checked_add(63) { Some(v) => v & !63, None => return core::ptr::null_mut() };
            }
        }
        off
    } else {
        if cfg.enabled_features & feature_bit == 0 || component.offset < 512 {
            return core::ptr::null_mut();
        }
        (component.offset - 512) as usize
    };
    let size = component.size as usize;
    if offset.checked_add(size).map_or(true, |end| end > chunk.Length as usize) {
        return core::ptr::null_mut();
    }
    unsafe { xstate.add(offset).cast() }
}
#[no_mangle]
pub unsafe extern "system" fn RtlSetExtendedFeaturesMask(ex: *mut CONTEXT_EX, mask: ULONG64) {
    if ex.is_null() { return; }
    let chunk = unsafe { core::ptr::addr_of!((*ex).XState).read_unaligned() };
    if chunk.Offset < 0 || chunk.Length < 8 { return; }
    let mut enabled = RtlGetEnabledExtendedFeatures(mask) & !3u64;
    let xstate = unsafe { ex.cast::<u8>().add(chunk.Offset as usize) };
    if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
        let compaction_mask = unsafe { xstate.add(8).cast::<ULONG64>().read_unaligned() };
        enabled &= compaction_mask & !(1u64 << 63);
    }
    unsafe { xstate.cast::<ULONG64>().write_unaligned(enabled); }
}
#[no_mangle]
pub unsafe extern "system" fn RtlGetExtendedFeaturesMask(ex: *mut CONTEXT_EX) -> ULONG64 {
    if ex.is_null() { return 0; }
    let chunk = unsafe { core::ptr::addr_of!((*ex).XState).read_unaligned() };
    if chunk.Offset < 0 || chunk.Length < 8 { return 0; }
    unsafe { ex.cast::<u8>().add(chunk.Offset as usize).cast::<ULONG64>().read_unaligned() & !3u64 }
}
fn copy_amd64_context_ranges(dst: *mut u8, src: *const u8, flags: ULONG) {
    // Windows x64 CONTEXT field boundaries from the native context copy map.
    const RANGES: [(usize, ULONG); 12] = [
        (0x38, 0x1), (0x3a, 0x4), (0x42, 0x1), (0x48, 0x10),
        (0x78, 0x2), (0x98, 0x1), (0xa0, 0x2), (0xf8, 0x1),
        (0x100, 0x8), (0x2a0, 0), (0x4b0, 0x10), (0x4d0, 0),
    ];
    let mut start = 0usize;
    for (boundary, group) in RANGES {
        if group & flags != 0 {
            if start == 0 { start = boundary; }
        } else if start != 0 {
            unsafe { core::ptr::copy(src.add(start), dst.add(start), boundary - start); }
            start = 0;
        }
    }
}

fn copy_context_ranges(dst: *mut u8, src: *const u8, flags: ULONG, arch: ULONG) {
    const AMD64: [(usize, ULONG); 12] = [
        (0x38, 0x1), (0x3a, 0x4), (0x42, 0x1), (0x48, 0x10),
        (0x78, 0x2), (0x98, 0x1), (0xa0, 0x2), (0xf8, 0x1),
        (0x100, 0x8), (0x2a0, 0), (0x4b0, 0x10), (0x4d0, 0),
    ];
    const I386: [(usize, ULONG); 8] = [
        (0x04, 0x10), (0x1c, 0x08), (0x8c, 0x04), (0x9c, 0x02),
        (0xb4, 0x01), (0xcc, 0x20), (0x1ec, 0), (0x2cc, 0),
    ];
    let flags_offset = if arch == CONTEXT_AMD64 { 0x30 } else { 0 };
    unsafe {
        let field = dst.add(flags_offset).cast::<ULONG>();
        field.write_unaligned(field.read_unaligned() | flags);
    }
    let mut start = 0usize;
    if arch == CONTEXT_AMD64 {
        for (boundary, group) in AMD64 {
            if group & flags != 0 {
                if start == 0 { start = boundary; }
            } else if start != 0 {
                unsafe { core::ptr::copy(src.add(start), dst.add(start), boundary - start); }
                start = 0;
            }
        }
    } else {
        for (boundary, group) in I386 {
            if group & flags != 0 {
                if start == 0 { start = boundary; }
            } else if start != 0 {
                unsafe { core::ptr::copy(src.add(start), dst.add(start), boundary - start); }
                start = 0;
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyExtendedContext(dst: *mut CONTEXT_EX, flags: ULONG, src: *const CONTEXT_EX) -> NTSTATUS {
    let Some((_, _)) = context_parameters(flags) else { return STATUS_INVALID_PARAMETER; };
    if dst.is_null() || src.is_null() { return STATUS_INVALID_PARAMETER; }
    if flags & CONTEXT_XSTATE != 0 && !ensure_ntdll_xstate_config() { return STATUS_NOT_SUPPORTED; }
    let s = unsafe { src.read_unaligned() };
    let d = unsafe { dst.read_unaligned() };
    if s.Legacy.Length > d.Legacy.Length { return STATUS_BUFFER_OVERFLOW; }
    let source_legacy = unsafe { src.cast::<u8>().offset(s.Legacy.Offset as isize) };
    let dest_legacy = unsafe { dst.cast::<u8>().offset(d.Legacy.Offset as isize) };
    copy_context_ranges(dest_legacy, source_legacy, flags, flags & 0x0011_0000);
    if flags & CONTEXT_XSTATE == 0 { return STATUS_SUCCESS; }
    if s.XState.Offset < 0 || d.XState.Offset < 0 || s.XState.Length < 64 || d.XState.Length < 64 {
        return STATUS_BUFFER_OVERFLOW;
    }
    let source_xstate = unsafe { src.cast::<u8>().add(s.XState.Offset as usize) };
    let dest_xstate = unsafe { dst.cast::<u8>().add(d.XState.Offset as usize) };
    let source_mask = unsafe { source_xstate.cast::<ULONG64>().read_unaligned() };
    let source_compaction = unsafe { source_xstate.add(8).cast::<ULONG64>().read_unaligned() };
    let enabled = NTDLL_XSTATE_ENABLED.load(Ordering::Acquire);
    let selected = source_mask & enabled & !3u64;
    unsafe {
        core::ptr::write_bytes(dest_xstate, 0, 64);
        dest_xstate.cast::<ULONG64>().write_unaligned(selected);
        let compacted = NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire);
        dest_xstate.add(8).cast::<ULONG64>().write_unaligned(
            if compacted { (1u64 << 63) | (source_compaction & enabled & !3u64) } else { 0 }
        );
    }
    for feature in 2..64 {
        let bit = 1u64 << feature;
        if selected & bit == 0 { continue; }
        let size = NTDLL_XSTATE_SIZES[feature].load(Ordering::Acquire) as usize;
        if size == 0 { continue; }
        let offset = if NTDLL_XSTATE_COMPACTION_ENABLED.load(Ordering::Acquire) {
            if source_compaction & (1u64 << 63) == 0 || source_compaction & bit == 0 { continue; }
            let mut off = 64usize;
            for previous in 2..feature {
                if source_compaction & (1u64 << previous) == 0 { continue; }
                let previous_size = NTDLL_XSTATE_SIZES[previous].load(Ordering::Acquire) as usize;
                if previous_size == 0 { return STATUS_INVALID_PARAMETER; }
                off = match off.checked_add(previous_size) { Some(v) => v, None => return STATUS_BUFFER_OVERFLOW };
                if NTDLL_XSTATE_ALIGNED_MASK.load(Ordering::Acquire) & (1u64 << (previous + 1)) != 0 {
                    off = match off.checked_add(63) { Some(v) => v & !63, None => return STATUS_BUFFER_OVERFLOW };
                }
            }
            off
        } else {
            let standard = NTDLL_XSTATE_OFFSETS[feature].load(Ordering::Acquire) as usize;
            if standard < 512 { continue; }
            standard - 512
        };
        if offset.checked_add(size).map_or(true, |end| end > s.XState.Length as usize || end > d.XState.Length as usize) {
            return STATUS_BUFFER_OVERFLOW;
        }
        unsafe { core::ptr::copy(source_xstate.add(offset), dest_xstate.add(offset), size); }
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyContext(dst: PVOID, flags: ULONG, src: PVOID) -> NTSTATUS {
    if dst.is_null() || src.is_null() { return STATUS_INVALID_PARAMETER; }
    let arch = flags & (CONTEXT_AMD64 | CONTEXT_I386);
    let (size, flags_offset) = match context_parameters(flags) {
        Some(v) => v,
        None => return STATUS_INVALID_PARAMETER,
    };
    let dflags = unsafe { dst.cast::<u8>().add(flags_offset).cast::<ULONG>().read_unaligned() };
    let sflags = unsafe { src.cast::<u8>().add(flags_offset).cast::<ULONG>().read_unaligned() };
    if dflags & (CONTEXT_AMD64 | CONTEXT_I386) != arch || sflags & (CONTEXT_AMD64 | CONTEXT_I386) != arch {
        return STATUS_INVALID_PARAMETER;
    }
    let copy_flags = flags & sflags;
    if copy_flags & !dflags & CONTEXT_XSTATE != 0 { return STATUS_BUFFER_OVERFLOW; }
    if copy_flags & CONTEXT_XSTATE != 0 {
        return RtlCopyExtendedContext(
            unsafe { dst.cast::<u8>().add(size).cast::<CONTEXT_EX>() },
            copy_flags,
            unsafe { src.cast::<u8>().add(size).cast::<CONTEXT_EX>() },
        );
    }
    copy_context_ranges(dst.cast(), src.cast(), copy_flags, arch);
    STATUS_SUCCESS
}

/// Query one enabled component's standard XSAVE-area offset and size.
pub unsafe extern "system" fn CognacRtlQueryExtendedFeature(
    feature_id: ULONG, offset: *mut ULONG, size: *mut ULONG,
) -> NTSTATUS {
    if offset.is_null() || size.is_null() || feature_id >= 64 {
        return STATUS_INVALID_PARAMETER;
    }
    if !ensure_ntdll_xstate_config() {
        return 0xC000_00BBu32 as i32;
    }
    let bit = 1u64 << feature_id;
    if NTDLL_XSTATE_ENABLED.load(Ordering::Acquire) & bit == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    *offset = NTDLL_XSTATE_OFFSETS[feature_id as usize].load(Ordering::Acquire);
    *size = NTDLL_XSTATE_SIZES[feature_id as usize].load(Ordering::Acquire);
    STATUS_SUCCESS
}


#[cfg(test)]
mod xstate_context_layout_tests {
    use super::*;
    #[test]
    fn context_extension_layout_matches_x64_chunk_contract() {
        assert_eq!(core::mem::size_of::<RTL_CONTEXT_CHUNK>(), 8);
        assert_eq!(core::mem::size_of::<CONTEXT_EX>(), 24);
    }
    #[test]
    fn native_context_sizes_are_x64_and_x86_sized() {
        assert_eq!(context_parameters(CONTEXT_AMD64 | 0x1f).unwrap().0, AMD64_CONTEXT_SIZE);
        assert_eq!(context_parameters(CONTEXT_I386 | 0x7f).unwrap().0, I386_CONTEXT_SIZE);
    }
    #[test]
    fn extended_context_length_matches_amd64_non_xstate_layout() {
        let mut length = 0;
        let status = unsafe { RtlGetExtendedContextLength2(CONTEXT_AMD64 | 0x1f, &mut length, 0) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(length as usize, AMD64_CONTEXT_SIZE + CONTEXT_EX_SIZE + 15);
    }
    #[test]
    fn extended_context_length_rejects_invalid_architecture_and_null_output() {
        let mut length = 99;
        assert_eq!(unsafe { RtlGetExtendedContextLength2(0, &mut length, 0) }, STATUS_INVALID_PARAMETER);
        assert_eq!(length, 0);
        assert_eq!(unsafe { RtlGetExtendedContextLength2(CONTEXT_AMD64, core::ptr::null_mut(), 0) }, STATUS_INVALID_PARAMETER);
    }
    #[test]
    fn locate_feature2_honors_custom_standard_configuration() {
        let mut raw = [0u64; 96];
        let ex = raw.as_mut_ptr().cast::<CONTEXT_EX>();
        unsafe {
            core::ptr::addr_of_mut!((*ex).XState).write_unaligned(RTL_CONTEXT_CHUNK { Offset: 24, Length: 320 });
        }
        let mut cfg = WindowsXStateConfiguration {
            enabled_features: 1 << 2, enabled_volatile_features: 0, size: 832, control_flags: 0,
            features: [WindowsXStateFeature { offset: 0, size: 0 }; 64],
            enabled_supervisor_features: 0, aligned_features: 0, all_feature_size: 832,
            all_features: [0; 64], enabled_user_visible_supervisor_features: 0,
            extended_feature_disable_features: 0, all_non_large_feature_size: 832, max_sve_vector_length: 0, spare1: 0,
        };
        cfg.features[2] = WindowsXStateFeature { offset: 576, size: 256 };
        let mut length = 0;
        let found = unsafe { RtlLocateExtendedFeature2(ex, 2, (&mut cfg as *mut WindowsXStateConfiguration).cast(), &mut length) };
        assert_eq!(length, 256);
        assert_eq!(found as usize, ex as usize + 24 + 64);
    }
    #[test]
    fn locate_feature2_honors_custom_compacted_configuration() {
        let mut raw = [0u64; 96];
        let ex = raw.as_mut_ptr().cast::<CONTEXT_EX>();
        unsafe {
            core::ptr::addr_of_mut!((*ex).XState).write_unaligned(RTL_CONTEXT_CHUNK { Offset: 24, Length: 320 });
            let xstate = ex.cast::<u8>().add(24);
            xstate.add(8).cast::<ULONG64>().write_unaligned((1u64 << 63) | (1 << 2));
        }
        let mut cfg = WindowsXStateConfiguration {
            enabled_features: 1 << 2, enabled_volatile_features: 0, size: 832, control_flags: 2,
            features: [WindowsXStateFeature { offset: 0, size: 0 }; 64],
            enabled_supervisor_features: 0, aligned_features: 0, all_feature_size: 832,
            all_features: [0; 64], enabled_user_visible_supervisor_features: 0,
            extended_feature_disable_features: 0, all_non_large_feature_size: 832, max_sve_vector_length: 0, spare1: 0,
        };
        cfg.features[2] = WindowsXStateFeature { offset: 576, size: 256 };
        let mut length = 0;
        let found = unsafe { RtlLocateExtendedFeature2(ex, 2, (&mut cfg as *mut WindowsXStateConfiguration).cast(), &mut length) };
        assert_eq!(length, 256);
        assert_eq!(found as usize, ex as usize + 24 + 64);
    }
    #[test]
    fn locate_feature2_rejects_a_truncated_xstate_chunk() {
        let mut raw = [0u64; 16];
        let ex = raw.as_mut_ptr().cast::<CONTEXT_EX>();
        unsafe {
            core::ptr::addr_of_mut!((*ex).XState).write_unaligned(RTL_CONTEXT_CHUNK { Offset: 24, Length: 64 });
        }
        let mut cfg = WindowsXStateConfiguration {
            enabled_features: 1 << 2, enabled_volatile_features: 0, size: 832, control_flags: 0,
            features: [WindowsXStateFeature { offset: 0, size: 0 }; 64],
            enabled_supervisor_features: 0, aligned_features: 0, all_feature_size: 832,
            all_features: [0; 64], enabled_user_visible_supervisor_features: 0,
            extended_feature_disable_features: 0, all_non_large_feature_size: 832,
            max_sve_vector_length: 0, spare1: 0,
        };
        cfg.features[2] = WindowsXStateFeature { offset: 576, size: 256 };
        let mut length = 0;
        let found = unsafe { RtlLocateExtendedFeature2(ex, 2, (&mut cfg as *mut WindowsXStateConfiguration).cast(), &mut length) };
        assert!(found.is_null());
        assert_eq!(length, 256);
    }
    #[test]
    fn locate_feature2_null_inputs_clear_output_length() {
        let mut length = 99;
        let found = unsafe { RtlLocateExtendedFeature2(core::ptr::null_mut(), 2, core::ptr::null_mut(), &mut length) };
        assert!(found.is_null());
        assert_eq!(length, 0);
    }
    #[test]
    fn partial_context_copy_updates_only_requested_control_group() {
        let mut source = [0u8; AMD64_CONTEXT_SIZE];
        let mut destination = [0xA5u8; AMD64_CONTEXT_SIZE];
        unsafe {
            source.as_mut_ptr().add(0x30).cast::<ULONG>().write_unaligned(CONTEXT_AMD64 | 1);
            destination.as_mut_ptr().add(0x30).cast::<ULONG>().write_unaligned(CONTEXT_AMD64);
            source.as_mut_ptr().add(0x38).write(0x11);
            source.as_mut_ptr().add(0x42).write(0x22);
            destination.as_mut_ptr().add(0x38).write(0x33);
            destination.as_mut_ptr().add(0x42).write(0x44);
        }
        let status = unsafe { RtlCopyContext(destination.as_mut_ptr().cast(), CONTEXT_AMD64 | 1, source.as_mut_ptr().cast()) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(destination[0x38], 0x11);
        assert_eq!(destination[0x42], 0x22);
        assert_eq!(destination[0x100], 0xA5);
    }
    #[test]
    fn published_configuration_fits_one_user_page() {
        assert_eq!(core::mem::size_of::<WindowsXStateConfiguration>(), 0x348);
        assert!(KUSER_XSTATE_OFFSET + core::mem::size_of::<WindowsXStateConfiguration>() <= 4096);
    }
}

}

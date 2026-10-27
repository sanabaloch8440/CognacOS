//! Windows 11 x64 NTDLL process export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::arch::global_asm;

global_asm!(r#"
    .text
    .p2align 4
    .global cognacos_nt_open_process
cognacos_nt_open_process:
    mov r10, rcx
    mov eax, 0x150
    syscall
    ret
    .p2align 4
    .global cognacos_nt_get_next_process
cognacos_nt_get_next_process:
    mov r10, rcx
    mov eax, 0x151
    syscall
    ret
"#);

unsafe extern "system" {
    fn cognacos_nt_open_process(out: *mut *mut core::ffi::c_void, access: u32, attributes: *const core::ffi::c_void, client_id: *const core::ffi::c_void) -> i32;
    fn cognacos_nt_get_next_process(previous: *mut core::ffi::c_void, access: u32, attributes: u32, flags: u32, out: *mut *mut core::ffi::c_void) -> i32;
}

#[no_mangle]
pub unsafe extern "system" fn NtAcceptConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAcquireProcessActivityReference() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAdjustGroupsToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAdjustPrivilegesToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAdjustTokenClaimsAndDeviceGroups() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlertMultipleThreadByThreadId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlertResumeThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlertThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlertThreadByThreadId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlertThreadByThreadIdEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAssignProcessToJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtChangeProcessState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtChangeThreadState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCompareTokens() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCompleteConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateJobSet() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateLowBoxToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreatePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateProcessEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateProcessStateChange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateThreadEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateThreadStateChange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateTokenEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateUserProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateWaitablePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDebugActiveProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDuplicateToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFilterToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFilterTokenEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub extern "system" fn NtFlushProcessWriteBuffers() -> i32 {
    // Full memory fence: orders writes before this call against subsequent
    // loads/stores on the calling processor. A true cross-processor flush
    // requires scheduler/IPI support from the kernel.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("mfence", options(nostack, preserves_flags));
    }
    0
}

/// Thread context retrieval is unavailable until CognacOS implements
/// scheduler-owned saved register contexts. Keep the Windows ABI signature so
/// callers receive a defined status instead of entering with the wrong stack
/// layout.
#[no_mangle]
pub unsafe extern "system" fn NtGetContextThread(
    thread_handle: *mut core::ffi::c_void,
    context: *mut core::ffi::c_void,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    if thread_handle.is_null() || context.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub extern "system" fn NtGetCurrentProcessorNumber() -> u32 {
    // Windows x64 kernels commonly publish the current logical processor in
    // IA32_TSC_AUX. This is only valid when the kernel programs that MSR.
    // RDTSCP is feature-gated so older CPUs never receive an invalid opcode.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Use the compiler's CPUID intrinsic so this remains valid on ELF/Linux
        // test builds, where LLVM reserves RBX for its own use.
        let max_ext = core::arch::x86_64::__cpuid(0x8000_0000).eax;
        if max_ext >= 0x8000_0001 {
            let features = core::arch::x86_64::__cpuid(0x8000_0001).edx;
            if features & (1 << 27) != 0 {
                let aux: u32;
                let low: u32;
                let high: u32;
                core::arch::asm!("rdtscp", out("eax") low, out("edx") high, out("ecx") aux,
                    options(nomem, nostack));
                let _ = (low, high);
                return aux & 0xFFFF;
            }
        }
    }

    // A single-processor bootstrap has processor number zero until the kernel
    // exposes a processor-number service. Do not guess from APIC IDs.
    0
}

#[repr(C)]
pub struct COGNAC_PROCESSOR_NUMBER {
    pub Group: u16,
    pub Number: u8,
    pub Reserved: u8,
}

#[no_mangle]
pub unsafe extern "system" fn NtGetCurrentProcessorNumberEx(processor_number: *mut COGNAC_PROCESSOR_NUMBER) -> i32 {
    const STATUS_SUCCESS: i32 = 0;
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    if processor_number.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    // The native kernel currently exposes only the bootstrap processor group.
    // Use the same source as NtGetCurrentProcessorNumber and report group zero.
    let number = NtGetCurrentProcessorNumber();
    unsafe {
        processor_number.write(COGNAC_PROCESSOR_NUMBER {
            Group: 0,
            Number: number.min(u8::MAX as u32) as u8,
            Reserved: 0,
        });
    }
    STATUS_SUCCESS
}

/// Enumerates process handles through the native process-table syscall path.
#[no_mangle]
pub unsafe extern "system" fn NtGetNextProcess(
    process_handle: *mut core::ffi::c_void, desired_access: u32,
    handle_attributes: u32, flags: u32,
    new_process_handle: *mut *mut core::ffi::c_void,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    if new_process_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { new_process_handle.write(core::ptr::null_mut()); }
    cognacos_nt_get_next_process(process_handle, desired_access, handle_attributes, flags, new_process_handle)
}

pub unsafe extern "system" fn NtGetNextThread(
    process_handle: *mut core::ffi::c_void,
    thread_handle: *mut core::ffi::c_void,
    desired_access: u32,
    handle_attributes: u32,
    flags: u32,
    new_thread_handle: *mut *mut core::ffi::c_void,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;
    if new_thread_handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let _ = (process_handle, thread_handle, desired_access, handle_attributes, flags);
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtImpersonateAnonymousToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtImpersonateClientOfPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtImpersonateThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

/// Checks job membership for the current process.
///
/// CognacOS does not yet implement job objects. For the current-process
/// pseudo handle and a null JobHandle (the query "is this process in any
/// job?"), the answer is therefore FALSE. Other handles are not guessed at.
#[no_mangle]
pub unsafe extern "system" fn NtIsProcessInJob(
    process_handle: *mut core::ffi::c_void,
    job_handle: *mut core::ffi::c_void,
    result: *mut u8,
) -> i32 {
    const STATUS_SUCCESS: i32 = 0;
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;
    if result.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if process_handle as isize != -1 {
        return if process_handle.is_null() {
            0xC000_0008u32 as i32 // STATUS_INVALID_HANDLE
        } else {
            STATUS_NOT_IMPLEMENTED
        };
    }
    if !job_handle.is_null() {
        return STATUS_NOT_IMPLEMENTED;
    }
    unsafe { result.write(0) };
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn NtListenPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

/// Opens a process by client ID through the kernel Object Manager.
#[no_mangle]
pub unsafe extern "system" fn NtOpenProcess(
    process_handle: *mut *mut core::ffi::c_void,
    desired_access: u32,
    object_attributes: *const core::ffi::c_void,
    client_id: *const core::ffi::c_void,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    if process_handle.is_null() || object_attributes.is_null() || client_id.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { process_handle.write(core::ptr::null_mut()); }
    cognacos_nt_open_process(process_handle, desired_access, object_attributes, client_id)
}

pub unsafe extern "system" fn NtOpenProcessToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenProcessTokenEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

/// Opens a thread by client ID. The ABI is defined, but thread objects and
/// handle lookup are not implemented by the current kernel.
#[no_mangle]
pub unsafe extern "system" fn NtOpenThread(
    thread_handle: *mut *mut core::ffi::c_void,
    desired_access: u32,
    object_attributes: *const core::ffi::c_void,
    client_id: *const core::ffi::c_void,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;
    if thread_handle.is_null() || object_attributes.is_null() || client_id.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let _ = desired_access;
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenThreadToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenThreadTokenEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

// NtQueryInformationProcess and NtQueryInformationThread are emitted from the
// canonical syscall manifest by build.rs and implemented in syscall_stubs.rs.

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryPortInformationProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySecurityAttributesToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueueApcThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueueApcThreadEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueueApcThreadEx2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRegisterThreadTerminatePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRemoveProcessDebug() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReplyPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReplyWaitReceivePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReplyWaitReceivePortEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReplyWaitReplyPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRequestPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRequestWaitReplyPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtResumeProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtResumeThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSecureConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

/// Setting another thread's register context requires scheduler support.
#[no_mangle]
pub unsafe extern "system" fn NtSetContextThread(
    thread_handle: *mut core::ffi::c_void,
    context: *const core::ffi::c_void,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    if thread_handle.is_null() || context.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetDefaultHardErrorPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetThreadExecutionState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetWnfProcessNotificationEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSuspendProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSuspendThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtTerminateJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

/// Terminates the current process through CognacOS's native exit gate.
///
/// The bootstrap kernel currently exposes process exit but not a general
/// process-handle termination service. Consequently, this implementation
/// intentionally accepts only the current-process pseudo handle and refuses
/// to pretend that arbitrary process handles are supported.
#[no_mangle]
pub unsafe extern "system" fn NtTerminateProcess(
    process_handle: *mut core::ffi::c_void,
    exit_status: i32,
) -> i32 {
    const STATUS_SUCCESS: i32 = 0;
    const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;
    const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;
    // NT's current-process pseudo handle is (HANDLE)-1.
    if process_handle as isize != -1 {
        return if process_handle.is_null() { STATUS_INVALID_HANDLE } else { STATUS_NOT_IMPLEMENTED };
    }
    #[cfg(any(target_os = "none", target_os = "windows"))]
    unsafe {
        core::arch::asm!(
            "int 0x81",
            in("rax") 0u64,
            in("rdi") exit_status as u64,
            options(noreturn)
        );
    }
    #[cfg(not(any(target_os = "none", target_os = "windows")))]
    {
        let _ = exit_status;
        STATUS_SUCCESS
    }
}

#[no_mangle]
pub unsafe extern "system" fn NtTerminateThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtUmsThreadYield() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWaitForAlertByThreadId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateProcessParametersWithTemplate() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUserProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUserProcessEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlExitUserProcess(exit_status: i32) -> ! {
    // Run loader shutdown and user-mode rundown first. The actual exit transition
    // is the kernel's SYS_EXIT gate, whose EXIT_SENTINEL diverts to the process
    // exit stub rather than returning to a dying address space.
    let _ = unsafe { super::loader::LdrShutdownProcess() };
    #[cfg(any(target_os = "none", target_os = "windows"))]
    unsafe {
        core::arch::asm!("int 0x81", in("rax") 0u64, in("rdi") exit_status as u64, options(noreturn));
    }
    #[cfg(not(any(target_os = "none", target_os = "windows")))]
    {
        let _ = exit_status;
        loop { core::hint::spin_loop(); }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlExitUserThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUserThreadStart() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAcceptConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAcquireProcessActivityReference() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAdjustGroupsToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAdjustPrivilegesToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAdjustTokenClaimsAndDeviceGroups() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlertMultipleThreadByThreadId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlertResumeThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlertThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlertThreadByThreadId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlertThreadByThreadIdEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcAcceptConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcConnectPortEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcCreatePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcCreatePortSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcDeletePortSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcDisconnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcImpersonateClientContainerOfPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcImpersonateClientOfPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcOpenSenderProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcOpenSenderThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcSendWaitReceivePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAssignProcessToJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwChangeProcessState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwChangeThreadState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCompareTokens() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCompleteConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateJobSet() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateLowBoxToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreatePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateProcessEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateProcessStateChange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateThreadEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateThreadStateChange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateTokenEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateUserProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateWaitablePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDebugActiveProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDuplicateToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFilterToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFilterTokenEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub extern "system" fn ZwFlushProcessWriteBuffers() -> i32 {
    NtFlushProcessWriteBuffers()
}

#[no_mangle]
pub unsafe extern "system" fn ZwGetContextThread(
    thread_handle: *mut core::ffi::c_void,
    context: *mut core::ffi::c_void,
) -> i32 {
    unsafe { NtGetContextThread(thread_handle, context) }
}

#[no_mangle]
pub extern "system" fn ZwGetCurrentProcessorNumber() -> u32 {
    NtGetCurrentProcessorNumber()
}

#[no_mangle]
pub unsafe extern "system" fn ZwGetCurrentProcessorNumberEx(processor_number: *mut COGNAC_PROCESSOR_NUMBER) -> i32 {
    unsafe { NtGetCurrentProcessorNumberEx(processor_number) }
}

#[no_mangle]
pub unsafe extern "system" fn ZwGetNextProcess(
    process_handle: *mut core::ffi::c_void,
    desired_access: u32,
    handle_attributes: u32,
    flags: u32,
    new_process_handle: *mut *mut core::ffi::c_void,
) -> i32 {
    unsafe { NtGetNextProcess(process_handle, desired_access, handle_attributes, flags, new_process_handle) }
}

#[no_mangle]
pub unsafe extern "system" fn ZwGetNextThread(
    process_handle: *mut core::ffi::c_void,
    thread_handle: *mut core::ffi::c_void,
    desired_access: u32,
    handle_attributes: u32,
    flags: u32,
    new_thread_handle: *mut *mut core::ffi::c_void,
) -> i32 {
    unsafe { NtGetNextThread(process_handle, thread_handle, desired_access, handle_attributes, flags, new_thread_handle) }
}

#[no_mangle]
pub unsafe extern "system" fn ZwImpersonateAnonymousToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwImpersonateClientOfPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwImpersonateThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwIsProcessInJob(
    process_handle: *mut core::ffi::c_void,
    job_handle: *mut core::ffi::c_void,
    result: *mut u8,
) -> i32 {
    unsafe { NtIsProcessInJob(process_handle, job_handle, result) }
}

#[no_mangle]
pub unsafe extern "system" fn ZwListenPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenProcess(
    process_handle: *mut *mut core::ffi::c_void,
    desired_access: u32,
    object_attributes: *const core::ffi::c_void,
    client_id: *const core::ffi::c_void,
) -> i32 {
    unsafe { NtOpenProcess(process_handle, desired_access, object_attributes, client_id) }
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenProcessToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenProcessTokenEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenThread(
    thread_handle: *mut *mut core::ffi::c_void,
    desired_access: u32,
    object_attributes: *const core::ffi::c_void,
    client_id: *const core::ffi::c_void,
) -> i32 {
    unsafe { NtOpenThread(thread_handle, desired_access, object_attributes, client_id) }
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenThreadToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenThreadTokenEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

/// Native Zw spelling; on Windows this forwards to the matching syscall entry.
#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationProcess(
    process: *mut core::ffi::c_void,
    information_class: u32,
    information: *mut core::ffi::c_void,
    information_length: u32,
    return_length: *mut u32,
) -> i32 {
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" {
            fn NtQueryInformationProcess(process: *mut core::ffi::c_void, information_class: u32, information: *mut core::ffi::c_void, information_length: u32, return_length: *mut u32) -> i32;
        }
        return unsafe { NtQueryInformationProcess(process, information_class, information, information_length, return_length) };
    }
    #[cfg(not(target_os = "windows"))]
    { let _ = (process, information_class, information, information_length, return_length); 0xC0000002u32 as i32 }
}

/// Native Zw spelling; on Windows this forwards to the matching syscall entry.
#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationThread(
    thread: *mut core::ffi::c_void,
    information_class: u32,
    information: *mut core::ffi::c_void,
    information_length: u32,
    return_length: *mut u32,
) -> i32 {
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" {
            fn NtQueryInformationThread(thread: *mut core::ffi::c_void, information_class: u32, information: *mut core::ffi::c_void, information_length: u32, return_length: *mut u32) -> i32;
        }
        return unsafe { NtQueryInformationThread(thread, information_class, information, information_length, return_length) };
    }
    #[cfg(not(target_os = "windows"))]
    { let _ = (thread, information_class, information, information_length, return_length); 0xC0000002u32 as i32 }
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryPortInformationProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySecurityAttributesToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueueApcThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueueApcThreadEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueueApcThreadEx2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRegisterThreadTerminatePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRemoveProcessDebug() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReplyPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReplyWaitReceivePort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReplyWaitReceivePortEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReplyWaitReplyPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRequestPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRequestWaitReplyPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwResumeProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwResumeThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSecureConnectPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetContextThread(
    thread_handle: *mut core::ffi::c_void,
    context: *const core::ffi::c_void,
) -> i32 {
    unsafe { NtSetContextThread(thread_handle, context) }
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetDefaultHardErrorPort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetThreadExecutionState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetWnfProcessNotificationEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSuspendProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSuspendThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwTerminateJobObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwTerminateProcess(
    process_handle: *mut core::ffi::c_void,
    exit_status: i32,
) -> i32 {
    unsafe { NtTerminateProcess(process_handle, exit_status) }
}

#[no_mangle]
pub unsafe extern "system" fn ZwTerminateThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwUmsThreadYield() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWaitForAlertByThreadId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


// Compatibility routines ported from the former process source. They remain
// in this file under a private implementation namespace to keep one process
// subsystem source while avoiding collisions with the canonical Nt* routines.
mod wow64_compat {
//! Source-first Rust port of selected routines from Wine's ntdll/process.c.
//!
//! This module keeps the public NT calling convention and data layouts explicit.
//! Kernel operations are forwarded to native Nt* services; the corresponding
//! CognacOS syscall implementations and module/build integration remain separate work.
//! Wine source is LGPL-2.1-or-later; see the repository's Wine license notices.

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use core::ffi::c_void;
use core::ptr::null_mut;

pub type BOOLEAN = u8;
pub type BYTE = u8;
pub type DWORD = u32;
pub type HANDLE = *mut c_void;
pub type LONG = i32;
pub type NTSTATUS = i32;
pub type ULONG = u32;
pub type ULONG_PTR = usize;
pub type USHORT = u16;
pub type PVOID = *mut c_void;

pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
pub const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
pub const STATUS_INFO_LENGTH_MISMATCH: NTSTATUS = 0xC000_0004u32 as i32;
pub const STATUS_UNSUCCESSFUL: NTSTATUS = 0xC000_0001u32 as i32;
pub const STATUS_ACCESS_VIOLATION: NTSTATUS = 0xC000_0005u32 as i32;

pub const IMAGE_FILE_MACHINE_I386: USHORT = 0x014c;
pub const IMAGE_FILE_MACHINE_AMD64: USHORT = 0x8664;
pub const IMAGE_FILE_MACHINE_ARMNT: USHORT = 0x01c4;
pub const IMAGE_FILE_MACHINE_ARM64: USHORT = 0xaa64;
pub const PROCESSOR_ARCHITECTURE_UNKNOWN: USHORT = 0xffff;

pub const PROCESS_WOW64_INFORMATION_CLASS: ULONG = 26;
pub const THREAD_WOW64_CONTEXT_INFORMATION_CLASS: ULONG = 29;
pub const THREAD_BASIC_INFORMATION_CLASS: ULONG = 0;
pub const DUPLICATE_SAME_ACCESS: ULONG = 0x0000_0002;

/// Pseudo-handle value used by Windows for the current process.
pub const CURRENT_PROCESS_PSEUDO_HANDLE: HANDLE = (-1isize) as HANDLE;

/// ABI prefix for SYSTEM_SUPPORTED_PROCESSOR_ARCHITECTURES_INFORMATION.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SYSTEM_SUPPORTED_PROCESSOR_ARCHITECTURES_INFORMATION {
    pub Machine: USHORT,
    pub KernelMode: BOOLEAN,
    pub UserMode: BOOLEAN,
    pub Native: BOOLEAN,
    pub Process: BOOLEAN,
    pub WoW64Container: BOOLEAN,
    pub Reserved: [BYTE; 2],
}

/// ABI-compatible WOW64 shared information payload used by process helpers.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct WOW64INFO {
    pub Version: ULONG,
    pub Machine: ULONG,
    pub Flags: ULONG,
    pub SharedInformation: ULONG_PTR,
    pub SectionHandle: ULONG_PTR,
}

/// Basic process information prefix returned by NtQueryInformationProcess.
#[repr(C)]
pub struct PROCESS_BASIC_INFORMATION {
    pub ExitStatus: NTSTATUS,
    pub PebBaseAddress: PVOID,
    pub AffinityMask: ULONG_PTR,
    pub BasePriority: LONG,
    pub UniqueProcessId: ULONG_PTR,
    pub InheritedFromUniqueProcessId: ULONG_PTR,
}

/// Native thread basic information layout.
#[repr(C)]
pub struct THREAD_BASIC_INFORMATION {
    pub ExitStatus: NTSTATUS,
    pub TebBaseAddress: PVOID,
    pub ClientIdUniqueProcess: HANDLE,
    pub ClientIdUniqueThread: HANDLE,
    pub AffinityMask: ULONG_PTR,
    pub Priority: LONG,
    pub BasePriority: LONG,
}

extern "system" {
    fn NtCompareObjects(first: HANDLE, second: HANDLE) -> NTSTATUS;
    fn NtQuerySystemInformationEx(
        information_class: ULONG,
        input: PVOID,
        input_length: ULONG,
        output: PVOID,
        output_length: ULONG,
        return_length: *mut ULONG,
    ) -> NTSTATUS;
    fn NtQueryInformationProcess(
        process: HANDLE,
        information_class: ULONG,
        information: PVOID,
        information_length: ULONG,
        return_length: *mut ULONG,
    ) -> NTSTATUS;
    fn NtReadVirtualMemory(
        process: HANDLE,
        base: PVOID,
        buffer: PVOID,
        size: usize,
        bytes_read: *mut usize,
    ) -> NTSTATUS;
    fn NtQueryInformationThread(
        thread: HANDLE,
        information_class: ULONG,
        information: PVOID,
        information_length: ULONG,
        return_length: *mut ULONG,
    ) -> NTSTATUS;
    fn NtSetInformationThread(
        thread: HANDLE,
        information_class: ULONG,
        information: PVOID,
        information_length: ULONG,
    ) -> NTSTATUS;
    fn NtSuspendThread(thread: HANDLE, previous_count: *mut ULONG) -> NTSTATUS;
    fn NtCreateDebugObject(
        debug_object: *mut HANDLE,
        desired_access: ULONG,
        object_attributes: PVOID,
        flags: ULONG,
    ) -> NTSTATUS;
    fn NtDebugActiveProcess(process: HANDLE, debug_object: HANDLE) -> NTSTATUS;
    fn NtRemoveProcessDebug(process: HANDLE, debug_object: HANDLE) -> NTSTATUS;
    fn NtDebugContinue(debug_object: HANDLE, client_id: PVOID, status: NTSTATUS) -> NTSTATUS;
    fn NtWaitForDebugEvent(
        debug_object: HANDLE,
        alertable: BOOLEAN,
        timeout: PVOID,
        state: PVOID,
    ) -> NTSTATUS;
    fn NtClose(handle: HANDLE) -> NTSTATUS;
    fn RtlGetCurrentPeb() -> PVOID;
}

/// Matches Wine's object-comparison semantics for the current-process pseudo-handle.
#[no_mangle]
pub unsafe extern "system" fn RtlIsCurrentProcess(handle: HANDLE) -> BOOLEAN {
    if handle == CURRENT_PROCESS_PSEUDO_HANDLE {
        return 1;
    }
    (unsafe { NtCompareObjects(handle, CURRENT_PROCESS_PSEUDO_HANDLE) } == STATUS_SUCCESS) as BOOLEAN
}

/// Returns the machine architecture of this native NTDLL build.
/// WOW64-specific TEB/CPU-area detection is deferred until CognacOS provides
/// a validated WOW64 TEB layout.
#[no_mangle]
pub extern "system" fn RtlWow64GetCurrentMachine() -> USHORT {
    #[cfg(target_arch = "x86_64")]
    { return IMAGE_FILE_MACHINE_AMD64; }
    #[cfg(target_arch = "x86")]
    { return IMAGE_FILE_MACHINE_I386; }
    #[cfg(target_arch = "aarch64")]
    { return IMAGE_FILE_MACHINE_ARM64; }
    #[cfg(target_arch = "arm")]
    { return IMAGE_FILE_MACHINE_ARMNT; }
    #[allow(unreachable_code)]
    PROCESSOR_ARCHITECTURE_UNKNOWN
}

/// Queries the kernel's supported-processor list and returns process/native machines.
#[no_mangle]
pub unsafe extern "system" fn RtlWow64GetProcessMachines(
    process: HANDLE,
    current_ret: *mut USHORT,
    native_ret: *mut USHORT,
) -> NTSTATUS {
    let mut machines = [SYSTEM_SUPPORTED_PROCESSOR_ARCHITECTURES_INFORMATION::default(); 8];
    let mut return_length = 0u32;
    let status = unsafe {
        NtQuerySystemInformationEx(
            181, // SystemSupportedProcessorArchitectures (Wine winternl.h).
            (&process as *const HANDLE).cast_mut().cast(),
            core::mem::size_of::<HANDLE>() as ULONG,
            machines.as_mut_ptr().cast(),
            core::mem::size_of_val(&machines) as ULONG,
            &mut return_length,
        )
    };
    if status < 0 {
        return status;
    }
    let mut current = 0;
    let mut native = 0;
    for entry in machines.iter().take_while(|entry| entry.Machine != 0) {
        if entry.Native != 0 {
            native = entry.Machine;
        } else if entry.Process != 0 {
            current = entry.Machine;
        }
    }
    if !current_ret.is_null() { unsafe { current_ret.write(current) }; }
    if !native_ret.is_null() { unsafe { native_ret.write(native) }; }
    STATUS_SUCCESS
}

/// Tests whether a non-native guest machine is exposed by the kernel.
#[no_mangle]
pub unsafe extern "system" fn RtlWow64IsWowGuestMachineSupported(
    machine: USHORT,
    supported: *mut BOOLEAN,
) -> NTSTATUS {
    if supported.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { supported.write(0) };
    let process: HANDLE = null_mut();
    let mut machines = [SYSTEM_SUPPORTED_PROCESSOR_ARCHITECTURES_INFORMATION::default(); 8];
    let status = unsafe {
        NtQuerySystemInformationEx(
            181,
            (&process as *const HANDLE).cast_mut().cast(),
            core::mem::size_of::<HANDLE>() as ULONG,
            machines.as_mut_ptr().cast(),
            core::mem::size_of_val(&machines) as ULONG,
            null_mut(),
        )
    };
    if status < 0 { return status; }
    for entry in machines.iter().take_while(|entry| entry.Machine != 0) {
        if entry.Native == 0 && entry.Machine == machine {
            unsafe { supported.write(1) };
            break;
        }
    }
    STATUS_SUCCESS
}

/// Reads the WOW64 PEB address and the adjacent WOW64INFO structure from another process.
#[no_mangle]
pub unsafe extern "system" fn RtlWow64GetSharedInfoProcess(
    process: HANDLE,
    is_wow64: *mut BOOLEAN,
    info: *mut WOW64INFO,
) -> NTSTATUS {
    if is_wow64.is_null() || info.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut peb32: PVOID = null_mut();
    let status = unsafe {
        NtQueryInformationProcess(
            process, PROCESS_WOW64_INFORMATION_CLASS, (&mut peb32 as *mut PVOID).cast(),
            core::mem::size_of::<PVOID>() as ULONG, null_mut(),
        )
    };
    if status < 0 { return status; }
    unsafe { is_wow64.write((!peb32.is_null()) as BOOLEAN) };
    if peb32.is_null() {
        unsafe { info.write(WOW64INFO::default()) };
        return STATUS_SUCCESS;
    }
    let info_address = (peb32 as usize).wrapping_add(core::mem::size_of::<u32>()) as PVOID;
    unsafe {
        NtReadVirtualMemory(
            process, info_address, info.cast(), core::mem::size_of::<WOW64INFO>(), null_mut(),
        )
    }
}

/// Native wrappers intentionally pass the Windows information class and buffer through.
#[no_mangle]
pub unsafe extern "system" fn RtlWow64GetThreadContext(
    thread: HANDLE, context: PVOID,
) -> NTSTATUS {
    if context.is_null() { return STATUS_INVALID_PARAMETER; }
    // WOW64_CONTEXT is the fixed 32-bit CONTEXT ABI (716 bytes on Windows).
    unsafe {
        NtQueryInformationThread(
            thread, THREAD_WOW64_CONTEXT_INFORMATION_CLASS, context, 716, null_mut(),
        )
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64SetThreadContext(
    thread: HANDLE, context: PVOID,
) -> NTSTATUS {
    if context.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe {
        NtSetInformationThread(thread, THREAD_WOW64_CONTEXT_INFORMATION_CLASS, context, 716)
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64SuspendThread(
    thread: HANDLE, previous_count: *mut ULONG,
) -> NTSTATUS {
    unsafe { NtSuspendThread(thread, previous_count) }
}

/// Thread debug-object storage is accessed through the native TEB by a future TEB-layout bridge.
/// These APIs avoid fabricating a handle when that bridge is unavailable.
#[no_mangle]
pub unsafe extern "system" fn DbgUiConnectToDbg() -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiDebugActiveProcess(_process: HANDLE) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiStopDebugging(process: HANDLE) -> NTSTATUS {
    let peb = unsafe { RtlGetCurrentPeb() };
    if peb.is_null() { return STATUS_NOT_IMPLEMENTED; }
    // Debug-object location in TEB is intentionally not guessed here.
    let _ = process;
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiContinue(_client: PVOID, _status: NTSTATUS) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiWaitStateChange(_state: PVOID, _timeout: PVOID) -> NTSTATUS {
    STATUS_NOT_IMPLEMENTED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_constants_match_pe_coff_values() {
        assert_eq!(IMAGE_FILE_MACHINE_I386, 0x014c);
        assert_eq!(IMAGE_FILE_MACHINE_AMD64, 0x8664);
        assert_eq!(IMAGE_FILE_MACHINE_ARMNT, 0x01c4);
        assert_eq!(IMAGE_FILE_MACHINE_ARM64, 0xaa64);
    }

    #[test]
    fn architecture_detection_is_nonzero_on_supported_targets() {
        assert_ne!(RtlWow64GetCurrentMachine(), PROCESSOR_ARCHITECTURE_UNKNOWN);
    }

    #[test]
    fn processor_architecture_record_layout_is_stable() {
        assert_eq!(core::mem::size_of::<SYSTEM_SUPPORTED_PROCESSOR_ARCHITECTURES_INFORMATION>(), 8);
    }
}

}

pub use wow64_compat::*;

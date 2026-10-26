//! Windows 11 x64 NTDLL process export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

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
        let max_ext: u32;
        core::arch::asm!("cpuid", inout("eax") 0x8000_0000u32 => max_ext,
            lateout("ebx") _, lateout("ecx") _, lateout("edx") _, options(nomem, nostack));
        if max_ext >= 0x8000_0001 {
            let features: u32;
            core::arch::asm!("cpuid", inout("eax") 0x8000_0001u32 => features,
                lateout("ebx") _, lateout("ecx") _, lateout("edx") _, options(nomem, nostack));
            if features & (1 << 27) != 0 {
                let low: u32;
                let high: u32;
                let aux: u32;
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

/// Enumerates process handles. CognacOS has no process object table yet;
/// this keeps the native ABI correct while reporting the missing service.
#[no_mangle]
pub unsafe extern "system" fn NtGetNextProcess(
    process_handle: *mut core::ffi::c_void,
    desired_access: u32,
    handle_attributes: u32,
    flags: u32,
    new_process_handle: *mut *mut core::ffi::c_void,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;
    if new_process_handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let _ = (process_handle, desired_access, handle_attributes, flags);
    STATUS_NOT_IMPLEMENTED
}

/// Enumerates threads belonging to a process. Thread enumeration depends on
/// the not-yet-implemented scheduler/thread object table.
#[no_mangle]
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

/// Opens a process by client ID. The ABI is defined, but CognacOS does not
/// yet provide a process-handle/object-manager service, so valid requests
/// return STATUS_NOT_IMPLEMENTED without fabricating a handle.
#[no_mangle]
pub unsafe extern "system" fn NtOpenProcess(
    process_handle: *mut *mut core::ffi::c_void,
    desired_access: u32,
    object_attributes: *const core::ffi::c_void,
    client_id: *const core::ffi::c_void,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;
    if process_handle.is_null() || object_attributes.is_null() || client_id.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let _ = desired_access;
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
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

// NtQueryInformationProcess is provided by the typed x64 syscall stub in syscall_x86_64.rs.

// NtQueryInformationThread is provided by the typed x64 syscall stub in syscall_x86_64.rs.

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

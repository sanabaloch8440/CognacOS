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

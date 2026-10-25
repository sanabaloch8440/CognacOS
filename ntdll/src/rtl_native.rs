//! Source-first Rust port of CognacOS's native NTDLL bootstrap helpers.
//!
//! Source correspondence: root/wine/dlls/ntdll/cognac_native.c.
//! This ports the existing bootstrap behavior, including its fixed reported
//! version tuple and deliberately small NTSTATUS-to-DOS-error table. It is not
//! a complete Windows NTDLL implementation. Build integration is deferred.
//!
//! # Safety
//! These exports use raw pointers and Windows x64 GS-relative TEB fields.
//! The launcher must install the expected TEB layout before calling them, and
//! callers must pass valid pointers for all non-null input/output parameters.

#![allow(non_camel_case_types, non_snake_case)]

use core::ffi::c_void;

pub type LONG = i32;
pub type ULONG = u32;
pub type DWORD = u32;
pub type WCHAR = u16;
pub type PVOID = *mut c_void;
pub type PULONG = *mut ULONG;
pub type HINSTANCE = *mut c_void;
pub type LPVOID = *mut c_void;
pub type BOOL = i32;
pub type NTSTATUS = LONG;

pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002_u32 as NTSTATUS;
pub const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000D_u32 as NTSTATUS;

pub const ERROR_SUCCESS: ULONG = 0;
pub const ERROR_INVALID_HANDLE: ULONG = 6;
pub const ERROR_NOT_ENOUGH_MEMORY: ULONG = 8;
pub const ERROR_ACCESS_DENIED: ULONG = 5;
pub const ERROR_INVALID_PARAMETER: ULONG = 87;
pub const ERROR_INSUFFICIENT_BUFFER: ULONG = 122;
pub const ERROR_CALL_NOT_IMPLEMENTED: ULONG = 120;
pub const ERROR_NOACCESS: ULONG = 998;
pub const ERROR_FILE_NOT_FOUND: ULONG = 2;
pub const ERROR_MR_MID_NOT_FOUND: ULONG = 317;

/// ABI-compatible RTL_OSVERSIONINFOW layout used by the native C bootstrap.
#[repr(C)]
pub struct COGNAC_RTL_OSVERSIONINFOW {
    pub dwOSVersionInfoSize: ULONG,
    pub dwMajorVersion: ULONG,
    pub dwMinorVersion: ULONG,
    pub dwBuildNumber: ULONG,
    pub dwPlatformId: ULONG,
    pub szCSDVersion: [WCHAR; 128],
}

/// Reports the version tuple currently hard-coded by the native bootstrap.
///
/// This intentionally preserves the existing C implementation's fixed
/// Windows 10 / build 26100 values; it does not discover the running kernel.
#[no_mangle]
pub unsafe extern "system" fn RtlGetVersion(version: *mut COGNAC_RTL_OSVERSIONINFOW) -> NTSTATUS {
    if version.is_null() {
        return STATUS_INVALID_PARAMETER;
    }

    // SAFETY: A non-null pointer must reference a writable version structure.
    let version = unsafe { &mut *version };
    if (version.dwOSVersionInfoSize as usize) < core::mem::size_of::<COGNAC_RTL_OSVERSIONINFOW>() {
        return STATUS_INVALID_PARAMETER;
    }

    version.dwMajorVersion = 10;
    version.dwMinorVersion = 0;
    version.dwBuildNumber = 26100;
    version.dwPlatformId = 2; // VER_PLATFORM_WIN32_NT
    version.szCSDVersion.fill(0);
    STATUS_SUCCESS
}

/// Returns the PEB pointer from the Windows x64 TEB at GS:[0x60].
#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentPeb() -> PVOID {
    let peb: PVOID;
    // SAFETY: Native startup must install a valid TEB at the GS base.
    unsafe {
        core::arch::asm!(
            "mov {}, gs:[0x60]",
            out(reg) peb,
            options(nostack, preserves_flags, readonly)
        );
    }
    peb
}

/// Writes the fixed NT version tuple to each non-null output pointer.
#[no_mangle]
pub unsafe extern "system" fn RtlGetNtVersionNumbers(
    major: PULONG,
    minor: PULONG,
    build: PULONG,
) {
    if !major.is_null() {
        // SAFETY: Non-null output pointer must be writable.
        unsafe { major.write(10) };
    }
    if !minor.is_null() {
        // SAFETY: Non-null output pointer must be writable.
        unsafe { minor.write(0) };
    }
    if !build.is_null() {
        // SAFETY: Non-null output pointer must be writable.
        unsafe { build.write(26100) };
    }
}

/// Maps the small set of NTSTATUS values handled by the native C bootstrap.
// Canonical C ABI export is implemented in rtl_error.rs.
pub extern "system" fn RtlNtStatusToDosError(status: NTSTATUS) -> ULONG {
    match status as ULONG {
        0x0000_0000 => ERROR_SUCCESS,
        0xC000_0002 => ERROR_CALL_NOT_IMPLEMENTED,
        0xC000_0005 => ERROR_NOACCESS,
        0xC000_0008 => ERROR_INVALID_HANDLE,
        0xC000_000D => ERROR_INVALID_PARAMETER,
        0xC000_0017 => ERROR_NOT_ENOUGH_MEMORY,
        0xC000_0022 => ERROR_ACCESS_DENIED,
        0xC000_0023 => ERROR_INSUFFICIENT_BUFFER,
        0xC000_0034 => ERROR_FILE_NOT_FOUND,
        _ => ERROR_MR_MID_NOT_FOUND,
    }
}

/// Converts NTSTATUS without relying on TEB state.
// Canonical C ABI export is implemented in rtl_error.rs.
pub extern "system" fn RtlNtStatusToDosErrorNoTeb(status: NTSTATUS) -> ULONG {
    RtlNtStatusToDosError(status)
}

/// Reads TEB.LastErrorValue from GS:[0x68].
// Internal implementation retained for module callers; the canonical C ABI
// export is defined in rtl_error.rs until the Rust exports are consolidated.
pub unsafe extern "system" fn RtlGetLastWin32Error() -> ULONG {
    let value: ULONG;
    // SAFETY: Requires the CognacOS launcher to install the expected x64 TEB.
    unsafe {
        core::arch::asm!(
            "mov {}, gs:[0x68]",
            out(reg) value,
            options(nostack, preserves_flags, readonly)
        );
    }
    value
}

/// Writes TEB.LastErrorValue at GS:[0x68].
// Internal implementation retained for module callers; the canonical C ABI
// export is defined in rtl_error.rs until the Rust exports are consolidated.
pub unsafe extern "system" fn RtlSetLastWin32Error(error: ULONG) {
    // SAFETY: Requires a writable TEB at the current GS base.
    unsafe {
        core::arch::asm!(
            "mov gs:[0x68], {}",
            in(reg) error,
            options(nostack, preserves_flags)
        );
    }
}

/// Reads the bootstrap's configured LastStatusValue location, GS:[0x1250].
// Internal implementation retained for module callers; the canonical C ABI
// export is defined in rtl_error.rs until the Rust exports are consolidated.
pub unsafe extern "system" fn RtlGetLastNtStatus() -> NTSTATUS {
    let status: NTSTATUS;
    // SAFETY: Requires the CognacOS launcher to provide this TEB layout.
    unsafe {
        core::arch::asm!(
            "mov {}, gs:[0x1250]",
            out(reg) status,
            options(nostack, preserves_flags, readonly)
        );
    }
    status
}

/// Updates LastErrorValue and LastStatusValue from one NTSTATUS.
// Canonical C ABI export is implemented in rtl_error.rs.
pub unsafe extern "system" fn RtlSetLastWin32ErrorAndNtStatusFromNtStatus(status: NTSTATUS) {
    let error = RtlNtStatusToDosError(status);
    // SAFETY: Preserves the C bootstrap's two GS-relative writes. The second
    // offset is project-specific and must match the TEB layout installed by
    // the CognacOS launcher.
    unsafe {
        core::arch::asm!(
            "mov gs:[0x68], {error:e}",
            "mov gs:[0x1250], {status:e}",
            error = in(reg) error,
            status = in(reg) status,
            options(nostack, preserves_flags)
        );
    }
}

/// Minimal DLL entry point: deliberately performs no initialization.
#[no_mangle]
pub unsafe extern "system" fn DllMain(
    _instance: HINSTANCE,
    _reason: DWORD,
    _reserved: LPVOID,
) -> BOOL {
    1 // TRUE
}

/// Explicit image-relative relocation anchor retained for the PE image.
///
/// The pointer is exported by the C source. The Rust equivalent preserves a
/// data relocation to DllMain so the linker can retain a relocation directory
/// when the image is mapped away from its preferred base.
#[no_mangle]
pub static mut CognacNtdllImageAnchor:
    Option<unsafe extern "system" fn(HINSTANCE, DWORD, LPVOID) -> BOOL> = Some(DllMain);

//! Windows 11 x64 NTDLL loader exports.
//!
//! This module implements the process-local DLL search-directory controls and
//! loader-lock contract using explicit x64 ABI types. Mapping images, resolving
//! dependencies, activation contexts, and loader notifications still require
//! integration with the native image manager and are not emulated here.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::{ffi::c_void, ptr, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};

type NTSTATUS = i32;
type ULONG = u32;
type BOOLEAN = u8;
type PVOID = *mut c_void;
type ULONG_PTR = usize;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008u32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
const STATUS_DLL_NOT_FOUND: NTSTATUS = 0xC000_0135u32 as i32;
const STATUS_RESOURCE_IN_USE: NTSTATUS = 0xC000_0708u32 as i32;

const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: ULONG = 0x0000_0100;
const LOAD_LIBRARY_SEARCH_APPLICATION_DIR: ULONG = 0x0000_0200;
const LOAD_LIBRARY_SEARCH_USER_DIRS: ULONG = 0x0000_0400;
const LOAD_LIBRARY_SEARCH_SYSTEM32: ULONG = 0x0000_0800;
const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: ULONG = 0x0000_1000;
const VALID_SEARCH_FLAGS: ULONG = LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR
    | LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_USER_DIRS
    | LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UNICODE_STRING {
    pub Length: u16,
    pub MaximumLength: u16,
    pub Buffer: *mut u16,
}

const MAX_PATH_UNITS: usize = 512;
const MAX_DLL_DIRECTORIES: usize = 32;
const MAX_DISABLED_CALLOUT_MODULES: usize = 64;

#[derive(Clone, Copy)]
struct PathSlot { len: usize, data: [u16; MAX_PATH_UNITS], active: bool, generation: usize }
impl PathSlot { const EMPTY: Self = Self { len: 0, data: [0; MAX_PATH_UNITS], active: false, generation: 0 }; }
static STATE_LOCK: AtomicBool = AtomicBool::new(false);
static mut DLL_DIRECTORY: PathSlot = PathSlot::EMPTY;
static mut USER_DIRECTORIES: [PathSlot; MAX_DLL_DIRECTORIES] = [PathSlot::EMPTY; MAX_DLL_DIRECTORIES];
static mut DISABLED_CALLOUT_MODULES: [usize; MAX_DISABLED_CALLOUT_MODULES] = [0; MAX_DISABLED_CALLOUT_MODULES];
static mut DEFAULT_SEARCH_FLAGS: ULONG = LOAD_LIBRARY_SEARCH_DEFAULT_DIRS;
static LOADER_LOCK: AtomicBool = AtomicBool::new(false);
static ACTIVE_LOADER_COOKIE: AtomicUsize = AtomicUsize::new(0);
static NEXT_COOKIE: AtomicUsize = AtomicUsize::new(1);

struct StateGuard;
impl StateGuard {
    fn acquire() -> Self {
        while STATE_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for StateGuard { fn drop(&mut self) { STATE_LOCK.store(false, Ordering::Release); } }

unsafe fn copy_absolute_path(source: *const UNICODE_STRING, output: &mut PathSlot) -> NTSTATUS {
    if source.is_null() { return STATUS_INVALID_PARAMETER; }
    let descriptor = unsafe { &*source };
    if (descriptor.Length & 1) != 0 || descriptor.Length > descriptor.MaximumLength
        || descriptor.Length as usize / 2 >= MAX_PATH_UNITS
        || (descriptor.Length != 0 && descriptor.Buffer.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }
    let count = descriptor.Length as usize / 2;
    if count == 0 { *output = PathSlot::EMPTY; output.active = true; return STATUS_SUCCESS; }
    let path = unsafe { core::slice::from_raw_parts(descriptor.Buffer, count) };
    // Windows AddDllDirectory requires an absolute path. Accept drive-rooted and
    // NT/UNC rooted forms; relative paths are rejected.
    let absolute = (count >= 3 && path[1] == b':' as u16 && (path[2] == b'\\' as u16 || path[2] == b'/' as u16))
        || (count >= 2 && (path[0] == b'\\' as u16 || path[0] == b'/' as u16)
            && (path[1] == b'\\' as u16 || path[1] == b'/' as u16));
    if !absolute { return STATUS_INVALID_PARAMETER; }
    *output = PathSlot::EMPTY;
    output.len = count;
    output.data[..count].copy_from_slice(path);
    output.active = true;
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrAddDllDirectory(directory: *const UNICODE_STRING, cookie_out: *mut PVOID) -> NTSTATUS {
    if cookie_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { cookie_out.write(ptr::null_mut()); }
    let mut candidate = PathSlot::EMPTY;
    let status = unsafe { copy_absolute_path(directory, &mut candidate) };
    if status != STATUS_SUCCESS { return status; }
    let _guard = StateGuard::acquire();
    let mut i = 0;
    while i < MAX_DLL_DIRECTORIES {
        if !unsafe { USER_DIRECTORIES[i].active } {
            let id = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed).max(1) & (usize::MAX >> 8);
            candidate.generation = id;
            unsafe { USER_DIRECTORIES[i] = candidate; }
            unsafe { cookie_out.write((id << 8 | (i + 1)) as PVOID); }
            return STATUS_SUCCESS;
        }
        i += 1;
    }
    STATUS_RESOURCE_IN_USE
}

#[no_mangle]
pub unsafe extern "system" fn LdrRemoveDllDirectory(cookie: PVOID) -> NTSTATUS {
    let raw = cookie as usize;
    let slot = raw & 0xff;
    if slot == 0 || slot > MAX_DLL_DIRECTORIES { return STATUS_INVALID_HANDLE; }
    let _guard = StateGuard::acquire();
    let entry = unsafe { &USER_DIRECTORIES[slot - 1] };
    if !entry.active || entry.generation != (raw >> 8) { return STATUS_INVALID_HANDLE; }
    unsafe { USER_DIRECTORIES[slot - 1] = PathSlot::EMPTY; }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetDefaultDllDirectories(flags: ULONG) -> NTSTATUS {
    if flags == 0 || flags & !VALID_SEARCH_FLAGS != 0 { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    unsafe { DEFAULT_SEARCH_FLAGS = flags; }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetDllDirectory(directory: *const UNICODE_STRING) -> NTSTATUS {
    let mut candidate = PathSlot::EMPTY;
    if !directory.is_null() {
        let status = unsafe { copy_absolute_path(directory, &mut candidate) };
        if status != STATUS_SUCCESS { return status; }
    }
    let _guard = StateGuard::acquire();
    unsafe { DLL_DIRECTORY = candidate; }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllDirectory(buffer_bytes: ULONG, buffer: *mut u16, required_bytes: *mut ULONG) -> NTSTATUS {
    if required_bytes.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    let path = unsafe { &DLL_DIRECTORY };
    let bytes = path.len.saturating_mul(2);
    unsafe { required_bytes.write(bytes as ULONG); }
    if bytes == 0 { return STATUS_SUCCESS; }
    if buffer.is_null() || buffer_bytes < bytes as ULONG { return STATUS_BUFFER_TOO_SMALL; }
    unsafe { ptr::copy_nonoverlapping(path.data.as_ptr(), buffer, path.len); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrLockLoaderLock(flags: ULONG, disposition: *mut ULONG, cookie: *mut ULONG_PTR) -> NTSTATUS {
    if flags & !0x3 != 0 || disposition.is_null() || cookie.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { disposition.write(0); cookie.write(0); }
    if flags & 0x2 != 0 {
        if LOADER_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            let token = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed).max(1);
            ACTIVE_LOADER_COOKIE.store(token, Ordering::Release);
            unsafe { disposition.write(1); cookie.write(token); }
        } else { unsafe { disposition.write(2); } }
        return STATUS_SUCCESS;
    }
    while LOADER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let token = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed).max(1);
    ACTIVE_LOADER_COOKIE.store(token, Ordering::Release);
    unsafe { disposition.write(1); cookie.write(token); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnlockLoaderLock(flags: ULONG, cookie: ULONG_PTR) -> NTSTATUS {
    if flags != 0 || cookie == 0 || ACTIVE_LOADER_COOKIE.load(Ordering::Acquire) != cookie { return STATUS_INVALID_PARAMETER; }
    if LOADER_LOCK.compare_exchange(true, false, Ordering::Release, Ordering::Relaxed).is_err() {
        return STATUS_INVALID_PARAMETER;
    }
    ACTIVE_LOADER_COOKIE.store(0, Ordering::Release);
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrDisableThreadCalloutsForDll(module_base: PVOID) -> NTSTATUS {
    if module_base.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    let address = module_base as usize;
    let mut empty = None;
    let mut i = 0;
    while i < MAX_DISABLED_CALLOUT_MODULES {
        let value = unsafe { DISABLED_CALLOUT_MODULES[i] };
        if value == address { return STATUS_SUCCESS; }
        if value == 0 && empty.is_none() { empty = Some(i); }
        i += 1;
    }
    if let Some(index) = empty {
        unsafe { DISABLED_CALLOUT_MODULES[index] = address; }
        STATUS_SUCCESS
    } else { STATUS_RESOURCE_IN_USE }
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllPath(_name: *const UNICODE_STRING, _flags: ULONG, _path: *mut UNICODE_STRING, _path_flags: *mut ULONG) -> NTSTATUS {
    // Correct path construction needs process parameters, application/system
    // directories, API-set and package graph integration. Never return a fake path.
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrAccessResource() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrAddLoadAsDataTable() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrAddRefDll() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrAppxHandleIntegrityFailure() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrCallEnclave() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrControlFlowGuardEnforced() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrCreateEnclave() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrDeleteEnclave() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrEnumerateLoadedModules() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrEnumResources() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFastFailInLoaderCallout() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFindEntryForAddress() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFindResource_U() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFindResourceDirectory_U() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFindResourceEx_U() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFlushAlternateResourceModules() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllFullName() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleByMapping() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleByName() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleEx(
    flags: ULONG,
    dll_path: *const u16,
    dll_characteristics: *mut ULONG,
    dll_name: *const UNICODE_STRING,
    dll_handle: *mut PVOID,
) -> NTSTATUS {
    if flags & !0x3 != 0 || dll_name.is_null() || dll_handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe {
        crate::rtl_loader::LdrGetDllHandle(
            dll_path as PVOID, dll_characteristics, dll_name.cast(), dll_handle,
        )
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetFailureData() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetFileNameFromLoadAsDataTable() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetKnownDllSectionHandle() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetProcedureAddressEx(
    dll_handle: PVOID,
    procedure_name: *const crate::rtl_loader::COGNAC_ANSI_STRING,
    procedure_number: ULONG,
    procedure_address: *mut PVOID,
    flags: ULONG,
) -> NTSTATUS {
    if flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe {
        crate::rtl_loader::LdrGetProcedureAddress(
            dll_handle, procedure_name, procedure_number, procedure_address,
        )
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetProcedureAddressForCaller() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrHotPatchNotify() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrInitializeEnclave() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrInitializeThunk() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrInitShimEngineDynamic() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrIsModuleSxsRedirected() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrLoadAlternateResourceModule() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrLoadAlternateResourceModuleEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrLoadDll() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrLoadEnclaveModule() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrOpenImageFileOptionsKey() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrpResGetMappingSize() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrpResGetResourceDirectory() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrProcessInitializationComplete() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrProcessRelocationBlock() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrProcessRelocationBlockEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileExecutionOptions() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileExecutionOptionsEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileKeyOption() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryModuleServiceTags() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryOptionalDelayLoadedAPI() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryProcessModuleInformation() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrRegisterDllNotification() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrRemoveLoadAsDataTable() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResFindResource() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResFindResourceDirectory() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResGetRCConfig() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResolveDelayLoadedAPI() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResolveDelayLoadsFromDll() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResRelease() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResSearchResource() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrRscIsTypeExist() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSetAppCompatDllRedirectionCallback() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSetDllManifestProber() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSetImplicitPathOptions() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSetMUICacheType() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrShutdownProcess() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrShutdownThread() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrStandardizeSystemPath() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSystemDllInitBlock() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadAlternateResourceModule() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadAlternateResourceModuleEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadDll() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrUnregisterDllNotification() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrUpdatePackageSearchPath() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrVerifyImageMatchesChecksum() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrVerifyImageMatchesChecksumEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn NtLoadDriver() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn NtUnloadDriver() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn RtlComputePrivatizedDllName_U() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn RtlDllShutdownInProgress() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn RtlIsThreadWithinLoaderCallout() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn RtlWnfDllUnloadCallback() -> i32 { STATUS_NOT_IMPLEMENTED }

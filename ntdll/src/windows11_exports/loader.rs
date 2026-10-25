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

const STATUS_NOT_FOUND: NTSTATUS = 0xC000_0225u32 as i32;
const STATUS_PROCEDURE_NOT_FOUND: NTSTATUS = 0xC000_007Au32 as i32;
const STATUS_DLL_INIT_FAILED: NTSTATUS = 0xC000_0142u32 as i32;
const MAX_LOADER_ENTRIES: usize = 128;
const MAX_REF_RECORDS: usize = 128;

#[repr(C)]
#[derive(Clone, Copy)]
struct LdrUnicodeString { Length: u16, MaximumLength: u16, Buffer: *mut u16 }

#[derive(Clone, Copy)]
struct ModuleRef { base: usize, references: u32, pinned: bool }
impl ModuleRef { const EMPTY: Self = Self { base: 0, references: 0, pinned: false }; }
static MODULE_REF_LOCK: AtomicBool = AtomicBool::new(false);
static mut MODULE_REFS: [ModuleRef; MAX_REF_RECORDS] = [ModuleRef::EMPTY; MAX_REF_RECORDS];

struct ModuleRefGuard;
impl ModuleRefGuard {
    fn acquire() -> Self {
        while MODULE_REF_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for ModuleRefGuard { fn drop(&mut self) { MODULE_REF_LOCK.store(false, Ordering::Release); } }

unsafe fn current_peb_ldr_head() -> Option<*const u8> {
    let peb: *const u8;
    unsafe { core::arch::asm!("mov {}, gs:[0x60]", out(reg) peb, options(nostack, preserves_flags, readonly)); }
    if peb.is_null() { return None; }
    let ldr = unsafe { ptr::read_unaligned(peb.add(0x18).cast::<usize>()) } as *const u8;
    if ldr.is_null() { return None; }
    Some(unsafe { ldr.add(0x10) })
}

unsafe fn find_entry_by_base(base: usize) -> Option<*mut u8> {
    let head = unsafe { current_peb_ldr_head()? };
    let head_addr = head as usize;
    let mut link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let mut count = 0usize;
    while link != 0 && link != head_addr && count < MAX_LOADER_ENTRIES {
        let entry = link as *mut u8;
        if unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) } == base { return Some(entry); }
        link = unsafe { ptr::read_unaligned(entry.cast::<usize>()) };
        count += 1;
    }
    None
}

unsafe fn entry_name_matches(entry: *const u8, name: *const UNICODE_STRING, full: bool) -> bool {
    if name.is_null() { return false; }
    let wanted = unsafe { &*name };
    if wanted.Length & 1 != 0 || wanted.Length > wanted.MaximumLength
        || (wanted.Length != 0 && wanted.Buffer.is_null()) { return false; }
    let offset = if full { 0x48 } else { 0x58 };
    let length = unsafe { ptr::read_unaligned(entry.add(offset).cast::<u16>()) };
    let buffer = unsafe { ptr::read_unaligned(entry.add(offset + 8).cast::<usize>()) } as *const u16;
    if length != wanted.Length || length & 1 != 0 || (length != 0 && (buffer.is_null() || wanted.Buffer.is_null())) { return false; }
    for i in 0..(length as usize / 2) {
        let a = unsafe { ptr::read_unaligned(buffer.add(i)) };
        let b = unsafe { ptr::read_unaligned(wanted.Buffer.add(i)) };
        let fold = |v: u16| if (b'A' as u16..=b'Z' as u16).contains(&v) { v + 32 } else { v };
        if fold(a) != fold(b) { return false; }
    }
    true
}

unsafe fn find_entry_by_name(base_name: *const UNICODE_STRING, full_name: *const UNICODE_STRING) -> Option<*mut u8> {
    let head = unsafe { current_peb_ldr_head()? };
    let head_addr = head as usize;
    let mut link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let mut count = 0usize;
    while link != 0 && link != head_addr && count < MAX_LOADER_ENTRIES {
        let entry = link as *mut u8;
        let base_match = base_name.is_null() || unsafe { entry_name_matches(entry, base_name, false) };
        let full_match = full_name.is_null() || unsafe { entry_name_matches(entry, full_name, true) };
        if base_match && full_match { return Some(entry); }
        link = unsafe { ptr::read_unaligned(entry.cast::<usize>()) };
        count += 1;
    }
    None
}

unsafe fn track_module_reference(base: usize, pin: bool) -> NTSTATUS {
    if base == 0 || unsafe { find_entry_by_base(base) }.is_none() { return STATUS_DLL_NOT_FOUND; }
    let _guard = ModuleRefGuard::acquire();
    let records = core::ptr::addr_of_mut!(MODULE_REFS).cast::<ModuleRef>();
    let mut empty: Option<usize> = None;
    for i in 0..MAX_REF_RECORDS {
        let record = unsafe { &mut *records.add(i) };
        if record.base == base {
            if pin { record.pinned = true; }
            if !record.pinned {
                let Some(next) = record.references.checked_add(1) else { return STATUS_RESOURCE_IN_USE; };
                record.references = next;
            }
            return STATUS_SUCCESS;
        }
        if record.base == 0 && empty.is_none() { empty = Some(i); }
    }
    let Some(i) = empty else { return STATUS_RESOURCE_IN_USE; };
    unsafe { *records.add(i) = ModuleRef { base, references: 2, pinned: pin }; }
    STATUS_SUCCESS
}


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
pub unsafe extern "system" fn LdrGetDllDirectory(path_name: *mut UNICODE_STRING) -> NTSTATUS {
    if path_name.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    let path = unsafe { &DLL_DIRECTORY };
    let bytes = path.len.saturating_mul(2);
    if bytes > u16::MAX as usize { return STATUS_BUFFER_TOO_SMALL; }
    let descriptor = unsafe { &mut *path_name };
    if bytes != 0 && (descriptor.Buffer.is_null() || (descriptor.MaximumLength as usize) < bytes) {
        descriptor.Length = bytes as u16;
        return STATUS_BUFFER_TOO_SMALL;
    }
    if bytes != 0 { unsafe { ptr::copy_nonoverlapping(path.data.as_ptr(), descriptor.Buffer, path.len); } }
    descriptor.Length = bytes as u16;
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
pub unsafe extern "system" fn LdrGetDllPath(
    _dll_name: *const u16, _flags: ULONG, _dll_path: *mut *mut u16, _search_paths: *mut *mut u16,
) -> NTSTATUS {
    // The documented ABI returns allocated search-path strings. Until the process
    // heap and package/application path providers are connected, fail closed.
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrAccessResource() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrAddLoadAsDataTable() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrAddRefDll(flags: ULONG, dll_handle: PVOID) -> NTSTATUS {
    if flags & !1 != 0 || dll_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { track_module_reference(dll_handle as usize, flags & 1 != 0) }
}

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

#[repr(C)]
pub struct LDR_DATA_TABLE_ENTRY_PREFIX {
    pub InLoadOrderLinks: [usize; 2], pub InMemoryOrderLinks: [usize; 2],
    pub InInitializationOrderLinks: [usize; 2], pub DllBase: PVOID, pub EntryPoint: PVOID,
    pub SizeOfImage: ULONG, pub _padding: ULONG,
}
pub type PLDR_ENUM_CALLBACK = Option<unsafe extern "system" fn(*const LDR_DATA_TABLE_ENTRY_PREFIX, PVOID, *mut BOOLEAN)>;
#[no_mangle]
pub unsafe extern "system" fn LdrEnumerateLoadedModules(flags: ULONG, callback: PLDR_ENUM_CALLBACK, context: PVOID) -> NTSTATUS {
    if flags != 0 || callback.is_none() { return STATUS_INVALID_PARAMETER; }
    let Some(head) = (unsafe { current_peb_ldr_head() }) else { return STATUS_DLL_NOT_FOUND; };
    let head_addr = head as usize;
    let mut link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let mut count = 0usize;
    let callback = callback.unwrap();
    while link != 0 && link != head_addr && count < MAX_LOADER_ENTRIES {
        let entry = link as *const LDR_DATA_TABLE_ENTRY_PREFIX;
        let mut stop: BOOLEAN = 0;
        unsafe { callback(entry, context, &mut stop); }
        if stop != 0 { return STATUS_SUCCESS; }
        link = unsafe { ptr::read_unaligned(link as *const usize) };
        count += 1;
    }
    if count == MAX_LOADER_ENTRIES { STATUS_INVALID_PARAMETER } else { STATUS_SUCCESS }
}

#[no_mangle]
pub unsafe extern "system" fn LdrEnumResources() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFastFailInLoaderCallout() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFindEntryForAddress(address: PVOID, entry_out: *mut PVOID) -> NTSTATUS {
    if address.is_null() || entry_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { entry_out.write(ptr::null_mut()); }
    let Some(head) = (unsafe { current_peb_ldr_head() }) else { return STATUS_DLL_NOT_FOUND; };
    let head_addr = head as usize;
    let mut link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let target = address as usize;
    let mut count = 0usize;
    while link != 0 && link != head_addr && count < MAX_LOADER_ENTRIES {
        let entry = link as *mut u8;
        let base = unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) };
        let size = unsafe { ptr::read_unaligned(entry.add(0x40).cast::<u32>()) } as usize;
        if size != 0 && target >= base && target.checked_sub(base).is_some_and(|offset| offset < size) {
            unsafe { entry_out.write(entry.cast()); }
            return STATUS_SUCCESS;
        }
        link = unsafe { ptr::read_unaligned(entry.cast::<usize>()) };
        count += 1;
    }
    STATUS_NOT_FOUND
}

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
pub unsafe extern "system" fn LdrGetDllHandleByMapping(mapping_base: PVOID, entry_out: *mut PVOID) -> NTSTATUS {
    if mapping_base.is_null() || entry_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { entry_out.write(ptr::null_mut()); }
    match unsafe { find_entry_by_base(mapping_base as usize) } {
        Some(entry) => { let base = unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) }; unsafe { entry_out.write(base as PVOID); } STATUS_SUCCESS },
        None => STATUS_DLL_NOT_FOUND,
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllFullName(dll_handle: PVOID, full_dll_name: *mut UNICODE_STRING) -> NTSTATUS {
    if dll_handle.is_null() || full_dll_name.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(entry) = (unsafe { find_entry_by_base(dll_handle as usize) }) else { return STATUS_DLL_NOT_FOUND; };
    let source_length = unsafe { ptr::read_unaligned(entry.add(0x48).cast::<u16>()) };
    let source_buffer = unsafe { ptr::read_unaligned(entry.add(0x50).cast::<usize>()) } as *const u16;
    if source_length & 1 != 0 || (source_length != 0 && source_buffer.is_null()) { return STATUS_INVALID_PARAMETER; }
    let output = unsafe { &mut *full_dll_name };
    if output.Buffer.is_null() || output.MaximumLength < source_length {
        output.Length = source_length;
        return STATUS_BUFFER_TOO_SMALL;
    }
    unsafe { ptr::copy_nonoverlapping(source_buffer, output.Buffer, source_length as usize / 2); }
    output.Length = source_length;
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleByName(
    base_name: *const UNICODE_STRING, full_name: *const UNICODE_STRING, entry_out: *mut PVOID,
) -> NTSTATUS {
    if (base_name.is_null() && full_name.is_null()) || entry_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { entry_out.write(ptr::null_mut()); }
    match unsafe { find_entry_by_name(base_name, full_name) } {
        Some(entry) => { unsafe { entry_out.write(entry.cast()); } STATUS_SUCCESS },
        None => STATUS_DLL_NOT_FOUND,
    }
}

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
    let status = unsafe {
        crate::rtl_loader::LdrGetDllHandle(
            dll_path as PVOID, dll_characteristics, dll_name.cast(), dll_handle,
        )
    };
    if status != STATUS_SUCCESS || (flags & 1 != 0 && flags & 2 == 0) { return status; }
    unsafe { track_module_reference(*dll_handle as usize, flags & 2 != 0) }
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
pub unsafe extern "system" fn LdrGetProcedureAddressForCaller(
    dll_handle: PVOID,
    procedure_name: *const crate::rtl_loader::COGNAC_ANSI_STRING,
    procedure_number: ULONG,
    procedure_address: *mut PVOID,
    flags: ULONG,
    _callback: *mut PVOID,
) -> NTSTATUS {
    if flags & !1 != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { crate::rtl_loader::LdrGetProcedureAddress(dll_handle, procedure_name, procedure_number, procedure_address) }
}

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
pub unsafe extern "system" fn LdrLoadDll(
    dll_path: *const u16, dll_characteristics: *mut ULONG,
    dll_name: *const UNICODE_STRING, dll_handle: *mut PVOID,
) -> NTSTATUS {
    if dll_name.is_null() || dll_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { dll_handle.write(ptr::null_mut()); }
    // Reuse an existing mapping without pretending that a missing module was
    // loaded. New-image mapping requires the not-yet-implemented native section
    // mapping/loader transaction bridge; fail closed until that exists.
    let status = unsafe { crate::rtl_loader::LdrGetDllHandle(dll_path as PVOID, dll_characteristics, dll_name.cast(), dll_handle) };
    if status == STATUS_SUCCESS { return unsafe { track_module_reference(*dll_handle as usize, false) }; }
    STATUS_NOT_IMPLEMENTED
}

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
pub unsafe extern "system" fn LdrUnloadDll(dll_handle: PVOID) -> NTSTATUS {
    if dll_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    if unsafe { find_entry_by_base(dll_handle as usize) }.is_none() { return STATUS_DLL_NOT_FOUND; }
    let _guard = ModuleRefGuard::acquire();
    let records = core::ptr::addr_of_mut!(MODULE_REFS).cast::<ModuleRef>();
    for i in 0..MAX_REF_RECORDS {
        let record = unsafe { &mut *records.add(i) };
        if record.base == dll_handle as usize {
            if record.pinned { return STATUS_SUCCESS; }
            if record.references > 1 { record.references -= 1; return STATUS_SUCCESS; }
            // Do not unlink the PEB entry or free memory without kernel-backed
            // unmap and dependency rundown services.
            return STATUS_NOT_IMPLEMENTED;
        }
    }
    STATUS_NOT_IMPLEMENTED
}

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

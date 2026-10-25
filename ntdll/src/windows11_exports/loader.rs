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
static DLL_SHUTDOWN: AtomicBool = AtomicBool::new(false);

const STATUS_NOT_FOUND: NTSTATUS = 0xC000_0225u32 as i32;
const STATUS_PROCEDURE_NOT_FOUND: NTSTATUS = 0xC000_007Au32 as i32;
const STATUS_DLL_INIT_FAILED: NTSTATUS = 0xC000_0142u32 as i32;
const MAX_LOADER_ENTRIES: usize = 128;
const MAX_REF_RECORDS: usize = 128;

const IMAGE_DIRECTORY_ENTRY_RESOURCE: usize = 2;
const MAX_RESOURCE_DEPTH: usize = 3;
const MAX_RESOURCE_ENTRIES: usize = 4096;

#[repr(C)]
pub struct LDR_RESOURCE_INFO { pub Type: ULONG_PTR, pub Name: ULONG_PTR, pub Language: u32 }

#[repr(C)]
pub struct IMAGE_RESOURCE_DATA_ENTRY { pub OffsetToData: u32, pub Size: u32, pub CodePage: u32, pub Reserved: u32 }

#[repr(C)]
pub struct LDR_RESOURCE_DIRECTORY_ENTRY { pub Data: [u32; 2] }

unsafe fn mapped_image_info(module: PVOID) -> Result<(usize, usize, usize), NTSTATUS> {
    if module.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let base = module as *const u8;
    if unsafe { ptr::read_unaligned(base.cast::<u16>()) } != 0x5a4d { return Err(0xC000_007Bu32 as i32); }
    let nt = unsafe { ptr::read_unaligned(base.add(0x3c).cast::<u32>()) } as usize;
    if nt > 0x100000 || unsafe { ptr::read_unaligned(base.add(nt).cast::<u32>()) } != 0x0000_4550 {
        return Err(0xC000_007Bu32 as i32);
    }
    let coff = nt + 4;
    if unsafe { ptr::read_unaligned(base.add(coff).cast::<u16>()) } != 0x8664 { return Err(0xC000_007Bu32 as i32); }
    let optional_size = unsafe { ptr::read_unaligned(base.add(coff + 16).cast::<u16>()) } as usize;
    let opt = coff + 20;
    if optional_size < 112 || unsafe { ptr::read_unaligned(base.add(opt).cast::<u16>()) } != 0x20b {
        return Err(0xC000_007Bu32 as i32);
    }
    let image_size = unsafe { ptr::read_unaligned(base.add(opt + 56).cast::<u32>()) } as usize;
    let dirs = unsafe { ptr::read_unaligned(base.add(opt + 108).cast::<u32>()) } as usize;
    if image_size < 0x1000 || image_size > 0x4000_0000 { return Err(0xC000_007Bu32 as i32); }
    if dirs <= IMAGE_DIRECTORY_ENTRY_RESOURCE { return Ok((base as usize, image_size, 0)); }
    if optional_size < 112 + (IMAGE_DIRECTORY_ENTRY_RESOURCE + 1) * 8 { return Err(0xC000_007Bu32 as i32); }
    let rva = unsafe { ptr::read_unaligned(base.add(opt + 112 + IMAGE_DIRECTORY_ENTRY_RESOURCE * 8).cast::<u32>()) } as usize;
    let size = unsafe { ptr::read_unaligned(base.add(opt + 112 + IMAGE_DIRECTORY_ENTRY_RESOURCE * 8 + 4).cast::<u32>()) } as usize;
    if rva == 0 || size < 16 || rva.checked_add(size).filter(|end| *end <= image_size).is_none() {
        return Ok((base as usize, image_size, 0));
    }
    Ok((base as usize, image_size, (size << 32) | rva))
}

unsafe fn resource_key_matches(base: usize, image_size: usize, resource_rva: usize, resource_size: usize, encoded: u32, wanted: ULONG_PTR) -> bool {
    if encoded & 0x8000_0000 != 0 {
        if wanted <= 0xffff { return false; }
        let name_off = (encoded & 0x7fff_ffff) as usize;
        if name_off.checked_add(2).filter(|x| *x <= resource_size).is_none() { return false; }
        let name_ptr = (base + resource_rva + name_off) as *const u16;
        let count = unsafe { ptr::read_unaligned(name_ptr) } as usize;
        if count > 512 || name_off.checked_add(2 + count * 2).filter(|x| *x <= resource_size).is_none() { return false; }
        let wanted_ptr = wanted as *const u16;
        for i in 0..count {
            let a = unsafe { ptr::read_unaligned((name_ptr.add(1)).add(i)) };
            let b = unsafe { ptr::read_unaligned(wanted_ptr.add(i)) };
            if a != b { return false; }
        }
        unsafe { ptr::read_unaligned(wanted_ptr.add(count)) == 0 }
    } else {
        wanted <= 0xffff && (encoded as u16) == wanted as u16
    }
}

unsafe fn find_resource_entry(module: PVOID, info: *const LDR_RESOURCE_INFO, level: ULONG, directory_only: bool) -> Result<*mut IMAGE_RESOURCE_DATA_ENTRY, NTSTATUS> {
    if info.is_null() || !(1..=3).contains(&level) { return Err(STATUS_INVALID_PARAMETER); }
    let (base, image_size, packed) = unsafe { mapped_image_info(module)? };
    let resource_rva = packed as u32 as usize;
    let resource_size = (packed >> 32) as usize;
    if resource_rva == 0 { return Err(0xC000_008Au32 as i32); }
    let info = unsafe { &*info };
    let keys = [info.Type, info.Name, info.Language as ULONG_PTR];
    let mut directory_off = 0usize;
    for depth in 0..level as usize {
        if directory_off.checked_add(16).filter(|end| *end <= resource_size).is_none() { return Err(0xC000_007Bu32 as i32); }
        let dir = (base + resource_rva + directory_off) as *const u8;
        let named = unsafe { ptr::read_unaligned(dir.add(12).cast::<u16>()) } as usize;
        let ids = unsafe { ptr::read_unaligned(dir.add(14).cast::<u16>()) } as usize;
        let count = named.checked_add(ids).filter(|n| *n <= MAX_RESOURCE_ENTRIES).ok_or(0xC000_007Bu32 as i32)?;
        if directory_off.checked_add(16 + count * 8).filter(|end| *end <= resource_size).is_none() { return Err(0xC000_007Bu32 as i32); }
        let mut found = None;
        for i in 0..count {
            let ent = dir.add(16 + i * 8);
            let name = unsafe { ptr::read_unaligned(ent.cast::<u32>()) };
            let child = unsafe { ptr::read_unaligned(ent.add(4).cast::<u32>()) };
            if unsafe { resource_key_matches(base, image_size, resource_rva, resource_size, name, keys[depth]) } {
                found = Some(child); break;
            }
        }
        let child = found.ok_or(match depth { 0 => 0xC000_008Au32 as i32, 1 => 0xC000_008Bu32 as i32, _ => 0xC000_008Cu32 as i32 })?;
        if depth + 1 < level as usize {
            if child & 0x8000_0000 == 0 { return Err(0xC000_007Bu32 as i32); }
            directory_off = (child & 0x7fff_ffff) as usize;
        } else {
            if directory_only {
                if child & 0x8000_0000 == 0 { return Err(0xC000_007Bu32 as i32); }
                return Ok((base + resource_rva + (child & 0x7fff_ffff) as usize) as *mut IMAGE_RESOURCE_DATA_ENTRY);
            }
            if child & 0x8000_0000 != 0 { return Err(0xC000_007Bu32 as i32); }
            let data_off = (child & 0x7fff_ffff) as usize;
            if data_off.checked_add(16).filter(|end| *end <= resource_size).is_none() { return Err(0xC000_007Bu32 as i32); }
            let entry = (base + resource_rva + data_off) as *mut IMAGE_RESOURCE_DATA_ENTRY;
            let data_rva = unsafe { ptr::read_unaligned(ptr::addr_of!((*entry).OffsetToData)) } as usize;
            let size = unsafe { ptr::read_unaligned(ptr::addr_of!((*entry).Size)) } as usize;
            if data_rva.checked_add(size).filter(|end| *end <= image_size).is_none() { return Err(0xC000_007Bu32 as i32); }
            return Ok(entry);
        }
    }
    Err(0xC000_007Bu32 as i32)
}


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
    // This native API accepts a UNICODE_STRING descriptor, not a separate
    // caller buffer/length pair. Return a borrowed pointer to process-local
    // loader storage, matching the one-argument ABI.
    descriptor.Length = bytes as u16;
    descriptor.MaximumLength = bytes as u16;
    descriptor.Buffer = if path.len == 0 { ptr::null_mut() } else { path.data.as_ptr() as *mut u16 };
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
pub unsafe extern "system" fn LdrFlushAlternateResourceModules() -> i32 { STATUS_NOT_IMPLEMENTED }

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
    let source_maximum = unsafe { ptr::read_unaligned(entry.add(0x4a).cast::<u16>()) };
    let source_buffer = unsafe { ptr::read_unaligned(entry.add(0x50).cast::<usize>()) } as *mut u16;
    if source_length & 1 != 0 || source_length > source_maximum
        || (source_length != 0 && source_buffer.is_null()) { return STATUS_INVALID_PARAMETER; }
    let output = unsafe { &mut *full_dll_name };
    // The returned descriptor borrows the loader-owned name; it does not copy
    // into a caller-owned buffer or transfer ownership of the string.
    output.Length = source_length;
    output.MaximumLength = source_maximum;
    output.Buffer = source_buffer;
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleByName(
    base_name: *const UNICODE_STRING, full_name: *const UNICODE_STRING, dll_handle: *mut PVOID,
) -> NTSTATUS {
    if (base_name.is_null() && full_name.is_null()) || dll_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { dll_handle.write(ptr::null_mut()); }
    match unsafe { find_entry_by_name(base_name, full_name) } {
        Some(entry) => {
            let base = unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) };
            unsafe { dll_handle.write(base as PVOID); }
            STATUS_SUCCESS
        },
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
    _caller_address: PVOID,
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

static PROCESS_INITIALIZATION_COMPLETE: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub unsafe extern "system" fn LdrProcessInitializationComplete() -> NTSTATUS {
    PROCESS_INITIALIZATION_COMPLETE.store(true, Ordering::Release);
    STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn LdrFindResource_U(
    module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    resource_data_entry: *mut *mut IMAGE_RESOURCE_DATA_ENTRY,
) -> NTSTATUS {
    if resource_data_entry.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { resource_data_entry.write(ptr::null_mut()); }
    match unsafe { find_resource_entry(module, resource_info, level, false) } {
        Ok(entry) => { unsafe { resource_data_entry.write(entry); } STATUS_SUCCESS },
        Err(status) => status,
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrFindResourceDirectory_U(
    module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    resource_directory: *mut *mut c_void,
) -> NTSTATUS {
    if resource_directory.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { resource_directory.write(ptr::null_mut()); }
    match unsafe { find_resource_entry(module, resource_info, level, true) } {
        Ok(entry) => { unsafe { resource_directory.write(entry.cast()); } STATUS_SUCCESS },
        Err(status) => status,
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrFindResourceEx_U(
    flags: ULONG, module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    resource_data_entry: *mut *mut IMAGE_RESOURCE_DATA_ENTRY,
) -> NTSTATUS {
    if flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { LdrFindResource_U(module, resource_info, level, resource_data_entry) }
}

#[no_mangle]
pub unsafe extern "system" fn LdrAccessResource(
    module: PVOID, resource_data_entry: *const IMAGE_RESOURCE_DATA_ENTRY,
    resource_address: *mut *mut c_void, resource_size: *mut ULONG,
) -> NTSTATUS {
    if module.is_null() || resource_data_entry.is_null() || resource_address.is_null() || resource_size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { resource_address.write(ptr::null_mut()); resource_size.write(0); }
    let (base, image_size, packed) = match unsafe { mapped_image_info(module) } { Ok(v) => v, Err(e) => return e };
    let resource_rva = packed as u32 as usize;
    let resource_size = (packed >> 32) as usize;
    let entry_address = resource_data_entry as usize;
    let image_base = base;
    let expected_start = image_base.saturating_add(resource_rva);
    let expected_end = expected_start.checked_add(resource_size);
    if resource_rva == 0 || expected_end.map_or(true, |end| entry_address < expected_start || entry_address.checked_add(16).map_or(true, |entry_end| entry_end > end)) {
        return STATUS_INVALID_PARAMETER;
    }
    let rva = unsafe { ptr::read_unaligned(ptr::addr_of!((*resource_data_entry).OffsetToData)) } as usize;
    let resource_bytes = unsafe { ptr::read_unaligned(ptr::addr_of!((*resource_data_entry).Size)) } as usize;
    if rva.checked_add(resource_bytes).filter(|end| *end <= image_size).is_none() { return 0xC000_007Bu32 as i32; }
    unsafe { resource_address.write((base + rva) as *mut c_void); resource_size.write(resource_bytes as ULONG); }
    STATUS_SUCCESS
}

const MAX_DLL_NOTIFICATIONS: usize = 32;
#[derive(Clone, Copy)]
struct Notification { cookie: usize, callback: usize, context: usize }
impl Notification { const EMPTY: Self = Self { cookie: 0, callback: 0, context: 0 }; }
static mut DLL_NOTIFICATIONS: [Notification; MAX_DLL_NOTIFICATIONS] = [Notification::EMPTY; MAX_DLL_NOTIFICATIONS];

#[no_mangle]
pub unsafe extern "system" fn LdrRegisterDllNotification(
    flags: ULONG, callback: Option<unsafe extern "system" fn(ULONG, *const c_void, PVOID)>,
    context: PVOID, cookie_out: *mut PVOID,
) -> NTSTATUS {
    if flags != 0 || callback.is_none() || cookie_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { cookie_out.write(ptr::null_mut()); }
    let _guard = StateGuard::acquire();
    let entries = core::ptr::addr_of_mut!(DLL_NOTIFICATIONS).cast::<Notification>();
    for i in 0..MAX_DLL_NOTIFICATIONS {
        let entry = unsafe { &mut *entries.add(i) };
        if entry.cookie == 0 {
            let cookie = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed).max(1);
            *entry = Notification { cookie, callback: callback.unwrap() as usize, context: context as usize };
            unsafe { cookie_out.write(cookie as PVOID); }
            return STATUS_SUCCESS;
        }
    }
    STATUS_RESOURCE_IN_USE
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnregisterDllNotification(cookie: PVOID) -> NTSTATUS {
    let value = cookie as usize;
    if value == 0 { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    let entries = core::ptr::addr_of_mut!(DLL_NOTIFICATIONS).cast::<Notification>();
    for i in 0..MAX_DLL_NOTIFICATIONS {
        let entry = unsafe { &mut *entries.add(i) };
        if entry.cookie == value { *entry = Notification::EMPTY; return STATUS_SUCCESS; }
    }
    STATUS_INVALID_HANDLE
}

#[no_mangle]
pub unsafe extern "system" fn RtlDllShutdownInProgress() -> BOOLEAN {
    DLL_SHUTDOWN.load(Ordering::Acquire) as BOOLEAN
}


/// Apply the AMD64 base-relocation entries for one image page.
#[no_mangle]
pub unsafe extern "system" fn LdrProcessRelocationBlock(
    va: ULONG_PTR, size_of_block: ULONG, next_offset: *mut u16, diff: isize,
) -> *mut u16 {
    if va == 0 || size_of_block & 1 != 0 || (size_of_block != 0 && next_offset.is_null()) {
        return ptr::null_mut();
    }
    if size_of_block == 0 { return next_offset; }
    let count = (size_of_block / 2) as usize;
    for index in 0..count {
        let item = unsafe { ptr::read_unaligned(next_offset.add(index)) };
        let kind = item >> 12;
        let offset = (item & 0x0fff) as usize;
        if kind == 0 { continue; } // IMAGE_REL_BASED_ABSOLUTE padding
        if kind != 10 { return ptr::null_mut(); } // IMAGE_REL_BASED_DIR64 only on AMD64
        let Some(address) = va.checked_add(offset) else { return ptr::null_mut(); };
        let location = address as *mut u64;
        let value = unsafe { ptr::read_unaligned(location) };
        unsafe { ptr::write_unaligned(location, value.wrapping_add(diff as u64)); }
    }
    unsafe { next_offset.add(count) }
}

#[no_mangle]
pub unsafe extern "system" fn LdrProcessRelocationBlockEx(
    machine: ULONG, va: ULONG_PTR, size_of_block: ULONG, next_offset: *mut u16, diff: isize,
) -> *mut u16 {
    if machine != 0x8664 { return ptr::null_mut(); } // IMAGE_FILE_MACHINE_AMD64
    unsafe { LdrProcessRelocationBlock(va, size_of_block, next_offset, diff) }
}



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

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RTL_PROCESS_MODULE_INFORMATION {
    pub Section: PVOID,
    pub MappedBase: PVOID,
    pub ImageBase: PVOID,
    pub ImageSize: ULONG,
    pub Flags: ULONG,
    pub LoadOrderIndex: u16,
    pub InitOrderIndex: u16,
    pub LoadCount: u16,
    pub OffsetToFileName: u16,
    pub FullPathName: [u8; 256],
}
#[repr(C)]
pub struct RTL_PROCESS_MODULES {
    pub NumberOfModules: ULONG,
    pub Modules: [RTL_PROCESS_MODULE_INFORMATION; 1],
}
const RTL_PROCESS_MODULE_INFORMATION_SIZE: usize = core::mem::size_of::<RTL_PROCESS_MODULE_INFORMATION>();

#[no_mangle]
pub unsafe extern "system" fn LdrQueryProcessModuleInformation(
    module_information: *mut RTL_PROCESS_MODULES,
    size: ULONG,
    returned_size: *mut ULONG,
) -> NTSTATUS {
    if returned_size.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(head) = (unsafe { current_peb_ldr_head() }) else {
        unsafe { returned_size.write(0); }
        return STATUS_DLL_NOT_FOUND;
    };
    let head_addr = head as usize;
    let mut link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let mut count = 0usize;
    while link != 0 && link != head_addr && count < MAX_LOADER_ENTRIES {
        count += 1;
        link = unsafe { ptr::read_unaligned((link as *const u8).cast::<usize>()) };
    }
    if count == MAX_LOADER_ENTRIES { return STATUS_INVALID_PARAMETER; }
    let module_array_offset = core::mem::offset_of!(RTL_PROCESS_MODULES, Modules);
    let required = module_array_offset
        .checked_add(count.checked_mul(RTL_PROCESS_MODULE_INFORMATION_SIZE).unwrap_or(usize::MAX))
        .filter(|n| *n <= u32::MAX as usize).ok_or(STATUS_INVALID_PARAMETER);
    let required = match required { Ok(n) => n, Err(e) => return e };
    unsafe { returned_size.write(required as ULONG); }
    if module_information.is_null() || (size as usize) < required { return STATUS_BUFFER_TOO_SMALL; }
    unsafe { ptr::write_unaligned(ptr::addr_of_mut!((*module_information).NumberOfModules), count as ULONG); }
    link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let output = unsafe { (module_information.cast::<u8>().add(module_array_offset)).cast::<RTL_PROCESS_MODULE_INFORMATION>() };
    let mut index = 0usize;
    while link != 0 && link != head_addr && index < count {
        let entry = link as *const u8;
        let base = unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) };
        let image_size = unsafe { ptr::read_unaligned(entry.add(0x40).cast::<ULONG>()) };
        let name_length = unsafe { ptr::read_unaligned(entry.add(0x48).cast::<u16>()) } as usize;
        let name_buffer = unsafe { ptr::read_unaligned(entry.add(0x50).cast::<usize>()) } as *const u16;
        let mut info = RTL_PROCESS_MODULE_INFORMATION {
            Section: ptr::null_mut(), MappedBase: base as PVOID, ImageBase: base as PVOID,
            ImageSize: image_size, Flags: 0, LoadOrderIndex: index as u16, InitOrderIndex: 0,
            LoadCount: 1, OffsetToFileName: 0, FullPathName: [0; 256],
        };
        if name_length & 1 == 0 && name_length != 0 && !name_buffer.is_null() {
            let units = core::cmp::min(name_length / 2, 255);
            for i in 0..units {
                let ch = unsafe { ptr::read_unaligned(name_buffer.add(i)) };
                info.FullPathName[i] = if ch <= 0x7f { ch as u8 } else { b'?' };
                if ch == b'\\' as u16 || ch == b'/' as u16 { info.OffsetToFileName = (i + 1) as u16; }
            }
        }
        unsafe { ptr::write_unaligned(output.add(index), info); }
        link = unsafe { ptr::read_unaligned(entry.cast::<usize>()) };
        index += 1;
    }
    STATUS_SUCCESS
}



#[no_mangle]
pub unsafe extern "system" fn LdrRemoveLoadAsDataTable() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResFindResource() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResFindResourceDirectory() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrResGetRCConfig() -> i32 { STATUS_NOT_IMPLEMENTED }


const IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT: usize = 13;
const IMAGE_ORDINAL_FLAG64: u64 = 1u64 << 63;
const MAX_DELAY_IMPORTS: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IMAGE_DELAYLOAD_DESCRIPTOR {
    pub Attributes: u32,
    pub DllNameRVA: u32,
    pub ModuleHandleRVA: u32,
    pub ImportAddressTableRVA: u32,
    pub ImportNameTableRVA: u32,
    pub BoundImportAddressTableRVA: u32,
    pub UnloadInformationTableRVA: u32,
    pub TimeDateStamp: u32,
}

unsafe fn delay_directory(module: PVOID) -> Result<(usize, usize, usize), NTSTATUS> {
    let (base, image_size, _) = unsafe { mapped_image_info(module) }?;
    let raw = base as *const u8;
    let nt = unsafe { ptr::read_unaligned(raw.add(0x3c).cast::<u32>()) } as usize;
    let optional_size = unsafe { ptr::read_unaligned(raw.add(nt + 4 + 16).cast::<u16>()) } as usize;
    let optional = nt.checked_add(24).ok_or(STATUS_INVALID_PARAMETER)?;
    if optional_size < 112 + (IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT + 1) * 8 {
        return Err(STATUS_NOT_FOUND);
    }
    let directory_count = unsafe { ptr::read_unaligned(raw.add(optional + 108).cast::<u32>()) } as usize;
    if directory_count <= IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT { return Err(STATUS_NOT_FOUND); }
    let directory = optional + 112 + IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT * 8;
    let rva = unsafe { ptr::read_unaligned(raw.add(directory).cast::<u32>()) } as usize;
    let size = unsafe { ptr::read_unaligned(raw.add(directory + 4).cast::<u32>()) } as usize;
    if rva == 0 || size < core::mem::size_of::<IMAGE_DELAYLOAD_DESCRIPTOR>()
        || rva.checked_add(size).is_none_or(|end| end > image_size) {
        return Err(STATUS_NOT_FOUND);
    }
    Ok((base, image_size, rva))
}

unsafe fn image_ascii_string(base: usize, image_size: usize, rva: usize, out: &mut [u8]) -> Result<usize, NTSTATUS> {
    if rva >= image_size || out.is_empty() { return Err(STATUS_INVALID_PARAMETER); }
    for i in 0..core::cmp::min(out.len(), image_size - rva) {
        let ch = unsafe { ptr::read_unaligned((base + rva + i) as *const u8) };
        if ch == 0 { return Ok(i); }
        if !ch.is_ascii() { return Err(STATUS_INVALID_PARAMETER); }
        out[i] = ch;
    }
    Err(STATUS_INVALID_PARAMETER)
}

/// Resolve one delay-import thunk in an already mapped PE32+ image. Dependency
/// mapping is delegated to LdrLoadDll; until that service can map new files, this
/// correctly resolves only dependencies already present in the process loader list.
#[no_mangle]
pub unsafe extern "system" fn LdrResolveDelayLoadedAPI(
    parent_module_base: PVOID,
    descriptor: *const IMAGE_DELAYLOAD_DESCRIPTOR,
    _dll_failure_hook: PVOID,
    _system_failure_hook: PVOID,
    thunk_address: *mut u64,
    flags: ULONG,
) -> PVOID {
    if parent_module_base.is_null() || descriptor.is_null() || thunk_address.is_null() || flags != 0 {
        return ptr::null_mut();
    }
    let (base, image_size, _) = match unsafe { mapped_image_info(parent_module_base) } {
        Ok(v) => v, Err(_) => return ptr::null_mut(),
    };
    let descriptor_address = descriptor as usize;
    if descriptor_address < base || descriptor_address.checked_add(32).is_none_or(|end| end > base + image_size) {
        return ptr::null_mut();
    }
    let d = unsafe { &*descriptor };
    if d.Attributes & 1 == 0 || d.DllNameRVA == 0 || d.ModuleHandleRVA == 0
        || d.ImportAddressTableRVA == 0 || d.ImportNameTableRVA == 0 {
        return ptr::null_mut();
    }
    let rva_ok = |rva: u32, size: usize| (rva as usize).checked_add(size).is_some_and(|end| end <= image_size);
    if !rva_ok(d.DllNameRVA, 1) || !rva_ok(d.ModuleHandleRVA, 8)
        || !rva_ok(d.ImportAddressTableRVA, 8) || !rva_ok(d.ImportNameTableRVA, 8) {
        return ptr::null_mut();
    }
    let iat = base + d.ImportAddressTableRVA as usize;
    let int = base + d.ImportNameTableRVA as usize;
    let target = thunk_address as usize;
    if target < iat || (target - iat) % 8 != 0 || target.checked_add(8).is_none_or(|end| end > base + image_size) {
        return ptr::null_mut();
    }
    let index = (target - iat) / 8;
    let Some(lookup_rva) = (d.ImportNameTableRVA as usize).checked_add(index.saturating_mul(8)) else { return ptr::null_mut(); };
    if lookup_rva.checked_add(8).is_none_or(|end| end > image_size) { return ptr::null_mut(); }
    let thunk = unsafe { ptr::read_unaligned((int + index * 8) as *const u64) };
    if thunk == 0 { return ptr::null_mut(); }

    let module_slot = (base + d.ModuleHandleRVA as usize) as *mut PVOID;
    let mut dll_handle = unsafe { ptr::read_unaligned(module_slot) };
    if dll_handle.is_null() {
        let mut dll_ascii = [0u8; 260];
        let dll_len = match unsafe { image_ascii_string(base, image_size, d.DllNameRVA as usize, &mut dll_ascii) } {
            Ok(n) if n > 0 => n, _ => return ptr::null_mut(),
        };
        let mut dll_utf16 = [0u16; 260];
        for i in 0..dll_len { dll_utf16[i] = dll_ascii[i] as u16; }
        let name = UNICODE_STRING { Length: (dll_len * 2) as u16, MaximumLength: (dll_len * 2) as u16, Buffer: dll_utf16.as_mut_ptr() };
        if unsafe { LdrLoadDll(ptr::null(), ptr::null_mut(), &name, &mut dll_handle) } != STATUS_SUCCESS {
            return ptr::null_mut();
        }
        unsafe { ptr::write_unaligned(module_slot, dll_handle); }
    }

    let mut procedure = ptr::null_mut();
    let status = if thunk & IMAGE_ORDINAL_FLAG64 != 0 {
        unsafe { crate::rtl_loader::LdrGetProcedureAddress(dll_handle, ptr::null(), (thunk & 0xffff) as ULONG, &mut procedure) }
    } else {
        let name_rva = match usize::try_from(thunk) { Ok(v) => v, Err(_) => return ptr::null_mut() };
        if name_rva.checked_add(3).is_none_or(|end| end > image_size) { return ptr::null_mut(); }
        let mut function_ascii = [0u8; 512];
        let function_len = match unsafe { image_ascii_string(base, image_size, name_rva + 2, &mut function_ascii) } {
            Ok(n) if n > 0 => n, _ => return ptr::null_mut(),
        };
        let ansi = crate::rtl_loader::COGNAC_ANSI_STRING {
            Length: function_len as u16, MaximumLength: function_len as u16,
            Buffer: function_ascii.as_mut_ptr(),
        };
        unsafe { crate::rtl_loader::LdrGetProcedureAddress(dll_handle, &ansi, 0, &mut procedure) }
    };
    if status != STATUS_SUCCESS || procedure.is_null() { return ptr::null_mut(); }
    unsafe { ptr::write_unaligned(thunk_address, procedure as u64); }
    procedure
}

#[no_mangle]
pub unsafe extern "system" fn LdrResolveDelayLoadsFromDll(
    parent_module_base: PVOID, dll_name: *const u16,
) -> NTSTATUS {
    if parent_module_base.is_null() || dll_name.is_null() { return STATUS_INVALID_PARAMETER; }
    let (base, image_size, directory_rva) = match unsafe { delay_directory(parent_module_base) } {
        Ok(v) => v, Err(status) => return status,
    };
    let mut wanted = [0u16; 260];
    let mut wanted_len = 0usize;
    while wanted_len < wanted.len() {
        let ch = unsafe { ptr::read_unaligned(dll_name.add(wanted_len)) };
        if ch == 0 { break; }
        wanted[wanted_len] = ch;
        wanted_len += 1;
    }
    if wanted_len == 0 || wanted_len == wanted.len() { return STATUS_INVALID_PARAMETER; }
    let directory_size = unsafe {
        let nt = ptr::read_unaligned((base + 0x3c) as *const u32) as usize;
        let opt = base + nt + 24;
        let dir = opt + 112 + IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT * 8;
        ptr::read_unaligned((dir + 4) as *const u32) as usize
    };
    let count = core::cmp::min(directory_size / 32, MAX_DELAY_IMPORTS);
    let mut matched = false;
    for i in 0..count {
        let descriptor = (base + directory_rva + i * 32) as *const IMAGE_DELAYLOAD_DESCRIPTOR;
        let d = unsafe { &*descriptor };
        if d.Attributes == 0 && d.DllNameRVA == 0 && d.ImportAddressTableRVA == 0 { break; }
        if d.Attributes & 1 == 0 { continue; }
        let mut name = [0u8; 260];
        let len = match unsafe { image_ascii_string(base, image_size, d.DllNameRVA as usize, &mut name) } {
            Ok(n) => n, Err(_) => return STATUS_INVALID_PARAMETER,
        };
        if len != wanted_len || (0..len).any(|j| (name[j] as u16) != wanted[j]) { continue; }
        matched = true;
        if d.ImportAddressTableRVA == 0 || d.ImportNameTableRVA == 0
            || d.ImportAddressTableRVA as usize >= image_size || d.ImportNameTableRVA as usize >= image_size {
            return STATUS_INVALID_PARAMETER;
        }
        let mut resolved = 0usize;
        for thunk_index in 0..MAX_DELAY_IMPORTS {
            let iat_rva = d.ImportAddressTableRVA as usize + thunk_index * 8;
            let int_rva = d.ImportNameTableRVA as usize + thunk_index * 8;
            if iat_rva.checked_add(8).is_none_or(|end| end > image_size)
                || int_rva.checked_add(8).is_none_or(|end| end > image_size) { return STATUS_INVALID_PARAMETER; }
            let lookup = unsafe { ptr::read_unaligned((base + int_rva) as *const u64) };
            if lookup == 0 { break; }
            let address = unsafe { LdrResolveDelayLoadedAPI(parent_module_base, descriptor, ptr::null_mut(), ptr::null_mut(), (base + iat_rva) as *mut u64, 0) };
            if address.is_null() { return STATUS_PROCEDURE_NOT_FOUND; }
            resolved += 1;
        }
        if resolved == MAX_DELAY_IMPORTS { return STATUS_INVALID_PARAMETER; }
    }
    if matched { STATUS_SUCCESS } else { STATUS_RESOURCE_NOT_FOUND }
}

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
pub unsafe extern "system" fn LdrShutdownProcess() -> NTSTATUS {
    DLL_SHUTDOWN.store(true, Ordering::Release);
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrShutdownThread() -> NTSTATUS {
    // Thread-specific TLS/FLS and detach callbacks require a real thread/module
    // runtime. Do not fabricate callbacks; report success only for the no-op path.
    if DLL_SHUTDOWN.load(Ordering::Acquire) { STATUS_SUCCESS } else { STATUS_SUCCESS }
}

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
pub unsafe extern "system" fn RtlIsThreadWithinLoaderCallout() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn RtlWnfDllUnloadCallback() -> i32 { STATUS_NOT_IMPLEMENTED }


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dll_directory_round_trip_uses_native_unicode_string_abi() {
        let mut path: Vec<u16> = "C:\\System32".encode_utf16().collect();
        let input = UNICODE_STRING {
            Length: (path.len() * 2) as u16,
            MaximumLength: (path.len() * 2) as u16,
            Buffer: path.as_mut_ptr(),
        };
        assert_eq!(unsafe { LdrSetDllDirectory(&input) }, STATUS_SUCCESS);
        let mut output = UNICODE_STRING { Length: 0, MaximumLength: 0, Buffer: ptr::null_mut() };
        assert_eq!(unsafe { LdrGetDllDirectory(&mut output) }, STATUS_SUCCESS);
        assert_eq!(output.Length, input.Length);
        assert!(!output.Buffer.is_null());
        assert_eq!(unsafe { core::slice::from_raw_parts(output.Buffer, path.len()) }, path.as_slice());
        assert_eq!(unsafe { LdrSetDllDirectory(ptr::null()) }, STATUS_SUCCESS);
    }

    #[test]
    fn dll_directory_cookie_rejects_stale_handles() {
        let mut path: Vec<u16> = "C:\\Plugins".encode_utf16().collect();
        let input = UNICODE_STRING {
            Length: (path.len() * 2) as u16,
            MaximumLength: (path.len() * 2) as u16,
            Buffer: path.as_mut_ptr(),
        };
        let mut cookie: PVOID = ptr::null_mut();
        assert_eq!(unsafe { LdrAddDllDirectory(&input, &mut cookie) }, STATUS_SUCCESS);
        assert!(!cookie.is_null());
        assert_eq!(unsafe { LdrRemoveDllDirectory(cookie) }, STATUS_SUCCESS);
        assert_eq!(unsafe { LdrRemoveDllDirectory(cookie) }, STATUS_INVALID_HANDLE);
    }

    #[test]
    fn default_search_flags_are_validated() {
        assert_eq!(unsafe { LdrSetDefaultDllDirectories(0) }, STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe { LdrSetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) }, STATUS_SUCCESS);
    }

    #[test]
    fn mapped_pe_resource_lookup_and_access_are_bounded() {
        let mut image = [0u8; 0x3000];
        image[0..2].copy_from_slice(&0x5a4du16.to_le_bytes());
        image[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        image[0x80..0x84].copy_from_slice(b"PE\\0\\0");
        image[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        image[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        let opt = 0x98;
        image[opt..opt+2].copy_from_slice(&0x20bu16.to_le_bytes());
        image[opt+56..opt+60].copy_from_slice(&0x3000u32.to_le_bytes());
        image[opt+108..opt+112].copy_from_slice(&16u32.to_le_bytes());
        image[opt+112+16..opt+112+20].copy_from_slice(&0x2000u32.to_le_bytes());
        image[opt+112+20..opt+112+24].copy_from_slice(&0x1000u32.to_le_bytes());
        // Three resource-directory levels: type 10, name 1, language 0x409.
        image[0x2000+14..0x2000+16].copy_from_slice(&1u16.to_le_bytes());
        image[0x2010..0x2014].copy_from_slice(&10u32.to_le_bytes());
        image[0x2014..0x2018].copy_from_slice(&0x8000_0020u32.to_le_bytes());
        image[0x2020+14..0x2020+16].copy_from_slice(&1u16.to_le_bytes());
        image[0x2030..0x2034].copy_from_slice(&1u32.to_le_bytes());
        image[0x2034..0x2038].copy_from_slice(&0x8000_0040u32.to_le_bytes());
        image[0x2040+14..0x2040+16].copy_from_slice(&1u16.to_le_bytes());
        image[0x2050..0x2054].copy_from_slice(&0x409u32.to_le_bytes());
        image[0x2054..0x2058].copy_from_slice(&0x60u32.to_le_bytes());
        image[0x2060..0x2064].copy_from_slice(&0x2100u32.to_le_bytes());
        image[0x2064..0x2068].copy_from_slice(&4u32.to_le_bytes());
        image[0x2100..0x2104].copy_from_slice(b"TEST");
        let info = LDR_RESOURCE_INFO { Type: 10, Name: 1, Language: 0x409 };
        let mut entry = ptr::null_mut();
        assert_eq!(unsafe { LdrFindResource_U(image.as_mut_ptr().cast(), &info, 3, &mut entry) }, STATUS_SUCCESS);
        assert!(!entry.is_null());
        let mut address = ptr::null_mut();
        let mut size = 0;
        assert_eq!(unsafe { LdrAccessResource(image.as_mut_ptr().cast(), entry, &mut address, &mut size) }, STATUS_SUCCESS);
        assert_eq!(size, 4);
        assert_eq!(unsafe { core::slice::from_raw_parts(address.cast::<u8>(), size as usize) }, b"TEST");
        assert_eq!(unsafe { LdrFindResource_U(image.as_mut_ptr().cast(), &info, 4, &mut entry) }, STATUS_INVALID_PARAMETER);
    }

    unsafe extern "system" fn notification_callback(_reason: ULONG, _data: *const c_void, _context: PVOID) {}

    #[test]
    fn dll_notification_cookie_lifetime_is_checked() {
        let mut cookie = ptr::null_mut();
        assert_eq!(unsafe { LdrRegisterDllNotification(0, Some(notification_callback), ptr::null_mut(), &mut cookie) }, STATUS_SUCCESS);
        assert!(!cookie.is_null());
        assert_eq!(unsafe { LdrUnregisterDllNotification(cookie) }, STATUS_SUCCESS);
        assert_eq!(unsafe { LdrUnregisterDllNotification(cookie) }, STATUS_INVALID_HANDLE);
    }
}

    #[test]
    fn amd64_relocation_block_applies_dir64_and_rejects_unknown_types() {
        let mut image = [0u8; 64];
        image[0x20..0x28].copy_from_slice(&0x1234u64.to_le_bytes());
        let mut entries = [0xA020u16, 0x0000u16];
        let end = unsafe { LdrProcessRelocationBlock(image.as_mut_ptr() as usize, 4, entries.as_mut_ptr(), 0x1000) };
        assert_eq!(end, unsafe { entries.as_mut_ptr().add(2) });
        assert_eq!(u64::from_le_bytes(image[0x20..0x28].try_into().unwrap()), 0x2234);
        let mut unsupported = [0x3020u16];
        assert!(unsafe { LdrProcessRelocationBlock(image.as_mut_ptr() as usize, 2, unsupported.as_mut_ptr(), 1) }.is_null());
        assert!(unsafe { LdrProcessRelocationBlockEx(0x14c, image.as_mut_ptr() as usize, 2, unsupported.as_mut_ptr(), 1) }.is_null());
    }


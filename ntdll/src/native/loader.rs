//! Windows 11 x64 NTDLL loader exports.
//!
//! Shared ABI/state façade for the Windows 11 x64 loader exports. Focused
//! implementations live under `loader/`. Search-directory controls, loaded-module
//! queries, resource access, relocation blocks, notification registration, and
//! delay-import lookup are implemented to their supported boundary. Mapping new
//! DLLs, activation contexts, enclave loading, and callback delivery still require
//! native image-manager and process-thread integration.
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

pub use crate::nt_abi::object::UNICODE_STRING;

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
    crate::trace::trace(crate::trace::TRACE_LOADER_ENTER, module as usize as u64, 0);
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
    crate::trace::trace(crate::trace::TRACE_LOADER_IMAGE, base as usize as u64, image_size as u64);
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
    if wanted.length & 1 != 0 || wanted.length > wanted.maximum_length
        || (wanted.length != 0 && wanted.buffer.is_null()) { return false; }
    let offset = if full { 0x48 } else { 0x58 };
    let length = unsafe { ptr::read_unaligned(entry.add(offset).cast::<u16>()) };
    let buffer = unsafe { ptr::read_unaligned(entry.add(offset + 8).cast::<usize>()) } as *const u16;
    if length != wanted.length || length & 1 != 0 || (length != 0 && (buffer.is_null() || wanted.buffer.is_null())) { return false; }
    for i in 0..(length as usize / 2) {
        let a = unsafe { ptr::read_unaligned(buffer.add(i)) };
        let b = unsafe { ptr::read_unaligned(wanted.buffer.add(i)) };
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
    if (descriptor.length & 1) != 0 || descriptor.length > descriptor.maximum_length
        || descriptor.length as usize / 2 >= MAX_PATH_UNITS
        || (descriptor.length != 0 && descriptor.buffer.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }
    let count = descriptor.length as usize / 2;
    if count == 0 { *output = PathSlot::EMPTY; output.active = true; return STATUS_SUCCESS; }
    let path = unsafe { core::slice::from_raw_parts(descriptor.buffer, count) };
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

mod search;
pub use search::*;
mod modules;
pub use modules::*;
mod resources;
pub use resources::*;
mod notifications;
pub use notifications::*;
mod delay;
pub use delay::*;
#[cfg(test)]
include!("loader/tests.rs");

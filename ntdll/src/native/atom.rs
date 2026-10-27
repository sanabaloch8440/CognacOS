//! Windows 11 x64 NTDLL atom routines.
//!
//! These routines provide a bounded, process-local atom-table implementation
//! until CognacOS exposes the executive's system-wide atom-table service.
//! Native atom syscalls are system-global on Windows; this fallback is not a
//! substitute for that kernel service.
//!
//! Atom name lengths passed to NtAddAtom/NtFindAtom are byte counts, not
//! NUL-terminated character counts. String atoms occupy 0xC000..=0xFFFF;
//! integer atoms occupy the separate low range.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicBool, Ordering};

type NTSTATUS = i32;
type USHORT = u16;
type ULONG = u32;
type PWSTR = *const u16;
type PULONG = *mut ULONG;
type PVOID = *mut c_void;
type BOOLEAN = u8;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017u32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023u32 as i32;
const STATUS_INFO_LENGTH_MISMATCH: NTSTATUS = 0xC000_0004u32 as i32;
const STATUS_OBJECT_NAME_NOT_FOUND: NTSTATUS = 0xC000_0034u32 as i32;
const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
const FIRST_STRING_ATOM: USHORT = 0xC000;
const MAX_NAME_UNITS: usize = 255;
const MAX_ENTRIES: usize = 256;
const ATOM_BASIC_INFORMATION: ULONG = 0;
const ATOM_TABLE_INFORMATION: ULONG = 1;

#[derive(Clone, Copy)]
struct AtomEntry {
    used: bool,
    atom: USHORT,
    refs: USHORT,
    length: USHORT,
    name: [u16; MAX_NAME_UNITS],
}

impl AtomEntry {
    const EMPTY: Self = Self {
        used: false,
        atom: 0,
        refs: 0,
        length: 0,
        name: [0; MAX_NAME_UNITS],
    };
}

struct AtomState {
    entries: [AtomEntry; MAX_ENTRIES],
    next_atom: USHORT,
}

impl AtomState {
    const fn new() -> Self {
        Self { entries: [AtomEntry::EMPTY; MAX_ENTRIES], next_atom: FIRST_STRING_ATOM }
    }
}

struct LockedAtomState {
    held: AtomicBool,
    state: UnsafeCell<AtomState>,
}
unsafe impl Sync for LockedAtomState {}

impl LockedAtomState {
    const fn new() -> Self {
        Self { held: AtomicBool::new(false), state: UnsafeCell::new(AtomState::new()) }
    }

    fn lock(&self) -> AtomGuard<'_> {
        while self.held.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            spin_loop();
        }
        AtomGuard { lock: self }
    }
}

struct AtomGuard<'a> { lock: &'a LockedAtomState }
impl Drop for AtomGuard<'_> {
    fn drop(&mut self) { self.lock.held.store(false, Ordering::Release); }
}

static ATOM_STATE: LockedAtomState = LockedAtomState::new();

#[inline]
fn state<'a>(guard: &'a AtomGuard<'_>) -> &'a mut AtomState {
    // The guard exclusively owns the spin lock for the lifetime of this borrow.
    unsafe { &mut *guard.lock.state.get() }
}

fn parse_integer_atom(name: &[u16]) -> Option<USHORT> {
    if name.len() < 2 || name[0] != b'#' as u16 { return None; }
    let mut value: u32 = 0;
    for ch in &name[1..] {
        if !(b'0' as u16..=b'9' as u16).contains(ch) { return None; }
        value = value.checked_mul(10)?.checked_add((*ch - b'0' as u16) as u32)?;
        if value > 0xBFFF { return None; }
    }
    if value == 0 { None } else { Some(value as USHORT) }
}

unsafe fn read_name(name: PWSTR, length: ULONG) -> Result<[u16; MAX_NAME_UNITS], NTSTATUS> {
    if name.is_null() || length == 0 || length % 2 != 0 {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let units = (length / 2) as usize;
    if units > MAX_NAME_UNITS { return Err(STATUS_INVALID_PARAMETER); }
    let mut copy = [0u16; MAX_NAME_UNITS];
    for i in 0..units {
        copy[i] = unsafe { *name.add(i) };
    }
    Ok(copy)
}

#[inline]
fn fold_ascii_case(ch: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&ch) { ch + 32 } else { ch }
}

fn equal_name(entry: &AtomEntry, name: &[u16; MAX_NAME_UNITS], len: usize) -> bool {
    entry.used && entry.length as usize == len && entry.name[..len].iter().zip(&name[..len])
        .all(|(a, b)| fold_ascii_case(*a) == fold_ascii_case(*b))
}

fn find_entry(s: &AtomState, name: &[u16; MAX_NAME_UNITS], len: usize) -> Option<usize> {
    s.entries.iter().position(|e| equal_name(e, name, len))
}

fn add_name(name: PWSTR, length: ULONG, atom_out: *mut USHORT, increment: bool) -> NTSTATUS {
    if atom_out.is_null() { return STATUS_INVALID_PARAMETER; }
    let name_buf = match unsafe { read_name(name, length) } { Ok(v) => v, Err(e) => return e };
    let len = (length / 2) as usize;
    if let Some(integer) = parse_integer_atom(&name_buf[..len]) {
        unsafe { atom_out.write(integer); }
        return STATUS_SUCCESS;
    }
    let guard = ATOM_STATE.lock();
    let s = state(&guard);
    if let Some(index) = find_entry(s, &name_buf, len) {
        let e = &mut s.entries[index];
        if increment { e.refs = e.refs.saturating_add(1); }
        unsafe { atom_out.write(e.atom); }
        return STATUS_SUCCESS;
    }
    if !increment { return STATUS_OBJECT_NAME_NOT_FOUND; }
    let Some(index) = s.entries.iter().position(|e| !e.used) else { return STATUS_NO_MEMORY; };
    let atom = allocate_atom(s);
    if atom == 0 { return STATUS_NO_MEMORY; }
    let entry = &mut s.entries[index];
    entry.used = true;
    entry.atom = atom;
    entry.refs = 1;
    entry.length = len as USHORT;
    entry.name[..len].copy_from_slice(&name_buf[..len]);
    unsafe { atom_out.write(atom); }
    STATUS_SUCCESS
}

fn allocate_atom(s: &mut AtomState) -> USHORT {
    // Walk the complete string-atom range, skipping IDs still in use.
    for _ in 0..(u16::MAX - FIRST_STRING_ATOM + 1) {
        let candidate = s.next_atom;
        s.next_atom = if candidate == u16::MAX { FIRST_STRING_ATOM } else { candidate + 1 };
        if !s.entries.iter().any(|e| e.used && e.atom == candidate) { return candidate; }
    }
    0
}

#[repr(C)]
struct AtomBasicInformation {
    usage_count: USHORT,
    flags: USHORT,
    name_length: USHORT,
    name: [u16; 1],
}

#[repr(C)]
struct AtomTableInformation {
    number_of_atoms: ULONG,
    atoms: [USHORT; 1],
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetIntegerAtom(string: PWSTR, atom: *mut USHORT) -> BOOLEAN {
    if string.is_null() || atom.is_null() { return 0; }
    let mut buf = [0u16; MAX_NAME_UNITS];
    for i in 0..MAX_NAME_UNITS {
        let ch = unsafe { *string.add(i) };
        if ch == 0 {
            return match parse_integer_atom(&buf[..i]) {
                Some(value) => { unsafe { atom.write(value); } 1 },
                None => 0,
            };
        }
        buf[i] = ch;
    }
    0
}

#[no_mangle]
pub unsafe extern "system" fn NtAddAtom(name: PWSTR, length: ULONG, atom: *mut USHORT) -> NTSTATUS {
    add_name(name, length, atom, true)
}

#[no_mangle]
pub unsafe extern "system" fn NtAddAtomEx(name: PWSTR, length: ULONG, atom: *mut USHORT, flags: ULONG) -> NTSTATUS {
    if flags != 0 { return STATUS_INVALID_PARAMETER; }
    add_name(name, length, atom, true)
}

#[no_mangle]
pub unsafe extern "system" fn NtFindAtom(name: PWSTR, length: ULONG, atom: *mut USHORT) -> NTSTATUS {
    add_name(name, length, atom, false)
}

#[no_mangle]
pub unsafe extern "system" fn NtDeleteAtom(atom: USHORT) -> NTSTATUS {
    if atom == 0 { return STATUS_INVALID_HANDLE; }
    if atom < FIRST_STRING_ATOM { return STATUS_SUCCESS; }
    let guard = ATOM_STATE.lock();
    let s = state(&guard);
    let Some(entry) = s.entries.iter_mut().find(|e| e.used && e.atom == atom) else {
        return STATUS_INVALID_HANDLE;
    };
    if entry.refs > 1 {
        entry.refs -= 1;
    } else {
        *entry = AtomEntry::EMPTY;
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationAtom(
    atom: USHORT,
    information_class: ULONG,
    information: PVOID,
    information_length: ULONG,
    return_length: PULONG,
) -> NTSTATUS {
    if information.is_null() { return STATUS_INVALID_PARAMETER; }
    let guard = ATOM_STATE.lock();
    let s = state(&guard);
    match information_class {
        ATOM_BASIC_INFORMATION => {
            let Some(entry) = s.entries.iter().find(|e| e.used && e.atom == atom) else {
                return if atom > 0 && atom < FIRST_STRING_ATOM { STATUS_NOT_IMPLEMENTED } else { STATUS_INVALID_HANDLE };
            };
            let required = core::mem::offset_of!(AtomBasicInformation, name) + (entry.length as usize + 1) * 2;
            if !return_length.is_null() { unsafe { return_length.write(required as ULONG); } }
            if (information_length as usize) < required { return STATUS_BUFFER_TOO_SMALL; }
            let out = information.cast::<AtomBasicInformation>();
            unsafe {
                (*out).usage_count = entry.refs;
                (*out).flags = 0;
                (*out).name_length = entry.length * 2;
                core::ptr::copy_nonoverlapping(entry.name.as_ptr(), (*out).name.as_mut_ptr(), entry.length as usize);
                *(*out).name.as_mut_ptr().add(entry.length as usize) = 0;
            }
            STATUS_SUCCESS
        }
        ATOM_TABLE_INFORMATION => {
            let count = s.entries.iter().filter(|e| e.used).count();
            let required = core::mem::offset_of!(AtomTableInformation, atoms) + count * core::mem::size_of::<USHORT>();
            if !return_length.is_null() { unsafe { return_length.write(required as ULONG); } }
            if (information_length as usize) < required { return STATUS_INFO_LENGTH_MISMATCH; }
            let out = information.cast::<AtomTableInformation>();
            unsafe { (*out).number_of_atoms = count as ULONG; }
            let mut i = 0usize;
            for entry in s.entries.iter().filter(|e| e.used) {
                unsafe { (*out).atoms.as_mut_ptr().add(i).write(entry.atom); }
                i += 1;
            }
            STATUS_SUCCESS
        }
        _ => STATUS_INVALID_PARAMETER,
    }
}

/// Initializes the process-local atom package. The table is statically allocated,
/// so initialization is idempotent and requires no allocator or kernel service.
#[no_mangle]
pub unsafe extern "system" fn RtlInitializeAtomPackage() -> BOOLEAN {
    1
}


// Legacy-compatible RTL atom-table APIs retained alongside the canonical Nt* APIs.
mod legacy_compat {
//! Rust source port of Wine's \`dlls/ntdll/atom.c\`.
//!
//! This is a source-only port. It intentionally calls the Windows-compatible
//! RTL heap, handle-table, critical-section, and Unicode helper ABI; those
//! dependencies must be supplied by CognacOS before this module is linked.
//! Wine's debug-channel macros are omitted, while the NTSTATUS and atom-table
//! behavior is kept close to the original implementation.

use core::ffi::c_void;
use core::{mem, ptr};

type NTSTATUS = i32;
type ULONG = u32;
type USHORT = u16;
type UCHAR = u8;
type BOOLEAN = u8;
type WCHAR = u16;
type RTL_ATOM = USHORT;
type HANDLE = *mut c_void;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_MORE_ENTRIES: NTSTATUS = 0x0000_0105;
const STATUS_WAS_LOCKED: NTSTATUS = 0x4000_0019_u32 as i32;
const STATUS_INVALID_HANDLE: NTSTATUS = 0xC000_0008_u32 as i32;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000D_u32 as i32;
const STATUS_NO_MEMORY: NTSTATUS = 0xC000_0017_u32 as i32;
const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC000_0023_u32 as i32;
const STATUS_OBJECT_NAME_INVALID: NTSTATUS = 0xC000_0033_u32 as i32;
const STATUS_OBJECT_NAME_NOT_FOUND: NTSTATUS = 0xC000_0034_u32 as i32;

const MAX_ATOM_LEN: usize = 255;
const MAX_ATOMS: ULONG = 0x4000;
const TABLE_SIGNATURE: ULONG = 0x6d6f_7441; // "Atom"
const HASH_SIZE: ULONG = 37;
const MIN_HASH_SIZE: ULONG = 4;
const MAX_HASH_SIZE: ULONG = 0x200;
const MAXINTATOM: RTL_ATOM = 0xC000;
const HEAP_ZERO_MEMORY: ULONG = 0x0000_0008;
const HASH_STRING_ALGORITHM_X65599: ULONG = 3;

#[repr(C)]
struct RTL_CRITICAL_SECTION {
    debug_info: *mut c_void,
    lock_count: i32,
    recursion_count: i32,
    owning_thread: HANDLE,
    lock_semaphore: HANDLE,
    spin_count: usize,
}

#[repr(C)]
struct RTL_HANDLE {
    next: *mut RTL_HANDLE,
}

#[repr(C)]
struct RTL_HANDLE_TABLE {
    max_handle_count: ULONG,
    handle_size: ULONG,
    unused: [ULONG; 2],
    next_free: *mut c_void,
    first_handle: *mut c_void,
    reserved_memory: *mut c_void,
    max_handle: *mut c_void,
}

#[repr(C)]
struct RTL_ATOM_TABLE_ENTRY {
    hash_link: *mut RTL_ATOM_TABLE_ENTRY,
    handle_index: USHORT,
    atom: USHORT,
    reference_count: USHORT,
    flags: UCHAR,
    name_length: UCHAR,
    name: [WCHAR; 1],
}

#[repr(C)]
struct RTL_ATOM_TABLE {
    signature: ULONG,
    critical_section: RTL_CRITICAL_SECTION,
    handle_table: RTL_HANDLE_TABLE,
    number_of_buckets: ULONG,
    buckets: [*mut RTL_ATOM_TABLE_ENTRY; 1],
}

type RTL_ATOM_TABLE_PTR = *mut RTL_ATOM_TABLE;

#[repr(C)]
struct RTL_HANDLE_WRAPPER {
    hdr: RTL_HANDLE,
    entry: *mut RTL_ATOM_TABLE_ENTRY,
}

#[repr(C)]
struct UNICODE_STRING {
    length: USHORT,
    maximum_length: USHORT,
    buffer: *mut WCHAR,
}

extern "system" {
    fn RtlAllocateHeap(heap: HANDLE, flags: ULONG, size: usize) -> *mut c_void;
    fn RtlFreeHeap(heap: HANDLE, flags: ULONG, ptr: *mut c_void) -> BOOLEAN;
    fn RtlEnterCriticalSection(cs: *mut RTL_CRITICAL_SECTION) -> NTSTATUS;
    fn RtlLeaveCriticalSection(cs: *mut RTL_CRITICAL_SECTION) -> NTSTATUS;
    fn RtlInitializeCriticalSection(cs: *mut RTL_CRITICAL_SECTION) -> NTSTATUS;
    fn RtlDeleteCriticalSection(cs: *mut RTL_CRITICAL_SECTION) -> NTSTATUS;
    fn RtlInitializeHandleTable(maximum: ULONG, size: ULONG, table: *mut RTL_HANDLE_TABLE);
    fn RtlDestroyHandleTable(table: *mut RTL_HANDLE_TABLE) -> NTSTATUS;
    fn RtlAllocateHandle(table: *mut RTL_HANDLE_TABLE, index: *mut ULONG) -> *mut RTL_HANDLE;
    fn RtlFreeHandle(table: *mut RTL_HANDLE_TABLE, handle: *mut RTL_HANDLE) -> BOOLEAN;
    fn RtlIsValidIndexHandle(
        table: *const RTL_HANDLE_TABLE,
        index: ULONG,
        handle: *mut *mut RTL_HANDLE,
    ) -> BOOLEAN;
    fn RtlHashUnicodeString(
        string: *const UNICODE_STRING,
        case_insensitive: BOOLEAN,
        algorithm: ULONG,
        hash: *mut ULONG,
    ) -> NTSTATUS;
    fn RtlCompareUnicodeStrings(
        first: *const WCHAR,
        first_length: usize,
        second: *const WCHAR,
        second_length: usize,
        case_insensitive: BOOLEAN,
    ) -> i32;
}

#[inline]
unsafe fn bucket(table: RTL_ATOM_TABLE_PTR, index: ULONG) -> *mut *mut RTL_ATOM_TABLE_ENTRY {
    // Buckets is a C flexible-array idiom: allocation reserves NumberOfBuckets slots.
    unsafe { ptr::addr_of_mut!((*table).buckets).cast::<*mut RTL_ATOM_TABLE_ENTRY>().add(index as usize) }
}

unsafe fn lock_atom_table(table: RTL_ATOM_TABLE_PTR) -> NTSTATUS {
    if table.is_null() { return STATUS_INVALID_PARAMETER; }
    if unsafe { (*table).signature } != TABLE_SIGNATURE { return STATUS_INVALID_PARAMETER; }
    unsafe { RtlEnterCriticalSection(ptr::addr_of_mut!((*table).critical_section)) }
}

unsafe fn unlock_atom_table(table: RTL_ATOM_TABLE_PTR) {
    unsafe { RtlLeaveCriticalSection(ptr::addr_of_mut!((*table).critical_section)); }
}

unsafe fn hash_str(table: RTL_ATOM_TABLE_PTR, name: *const WCHAR, len: usize) -> ULONG {
    let byte_len = len.saturating_mul(mem::size_of::<WCHAR>());
    let str = UNICODE_STRING {
        length: byte_len.min(USHORT::MAX as usize) as USHORT,
        maximum_length: byte_len.min(USHORT::MAX as usize) as USHORT,
        buffer: name as *mut WCHAR,
    };
    let mut hash = 0;
    unsafe {
        RtlHashUnicodeString(&str, 1, HASH_STRING_ALGORITHM_X65599, &mut hash);
        hash % (*table).number_of_buckets
    }
}

unsafe fn find_entry(
    table: RTL_ATOM_TABLE_PTR,
    name: *const WCHAR,
    len: usize,
    hash: ULONG,
) -> *mut RTL_ATOM_TABLE_ENTRY {
    let mut entry = unsafe { *bucket(table, hash) };
    while !entry.is_null() {
        let equal = unsafe {
            RtlCompareUnicodeStrings(
                (*entry).name.as_ptr(),
                (*entry).name_length as usize,
                name,
                len,
                1,
            ) == 0
        };
        if equal { break; }
        entry = unsafe { (*entry).hash_link };
    }
    entry
}

unsafe fn add_atom(
    table: RTL_ATOM_TABLE_PTR,
    name: *const WCHAR,
    len: usize,
    atom: *mut RTL_ATOM,
) -> NTSTATUS {
    let hash = unsafe { hash_str(table, name, len) };
    let existing = unsafe { find_entry(table, name, len, hash) };
    if !existing.is_null() {
        unsafe {
            (*existing).reference_count = (*existing).reference_count.wrapping_add(1);
            *atom = (*existing).atom;
        }
        return STATUS_SUCCESS;
    }

    let mut index: ULONG = 0;
    let handle = unsafe { RtlAllocateHandle(ptr::addr_of_mut!((*table).handle_table), &mut index) };
    if handle.is_null() { return STATUS_NO_MEMORY; }

    // offsetof(RTL_ATOM_TABLE_ENTRY, Name[len]) = offset(Name) + len * sizeof(WCHAR).
    let name_offset = mem::offset_of!(RTL_ATOM_TABLE_ENTRY, name);
    let allocation_size = name_offset.saturating_add(len.saturating_mul(mem::size_of::<WCHAR>()));
    let entry = unsafe { RtlAllocateHeap(crate::rtl_heap::RtlGetProcessHeap(), 0, allocation_size) as *mut RTL_ATOM_TABLE_ENTRY };
    if entry.is_null() {
        unsafe { RtlFreeHandle(ptr::addr_of_mut!((*table).handle_table), handle); }
        return STATUS_NO_MEMORY;
    }

    unsafe {
        ptr::write_bytes(entry.cast::<u8>(), 0, allocation_size);
        (*entry).handle_index = index as USHORT;
        (*entry).atom = MAXINTATOM.wrapping_add(index as USHORT);
        (*entry).reference_count = 1;
        (*entry).flags = 0;
        (*entry).name_length = len as UCHAR;
        (*entry).hash_link = *bucket(table, hash);
        ptr::copy_nonoverlapping(name, (*entry).name.as_mut_ptr(), len);
        (*handle).next = 1usize as *mut RTL_HANDLE;
        *bucket(table, hash) = entry;
        (*(handle as *mut RTL_HANDLE_WRAPPER)).entry = entry;
        *atom = (*entry).atom;
    }
    STATUS_SUCCESS
}

unsafe fn is_integral_atom(
    atom_string: *const WCHAR,
    len: usize,
    out_atom: *mut RTL_ATOM,
) -> NTSTATUS {
    if atom_string.is_null() { return STATUS_INVALID_PARAMETER; }
    let is_pointer_atom = ((atom_string as usize) >> 16) == 0;
    let mut atom: RTL_ATOM;
    if !is_pointer_atom {
        if len == 0 { return STATUS_OBJECT_NAME_INVALID; }
        if unsafe { *atom_string } == b'#' as WCHAR {
            atom = 0;
            let mut ptr = unsafe { atom_string.add(1) };
            let end = unsafe { atom_string.add(len) };
            while ptr < end {
                let ch = unsafe { *ptr };
                if !(b'0' as WCHAR..=b'9' as WCHAR).contains(&ch) { break; }
                atom = atom.wrapping_mul(10).wrapping_add(ch - b'0' as WCHAR);
                ptr = unsafe { ptr.add(1) };
            }
            if ptr > unsafe { atom_string.add(1) } && ptr == end {
                // parsed decimal atom
            } else {
                if len > MAX_ATOM_LEN { return STATUS_INVALID_PARAMETER; }
                return STATUS_MORE_ENTRIES;
            }
        } else {
            if len > MAX_ATOM_LEN { return STATUS_INVALID_PARAMETER; }
            return STATUS_MORE_ENTRIES;
        }
    } else {
        atom = atom_string as usize as RTL_ATOM;
        if atom >= MAXINTATOM { return STATUS_INVALID_PARAMETER; }
    }

    if atom >= MAXINTATOM { atom = 0; }
    if atom == 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { *out_atom = atom; }
    STATUS_SUCCESS
}

unsafe fn integral_atom_name(buffer: *mut WCHAR, length: ULONG, atom: RTL_ATOM) -> ULONG {
    let mut digits = [0u16; 16];
    let mut value = atom as u32;
    let mut n = 0usize;
    digits[n] = b'#' as WCHAR;
    n += 1;
    let mut reversed = [0u16; 10];
    let mut count = 0usize;
    loop {
        reversed[count] = (b'0' as u32 + value % 10) as WCHAR;
        count += 1;
        value /= 10;
        if value == 0 { break; }
    }
    while count > 0 {
        count -= 1;
        digits[n] = reversed[count];
        n += 1;
    }
    let mut copied = n;
    if length != 0 {
        copied = copied.min(length.saturating_sub(1) as usize);
        if !buffer.is_null() {
            unsafe {
                ptr::copy_nonoverlapping(digits.as_ptr(), buffer, copied);
                *buffer.add(copied) = 0;
            }
        }
    }
    (copied * mem::size_of::<WCHAR>()) as ULONG
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteAtomFromAtomTable(
    table: RTL_ATOM_TABLE_PTR,
    atom: RTL_ATOM,
) -> NTSTATUS {
    let mut status = unsafe { lock_atom_table(table) };
    if status != STATUS_SUCCESS { return status; }

    if atom == 0 {
        status = STATUS_INVALID_HANDLE;
    } else if atom < MAXINTATOM {
        status = STATUS_SUCCESS;
    } else {
        let mut raw_handle: *mut RTL_HANDLE = ptr::null_mut();
        let valid = unsafe {
            RtlIsValidIndexHandle(&(*table).handle_table, (atom - MAXINTATOM) as ULONG, &mut raw_handle)
        };
        if valid != 0 {
            let handle = raw_handle as *mut RTL_HANDLE_WRAPPER;
            let entry = unsafe { (*handle).entry };
            if unsafe { (*entry).flags } != 0 {
                status = STATUS_WAS_LOCKED;
            } else {
                unsafe { (*entry).reference_count = (*entry).reference_count.wrapping_sub(1); }
                if unsafe { (*entry).reference_count } == 0 {
                    let hash = unsafe { hash_str(table, (*entry).name.as_ptr(), (*entry).name_length as usize) };
                    let mut link = unsafe { bucket(table, hash) };
                    unsafe {
                        while !(*link).is_null() && *link != entry {
                            link = ptr::addr_of_mut!((**link).hash_link);
                        }
                        if !(*link).is_null() { *link = (*entry).hash_link; }
                        RtlFreeHeap(crate::rtl_heap::RtlGetProcessHeap(), 0, entry.cast());
                        RtlFreeHandle(ptr::addr_of_mut!((*table).handle_table), raw_handle);
                    }
                }
                status = STATUS_SUCCESS;
            }
        } else {
            status = STATUS_INVALID_HANDLE;
        }
    }
    unsafe { unlock_atom_table(table); }
    status
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryAtomInAtomTable(
    table: RTL_ATOM_TABLE_PTR,
    atom: RTL_ATOM,
    reference_count: *mut ULONG,
    pin_count: *mut ULONG,
    name: *mut WCHAR,
    length: *mut ULONG,
) -> NTSTATUS {
    if atom == 0 { return STATUS_INVALID_PARAMETER; }
    let mut status = unsafe { lock_atom_table(table) };
    if status != STATUS_SUCCESS { return status; }

    let mut written_len: ULONG = 0;
    if atom < MAXINTATOM {
        if !length.is_null() {
            written_len = unsafe { integral_atom_name(name, *length, atom) };
        }
        if !reference_count.is_null() { unsafe { *reference_count = 1; } }
        if !pin_count.is_null() { unsafe { *pin_count = 1; } }
    } else {
        let mut raw_handle: *mut RTL_HANDLE = ptr::null_mut();
        let valid = unsafe {
            RtlIsValidIndexHandle(&(*table).handle_table, (atom - MAXINTATOM) as ULONG, &mut raw_handle)
        };
        if valid != 0 {
            let entry = unsafe { (*(raw_handle as *mut RTL_HANDLE_WRAPPER)).entry };
            written_len = unsafe { (*entry).name_length as ULONG * mem::size_of::<WCHAR>() as ULONG };
            if !reference_count.is_null() { unsafe { *reference_count = (*entry).reference_count as ULONG; } }
            if !pin_count.is_null() { unsafe { *pin_count = (*entry).flags as ULONG; } }
            if !length.is_null() && unsafe { *length } != 0 {
                written_len = written_len.min(unsafe { (*length).saturating_sub(mem::size_of::<WCHAR>() as ULONG) });
                if !name.is_null() {
                    unsafe {
                        ptr::copy_nonoverlapping((*entry).name.as_ptr(), name, (written_len as usize) / 2);
                        *name.add((written_len as usize) / 2) = 0;
                    }
                }
            }
        } else {
            status = STATUS_INVALID_HANDLE;
        }
    }

    unsafe { unlock_atom_table(table); }
    if status == STATUS_SUCCESS && !length.is_null() {
        if unsafe { *length } == 0 { status = STATUS_BUFFER_TOO_SMALL; }
        unsafe { *length = written_len; }
    }
    status
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateAtomTable(
    size: ULONG,
    returned_table: *mut RTL_ATOM_TABLE_PTR,
) -> NTSTATUS {
    if returned_table.is_null() { return STATUS_INVALID_PARAMETER; }
    if unsafe { !(*returned_table).is_null() } {
        return if size != 0 { STATUS_INVALID_PARAMETER } else { STATUS_SUCCESS };
    }
    let size = if size < MIN_HASH_SIZE || size > MAX_HASH_SIZE { HASH_SIZE } else { size };
    let bucket_offset = mem::offset_of!(RTL_ATOM_TABLE, buckets);
    let allocation_size = bucket_offset.saturating_add(size as usize * mem::size_of::<*mut RTL_ATOM_TABLE_ENTRY>());
    let table = unsafe {
        RtlAllocateHeap(crate::rtl_heap::RtlGetProcessHeap(), HEAP_ZERO_MEMORY, allocation_size) as RTL_ATOM_TABLE_PTR
    };
    if table.is_null() { return STATUS_NO_MEMORY; }
    unsafe {
        (*table).signature = TABLE_SIGNATURE;
        (*table).number_of_buckets = size;
        if RtlInitializeCriticalSection(ptr::addr_of_mut!((*table).critical_section)) < 0 {
            RtlFreeHeap(crate::rtl_heap::RtlGetProcessHeap(), 0, table.cast());
            return STATUS_NO_MEMORY;
        }
        RtlInitializeHandleTable(MAX_ATOMS, mem::size_of::<RTL_HANDLE_WRAPPER>() as ULONG,
                                 ptr::addr_of_mut!((*table).handle_table));
        *returned_table = table;
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlDestroyAtomTable(table: RTL_ATOM_TABLE_PTR) -> NTSTATUS {
    if table.is_null() || unsafe { (*table).signature } != TABLE_SIGNATURE {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe {
        RtlEmptyAtomTable(table, 1);
        RtlDestroyHandleTable(ptr::addr_of_mut!((*table).handle_table));
        RtlDeleteCriticalSection(ptr::addr_of_mut!((*table).critical_section));
        (*table).signature = 0;
        RtlFreeHeap(crate::rtl_heap::RtlGetProcessHeap(), 0, table.cast());
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAtomToAtomTable(
    table: RTL_ATOM_TABLE_PTR,
    name: *const WCHAR,
    atom: *mut RTL_ATOM,
) -> NTSTATUS {
    let mut status = unsafe { lock_atom_table(table) };
    if status != STATUS_SUCCESS { return status; }
    if name.is_null() || atom.is_null() {
        unsafe { unlock_atom_table(table); }
        return STATUS_INVALID_PARAMETER;
    }
    let len = if ((name as usize) >> 16) == 0 {
        0
    } else {
        let mut n = 0usize;
        while unsafe { *name.add(n) } != 0 && n <= MAX_ATOM_LEN { n += 1; }
        n
    };
    status = unsafe { is_integral_atom(name, len, atom) };
    if status == STATUS_MORE_ENTRIES { status = unsafe { add_atom(table, name, len, atom) }; }
    unsafe { unlock_atom_table(table); }
    status
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupAtomInAtomTable(
    table: RTL_ATOM_TABLE_PTR,
    name: *const WCHAR,
    atom: *mut RTL_ATOM,
) -> NTSTATUS {
    let mut status = unsafe { lock_atom_table(table) };
    if status != STATUS_SUCCESS { return status; }
    if name.is_null() || atom.is_null() {
        unsafe { unlock_atom_table(table); }
        return STATUS_INVALID_PARAMETER;
    }
    let len = if ((name as usize) >> 16) == 0 {
        0
    } else {
        let mut n = 0usize;
        while unsafe { *name.add(n) } != 0 && n <= MAX_ATOM_LEN { n += 1; }
        n
    };
    status = unsafe { is_integral_atom(name, len, atom) };
    if status == STATUS_MORE_ENTRIES {
        let hash = unsafe { hash_str(table, name, len) };
        let entry = unsafe { find_entry(table, name, len, hash) };
        if !entry.is_null() {
            unsafe { *atom = (*entry).atom; }
            status = STATUS_SUCCESS;
        } else {
            status = STATUS_OBJECT_NAME_NOT_FOUND;
        }
    }
    unsafe { unlock_atom_table(table); }
    status
}

#[no_mangle]
pub unsafe extern "system" fn RtlEmptyAtomTable(
    table: RTL_ATOM_TABLE_PTR,
    delete_pinned: BOOLEAN,
) -> NTSTATUS {
    let status = unsafe { lock_atom_table(table) };
    if status != STATUS_SUCCESS { return status; }

    let buckets = unsafe { (*table).number_of_buckets };
    for i in 0..buckets {
        let mut link = unsafe { bucket(table, i) };
        loop {
            let entry = unsafe { *link };
            if entry.is_null() { break; }
            let pinned = unsafe { (*entry).flags != 0 };
            if delete_pinned == 0 && pinned {
                link = unsafe { ptr::addr_of_mut!((*entry).hash_link) };
            } else {
                unsafe {
                    *link = (*entry).hash_link;
                    let mut raw_handle: *mut RTL_HANDLE = ptr::null_mut();
                    if RtlIsValidIndexHandle(&(*table).handle_table, (*entry).handle_index as ULONG, &mut raw_handle) != 0 {
                        RtlFreeHandle(ptr::addr_of_mut!((*table).handle_table), raw_handle);
                    }
                    RtlFreeHeap(crate::rtl_heap::RtlGetProcessHeap(), 0, entry.cast());
                }
            }
        }
    }
    unsafe { unlock_atom_table(table); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlPinAtomInAtomTable(
    table: RTL_ATOM_TABLE_PTR,
    atom: RTL_ATOM,
) -> NTSTATUS {
    let status = unsafe { lock_atom_table(table) };
    if status != STATUS_SUCCESS { return status; }
    if atom >= MAXINTATOM {
        let mut raw_handle: *mut RTL_HANDLE = ptr::null_mut();
        let valid = unsafe {
            RtlIsValidIndexHandle(&(*table).handle_table, (atom - MAXINTATOM) as ULONG, &mut raw_handle)
        };
        if valid != 0 {
            unsafe { (*(raw_handle as *mut RTL_HANDLE_WRAPPER)).entry.as_mut().unwrap().flags = 1; }
        }
    }
    unsafe { unlock_atom_table(table); }
    STATUS_SUCCESS
}

}

pub use legacy_compat::{RtlAddAtomToAtomTable, RtlCreateAtomTable, RtlDeleteAtomFromAtomTable, RtlDestroyAtomTable, RtlEmptyAtomTable, RtlLookupAtomInAtomTable, RtlPinAtomInAtomTable, RtlQueryAtomInAtomTable};

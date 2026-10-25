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
fn state(guard: &AtomGuard<'_>) -> &mut AtomState {
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

fn equal_name(entry: &AtomEntry, name: &[u16; MAX_NAME_UNITS], len: usize) -> bool {
    entry.used && entry.length as usize == len && entry.name[..len].iter().zip(&name[..len])
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
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

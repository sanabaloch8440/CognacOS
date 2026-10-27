//! Windows 11 x64 NTDLL miscellaneous exports.
//! Unsupported routines remain explicit failures until their owning subsystem exists.
#![allow(non_snake_case, clippy::missing_safety_doc)]

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002u32 as i32;

// SQM/telemetry is optional. Until a provider is configured, event writes are
// no-ops and capability queries report disabled. APIs that require a real
// runtime subsystem or an output structure remain explicitly unsupported.
#[repr(C)] #[derive(Clone, Copy)] struct UnicodeString { length: u16, maximum_length: u16, buffer: *const u16 }
unsafe fn apiset_schema_has(name: *const UnicodeString) -> Result<bool, i32> {
 if name.is_null() { return Err(STATUS_INVALID_PARAMETER); } let n = unsafe { &*name };
 if n.length == 0 || n.length & 1 != 0 || n.length > n.maximum_length || n.buffer.is_null() || n.length > 520 { return Err(STATUS_INVALID_PARAMETER); }
 let chars = n.length as usize / 2; let wanted = unsafe { core::slice::from_raw_parts(n.buffer, chars) };
 let peb: *const u8; unsafe { core::arch::asm!("mov {}, gs:[0x60]", out(reg) peb, options(nostack, preserves_flags, readonly)); }
 if peb.is_null() { return Ok(false); } let map = unsafe { core::ptr::read_unaligned(peb.add(0x68).cast::<usize>()) } as *const u8; if map.is_null() { return Ok(false); }
 let version = unsafe { core::ptr::read_unaligned(map.cast::<u32>()) }; let size = unsafe { core::ptr::read_unaligned(map.add(4).cast::<u32>()) } as usize;
 let count = unsafe { core::ptr::read_unaligned(map.add(12).cast::<u32>()) } as usize; let off = unsafe { core::ptr::read_unaligned(map.add(16).cast::<u32>()) } as usize;
 if version != 6 || size < 28 || size > 16*1024*1024 || count > 65536 || count.checked_mul(24).and_then(|v| off.checked_add(v)).filter(|v| *v <= size).is_none() { return Ok(false); }
 let fold = |c: u16| if (65..=90).contains(&c) { c+32 } else { c };
 for i in 0..count { let e = unsafe { map.add(off+i*24) }; let no = unsafe { core::ptr::read_unaligned(e.add(4).cast::<u32>()) } as usize; let nl = unsafe { core::ptr::read_unaligned(e.add(8).cast::<u32>()) } as usize;
  if nl == 0 || nl&1 != 0 || no.checked_add(nl).filter(|v| *v <= size).is_none() || nl/2 != chars { continue; }
  let mut eq=true; for j in 0..chars { let c=unsafe { core::ptr::read_unaligned(map.add(no+j*2).cast::<u16>()) }; if fold(c)!=fold(wanted[j]) {eq=false;break;} } if eq { let vo=unsafe{core::ptr::read_unaligned(e.add(16).cast::<u32>())} as usize; let vc=unsafe{core::ptr::read_unaligned(e.add(20).cast::<u32>())} as usize; if vc==0 || vc>65536 || vc.checked_mul(20).and_then(|v|vo.checked_add(v)).filter(|v|*v<=size).is_none(){return Ok(false);} return Ok(true); }
 } Ok(false)
}

fn strip_dll_suffix(v: &[u16]) -> &[u16] { if v.len() >= 4 && v[v.len()-4..].iter().zip([46u16,100,108,108]).all(|(a,b)| (*a | 32) == b) { &v[..v.len()-4] } else { v } }
const STATUS_NOT_FOUND: i32 = 0xC0000225u32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC0000023u32 as i32;
#[no_mangle]
pub unsafe extern "system" fn ApiSetGetImplementationHost(name: *const UnicodeString, parent_name: *const UnicodeString, host_name: *mut UnicodeString) -> i32 {
    if name.is_null() || host_name.is_null() { return STATUS_INVALID_PARAMETER; }
    let requested = unsafe { &*name };
    let output = unsafe { &mut *host_name };
    if requested.buffer.is_null() || requested.length == 0 || requested.length & 1 != 0 || requested.length > requested.maximum_length || output.buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let wanted = unsafe { core::slice::from_raw_parts(requested.buffer, requested.length as usize / 2) };
    let wanted = strip_dll_suffix(wanted);
    let peb: *const u8;
    unsafe { core::arch::asm!("mov {}, gs:[0x60]", out(reg) peb, options(nostack, preserves_flags, readonly)); }
    if peb.is_null() { return STATUS_NOT_FOUND; }
    let map = unsafe { core::ptr::read_unaligned(peb.add(0x68).cast::<usize>()) } as *const u8;
    if map.is_null() { return STATUS_NOT_FOUND; }
    let version = unsafe { core::ptr::read_unaligned(map.cast::<u32>()) };
    let size = unsafe { core::ptr::read_unaligned(map.add(4).cast::<u32>()) } as usize;
    let count = unsafe { core::ptr::read_unaligned(map.add(12).cast::<u32>()) } as usize;
    let entry_off = unsafe { core::ptr::read_unaligned(map.add(16).cast::<u32>()) } as usize;
    if version != 6 || size < 28 || size > 16*1024*1024 || count > 65536 || count.checked_mul(24).and_then(|n|entry_off.checked_add(n)).filter(|n|*n<=size).is_none() { return STATUS_NOT_FOUND; }
    let fold = |c:u16| if (65..=90).contains(&c) { c+32 } else { c };
    let mut selected_value: Option<(usize,usize)> = None;
    for i in 0..count {
        let e = unsafe { map.add(entry_off+i*24) };
        let no = unsafe { core::ptr::read_unaligned(e.add(4).cast::<u32>()) } as usize;
        let nl = unsafe { core::ptr::read_unaligned(e.add(8).cast::<u32>()) } as usize;
        if nl == 0 || nl&1 != 0 || no.checked_add(nl).filter(|v|*v<=size).is_none() || nl/2 != wanted.len() { continue; }
        let mut equal=true;
        for j in 0..wanted.len() { let c=unsafe{core::ptr::read_unaligned(map.add(no+j*2).cast::<u16>())}; if fold(c)!=fold(wanted[j]) {equal=false;break;} }
        if !equal {continue;}
        let vo=unsafe{core::ptr::read_unaligned(e.add(16).cast::<u32>())} as usize;
        let vc=unsafe{core::ptr::read_unaligned(e.add(20).cast::<u32>())} as usize;
        if vc==0 || vc>65536 || vc.checked_mul(20).and_then(|n|vo.checked_add(n)).filter(|n|*n<=size).is_none(){return STATUS_NOT_FOUND;}
        let parent = if parent_name.is_null(){&[][..]}else{let p=unsafe{&*parent_name};if p.buffer.is_null()||p.length&1!=0||p.length>p.maximum_length{return STATUS_INVALID_PARAMETER;}strip_dll_suffix(unsafe{core::slice::from_raw_parts(p.buffer,p.length as usize/2)})};
        let mut default:Option<(usize,usize)>=None;
        for j in 0..vc {
            let v=unsafe{map.add(vo+j*20)};
            let pno=unsafe{core::ptr::read_unaligned(v.add(4).cast::<u32>())} as usize;
            let pnl=unsafe{core::ptr::read_unaligned(v.add(8).cast::<u32>())} as usize;
            let hno=unsafe{core::ptr::read_unaligned(v.add(12).cast::<u32>())} as usize;
            let hnl=unsafe{core::ptr::read_unaligned(v.add(16).cast::<u32>())} as usize;
            if hnl==0||hnl&1!=0||hno.checked_add(hnl).filter(|n|*n<=size).is_none()||pnl&1!=0||pno.checked_add(pnl).filter(|n|*n<=size).is_none(){continue;}
            if pnl==0 {default=Some((hno,hnl));continue;}
            if parent.len()==pnl/2 {
                let mut match_parent=true;
                for k in 0..parent.len(){let c=unsafe{core::ptr::read_unaligned(map.add(pno+k*2).cast::<u16>())};if fold(c)!=fold(parent[k]){match_parent=false;break;}}
                if match_parent {selected_value=Some((hno,hnl));break;}
            }
        }
        if selected_value.is_none(){selected_value=default;}
        break;
    }
    let Some((host_off,host_len))=selected_value else{return STATUS_NOT_FOUND;};
    if host_len>u16::MAX as usize {return STATUS_NOT_FOUND;}
    if (output.maximum_length as usize) < host_len { output.length=host_len as u16; return STATUS_BUFFER_TOO_SMALL; }
    unsafe { core::ptr::copy_nonoverlapping(map.add(host_off).cast::<u16>(), output.buffer.cast_mut(), host_len/2); }
    output.length=host_len as u16;
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn ApiSetQueryApiSetPresence(name: *const UnicodeString, present: *mut u8) -> i32 { if present.is_null() { return STATUS_INVALID_PARAMETER; } match unsafe { apiset_schema_has(name) } { Ok(v) => { unsafe { present.write(v as u8); } STATUS_SUCCESS }, Err(e) => e } }

#[no_mangle]
pub unsafe extern "system" fn ApiSetQueryApiSetPresenceEx(name: *const UnicodeString, in_schema: *mut u8, present: *mut u8) -> i32 { if name.is_null() || in_schema.is_null() || present.is_null() { return STATUS_INVALID_PARAMETER; } let n=unsafe{&*name}; if n.length&1!=0 || n.length>n.maximum_length || n.buffer.is_null() {return STATUS_INVALID_PARAMETER;} let u=unsafe{core::slice::from_raw_parts(n.buffer,n.length as usize/2)}; if u.iter().any(|c|*c==46) {return STATUS_INVALID_PARAMETER;} match unsafe{apiset_schema_has(name)} {Ok(v)=>{unsafe{in_schema.write(v as u8);present.write(v as u8);} STATUS_SUCCESS},Err(e)=>e} }

#[no_mangle]
pub unsafe extern "system" fn ApiSetQuerySchema(schema: *mut *const u8, schema_size: *mut u32) -> i32 {
    if schema.is_null() || schema_size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }

    // On Windows x64, PEB.ApiSetMap is at offset 0x68. The schema is owned
    // by the process loader; this routine returns a borrowed pointer and does
    // not allocate, copy, or transfer ownership of the mapping.
    let peb: *const u8;
    unsafe {
        core::arch::asm!(
            "mov {}, gs:[0x60]",
            out(reg) peb,
            options(nostack, preserves_flags, readonly)
        );
    }
    if peb.is_null() {
        return STATUS_NOT_FOUND;
    }
    let map = unsafe {
        core::ptr::read_unaligned(peb.add(0x68).cast::<usize>())
    } as *const u8;
    if map.is_null() {
        return STATUS_NOT_FOUND;
    }

    let version = unsafe { core::ptr::read_unaligned(map.cast::<u32>()) };
    let size = unsafe { core::ptr::read_unaligned(map.add(4).cast::<u32>()) };
    // Validate the fixed v6 header before exposing the pointer. Detailed
    // entry validation remains the responsibility of the lookup routines.
    if version != 6 || size < 28 || size as usize > 16 * 1024 * 1024 {
        return STATUS_NOT_FOUND;
    }

    unsafe {
        schema.write(map);
        schema_size.write(size);
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn ExpInterlockedPopEntrySListEnd() -> i32 {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ExpInterlockedPopEntrySListFault() -> i32 {
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ExpInterlockedPopEntrySListResume() -> i32 {
    STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn KiUserInvertedFunctionTable() -> i32 {
    STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub static mut NlsAnsiCodePage: u16 = 1252; // Default ANSI code page until NLS configuration is loaded.

#[no_mangle]
pub static mut NlsMbCodePageTag: u8 = 0; // Single-byte default; updated by NLS initialization.

#[no_mangle]
pub static mut NlsMbOemCodePageTag: u8 = 0; // Single-byte OEM default; updated by NLS initialization.






















#[no_mangle]
pub unsafe extern "system" fn vswprintf_s() -> i32 {
    STATUS_NOT_IMPLEMENTED
}

































// Consolidated sorting, searching and optional SQM compatibility helpers.
pub mod compat {
//! Source-first Rust port of the architecture-independent helpers in Wine's
//! `dlls/ntdll/misc.c`.
//!
//! The sorting/searching APIs preserve the C callback ABI. ETW and SQM entry
//! points below intentionally preserve Wine's current stub return behavior;
//! they are compatibility placeholders, not an event-tracing implementation.
//! The debug-only `debugstr_us` helper and C variadic `EtwTraceMessage`
//! wrapper are omitted because they depend on Wine's debug runtime / C varargs.
//! This module is not linked into the freestanding CognacOS PE32+ build yet.

#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::ptr;

pub type SIZE_T = usize;
pub type SSIZE_T = isize;
pub type ULONG = u32;
pub type DWORD = u32;
pub type UCHAR = u8;
pub type BOOLEAN = u8;
pub type BOOL = i32;
pub type NTSTATUS = i32;
pub type HANDLE = *mut c_void;
pub type REGHANDLE = u64;
pub type TRACEHANDLE = u64;
pub type GUID = [u8; 16];

pub const ERROR_SUCCESS: ULONG = 0;
pub const ERROR_INVALID_PARAMETER: ULONG = 87;
pub const ERROR_CALL_NOT_IMPLEMENTED: ULONG = 120;
pub const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
pub const INVALID_HANDLE_VALUE: HANDLE = usize::MAX as HANDLE;
pub const INVALID_PROCESSTRACE_HANDLE: TRACEHANDLE = u64::MAX;
pub const TRACE_LEVEL_VERBOSE: UCHAR = 5;

pub type CompareS = unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void) -> i32;
pub type Compare = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;

/// Overflow-safe byte offset helper for element arrays.
#[inline]
fn offset(index: usize, size: usize) -> Option<usize> {
    index.checked_mul(size)
}

#[inline]
unsafe fn swap_bytes(a: *mut u8, b: *mut u8, size: usize) {
    if a == b || size == 0 { return; }
    for i in 0..size {
        // SAFETY: caller guarantees both element ranges are valid for size bytes.
        unsafe { ptr::swap(a.add(i), b.add(i)); }
    }
}

/// Sort an array using a context-aware comparator. Invalid arguments are ignored,
/// matching Wine's defensive early returns.
pub unsafe fn qsort_s(
    base: *mut c_void,
    nmemb: SIZE_T,
    size: SIZE_T,
    compar: Option<CompareS>,
    context: *mut c_void,
) {
    let Some(cmp) = compar else { return };
    if (base.is_null() && nmemb != 0) || size == 0 || nmemb < 2 || nmemb.checked_mul(size).is_none() {
        return;
    }
    // Introspective quicksort is unnecessary here; heapsort guarantees O(n log n)
    // and bounded stack use even for adversarial input.
    let bytes = base.cast::<u8>();
    unsafe {
        let less = |i: usize, j: usize| unsafe { cmp(context, bytes.add(i * size).cast(), bytes.add(j * size).cast()) < 0 };
        let sift_down = |mut root: usize, end: usize| {
            loop {
                let child = root.saturating_mul(2).saturating_add(1);
                if child >= end { break; }
                let mut larger = child;
                if child + 1 < end && less(child, child + 1) { larger = child + 1; }
                if !less(root, larger) { break; }
                unsafe { swap_bytes(bytes.add(root * size), bytes.add(larger * size), size); }
                root = larger;
            }
        };
        for start in (0..(nmemb / 2)).rev() { sift_down(start, nmemb); }
        for end in (1..nmemb).rev() {
            unsafe { swap_bytes(bytes, bytes.add(end * size), size); }
            sift_down(0, end);
        }
    }
}

/// C `qsort` adapter for a two-argument comparator.
pub unsafe fn qsort(
    base: *mut c_void,
    nmemb: SIZE_T,
    size: SIZE_T,
    compar: Option<Compare>,
) {
    let Some(cmp) = compar else { return };
    unsafe extern "C" fn adapter(
        ctx: *mut c_void,
        a: *const c_void,
        b: *const c_void,
    ) -> i32 {
        // The context stores the function pointer passed by qsort.
        let cmp = unsafe { *(ctx.cast::<Option<Compare>>()) };
        match cmp { Some(f) => unsafe { f(a, b) }, None => 0 }
    }
    let mut cmp_ctx = compar;
    unsafe { qsort_s(base, nmemb, size, Some(adapter), (&mut cmp_ctx as *mut Option<Compare>).cast()); }
}

/// Context-aware binary search. Returns a pointer to a matching element or null.
pub unsafe fn bsearch_s(
    key: *const c_void,
    base: *const c_void,
    nmemb: SIZE_T,
    size: SIZE_T,
    compare: Option<CompareS>,
    ctx: *mut c_void,
) -> *mut c_void {
    let Some(cmp) = compare else { return ptr::null_mut() };
    if size == 0 || (base.is_null() && nmemb != 0) || (key.is_null() && nmemb != 0) {
        return ptr::null_mut();
    }
    let mut low = 0usize;
    let mut high = nmemb;
    let bytes = base.cast::<u8>();
    while low < high {
        let mid = low + (high - low) / 2;
        let Some(off) = offset(mid, size) else { return ptr::null_mut() };
        let element = unsafe { bytes.add(off).cast::<c_void>() };
        let result = unsafe { cmp(ctx, key, element) };
        if result == 0 { return element.cast_mut(); }
        if result < 0 { high = mid; } else { low = mid + 1; }
    }
    ptr::null_mut()
}

/// Two-argument comparator variant of binary search.
pub unsafe fn bsearch(
    key: *const c_void,
    base: *const c_void,
    nmemb: SIZE_T,
    size: SIZE_T,
    compar: Option<Compare>,
) -> *mut c_void {
    let Some(cmp) = compar else { return ptr::null_mut() };
    unsafe extern "C" fn adapter(
        ctx: *mut c_void,
        a: *const c_void,
        b: *const c_void,
    ) -> i32 {
        let cmp = unsafe { *(ctx.cast::<Option<Compare>>()) };
        match cmp { Some(f) => unsafe { f(a, b) }, None => 0 }
    }
    let mut cmp_ctx = compar;
    unsafe { bsearch_s(key, base, nmemb, size, Some(adapter), (&mut cmp_ctx as *mut Option<Compare>).cast()) }
}

/// Linear search using the C comparator. The element count is not modified.
pub unsafe fn _lfind(
    key: *const c_void,
    base: *const c_void,
    nmemb: *const u32,
    size: SIZE_T,
    compar: Option<Compare>,
) -> *mut c_void {
    let Some(cmp) = compar else { return ptr::null_mut() };
    if nmemb.is_null() || (base.is_null() && unsafe { *nmemb } != 0) || size == 0 {
        return ptr::null_mut();
    }
    let count = unsafe { *nmemb as usize };
    let bytes = base.cast::<u8>();
    for i in 0..count {
        let Some(off) = offset(i, size) else { return ptr::null_mut() };
        let item = unsafe { bytes.add(off).cast::<c_void>() };
        if unsafe { cmp(key, item) } == 0 { return item.cast_mut(); }
    }
    ptr::null_mut()
}

// SQM exports: these are deliberate Wine-compatible stubs, not telemetry storage.
pub unsafe fn WinSqmEndSession(_session: HANDLE) -> NTSTATUS { STATUS_NOT_IMPLEMENTED }
pub unsafe fn WinSqmIncrementDWORD(_a: DWORD, _b: DWORD, _c: DWORD) {}
pub unsafe fn WinSqmIsOptedIn() -> BOOL { 0 }
pub unsafe fn WinSqmStartSession(_guid: *mut GUID, _session_id: DWORD, _unknown: DWORD) -> HANDLE {
    INVALID_HANDLE_VALUE
}
pub unsafe fn WinSqmSetDWORD(_session: HANDLE, _id: DWORD, _value: DWORD) {}
pub unsafe fn WinSqmSetIfMaxDWORD(_a: DWORD, _b: DWORD, _c: DWORD) {}


#[repr(C)]
pub struct TraceGuidRegistration {
    pub guid: *const GUID,
    pub reg_handle: *mut c_void,
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn cmp_i32(a: *const c_void, b: *const c_void) -> i32 {
        let a = unsafe { *a.cast::<i32>() };
        let b = unsafe { *b.cast::<i32>() };
        a.cmp(&b) as i32
    }

    unsafe extern "C" fn cmp_i32_ctx(_ctx: *mut c_void, a: *const c_void, b: *const c_void) -> i32 {
        unsafe { cmp_i32(a, b) }
    }

    #[test]
    fn qsort_orders_values_and_handles_duplicates() {
        let mut values = [7i32, -1, 4, 4, 0, i32::MIN, i32::MAX];
        unsafe { qsort(values.as_mut_ptr().cast(), values.len(), core::mem::size_of::<i32>(), Some(cmp_i32)); }
        assert_eq!(values, [i32::MIN, -1, 0, 4, 4, 7, i32::MAX]);
    }

    #[test]
    fn qsort_s_empty_and_invalid_arguments_are_noops() {
        let mut values = [2i32, 1];
        unsafe { qsort_s(values.as_mut_ptr().cast(), 2, 4, Some(cmp_i32_ctx), ptr::null_mut()); }
        assert_eq!(values, [1, 2]);
        unsafe { qsort_s(values.as_mut_ptr().cast(), 2, 0, Some(cmp_i32_ctx), ptr::null_mut()); }
        assert_eq!(values, [1, 2]);
        unsafe { qsort_s(ptr::null_mut(), 3, 4, Some(cmp_i32_ctx), ptr::null_mut()); }
    }

    #[test]
    fn binary_search_finds_values_and_rejects_empty_or_invalid_input() {
        let values = [-8i32, -1, 3, 9, 20];
        let key = 9i32;
        let found = unsafe { bsearch((&key as *const i32).cast(), values.as_ptr().cast(), values.len(), 4, Some(cmp_i32)) };
        assert_eq!(unsafe { found.cast::<i32>().read() }, 9);
        let absent = 10i32;
        assert!(unsafe { bsearch((&absent as *const i32).cast(), values.as_ptr().cast(), values.len(), 4, Some(cmp_i32)) }.is_null());
        assert!(unsafe { bsearch((&key as *const i32).cast(), values.as_ptr().cast(), 0, 4, Some(cmp_i32)) }.is_null());
        assert!(unsafe { bsearch((&key as *const i32).cast(), values.as_ptr().cast(), 5, 0, Some(cmp_i32)) }.is_null());
    }

    #[test]
    fn linear_search_returns_first_matching_element() {
        let values = [5i32, 2, 5, 7];
        let key = 5i32;
        let count = values.len() as u32;
        let found = unsafe { _lfind((&key as *const i32).cast(), values.as_ptr().cast(), &count, 4, Some(cmp_i32)) };
        assert_eq!(unsafe { found.cast::<i32>().read() }, 5);
        let missing = 99i32;
        assert!(unsafe { _lfind((&missing as *const i32).cast(), values.as_ptr().cast(), &count, 4, Some(cmp_i32)) }.is_null());
    }

    #[test]
    fn etw_registration_stub_validates_output_pointer() {
        assert_eq!(unsafe { EtwEventRegister(ptr::null(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) }, ERROR_INVALID_PARAMETER);
        let mut handle = 0;
        assert_eq!(unsafe { EtwEventRegister(ptr::null(), ptr::null_mut(), ptr::null_mut(), &mut handle) }, ERROR_SUCCESS);
        assert_eq!(handle, 0xdead_beef);
    }
}

}
pub use compat::{qsort, qsort_s, bsearch, bsearch_s, _lfind, WinSqmEndSession, WinSqmIncrementDWORD, WinSqmIsOptedIn, WinSqmStartSession, WinSqmSetDWORD, WinSqmSetIfMaxDWORD};

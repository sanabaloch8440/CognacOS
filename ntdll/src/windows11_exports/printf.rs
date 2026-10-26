//! Windows x64 NTDLL printf-family ABI bridges.
//!
//! The public variadic entry points are small assembly shims because stable Rust
//! cannot define C-variadic functions. They home the Windows x64 argument
//! registers and pass a native va_list cursor to Rust helpers. The helpers use
//! the in-tree typed formatter and never call CRT/NTDLL formatting exports.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use alloc::{string::String, vec::Vec};
use core::ptr;
use crate::printf::{self as formatter, Argument, Length};

const MAX_FORMAT_UNITS: usize = 65_536;
const MAX_STRING_UNITS: usize = 65_536;

#[derive(Debug)]
enum OwnedArg {
    Signed(i64),
    Unsigned(u64),
    Pointer(usize),
    Char(u32),
    Ansi(Vec<u8>),
    Wide(Vec<u16>),
    Float(f64),
}

unsafe fn read_c_bytes(p: *const u8, max: usize) -> Option<Vec<u8>> {
    if p.is_null() { return None; }
    let mut out = Vec::new();
    for i in 0..max {
        let b = unsafe { p.add(i).read() };
        if b == 0 { return Some(out); }
        out.push(b);
    }
    None
}

unsafe fn read_c_wide(p: *const u16, max: usize) -> Option<Vec<u16>> {
    if p.is_null() { return None; }
    let mut out = Vec::new();
    for i in 0..max {
        let u = unsafe { p.add(i).read() };
        if u == 0 { return Some(out); }
        out.push(u);
    }
    None
}

unsafe fn next_slot(cursor: &mut *const u8) -> Option<u64> {
    if cursor.is_null() || (*cursor).is_null() { return None; }
    let v = unsafe { ptr::read_unaligned((*cursor).cast::<u64>()) };
    *cursor = unsafe { (*cursor).add(8) };
    Some(v)
}

unsafe fn format_narrow(format_ptr: *const u8, va: *const u8) -> Option<Vec<u8>> {
    let fmt = unsafe { read_c_bytes(format_ptr, MAX_FORMAT_UNITS)? };
    let specs = formatter::parse_format(&fmt).ok()?;
    let mut cursor = va;
    let mut owned: Vec<OwnedArg> = Vec::new();
    for s in specs.iter() {
        if s.conversion == b'%' { continue; }
        if s.width_from_arg {
            owned.push(OwnedArg::Signed(unsafe { next_slot(&mut cursor)? } as u32 as i32 as i64));
        }
        if s.precision_from_arg {
            owned.push(OwnedArg::Signed(unsafe { next_slot(&mut cursor)? } as u32 as i32 as i64));
        }
        let raw = unsafe { next_slot(&mut cursor)? };
        owned.push(match s.conversion {
            b'd' | b'i' => OwnedArg::Signed(match s.length {
                Length::LongLong | Length::Native => raw as i64,
                _ => raw as u32 as i32 as i64,
            }),
            b'u' | b'o' | b'x' | b'X' => OwnedArg::Unsigned(match s.length {
                Length::LongLong | Length::Native => raw,
                _ => raw as u32 as u64,
            }),
            b'p' => OwnedArg::Pointer(raw as usize),
            b'c' | b'C' => OwnedArg::Char(raw as u32),
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => OwnedArg::Float(f64::from_bits(raw)),
            b's' | b'S' if s.wide_string => OwnedArg::Wide(unsafe { read_c_wide(raw as *const u16, MAX_STRING_UNITS)? }),
            b's' | b'S' => OwnedArg::Ansi(unsafe { read_c_bytes(raw as *const u8, MAX_STRING_UNITS)? }),
            _ => return None,
        });
    }
    let mut args: Vec<Argument<'_>> = Vec::with_capacity(owned.len());
    for a in owned.iter() {
        args.push(match a {
            OwnedArg::Signed(v) => Argument::Signed(*v),
            OwnedArg::Unsigned(v) => Argument::Unsigned(*v),
            OwnedArg::Pointer(v) => Argument::Pointer(*v),
            OwnedArg::Char(v) => Argument::Char(*v),
            OwnedArg::Ansi(v) => Argument::Ansi(v),
            OwnedArg::Wide(v) => Argument::Wide(v),
            OwnedArg::Float(v) => Argument::Float(*v),
        });
    }
    formatter::format_bytes(&fmt, &args, None).ok().map(|r| r.bytes)
}

unsafe fn format_wide(format_ptr: *const u16, va: *const u8) -> Option<Vec<u16>> {
    let wide_fmt = unsafe { read_c_wide(format_ptr, MAX_FORMAT_UNITS)? };
    // Convert the UTF-16 format to UTF-8 before parsing. Format directives
    // are ASCII, while non-ASCII literal characters must survive unchanged.
    // Reject malformed surrogate sequences rather than silently replacing
    // them; the resulting UTF-8 bytes are decoded back to UTF-16 below.
    let fmt: Vec<u8> = String::from_utf16(&wide_fmt).ok()?.into_bytes();
    let specs = formatter::parse_format(&fmt).ok()?;
    let mut cursor = va;
    let mut owned: Vec<OwnedArg> = Vec::new();
    for s in specs.iter() {
        if s.conversion == b'%' { continue; }
        if s.width_from_arg { owned.push(OwnedArg::Signed(unsafe { next_slot(&mut cursor)? } as u32 as i32 as i64)); }
        if s.precision_from_arg { owned.push(OwnedArg::Signed(unsafe { next_slot(&mut cursor)? } as u32 as i32 as i64)); }
        let raw = unsafe { next_slot(&mut cursor)? };
        owned.push(match s.conversion {
            b'd' | b'i' => OwnedArg::Signed(match s.length { Length::LongLong | Length::Native => raw as i64, _ => raw as u32 as i32 as i64 }),
            b'u' | b'o' | b'x' | b'X' => OwnedArg::Unsigned(match s.length { Length::LongLong | Length::Native => raw, _ => raw as u32 as u64 }),
            b'p' => OwnedArg::Pointer(raw as usize),
            b'c' | b'C' => OwnedArg::Char(raw as u32),
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => OwnedArg::Float(f64::from_bits(raw)),
            b's' | b'S' if s.length != Length::Short => OwnedArg::Wide(unsafe { read_c_wide(raw as *const u16, MAX_STRING_UNITS)? }),
            b's' | b'S' => OwnedArg::Ansi(unsafe { read_c_bytes(raw as *const u8, MAX_STRING_UNITS)? }),
            _ => return None,
        });
    }
    let mut args: Vec<Argument<'_>> = Vec::with_capacity(owned.len());
    for a in owned.iter() {
        args.push(match a {
            OwnedArg::Signed(v) => Argument::Signed(*v),
            OwnedArg::Unsigned(v) => Argument::Unsigned(*v),
            OwnedArg::Pointer(v) => Argument::Pointer(*v),
            OwnedArg::Char(v) => Argument::Char(*v),
            OwnedArg::Ansi(v) => Argument::Ansi(v),
            OwnedArg::Wide(v) => Argument::Wide(v),
            OwnedArg::Float(v) => Argument::Float(*v),
        });
    }
    let bytes = formatter::format_bytes(&fmt, &args, None).ok()?.bytes;
    let text = core::str::from_utf8(&bytes).ok()?;
    Some(text.encode_utf16().collect())
}

unsafe fn write_narrow(dst: *mut u8, cap: Option<usize>, text: &[u8], secure: bool) -> i32 {
    if dst.is_null() { return -1; }
    if text.len().checked_add(1).is_none() { return -1; }
    if let Some(capacity) = cap {
        if capacity == 0 || (secure && text.len() >= capacity) {
            if capacity > 0 { unsafe { dst.write(0) }; }
            return -1;
        }
        let n = text.len().min(capacity - 1);
        unsafe { ptr::copy_nonoverlapping(text.as_ptr(), dst, n); dst.add(n).write(0); }
        if secure && n != text.len() { unsafe { dst.write(0) }; return -1; }
        return if n == text.len() { text.len() as i32 } else { -1 };
    }
    unsafe { ptr::copy_nonoverlapping(text.as_ptr(), dst, text.len()); dst.add(text.len()).write(0); }
    text.len().try_into().ok().unwrap_or(-1)
}

unsafe fn write_wide(dst: *mut u16, cap: Option<usize>, text: &[u16], secure: bool) -> i32 {
    if dst.is_null() { return -1; }
    if text.len().checked_add(1).is_none() { return -1; }
    if let Some(capacity) = cap {
        if capacity == 0 || (secure && text.len() >= capacity) {
            if capacity > 0 { unsafe { dst.write(0) }; }
            return -1;
        }
        let n = text.len().min(capacity - 1);
        unsafe { ptr::copy_nonoverlapping(text.as_ptr(), dst, n); dst.add(n).write(0); }
        if secure && n != text.len() { unsafe { dst.write(0) }; return -1; }
        return if n == text.len() { text.len() as i32 } else { -1 };
    }
    unsafe { ptr::copy_nonoverlapping(text.as_ptr(), dst, text.len()); dst.add(text.len()).write(0); }
    text.len().try_into().ok().unwrap_or(-1)
}

unsafe fn format_wide_entry(dst: *mut u16, cap: Option<usize>, fmt: *const u16, va: *const u8, secure: bool) -> i32 {
    let Some(out) = (unsafe { format_wide(fmt, va) }) else {
        if secure && !dst.is_null() && cap.unwrap_or(0) > 0 { unsafe { dst.write(0) }; }
        return -1;
    };
    unsafe { write_wide(dst, cap, &out, secure) }
}

#[inline(never)]
unsafe extern "system" fn vscprintf_impl(fmt: *const u8, va: *const u8) -> i32 {
    let Some(out) = (unsafe { format_narrow(fmt, va) }) else { return -1; };
    out.len().try_into().ok().unwrap_or(-1)
}

#[inline(never)]
unsafe extern "system" fn vscwprintf_impl(fmt: *const u16, va: *const u8) -> i32 {
    let Some(out) = (unsafe { format_wide(fmt, va) }) else { return -1; };
    out.len().try_into().ok().unwrap_or(-1)
}

#[inline(never)]
unsafe extern "system" fn sprintf_impl(dst: *mut u8, fmt: *const u8, va: *const u8) -> i32 {
    let Some(out) = (unsafe { format_narrow(fmt, va) }) else { return -1; };
    unsafe { write_narrow(dst, None, &out, false) }
}

#[inline(never)]
unsafe extern "system" fn sprintf_s_impl(dst: *mut u8, cap: usize, fmt: *const u8, va: *const u8) -> i32 {
    let Some(out) = (unsafe { format_narrow(fmt, va) }) else {
        if !dst.is_null() && cap > 0 { unsafe { dst.write(0) }; }
        return -1;
    };
    unsafe { write_narrow(dst, Some(cap), &out, true) }
}

#[inline(never)]
unsafe extern "system" fn swprintf_impl(dst: *mut u16, fmt: *const u16, va: *const u8) -> i32 {
    let Some(out) = (unsafe { format_wide(fmt, va) }) else { return -1; };
    unsafe { write_wide(dst, None, &out, false) }
}

#[inline(never)]
unsafe extern "system" fn swprintf_s_impl(dst: *mut u16, cap: usize, fmt: *const u16, va: *const u8) -> i32 {
    unsafe { format_wide_entry(dst, Some(cap), fmt, va, true) }
}

#[cfg(target_os = "windows")]
core::arch::global_asm!(r#"
    .text
    .p2align 4
    .globl _vscprintf
_vscprintf:
    subq $40, %rsp
    callq {vscprintf_impl}
    addq $40, %rsp
    retq

    .p2align 4
    .globl _vscwprintf
_vscwprintf:
    subq $40, %rsp
    callq {vscwprintf_impl}
    addq $40, %rsp
    retq

    .p2align 4
    .globl sprintf
sprintf:
    movq %r8, 24(%rsp)
    movq %r9, 32(%rsp)
    subq $40, %rsp
    movq %rcx, %r10
    movq %rdx, %r11
    leaq 64(%rsp), %r8
    movq %r10, %rcx
    movq %r11, %rdx
    callq {sprintf_impl}
    addq $40, %rsp
    retq

    .p2align 4
    .globl sprintf_s
sprintf_s:
    movq %r9, 32(%rsp)
    subq $40, %rsp
    movq %rcx, %r10
    movq %rdx, %r11
    movq %r8, %rax
    leaq 72(%rsp), %r9
    movq %r10, %rcx
    movq %r11, %rdx
    movq %rax, %r8
    callq {sprintf_s_impl}
    addq $40, %rsp
    retq

    .p2align 4
    .globl swprintf
swprintf:
    movq %r8, 24(%rsp)
    movq %r9, 32(%rsp)
    subq $40, %rsp
    movq %rcx, %r10
    movq %rdx, %r11
    leaq 64(%rsp), %r8
    movq %r10, %rcx
    movq %r11, %rdx
    callq {swprintf_impl}
    addq $40, %rsp
    retq

    .p2align 4
    .globl swprintf_s
swprintf_s:
    movq %r9, 32(%rsp)
    subq $40, %rsp
    movq %rcx, %r10
    movq %rdx, %r11
    movq %r8, %rax
    leaq 72(%rsp), %r9
    movq %r10, %rcx
    movq %r11, %rdx
    movq %rax, %r8
    callq {swprintf_s_impl}
    addq $40, %rsp
    retq

    # The scanf family is ABI-callable but intentionally reports EOF until a
    # bounds-checked scanf parser is implemented; do not return NTSTATUS values.
    .p2align 4
    .globl sscanf
sscanf:
    movl $-1, %eax
    retq
    .p2align 4
    .globl sscanf_s
sscanf_s:
    movl $-1, %eax
    retq
    .p2align 4
    .globl swscanf_s
swscanf_s:
    movl $-1, %eax
    retq
"#,
    vscprintf_impl = sym vscprintf_impl,
    vscwprintf_impl = sym vscwprintf_impl,
    sprintf_impl = sym sprintf_impl,
    sprintf_s_impl = sym sprintf_s_impl,
    swprintf_impl = sym swprintf_impl,
    swprintf_s_impl = sym swprintf_s_impl,
    options(att_syntax)
);


// Windows x64 va_list entry point for the CRT-compatible vsprintf export.
#[no_mangle]
pub unsafe extern "system" fn vsprintf(
    destination: *mut u8,
    format: *const u8,
    arguments: *const u8,
) -> i32 {
    let Some(output) = (unsafe { format_narrow(format, arguments) }) else { return -1; };
    unsafe { write_narrow(destination, None, &output, false) }
}

// Bounded secure CRT variant. Capacity includes the terminating NUL.
#[no_mangle]
pub unsafe extern "system" fn vsprintf_s(
    destination: *mut u8,
    capacity: usize,
    format: *const u8,
    arguments: *const u8,
) -> i32 {
    let Some(output) = (unsafe { format_narrow(format, arguments) }) else {
        if !destination.is_null() && capacity != 0 { unsafe { destination.write(0) }; }
        return -1;
    };
    unsafe { write_narrow(destination, Some(capacity), &output, true) }
}


// Declarations keep these assembly-defined export names out of the generated
// zero-argument fallback table. The entrypoints above accept arbitrary x64
// caller arguments even though they currently return EOF without parsing.
#[cfg(target_os = "windows")]
extern "system" {
    fn sscanf();
    fn sscanf_s();
    fn swscanf_s();
}

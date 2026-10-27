//! Windows x64 NTDLL printf-family ABI bridges.
//!
//! The public variadic entry points are small assembly shims because stable Rust
//! cannot define C-variadic functions. They home the Windows x64 argument
//! registers and pass a native va_list cursor to Rust helpers. The helpers use
//! the in-tree typed formatter and never call CRT/NTDLL formatting exports.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use alloc::{string::String, vec::Vec};
use core::ptr;
use crate::printf as formatter;

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


// Consolidated typed format parser/formatter formerly in legacy_printf.rs.
pub mod format_core {
//! Source-first Wine NTDLL printf formatter over typed arguments.
//! C variadic ABI shims require a target-specific bridge and are not yet linked
//! into the freestanding PE32+ build.
use alloc::{format, string::String, vec::Vec};
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)] pub struct FormatFlags{pub left_align:bool,pub alternate:bool,pub zero_pad:bool,pub sign:u8}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum Length{Default,Short,Long,LongLong,Native}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub struct FormatSpec{pub flags:FormatFlags,pub width:Option<usize>,pub width_from_arg:bool,pub precision:Option<usize>,pub precision_from_arg:bool,pub length:Length,pub wide_string:bool,pub conversion:u8,pub start:usize,pub end:usize}
#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum ParseError{TrailingPercent{offset:usize},InvalidWidth{offset:usize},InvalidPrecision{offset:usize},UnsupportedPositionalParameter{offset:usize},MissingConversion{offset:usize}}
#[derive(Clone,Copy,Debug,PartialEq)] pub enum Argument<'a>{Signed(i64),Unsigned(u64),Pointer(usize),Char(u32),Ansi(&'a[u8]),Wide(&'a[u16]),Float(f64)}
#[derive(Clone,Debug,PartialEq,Eq)] pub enum FormatError{Parse(ParseError),MissingArgument{index:usize,conversion:u8},WrongArgumentType{index:usize,conversion:u8},UnsupportedConversion{offset:usize,conversion:u8},OutputTooLarge}
#[derive(Clone,Debug,PartialEq,Eq)] pub struct FormatResult{pub bytes:Vec<u8>,pub required:usize,pub truncated:bool}
fn is_int(c:u8)->bool{matches!(c,b'd'|b'i'|b'o'|b'u'|b'x'|b'X')}
fn decimal(b:&[u8],mut p:usize)->Result<(usize,usize),ParseError>{let start=p;let mut n=0usize;while let Some(&c)=b.get(p){if !c.is_ascii_digit(){break}n=n.checked_mul(10).and_then(|x|x.checked_add((c-b'0')as usize)).ok_or(ParseError::InvalidWidth{offset:start})?;p+=1}Ok((n,p))}
pub fn parse_spec(f:&[u8],start:usize)->Result<FormatSpec,ParseError>{
 if f.get(start)!=Some(&b'%'){return Err(ParseError::MissingConversion{offset:start})}let mut p=start+1;if p>=f.len(){return Err(ParseError::TrailingPercent{offset:start})}
 if f[p]==b'%'{return Ok(FormatSpec{flags:FormatFlags::default(),width:None,width_from_arg:false,precision:None,precision_from_arg:false,length:Length::Default,wide_string:false,conversion:b'%',start,end:p+1})}
 let mut flags=FormatFlags::default();loop{match f.get(p).copied(){Some(b'+')|Some(b' ')=>{let c=f[p];if flags.sign!=b'+'{flags.sign=c}p+=1},Some(b'-')=>{flags.left_align=true;p+=1},Some(b'0')=>{flags.zero_pad=true;p+=1},Some(b'#')=>{flags.alternate=true;p+=1},_=>break}}
 let(width,width_from_arg)=if f.get(p)==Some(&b'*'){p+=1;(None,true)}else if f.get(p).is_some_and(u8::is_ascii_digit){let(v,q)=decimal(f,p)?;p=q;(Some(v),false)}else{(None,false)};
 let(precision,precision_from_arg)=if f.get(p)==Some(&b'.'){p+=1;if f.get(p)==Some(&b'*'){p+=1;(None,true)}else{let(v,q)=decimal(f,p).map_err(|_|ParseError::InvalidPrecision{offset:p})?;p=q;(Some(v),false)}}else{(None,false)};
 let mut length=Length::Default;let mut wide_string=false;loop{match f.get(p).copied(){Some(b'l')if f.get(p+1)==Some(&b'l')=>{length=Length::LongLong;p+=2},Some(b'l')=>{if length!=Length::Short{length=Length::Long}wide_string=true;p+=1},Some(b'h')=>{length=Length::Short;p+=1},Some(b'L'|b'F')=>p+=1,Some(b'I')if f.get(p+1..p+3)==Some(b"64")=>{length=Length::LongLong;p+=3},Some(b'I')if f.get(p+1..p+3)==Some(b"32")=>p+=3,Some(b'I')if f.get(p+1).is_some_and(|c|is_int(*c))=>{length=Length::Native;p+=1},Some(b'z'|b't')if f.get(p+1).is_some_and(|c|is_int(*c))=>{length=Length::Native;p+=1},Some(b'j')=>{length=Length::LongLong;p+=1},Some(b'w')=>{wide_string=true;p+=1},_=>break}}
 let c=*f.get(p).ok_or(ParseError::TrailingPercent{offset:start})?;if c==b'$'{return Err(ParseError::UnsupportedPositionalParameter{offset:p})}Ok(FormatSpec{flags,width,width_from_arg,precision,precision_from_arg,length,wide_string,conversion:c,start,end:p+1})
}
pub fn parse_format(f:&[u8])->Result<Vec<FormatSpec>,ParseError>{let mut v=Vec::new();let mut p=0;while p<f.len(){if f[p]!=b'%'{p+=1;continue}let s=parse_spec(f,p)?;p=s.end;v.push(s)}Ok(v)}
pub fn signed_decimal(v:i64)->([u8;20],usize,bool){let neg=v<0;let mut n=v.unsigned_abs();let mut b=[0;20];let mut l=0;loop{b[l]=b'0'+(n%10)as u8;l+=1;n/=10;if n==0{break}}b[..l].reverse();(b,l,neg)}
pub fn unsigned_digits(mut v:u64,base:u32,upper:bool)->Option<([u8;64],usize)>{if !(2..=16).contains(&base){return None}let a=if upper{b"0123456789ABCDEF"}else{b"0123456789abcdef"};let mut b=[0;64];let mut l=0;loop{b[l]=a[(v%base as u64)as usize];l+=1;v/=base as u64;if v==0{break}}b[..l].reverse();Some((b,l))}
fn get<'a>(a:&'a[Argument<'a>],i:usize,c:u8)->Result<Argument<'a>,FormatError>{a.get(i).copied().ok_or(FormatError::MissingArgument{index:i,conversion:c})}
fn signed(a:Argument<'_>,i:usize,c:u8)->Result<i64,FormatError>{match a{Argument::Signed(v)=>Ok(v),Argument::Unsigned(v)=>Ok(v as i64),_=>Err(FormatError::WrongArgumentType{index:i,conversion:c})}}
fn unsigned(a:Argument<'_>,i:usize,c:u8)->Result<u64,FormatError>{match a{Argument::Unsigned(v)=>Ok(v),Argument::Signed(v)=>Ok(v as u64),Argument::Pointer(v)=>Ok(v as u64),_=>Err(FormatError::WrongArgumentType{index:i,conversion:c})}}
fn pad(o:&mut Vec<u8>,s:&[u8],w:usize,left:bool,ch:u8)->Result<(),FormatError>{let n=w.saturating_sub(s.len());if o.len().checked_add(s.len()).and_then(|v|v.checked_add(n)).filter(|v|*v<=isize::MAX as usize).is_none(){return Err(FormatError::OutputTooLarge)}if !left{o.extend(core::iter::repeat(ch).take(n))}o.extend_from_slice(s);if left{o.extend(core::iter::repeat(b' ').take(n))}Ok(())}
fn integer(v:u64,neg:bool,base:u32,upper:bool,fl:FormatFlags,prec:Option<usize>,w:usize,ptr:bool,c:u8)->Vec<u8>{let(d,l)=unsigned_digits(v,base,upper).unwrap();let mut body=d[..l].to_vec();if v==0&&prec==Some(0)&&!ptr{body.clear()}let mut pre=Vec::new();if neg{pre.push(b'-')}else if matches!(c,b'd'|b'i')&&fl.sign!=0{pre.push(fl.sign)}if ptr{pre.extend_from_slice(b"0x")}else if fl.alternate&&v!=0{match base{8=>if body.first()!=Some(&b'0'){pre.push(b'0')},16=>pre.extend_from_slice(if upper{b"0X"}else{b"0x"}),_=>{}}}let z=prec.unwrap_or(0).saturating_sub(body.len());let mut out=pre.clone();if fl.zero_pad&&!fl.left_align&&prec.is_none()&&w>pre.len()+body.len(){out.extend(core::iter::repeat(b'0').take(w-pre.len()-body.len()));out.extend_from_slice(&body);return out}out.extend(core::iter::repeat(b'0').take(z));out.extend_from_slice(&body);out}
fn fit_numeric(mut v:Vec<u8>,width:usize,left:bool)->Vec<u8>{let n=width.saturating_sub(v.len());if left{v.extend(core::iter::repeat(b' ').take(n));v}else{let mut r=Vec::with_capacity(width);r.extend(core::iter::repeat(b' ').take(n));r.append(&mut v);r}}
fn append(o:&mut Vec<u8>,s:&[u8],cap:Option<usize>,req:&mut usize)->Result<(),FormatError>{*req=req.checked_add(s.len()).ok_or(FormatError::OutputTooLarge)?;let n=cap.map(|c|c.saturating_sub(o.len()).min(s.len())).unwrap_or(s.len());o.extend_from_slice(&s[..n]);Ok(())}
/// Format to bounded bytes; capacity excludes the final NUL and required counts untruncated bytes.
pub fn format_bytes(f:&[u8],args:&[Argument<'_>],capacity:Option<usize>)->Result<FormatResult,FormatError>{
 let(mut out,mut req,mut ai,mut p)=(Vec::new(),0usize,0usize,0usize);while p<f.len(){if f[p]!=b'%'{let e=f[p..].iter().position(|&x|x==b'%').map(|n|p+n).unwrap_or(f.len());append(&mut out,&f[p..e],capacity,&mut req)?;p=e;continue}
 let s=parse_spec(f,p).map_err(FormatError::Parse)?;p=s.end;if s.conversion==b'%'{append(&mut out,b"%",capacity,&mut req)?;continue}let mut fl=s.flags;let mut w=s.width.unwrap_or(0);
 if s.width_from_arg{let n=signed(get(args,ai,s.conversion)?,ai,s.conversion)?;ai+=1;if n<0{fl.left_align=true;w=n.unsigned_abs().min(usize::MAX as u64)as usize}else{w=n as usize}}
 let mut pr=s.precision;if s.precision_from_arg{let n=signed(get(args,ai,s.conversion)?,ai,s.conversion)?;ai+=1;pr=if n<0{None}else{Some(n as usize)}}fl.zero_pad&=!fl.left_align;let mut piece=Vec::new();
 match s.conversion{
 b'd'|b'i'=>{let v=signed(get(args,ai,s.conversion)?,ai,s.conversion)?;ai+=1;piece=fit_numeric(integer(v.unsigned_abs(),v<0,10,false,fl,pr,w,false,s.conversion),w,fl.left_align)}
 b'u'|b'o'|b'x'|b'X'=>{let v=unsigned(get(args,ai,s.conversion)?,ai,s.conversion)?;ai+=1;let base=match s.conversion{b'o'=>8,b'x'|b'X'=>16,_=>10};piece=fit_numeric(integer(v,false,base,s.conversion==b'X',fl,pr,w,false,s.conversion),w,fl.left_align)}
 b'p'=>{let v=unsigned(get(args,ai,b'p')?,ai,b'p')?;ai+=1;piece=fit_numeric(integer(v,false,16,false,fl,Some(core::mem::size_of::<usize>()*2),w,true,b'p'),w,fl.left_align)}
 b's'|b'S'=>{let a=get(args,ai,s.conversion)?;ai+=1;let mut v=match a{Argument::Ansi(x)=>x.to_vec(),Argument::Wide(x)=>String::from_utf16_lossy(x).into_bytes(),_=>return Err(FormatError::WrongArgumentType{index:ai-1,conversion:s.conversion})};if let Some(n)=pr{if n<v.len(){v.truncate(n)}}pad(&mut piece,&v,w,fl.left_align,b' ')?}
 b'c'|b'C'=>{let v=match get(args,ai,s.conversion)?{Argument::Char(x)=>x,Argument::Signed(x)=>x as u32,Argument::Unsigned(x)=>x as u32,_=>return Err(FormatError::WrongArgumentType{index:ai,conversion:s.conversion})};ai+=1;let c=char::from_u32(v).unwrap_or(char::REPLACEMENT_CHARACTER);let mut b=[0;4];pad(&mut piece,c.encode_utf8(&mut b).as_bytes(),w,fl.left_align,b' ')?}
 b'f'|b'F'|b'e'|b'E'|b'g'|b'G'=>{let v=match get(args,ai,s.conversion)?{Argument::Float(x)=>x,_=>return Err(FormatError::WrongArgumentType{index:ai,conversion:s.conversion})};ai+=1;let n=pr.unwrap_or(6).min(100);let mut b=match s.conversion{b'f'|b'F'=>format!("{:.*}",n,v),b'e'=>format!("{:.*e}",n,v),b'E'=>format!("{:.*E}",n,v),_=>{let mut x=format!("{:.*}",n.max(1).saturating_sub(1),v);if !fl.alternate{while x.contains('.')&&x.ends_with('0'){x.pop();}if x.ends_with('.') {x.pop();}}if s.conversion==b'G'{x.make_ascii_uppercase()}x}}.into_bytes();if !b.starts_with(b"-")&&fl.sign!=0{b.insert(0,fl.sign)}pad(&mut piece,&b,w,fl.left_align,if fl.zero_pad{b'0'}else{b' '})?}
 c=>return Err(FormatError::UnsupportedConversion{offset:s.start,conversion:c})}append(&mut out,&piece,capacity,&mut req)?}
 Ok(FormatResult{truncated:capacity.is_some_and(|n|req>n),bytes:out,required:req})
}
#[cfg(test)]mod tests{use super::*;
#[test]fn parser(){let s=parse_spec(b"%#0-+12.5I64X",0).unwrap();assert_eq!(s.flags,FormatFlags{left_align:true,alternate:true,zero_pad:true,sign:b'+'});assert_eq!(s.width,Some(12));assert_eq!(s.precision,Some(5));assert_eq!(s.length,Length::LongLong)}
#[test]fn dynamic(){let s=parse_spec(b"%*.*ls",0).unwrap();assert!(s.width_from_arg&&s.precision_from_arg&&s.wide_string)}
#[test]fn malformed(){assert_eq!(parse_spec(b"%",0),Err(ParseError::TrailingPercent{offset:0}));assert_eq!(parse_spec(b"%1$d",0),Err(ParseError::UnsupportedPositionalParameter{offset:2}))}
#[test]fn min(){let(d,n,neg)=signed_decimal(i64::MIN);assert_eq!(&d[..n],b"9223372036854775808");assert!(neg)}
#[test]fn integers_strings_pointer(){let r=format_bytes(b"%+06d %#x %.3s %p",&[Argument::Signed(-42),Argument::Unsigned(42),Argument::Ansi(b"hello"),Argument::Pointer(0x1234)],None).unwrap();assert_eq!(r.bytes,b"-00042 0x2a hel 0x0000000000001234")}
#[test]fn dynamic_width(){let r=format_bytes(b"%*.*s",&[Argument::Signed(-6),Argument::Signed(2),Argument::Ansi(b"abcd")],None).unwrap();assert_eq!(r.bytes,b"ab    ")}
#[test]fn truncate(){let r=format_bytes(b"abcdef",&[],Some(3)).unwrap();assert_eq!(r.bytes,b"abc");assert_eq!(r.required,6);assert!(r.truncated)}
#[test]fn float(){let r=format_bytes(b"%+.2f",&[Argument::Float(2.5)],None).unwrap();assert_eq!(r.bytes,b"+2.50")}
}
}
pub use format_core::{Argument, Length, FormatFlags, FormatSpec, ParseError, FormatError, FormatResult, parse_format, format_bytes};

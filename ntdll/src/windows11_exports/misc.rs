//! Windows 11 x64 NTDLL misc export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
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
pub unsafe extern "system" fn ApiSetQuerySchema() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}











































































































#[no_mangle]
pub unsafe extern "system" fn ExpInterlockedPopEntrySListEnd() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ExpInterlockedPopEntrySListFault() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ExpInterlockedPopEntrySListResume() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswalnum(c: u32) -> i32 { (c <= 0x7f && (c as u8).is_ascii_alphanumeric()) as i32 }

#[no_mangle]
pub unsafe extern "system" fn iswalpha(c: u32) -> i32 { (c <= 0x7f && (c as u8).is_ascii_alphabetic()) as i32 }

#[no_mangle]
pub unsafe extern "system" fn iswascii(c: u32) -> i32 { (c <= 0x7f) as i32 }

#[no_mangle]
pub unsafe extern "system" fn iswctype() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswdigit(c: u32) -> i32 { (c <= 0x7f && (c as u8).is_ascii_digit()) as i32 }

#[no_mangle]
pub unsafe extern "system" fn iswgraph(c: u32) -> i32 { (c <= 0x7f && (c as u8).is_ascii_graphic()) as i32 }

#[no_mangle]
pub unsafe extern "system" fn iswlower(c: u32) -> i32 { (c <= 0x7f && (c as u8).is_ascii_lowercase()) as i32 }

#[no_mangle]
pub unsafe extern "system" fn iswprint(c: u32) -> i32 { (c <= 0x7f && ((c as u8).is_ascii_graphic() || c==32)) as i32 }

#[no_mangle]
pub unsafe extern "system" fn iswspace(c: u32) -> i32 { matches!(c, 9..=13 | 32) as i32 }

#[no_mangle]
pub unsafe extern "system" fn iswxdigit(c: u32) -> i32 { (c <= 0x7f && (c as u8).is_ascii_hexdigit()) as i32 }

#[no_mangle]
pub unsafe extern "system" fn KiUserApcDispatcher() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn KiUserCallbackDispatcher() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn KiUserInvertedFunctionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn labs(value: i32) -> i32 { value.wrapping_abs() }

#[no_mangle]
pub unsafe extern "system" fn longjmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn mbstowcs() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn MicrosoftTelemetryAssertTriggeredUM() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NlsAnsiCodePage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NlsMbCodePageTag() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NlsMbOemCodePageTag() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]

#[no_mangle]

#[no_mangle]

#[no_mangle]

#[no_mangle]
pub unsafe extern "system" fn PssNtCaptureSnapshot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtDuplicateSnapshot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtFreeRemoteSnapshot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtFreeSnapshot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtFreeWalkMarker() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtQuerySnapshot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtValidateDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PssNtWalkSnapshot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn SbExecuteProcedure() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn SbSelectProcedure() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ShipAssert() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ShipAssertGetBufferInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ShipAssertMsgA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ShipAssertMsgW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn sscanf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn sscanf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strtok_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn swscanf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn towlower(c: u32) -> u32 { if (65..=90).contains(&c) {c+32} else {c} }

#[no_mangle]
pub unsafe extern "system" fn towupper(c: u32) -> u32 { if (97..=122).contains(&c) {c-32} else {c} }



































































































































#[no_mangle]
pub unsafe extern "system" fn vDbgPrintEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn vDbgPrintExWithPrefix() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn vsprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn vsprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn vswprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcstok_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcstombs() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WerReportSQMEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmAddToAverageDWORD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmAddToStream() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmAddToStreamEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCheckEscalationAddToStreamEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCheckEscalationSetDWORD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCheckEscalationSetDWORD64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCheckEscalationSetString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointDelete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointSetDWORD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointSetDWORD64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointSetStreamEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointSetString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmEndSession() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmEventEnabled() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmEventWrite() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmGetEscalationRuleStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmGetInstrumentationProperty() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmIncrementDWORD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmIsOptedIn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmIsOptedInEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmIsSessionDisabled() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetDWORD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetDWORD64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetEscalationInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetIfMaxDWORD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetIfMinDWORD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmStartSession() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmStartSessionForPartner() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmStartSqmOptinListener() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

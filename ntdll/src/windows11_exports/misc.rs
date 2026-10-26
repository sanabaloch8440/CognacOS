//! Windows 11 x64 NTDLL misc export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_PARAMETER: u32 = 87;
#[repr(C)] #[derive(Clone, Copy)] struct UnicodeString { length: u16, maximum_length: u16, buffer: *const u16 }
unsafe 
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

#[no_mangle]
pub unsafe extern "system" fn ApiSetGetImplementationHost() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
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
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

#[no_mangle]
pub unsafe extern "system" 

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
pub unsafe extern "system" fn PfxFindPrefix() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PfxInitialize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PfxInsertPrefix() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn PfxRemovePrefix() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

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

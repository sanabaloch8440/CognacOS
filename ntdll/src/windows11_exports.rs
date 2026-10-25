//! Windows 11 x64 NTDLL named-export surface.
//!
//! This file is checked in as Rust source. Every name in the pinned export
//! inventory is either implemented in another NTDLL module or represented
//! here by a name-level STATUS_NOT_IMPLEMENTED placeholder. These generic
//! zero-argument stubs provide symbol resolution only; they are not ABI-safe
//! replacements for the real Windows function signatures.
#![allow(non_snake_case, clippy::missing_safety_doc)]

mod loader;
mod heap;
mod memory;
mod registry;
mod process;

#[no_mangle]
pub unsafe extern "system" fn __C_specific_handler() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn __chkstk() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn __isascii() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn __iscsym() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn __iscsymf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn __misaligned_access() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn __toascii() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _atoi64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _errno() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _fltused() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _i64toa() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _i64toa_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _i64tow() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _i64tow_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _itoa() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _itoa_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _itow() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _itow_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _lfind() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _local_unwind() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ltoa() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ltoa_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ltow() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ltow_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _makepath_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _memccpy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _memicmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _setjmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _setjmpex() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _snprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _snprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _snscanf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _snwprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _snwprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _snwscanf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _splitpath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _splitpath_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _strcmpi() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _stricmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _strlwr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _strlwr_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _strnicmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _strnset_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _strset_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _strupr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _strupr_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _swprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ui64toa() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ui64toa_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ui64tow() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ui64tow_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ultoa() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ultoa_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ultow() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _ultow_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _vscprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _vscwprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _vsnprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _vsnprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _vsnwprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _vsnwprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _vswprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcsicmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcslwr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcslwr_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcsnicmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcsnset_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcsset_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcstoi64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcstoui64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcsupr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wcsupr_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wmakepath_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wsplitpath_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wtoi() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wtoi64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn _wtol() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn abs() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcAdjustCompletionListConcurrencyCount() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcFreeCompletionListMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcGetCompletionListLastMessageInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcGetCompletionListMessageAttributes() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcGetMessageFromCompletionList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcGetOutstandingCompletionListMessageCount() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcMaxAllowedMessageLength() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcRegisterCompletionList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcRegisterCompletionListWorkerThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcRundownCompletionList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcUnregisterCompletionList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn AlpcUnregisterCompletionListWorkerThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ApiSetGetImplementationHost() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ApiSetQueryApiSetPresence() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ApiSetQueryApiSetPresenceEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ApiSetQuerySchema() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn atan() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn atan2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn atoi() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn atol() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn bsearch() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn bsearch_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ceil() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn cos() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrAllocateCaptureBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrAllocateMessagePointer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrCaptureMessageBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrCaptureMessageMultiUnicodeStringsInPlace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrCaptureMessageString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrCaptureTimeout() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrClientCallServer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrClientConnectToServer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrFreeCaptureBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrGetProcessId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrIdentifyAlertableThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrSetPriorityClass() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrVerifyRegion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgBreakPoint() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgPrint() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgPrintEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgPrintReturnControlC() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgPrompt() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgQueryDebugFilterState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgSetDebugFilterState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiConvertStateChangeStructure() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiConvertStateChangeStructureEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiGetThreadDebugObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiIssueRemoteBreakin() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiRemoteBreakin() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUiSetThreadDebugObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn DbgUserBreakPoint() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwCheckCoverage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwCreateTraceInstanceId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwDeliverDataBlock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEnumerateProcessRegGuids() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventActivityIdControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventEnabled() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventProviderEnabled() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventRegister() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventSetInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventUnregister() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWrite() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWriteEndScenario() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWriteEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWriteFull() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWriteNoRegistration() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWriteStartScenario() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWriteString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWriteTransfer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwGetTraceEnableFlags() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwGetTraceEnableLevel() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwGetTraceLoggerHandle() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwLogTraceEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwNotificationRegister() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwNotificationUnregister() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwpCreateEtwThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwpGetCpuSpeed() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwProcessPrivateLoggerRequest() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwRegisterSecurityProvider() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwRegisterTraceGuidsA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwRegisterTraceGuidsW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwReplyNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwSendNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwSetMark() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwTraceEventInstance() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwTraceMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwTraceMessageVa() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwUnregisterTraceGuids() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EtwWriteUMSecurityEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EvtIntReportAuthzEventAndSourceAsync() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn EvtIntReportEventAndSourceAsync() -> i32 {
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
pub unsafe extern "system" fn fabs() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn floor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn isalnum() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn isalpha() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iscntrl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn isdigit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn isgraph() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn islower() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn isprint() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ispunct() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn isspace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn isupper() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswalnum() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswalpha() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswascii() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswctype() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswdigit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswgraph() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswlower() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswprint() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswspace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn iswxdigit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn isxdigit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn KiRaiseUserExceptionDispatcher() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn KiUserApcDispatcher() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn KiUserCallbackDispatcher() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn KiUserExceptionDispatcher() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn KiUserInvertedFunctionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn labs() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

















































































#[no_mangle]
pub unsafe extern "system" fn log() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn longjmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn mbstowcs() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn memchr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn memcmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn memcpy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn memcpy_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn memmove() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn memmove_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn memset() -> i32 {
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
pub unsafe extern "system" fn NtAccessCheck() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAccessCheckAndAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAccessCheckByType() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAccessCheckByTypeAndAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAccessCheckByTypeResultList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAccessCheckByTypeResultListAndAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAccessCheckByTypeResultListAndAuditAlarmByHandle() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAcquireCrossVmMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtAddAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAddAtomEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAddBootEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAddDriverEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}









#[no_mangle]
pub unsafe extern "system" fn NtAllocateLocallyUniqueId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAllocateReserveObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAllocateUserPhysicalPages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAllocateUserPhysicalPagesEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAllocateUuids() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtAlpcCancelMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn NtAlpcCreateResourceReserve() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlpcCreateSectionView() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlpcCreateSecurityContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtAlpcDeleteResourceReserve() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlpcDeleteSectionView() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlpcDeleteSecurityContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}






#[no_mangle]
pub unsafe extern "system" fn NtAlpcQueryInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlpcQueryInformationMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAlpcRevokeSecurityContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtAlpcSetInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtApphelpCacheControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtAreMappedFilesTheSame() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtAssociateWaitCompletionPacket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCallbackReturn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCallEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCancelIoFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCancelIoFileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCancelSynchronousIoFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCancelTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCancelTimer2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCancelWaitCompletionPacket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtClearEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtClose() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCloseObjectAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCommitComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCommitEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtCommitTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtCompareObjects() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCompareSigningLevels() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn NtContinue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtContinueEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtConvertBetweenAuxiliaryCounterAndPerformanceCounter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCopyFileChunk() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateCpuPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateCrossVmEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateCrossVmMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateDebugObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateDirectoryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateDirectoryObjectEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateIoRing() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateIRTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}







#[no_mangle]
pub unsafe extern "system" fn NtCreateMailslotFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateNamedPipeFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreatePagingFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreatePartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtCreatePrivateNamespace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtCreateProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateProfileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtCreateResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateSectionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateSemaphore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateSymbolicLinkObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtCreateTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateTimer2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtCreateTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtCreateWaitCompletionPacket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateWnfStateName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtCreateWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtDebugContinue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDelayExecution() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDeleteAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDeleteBootEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDeleteDriverEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDeleteFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtDeleteObjectAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDeletePrivateNamespace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtDeleteWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDeleteWnfStateName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDeviceIoControlFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDirectGraphicsCall() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDisableLastKnownGood() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDisplayString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtdllDefWindowProc_A() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtdllDefWindowProc_W() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtdllDialogWndProc_A() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtdllDialogWndProc_W() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDrawText() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtDuplicateObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtEnableLastKnownGood() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtEnumerateBootEntries() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtEnumerateDriverEntries() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtEnumerateSystemEnvironmentValuesEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtEnumerateTransactionObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtExtendSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFilterBootOption() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtFindAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFlushBuffersFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFlushBuffersFileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFlushInstallUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFlushInstructionCache() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtFlushWriteBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFreeUserPhysicalPages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtFreezeTransactions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtFsControlFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtGetCachedSigningLevel() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtGetCompleteWnfStateSubscription() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtGetDevicePowerState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtGetNlsSectionPtr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtGetNotificationResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtGetWriteWatch() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtInitializeEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtInitializeNlsFiles() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtInitiatePowerAction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtIsSystemResumeAutomatic() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtIsUILanguageComitted() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtLoadEnclaveData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn NtLockFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtMakePermanentObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtMakeTemporaryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtManageHotPatch() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtManagePartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtMapCMFModule() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtMapUserPhysicalPages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtMapUserPhysicalPagesScatter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtMapViewOfSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtMapViewOfSectionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtModifyBootEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtModifyDriverEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtNotifyChangeDirectoryFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtNotifyChangeDirectoryFileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtNotifyChangeSession() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenCpuPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenDirectoryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}







#[no_mangle]
pub unsafe extern "system" fn NtOpenMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenObjectAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenPrivateNamespace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn NtOpenResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenSemaphore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenSession() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenSymbolicLinkObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtOpenTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtOpenTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPlugPlayControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPowerInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPrepareComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPrepareEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPrePrepareComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPrePrepareEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPrivilegeCheck() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPrivilegedServiceAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPrivilegeObjectAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPropagationComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPropagationFailed() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtPssCaptureVaSpaceBulk() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtPulseEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryAttributesFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryAuxiliaryCounterFrequency() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryBootEntryOrder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryBootOptions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryDebugFilterState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryDefaultLocale() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryDefaultUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryDirectoryFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryDirectoryFileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryDirectoryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryDriverEntryOrder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryEaFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryFullAttributesFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationByName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationCpuPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInformationWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryInstallUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryIntervalProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryIoRingCapabilities() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtQueryLicenseValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtQueryMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtQueryPerformanceCounter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtQueryQuotaInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtQuerySecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySecurityPolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySemaphore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySymbolicLinkObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySystemEnvironmentValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySystemEnvironmentValueEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySystemInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySystemInformationEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQuerySystemTime() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryTimerResolution() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtQueryVolumeInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtQueryWnfStateNameInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtRaiseException() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRaiseHardError() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReadFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReadFileScatter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReadOnlyEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReadRequestData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtRecoverEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRecoverResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRecoverTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRegisterProtocolAddressInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtReleaseMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReleaseSemaphore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtReleaseWorkerFactoryWorker() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRemoveIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRemoveIoCompletionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtRenameTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtReplacePartitionUnit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}







#[no_mangle]
pub unsafe extern "system" fn NtResetEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtResetWriteWatch() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtRevertContainerImpersonation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRollbackComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRollbackEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtRollbackTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtRollforwardTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn NtSerializeBoot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetBootEntryOrder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetBootOptions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetCachedSigningLevel() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetCachedSigningLevel2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtSetDebugFilterState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtSetDefaultLocale() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetDefaultUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetDriverEntryOrder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetEaFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetEventBoostPriority() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetEventEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetHighEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetHighWaitLowEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationCpuPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationDebugObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationIoRing() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtSetInformationObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtSetInformationResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationSymbolicLink() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtSetInformationTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetInformationWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetIntervalProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetIoCompletionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetIRTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetLdtEntries() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetLowEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetLowWaitHighEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetQuotaInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetSecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetSystemEnvironmentValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetSystemEnvironmentValueEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetSystemInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetSystemPowerState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetSystemTime() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtSetTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetTimer2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetTimerEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetTimerResolution() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSetUuidSeed() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtSetVolumeInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtShutdownSystem() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtShutdownWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSignalAndWaitForSingleObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSinglePhaseReject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtStartProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtStopProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSubmitIoRing() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtSubscribeWnfStateChange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn NtSystemDebugControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtTerminateEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn NtTestAlert() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtThawTransactions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtTraceControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtTraceEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtTranslateFilePath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}






#[no_mangle]
pub unsafe extern "system" fn NtUnlockFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtUnmapViewOfSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtUnmapViewOfSectionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtUnsubscribeWnfStateChange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtUpdateWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtVdmControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtWaitForDebugEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtWaitForMultipleObjects() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWaitForMultipleObjects32() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWaitForSingleObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWaitForWorkViaWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWaitHighEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWaitLowEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWorkerFactoryWorkerReady() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWriteFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWriteFileGather() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtWriteRequestData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn NtYieldExecution() -> i32 {
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
pub unsafe extern "system" fn pow() -> i32 {
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
pub unsafe extern "system" fn qsort() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn qsort_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAbortRXact() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAbsoluteToSelfRelativeSD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAcquirePrivilege() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAcquireReleaseSRWLockExclusive() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAcquireResourceExclusive() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAcquireResourceShared() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAcquireSRWLockExclusive() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAcquireSRWLockShared() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlActivateActivationContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlActivateActivationContextEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlActivateActivationContextUnsafeFast() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAccessAllowedAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAccessAllowedAceEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAccessAllowedObjectAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAccessDeniedAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAccessDeniedAceEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAccessDeniedObjectAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAccessFilterAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddActionToRXact() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAttributeActionToRXact() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAuditAccessAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAuditAccessAceEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddAuditAccessObjectAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddCompoundAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddFunctionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddGrowableFunctionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddIntegrityLabelToBoundaryDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddMandatoryAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddProcessTrustLabelAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddRefActivationContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlAddResourceAttributeAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddressInSectionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddScopedPolicyIDAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAddSIDToBoundaryDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAdjustPrivilege() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateActivationContextStack() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateAndInitializeSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAllocateAndInitializeSidEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlAllocateWnfSerializationGroup() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAnsiCharToUnicodeChar() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAnsiStringToUnicodeSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAnsiStringToUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAppendPathElement() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlApplicationVerifierStop() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlApplyRXact() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlApplyRXactNoFlush() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAppxIsFileOwnedByTrustedInstaller() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAreAllAccessesGranted() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAreAnyAccessesGranted() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAreBitsClearEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAreLongPathsEnabled() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAssert() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAvlInsertNodeEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlAvlRemoveNode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlBarrier() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlBarrierForDelete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCallEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCallEnclaveReturn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCancelTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCanonicalizeDomainName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCapabilityCheck() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCapabilityCheckForSingleSessionSku() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCaptureContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCaptureContext2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCaptureStackBackTrace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCharToInteger() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckBootStatusIntegrity() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckForOrphanedCriticalSections() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckPortableOperatingSystem() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckSandboxedToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckSystemBootStatusIntegrity() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCheckTokenCapability() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlCleanUpTEBLangLists() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlClearAllBitsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlClearBit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlClearBitEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlClearBitsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlClearThreadWorkOnBehalfTicket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlCloneUserProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlCommitDebugInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlCompareAltitudes() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCompareExchangePointerMapping() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCompareExchangePropertyStore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCompareUnicodeStrings() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCompleteProcessCloning() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCompressBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlComputeImportTableHash() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlConnectToSm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConsoleMultiByteToUnicodeN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConstructCrossVmEventPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConstructCrossVmMutexPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlContractHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConvertDeviceFamilyInfoToString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConvertExclusiveToShared() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConvertHostPerfCounterToPerfCounter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConvertLCIDToString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConvertSharedToExclusive() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConvertSidToUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConvertSRWLockExclusiveToShared() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlConvertToAutoInheritSecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyBitMap() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyLuid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopyLuidAndAttributesArray() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn RtlCopySecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopySid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCopySidAndAttributesArray() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCrc64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateAcl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateActivationContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateAndSetSD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateBootStatusDataFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateBoundaryDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateEnvironmentEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateHashTableEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn RtlCreateProcessReflection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateServiceSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateSystemVolumeInformationFolder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlCreateTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateTimerQueue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUmsCompletionList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUmsThreadContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUnicodeStringFromAsciiz() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUserFiberShadowStack() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlCreateUserSecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUserStack() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateUserThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateVirtualAccountSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCultureNameToLCID() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCustomCPToUnicodeN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlCutoverTimeToSystemTime() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeactivateActivationContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeactivateActivationContextUnsafeFast() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDebugPrintTimes() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDecodePointer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDecodeRemotePointer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDecodeSystemPointer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeCommitDebugInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDecompressBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDecompressBufferEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDecompressFragment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDefaultNpAcl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDelayExecution() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDelete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteBarrier() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteBoundaryDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteCriticalSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteElementGenericTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteElementGenericTableAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteElementGenericTableAvlEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteFunctionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteGrowableFunctionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteNoSplay() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteSecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteTimerQueue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteTimerQueueEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteUmsCompletionList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteUmsThreadContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDequeueUmsCompletionListItems() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlDeregisterWait() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeregisterWaitEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeriveCapabilitySidsFromName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn RtlDisableThreadProfiling() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlDnsHostNameToComputerName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDoesFileExists_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDoesNameContainWildCards() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosApplyFileIsolationRedirection_Ustr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosLongPathNameToNtPathName_U_WithStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosLongPathNameToRelativeNtPathName_U_WithStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosPathNameToNtPathName_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosPathNameToNtPathName_U_WithStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosPathNameToRelativeNtPathName_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosPathNameToRelativeNtPathName_U_WithStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosSearchPath_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDosSearchPath_Ustr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDowncaseUnicodeChar() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDowncaseUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDrainNonVolatileFlush() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDumpResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDuplicateUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnableEarlyCriticalSectionEventCreation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnableThreadProfiling() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnclaveCallDispatch() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnclaveCallDispatchReturn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEncodePointer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEncodeRemotePointer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEncodeSystemPointer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEndEnumerationHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEndStrongEnumerationHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEndWeakEnumerationHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnterUmsSchedulingMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnumerateEntryHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnumerateGenericTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnumerateGenericTableAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnumerateGenericTableLikeADirectory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnumerateGenericTableWithoutSplaying() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnumerateGenericTableWithoutSplayingAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlEqualComputerName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEqualDomainName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEqualLuid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEqualPrefixSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEqualSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEqualWnfChangeStamps() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEraseUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEthernetAddressToStringA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEthernetAddressToStringW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEthernetStringToAddressA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlEthernetStringToAddressW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlExecuteUmsThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlExpandHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlExtendCorrelationVector() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlExtractBitMap() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn RtlFindAceByType() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindActivationContextSectionGuid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindActivationContextSectionString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindCharInUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindClearBitsAndSetEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindClearBitsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindClosestEncodableLength() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindExportedRoutineByName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindSetBitsAndClearEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindSetBitsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindUnicodeSubstring() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFirstEntrySList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFirstFreeAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlsAlloc() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlsAllocEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlsFree() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlsGetValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlsGetValue2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFlsSetValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn RtlFormatCurrentUserKeyPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFormatMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFormatMessageEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeActivationContextStack() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeAnsiString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlFreeNonVolatileToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeOemString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeThreadActivationContextStack() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeUserFiberShadowStack() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeUserStack() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlFreeUTF8String() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGenerate8dot3Name() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetAce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetAcesBufferSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetActiveActivationContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetActiveConsoleId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetAppContainerNamedObjectPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetAppContainerParent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetAppContainerSidType() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCallersAddress() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCompressionWorkSpaceSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetConsoleSessionForegroundProcessId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetControlSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCriticalSectionRecursionCount() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentProcessorNumber() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentProcessorNumberEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentServiceSessionId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentThreadPrimaryGroup() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetCurrentUmsThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetDaclSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetDeviceFamilyInfoEnum() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetElementGenericTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetElementGenericTableAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetExePath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFeatureToggleConfiguration() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFeatureTogglesChangeToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFileMUIPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFrame() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFullPathName_UstrEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetFunctionTableListHead() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetGroupSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetImageFileMachines() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetIntegerAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetInterruptTimePrecise() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetLengthWithoutLastFullDosOrNtPathElement() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetLengthWithoutTrailingPathSeperators() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetLocaleFileMappingAddress() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetLongestNtPathLength() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetMultiTimePrecise() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNativeSystemInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNextEntryHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNextUmsListItem() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNonVolatileToken() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNtGlobalFlags() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNtProductType() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNtSystemRoot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetOwnerSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetParentLocaleName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetPersistedStateLocation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetProcessPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetProductInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetReturnAddressHijackTarget() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSaclSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSearchPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSecurityDescriptorRMControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSessionProperties() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSetBootStatusData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSuiteMask() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSystemBootStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSystemBootStatusEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSystemGlobalData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSystemPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetSystemTimeAndBias() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetThreadErrorMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetThreadLangIdByIndex() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetThreadPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetThreadWorkOnBehalfTicket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetTokenNamedObjectPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetUILanguageInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetUmsCompletionListEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetUnloadEventTrace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetUnloadEventTraceEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetUserPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGrowFunctionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGuardCheckLongJumpTarget() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlGUIDFromString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlHashUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlIdentifierAuthoritySid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIdnToAscii() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIdnToNameprepUnicode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIdnToUnicode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlImageDirectoryEntryToData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlImageNtHeader() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlImageNtHeaderEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlImageRvaToSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlImageRvaToVa() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlImpersonateSelf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlImpersonateSelfEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIncrementCorrelationVector() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitBarrier() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitCodePageTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitEnumerationHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeAtomPackage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeBitMapEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeConditionVariable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeCorrelationVector() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeCriticalSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeCriticalSectionAndSpinCount() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeCriticalSectionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeGenericTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeGenericTableAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeNtUserPfn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeRXact() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeSidEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeSListHead() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeSRWLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlInitNlsTables() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlInitString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitStringEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitStrongEnumerationHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitUTF8String() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitUTF8StringEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitWeakEnumerationHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInsertElementGenericTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInsertElementGenericTableAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInsertElementGenericTableFull() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInsertElementGenericTableFullAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInsertEntryHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInstallFunctionTableCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIntegerToChar() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInterlockedClearBitRun() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInterlockedClearBitRunEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInterlockedFlushSList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInterlockedPopEntrySList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInterlockedPushEntrySList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInterlockedPushListSList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInterlockedPushListSListEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlInterlockedSetBitRun() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlIpv4AddressToStringA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv4AddressToStringExA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv4AddressToStringExW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv4AddressToStringW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv4StringToAddressA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv4StringToAddressExA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv4StringToAddressExW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv4StringToAddressW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv6AddressToStringA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv6AddressToStringExA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv6AddressToStringExW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv6AddressToStringW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv6StringToAddressA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv6StringToAddressExA() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv6StringToAddressExW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIpv6StringToAddressW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsActivationContextActive() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsApiSetImplemented() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsCapabilitySid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsCloudFilesPlaceholder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsCriticalSectionLocked() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsCriticalSectionLockedByThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsCurrentThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsCurrentThreadAttachExempt() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsEcCode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsElevatedRid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsEnclaveFeaturePresent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsFeatureEnabledForEnterprise() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsGenericTableEmpty() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsGenericTableEmptyAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsMultiSessionSku() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsMultiUsersInSessionSku() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsNameInExpression() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsNameInUnUpcasedExpression() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsNameLegalDOS8Dot3() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsNonEmptyDirectoryReparsePointAllowed() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsNormalizedString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsPackageSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsParentOfChildAppContainer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsPartialPlaceholder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsPartialPlaceholderFileHandle() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsPartialPlaceholderFileInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsProcessorFeaturePresent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsStateSeparationEnabled() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsTextUnicode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlIsUntrustedObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsValidLocaleName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsValidProcessTrustLabelSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlKnownExceptionFilter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLCIDToCultureName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLcidToLocaleName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLengthCurrentClearRunBackwardEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLengthCurrentClearRunForwardEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLengthRequiredSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLengthSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLengthSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLengthSidAsUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLoadString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLocaleNameToLcid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockBootStatusData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLockCurrentThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn RtlLockModuleSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLogStackBackTrace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLogUnexpectedCodepath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupElementGenericTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupElementGenericTableAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupElementGenericTableFull() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupElementGenericTableFullAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupEntryHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupFirstMatchingElementGenericTableAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupFunctionEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupFunctionTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlMakeSelfRelativeSD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlMapGenericMask() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlMapSecurityErrorToNtStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlMultiAppendUnicodeStringBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlMultiByteToUnicodeN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlMultiByteToUnicodeSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlNewInstanceSecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNewSecurityGrantedAccess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNewSecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNewSecurityObjectEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNewSecurityObjectWithMultipleInheritance() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNormalizeSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNormalizeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNotifyFeatureToggleUsage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNotifyFeatureUsage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNtdllName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNtPathNameToDosPathName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberGenericTableElements() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberGenericTableElementsAvl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberOfClearBitsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberOfClearBitsInRange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberOfSetBitsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberOfSetBitsInRange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberOfSetBitsUlongPtr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlOemStringToUnicodeSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlOemStringToUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlOemToUnicodeN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlOpenCrossProcessEmulatorWorkConnection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlOpenCurrentUser() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlOsDeploymentState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlOverwriteFeatureConfigurationBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlOwnerAcesPresent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpApplyLengthFunction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpCheckDynamicTimeZoneInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlpConvertAbsoluteToRelativeSecurityAttribute() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpConvertCultureNamesToLCIDs() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpConvertLCIDsToCultureNames() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpConvertRelativeToAbsoluteSecurityAttribute() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlPcToFileHeader() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpEnsureBufferSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpFreezeTimeBias() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpGetDeviceFamilyInfoEnum() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpGetLCIDFromLangInfoNode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpGetNameFromLangInfoNode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpGetSystemDefaultUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpGetUserOrMachineUILanguage4NLS() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlpIsQualifiedLanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpLoadMachineUIByPolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpLoadUserUIByPolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpMergeSecurityAttributeInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn RtlpNotOwnerCriticalSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpNtEnumerateSubKey() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpNtQueryValueKey() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlPopFrame() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpQueryDefaultUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpQueryProcessDebugInformationFromWow64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpQueryProcessDebugInformationRemote() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlPrefixUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpRefreshCachedUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlPrepareForProcessCloning() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlProcessFlsData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlpScpCfgNtdllExports() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpSetInstallLanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpSetPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpSetUserPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpTimeFieldsToTime() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpTimeToTimeFields() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlPublishWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpUnWaitCriticalSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlPushFrame() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpVerifyAndCommitUILanguageSettings() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpWaitForCriticalSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpWow64CtxFromAmd64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpWow64GetContextOnAmd64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlpWow64SetContextOnAmd64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryActivationContextApplicationSettings() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryAllFeatureConfigurations() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryAllInternalFeatureConfigurations() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryAllInternalRuntimeFeatureConfigurations() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryCriticalSectionOwner() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryDepthSList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryElevationFlags() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryFeatureConfiguration() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryFeatureConfigurationChangeStamp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryFeatureUsageNotificationSubscriptions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryImageMitigationPolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryInformationAcl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryInformationActivationContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryInformationActiveActivationContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlQueryInternalFeatureConfiguration() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryModuleInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryPackageClaims() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryPackageIdentity() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryPackageIdentityEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryPointerMapping() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryProcessBackTraceInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlQueryProcessLockInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryProcessPlaceholderCompatibilityMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryPropertyStore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryProtectedPolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn RtlQueryResourcePolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQuerySecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlQueryThreadPlaceholderCompatibilityMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryThreadProfiling() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryTokenHostIdAsUlong64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryUmsThreadInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryValidationRunlevel() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryWnfMetaNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueryWnfStateDataWithExplicitScope() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueueApcWow64Thread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlQueueWorkItem() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRaiseCustomSystemEventTrigger() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRaiseException() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRaiseExceptionForReturnAddressHijack() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRaiseNoncontinuableException() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRaiseStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRandom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRandomEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRbInsertNodeEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRbRemoveNode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRcuAllocate() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRcuFree() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRcuReadLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRcuReadUnlock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRcuSynchronize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlReadThreadProfilingData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRealPredecessor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRealSuccessor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRegisterFeatureConfigurationChangeNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRegisterForWnfMetaNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlRegisterThreadWithCsrss() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRegisterWait() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleaseActivationContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlReleasePath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleasePrivilege() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleaseRelativeName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleaseResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleaseSRWLockExclusive() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReleaseSRWLockShared() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemoteCall() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemoveEntryHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemovePointerMapping() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemovePrivileges() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemovePropertyStore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReplaceSidInSd() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReplaceSystemDirectoryInPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReportException() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReportExceptionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReportSilentProcessExit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlReportSqmEscalation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlResetNtUserPfn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlResetRtlTranslations() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRestoreBootStatusDefaults() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRestoreContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRestoreSystemBootStatusDefaults() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRestoreThreadPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRetrieveNtUserPfn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlRunDecodeUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRunEncodeUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRunOnceBeginInitialize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRunOnceComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRunOnceExecuteOnce() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlRunOnceInitialize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlSelfRelativeToAbsoluteSD() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSelfRelativeToAbsoluteSD2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSendMsgToSm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetAllBitsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetAttributesSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetBit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetBitEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetBitsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetControlSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetCriticalSectionSpinCount() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetCurrentDirectory_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetCurrentTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetDaclSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetDynamicTimeZoneInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetEnvironmentStrings() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetEnvironmentVar() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetFeatureConfigurations() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetGroupSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetImageMitigationPolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetInformationAcl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetIoCompletionCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlSetOwnerSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetPortableOperatingSystem() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetProcessDebugInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetProcessIsCritical() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetProcessPlaceholderCompatibilityMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetProcessPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetProtectedPolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetProxiedProcessId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetSaclSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetSearchPathMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetSecurityDescriptorRMControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetSecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetSecurityObjectEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetSystemBootStatus() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetSystemBootStatusEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetThreadErrorMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetThreadIsCritical() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetThreadPlaceholderCompatibilityMode() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetThreadPoolStartFunc() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetThreadPreferredUILanguages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetThreadPreferredUILanguages2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetThreadSubProcessTag() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetThreadWorkOnBehalfTicket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetUmsThreadInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSidDominates() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSidDominatesForTrust() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSidEqualLevel() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSidHashInitialize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSidHashLookup() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSidIsHigherLevel() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSleepConditionVariableCS() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSleepConditionVariableSRW() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSplay() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlStartRXact() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlStringFromGUID() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlStringFromGUIDEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlStronglyEnumerateEntryHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSubAuthorityCountSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSubAuthoritySid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSubscribeForFeatureUsageNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSubscribeWnfStateChangeNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSubtreePredecessor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSubtreeSuccessor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlSwitchedVVI() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTestAndPublishWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTestBit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTestBitEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTestProtectedAccess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTlsAlloc() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTlsFree() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTlsSetValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTraceDatabaseAdd() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTraceDatabaseCreate() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTraceDatabaseDestroy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTraceDatabaseEnumerate() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTraceDatabaseFind() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTraceDatabaseLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTraceDatabaseUnlock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTraceDatabaseValidate() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTryAcquirePebLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTryAcquireSRWLockExclusive() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTryAcquireSRWLockShared() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTryConvertSRWLockSharedToExclusiveOrRelease() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlTryEnterCriticalSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUdiv128() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUmsThreadYield() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnhandledExceptionFilter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnhandledExceptionFilter2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeStringToAnsiSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeStringToAnsiString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeStringToCountedOemString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeStringToOemSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeStringToOemString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeStringToUTF8String() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeToCustomCPN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeToMultiByteN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeToMultiByteSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeToOemN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnicodeToUTF8N() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUniform() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockBootStatusData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnlockCurrentThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn RtlUnlockModuleSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnregisterFeatureConfigurationChangeNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnsubscribeFromFeatureUsageNotifications() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnsubscribeWnfNotificationWaitForCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnsubscribeWnfNotificationWithCompletionCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnsubscribeWnfStateChangeNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnwind() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUnwindEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpcaseUnicodeChar() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpcaseUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpcaseUnicodeStringToAnsiString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpcaseUnicodeStringToCountedOemString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpcaseUnicodeStringToOemString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpcaseUnicodeToCustomCPN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpcaseUnicodeToMultiByteN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpcaseUnicodeToOemN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpdateClonedCriticalSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpdateClonedSRWLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpdateTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpperChar() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUpperString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUserFiberStart() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlUTF8StringToUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlUTF8ToUnicodeN() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidAcl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidateCorrelationVector() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidateUnicodeString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidateUserCallTarget() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidProcessProtection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidRelativeSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidSecurityDescriptor() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlValidSid() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlVerifyVersionInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlVirtualUnwind() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlVirtualUnwind2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWaitForWnfMetaNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWaitOnAddress() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWakeAddressAll() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWakeAddressAllNoFence() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWakeAddressSingle() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWakeAddressSingleNoFence() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWakeAllConditionVariable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWakeConditionVariable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWalkFrameChain() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWeaklyEnumerateEntryHashTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWerpReportException() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlWow64CallFunction64() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64ChangeProcessState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64ChangeThreadState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64EnableFsRedirection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64EnableFsRedirectionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64GetCpuAreaEnabledFeatures() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64GetCpuAreaInfo() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64GetCurrentCpuArea() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64GetEquivalentMachineCHPE() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64GetThreadSelectorEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64LogMessageInEventLogger() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64PopAllCrossProcessWorkFromWorkList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64PopCrossProcessWorkFromFreeList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64PushCrossProcessWorkOntoFreeList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64PushCrossProcessWorkOntoWorkList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64RequestCrossProcessHeavyFlush() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWow64SuspendProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn RtlxAnsiStringToUnicodeSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlxOemStringToUnicodeSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlXRestore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlXSave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlxUnicodeStringToAnsiSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlxUnicodeStringToOemSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn RtlZombifyActivationContext() -> i32 {
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
pub unsafe extern "system" fn sin() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn sprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn sprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn sqrt() -> i32 {
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
pub unsafe extern "system" fn strcat() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strcat_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strchr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strcmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strcpy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strcpy_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strcspn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strlen() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strncat() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strncat_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strncmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strncpy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strncpy_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strnlen() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strpbrk() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strrchr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strspn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strstr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strtok_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strtol() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn strtoul() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn swprintf() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn swprintf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn swscanf_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn tan() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn tolower() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn toupper() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn towlower() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn towupper() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocAlpcCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocAlpcCompletionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocCleanupGroup() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocJobNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocPool() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocWait() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAllocWork() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAlpcRegisterCompletionList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpAlpcUnregisterCompletionList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackDetectedUnrecoverableError() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackIndependent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackLeaveCriticalSectionOnCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackMayRunLong() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackReleaseMutexOnCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackReleaseSemaphoreOnCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackSendAlpcMessageOnCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackSendPendingAlpcMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackSetEventOnCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCallbackUnloadDllOnCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCancelAsyncIoOperation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCaptureCaller() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpCheckTerminateWorker() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpDbgDumpHeapUsage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpDbgSetLogRoutine() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpDisablePoolCallbackChecks() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpDisassociateCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpIsTimerSet() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpPostWork() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpQueryPoolStackInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleaseAlpcCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleaseCleanupGroup() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleaseCleanupGroupMembers() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleaseIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleaseJobNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleasePool() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleaseTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleaseWait() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpReleaseWork() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetDefaultPoolMaxThreads() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetDefaultPoolStackInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolMaxThreads() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolMaxThreadsSoftLimit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolMinThreads() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolStackInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolThreadBasePriority() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolThreadCpuSets() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetPoolWorkerThreadIdleTimeout() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetTimerEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetWait() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSetWaitEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpSimpleTryPost() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpStartAsyncIoOperation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpTimerOutstandingCallbackCount() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpTrimPools() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpWaitForAlpcCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpWaitForIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpWaitForJobNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpWaitForTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpWaitForWait() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpWaitForWork() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpWorkOnBehalfClearTicket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn TpWorkOnBehalfSetTicket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn vDbgPrintEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn vDbgPrintExWithPrefix() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn VerSetConditionMask() -> i32 {
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
pub unsafe extern "system" fn wcscat() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcscat_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcschr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcscmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcscpy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcscpy_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcscspn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcslen() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsncat() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsncat_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsncmp() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsncpy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsncpy_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsnlen() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcspbrk() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsrchr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsspn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcsstr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcstok_s() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcstol() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcstombs() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn wcstoul() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn WerReportExceptionWorker() -> i32 {
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


#[no_mangle]
pub unsafe extern "system" fn ZwAccessCheck() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAccessCheckAndAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAccessCheckByType() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAccessCheckByTypeAndAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAccessCheckByTypeResultList() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAccessCheckByTypeResultListAndAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAccessCheckByTypeResultListAndAuditAlarmByHandle() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAcquireCrossVmMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwAddAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAddAtomEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAddBootEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAddDriverEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}









#[no_mangle]
pub unsafe extern "system" fn ZwAllocateLocallyUniqueId() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAllocateReserveObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAllocateUserPhysicalPages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAllocateUserPhysicalPagesEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAllocateUuids() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwAlpcCancelMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn ZwAlpcCreateResourceReserve() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcCreateSectionView() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcCreateSecurityContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwAlpcDeleteResourceReserve() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcDeleteSectionView() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcDeleteSecurityContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}






#[no_mangle]
pub unsafe extern "system" fn ZwAlpcQueryInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcQueryInformationMessage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAlpcRevokeSecurityContext() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwAlpcSetInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwApphelpCacheControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwAreMappedFilesTheSame() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwAssociateWaitCompletionPacket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCallbackReturn() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCallEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCancelIoFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCancelIoFileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCancelSynchronousIoFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCancelTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCancelTimer2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCancelWaitCompletionPacket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwClearEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwClose() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCloseObjectAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCommitComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCommitEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwCommitTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwCompareObjects() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCompareSigningLevels() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn ZwContinue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwContinueEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwConvertBetweenAuxiliaryCounterAndPerformanceCounter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCopyFileChunk() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateCpuPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateCrossVmEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateCrossVmMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateDebugObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateDirectoryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateDirectoryObjectEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateIoRing() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateIRTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}







#[no_mangle]
pub unsafe extern "system" fn ZwCreateMailslotFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateNamedPipeFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreatePagingFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreatePartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwCreatePrivateNamespace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwCreateProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateProfileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwCreateResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateSectionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateSemaphore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateSymbolicLinkObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwCreateTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateTimer2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwCreateTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwCreateWaitCompletionPacket() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateWnfStateName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwCreateWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwDebugContinue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDelayExecution() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDeleteAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDeleteBootEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDeleteDriverEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDeleteFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwDeleteObjectAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDeletePrivateNamespace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwDeleteWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDeleteWnfStateName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDeviceIoControlFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDirectGraphicsCall() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDisableLastKnownGood() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDisplayString() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDrawText() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwDuplicateObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwEnableLastKnownGood() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwEnumerateBootEntries() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwEnumerateDriverEntries() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwEnumerateSystemEnvironmentValuesEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwEnumerateTransactionObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwExtendSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFilterBootOption() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwFindAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFlushBuffersFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFlushBuffersFileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFlushInstallUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFlushInstructionCache() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwFlushWriteBuffer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFreeUserPhysicalPages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwFreezeTransactions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwFsControlFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwGetCachedSigningLevel() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwGetCompleteWnfStateSubscription() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwGetDevicePowerState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwGetNlsSectionPtr() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwGetNotificationResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwGetWriteWatch() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwInitializeEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwInitializeNlsFiles() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwInitiatePowerAction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwIsSystemResumeAutomatic() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwIsUILanguageComitted() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwLoadDriver() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwLoadEnclaveData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn ZwLockFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwMakePermanentObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwMakeTemporaryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwManageHotPatch() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwManagePartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwMapCMFModule() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwMapUserPhysicalPages() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwMapUserPhysicalPagesScatter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwMapViewOfSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwMapViewOfSectionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwModifyBootEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwModifyDriverEntry() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwNotifyChangeDirectoryFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwNotifyChangeDirectoryFileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwNotifyChangeSession() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenCpuPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenDirectoryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}







#[no_mangle]
pub unsafe extern "system" fn ZwOpenMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenObjectAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenPrivateNamespace() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn ZwOpenResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenSemaphore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenSession() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenSymbolicLinkObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwOpenTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwOpenTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPlugPlayControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPowerInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPrepareComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPrepareEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPrePrepareComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPrePrepareEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPrivilegeCheck() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPrivilegedServiceAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPrivilegeObjectAuditAlarm() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPropagationComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPropagationFailed() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwPssCaptureVaSpaceBulk() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwPulseEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryAttributesFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryAuxiliaryCounterFrequency() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryBootEntryOrder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryBootOptions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryDebugFilterState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryDefaultLocale() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryDefaultUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryDirectoryFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryDirectoryFileEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryDirectoryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryDriverEntryOrder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryEaFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryFullAttributesFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationAtom() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationByName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationCpuPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInformationWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryInstallUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryIntervalProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryIoRingCapabilities() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwQueryLicenseValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwQueryMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwQueryPerformanceCounter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwQueryQuotaInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwQuerySecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySecurityPolicy() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySemaphore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySymbolicLinkObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySystemEnvironmentValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySystemEnvironmentValueEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySystemInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySystemInformationEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQuerySystemTime() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryTimerResolution() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwQueryVolumeInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwQueryWnfStateNameInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwRaiseException() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRaiseHardError() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReadFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReadFileScatter() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReadOnlyEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReadRequestData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwRecoverEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRecoverResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRecoverTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRegisterProtocolAddressInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwReleaseMutant() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReleaseSemaphore() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwReleaseWorkerFactoryWorker() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRemoveIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRemoveIoCompletionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwRenameTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwReplacePartitionUnit() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}







#[no_mangle]
pub unsafe extern "system" fn ZwResetEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwResetWriteWatch() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwRevertContainerImpersonation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRollbackComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRollbackEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwRollbackTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwRollforwardTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}





#[no_mangle]
pub unsafe extern "system" fn ZwSerializeBoot() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetBootEntryOrder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetBootOptions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetCachedSigningLevel() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetCachedSigningLevel2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwSetDebugFilterState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwSetDefaultLocale() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetDefaultUILanguage() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetDriverEntryOrder() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetEaFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetEventBoostPriority() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetEventEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetHighEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetHighWaitLowEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationCpuPartition() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationDebugObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationEnlistment() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationIoRing() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationResourceManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationSymbolicLink() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationTransaction() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationTransactionManager() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationVirtualMemory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetInformationWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetIntervalProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetIoCompletion() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetIoCompletionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetIRTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetLdtEntries() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetLowEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetLowWaitHighEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetQuotaInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetSecurityObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetSystemEnvironmentValue() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetSystemEnvironmentValueEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetSystemInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetSystemPowerState() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetSystemTime() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwSetTimer() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetTimer2() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetTimerEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetTimerResolution() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSetUuidSeed() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwSetVolumeInformationFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwShutdownSystem() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwShutdownWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSignalAndWaitForSingleObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSinglePhaseReject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwStartProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwStopProfile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSubmitIoRing() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwSubscribeWnfStateChange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn ZwSystemDebugControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwTerminateEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwTestAlert() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwThawTransactions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwTraceControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwTraceEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwTranslateFilePath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwUnloadDriver() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}




#[no_mangle]
pub unsafe extern "system" fn ZwUnlockFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwUnmapViewOfSection() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwUnmapViewOfSectionEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwUnsubscribeWnfStateChange() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwUpdateWnfStateData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwVdmControl() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwWaitForDebugEvent() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwWaitForMultipleObjects() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWaitForMultipleObjects32() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWaitForSingleObject() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWaitForWorkViaWorkerFactory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWaitHighEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWaitLowEventPair() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWorkerFactoryWorkerReady() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWriteFile() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWriteFileGather() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn ZwWriteRequestData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}


#[no_mangle]
pub unsafe extern "system" fn ZwYieldExecution() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

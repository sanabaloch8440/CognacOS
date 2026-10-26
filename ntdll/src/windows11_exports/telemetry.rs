//! Windows SQM/telemetry exports with ABI-typed fallback behavior.
//!
//! The public signatures below are based on Wine's ntdll/misc.c declarations.
//! Telemetry is optional in this runtime; no provider/session is fabricated.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;

type HANDLE = *mut c_void;
type DWORD = u32;
type BOOL = i32;
type NTSTATUS = i32;

const STATUS_SUCCESS: NTSTATUS = 0;
const INVALID_HANDLE_VALUE: HANDLE = usize::MAX as HANDLE;

#[repr(C)]
pub struct GUID {
    pub Data1: u32,
    pub Data2: u16,
    pub Data3: u16,
    pub Data4: [u8; 8],
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmEndSession(_session: HANDLE) -> NTSTATUS {
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmIncrementDWORD(_unknown1: DWORD, _unknown2: DWORD, _unknown3: DWORD) {
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmIsOptedIn() -> BOOL {
    0
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmStartSession(
    _session_guid: *mut GUID,
    _session_id: DWORD,
    _unknown: DWORD,
) -> HANDLE {
    INVALID_HANDLE_VALUE
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetDWORD(
    _session: HANDLE,
    _datapoint_id: DWORD,
    _datapoint_value: DWORD,
) {
}

#[no_mangle]
pub unsafe extern "system" fn WinSqmSetIfMaxDWORD(_unknown1: DWORD, _unknown2: DWORD, _unknown3: DWORD) {
}


// Remaining SQM setters are deliberately typed and inert: no telemetry provider
// exists in this process. Their return type follows the export's API family.
#[no_mangle]
pub unsafe extern "system" fn WinSqmAddToAverageDWORD(_session: HANDLE, _id: DWORD, _value: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmAddToStream(_session: HANDLE, _id: DWORD, _data: *const c_void, _size: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmAddToStreamEx(_session: HANDLE, _id: DWORD, _data: *const c_void, _size: DWORD, _flags: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCheckEscalationAddToStreamEx(_session: HANDLE, _id: DWORD, _data: *const c_void, _size: DWORD, _flags: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCheckEscalationSetDWORD(_session: HANDLE, _id: DWORD, _value: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCheckEscalationSetDWORD64(_session: HANDLE, _id: DWORD, _value: u64) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCheckEscalationSetString(_session: HANDLE, _id: DWORD, _value: *const u16) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointDelete(_session: HANDLE, _id: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointSetDWORD(_session: HANDLE, _id: DWORD, _value: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointSetDWORD64(_session: HANDLE, _id: DWORD, _value: u64) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointSetStreamEx(_session: HANDLE, _id: DWORD, _data: *const c_void, _size: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmCommonDatapointSetString(_session: HANDLE, _id: DWORD, _value: *const u16) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmEventEnabled(_session: HANDLE, _event_id: DWORD) -> BOOL { 0 }
#[no_mangle]
pub unsafe extern "system" fn WinSqmEventWrite(_session: HANDLE, _event_id: DWORD, _data: *const c_void) -> NTSTATUS { STATUS_SUCCESS }
#[no_mangle]
pub unsafe extern "system" fn WinSqmGetEscalationRuleStatus(_session: HANDLE, _rule_id: DWORD) -> NTSTATUS { 0xC000_0002u32 as i32 }
#[no_mangle]
pub unsafe extern "system" fn WinSqmGetInstrumentationProperty(_property: DWORD, _buffer: *mut c_void, _buffer_length: *mut DWORD) -> NTSTATUS { 0xC000_0002u32 as i32 }
#[no_mangle]
pub unsafe extern "system" fn WinSqmIsOptedInEx(_flags: DWORD) -> BOOL { 0 }
#[no_mangle]
pub unsafe extern "system" fn WinSqmIsSessionDisabled(_session: HANDLE) -> BOOL { 1 }
#[no_mangle]
pub unsafe extern "system" fn WinSqmSetDWORD64(_session: HANDLE, _id: DWORD, _value: u64) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmSetEscalationInfo(_session: HANDLE, _information: *const c_void) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmSetIfMinDWORD(_id: DWORD, _current: DWORD, _candidate: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmSetString(_session: HANDLE, _id: DWORD, _value: *const u16) {}
#[no_mangle]
pub unsafe extern "system" fn WinSqmStartSessionForPartner(_session_guid: *mut GUID, _session_id: DWORD, _unknown: DWORD, _partner_guid: *const GUID) -> HANDLE { INVALID_HANDLE_VALUE }
#[no_mangle]
pub unsafe extern "system" fn WinSqmStartSqmOptinListener(_callback: *mut c_void) -> NTSTATUS { 0xC000_0002u32 as i32 }

/// Microsoft telemetry assertion notification; intentionally does not abort the
/// process when optional telemetry is not configured.
#[no_mangle]
pub unsafe extern "system" fn MicrosoftTelemetryAssertTriggeredUM() {}

/// ShipAssert entry points are optional diagnostics. These ABI declarations use
/// pointer-sized parameters where the underlying private structures are opaque.
#[no_mangle]
pub unsafe extern "system" fn ShipAssert(_assert_info: *const c_void, _flags: DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn ShipAssertGetBufferInfo(_buffer: *mut *mut c_void, _length: *mut DWORD) {}
#[no_mangle]
pub unsafe extern "system" fn ShipAssertMsgA(_flags: DWORD, _message: *const i8, _source: *const i8) {}
#[no_mangle]
pub unsafe extern "system" fn ShipAssertMsgW(_flags: DWORD, _message: *const u16, _source: *const u16) {}

/// WER SQM reporting is disabled; preserve a four-argument status-return ABI.
#[no_mangle]
pub unsafe extern "system" fn WerReportSQMEvent(_report: HANDLE, _event_name: *const u16, _event_type: DWORD, _event_data: *const c_void) -> i32 { 0 }

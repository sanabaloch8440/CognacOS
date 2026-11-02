//! Stable C ABI between ataport.sys and controller miniports such as pciide.sys.
//! This is a project-owned WDM-facing contract, not a claim of Microsoft binary ABI parity.
#![allow(non_camel_case_types)]

use core::ffi::c_void;

pub const ATA_PORT_INTERFACE_VERSION: u32 = 1;
pub const ATA_PORT_MAX_SG_ELEMENTS: u32 = 128;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtaPortOperation { Read = 0, Write = 1, Flush = 2, Identify = 3, Packet = 4 }

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AtaPortSgElement {
    pub physical_address: u64,
    pub length: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AtaPortRequest {
    pub size: u32,
    pub operation: AtaPortOperation,
    pub request_id: u64,
    pub lba: u64,
    pub sector_count: u32,
    pub flags: u32,
    pub sg_list: *const AtaPortSgElement,
    pub sg_count: u32,
    pub timeout_ms: u32,
    pub packet_cdb: *const u8,
    pub packet_cdb_length: u8,
    pub reserved: [u8; 7],
}

pub type AtaPortCompletion = unsafe extern "win64" fn(
    request_context: *mut c_void,
    request_id: u64,
    ntstatus: i32,
    bytes_transferred: u32,
);

pub type AtaMiniportInitialize = unsafe extern "win64" fn(
    miniport_context: *mut c_void,
    translated_resources: *const c_void,
) -> i32;
pub type AtaMiniportStart = unsafe extern "win64" fn(miniport_context: *mut c_void) -> i32;
pub type AtaMiniportSubmit = unsafe extern "win64" fn(
    miniport_context: *mut c_void,
    request: *const AtaPortRequest,
    request_context: *mut c_void,
    completion: AtaPortCompletion,
) -> i32;
pub type AtaMiniportReset = unsafe extern "win64" fn(miniport_context: *mut c_void) -> i32;
pub type AtaMiniportStop = unsafe extern "win64" fn(miniport_context: *mut c_void) -> i32;
pub type AtaMiniportUnload = unsafe extern "win64" fn(miniport_context: *mut c_void);

/// Controller miniports register this table with ataport during AddDevice/start.
/// The port driver owns IRP queueing, cancellation, request lifetime, DMA mapping,
/// timeout/reset policy and PnP teardown; the miniport owns controller registers.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AtaPortMiniportInterface {
    pub size: u32,
    pub version: u32,
    pub miniport_context: *mut c_void,
    pub initialize: Option<AtaMiniportInitialize>,
    pub start: Option<AtaMiniportStart>,
    pub submit: Option<AtaMiniportSubmit>,
    pub reset: Option<AtaMiniportReset>,
    pub stop: Option<AtaMiniportStop>,
    pub unload: Option<AtaMiniportUnload>,
}

/// Registration callback exported by ataport to a controller miniport.
pub type AtaPortRegisterMiniport = unsafe extern "win64" fn(
    interface: *const AtaPortMiniportInterface,
) -> i32;

/// Versioned port services exposed to miniports. DMA and interrupt services are
/// supplied by ntoskrnl/HAL, never reimplemented by pciide.sys.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AtaPortServices {
    pub size: u32,
    pub version: u32,
    pub register_miniport: Option<AtaPortRegisterMiniport>,
    pub allocate_common_buffer: Option<unsafe extern "win64" fn(*mut c_void, u32, *mut u64) -> *mut c_void>,
    pub free_common_buffer: Option<unsafe extern "win64" fn(*mut c_void, *mut c_void, u64, u32)>,
    pub connect_interrupt: Option<unsafe extern "win64" fn(*mut c_void) -> i32>,
    pub disconnect_interrupt: Option<unsafe extern "win64" fn(*mut c_void)>,
}

//! Stable C ABI between ataport.sys and controller miniports such as pciide.sys.
//! This is a project-owned WDM-facing contract, not a claim of Microsoft binary ABI parity.
#![allow(non_camel_case_types)]

use core::ffi::c_void;

pub const ATA_PORT_INTERFACE_VERSION: u32 = 1;
pub const ATA_PORT_MAX_SG_ELEMENTS: u32 = 128;
pub const ATA_PORT_MAX_PIO_SECTORS: u32 = 256;
/// Low two request flag bits select the channel/device slot (0..=3).
pub const ATA_PORT_REQUEST_DEVICE_INDEX_MASK: u32 = 0x3;

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
    /// Request flags; low two bits select the channel/device slot.
    pub flags: u32,
    /// Kernel-owned, nonpaged system buffer for PIO transfers. The port driver
    /// accepts this only for buffered IRPs; miniports must never dereference a
    /// user virtual address. DMA miniports use the SG list instead.
    pub system_buffer: *mut u8,
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
/// Synchronous PIO submission contract. The port invokes this only from a
/// PASSIVE_LEVEL request path; the miniport must finish the bounded transfer
/// before returning and report the exact byte count. Asynchronous/DMA queues
/// require a later interface version with cancellation and rundown services.
pub type AtaMiniportSubmit = unsafe extern "win64" fn(
    miniport_context: *mut c_void,
    request: *const AtaPortRequest,
    bytes_transferred: *mut u32,
) -> i32;
pub type AtaMiniportReset = unsafe extern "win64" fn(miniport_context: *mut c_void) -> i32;
pub type AtaMiniportStop = unsafe extern "win64" fn(miniport_context: *mut c_void) -> i32;
pub type AtaMiniportUnload = unsafe extern "win64" fn(miniport_context: *mut c_void);

/// Controller miniports register this table with ataport during AddDevice/start.
/// Version 1 intentionally supports bounded synchronous PIO only. The port owns
/// IRP validation/completion and request lifetime; the miniport owns registers.
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
pub type AtaPortUnregisterMiniport = unsafe extern "win64" fn(miniport_context: *mut c_void) -> i32;

/// Versioned port services exposed to miniports. DMA and interrupt services are
/// supplied by ntoskrnl/HAL, never reimplemented by pciide.sys.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AtaPortServices {
    pub size: u32,
    pub version: u32,
    pub register_miniport: Option<AtaPortRegisterMiniport>,
    pub unregister_miniport: Option<AtaPortUnregisterMiniport>,
    pub allocate_common_buffer: Option<unsafe extern "win64" fn(*mut c_void, u32, *mut u64) -> *mut c_void>,
    pub free_common_buffer: Option<unsafe extern "win64" fn(*mut c_void, *mut c_void, u64, u32)>,
    pub connect_interrupt: Option<unsafe extern "win64" fn(*mut c_void) -> i32>,
    pub disconnect_interrupt: Option<unsafe extern "win64" fn(*mut c_void)>,
}

/// Validate the transport-independent part of an ATA request before it reaches
/// a controller miniport. Buffer mapping and device-capacity checks remain
/// port/controller responsibilities because they require the live device stack.
pub fn validate_request(request: &AtaPortRequest) -> Result<u32, i32> {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_INVALID_DEVICE_REQUEST: i32 = 0xC000_0010u32 as i32;
    if request.size as usize != core::mem::size_of::<AtaPortRequest>()
        || request.sg_count > ATA_PORT_MAX_SG_ELEMENTS
    {
        return Err(STATUS_INVALID_PARAMETER);
    }
    match request.operation {
        AtaPortOperation::Read | AtaPortOperation::Write => {
            if request.sector_count == 0 || request.sector_count > ATA_PORT_MAX_PIO_SECTORS || request.system_buffer.is_null() {
                return Err(STATUS_INVALID_PARAMETER);
            }
            let bytes = request.sector_count.checked_mul(512)
                .ok_or(STATUS_INVALID_PARAMETER)?;
            if request.sg_count != 0 && request.sg_list.is_null() {
                return Err(STATUS_INVALID_PARAMETER);
            }
            if request.sg_count == 0 && !request.sg_list.is_null() {
                return Err(STATUS_INVALID_PARAMETER);
            }
            Ok(bytes)
        }
        AtaPortOperation::Flush => {
            if request.sector_count != 0 || request.lba != 0
                || !request.system_buffer.is_null() || request.sg_count != 0
                || !request.sg_list.is_null()
            {
                return Err(STATUS_INVALID_PARAMETER);
            }
            Ok(0)
        }
        AtaPortOperation::Identify => {
            if request.sector_count != 1 || request.system_buffer.is_null()
                || request.sg_count != 0 || !request.sg_list.is_null()
            {
                return Err(STATUS_INVALID_PARAMETER);
            }
            Ok(512)
        }
        AtaPortOperation::Packet => Err(STATUS_INVALID_DEVICE_REQUEST),
    }
}

/// Bounded request ownership table for ataport. Queue slots are reserved before
/// a miniport sees a request and are released only by completion or reset.
/// Callers serialize access with the port's interrupt-safe queue lock.
pub const ATA_PORT_MAX_OUTSTANDING: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtaRequestState { Free, Queued, Submitted, Completing }
#[derive(Clone, Copy)]
pub struct AtaRequestSlot {
    pub state: AtaRequestState,
    pub request_id: u64,
    pub request_context: *mut c_void,
    pub expected_bytes: u32,
}
impl AtaRequestSlot {
    pub const EMPTY: Self = Self { state: AtaRequestState::Free, request_id: 0, request_context: core::ptr::null_mut(), expected_bytes: 0 };
}
pub struct AtaRequestTable { slots: [AtaRequestSlot; ATA_PORT_MAX_OUTSTANDING], next_id: u64 }
impl AtaRequestTable {
    pub const fn new() -> Self { Self { slots: [AtaRequestSlot::EMPTY; ATA_PORT_MAX_OUTSTANDING], next_id: 1 } }
    pub fn reserve(&mut self, context: *mut c_void, expected_bytes: u32) -> Result<u64, i32> {
        let slot = self.slots.iter_mut().find(|s| s.state == AtaRequestState::Free).ok_or(0xC000_009Au32 as i32)?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        *slot = AtaRequestSlot { state: AtaRequestState::Queued, request_id: id, request_context: context, expected_bytes };
        Ok(id)
    }
    pub fn mark_submitted(&mut self, id: u64) -> Result<(), i32> {
        let s = self.find_mut(id)?;
        if s.state != AtaRequestState::Queued { return Err(0xC000_0184u32 as i32); }
        s.state = AtaRequestState::Submitted; Ok(())
    }
    pub fn begin_completion(&mut self, id: u64, bytes: u32) -> Result<*mut c_void, i32> {
        let s = self.find_mut(id)?;
        if s.state != AtaRequestState::Submitted || bytes > s.expected_bytes {
            return Err(0xC000_000Du32 as i32);
        }
        s.state = AtaRequestState::Completing;
        Ok(s.request_context)
    }
    /// Release a request rejected by the miniport before it accepted ownership.
    pub fn abort(&mut self, id: u64) -> Result<(), i32> {
        let s = self.find_mut(id)?;
        if s.state != AtaRequestState::Queued && s.state != AtaRequestState::Submitted {
            return Err(0xC000_0184u32 as i32);
        }
        *s = AtaRequestSlot::EMPTY;
        Ok(())
    }
    pub fn release(&mut self, id: u64) -> Result<(), i32> {
        let s = self.find_mut(id)?;
        if s.state != AtaRequestState::Completing { return Err(0xC000_0184u32 as i32); }
        *s = AtaRequestSlot::EMPTY; Ok(())
    }
    /// Reset invalidates all outstanding transport requests. Caller must first
    /// stop hardware DMA and detach interrupt delivery before invoking this.
    pub fn reset_quiesced(&mut self) -> usize {
        let mut cancelled = 0;
        for slot in &mut self.slots { if slot.state != AtaRequestState::Free { cancelled += 1; *slot = AtaRequestSlot::EMPTY; } }
        cancelled
    }
    fn find_mut(&mut self, id: u64) -> Result<&mut AtaRequestSlot, i32> {
        self.slots.iter_mut().find(|s| s.state != AtaRequestState::Free && s.request_id == id).ok_or(0xC000_0008u32 as i32)
    }
    pub fn outstanding(&self) -> usize { self.slots.iter().filter(|s| s.state != AtaRequestState::Free).count() }
}

#[cfg(test)]
mod request_table_tests {
    use super::*;
    #[test]
    fn request_lifetime_is_ordered_and_bounded() {
        let mut q = AtaRequestTable::new();
        let id = q.reserve(core::ptr::null_mut(), 4096).unwrap();
        q.mark_submitted(id).unwrap();
        assert!(q.begin_completion(id, 4097).is_err());
        assert!(q.begin_completion(id, 512).is_ok());
        q.release(id).unwrap();
        assert_eq!(q.outstanding(), 0);
        assert!(q.mark_submitted(id).is_err());
    }
    #[test]
    fn queue_exhaustion_is_reported_without_overwriting_live_requests() {
        let mut q = AtaRequestTable::new();
        for _ in 0..ATA_PORT_MAX_OUTSTANDING { q.reserve(core::ptr::null_mut(), 512).unwrap(); }
        assert_eq!(q.reserve(core::ptr::null_mut(), 512), Err(0xC000_009Au32 as i32));
        assert_eq!(q.outstanding(), ATA_PORT_MAX_OUTSTANDING);
        assert_eq!(q.reset_quiesced(), ATA_PORT_MAX_OUTSTANDING);
        assert_eq!(q.outstanding(), 0);
    }
}

#[cfg(test)]
mod request_validation_tests {
    use super::*;

    fn base(operation: AtaPortOperation) -> AtaPortRequest {
        AtaPortRequest {
            size: core::mem::size_of::<AtaPortRequest>() as u32,
            operation, request_id: 1, lba: 0, sector_count: 1, flags: 0,
            system_buffer: 1usize as *mut u8,
            sg_list: core::ptr::null(), sg_count: 0, timeout_ms: 1000,
            packet_cdb: core::ptr::null(), packet_cdb_length: 0, reserved: [0; 7],
        }
    }

    #[test]
    fn buffered_pio_read_write_require_a_system_buffer_and_bounded_count() {
        let read = base(AtaPortOperation::Read);
        assert_eq!(validate_request(&read), Ok(512));
        let mut write = base(AtaPortOperation::Write);
        write.sector_count = 256;
        assert_eq!(validate_request(&write), Ok(256 * 512));
        write.sector_count = 257;
        assert!(validate_request(&write).is_err());
        write.sector_count = u32::MAX;
        assert!(validate_request(&write).is_err());
        write.sector_count = 1;
        write.system_buffer = core::ptr::null_mut();
        assert!(validate_request(&write).is_err());
    }

    #[test]
    fn flush_request_has_no_data_payload() {
        let mut flush = base(AtaPortOperation::Flush);
        flush.sector_count = 0;
        flush.system_buffer = core::ptr::null_mut();
        assert_eq!(validate_request(&flush), Ok(0));
        flush.lba = 1;
        assert!(validate_request(&flush).is_err());
    }
}

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
        || request.flags & !ATA_PORT_REQUEST_DEVICE_INDEX_MASK != 0
        || request.timeout_ms == 0
    {
        return Err(STATUS_INVALID_PARAMETER);
    }
    match request.operation {
        AtaPortOperation::Read | AtaPortOperation::Write => {
            if request.sector_count == 0 || request.sector_count > ATA_PORT_MAX_PIO_SECTORS
                || request.system_buffer.is_null() || request.sg_count != 0
                || !request.sg_list.is_null() || !request.packet_cdb.is_null()
                || request.packet_cdb_length != 0 {
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
        write.system_buffer = 1usize as *mut u8;
        write.flags = 0x4;
        assert!(validate_request(&write).is_err());
        write.flags = 0;
        write.sg_count = 1;
        write.sg_list = 1usize as *const AtaPortSgElement;
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


/// PASSIVE_LEVEL request scheduling metadata for the port driver. This is a
/// bounded, allocation-free queue; the caller must protect it with the port's
/// queue lock and invoke dequeue/dispatch only from a PASSIVE_LEVEL work item.
/// It deliberately does not execute miniport callbacks or provide a scheduler.
pub const ATA_PASSIVE_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassiveRequestState { Free, Queued, Running, CancelRequested, Completed, Cancelled, TimedOut }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassiveRequest {
    pub request_id: u64,
    pub enqueue_sequence: u64,
    pub deadline_ms: u64,
    pub state: PassiveRequestState,
}
impl PassiveRequest {
    pub const EMPTY: Self = Self {
        request_id: 0, enqueue_sequence: 0, deadline_ms: 0,
        state: PassiveRequestState::Free,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassiveQueueError { InvalidRequest, Full, Duplicate, NotFound, InvalidState, Overflow }

pub struct PassiveRequestQueue {
    entries: [PassiveRequest; ATA_PASSIVE_QUEUE_CAPACITY],
    next_sequence: u64,
}
impl PassiveRequestQueue {
    pub const fn new() -> Self {
        Self { entries: [PassiveRequest::EMPTY; ATA_PASSIVE_QUEUE_CAPACITY], next_sequence: 1 }
    }

    /// Enqueue a request with an absolute monotonic deadline. now_ms must
    /// come from the kernel monotonic clock, not wall-clock time.
    pub fn enqueue(&mut self, request_id: u64, now_ms: u64, timeout_ms: u32) -> Result<(), PassiveQueueError> {
        if request_id == 0 || timeout_ms == 0 { return Err(PassiveQueueError::InvalidRequest); }
        if self.entries.iter().any(|entry| entry.state != PassiveRequestState::Free && entry.request_id == request_id) {
            return Err(PassiveQueueError::Duplicate);
        }
        let deadline_ms = now_ms.checked_add(timeout_ms as u64).ok_or(PassiveQueueError::Overflow)?;
        let slot = self.entries.iter_mut().find(|entry| entry.state == PassiveRequestState::Free
            || matches!(entry.state, PassiveRequestState::Completed | PassiveRequestState::Cancelled | PassiveRequestState::TimedOut))
            .ok_or(PassiveQueueError::Full)?;
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1).max(1);
        *slot = PassiveRequest { request_id, enqueue_sequence: sequence, deadline_ms, state: PassiveRequestState::Queued };
        Ok(())
    }

    /// Select the oldest queued request. Expired queued requests are marked
    /// TimedOut and are never returned for miniport execution.
    pub fn dequeue(&mut self, now_ms: u64) -> Option<u64> {
        for entry in &mut self.entries {
            if entry.state == PassiveRequestState::Queued && now_ms >= entry.deadline_ms {
                entry.state = PassiveRequestState::TimedOut;
            }
        }
        let index = self.entries.iter().enumerate()
            .filter(|(_, entry)| entry.state == PassiveRequestState::Queued)
            .min_by_key(|(_, entry)| entry.enqueue_sequence)
            .map(|(index, _)| index)?;
        self.entries[index].state = PassiveRequestState::Running;
        Some(self.entries[index].request_id)
    }

    /// Cancellation of queued work is immediate. Running work is marked for
    /// cancellation; the worker must observe the flag at a safe checkpoint.
    pub fn cancel(&mut self, request_id: u64) -> Result<PassiveRequestState, PassiveQueueError> {
        let entry = self.entries.iter_mut().find(|entry| entry.state != PassiveRequestState::Free
            && entry.request_id == request_id).ok_or(PassiveQueueError::NotFound)?;
        match entry.state {
            PassiveRequestState::Queued => entry.state = PassiveRequestState::Cancelled,
            PassiveRequestState::Running => entry.state = PassiveRequestState::CancelRequested,
            PassiveRequestState::CancelRequested => {},
            _ => return Err(PassiveQueueError::InvalidState),
        }
        Ok(entry.state)
    }

    /// Complete a running request only if it was not cancelled or timed out.
    pub fn complete(&mut self, request_id: u64, now_ms: u64) -> Result<PassiveRequestState, PassiveQueueError> {
        let entry = self.entries.iter_mut().find(|entry| entry.state != PassiveRequestState::Free
            && entry.request_id == request_id).ok_or(PassiveQueueError::NotFound)?;
        match entry.state {
            PassiveRequestState::Running if now_ms >= entry.deadline_ms => {
                entry.state = PassiveRequestState::TimedOut;
            }
            PassiveRequestState::Running => entry.state = PassiveRequestState::Completed,
            PassiveRequestState::CancelRequested => entry.state = PassiveRequestState::Cancelled,
            _ => return Err(PassiveQueueError::InvalidState),
        }
        Ok(entry.state)
    }

    pub fn state(&self, request_id: u64) -> Option<PassiveRequestState> {
        self.entries.iter().find(|entry| entry.state != PassiveRequestState::Free && entry.request_id == request_id)
            .map(|entry| entry.state)
    }

    pub fn queued(&self) -> usize {
        self.entries.iter().filter(|entry| entry.state == PassiveRequestState::Queued).count()
    }
}

#[cfg(test)]
mod passive_queue_tests {
    use super::*;

    #[test]
    fn passive_queue_is_fifo_and_timeout_is_enforced_before_dispatch() {
        let mut queue = PassiveRequestQueue::new();
        queue.enqueue(11, 100, 50).unwrap();
        queue.enqueue(12, 100, 500).unwrap();
        assert_eq!(queue.dequeue(150), Some(12));
        assert_eq!(queue.state(11), Some(PassiveRequestState::TimedOut));
        assert_eq!(queue.complete(12, 151), Ok(PassiveRequestState::Completed));
    }

    #[test]
    fn cancellation_distinguishes_queued_and_running_requests() {
        let mut queue = PassiveRequestQueue::new();
        queue.enqueue(21, 0, 100).unwrap();
        assert_eq!(queue.cancel(21), Ok(PassiveRequestState::Cancelled));
        queue.enqueue(22, 0, 100).unwrap();
        assert_eq!(queue.dequeue(1), Some(22));
        assert_eq!(queue.cancel(22), Ok(PassiveRequestState::CancelRequested));
        assert_eq!(queue.complete(22, 2), Ok(PassiveRequestState::Cancelled));
    }

    #[test]
    fn queue_rejects_duplicate_ids_and_deadline_overflow() {
        let mut queue = PassiveRequestQueue::new();
        queue.enqueue(31, 0, 10).unwrap();
        assert_eq!(queue.enqueue(31, 0, 10), Err(PassiveQueueError::Duplicate));
        assert_eq!(queue.enqueue(32, u64::MAX, 1), Err(PassiveQueueError::Overflow));
    }
}

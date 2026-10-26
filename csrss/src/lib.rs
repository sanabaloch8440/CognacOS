#![no_std]
//! CognacOS CSR server core. The executable is named `csrss.exe` in the native
//! image manifest. This crate implements the ABI-neutral dispatcher/state core;
//! ALPC transport is supplied by the native kernel port subsystem.

use core::{mem::size_of, ptr, sync::atomic::{AtomicBool, AtomicU32, Ordering}};

pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
pub const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002_u32 as i32;
pub const STATUS_PORT_DISCONNECTED: i32 = 0xC000_0037_u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023_u32 as i32;
pub const STATUS_INVALID_SYSTEM_SERVICE: i32 = 0xC000_001C_u32 as i32;
pub const MAX_CSR_MESSAGE: usize = 0x1000;
pub const MAX_CLIENTS: usize = 256;

// AMD64 PORT_MESSAGE layout: 40 bytes, with the client ID and message ID
// preserved exactly as supplied by the kernel port implementation.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PortMessage {
    pub data_length: u16,
    pub total_length: u16,
    pub message_type: u16,
    pub data_info_offset: u16,
    pub client_process: usize,
    pub client_thread: usize,
    pub message_id: u32,
    pub client_view_size: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CsrApiMessage {
    pub port: PortMessage,
    pub api_number: u32,
    pub return_value: i32,
    pub reserved: u32,
    pub data: [u8; MAX_CSR_MESSAGE - size_of::<PortMessage>() - 12],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CsrClientId {
    pub process_id: u32,
    pub thread_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CsrClientRecord {
    pub client_id: CsrClientId,
    pub flags: u32,
    pub references: u32,
    pub last_message_id: u32,
    pub connected: u32,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static NEXT_GENERATION: AtomicU32 = AtomicU32::new(1);
static mut CLIENTS: [CsrClientRecord; MAX_CLIENTS] = [CsrClientRecord {
    client_id: CsrClientId { process_id: 0, thread_id: 0 },
    flags: 0, references: 0, last_message_id: 0, connected: 0,
}; MAX_CLIENTS];
static CLIENT_LOCK: AtomicBool = AtomicBool::new(false);

struct Lock;
impl Lock {
    fn acquire() -> Self {
        while CLIENT_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Lock {
    fn drop(&mut self) { CLIENT_LOCK.store(false, Ordering::Release); }
}

#[derive(Clone, Copy)]
struct ApiEntry {
    number: u32,
    handler: unsafe extern "system" fn(*mut CsrApiMessage, usize) -> i32,
}

// The server DLL index occupies the high 16 bits. CognacOS starts with the
// core CSRSRV namespace (DLL index 0); subsystem-specific APIs are registered
// separately as their owning server DLLs are added.
static mut APIS: [Option<ApiEntry>; 64] = [None; 64];
static API_COUNT: AtomicU32 = AtomicU32::new(0);

/// Initialize the CSR dispatcher exactly once. Repeated initialization is safe.
#[no_mangle]
pub extern "system" fn CsrServerInitialize() -> i32 {
    INITIALIZED.store(true, Ordering::Release);
    STATUS_SUCCESS
}

/// Register an API routine using its complete Windows CSR API number.
/// Duplicate numbers are rejected to avoid ambiguous dispatch.
#[no_mangle]
pub unsafe extern "system" fn CsrServerRegisterApi(
    api_number: u32,
    handler: Option<unsafe extern "system" fn(*mut CsrApiMessage, usize) -> i32>,
) -> i32 {
    if !INITIALIZED.load(Ordering::Acquire) || handler.is_none() {
        return STATUS_INVALID_PARAMETER;
    }
    let _guard = Lock::acquire();
    let count = API_COUNT.load(Ordering::Relaxed) as usize;
    if count > 64 { return STATUS_BUFFER_TOO_SMALL; }
    let entries = ptr::addr_of_mut!(APIS).cast::<Option<ApiEntry>>();
    for i in 0..count {
        let entry = unsafe { (*entries.add(i)).unwrap_unchecked() };
        if entry.number == api_number { return STATUS_INVALID_PARAMETER; }
    }
    if count == 64 { return STATUS_BUFFER_TOO_SMALL; }
    unsafe { entries.add(count).write(Some(ApiEntry { number: api_number, handler: handler.unwrap_unchecked() })); }
    API_COUNT.store((count + 1) as u32, Ordering::Release);
    STATUS_SUCCESS
}

/// Track a process/thread pair associated with a successfully accepted client.
#[no_mangle]
pub unsafe extern "system" fn CsrServerRegisterClient(process_id: u32, thread_id: u32) -> i32 {
    if !INITIALIZED.load(Ordering::Acquire) || process_id == 0 || thread_id == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    let _guard = Lock::acquire();
    let clients = ptr::addr_of_mut!(CLIENTS).cast::<CsrClientRecord>();
    let mut free = None;
    for i in 0..MAX_CLIENTS {
        let record = unsafe { &mut *clients.add(i) };
        if record.connected != 0 && record.client_id.process_id == process_id {
            record.client_id.thread_id = thread_id;
            record.references = record.references.saturating_add(1);
            return STATUS_SUCCESS;
        }
        if record.connected == 0 && free.is_none() { free = Some(i); }
    }
    let Some(index) = free else { return STATUS_BUFFER_TOO_SMALL; };
    unsafe {
        clients.add(index).write(CsrClientRecord {
            client_id: CsrClientId { process_id, thread_id },
            flags: 0, references: 1, last_message_id: 0, connected: 1,
        });
    }
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn CsrServerUnregisterClient(process_id: u32) -> i32 {
    if process_id == 0 { return STATUS_INVALID_PARAMETER; }
    let _guard = Lock::acquire();
    let clients = ptr::addr_of_mut!(CLIENTS).cast::<CsrClientRecord>();
    for i in 0..MAX_CLIENTS {
        let record = unsafe { &mut *clients.add(i) };
        if record.connected != 0 && record.client_id.process_id == process_id {
            if record.references > 1 { record.references -= 1; }
            else { record.connected = 0; record.references = 0; }
            return STATUS_SUCCESS;
        }
    }
    STATUS_INVALID_PARAMETER
}

/// Dispatch one validated CSR message. The caller must already have obtained
/// the message from the kernel ALPC port and validated the sender's access.
#[no_mangle]
pub unsafe extern "system" fn CsrServerDispatchMessage(
    message: *mut CsrApiMessage,
    received_length: usize,
) -> i32 {
    if !INITIALIZED.load(Ordering::Acquire) || message.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if received_length < size_of::<PortMessage>() + 8 || received_length > size_of::<CsrApiMessage>() || received_length > MAX_CSR_MESSAGE {
        return STATUS_INVALID_PARAMETER;
    }
    let msg = unsafe { &mut *message };
    let declared = msg.port.total_length as usize;
    if declared < size_of::<PortMessage>() + 8 || declared > received_length ||
       msg.port.data_length as usize > declared - size_of::<PortMessage>() {
        return STATUS_INVALID_PARAMETER;
    }
    let api = msg.api_number;
    let count = API_COUNT.load(Ordering::Acquire) as usize;
    let entries = ptr::addr_of!(APIS).cast::<Option<ApiEntry>>();
    for i in 0..count {
        let entry = unsafe { (*entries.add(i)).unwrap_unchecked() };
        if entry.number == api {
            let status = unsafe { (entry.handler)(message, received_length) };
            msg.return_value = status;
            return status;
        }
    }
    msg.return_value = STATUS_INVALID_SYSTEM_SERVICE;
    STATUS_INVALID_SYSTEM_SERVICE
}

/// A connection endpoint name compatible with the Windows subsystem naming
/// convention. The object manager must publish this name when ALPC is wired.
#[no_mangle]
pub extern "system" fn CsrServerPortName() -> *const u16 {
    static NAME: [u16; 17] = [
        92, 87, 105, 110, 100, 111, 119, 115, 92, 65, 112, 105, 80, 111, 114, 116, 0,
    ];
    NAME.as_ptr()
}

/// Query a small, stable runtime status record for boot diagnostics.
#[no_mangle]
pub extern "system" fn CsrServerQueryState(out: *mut u32, capacity: u32) -> i32 {
    if out.is_null() || capacity < 2 || !INITIALIZED.load(Ordering::Acquire) {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe {
        out.write(1); // initialized
        out.add(1).write(API_COUNT.load(Ordering::Acquire));
    }
    STATUS_SUCCESS
}


#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn test_handler(message: *mut CsrApiMessage, _length: usize) -> i32 {
        if message.is_null() { return STATUS_INVALID_PARAMETER; }
        unsafe { (*message).return_value = 0x1234; }
        STATUS_SUCCESS
    }

    #[test]
    fn dispatch_validates_message_and_calls_registered_handler() {
        assert_eq!(CsrServerInitialize(), STATUS_SUCCESS);
        assert_eq!(unsafe { CsrServerRegisterApi(0x42, Some(test_handler)) }, STATUS_SUCCESS);
        let mut message: CsrApiMessage = unsafe { core::mem::zeroed() };
        message.api_number = 0x42;
        message.port.total_length = (size_of::<PortMessage>() + 8) as u16;
        message.port.data_length = 8;
        assert_eq!(
            unsafe { CsrServerDispatchMessage(&mut message, size_of::<PortMessage>() + 8) },
            STATUS_SUCCESS
        );
        assert_eq!(message.return_value, STATUS_SUCCESS);
    }

    #[test]
    fn unknown_api_is_rejected_and_bad_lengths_fail_closed() {
        assert_eq!(CsrServerInitialize(), STATUS_SUCCESS);
        let mut message: CsrApiMessage = unsafe { core::mem::zeroed() };
        message.api_number = 0x7fff_ffff;
        message.port.total_length = (size_of::<PortMessage>() + 8) as u16;
        message.port.data_length = 8;
        assert_eq!(
            unsafe { CsrServerDispatchMessage(&mut message, size_of::<PortMessage>() + 8) },
            STATUS_INVALID_SYSTEM_SERVICE
        );
        assert_eq!(
            unsafe { CsrServerDispatchMessage(&mut message, size_of::<PortMessage>()) },
            STATUS_INVALID_PARAMETER
        );
    }

    #[test]
    fn client_lifecycle_is_bounded_and_reference_counted() {
        assert_eq!(CsrServerInitialize(), STATUS_SUCCESS);
        assert_eq!(unsafe { CsrServerRegisterClient(77, 12) }, STATUS_SUCCESS);
        assert_eq!(unsafe { CsrServerRegisterClient(77, 13) }, STATUS_SUCCESS);
        assert_eq!(unsafe { CsrServerUnregisterClient(77) }, STATUS_SUCCESS);
        assert_eq!(unsafe { CsrServerUnregisterClient(77) }, STATUS_SUCCESS);
        assert_eq!(unsafe { CsrServerUnregisterClient(77) }, STATUS_INVALID_PARAMETER);
    }
}

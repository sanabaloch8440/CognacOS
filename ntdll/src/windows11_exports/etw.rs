//! Event Tracing for Windows (ETW) provider runtime for CognacOS NTDLL.
//! The current backend is a bounded process-local diagnostic session. It is not
//! yet connected to the Windows ETW controller/session ABI or a kernel logger.
#![allow(non_snake_case, clippy::missing_safety_doc, dead_code)]
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
const ERROR_MORE_DATA: u32 = 234;
const PROVIDER_CAPACITY: usize = 64;
const EVENT_CAPACITY: usize = 64;
const MAX_EVENT_PAYLOAD: usize = 256;
const MAX_DATA_DESCRIPTORS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Guid { data1: u32, data2: u16, data3: u16, data4: [u8; 8] }
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EventDescriptor { id: u16, version: u8, channel: u8, level: u8, opcode: u8, task: u16, keyword: u64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct EventDataDescriptor { ptr: u64, size: u32, reserved: u32 }
#[derive(Clone, Copy)]
struct EtwProvider { handle: u64, guid: Guid, active: bool }
impl EtwProvider { const EMPTY: Self = Self { handle: 0, guid: Guid::default(), active: false }; }

#[derive(Clone, Copy)]
struct EventRecord {
    active: bool,
    provider: Guid,
    descriptor: EventDescriptor,
    payload_len: u16,
    payload: [u8; MAX_EVENT_PAYLOAD],
}
impl EventRecord {
    const EMPTY: Self = Self { active: false, provider: Guid::default(), descriptor: EventDescriptor::default(), payload_len: 0, payload: [0; MAX_EVENT_PAYLOAD] };
}
static ETW_LOCK: AtomicBool = AtomicBool::new(false);
static ETW_NEXT: AtomicU64 = AtomicU64::new(1);
static mut ETW_PROVIDERS: [EtwProvider; PROVIDER_CAPACITY] = [EtwProvider::EMPTY; PROVIDER_CAPACITY];
static mut ETW_EVENTS: [EventRecord; EVENT_CAPACITY] = [EventRecord::EMPTY; EVENT_CAPACITY];
static mut ETW_EVENT_HEAD: usize = 0;
static mut ETW_EVENT_LEN: usize = 0;

struct EtwGuard;
impl EtwGuard {
    fn lock() -> Self {
        while ETW_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for EtwGuard { fn drop(&mut self) { ETW_LOCK.store(false, Ordering::Release); } }

fn provider_index(entries: *const EtwProvider, handle: u64) -> Option<usize> {
    (0..PROVIDER_CAPACITY).find(|&i| unsafe { let p = &*entries.add(i); p.active && p.handle == handle })
}

/// Internal dequeue hook for a future kernel ETW/session bridge. Payloads are
/// copied into owned storage; no caller pointers are retained.
pub(crate) fn take_event(out_provider: &mut [u8; 16], out_descriptor: &mut [u8; 16], out_payload: &mut [u8; MAX_EVENT_PAYLOAD], out_length: &mut usize) -> bool {
    let _guard = EtwGuard::lock();
    unsafe {
        if ETW_EVENT_LEN == 0 { return false; }
        let events = core::ptr::addr_of_mut!(ETW_EVENTS).cast::<EventRecord>();
        let record = &mut *events.add(ETW_EVENT_HEAD);
        if !record.active { ETW_EVENT_LEN = 0; return false; }
        let guid_bytes = core::slice::from_raw_parts((&record.provider as *const Guid).cast::<u8>(), 16);
        out_provider.copy_from_slice(guid_bytes);
        let desc_bytes = core::slice::from_raw_parts((&record.descriptor as *const EventDescriptor).cast::<u8>(), 16);
        out_descriptor.copy_from_slice(desc_bytes);
        let n = (record.payload_len as usize).min(out_payload.len());
        out_payload[..n].copy_from_slice(&record.payload[..n]);
        *out_length = n;
        record.active = false;
        ETW_EVENT_HEAD = (ETW_EVENT_HEAD + 1) % EVENT_CAPACITY;
        ETW_EVENT_LEN -= 1;
        true
    }
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventRegister(provider: *const Guid, _callback: *const core::ffi::c_void, _context: *mut core::ffi::c_void, handle: *mut u64) -> u32 {
    if provider.is_null() || handle.is_null() { return ERROR_INVALID_PARAMETER; }
    let guid = unsafe { core::ptr::read_unaligned(provider) };
    let _guard = EtwGuard::lock();
    let entries = core::ptr::addr_of_mut!(ETW_PROVIDERS).cast::<EtwProvider>();
    // Registering an already registered GUID is permitted and creates an
    // independent handle, as the native API associates handles with callers.
    for i in 0..PROVIDER_CAPACITY {
        let e = unsafe { &mut *entries.add(i) };
        if !e.active {
            let id = ETW_NEXT.fetch_add(1, Ordering::Relaxed).max(1);
            *e = EtwProvider { handle: id, guid, active: true };
            unsafe { handle.write(id); }
            return ERROR_SUCCESS;
        }
    }
    unsafe { handle.write(0); }
    ERROR_NOT_ENOUGH_MEMORY
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventUnregister(handle: u64) -> u32 {
    let _guard = EtwGuard::lock();
    let entries = core::ptr::addr_of_mut!(ETW_PROVIDERS).cast::<EtwProvider>();
    if let Some(i) = provider_index(entries, handle) {
        unsafe { (*entries.add(i)).active = false; }
        ERROR_SUCCESS
    } else { ERROR_INVALID_PARAMETER }
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventEnabled(handle: u64, descriptor: *const EventDescriptor) -> u8 {
    if descriptor.is_null() { return 0; }
    let _guard = EtwGuard::lock();
    let entries = core::ptr::addr_of!(ETW_PROVIDERS).cast::<EtwProvider>();
    provider_index(entries, handle).is_some() as u8
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventProviderEnabled(handle: u64, _level: u8, _keyword: u64) -> u8 {
    let _guard = EtwGuard::lock();
    let entries = core::ptr::addr_of!(ETW_PROVIDERS).cast::<EtwProvider>();
    provider_index(entries, handle).is_some() as u8
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventWrite(handle: u64, descriptor: *const EventDescriptor, count: u32, data: *const EventDataDescriptor) -> u32 {
    if descriptor.is_null() || count as usize > MAX_DATA_DESCRIPTORS || (count != 0 && data.is_null()) {
        return ERROR_INVALID_PARAMETER;
    }
    let descriptor_value = unsafe { core::ptr::read_unaligned(descriptor) };
    let mut payload = [0u8; MAX_EVENT_PAYLOAD];
    let mut payload_len = 0usize;
    for i in 0..count as usize {
        let item = unsafe { core::ptr::read_unaligned(data.add(i)) };
        let len = item.size as usize;
        if len != 0 && item.ptr == 0 { return ERROR_INVALID_PARAMETER; }
        let Some(end) = payload_len.checked_add(len) else { return ERROR_MORE_DATA; };
        if end > MAX_EVENT_PAYLOAD { return ERROR_MORE_DATA; }
        if len != 0 {
            unsafe { core::ptr::copy_nonoverlapping(item.ptr as *const u8, payload.as_mut_ptr().add(payload_len), len); }
        }
        payload_len = end;
    }

    let _guard = EtwGuard::lock();
    let providers = core::ptr::addr_of!(ETW_PROVIDERS).cast::<EtwProvider>();
    let Some(provider_slot) = provider_index(providers, handle) else { return ERROR_INVALID_PARAMETER; };
    let guid = unsafe { (*providers.add(provider_slot)).guid };
    unsafe {
        let events = core::ptr::addr_of_mut!(ETW_EVENTS).cast::<EventRecord>();
        let slot = (ETW_EVENT_HEAD + ETW_EVENT_LEN) % EVENT_CAPACITY;
        if ETW_EVENT_LEN == EVENT_CAPACITY {
            // A full ring drops the oldest event, matching a bounded tracing
            // buffer's drop-on-overrun behavior without allocating in NTDLL.
            (*events.add(ETW_EVENT_HEAD)).active = false;
            ETW_EVENT_HEAD = (ETW_EVENT_HEAD + 1) % EVENT_CAPACITY;
            ETW_EVENT_LEN -= 1;
        }
        let slot = (ETW_EVENT_HEAD + ETW_EVENT_LEN) % EVENT_CAPACITY;
        let record = &mut *events.add(slot);
        record.active = true;
        record.provider = guid;
        record.descriptor = descriptor_value;
        record.payload_len = payload_len as u16;
        record.payload[..payload_len].copy_from_slice(&payload[..payload_len]);
        ETW_EVENT_LEN += 1;
    }
    ERROR_SUCCESS
}

// These internal/private ETW routines deliberately remain unsupported until a
// controller, activity-ID state, notification lifecycle, and kernel logger exist.
fn EtwCheckCoverage() -> i32 { 0xC0000002u32 as i32 }
fn EtwCreateTraceInstanceId() -> i32 { 0xC0000002u32 as i32 }
fn EtwDeliverDataBlock() -> i32 { 0xC0000002u32 as i32 }
fn EtwEnumerateProcessRegGuids() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventActivityIdControl() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventSetInformation() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventWriteEndScenario() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventWriteEx() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventWriteFull() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventWriteNoRegistration() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventWriteStartScenario() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventWriteString() -> i32 { 0xC0000002u32 as i32 }
fn EtwEventWriteTransfer() -> i32 { 0xC0000002u32 as i32 }
fn EtwGetTraceEnableFlags() -> i32 { 0xC0000002u32 as i32 }
fn EtwGetTraceEnableLevel() -> i32 { 0xC0000002u32 as i32 }
fn EtwGetTraceLoggerHandle() -> i32 { 0xC0000002u32 as i32 }
fn EtwLogTraceEvent() -> i32 { 0xC0000002u32 as i32 }
fn EtwNotificationRegister() -> i32 { 0xC0000002u32 as i32 }
fn EtwNotificationUnregister() -> i32 { 0xC0000002u32 as i32 }
fn EtwpCreateEtwThread() -> i32 { 0xC0000002u32 as i32 }
fn EtwpGetCpuSpeed() -> i32 { 0xC0000002u32 as i32 }
fn EtwProcessPrivateLoggerRequest() -> i32 { 0xC0000002u32 as i32 }
fn EtwRegisterSecurityProvider() -> i32 { 0xC0000002u32 as i32 }
fn EtwRegisterTraceGuidsA() -> i32 { 0xC0000002u32 as i32 }
fn EtwRegisterTraceGuidsW() -> i32 { 0xC0000002u32 as i32 }
fn EtwReplyNotification() -> i32 { 0xC0000002u32 as i32 }
fn EtwSendNotification() -> i32 { 0xC0000002u32 as i32 }
fn EtwSetMark() -> i32 { 0xC0000002u32 as i32 }
fn EtwTraceEventInstance() -> i32 { 0xC0000002u32 as i32 }
fn EtwTraceMessage() -> i32 { 0xC0000002u32 as i32 }
fn EtwTraceMessageVa() -> i32 { 0xC0000002u32 as i32 }
fn EtwUnregisterTraceGuids() -> i32 { 0xC0000002u32 as i32 }
fn EtwWriteUMSecurityEvent() -> i32 { 0xC0000002u32 as i32 }
fn EvtIntReportAuthzEventAndSourceAsync() -> i32 { 0xC0000002u32 as i32 }
fn EvtIntReportEventAndSourceAsync() -> i32 { 0xC0000002u32 as i32 }

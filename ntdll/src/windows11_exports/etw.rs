//! Event Tracing for Windows (ETW) provider runtime for CognacOS NTDLL.
//! Bounded provider runtime with enable callbacks, activity IDs, pull-based consumer
//! transport and teardown hooks. Kernel-wide session control still needs syscall wiring.
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
const ACTIVITY_CAPACITY: usize = 128;
const EVENT_ACTIVITY_CTRL_GET_ID: u32 = 1;
const EVENT_ACTIVITY_CTRL_SET_ID: u32 = 2;
const EVENT_ACTIVITY_CTRL_CREATE_ID: u32 = 3;
const EVENT_ACTIVITY_CTRL_GET_SET_ID: u32 = 4;
const EVENT_ACTIVITY_CTRL_CREATE_SET_ID: u32 = 5;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Guid { data1: u32, data2: u16, data3: u16, data4: [u8; 8] }
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EventDescriptor { id: u16, version: u8, channel: u8, level: u8, opcode: u8, task: u16, keyword: u64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct EventDataDescriptor { ptr: u64, size: u32, reserved: u32 }
type EnableCallback = unsafe extern "system" fn(*const Guid, u32, u8, u64, u64, *const core::ffi::c_void, *mut core::ffi::c_void);
#[derive(Clone, Copy)]
struct EtwProvider {
    handle: u64, guid: Guid, active: bool, owner_pid: u64,
    callback: usize, context: usize, enabled: bool, level: u8,
    match_any: u64, match_all: u64,
}
impl EtwProvider {
    const EMPTY: Self = Self { handle: 0, guid: Guid::default(), active: false, owner_pid: 0,
        callback: 0, context: 0, enabled: false, level: 0, match_any: 0, match_all: 0 };
}
#[derive(Clone, Copy)]
struct ActivitySlot { pid: u64, tid: u64, id: Guid, active: bool }
impl ActivitySlot { const EMPTY: Self = Self { pid: 0, tid: 0, id: Guid::default(), active: false }; }

#[derive(Clone, Copy)]
struct EventRecord {
    active: bool,
    provider: Guid,
    descriptor: EventDescriptor,
    payload_len: u16,
    activity: Guid, related_activity: Guid, process_id: u64, thread_id: u64,
    payload: [u8; MAX_EVENT_PAYLOAD],
}
impl EventRecord {
    const EMPTY: Self = Self { active: false, provider: Guid::default(), descriptor: EventDescriptor::default(), payload_len: 0, activity: Guid::default(), related_activity: Guid::default(), process_id: 0, thread_id: 0, payload: [0; MAX_EVENT_PAYLOAD] };
}
static ETW_LOCK: AtomicBool = AtomicBool::new(false);
static ETW_NEXT: AtomicU64 = AtomicU64::new(1);
static mut ETW_PROVIDERS: [EtwProvider; PROVIDER_CAPACITY] = [EtwProvider::EMPTY; PROVIDER_CAPACITY];
static mut ETW_EVENTS: [EventRecord; EVENT_CAPACITY] = [EventRecord::EMPTY; EVENT_CAPACITY];
static mut ETW_EVENT_HEAD: usize = 0;
static mut ETW_EVENT_LEN: usize = 0;
static mut ETW_ACTIVITIES: [ActivitySlot; ACTIVITY_CAPACITY] = [ActivitySlot::EMPTY; ACTIVITY_CAPACITY];

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


fn provider_matches(p: &EtwProvider, level: u8, keyword: u64) -> bool {
    if !p.active || !p.enabled { return false; }
    if p.level != 0 && level != 0 && level > p.level { return false; }
    if keyword != 0 && p.match_any != 0 && p.match_any & keyword == 0 { return false; }
    keyword == 0 || p.match_all == 0 || p.match_all & keyword == p.match_all
}
fn current_process_thread() -> (u64, u64) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let teb: *const u8;
        core::arch::asm!("mov {}, gs:[0x30]", out(reg) teb, options(nostack, preserves_flags, readonly));
        if teb.is_null() { return (0, 0); }
        return (core::ptr::read_unaligned(teb.add(0x40).cast::<u64>()),
                core::ptr::read_unaligned(teb.add(0x48).cast::<u64>()));
    }
    #[allow(unreachable_code)] (0, 0)
}
fn current_activity(pid: u64, tid: u64) -> Guid {
    let slots = core::ptr::addr_of!(ETW_ACTIVITIES).cast::<ActivitySlot>();
    for i in 0..ACTIVITY_CAPACITY {
        let slot = unsafe { &*slots.add(i) };
        if slot.active && slot.pid == pid && slot.tid == tid { return slot.id; }
    }
    Guid::default()
}
fn new_activity_id() -> Guid {
    let n = ETW_NEXT.fetch_add(1, Ordering::Relaxed);
    Guid { data1: n as u32, data2: (n >> 32) as u16, data3: 0xC06A, data4: [0xC0,0x6A,0x4E,0x54,0x52,0x41,0x43,0x45] }
}

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
pub unsafe extern "system" fn EtwEventRegister(provider: *const Guid, callback: *const core::ffi::c_void, context: *mut core::ffi::c_void, handle: *mut u64) -> u32 {
    if provider.is_null() || handle.is_null() { return ERROR_INVALID_PARAMETER; }
    let guid = unsafe { core::ptr::read_unaligned(provider) };
    let (owner_pid, _) = current_process_thread();
    let _guard = EtwGuard::lock();
    let entries = core::ptr::addr_of_mut!(ETW_PROVIDERS).cast::<EtwProvider>();
    // Registering an already registered GUID is permitted and creates an
    // independent handle, as the native API associates handles with callers.
    for i in 0..PROVIDER_CAPACITY {
        let e = unsafe { &mut *entries.add(i) };
        if !e.active {
            let id = ETW_NEXT.fetch_add(1, Ordering::Relaxed).max(1);
            *e = EtwProvider { handle: id, guid, active: true, owner_pid, callback: callback as usize, context: context as usize, enabled: false, level: 0, match_any: 0, match_all: 0 };
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
    provider_index(entries, handle).map(|i| {
        let p = unsafe { &*entries.add(i) }; p.enabled && provider_matches(p, unsafe { &*descriptor }.level, unsafe { &*descriptor }.keyword)
    }).unwrap_or(false) as u8
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventProviderEnabled(handle: u64, level: u8, keyword: u64) -> u8 {
    let _guard = EtwGuard::lock();
    let entries = core::ptr::addr_of!(ETW_PROVIDERS).cast::<EtwProvider>();
    provider_index(entries, handle).map(|i| provider_matches(unsafe { &*entries.add(i) }, level, keyword)).unwrap_or(false) as u8
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
    let p = unsafe { &*providers.add(provider_slot) };
    if !provider_matches(p, descriptor_value.level, descriptor_value.keyword) { return ERROR_SUCCESS; }
    let guid = p.guid;
    let (process_id, thread_id) = current_process_thread();
    let activity = current_activity(process_id, thread_id);
    unsafe {
        let events = core::ptr::addr_of_mut!(ETW_EVENTS).cast::<EventRecord>();
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
        record.activity = activity; record.related_activity = Guid::default();
        record.process_id = process_id; record.thread_id = thread_id;
        record.payload[..payload_len].copy_from_slice(&payload[..payload_len]);
        ETW_EVENT_LEN += 1;
    }
    ERROR_SUCCESS
}


/// Apply a controller's enable state and filters. Callback execution is outside the lock.
pub(crate) fn controller_set_provider(guid: &Guid, enabled: bool, level: u8, match_any: u64, match_all: u64) -> usize {
    let mut callbacks: [(usize, usize, Guid); PROVIDER_CAPACITY] = [(0, 0, Guid::default()); PROVIDER_CAPACITY];
    let mut count = 0usize;
    {
        let _guard = EtwGuard::lock();
        let entries = core::ptr::addr_of_mut!(ETW_PROVIDERS).cast::<EtwProvider>();
        for i in 0..PROVIDER_CAPACITY {
            let p = unsafe { &mut *entries.add(i) };
            if p.active && p.guid == *guid {
                p.enabled = enabled; p.level = level; p.match_any = match_any; p.match_all = match_all;
                if p.callback != 0 { callbacks[count] = (p.callback, p.context, p.guid); count += 1; }
            }
        }
    }
    for (callback, context, source) in callbacks.into_iter().take(count) {
        let f: EnableCallback = unsafe { core::mem::transmute(callback) };
        unsafe { f(&source, enabled as u32, level, match_any, match_all, core::ptr::null(), context as *mut core::ffi::c_void); }
    }
    count
}

/// Pull one owned event for a consumer; no pointer into the ring escapes this function.
pub(crate) fn consumer_read_event(provider: &mut Guid, descriptor: &mut EventDescriptor, activity: &mut Guid,
    related: &mut Guid, process_id: &mut u64, thread_id: &mut u64,
    payload: &mut [u8; MAX_EVENT_PAYLOAD], payload_len: &mut usize) -> bool {
    let _guard = EtwGuard::lock();
    unsafe {
        if ETW_EVENT_LEN == 0 { return false; }
        let events = core::ptr::addr_of_mut!(ETW_EVENTS).cast::<EventRecord>();
        let r = &mut *events.add(ETW_EVENT_HEAD);
        if !r.active { ETW_EVENT_HEAD = (ETW_EVENT_HEAD + 1) % EVENT_CAPACITY; ETW_EVENT_LEN -= 1; return false; }
        *provider = r.provider; *descriptor = r.descriptor; *activity = r.activity; *related = r.related_activity;
        *process_id = r.process_id; *thread_id = r.thread_id;
        let n = (r.payload_len as usize).min(payload.len()); payload[..n].copy_from_slice(&r.payload[..n]); *payload_len = n;
        r.active = false; ETW_EVENT_HEAD = (ETW_EVENT_HEAD + 1) % EVENT_CAPACITY; ETW_EVENT_LEN -= 1; true
    }
}

/// Process-exit rundown hook. Call before releasing the process's TEB/address space.
pub(crate) fn process_exit_rundown(process_id: u64) {
    let _guard = EtwGuard::lock();
    unsafe {
        let providers = core::ptr::addr_of_mut!(ETW_PROVIDERS).cast::<EtwProvider>();
        for i in 0..PROVIDER_CAPACITY { let p = &mut *providers.add(i); if p.active && p.owner_pid == process_id { *p = EtwProvider::EMPTY; } }
        let activities = core::ptr::addr_of_mut!(ETW_ACTIVITIES).cast::<ActivitySlot>();
        for i in 0..ACTIVITY_CAPACITY { let a = &mut *activities.add(i); if a.active && a.pid == process_id { *a = ActivitySlot::EMPTY; } }
        let events = core::ptr::addr_of_mut!(ETW_EVENTS).cast::<EventRecord>();
        let old_len = ETW_EVENT_LEN;
        let old_head = ETW_EVENT_HEAD;
        let mut retained = 0usize;
        for offset in 0..old_len {
            let source = (old_head + offset) % EVENT_CAPACITY;
            let event = *events.add(source);
            if !event.active || event.process_id == process_id {
                *events.add(source) = EventRecord::EMPTY;
                continue;
            }
            let destination = (old_head + retained) % EVENT_CAPACITY;
            if destination != source { *events.add(destination) = event; *events.add(source) = EventRecord::EMPTY; }
            retained += 1;
        }
        ETW_EVENT_LEN = retained;
        if retained == 0 { ETW_EVENT_HEAD = 0; }
    }
}

#[no_mangle]
pub unsafe extern "system" fn EtwEventActivityIdControl(control_code: u32, activity_id: *mut Guid) -> i32 {
    if activity_id.is_null() { return 0xC000000Du32 as i32; }
    let (pid, tid) = current_process_thread(); if tid == 0 { return 0xC0000225u32 as i32; }
    let mut value = unsafe { core::ptr::read_unaligned(activity_id) };
    let _guard = EtwGuard::lock();
    let slots = core::ptr::addr_of_mut!(ETW_ACTIVITIES).cast::<ActivitySlot>();
    let mut found = None; let mut empty = None;
    for i in 0..ACTIVITY_CAPACITY {
        let a = unsafe { &*slots.add(i) };
        if a.active && a.pid == pid && a.tid == tid { found = Some(i); break; }
        if !a.active && empty.is_none() { empty = Some(i); }
    }
    let idx = match found.or(empty) { Some(i) => i, None => return ERROR_NOT_ENOUGH_MEMORY as i32 };
    let old = if let Some(i) = found { unsafe { (*slots.add(i)).id } } else { Guid::default() };
    let mut next = old;
    match control_code {
        EVENT_ACTIVITY_CTRL_GET_ID => value = old,
        EVENT_ACTIVITY_CTRL_SET_ID => next = value,
        EVENT_ACTIVITY_CTRL_CREATE_ID => value = new_activity_id(),
        EVENT_ACTIVITY_CTRL_GET_SET_ID => { next = value; value = old; }
        EVENT_ACTIVITY_CTRL_CREATE_SET_ID => { value = old; next = new_activity_id(); }
        _ => return 0xC000000Du32 as i32,
    }
    if matches!(control_code, EVENT_ACTIVITY_CTRL_SET_ID | EVENT_ACTIVITY_CTRL_GET_SET_ID | EVENT_ACTIVITY_CTRL_CREATE_SET_ID) || found.is_none() {
        unsafe { *slots.add(idx) = ActivitySlot { pid, tid, id: next, active: true }; }
    }
    unsafe { activity_id.write_unaligned(value); } 0
}

// Session-management internals still require the native kernel syscall bridge.
fn EtwCheckCoverage() -> i32 { 0xC0000002u32 as i32 }
fn EtwCreateTraceInstanceId() -> i32 { 0xC0000002u32 as i32 }
fn EtwDeliverDataBlock() -> i32 { 0xC0000002u32 as i32 }
fn EtwEnumerateProcessRegGuids() -> i32 { 0xC0000002u32 as i32 }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_event_payload_round_trip_and_rundown() {
        let guid = Guid { data1: 0x12345678, data2: 0x1234, data3: 0x5678, data4: [1,2,3,4,5,6,7,8] };
        let mut handle = 0u64;
        assert_eq!(unsafe { EtwEventRegister(&guid, core::ptr::null(), core::ptr::null_mut(), &mut handle) }, ERROR_SUCCESS);
        assert_ne!(handle, 0);
        let descriptor = EventDescriptor { id: 7, version: 1, channel: 0, level: 4, opcode: 0, task: 2, keyword: 1 };
        let payload = [0x10u8, 0x20, 0x30, 0x40];
        let data = EventDataDescriptor { ptr: payload.as_ptr() as u64, size: payload.len() as u32, reserved: 0 };
        assert_eq!(unsafe { EtwEventEnabled(handle, &descriptor) }, 0);
        assert_eq!(controller_set_provider(&guid, true, 5, 1, 0), 0);
        assert_eq!(unsafe { EtwEventEnabled(handle, &descriptor) }, 1);
        assert_eq!(unsafe { EtwEventWrite(handle, &descriptor, 1, &data) }, ERROR_SUCCESS);

        let mut provider = Guid::default();
        let mut received_descriptor = EventDescriptor::default();
        let mut activity = Guid::default();
        let mut related = Guid::default();
        let mut pid = 0u64;
        let mut tid = 0u64;
        let mut payload_bytes = [0u8; MAX_EVENT_PAYLOAD];
        let mut payload_len = 0usize;
        assert!(consumer_read_event(&mut provider, &mut received_descriptor, &mut activity, &mut related,
            &mut pid, &mut tid, &mut payload_bytes, &mut payload_len));
        assert_eq!(provider, guid);
        assert_eq!(received_descriptor.id, descriptor.id);
        assert_eq!(payload_len, payload.len());
        assert_eq!(&payload_bytes[..payload_len], &payload);
        assert_eq!(unsafe { EtwEventUnregister(handle) }, ERROR_SUCCESS);
        assert_eq!(unsafe { EtwEventProviderEnabled(handle, 0, 0) }, 0);
    }
}

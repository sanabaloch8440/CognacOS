#![no_std]
#![no_main]

use core::{cell::UnsafeCell, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
use cognacos_hal_contract::{KernelIoServiceTable, KERNEL_IO_SERVICES_VERSION};
use cognacos_kbdclass::{KeyboardInputData, KeyboardInputStack, QUEUE_CAPACITY};
use cognacos_nt_abi::{
    io::{DEVICE_OBJECT, DRIVER_OBJECT, IO_STACK_LOCATION, IRP},
    ntstatus::*,
    types::PVOID,
};

const HANDOFF_MAGIC: u64 = 0x434f_474e_4143_4b42; // COGNACKB
const HANDOFF_VERSION: u32 = 1;
const MAX_PENDING_READS: usize = 64;
const IRP_MJ_CREATE: usize = 0x00;
const IRP_MJ_CLOSE: usize = 0x02;
const IRP_MJ_READ: usize = 0x03;
const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_SURPRISE_REMOVAL: u8 = 0x17;

#[repr(C)]
pub struct KbdClassDriverHandoff {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    pub io: *const KernelIoServiceTable,
}
struct SharedKeyboard {
    lock: AtomicBool,
    ready: AtomicBool,
    stack: UnsafeCell<KeyboardInputStack>,
}
unsafe impl Sync for SharedKeyboard {}
static KEYBOARD: SharedKeyboard = SharedKeyboard {
    lock: AtomicBool::new(false),
    ready: AtomicBool::new(false),
    stack: UnsafeCell::new(KeyboardInputStack::new()),
};
struct PendingReads(UnsafeCell<[*mut IRP; MAX_PENDING_READS]>);
unsafe impl Sync for PendingReads {}
static PENDING: PendingReads = PendingReads(UnsafeCell::new([core::ptr::null_mut(); MAX_PENDING_READS]));
static HANDOFF: AtomicU64 = AtomicU64::new(0);

struct Guard;
impl Guard {
    fn acquire() -> Self {
        while KEYBOARD.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) { KEYBOARD.lock.store(false, Ordering::Release); }
}
fn io_services() -> Option<&'static KernelIoServiceTable> {
    let address = HANDOFF.load(Ordering::Acquire);
    if address == 0 { return None; }
    let handoff = unsafe { &*(address as *const KbdClassDriverHandoff) };
    unsafe { handoff.io.as_ref() }
}
fn complete(irp: *mut IRP) {
    if irp.is_null() { return; }
    if let Some(io) = io_services() {
        if let Some(callback) = io.complete_irp { unsafe { callback(irp, 0); } }
    }
}
unsafe fn set_irp_result(irp: *mut IRP, status: i32, information: usize) {
    unsafe {
        (*irp).io_status.status.status = status;
        (*irp).io_status.information = information;
    }
}
unsafe fn current_stack(irp: *mut IRP) -> *mut IO_STACK_LOCATION {
    if irp.is_null() { return core::ptr::null_mut(); }
    unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION }
}

fn cancel_pending_reads(status: i32) {
    loop {
        let irp = {
            let _guard = Guard::acquire();
            let slots = unsafe { &mut *PENDING.0.get() };
            let Some(index) = slots.iter().position(|p| !p.is_null()) else { break; };
            let irp = slots[index];
            slots[index] = core::ptr::null_mut();
            unsafe { set_irp_result(irp, status, 0); }
            irp
        };
        complete(irp);
    }
}

/// Complete queued reads after the port DPC has delivered new scan-code packets.
/// The IRP and its METHOD_BUFFERED SystemBuffer remain owned by the I/O manager
/// until IoCompleteRequest is called.
fn service_pending_reads() {
    loop {
        let completion = {
            let _guard = Guard::acquire();
            let slots = unsafe { &mut *PENDING.0.get() };
            let Some(index) = slots.iter().position(|p| !p.is_null()) else { break; };
            let irp = slots[index];
            let stack = unsafe { current_stack(irp) };
            if stack.is_null() {
                slots[index] = core::ptr::null_mut();
                unsafe { set_irp_result(irp, STATUS_INVALID_PARAMETER, 0); }
                Some(irp)
            } else {
                let length = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>()) } as usize;
                let buffer = unsafe { (*irp).associated_irp[0] as *mut KeyboardInputData };
                if buffer.is_null() || length == 0 || length % core::mem::size_of::<KeyboardInputData>() != 0 {
                    slots[index] = core::ptr::null_mut();
                    unsafe { set_irp_result(irp, STATUS_INVALID_PARAMETER, 0); }
                    Some(irp)
                } else {
                    let capacity = (length / core::mem::size_of::<KeyboardInputData>()).min(QUEUE_CAPACITY);
                    let output = unsafe { core::slice::from_raw_parts_mut(buffer, capacity) };
                    let stack_state = unsafe { &mut *KEYBOARD.stack.get() };
                    match stack_state.read_packets(output) {
                        Ok(0) => None,
                        Ok(count) => {
                            slots[index] = core::ptr::null_mut();
                            unsafe { set_irp_result(irp, STATUS_SUCCESS, count * core::mem::size_of::<KeyboardInputData>()); }
                            Some(irp)
                        }
                        Err(_) => {
                            slots[index] = core::ptr::null_mut();
                            unsafe { set_irp_result(irp, STATUS_DEVICE_NOT_READY, 0); }
                            Some(irp)
                        }
                    }
                }
            }
        };
        match completion {
            Some(irp) => complete(irp),
            None => break,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    let object = unsafe { &mut *(driver as *mut DRIVER_OBJECT) };
    if object.driver_extension.is_null() { return STATUS_INVALID_PARAMETER; }
    let handoff = unsafe { &*(object.driver_extension as *const KbdClassDriverHandoff) };
    if handoff.magic != HANDOFF_MAGIC || handoff.version != HANDOFF_VERSION
        || (handoff.size as usize) < core::mem::size_of::<KbdClassDriverHandoff>()
        || handoff.io.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(io) = (unsafe { handoff.io.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    if !io.compatible() || io.version != KERNEL_IO_SERVICES_VERSION { return STATUS_NOT_SUPPORTED; }
    HANDOFF.store(handoff as *const _ as u64, Ordering::Release);
    let _guard = Guard::acquire();
    let stack = unsafe { &mut *KEYBOARD.stack.get() };
    if stack.attach_port().is_err() { return STATUS_DEVICE_BUSY; }
    KEYBOARD.ready.store(true, Ordering::Release);
    object.major_function[IRP_MJ_CREATE] = Some(create_dispatch);
    object.major_function[IRP_MJ_CLOSE] = Some(close_dispatch);
    object.major_function[IRP_MJ_READ] = Some(read_dispatch);
    object.major_function[IRP_MJ_PNP] = Some(pnp_dispatch);
    object.driver_unload = Some(driver_unload);
    STATUS_SUCCESS
}

unsafe extern "system" fn create_dispatch(_device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let status = if !KEYBOARD.ready.load(Ordering::Acquire) { STATUS_DEVICE_NOT_READY } else {
        let _guard = Guard::acquire();
        match unsafe { &mut *KEYBOARD.stack.get() }.open() {
            Ok(()) => STATUS_SUCCESS,
            Err(cognacos_kbdclass::ClassError::AlreadyOpen) => STATUS_DEVICE_BUSY,
            Err(_) => STATUS_DEVICE_NOT_READY,
        }
    };
    unsafe { set_irp_result(irp, status, 0); }
    complete(irp);
    status
}
unsafe extern "system" fn close_dispatch(_device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    {
        let _guard = Guard::acquire();
        if KEYBOARD.ready.load(Ordering::Acquire) { unsafe { &mut *KEYBOARD.stack.get() }.close(); }
    }
    cancel_pending_reads(STATUS_CANCELLED);
    unsafe { set_irp_result(irp, STATUS_SUCCESS, 0); }
    complete(irp);
    STATUS_SUCCESS
}
unsafe extern "system" fn read_dispatch(device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if irp.is_null() || device.is_null() { return STATUS_INVALID_PARAMETER; }
    const DO_BUFFERED_IO: u32 = 0x0000_0004;
    if unsafe { (*device).flags } & DO_BUFFERED_IO == 0 {
        unsafe { set_irp_result(irp, STATUS_INVALID_DEVICE_REQUEST, 0); }
        complete(irp);
        return STATUS_INVALID_DEVICE_REQUEST;
    }
    if !KEYBOARD.ready.load(Ordering::Acquire) {
        unsafe { set_irp_result(irp, STATUS_DEVICE_NOT_READY, 0); }
        complete(irp);
        return STATUS_DEVICE_NOT_READY;
    }
    let stack = unsafe { current_stack(irp) };
    if stack.is_null() {
        unsafe { set_irp_result(irp, STATUS_INVALID_PARAMETER, 0); }
        complete(irp);
        return STATUS_INVALID_PARAMETER;
    }
    let length = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>()) } as usize;
    let buffer = unsafe { (*irp).associated_irp[0] as *mut KeyboardInputData };
    if buffer.is_null() || length == 0 || length % core::mem::size_of::<KeyboardInputData>() != 0 {
        unsafe { set_irp_result(irp, STATUS_INVALID_PARAMETER, 0); }
        complete(irp);
        return STATUS_INVALID_PARAMETER;
    }
    let capacity = (length / core::mem::size_of::<KeyboardInputData>()).min(QUEUE_CAPACITY);
    let _guard = Guard::acquire();
    let output = unsafe { core::slice::from_raw_parts_mut(buffer, capacity) };
    let result = unsafe { &mut *KEYBOARD.stack.get() }.read_packets(output);
    match result {
        Ok(count) if count != 0 => {
            let bytes = count * core::mem::size_of::<KeyboardInputData>();
            unsafe { set_irp_result(irp, STATUS_SUCCESS, bytes); }
            drop(_guard);
            complete(irp);
            STATUS_SUCCESS
        }
        Ok(_) => {
            let slots = unsafe { &mut *PENDING.0.get() };
            if let Some(slot) = slots.iter_mut().find(|p| p.is_null()) {
                *slot = irp;
                unsafe {
                    (*stack).control |= 0x01; // SL_PENDING_RETURNED / IoMarkIrpPending.
                    set_irp_result(irp, STATUS_PENDING, 0);
                }
                STATUS_PENDING
            } else {
                unsafe { set_irp_result(irp, STATUS_INSUFFICIENT_RESOURCES, 0); }
                drop(_guard);
                complete(irp);
                STATUS_INSUFFICIENT_RESOURCES
            }
        }
        Err(cognacos_kbdclass::ClassError::NotOpen) => {
            unsafe { set_irp_result(irp, STATUS_ACCESS_DENIED, 0); }
            drop(_guard);
            complete(irp);
            STATUS_ACCESS_DENIED
        }
        Err(_) => {
            unsafe { set_irp_result(irp, STATUS_INVALID_PARAMETER, 0); }
            drop(_guard);
            complete(irp);
            STATUS_INVALID_PARAMETER
        }
    }
}
unsafe extern "system" fn pnp_dispatch(_device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { current_stack(irp) };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let status = match unsafe { (*stack).minor_function } {
        IRP_MN_START_DEVICE => STATUS_SUCCESS,
        IRP_MN_STOP_DEVICE => {
            cancel_pending_reads(STATUS_DEVICE_NOT_READY);
            STATUS_SUCCESS
        }
        IRP_MN_SURPRISE_REMOVAL | IRP_MN_REMOVE_DEVICE => {
            cancel_pending_reads(STATUS_DEVICE_NOT_READY);
            let _guard = Guard::acquire();
            if KEYBOARD.ready.swap(false, Ordering::AcqRel) {
                unsafe { &mut *KEYBOARD.stack.get() }.detach_port();
            }
            STATUS_SUCCESS
        }
        _ => STATUS_INVALID_DEVICE_REQUEST,
    };
    unsafe { set_irp_result(irp, status, 0); }
    status
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn KeyboardClassServiceCallback(
    _class_device: *mut DEVICE_OBJECT,
    input_start: *const KeyboardInputData,
    input_end: *const KeyboardInputData,
    input_consumed: *mut u32,
) {
    if !input_consumed.is_null() { unsafe { input_consumed.write(0); } }
    if !KEYBOARD.ready.load(Ordering::Acquire) || input_start.is_null()
        || input_end.is_null() || input_consumed.is_null() { return; }
    let start = input_start as usize;
    let end = input_end as usize;
    if end < start { return; }
    let bytes = end - start;
    if bytes % core::mem::size_of::<KeyboardInputData>() != 0 { return; }
    let count = (bytes / core::mem::size_of::<KeyboardInputData>()).min(QUEUE_CAPACITY);
    let mut consumed = 0u32;
    {
        let _guard = Guard::acquire();
        let stack = unsafe { &mut *KEYBOARD.stack.get() };
        for index in 0..count {
            let packet = unsafe { input_start.add(index).read() };
            if stack.submit_from_port(packet).is_err() { break; }
            consumed += 1;
        }
    }
    unsafe { input_consumed.write(consumed); }
    service_pending_reads();
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacKbdClassOpen() -> i32 {
    if !KEYBOARD.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    let _guard = Guard::acquire();
    match unsafe { &mut *KEYBOARD.stack.get() }.open() {
        Ok(()) => STATUS_SUCCESS,
        Err(cognacos_kbdclass::ClassError::AlreadyOpen) => STATUS_DEVICE_BUSY,
        Err(_) => STATUS_DEVICE_NOT_READY,
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacKbdClassClose() {
    { let _guard = Guard::acquire(); if KEYBOARD.ready.load(Ordering::Acquire) { unsafe { &mut *KEYBOARD.stack.get() }.close(); } }
    cancel_pending_reads(STATUS_CANCELLED);
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacKbdClassReadPackets(output: *mut KeyboardInputData, capacity: usize, packets_read: *mut usize) -> i32 {
    if output.is_null() || packets_read.is_null() || capacity == 0 || capacity > QUEUE_CAPACITY { return STATUS_INVALID_PARAMETER; }
    if !KEYBOARD.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    let _guard = Guard::acquire();
    let output_slice = unsafe { core::slice::from_raw_parts_mut(output, capacity) };
    match unsafe { &mut *KEYBOARD.stack.get() }.read_packets(output_slice) {
        Ok(count) => { unsafe { packets_read.write(count); } STATUS_SUCCESS }
        Err(cognacos_kbdclass::ClassError::NotOpen) => STATUS_ACCESS_DENIED,
        Err(cognacos_kbdclass::ClassError::InvalidBuffer) => STATUS_INVALID_PARAMETER,
        Err(_) => STATUS_DEVICE_NOT_READY,
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacKbdClassPending() -> usize {
    if !KEYBOARD.ready.load(Ordering::Acquire) { return 0; }
    let _guard = Guard::acquire();
    unsafe { &*KEYBOARD.stack.get() }.pending()
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacKbdClassDropped() -> u64 {
    if !KEYBOARD.ready.load(Ordering::Acquire) { return 0; }
    let _guard = Guard::acquire();
    unsafe { &*KEYBOARD.stack.get() }.dropped_packets()
}
unsafe extern "system" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    cancel_pending_reads(STATUS_DEVICE_NOT_READY);
    let _guard = Guard::acquire();
    if KEYBOARD.ready.swap(false, Ordering::AcqRel) { unsafe { &mut *KEYBOARD.stack.get() }.detach_port(); }
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"KBDCLAS_";
#[used]
#[unsafe(link_section = ".data")]
static DRIVER_COOKIE: u64 = 0x5244_4F54_4F53_0001;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

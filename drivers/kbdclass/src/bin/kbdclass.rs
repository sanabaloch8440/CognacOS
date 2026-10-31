#![no_std]
#![no_main]

use core::{cell::UnsafeCell, sync::atomic::{AtomicBool, Ordering}};
use cognacos_kbdclass::{KeyboardInputData, KeyboardInputStack, QUEUE_CAPACITY};
use cognacos_nt_abi::{io::{DEVICE_OBJECT, DRIVER_OBJECT}, ntstatus::*, types::PVOID};

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

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    if KEYBOARD.ready.load(Ordering::Acquire) { return STATUS_DEVICE_BUSY; }
    let _guard = Guard::acquire();
    let stack = unsafe { &mut *KEYBOARD.stack.get() };
    if stack.attach_port().is_err() { return STATUS_DEVICE_BUSY; }
    KEYBOARD.ready.store(true, Ordering::Release);
    STATUS_SUCCESS
}

/// Windows Kbdclass service callback contract. The port driver supplies a
/// half-open packet range and receives the number of packets consumed. This
/// callback is nonblocking, bounded, and safe for the port driver's DPC path.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn KeyboardClassServiceCallback(
    class_device: *mut DEVICE_OBJECT,
    input_start: *const KeyboardInputData,
    input_end: *const KeyboardInputData,
    input_consumed: *mut u32,
) {
    let _ = class_device;
    if !input_consumed.is_null() { unsafe { input_consumed.write(0); } }
    if !KEYBOARD.ready.load(Ordering::Acquire) || input_start.is_null()
        || input_end.is_null() || input_consumed.is_null() {
        return;
    }
    let start = input_start as usize;
    let end = input_end as usize;
    if end < start { return; }
    let bytes = end - start;
    if bytes % core::mem::size_of::<KeyboardInputData>() != 0 { return; }
    let count = (bytes / core::mem::size_of::<KeyboardInputData>()).min(QUEUE_CAPACITY);
    let _guard = Guard::acquire();
    let stack = unsafe { &mut *KEYBOARD.stack.get() };
    let mut consumed = 0u32;
    for index in 0..count {
        let packet = unsafe { input_start.add(index).read() };
        if stack.submit_from_port(packet).is_err() { break; }
        consumed += 1;
    }
    unsafe { input_consumed.write(consumed); }
}

/// Nonblocking native read endpoint used by the current kernel input adapter.
/// The Windows IRP_MJ_READ wrapper will call this after capturing a buffered
/// output range; no user pointer is dereferenced here.
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
    let _guard = Guard::acquire();
    if KEYBOARD.ready.load(Ordering::Acquire) { unsafe { &mut *KEYBOARD.stack.get() }.close(); }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn CognacKbdClassReadPackets(
    output: *mut KeyboardInputData,
    capacity: usize,
    packets_read: *mut usize,
) -> i32 {
    if output.is_null() || packets_read.is_null() || capacity == 0 || capacity > QUEUE_CAPACITY {
        return STATUS_INVALID_PARAMETER;
    }
    if !KEYBOARD.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    let _guard = Guard::acquire();
    let stack = unsafe { &mut *KEYBOARD.stack.get() };
    let output_slice = unsafe { core::slice::from_raw_parts_mut(output, capacity) };
    match stack.read_packets(output_slice) {
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

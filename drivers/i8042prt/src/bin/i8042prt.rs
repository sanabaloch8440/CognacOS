#![no_std]
#![no_main]

use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
use cognacos_hal_contract::{
    DeviceServiceTable, HalExtendedServiceTable, HalServiceTable, KernelIoServiceTable,
    HAL_EXTENDED_SERVICES_VERSION, HAL_CONTRACT_VERSION, DEVICE_SERVICES_VERSION,
    KERNEL_IO_SERVICES_VERSION,
};
use cognacos_i8042prt::{self as i8042, KeyboardPacketSink, PortIo, PowerAwarePortIo, Ps2KeyboardPort, ScanDecoder, KeyboardPortState};
use cognacos_kbdclass::KeyboardInputData;
use cognacos_nt_abi::{
    io::{DEVICE_OBJECT, DRIVER_OBJECT, IO_STACK_LOCATION, IRP},
    ntstatus::*,
    types::PVOID,
};

const HANDOFF_MAGIC: u64 = 0x434f_474e_4143_4932; // COGNACI2
const HANDOFF_VERSION: u32 = 1;
const IRP_MJ_INTERNAL_DEVICE_CONTROL: usize = 0x0f;
const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_QUERY_REMOVE_DEVICE: u8 = 0x01;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_CANCEL_REMOVE_DEVICE: u8 = 0x03;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_QUERY_STOP_DEVICE: u8 = 0x05;
const IRP_MN_CANCEL_STOP_DEVICE: u8 = 0x06;
const IRP_MN_SURPRISE_REMOVAL: u8 = 0x17;
const IOCTL_INTERNAL_KEYBOARD_CONNECT: u32 = (0x0b << 16) | (0x80 << 2) | 3;

#[repr(C)]
pub struct I8042DriverHandoff {
    pub magic: u64,
    pub size: u32,
    pub version: u32,
    pub interrupt_vector: u32,
    pub reserved: u32,
    pub interrupt_context: u64,
    pub hal: *const HalServiceTable,
    pub extended_hal: *const HalExtendedServiceTable,
    pub device: *const DeviceServiceTable,
    pub io: *const KernelIoServiceTable,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ConnectData {
    class_device_object: *mut DEVICE_OBJECT,
    class_service: PVOID,
}

struct ConnectState(UnsafeCell<MaybeUninit<ConnectData>>);
unsafe impl Sync for ConnectState {}
static HANDOFF: AtomicU64 = AtomicU64::new(0);
static CONNECTED: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static PORT: UnsafeCell<Ps2KeyboardPort> = UnsafeCell::new(Ps2KeyboardPort::new());
static DECODER: UnsafeCell<ScanDecoder> = UnsafeCell::new(ScanDecoder::new());
static CONNECT: ConnectState = ConnectState(UnsafeCell::new(MaybeUninit::uninit()));
static RUNTIME_LOCK: AtomicBool = AtomicBool::new(false);
struct Guard;
impl Guard {
    fn acquire() -> Self {
        while RUNTIME_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) { RUNTIME_LOCK.store(false, Ordering::Release); }
}

fn handoff() -> Option<&'static I8042DriverHandoff> {
    let address = HANDOFF.load(Ordering::Acquire);
    if address == 0 { return None; }
    Some(unsafe { &*(address as *const I8042DriverHandoff) })
}
fn connected() -> Option<ConnectData> {
    if !CONNECTED.load(Ordering::Acquire) { return None; }
    Some(unsafe { (*CONNECT.0.get()).assume_init_read() })
}

struct HalPort<'a> { hal: &'a HalServiceTable }
impl PortIo for HalPort<'_> {
    fn read8(&mut self, port: u16) -> u8 {
        let Some(read) = self.hal.read_port_u8 else { return 0xff; };
        let mut value = 0xff;
        if unsafe { read(port, &mut value) }.code != 0 { 0xff } else { value }
    }
    fn write8(&mut self, port: u16, value: u8) {
        if let Some(write) = self.hal.write_port_u8 { let _ = unsafe { write(port, value) }; }
    }
}
impl PowerAwarePortIo for HalPort<'_> {
    fn set_keyboard_irq_enabled(&mut self, enabled: bool) -> Result<(), i8042::PortError> {
        let ext = handoff().ok_or(i8042::PortError::ControllerFailure)?;
        let dev = unsafe { ext.device.as_ref() }.ok_or(i8042::PortError::ControllerFailure)?;
        let set = dev.set_interrupt_enabled.ok_or(i8042::PortError::ControllerFailure)?;
        if unsafe { set(ext.interrupt_vector, enabled) }.code == 0 { Ok(()) } else { Err(i8042::PortError::ControllerFailure) }
    }
    fn synchronize_keyboard_irq(&mut self) -> Result<(), i8042::PortError> {
        let ext = handoff().ok_or(i8042::PortError::ControllerFailure)?;
        let hal = unsafe { ext.extended_hal.as_ref() }.ok_or(i8042::PortError::ControllerFailure)?;
        let sync = hal.synchronize_interrupt.ok_or(i8042::PortError::ControllerFailure)?;
        if unsafe { sync(ext.interrupt_vector, ext.interrupt_context, Some(i8042_isr)) }.code == 0 {
            Ok(())
        } else { Err(i8042::PortError::ControllerFailure) }
    }
}

struct ClassSink { connect: ConnectData }
impl KeyboardPacketSink for ClassSink {
    fn submit_keyboard_packet(&mut self, packet: KeyboardInputData) -> Result<(), ()> {
        let Some(service) = (unsafe { core::mem::transmute::<PVOID,
            unsafe extern "system" fn(*mut DEVICE_OBJECT, *const KeyboardInputData, *const KeyboardInputData, *mut u32)>(
                self.connect.class_service) }) else { return Err(()); };
        let input = &packet as *const KeyboardInputData;
        let mut consumed = 0u32;
        unsafe { service(self.connect.class_device_object, input, input.add(1), &mut consumed); }
        if consumed == 1 { Ok(()) } else { Err(()) }
    }
}

extern "C" fn i8042_isr(_context: u64) -> bool {
    if !RUNNING.load(Ordering::Acquire) { return false; }
    let Some(ext) = handoff() else { return false; };
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return false; };
    let Some(read) = hal.read_port_u8 else { return false; };
    let mut status = 0u8;
    if unsafe { read(i8042::STATUS_PORT, &mut status) }.code != 0 { return false; }
    status & i8042::STATUS_OUTPUT_FULL != 0 && status & 0x20 == 0
}
extern "C" fn i8042_dpc(_context: u64) {
    if !RUNNING.load(Ordering::Acquire) { return; }
    let Some(ext) = handoff() else { return; };
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return; };
    let Some(connect) = connected() else { return; };
    let _guard = Guard::acquire();
    let mut io = HalPort { hal };
    let mut sink = ClassSink { connect };
    let decoder = unsafe { &mut *DECODER.get() };
    let _ = i8042::drain_keyboard_irq(&mut io, decoder, &mut sink, 32);
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn I8042StartDevice() -> i32 {
    if RUNNING.load(Ordering::Acquire) { return STATUS_SUCCESS; }
    let Some(ext) = handoff() else { return STATUS_DEVICE_NOT_READY; };
    if !CONNECTED.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    let Some(device) = (unsafe { ext.device.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    let Some(register) = device.register_interrupt else { return STATUS_NOT_SUPPORTED; };
    if unsafe { register(ext.interrupt_vector, ext.interrupt_context, Some(i8042_isr), Some(i8042_dpc)) }.code != 0 {
        return STATUS_DEVICE_NOT_READY;
    }
    RUNNING.store(true, Ordering::Release);
    let _guard = Guard::acquire();
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else {
        RUNNING.store(false, Ordering::Release);
        if let Some(unregister) = device.unregister_interrupt { let _ = unsafe { unregister(ext.interrupt_vector, ext.interrupt_context) }; }
        return STATUS_INVALID_PARAMETER;
    };
    let mut io = HalPort { hal };
    let port = unsafe { &mut *PORT.get() };
    if port.start(&mut io, 100_000).is_err() {
        RUNNING.store(false, Ordering::Release);
        let _ = io.set_keyboard_irq_enabled(false);
        if let Some(unregister) = device.unregister_interrupt { let _ = unsafe { unregister(ext.interrupt_vector, ext.interrupt_context) }; }
        return STATUS_DEVICE_NOT_READY;
    }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn I8042StopDevice() -> i32 {
    if !RUNNING.load(Ordering::Acquire) { return STATUS_SUCCESS; }
    let Some(ext) = handoff() else { return STATUS_DEVICE_NOT_READY; };
    let Some(device) = (unsafe { ext.device.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    let _guard = Guard::acquire();
    let mut io = HalPort { hal };
    let port = unsafe { &mut *PORT.get() };
    if port.state() == KeyboardPortState::Running && port.suspend(&mut io, 100_000).is_err() {
        return STATUS_DEVICE_NOT_READY;
    }
    RUNNING.store(false, Ordering::Release);
    if let Some(unregister) = device.unregister_interrupt {
        if unsafe { unregister(ext.interrupt_vector, ext.interrupt_context) }.code != 0 {
            return STATUS_DEVICE_NOT_READY;
        }
    }
    STATUS_SUCCESS
}

unsafe extern "system" fn pnp_dispatch(device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if device.is_null() || irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let status = match unsafe { (*stack).minor_function } {
        IRP_MN_START_DEVICE => unsafe { I8042StartDevice() },
        IRP_MN_STOP_DEVICE | IRP_MN_SURPRISE_REMOVAL | IRP_MN_REMOVE_DEVICE => unsafe { I8042StopDevice() },
        IRP_MN_QUERY_REMOVE_DEVICE | IRP_MN_CANCEL_REMOVE_DEVICE
        | IRP_MN_QUERY_STOP_DEVICE | IRP_MN_CANCEL_STOP_DEVICE => STATUS_SUCCESS,
        _ => STATUS_INVALID_DEVICE_REQUEST,
    };
    unsafe {
        (*irp).io_status.status.status = status;
        (*irp).io_status.information = 0;
    }
    status
}

unsafe extern "system" fn internal_device_control_dispatch(device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if device.is_null() || irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let code = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(12).cast::<u32>()) };
    let status = if code != IOCTL_INTERNAL_KEYBOARD_CONNECT {
        STATUS_INVALID_DEVICE_REQUEST
    } else {
        let input = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(24).cast::<*const ConnectData>()) };
        if input.is_null() || CONNECTED.load(Ordering::Acquire) {
            STATUS_INVALID_PARAMETER
        } else {
            let connect = unsafe { input.read_unaligned() };
            if connect.class_device_object.is_null() || connect.class_service.is_null() {
                STATUS_INVALID_PARAMETER
            } else {
                let _guard = Guard::acquire();
                unsafe { (*CONNECT.0.get()).write(connect); }
                CONNECTED.store(true, Ordering::Release);
                STATUS_SUCCESS
            }
        }
    };
    unsafe {
        (*irp).io_status.status.status = status;
        (*irp).io_status.information = 0;
    }
    if let Some(ext) = handoff() {
        if let Some(io) = unsafe { ext.io.as_ref() } {
            if let Some(complete) = io.complete_irp { unsafe { complete(irp, 0); } }
        }
    }
    status
}

unsafe extern "system" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    let _ = unsafe { I8042StopDevice() };
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    let object = unsafe { &mut *(driver as *mut DRIVER_OBJECT) };
    if object.driver_extension.is_null() { return STATUS_INVALID_PARAMETER; }
    let ext = unsafe { &*(object.driver_extension as *const I8042DriverHandoff) };
    if ext.magic != HANDOFF_MAGIC || ext.version != HANDOFF_VERSION
        || (ext.size as usize) < core::mem::size_of::<I8042DriverHandoff>()
        || ext.interrupt_vector == 0 || ext.interrupt_context == 0
        || ext.hal.is_null() || ext.extended_hal.is_null() || ext.device.is_null() || ext.io.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(hal) = (unsafe { ext.hal.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    let Some(extended) = (unsafe { ext.extended_hal.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    let Some(device) = (unsafe { ext.device.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    let Some(io) = (unsafe { ext.io.as_ref() }) else { return STATUS_INVALID_PARAMETER; };
    if !hal.compatible() || hal.version != HAL_CONTRACT_VERSION
        || hal.read_port_u8.is_none() || hal.write_port_u8.is_none()
        || !extended.compatible() || extended.version != HAL_EXTENDED_SERVICES_VERSION
        || extended.synchronize_interrupt.is_none()
        || !device.compatible() || device.version != DEVICE_SERVICES_VERSION
        || device.register_interrupt.is_none() || device.unregister_interrupt.is_none()
        || device.set_interrupt_enabled.is_none()
        || !io.compatible() || io.version != KERNEL_IO_SERVICES_VERSION {
        return STATUS_NOT_SUPPORTED;
    }
    HANDOFF.store(ext as *const _ as u64, Ordering::Release);
    object.major_function[IRP_MJ_INTERNAL_DEVICE_CONTROL] = Some(internal_device_control_dispatch);
    object.major_function[IRP_MJ_PNP] = Some(pnp_dispatch);
    object.driver_unload = Some(driver_unload);
    STATUS_SUCCESS
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"I8042PRT";
#[used]
#[unsafe(link_section = ".data")]
static DRIVER_COOKIE: u64 = 0x5244_4F54_4F53_0001;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

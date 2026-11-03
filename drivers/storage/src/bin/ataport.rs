#![no_std]
#![no_main]
//! ATA port driver (ataport.sys): owns buffered IRP validation/completion and
//! a project-owned versioned controller miniport interface. This is not a
//! claim of Microsoft private ATA miniport binary ABI compatibility.
use core::{cell::UnsafeCell, ffi::c_void, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
use cognacos_nt_abi::{io::{DRIVER_OBJECT, IRP, DEVICE_OBJECT, IO_STACK_LOCATION}, ntstatus::*, types::PVOID};
use cognacos_storage::ata_port::{AtaPortMiniportInterface, AtaPortOperation, AtaPortRequest, validate_request, ATA_PORT_INTERFACE_VERSION};

const IRP_MJ_CREATE: usize = 0;
const IRP_MJ_CLOSE: usize = 2;
const IRP_MJ_READ: usize = 3;
const IRP_MJ_WRITE: usize = 4;
const IRP_MJ_FLUSH_BUFFERS: usize = 9;
const IRP_MJ_MAXIMUM_FUNCTION: usize = 27;
const IRP_BUFFERED_IO: u32 = 0x10;
const SECTOR_SIZE: u32 = 512;
const MAX_PIO_SECTORS: u32 = 256;

unsafe extern "win64" { fn IoCompleteRequest(irp: *mut IRP, priority_boost: i8); }
core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.global IoCompleteRequest
.type IoCompleteRequest,@function
IoCompleteRequest: jmp QWORD PTR [rip + __imp_ntoskrnl_IoCompleteRequest]
.size IoCompleteRequest, .-IoCompleteRequest
.section .data.imports.ntoskrnl,"aw",@progbits
.p2align 3
.global __imp_ntoskrnl_IoCompleteRequest
.type __imp_ntoskrnl_IoCompleteRequest,@object
__imp_ntoskrnl_IoCompleteRequest: .quad 0
.size __imp_ntoskrnl_IoCompleteRequest,8
"#);

struct MiniportCell(UnsafeCell<Option<AtaPortMiniportInterface>>);
unsafe impl Sync for MiniportCell {}
static MINIport_LOCK: AtomicBool = AtomicBool::new(false);
static MINIport: MiniportCell = MiniportCell(UnsafeCell::new(None));
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

struct Guard;
impl Guard {
    fn lock() -> Self {
        while MINIport_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Guard { fn drop(&mut self) { MINIport_LOCK.store(false, Ordering::Release); } }

fn miniport_snapshot() -> Option<AtaPortMiniportInterface> {
    let _guard = Guard::lock();
    unsafe { *MINIport.0.get() }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn AtaPortRegisterMiniport(interface: *const AtaPortMiniportInterface) -> i32 {
    if interface.is_null() { return STATUS_INVALID_PARAMETER; }
    let candidate = unsafe { core::ptr::read_unaligned(interface) };
    if candidate.size as usize != core::mem::size_of::<AtaPortMiniportInterface>()
        || candidate.version != ATA_PORT_INTERFACE_VERSION || candidate.submit.is_none()
    { return STATUS_REVISION_MISMATCH; }
    let _guard = Guard::lock();
    let slot = unsafe { &mut *MINIport.0.get() };
    if slot.is_some() { return STATUS_DEVICE_BUSY; }
    *slot = Some(candidate);
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn AtaPortUnregisterMiniport(context: *mut c_void) -> i32 {
    let _guard = Guard::lock();
    let slot = unsafe { &mut *MINIport.0.get() };
    match slot.as_ref() {
        Some(current) if current.miniport_context == context => { *slot = None; STATUS_SUCCESS }
        Some(_) => STATUS_ACCESS_DENIED,
        None => STATUS_DEVICE_NOT_READY,
    }
}

unsafe extern "win64" fn request_complete(context: *mut c_void, _id: u64, status: i32, bytes: u32) {
    let irp = context.cast::<IRP>();
    if irp.is_null() { return; }
    let expected = unsafe { (*irp).io_status.information as u32 };
    let final_status = if bytes > expected || (status == STATUS_SUCCESS && bytes != expected) {
        STATUS_DEVICE_DATA_ERROR
    } else { status };
    unsafe {
        (*irp).io_status.status.status = final_status;
        (*irp).io_status.information = if final_status == STATUS_SUCCESS { bytes as usize } else { 0 };
        IoCompleteRequest(irp, 0);
    }
}

fn current_stack(irp: *mut IRP) -> *mut IO_STACK_LOCATION {
    if irp.is_null() { core::ptr::null_mut() } else { unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION } }
}

unsafe extern "win64" fn dispatch(_device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = current_stack(irp);
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let major = unsafe { (*stack).major_function as usize };
    if major == IRP_MJ_CREATE || major == IRP_MJ_CLOSE {
        unsafe { (*irp).io_status.status.status = STATUS_SUCCESS; (*irp).io_status.information = 0; IoCompleteRequest(irp, 0); }
        return STATUS_SUCCESS;
    }
    let operation = match major {
        IRP_MJ_READ => AtaPortOperation::Read,
        IRP_MJ_WRITE => AtaPortOperation::Write,
        IRP_MJ_FLUSH_BUFFERS => AtaPortOperation::Flush,
        _ => return STATUS_INVALID_DEVICE_REQUEST,
    };
    let Some(miniport) = miniport_snapshot() else {
        unsafe { (*irp).io_status.status.status = STATUS_DEVICE_NOT_READY; (*irp).io_status.information = 0; IoCompleteRequest(irp, 0); }
        return STATUS_DEVICE_NOT_READY;
    };
    let mut request = AtaPortRequest {
        size: core::mem::size_of::<AtaPortRequest>() as u32, operation,
        request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed).max(1),
        lba: 0, sector_count: 0, flags: 0, system_buffer: core::ptr::null_mut(),
        sg_list: core::ptr::null(), sg_count: 0, timeout_ms: 30_000,
        packet_cdb: core::ptr::null(), packet_cdb_length: 0, reserved: [0; 7],
    };
    let expected = if operation == AtaPortOperation::Flush { 0 } else {
        let length = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>()) };
        let offset = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<i64>()) };
        let system_buffer = unsafe { (*irp).associated_irp[0] as *mut u8 };
        let flags = unsafe { (*irp).flags };
        if flags & IRP_BUFFERED_IO == 0 || system_buffer.is_null() || offset < 0
            || offset as u64 % SECTOR_SIZE as u64 != 0 || length == 0
            || length % SECTOR_SIZE != 0 || length / SECTOR_SIZE > MAX_PIO_SECTORS
        {
            unsafe { (*irp).io_status.status.status = STATUS_INVALID_PARAMETER; (*irp).io_status.information = 0; IoCompleteRequest(irp, 0); }
            return STATUS_INVALID_PARAMETER;
        }
        request.lba = offset as u64 / SECTOR_SIZE as u64;
        request.sector_count = length / SECTOR_SIZE;
        request.system_buffer = system_buffer;
        length
    };
    unsafe { (*irp).io_status.information = expected as usize; }
    if let Err(status) = validate_request(&request) {
        unsafe { (*irp).io_status.status.status = status; (*irp).io_status.information = 0; IoCompleteRequest(irp, 0); }
        return status;
    }
    let Some(submit) = miniport.submit else { return STATUS_DEVICE_NOT_READY; };
    let status = unsafe { submit(miniport.miniport_context, &request, irp.cast(), request_complete) };
    if status == STATUS_PENDING || status == STATUS_SUCCESS {
        // The miniport contract requires exactly one completion for accepted work.
        STATUS_PENDING
    } else {
        unsafe { (*irp).io_status.status.status = status; (*irp).io_status.information = 0; IoCompleteRequest(irp, 0); }
        status
    }
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"ATAPORT_";

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: *mut DRIVER_OBJECT, _context: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe {
        for index in 0..=IRP_MJ_MAXIMUM_FUNCTION { (*driver).major_function[index] = Some(dispatch); }
        (*driver).driver_unload = None;
    }
    STATUS_SUCCESS
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

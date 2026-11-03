#![no_std]
#![no_main]
//! ATA port driver: buffered IRP validation, bounded request ownership and
//! synchronous PIO dispatch through a versioned CognacOS miniport interface.
//! Version 1 deliberately does not advertise DMA or asynchronous cancellation.
use core::{cell::UnsafeCell, ffi::c_void, sync::atomic::{AtomicBool, Ordering}};
use cognacos_nt_abi::{
    io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STACK_LOCATION},
    ntstatus::*,
    types::PVOID,
};
use cognacos_storage::ata_port::{
    AtaPortMiniportInterface, AtaPortOperation, AtaPortRequest, AtaRequestTable,
    validate_request, AtaPortServices, AtaPortRegisterMiniport, ATA_PORT_INTERFACE_VERSION,
};

const IRP_MJ_CREATE: usize = 0;
const IRP_MJ_CLOSE: usize = 2;
const IRP_MJ_READ: usize = 3;
const IRP_MJ_WRITE: usize = 4;
const IRP_MJ_FLUSH_BUFFERS: usize = 9;
const IRP_MJ_MAXIMUM_FUNCTION: usize = 27;
const IRP_BUFFERED_IO: u32 = 0x10;
const SECTOR_SIZE: u32 = 512;
const MAX_PIO_SECTORS: u32 = 256;
const IO_NO_INCREMENT: i8 = 0;

unsafe extern "win64" { fn IoCompleteRequest(irp: *mut IRP, priority_boost: i8); fn IoRegisterDriverInterface(owner: *mut DRIVER_OBJECT, name: *const u8, name_length: u16, interface: *const c_void, size: u32) -> i32; }
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
struct RequestCell(UnsafeCell<AtaRequestTable>);
unsafe impl Sync for RequestCell {}
static PORT_LOCK: AtomicBool = AtomicBool::new(false);
static MINIPORT: MiniportCell = MiniportCell(UnsafeCell::new(None));
static REQUESTS: RequestCell = RequestCell(UnsafeCell::new(AtaRequestTable::new()));\nstatic PORT_SERVICES: AtaPortServices = AtaPortServices { size: core::mem::size_of::<AtaPortServices>() as u32, version: ATA_PORT_INTERFACE_VERSION, register_miniport: Some(AtaPortRegisterMiniport), allocate_common_buffer: None, free_common_buffer: None, connect_interrupt: None, disconnect_interrupt: None };\nstatic PORT_INTERFACE_NAME: &[u8] = b"CognacOS.AtaPort.Services.v1";

struct PortGuard;
impl PortGuard {
    fn acquire() -> Self {
        while PORT_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for PortGuard {
    fn drop(&mut self) { PORT_LOCK.store(false, Ordering::Release); }
}

fn miniport_snapshot() -> Option<AtaPortMiniportInterface> {
    let _guard = PortGuard::acquire();
    unsafe { *MINIPORT.0.get() }
}

/// Register one active controller for the initial QEMU PIO milestone.
/// A later interface revision will replace this bounded singleton with
/// per-controller contexts and asynchronous queue/rundown services.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn AtaPortRegisterMiniport(interface: *const AtaPortMiniportInterface) -> i32 {
    if interface.is_null() { return STATUS_INVALID_PARAMETER; }
    let candidate = unsafe { core::ptr::read_unaligned(interface) };
    if candidate.size as usize != core::mem::size_of::<AtaPortMiniportInterface>()
        || candidate.version != ATA_PORT_INTERFACE_VERSION
        || candidate.submit.is_none()
    {
        return STATUS_REVISION_MISMATCH;
    }
    if let Some(initialize) = candidate.initialize {
        let status = unsafe { initialize(candidate.miniport_context, core::ptr::null()) };
        if status != STATUS_SUCCESS { return status; }
    }
    if let Some(start) = candidate.start {
        let status = unsafe { start(candidate.miniport_context) };
        if status != STATUS_SUCCESS {
            if let Some(stop) = candidate.stop { let _ = unsafe { stop(candidate.miniport_context) }; }
            return status;
        }
    }
    let _guard = PortGuard::acquire();
    let slot = unsafe { &mut *MINIPORT.0.get() };
    if slot.is_some() {
        if let Some(stop) = candidate.stop { let _ = unsafe { stop(candidate.miniport_context) }; }
        return STATUS_DEVICE_BUSY;
    }
    *slot = Some(candidate);
    STATUS_SUCCESS
}

/// Controller teardown must quiesce requests before unregistering the miniport.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn AtaPortUnregisterMiniport(context: *mut c_void) -> i32 {
    let candidate = {
        let _guard = PortGuard::acquire();
        let slot = unsafe { &mut *MINIPORT.0.get() };
        match *slot {
            Some(current) if current.miniport_context == context => {
                if unsafe { &*REQUESTS.0.get() }.outstanding() != 0 { return STATUS_DEVICE_BUSY; }
                *slot = None;
                Some(current)
            }
            Some(_) => return STATUS_ACCESS_DENIED,
            None => return STATUS_DEVICE_NOT_READY,
        }
    };
    if let Some(current) = candidate {
        if let Some(stop) = current.stop {
            let status = unsafe { stop(current.miniport_context) };
            if status != STATUS_SUCCESS { return status; }
        }
        if let Some(unload) = current.unload { unsafe { unload(current.miniport_context) }; }
    }
    STATUS_SUCCESS
}

fn current_stack(irp: *mut IRP) -> *mut IO_STACK_LOCATION {
    if irp.is_null() { core::ptr::null_mut() }
    else { unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION } }
}

unsafe fn complete(irp: *mut IRP, status: i32, information: usize) -> i32 {
    unsafe {
        (*irp).io_status.status.status = status;
        (*irp).io_status.information = information;
        IoCompleteRequest(irp, IO_NO_INCREMENT);
    }
    status
}

/// This initial port path accepts only buffered sector-aligned I/O. It reserves
/// request ownership before calling the miniport and releases it on every path.
/// cancel/timeout contract.
unsafe extern "win64" fn dispatch(_device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = current_stack(irp);
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let major = unsafe { (*stack).major_function as usize };
    if major == IRP_MJ_CREATE || major == IRP_MJ_CLOSE {
        return unsafe { complete(irp, STATUS_SUCCESS, 0) };
    }
    let operation = match major {
        IRP_MJ_READ => AtaPortOperation::Read,
        IRP_MJ_WRITE => AtaPortOperation::Write,
        IRP_MJ_FLUSH_BUFFERS => AtaPortOperation::Flush,
        _ => return unsafe { complete(irp, STATUS_INVALID_DEVICE_REQUEST, 0) },
    };
    let Some(miniport) = miniport_snapshot() else {
        return unsafe { complete(irp, STATUS_DEVICE_NOT_READY, 0) };
    };

    let mut request = AtaPortRequest {
        size: core::mem::size_of::<AtaPortRequest>() as u32,
        operation,
        request_id: 0,
        lba: 0,
        sector_count: 0,
        flags: 0,
        system_buffer: core::ptr::null_mut(),
        sg_list: core::ptr::null(),
        sg_count: 0,
        timeout_ms: 30_000,
        packet_cdb: core::ptr::null(),
        packet_cdb_length: 0,
        reserved: [0; 7],
    };
    let expected = if operation == AtaPortOperation::Flush {
        0
    } else {
        let length = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>()) };
        let byte_offset = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(8).cast::<i64>()) };
        let buffer = unsafe { (*irp).associated_irp[0] as *mut u8 };
        let flags = unsafe { (*irp).flags };
        if flags & IRP_BUFFERED_IO == 0 || buffer.is_null() || byte_offset < 0
            || byte_offset as u64 % SECTOR_SIZE as u64 != 0
            || length == 0 || length % SECTOR_SIZE != 0
            || length / SECTOR_SIZE > MAX_PIO_SECTORS
        {
            return unsafe { complete(irp, STATUS_INVALID_PARAMETER, 0) };
        }
        request.lba = byte_offset as u64 / SECTOR_SIZE as u64;
        request.sector_count = length / SECTOR_SIZE;
        request.system_buffer = buffer;
        length
    };

    if let Err(status) = validate_request(&request) {
        return unsafe { complete(irp, status, 0) };
    }
    let request_id = {
        let _guard = PortGuard::acquire();
        let table = unsafe { &mut *REQUESTS.0.get() };
        let id = match table.reserve(irp.cast(), expected) {
            Ok(id) => id,
            Err(status) => return unsafe { complete(irp, status, 0) },
        };
        if let Err(status) = table.mark_submitted(id) {
            let _ = table.abort(id);
            return unsafe { complete(irp, status, 0) };
        }
        id
    };
    request.request_id = request_id;
    let mut transferred = 0u32;
    let submit_status = match miniport.submit {
        Some(submit) => unsafe { submit(miniport.miniport_context, &request, &mut transferred) },
        None => STATUS_DEVICE_NOT_READY,
    };

    let final_status = if submit_status != STATUS_SUCCESS {
        submit_status
    } else if transferred != expected {
        STATUS_DEVICE_DATA_ERROR
    } else {
        STATUS_SUCCESS
    };
    {
        let _guard = PortGuard::acquire();
        let table = unsafe { &mut *REQUESTS.0.get() };
        if table.begin_completion(request_id, transferred).is_ok() {
            let _ = table.release(request_id);
        } else {
            let _ = table.abort(request_id);
        }
    }
    unsafe { complete(irp, final_status, if final_status == STATUS_SUCCESS { transferred as usize } else { 0 }) }
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

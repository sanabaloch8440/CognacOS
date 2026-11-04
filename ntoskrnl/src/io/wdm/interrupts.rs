//! WDM interrupt connection and teardown exports.
//!
//! @partial implementation
//! The entry-point calling conventions and parameter layouts match WDM x64.
//! Current runtime support is one non-shared DIRQL interrupt per device
//! connection. Message-signaled interrupt message tables, shared ISR fanout,
//! caller-supplied spin-lock synchronization, and passive-level ISRs remain
//! unavailable and are rejected rather than reported as successful.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

const INTERRUPT_SIGNATURE: u64 = 0x434f_474e_494e_5431;
const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BB_u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009A_u32 as i32;

pub type InterruptServiceRoutine =
    unsafe extern "win64" fn(*mut c_void, *mut c_void) -> u8;
pub type InterruptMessageServiceRoutine =
    unsafe extern "win64" fn(*mut c_void, *mut c_void, u32) -> u8;

#[repr(C)]
pub struct InterruptObject {
    signature: AtomicU64,
    vector: AtomicU32,
    irql: AtomicU8,
    _reserved: [u8; 3],
    active: AtomicBool,
    owns_vector: AtomicBool,
    routine: AtomicUsize,
    context: AtomicUsize,
}
unsafe impl Sync for InterruptObject {}

impl InterruptObject {
    const EMPTY: Self = Self {
        signature: AtomicU64::new(0),
        vector: AtomicU32::new(0),
        irql: AtomicU8::new(0),
        _reserved: [0; 3],
        active: AtomicBool::new(false),
        owns_vector: AtomicBool::new(false),
        routine: AtomicUsize::new(0),
        context: AtomicUsize::new(0),
    };
}

static INTERRUPT_OBJECTS: [InterruptObject; 256] =
    [const { InterruptObject::EMPTY }; 256];

/// @partial implementation
/// ABI matches KSERVICE_ROUTINE. The first argument is the opaque interrupt
/// object returned to the driver; the second is its ServiceContext.
unsafe extern "win64" fn io_interrupt_dispatch(
    _vector: u8,
    _frame: *mut c_void,
    context: *mut c_void,
) -> bool {
    if context.is_null() {
        return false;
    }
    let object = unsafe { &*context.cast::<InterruptObject>() };
    if object.signature.load(Ordering::Acquire) != INTERRUPT_SIGNATURE
        || !object.active.load(Ordering::Acquire)
    {
        return false;
    }
    let raw = object.routine.load(Ordering::Acquire);
    if raw == 0 {
        return false;
    }
    let routine: InterruptServiceRoutine = unsafe { core::mem::transmute(raw) };
    let service_context = object.context.load(Ordering::Acquire) as *mut c_void;
    unsafe { routine(context, service_context) != 0 }
}

/// @partial implementation
/// Windows 11 x64 IoConnectInterrupt ABI. Exact translated vectors are claimed;
/// this function never substitutes a newly allocated vector. Shared-vector
/// fanout, supplied spin-lock synchronization, and passive-level ISR scheduling
/// are not implemented and are rejected.
#[unsafe(export_name = "IoConnectInterrupt")]
pub unsafe extern "win64" fn io_connect_interrupt(
    interrupt_object_out: *mut *mut c_void,
    service_routine: Option<InterruptServiceRoutine>,
    service_context: *mut c_void,
    spin_lock: *mut c_void,
    vector: u32,
    irql: u8,
    synchronize_irql: u8,
    interrupt_mode: u32,
    share_vector: u8,
    processor_enable_mask: u64,
    floating_save: u8,
) -> i32 {
    if interrupt_object_out.is_null()
        || service_routine.is_none()
        || !(0x20..=0xdf).contains(&vector)
        || synchronize_irql < irql
        || irql == 0
        || interrupt_mode > 1
        || processor_enable_mask == 0
    {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { interrupt_object_out.write(core::ptr::null_mut()) };

    // x64 does not need the legacy x87 context-save path. We still reject the
    // request because this implementation has no alternate FPU-save contract.
    if !spin_lock.is_null() || share_vector != 0 || floating_save != 0 {
        return STATUS_NOT_SUPPORTED;
    }

    let owns_vector = match crate::hal::claim_device_interrupt_vector(vector as u8) {
        Ok(owned) => owned,
        Err(_) => return STATUS_INSUFFICIENT_RESOURCES,
    };

    let slot = &INTERRUPT_OBJECTS[vector as usize];
    if slot
        .active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        if owns_vector {
            let _ = crate::hal::release_device_interrupt_vector(vector as u8);
        }
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    slot.signature.store(INTERRUPT_SIGNATURE, Ordering::Release);
    slot.vector.store(vector, Ordering::Relaxed);
    slot.irql.store(irql, Ordering::Relaxed);
    slot.owns_vector.store(owns_vector, Ordering::Relaxed);
    slot.context.store(service_context as usize, Ordering::Relaxed);
    slot.routine.store(service_routine.unwrap() as usize, Ordering::Release);

    let object = (slot as *const InterruptObject).cast_mut().cast::<c_void>();
    if crate::hal::register_device_interrupt_handler(
        vector as u8,
        io_interrupt_dispatch,
        object,
    )
    .is_err()
    {
        slot.active.store(false, Ordering::Release);
        slot.routine.store(0, Ordering::Release);
        slot.context.store(0, Ordering::Release);
        slot.signature.store(0, Ordering::Release);
        slot.owns_vector.store(false, Ordering::Release);
        if owns_vector {
            let _ = crate::hal::release_device_interrupt_vector(vector as u8);
        }
        return STATUS_NOT_SUPPORTED;
    }

    unsafe { interrupt_object_out.write(object) };
    STATUS_SUCCESS
}

/// @partial implementation
/// Windows 11 x64 IoDisconnectInterrupt ABI. Detaches and drains the callback
/// before releasing a vector owned by this connection.
#[unsafe(export_name = "IoDisconnectInterrupt")]
pub unsafe extern "win64" fn io_disconnect_interrupt(interrupt_object: *mut c_void) {
    if interrupt_object.is_null() {
        return;
    }
    let object = interrupt_object.cast::<InterruptObject>();
    let Some((vector, slot)) = INTERRUPT_OBJECTS
        .iter()
        .enumerate()
        .find(|(_, candidate)| core::ptr::eq(*candidate as *const InterruptObject, object))
    else {
        return;
    };
    if slot.signature.load(Ordering::Acquire) != INTERRUPT_SIGNATURE
        || !slot.active.swap(false, Ordering::AcqRel)
    {
        return;
    }

    // Mask the bound source before detaching the ISR; the device driver must
    // not leave an unhandled level-triggered source asserted during teardown.
    let _ = unsafe { crate::hal::set_device_interrupt_vector_enabled(vector as u8, false) };
    let _ = crate::hal::unregister_device_interrupt_handler(vector as u8);
    if slot.owns_vector.swap(false, Ordering::AcqRel) {
        let _ = crate::hal::release_device_interrupt_vector(vector as u8);
    }
    slot.routine.store(0, Ordering::Release);
    slot.context.store(0, Ordering::Release);
    slot.signature.store(0, Ordering::Release);
}

const CONNECT_FULLY_SPECIFIED: u32 = 0;
const CONNECT_LINE_BASED: u32 = 1;
const CONNECT_MESSAGE_BASED: u32 = 2;
const CONNECT_FULLY_SPECIFIED_GROUP: u32 = 3;

/// WDK x64 IO_CONNECT_INTERRUPT_FULLY_SPECIFIED_PARAMETERS.
#[repr(C)]
#[derive(Clone, Copy)]
struct IoConnectInterruptFullySpecified {
    physical_device_object: *mut c_void,
    interrupt_object: *mut *mut c_void,
    service_routine: Option<InterruptServiceRoutine>,
    service_context: *mut c_void,
    spin_lock: *mut c_void,
    synchronize_irql: u8,
    floating_save: u8,
    share_vector: u8,
    _padding: u8,
    vector: u32,
    irql: u8,
    _padding2: [u8; 3],
    interrupt_mode: u32,
    processor_enable_mask: u64,
    group: u16,
    _padding3: [u8; 6],
}

/// WDK x64 IO_CONNECT_INTERRUPT_LINE_BASED_PARAMETERS.
#[repr(C)]
#[derive(Clone, Copy)]
struct IoConnectInterruptLineBased {
    physical_device_object: *mut c_void,
    interrupt_object: *mut *mut c_void,
    service_routine: Option<InterruptServiceRoutine>,
    service_context: *mut c_void,
    spin_lock: *mut c_void,
    synchronize_irql: u8,
    floating_save: u8,
    _padding: [u8; 6],
}

/// WDK x64 IO_CONNECT_INTERRUPT_MESSAGE_BASED_PARAMETERS.
/// Only the fallback-service path is currently consumed.
#[repr(C)]
#[derive(Clone, Copy)]
struct IoConnectInterruptMessageBased {
    physical_device_object: *mut c_void,
    connection_context: *mut *mut c_void,
    message_service_routine: Option<InterruptMessageServiceRoutine>,
    service_context: *mut c_void,
    spin_lock: *mut c_void,
    synchronize_irql: u8,
    floating_save: u8,
    _padding: [u8; 6],
    fallback_service_routine: Option<InterruptServiceRoutine>,
}

#[repr(C)]
union IoConnectInterruptUnion {
    fully_specified: IoConnectInterruptFullySpecified,
    line_based: IoConnectInterruptLineBased,
    message_based: IoConnectInterruptMessageBased,
}

#[repr(C)]
struct IoConnectInterruptParameters {
    version: u32,
    parameters: IoConnectInterruptUnion,
}

// WDK x64: union size 72, aligned at offset 8, total parameter size 80.
const _: [(); 72] = [(); core::mem::size_of::<IoConnectInterruptFullySpecified>()];
const _: [(); 48] = [(); core::mem::size_of::<IoConnectInterruptLineBased>()];
const _: [(); 56] = [(); core::mem::size_of::<IoConnectInterruptMessageBased>()];
const _: [(); 72] = [(); core::mem::size_of::<IoConnectInterruptUnion>()];
const _: [(); 80] = [(); core::mem::size_of::<IoConnectInterruptParameters>()];

unsafe fn connect_line_based(
    physical_device_object: *mut c_void,
    interrupt_object_out: *mut *mut c_void,
    service_routine: Option<InterruptServiceRoutine>,
    service_context: *mut c_void,
    spin_lock: *mut c_void,
    synchronize_irql: u8,
    floating_save: u8,
) -> i32 {
    if physical_device_object.is_null()
        || interrupt_object_out.is_null()
        || service_routine.is_none()
    {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { interrupt_object_out.write(core::ptr::null_mut()) };
    if !spin_lock.is_null() || floating_save != 0 {
        return STATUS_NOT_SUPPORTED;
    }
    let Some(resource) =
        crate::io::wdm::io_query_pnp_line_interrupt_resource(physical_device_object)
    else {
        return STATUS_NOT_SUPPORTED;
    };
    let sync_irql = synchronize_irql.max(resource.irql);
    unsafe {
        io_connect_interrupt(
            interrupt_object_out,
            service_routine,
            service_context,
            core::ptr::null_mut(),
            resource.vector,
            resource.irql,
            sync_irql,
            resource.interrupt_mode,
            0,
            resource.affinity,
            0,
        )
    }
}

/// @partial implementation
/// CONNECT_FULLY_SPECIFIED and the single-exclusive-line CONNECT_LINE_BASED
/// form are supported. CONNECT_MESSAGE_BASED is supported only when the
/// device has no MSI resource and supplies a line-based fallback ISR.
#[unsafe(export_name = "IoConnectInterruptEx")]
pub unsafe extern "win64" fn io_connect_interrupt_ex(
    parameters: *mut IoConnectInterruptParameters,
) -> i32 {
    if parameters.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let params = unsafe { &mut *parameters };
    match params.version {
        CONNECT_FULLY_SPECIFIED | CONNECT_FULLY_SPECIFIED_GROUP => {
            let full = unsafe { &mut params.parameters.fully_specified };
            if full.physical_device_object.is_null()
                || full.interrupt_object.is_null()
                || full.service_routine.is_none()
            {
                return STATUS_INVALID_PARAMETER;
            }
            if params.version == CONNECT_FULLY_SPECIFIED_GROUP && full.group != 0 {
                return STATUS_NOT_SUPPORTED;
            }
            unsafe { full.interrupt_object.write(core::ptr::null_mut()) };
            unsafe {
                io_connect_interrupt(
                    full.interrupt_object,
                    full.service_routine,
                    full.service_context,
                    full.spin_lock,
                    full.vector,
                    full.irql,
                    full.synchronize_irql,
                    full.interrupt_mode,
                    full.share_vector,
                    full.processor_enable_mask,
                    full.floating_save,
                )
            }
        }
        CONNECT_LINE_BASED => {
            let line = unsafe { &mut params.parameters.line_based };
            unsafe {
                connect_line_based(
                    line.physical_device_object,
                    line.interrupt_object,
                    line.service_routine,
                    line.service_context,
                    line.spin_lock,
                    line.synchronize_irql,
                    line.floating_save,
                )
            }
        }
        CONNECT_MESSAGE_BASED => {
            let message = unsafe { &mut params.parameters.message_based };
            if message.connection_context.is_null()
                || message.message_service_routine.is_none()
                || message.fallback_service_routine.is_none()
            {
                return STATUS_NOT_SUPPORTED;
            }
            let status = unsafe {
                connect_line_based(
                    message.physical_device_object,
                    message.connection_context,
                    message.fallback_service_routine,
                    message.service_context,
                    message.spin_lock,
                    message.synchronize_irql,
                    message.floating_save,
                )
            };
            if status == STATUS_SUCCESS {
                // WDK changes Version to CONNECT_LINE_BASED when the
                // message-based request falls back to the line-based ISR.
                params.version = CONNECT_LINE_BASED;
            }
            status
        }
        _ => STATUS_INVALID_PARAMETER,
    }
}

/// WDK IO_DISCONNECT_INTERRUPT_PARAMETERS. The union is represented by its
/// generic pointer member; repr(C) inserts the required x64 alignment padding.
#[repr(C)]
struct IoDisconnectInterruptParameters {
    version: u32,
    _union_alignment: u32,
    connection_context: *mut c_void,
}
const _: [(); 16] = [(); core::mem::size_of::<IoDisconnectInterruptParameters>()];

/// @partial implementation
/// Accepts disconnect contexts returned by the implemented fully-specified
/// and line-based paths. Message interrupt-table teardown is not implemented.
#[unsafe(export_name = "IoDisconnectInterruptEx")]
pub unsafe extern "win64" fn io_disconnect_interrupt_ex(
    parameters: *mut IoDisconnectInterruptParameters,
) {
    if parameters.is_null() {
        return;
    }
    let params = unsafe { &mut *parameters };
    if !matches!(params.version, CONNECT_FULLY_SPECIFIED | CONNECT_LINE_BASED | CONNECT_FULLY_SPECIFIED_GROUP)
        || params.connection_context.is_null()
    {
        return;
    }
    unsafe { io_disconnect_interrupt(params.connection_context) };
    params.connection_context = core::ptr::null_mut();
}

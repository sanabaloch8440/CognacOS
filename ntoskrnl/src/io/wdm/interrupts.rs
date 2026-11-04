//! WDM interrupt connection and teardown exports.
//!
//! @partial implementation
//! The public DDI entry points use the Windows x64 calling convention. The
//! dispatcher currently supports one non-shared, DIRQL ISR per vector. The
//! PnP resource arbiter must supply an already translated system vector.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

const INTERRUPT_SIGNATURE: u64 = 0x434f_474e_494e_5431;
const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BB_u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009A_u32 as i32;

pub type InterruptServiceRoutine =
    unsafe extern "win64" fn(*mut c_void, *mut c_void) -> u8;

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
/// ABI matches KSERVICE_ROUTINE. The interrupt object passed to the driver's
/// ISR is the same opaque object returned by IoConnectInterrupt[Ex].
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
/// Windows 11 x64 IoConnectInterrupt ABI. The exact assigned vector is claimed;
/// this routine never substitutes a newly allocated vector. Shared-vector
/// fanout, caller-provided spin-lock synchronization, and passive-level ISR
/// scheduling are not yet implemented and are rejected.
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
        || irql == 0
        || synchronize_irql > irql
        || interrupt_mode > 1
        || processor_enable_mask == 0
    {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { interrupt_object_out.write(core::ptr::null_mut()) };

    // Windows x64 does not use FloatingSave. Reject non-NULL spin locks and
    // shared vectors until the dispatcher implements their synchronization.
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
/// Windows 11 x64 IoDisconnectInterrupt ABI. This detaches and drains the
/// callback before releasing a vector owned by this connection. It does not
/// release a vector reserved by the PnP/HAL resource owner.
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

    // unregister waits for in-flight dispatches; the route remains masked by
    // the PnP owner and is never implicitly enabled by ISR registration.
    let _ = crate::hal::unregister_device_interrupt_handler(vector as u8);
    if slot.owns_vector.swap(false, Ordering::AcqRel) {
        let _ = crate::hal::release_device_interrupt_vector(vector as u8);
    }
    slot.routine.store(0, Ordering::Release);
    slot.context.store(0, Ordering::Release);
    slot.signature.store(0, Ordering::Release);
}

/// WDK IO_CONNECT_INTERRUPT_PARAMETERS version values.
const CONNECT_FULLY_SPECIFIED: u32 = 0;
const CONNECT_LINE_BASED: u32 = 1;
const CONNECT_MESSAGE_BASED: u32 = 2;
const CONNECT_FULLY_SPECIFIED_GROUP: u32 = 3;

/// WDK x64 IO_CONNECT_INTERRUPT_FULLY_SPECIFIED_PARAMETERS.
/// Field order, natural alignment, and widths match wdm.h.
#[repr(C)]
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

#[repr(C)]
struct IoConnectInterruptParameters {
    version: u32,
    _union_alignment: u32,
    fully_specified: IoConnectInterruptFullySpecified,
}

// WDK x64: fully-specified parameters are 72 bytes, union starts at offset 8.
const _: [(); 72] = [(); core::mem::size_of::<IoConnectInterruptFullySpecified>()];
const _: [(); 80] = [(); core::mem::size_of::<IoConnectInterruptParameters>()];

/// @partial implementation
/// Supports CONNECT_FULLY_SPECIFIED for a translated resource on processor
/// group zero. Line-based discovery and MSI message-table publication require
/// the PnP resource/interface work that is still outstanding.
#[unsafe(export_name = "IoConnectInterruptEx")]
pub unsafe extern "win64" fn io_connect_interrupt_ex(
    parameters: *mut IoConnectInterruptParameters,
) -> i32 {
    if parameters.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let params = unsafe { &mut *parameters };
    match params.version {
        CONNECT_FULLY_SPECIFIED | CONNECT_FULLY_SPECIFIED_GROUP => {}
        CONNECT_LINE_BASED | CONNECT_MESSAGE_BASED => return STATUS_NOT_SUPPORTED,
        _ => return STATUS_INVALID_PARAMETER,
    }

    let full = &mut params.fully_specified;
    if full.physical_device_object.is_null()
        || full.interrupt_object.is_null()
        || full.service_routine.is_none()
    {
        return STATUS_INVALID_PARAMETER;
    }
    // CONNECT_FULLY_SPECIFIED always targets group zero; the group field is
    // ignored for that version. The explicit group variant is accepted only
    // for group zero because this kernel currently exposes one processor group.
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

/// WDK IO_DISCONNECT_INTERRUPT_PARAMETERS uses a union of connection-context
/// pointers. Its x64 union is naturally aligned at byte offset 8.
#[repr(C)]
struct IoDisconnectInterruptParameters {
    version: u32,
    _union_alignment: u32,
    connection_context: *mut c_void,
}
const _: [(); 16] = [(); core::mem::size_of::<IoDisconnectInterruptParameters>()];

/// @partial implementation
/// Windows x64 disconnect ABI; only the generic KINTERRUPT connection context
/// produced by the supported fully-specified connection is accepted.
#[unsafe(export_name = "IoDisconnectInterruptEx")]
pub unsafe extern "win64" fn io_disconnect_interrupt_ex(
    parameters: *mut IoDisconnectInterruptParameters,
) {
    if parameters.is_null() {
        return;
    }
    let params = unsafe { &mut *parameters };
    if params.version != CONNECT_FULLY_SPECIFIED || params.connection_context.is_null() {
        return;
    }
    unsafe { io_disconnect_interrupt(params.connection_context) };
    params.connection_context = core::ptr::null_mut();
}

// WDM interrupt connection and teardown exports.
//
// @partial implementation
// Windows x64 entry-point ABIs and parameter layouts are implemented. The
// dispatcher supports bounded shared-vector ISR fanout (eight registrations
// per vector) and line-based PnP resources. Full MSI message-table delivery,
// caller-supplied spin-lock semantics, and passive-level ISRs remain
// unsupported and fail closed.

use core::cell::UnsafeCell;

const INTERRUPT_SIGNATURE: u64 = 0x434f_474e_494e_5431;
/// @unimplemented
/// CONNECT_MESSAGE_BASED MSI vector-table allocation, per-message ISR routing,
/// and IO_INTERRUPT_MESSAGE_INFO teardown are not yet implemented.
const MAX_SHARED_ISR_PER_VECTOR: usize = 8;

pub type InterruptServiceRoutine =
    unsafe extern "win64" fn(*mut c_void, *mut c_void) -> u8;
pub type InterruptMessageServiceRoutine =
    unsafe extern "win64" fn(*mut c_void, *mut c_void, u32) -> u8;

/// Opaque KINTERRUPT-compatible connection record. All accesses occur while
/// holding the corresponding vector's IRQ-save lock.
#[repr(C)]
pub struct InterruptObject {
    signature: u64,
    vector: u32,
    irql: u8,
    synchronize_irql: u8,
    interrupt_mode: u32,
    share_vector: bool,
    active: bool,
    routine: Option<InterruptServiceRoutine>,
    context: *mut c_void,
    processor_enable_mask: u64,
}
impl InterruptObject {
    const EMPTY: Self = Self {
        signature: 0,
        vector: 0,
        irql: 0,
        synchronize_irql: 0,
        interrupt_mode: 0,
        share_vector: false,
        active: false,
        routine: None,
        context: core::ptr::null_mut(),
        processor_enable_mask: 0,
    };
}

struct InterruptObjectCell(UnsafeCell<InterruptObject>);
unsafe impl Sync for InterruptObjectCell {}
impl InterruptObjectCell {
    const EMPTY: Self = Self(UnsafeCell::new(InterruptObject::EMPTY));
}

struct InterruptVectorData {
    vector: u32,
    registered: bool,
    tearing_down: bool,
    teardown_busy: bool,
    owns_vector: bool,
    active_count: usize,
    irql: u8,
    synchronize_irql: u8,
    interrupt_mode: u32,
    processor_enable_mask: u64,
}
impl InterruptVectorData {
    const EMPTY: Self = Self {
        vector: 0,
        registered: false,
        tearing_down: false,
        teardown_busy: false,
        owns_vector: false,
        active_count: 0,
        irql: 0,
        synchronize_irql: 0,
        interrupt_mode: 0,
        processor_enable_mask: 0,
    };
}

static INTERRUPT_VECTOR_STATES: [crate::hal::IrqSpinLock<InterruptVectorData>; 256] =
    [const { crate::hal::IrqSpinLock::new(InterruptVectorData::EMPTY) }; 256];
static INTERRUPT_OBJECTS: [InterruptObjectCell; 256 * MAX_SHARED_ISR_PER_VECTOR] =
    [const { InterruptObjectCell::EMPTY }; 256 * MAX_SHARED_ISR_PER_VECTOR];

fn interrupt_object_index(vector: usize, slot: usize) -> usize {
    vector * MAX_SHARED_ISR_PER_VECTOR + slot
}

/// @partial implementation
/// ABI matches KSERVICE_ROUTINE. All registered ISRs on a shared vector are
/// invoked under the vector lock; TRUE is returned if any ISR claims the IRQ.
unsafe extern "win64" fn io_interrupt_dispatch(
    _vector: u8,
    _frame: *mut c_void,
    context: *mut c_void,
) -> bool {
    if context.is_null() {
        return false;
    }
    let state_lock = unsafe {
        &*context.cast::<crate::hal::IrqSpinLock<InterruptVectorData>>()
    };
    let state = state_lock.lock();
    if !state.registered || state.tearing_down || state.vector > 0xff {
        return false;
    }

    let start = interrupt_object_index(state.vector as usize, 0);
    let mut handled = false;
    for cell in INTERRUPT_OBJECTS[start..start + MAX_SHARED_ISR_PER_VECTOR].iter() {
        let object = unsafe { &*cell.0.get() };
        if object.signature != INTERRUPT_SIGNATURE || !object.active {
            continue;
        }
        if let Some(routine) = object.routine {
            let object_pointer = (object as *const InterruptObject).cast_mut().cast::<c_void>();
            if unsafe { routine(object_pointer, object.context) } != 0 {
                handled = true;
            }
        }
    }
    handled
}

/// @partial implementation
/// Windows 11 x64 IoConnectInterrupt ABI. Supports exclusive vectors and
/// shared line-based or message-signaled vectors when assigned properties match. One HAL callback
/// fans out to up to eight registered ISRs per vector. Caller-supplied spin
/// locks, floating-save requests, and passive-level ISRs remain unsupported.
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
        // @unimplemented: PASSIVE_LEVEL ISR scheduling requires the kernel's
        // passive interrupt worker/event path, which is not wired yet.
        || irql == 0
        || interrupt_mode > 1
        || share_vector > 1
        || processor_enable_mask == 0
        
    {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { interrupt_object_out.write(core::ptr::null_mut()) };
    // @unimplemented: caller-owned KSPIN_LOCK integration and FloatingSave
    // state preservation remain unsupported on this x64 dispatcher.
    if !spin_lock.is_null() || floating_save != 0 {
        return STATUS_NOT_SUPPORTED;
    }

    let vector_index = vector as usize;
    let state_lock = &INTERRUPT_VECTOR_STATES[vector_index];
    let mut state = state_lock.lock();
    if state.tearing_down {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    let slot = (0..MAX_SHARED_ISR_PER_VECTOR).find(|slot| {
        let cell = &INTERRUPT_OBJECTS[interrupt_object_index(vector_index, *slot)];
        unsafe { (*cell.0.get()).signature == 0 }
    });
    let Some(slot) = slot else {
        return STATUS_INSUFFICIENT_RESOURCES;
    };

    let first_connection = state.active_count == 0;
    if !first_connection {
        if share_vector == 0
            || !state.registered
            || state.irql != irql
            || state.interrupt_mode != interrupt_mode
            || state.processor_enable_mask != processor_enable_mask
        {
            return STATUS_INVALID_PARAMETER;
        }
        let start = interrupt_object_index(vector_index, 0);
        if INTERRUPT_OBJECTS[start..start + MAX_SHARED_ISR_PER_VECTOR]
            .iter()
            .any(|cell| unsafe {
                let object = &*cell.0.get();
                object.active && !object.share_vector
            })
        {
            return STATUS_INVALID_PARAMETER;
        }
    }

    let owns_vector = if first_connection {
        match crate::hal::claim_device_interrupt_vector(vector as u8) {
            Ok(owned) => owned,
            Err(_) => return STATUS_INSUFFICIENT_RESOURCES,
        }
    } else {
        false
    };

    let object_index = interrupt_object_index(vector_index, slot);
    let object = unsafe { &mut *INTERRUPT_OBJECTS[object_index].0.get() };
    *object = InterruptObject {
        signature: INTERRUPT_SIGNATURE,
        vector,
        irql,
        synchronize_irql,
        interrupt_mode,
        share_vector: share_vector != 0,
        active: true,
        routine: service_routine,
        context: service_context,
        processor_enable_mask,
    };

    if first_connection {
        state.vector = vector;
        state.owns_vector = owns_vector;
        state.irql = irql;
        state.synchronize_irql = synchronize_irql;
        state.interrupt_mode = interrupt_mode;
        state.processor_enable_mask = processor_enable_mask;
        state.active_count = 1;
        // Publish complete state before attaching the callback. A concurrent
        // interrupt waits for this lock and sees the fully initialized ISR.
        state.registered = true;
        let state_context = (state_lock as *const crate::hal::IrqSpinLock<InterruptVectorData>)
            .cast_mut()
            .cast::<c_void>();
        if crate::hal::register_device_interrupt_handler(
            vector as u8,
            io_interrupt_dispatch,
            state_context,
        )
        .is_err()
        {
            state.registered = false;
            state.active_count = 0;
            state.owns_vector = false;
            *object = InterruptObject::EMPTY;
            if owns_vector {
                let _ = crate::hal::release_device_interrupt_vector(vector as u8);
            }
            return STATUS_NOT_SUPPORTED;
        }
    } else {
        state.active_count += 1;
        state.synchronize_irql = state.synchronize_irql.max(synchronize_irql);
    }

    let object_pointer = (object as *const InterruptObject).cast_mut().cast::<c_void>();
    unsafe { interrupt_object_out.write(object_pointer) };
    STATUS_SUCCESS
}

/// @partial implementation
/// Windows 11 x64 IoDisconnectInterrupt ABI. Non-final shared registrations
/// are removed without masking the common line. The last registration masks
/// the route, drains the HAL callback, and releases only a locally owned vector.
#[unsafe(export_name = "IoDisconnectInterrupt")]
pub unsafe extern "win64" fn io_disconnect_interrupt(interrupt_object: *mut c_void) {
    if interrupt_object.is_null() {
        return;
    }
    let target = interrupt_object.cast::<InterruptObject>();
    let Some(index) = INTERRUPT_OBJECTS
        .iter()
        .position(|cell| cell.0.get() == target)
    else {
        return;
    };
    let vector = index / MAX_SHARED_ISR_PER_VECTOR;
    let state_lock = &INTERRUPT_VECTOR_STATES[vector];

    {
        let mut state = state_lock.lock();
        let object = unsafe { &mut *INTERRUPT_OBJECTS[index].0.get() };
        if object.signature != INTERRUPT_SIGNATURE {
            return;
        }
        if state.tearing_down {
            if state.teardown_busy {
                return;
            }
            state.teardown_busy = true;
        } else {
            if !object.active {
                return;
            }
            object.active = false;
            state.active_count = state.active_count.saturating_sub(1);
            if state.active_count != 0 {
                *object = InterruptObject::EMPTY;
                return;
            }
            state.tearing_down = true;
            state.teardown_busy = true;
        }
    }

    // No new ISR callback can pass the teardown gate. Mask the physical route
    // and detach the HAL dispatcher; unregister waits for any callback already
    // in flight before the KINTERRUPT storage can be reused.
    let _ = unsafe { crate::hal::set_device_interrupt_vector_enabled(vector as u8, false) };
    if crate::hal::unregister_device_interrupt_handler(vector as u8).is_err() {
        state_lock.lock().teardown_busy = false;
        return;
    }

    let owns_vector = {
        let mut state = state_lock.lock();
        state.registered = false;
        state.owns_vector
    };
    if owns_vector && crate::hal::release_device_interrupt_vector(vector as u8).is_err() {
        state_lock.lock().teardown_busy = false;
        return;
    }

    let mut state = state_lock.lock();
    let object = unsafe { &mut *INTERRUPT_OBJECTS[index].0.get() };
    *object = InterruptObject::EMPTY;
    *state = InterruptVectorData::EMPTY;
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
        crate::io::wdm::io_query_pnp_line_interrupt_resource(physical_device_object.cast::<crate::io::wdm::DeviceObject>())
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
            resource.share_vector,
            resource.affinity,
            0,
        )
    }
}

/// @partial implementation
/// CONNECT_FULLY_SPECIFIED, CONNECT_LINE_BASED, and the line-based fallback
/// from CONNECT_MESSAGE_BASED are supported. Full MSI message-table delivery
/// remains unimplemented.
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
            let group = if params.version == CONNECT_FULLY_SPECIFIED_GROUP { full.group } else { 0 };
            if !crate::io::wdm::io_validate_pnp_interrupt_resource(
                full.physical_device_object.cast::<crate::io::wdm::DeviceObject>(),
                full.vector,
                full.irql,
                full.interrupt_mode,
                full.share_vector,
                full.processor_enable_mask,
                group,
            ) {
                return STATUS_INVALID_PARAMETER;
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
/// Accepts connection contexts returned by fully-specified, line-based, and
/// message-based fallback registrations. MSI message-table teardown is pending.
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

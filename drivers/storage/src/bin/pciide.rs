#![no_std]
#![no_main]
//! PCI IDE hardware-dependent function driver (pciide.sys model).
//!
//! Binds above pciidex.sys, consumes the PnP-assigned raw/translated resource
//! lists on START_DEVICE, and commits controller state only after the lower
//! PCI stack completes successfully. Hardware register programming is kept
//! fail-closed until a HAL-backed port/MMIO mapping provider is connected.

use core::{ffi::c_void, sync::atomic::{AtomicBool, Ordering}};
use cognacos_nt_abi::{
    bus::{GUID_PCI_DEVICE_INTERFACE, PCI_DEVICE_INTERFACE, PCI_DEVICE_INFORMATION},
    io::{DEVICE_OBJECT, DRIVER_OBJECT, IRP, IO_STACK_LOCATION},
    ntstatus::*,
    types::PVOID,
};
use cognacos_storage::{
    ata_port::{AtaPortOperation, AtaPortRequest, ATA_PORT_REQUEST_DEVICE_INDEX_MASK},
    ide::{Channel, RegisterIo, MAX_LBA28}, pciidex::{
    parse_translated_resources, ControllerState, IdeResourceSet,
    PciIdeController, PciIdeIdentity, ResourceKind,
}};

const IRP_MJ_MAXIMUM_FUNCTION: usize = 27;
const IRP_MJ_PNP: u8 = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_SURPRISE_REMOVAL: u8 = 0x17;
const FILE_DEVICE_CONTROLLER: u32 = 4;
const FDO_EXTENSION_MAGIC: u64 = 0x434f_474e_5043_4944;

#[repr(C)]
struct PciIdeFdoExtension {
    magic: u64,
    lower_device: *mut DEVICE_OBJECT,
    physical_device: *mut DEVICE_OBJECT,
    controller: PciIdeController,
    previous_state: ControllerState,
    active_channels: u8,
    present_drives: u8,
    // One independently identified ATA device per channel/device position.
    // These channels are the miniport's persistent PIO transport state.
    channels: [Option<Channel>; 4],
    pio_lock: AtomicBool,
}
const _: () = assert!(core::mem::size_of::<PciIdeFdoExtension>() <= 512);

struct PioGuard<'a>(&'a AtomicBool);
impl<'a> PioGuard<'a> {
    fn acquire(lock: &'a AtomicBool) -> Self {
        while lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self(lock)
    }
}
impl Drop for PioGuard<'_> { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }

unsafe extern "win64" {
    fn IoCreateDevice(driver: *mut DRIVER_OBJECT, extension_size: u32,
        device_name: *mut cognacos_nt_abi::object::UNICODE_STRING,
        device_type: u32, characteristics: u32, exclusive: u8,
        output: *mut *mut DEVICE_OBJECT) -> i32;
    fn IoDeleteDevice(device: *mut DEVICE_OBJECT);
    fn IoAttachDeviceToDeviceStack(source: *mut DEVICE_OBJECT, target: *mut DEVICE_OBJECT) -> *mut DEVICE_OBJECT;
    fn IoDetachDevice(target: *mut DEVICE_OBJECT);
    fn IoCallDriver(device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32;
    fn IoAllocateIrp(stack_size: i8, charge_quota: u8) -> *mut IRP;
    fn IoFreeIrp(irp: *mut IRP);
    fn DbgPrint(format: *const u8) -> i32;
 fn KeInitializeEvent(event:PVOID,event_type:i32,state:u8);
 fn KeSetEvent(event:PVOID,increment:i32,wait:u8)->i32;
 fn KeWaitForSingleObject(event:PVOID,wait_reason:i32,wait_mode:i32,alertable:u8,timeout:*const i64)->i32;
}

core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.macro IMPORT name
.global \name
.type \name,@function
\name:
    jmp QWORD PTR [rip + __imp_ntoskrnl_\name]
.size \name, .-\name
.endm
IMPORT IoCreateDevice
IMPORT IoDeleteDevice
IMPORT IoAttachDeviceToDeviceStack
IMPORT IoDetachDevice
IMPORT IoCallDriver

IMPORT IoAllocateIrp

IMPORT IoFreeIrp
IMPORT DbgPrint

.section .data.imports.ntoskrnl,"aw",@progbits
.p2align 3
.macro IAT name
.global __imp_ntoskrnl_\name
.type __imp_ntoskrnl_\name,@object
__imp_ntoskrnl_\name: .quad 0
.size __imp_ntoskrnl_\name,8
.endm
IAT IoCreateDevice
IAT IoDeleteDevice
IAT IoAttachDeviceToDeviceStack
IAT IoDetachDevice
IAT IoCallDriver

IAT IoAllocateIrp

IAT IoFreeIrp
IAT DbgPrint
"#);


/// WDK inline macro equivalents: these are local source helpers, not PE imports.
fn current_stack_location(irp:*mut IRP)->*mut IO_STACK_LOCATION {
 if irp.is_null(){return core::ptr::null_mut()}
 unsafe{(*irp).tail[8] as *mut IO_STACK_LOCATION}
}
fn next_stack_location(irp:*mut IRP)->*mut IO_STACK_LOCATION {
 let current=current_stack_location(irp);
 if current.is_null(){core::ptr::null_mut()}else{unsafe{current.sub(1)}}
}
unsafe fn copy_current_irp_stack_location_to_next(irp:*mut IRP) {
 let current=current_stack_location(irp);let next=next_stack_location(irp);
 if !current.is_null()&&!next.is_null(){unsafe{core::ptr::copy(current,next,1)}}
}
unsafe fn skip_current_irp_stack_location(irp:*mut IRP) {
 if irp.is_null(){return}
 let current=current_stack_location(irp);if current.is_null(){return}
 unsafe{(*irp).current_location=(*irp).current_location.saturating_add(1);(*irp).tail[8]=current.add(1) as usize;}
}
unsafe fn set_completion_routine(
 irp:*mut IRP,
 routine:Option<unsafe extern "win64" fn(*mut DEVICE_OBJECT,*mut IRP,PVOID)->i32>,
 context:PVOID,success:u8,error:u8,cancel:u8,
) {
 let next=next_stack_location(irp);if next.is_null(){return}
 let mut control=0u8;
 if success!=0{control|=0x40} if error!=0{control|=0x80} if cancel!=0{control|=0x20}
 unsafe{(*next).completion_routine=routine;(*next).context=context;(*next).control|=control;}
}
unsafe extern "win64" fn query_interface_complete(_device:*mut DEVICE_OBJECT,_irp:*mut IRP,context:PVOID)->i32 {
 if !context.is_null(){let _=unsafe{KeSetEvent(context,0,0)};}
 0xC0000016u32 as i32
}
/// Windows IRP_MN_QUERY_INTERFACE path; no private kernel export is required.
unsafe fn query_pci_bus_interface(device:*mut DEVICE_OBJECT,guid:*const cognacos_nt_abi::bus::GUID,size:u16,version:u16,output:PVOID)->i32 {
 if device.is_null()||guid.is_null()||output.is_null(){return STATUS_INVALID_PARAMETER}
 let irp=unsafe{IoAllocateIrp((*device).stack_size.max(1) as i8,0)};
 if irp.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let mut event=[0u64;3];unsafe{KeInitializeEvent(event.as_mut_ptr().cast(),0,0);}
 let stack=next_stack_location(irp);
 if stack.is_null(){unsafe{IoFreeIrp(irp)};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=0x1b;(*stack).minor_function=0x08;
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<*const cognacos_nt_abi::bus::GUID>(),guid);
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<u16>(),size);
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(10).cast::<u16>(),version);
  core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(16).cast::<PVOID>(),output);
  set_completion_routine(irp,Some(query_interface_complete),event.as_mut_ptr().cast(),1,1,1);
 }
 let status=unsafe{IoCallDriver(device,irp)};
 if status==STATUS_PENDING{
  let wait=unsafe{KeWaitForSingleObject(event.as_mut_ptr().cast(),0,0,0,core::ptr::null())};
  if wait<0{unsafe{IoFreeIrp(irp)};return wait}
 }
 let io_status=unsafe{(*irp).io_status.status.status};
 unsafe{IoFreeIrp(irp)}
 if status==STATUS_PENDING{io_status}else if status<0{status}else{io_status}
}

unsafe fn resource_list_bytes(list: *const u8) -> Option<&'static [u8]> {
    if list.is_null() { return Some(&[]); }
    let full_count = unsafe { core::ptr::read_unaligned(list.cast::<u32>()) } as usize;
    if full_count > 16 { return None; }
    let mut cursor = 4usize;
    for _ in 0..full_count {
        let partial_count = unsafe { core::ptr::read_unaligned(list.add(cursor + 12).cast::<u32>()) } as usize;
        if partial_count > 64 { return None; }
        cursor = cursor.checked_add(16)?.checked_add(partial_count.checked_mul(16)?)?;
        if cursor > 4 + 16 * (16 + 64 * 16) { return None; }
    }
    Some(unsafe { core::slice::from_raw_parts(list, cursor) })
}

unsafe fn read_start_resources(stack: *mut IO_STACK_LOCATION) -> Result<IdeResourceSet, i32> {
    if stack.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let raw = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(4).cast::<*const u8>()) };
    let translated = unsafe { core::ptr::read_unaligned((*stack).parameters.as_ptr().add(12).cast::<*const u8>()) };
    if raw.is_null() != translated.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let raw_bytes = unsafe { resource_list_bytes(raw) }.ok_or(STATUS_INVALID_PARAMETER)?;
    let translated_bytes = unsafe { resource_list_bytes(translated) }.ok_or(STATUS_INVALID_PARAMETER)?;
    let raw_set = parse_translated_resources(raw_bytes)?;
    let translated_set = parse_translated_resources(translated_bytes)?;
    if raw_set.count != translated_set.count { return Err(STATUS_INVALID_PARAMETER); }
    for index in 0..raw_set.count {
        let raw_resource = raw_set.get(index).ok_or(STATUS_INVALID_PARAMETER)?;
        let translated_resource = translated_set.get(index).ok_or(STATUS_INVALID_PARAMETER)?;
        if raw_resource.kind != translated_resource.kind { return Err(STATUS_INVALID_PARAMETER); }
    }
    Ok(translated_set)
}

/// x86 port-I/O backend. The caller may construct channels only from
/// ranges present in the translated PnP resource list; no hard-coded port
/// is touched unless the bus has assigned that range to this controller.
struct X86PortIo;
impl RegisterIo for X86PortIo {
    fn read8(&mut self, port: u16) -> Result<u8, cognacos_storage::StorageError> {
        let value: u8;
        unsafe { core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags)); }
        Ok(value)
    }
    fn write8(&mut self, port: u16, value: u8) -> Result<(), cognacos_storage::StorageError> {
        unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
        Ok(())
    }
    fn read16(&mut self, port: u16) -> Result<u16, cognacos_storage::StorageError> {
        let value: u16;
        unsafe { core::arch::asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags)); }
        Ok(value)
    }
    fn write16(&mut self, port: u16, value: u16) -> Result<(), cognacos_storage::StorageError> {
        unsafe { core::arch::asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags)); }
        Ok(())
    }
    fn delay_400ns(&mut self, alternate_status: u16) -> Result<(), cognacos_storage::StorageError> {
        for _ in 0..4 { let _ = self.read8(alternate_status)?; }
        Ok(())
    }
}


fn assigned_port_range(resources: &IdeResourceSet, start: u64, length: u64) -> bool {
    resources.entries[..resources.count].iter().flatten().any(|resource| {
        resource.kind == ResourceKind::Port
            && resource.start <= start
            && resource.start.checked_add(resource.length as u64)
                .is_some_and(|end| end >= start.saturating_add(length))
    })
}

fn assigned_port_resource(resources: &IdeResourceSet, ordinal: usize) -> Option<(u16, u32)> {
    let mut seen = 0usize;
    for resource in resources.entries[..resources.count].iter().flatten() {
        if resource.kind != ResourceKind::Port { continue; }
        let end = resource.start.checked_add(resource.length as u64)?;
        if resource.start > u16::MAX as u64 || end > (u16::MAX as u64) + 1 { return None; }
        if seen == ordinal { return Some((resource.start as u16, resource.length)); }
        seen += 1;
    }
    None
}

const IRP_MJ_INTERNAL_DEVICE_CONTROL: u8 = 0x0f;
const IOCTL_ATA_PORT_REGISTER_MINIPORT: u32 = (FILE_DEVICE_CONTROLLER << 16) | (0x800 << 2) | 3;
const IOCTL_ATA_PORT_UNREGISTER_MINIPORT: u32 = (FILE_DEVICE_CONTROLLER << 16) | (0x801 << 2) | 3;
const IOCTL_ATA_PORT_SUBMIT_MINIPORT: u32 = (FILE_DEVICE_CONTROLLER << 16) | (0x802 << 2) | 3;

unsafe extern "win64" fn miniport_submit(
    context: *mut c_void,
    request: *const cognacos_storage::ata_port::AtaPortRequest,
    bytes_transferred: *mut u32,
) -> i32 {
    if context.is_null() || request.is_null() || bytes_transferred.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { bytes_transferred.write(0); }
    let ext = context.cast::<PciIdeFdoExtension>();
    if unsafe { (*ext).magic != FDO_EXTENSION_MAGIC || (*ext).controller.state != ControllerState::Started } {
        return STATUS_DEVICE_NOT_READY;
    }
    let request = unsafe { &*request };
    let slot = (request.flags & ATA_PORT_REQUEST_DEVICE_INDEX_MASK) as usize;
    let Some(channel) = (unsafe { (*ext).channels[slot] }) else { return STATUS_NOT_FOUND; };
    let _pio_guard = PioGuard::acquire(unsafe { &(*ext).pio_lock });
    let mut io = X86PortIo;
    let result = match request.operation {
        cognacos_storage::ata_port::AtaPortOperation::Read => {
            let Some(bytes) = request.sector_count.checked_mul(512) else { return STATUS_INVALID_PARAMETER; };
            if request.system_buffer.is_null() || bytes == 0 || bytes as usize > 256 * 512 {
                return STATUS_INVALID_PARAMETER;
            }
            let output = unsafe { core::slice::from_raw_parts_mut(request.system_buffer, bytes as usize) };
            channel.read_sectors(&mut io, request.lba, output).map(|_| bytes)
        }
        cognacos_storage::ata_port::AtaPortOperation::Write => {
            let Some(bytes) = request.sector_count.checked_mul(512) else { return STATUS_INVALID_PARAMETER; };
            if request.system_buffer.is_null() || bytes == 0 || bytes as usize > 256 * 512 {
                return STATUS_INVALID_PARAMETER;
            }
            let input = unsafe { core::slice::from_raw_parts(request.system_buffer, bytes as usize) };
            channel.write_sectors(&mut io, request.lba, input).map(|_| bytes)
        }
        cognacos_storage::ata_port::AtaPortOperation::Flush => channel.flush(&mut io).map(|_| 0),
        cognacos_storage::ata_port::AtaPortOperation::Identify => {
            if request.system_buffer.is_null() || request.sector_count != 1 {
                return STATUS_INVALID_PARAMETER;
            }
            let mut identified = channel;
            match identified.identify(&mut io) {
                Ok(words) => {
                    let output = unsafe { core::slice::from_raw_parts_mut(request.system_buffer, 512) };
                    for (index, word) in words.iter().copied().enumerate() {
                        output[index * 2..index * 2 + 2].copy_from_slice(&word.to_le_bytes());
                    }
                    unsafe { (*ext).channels[slot] = Some(identified); }
                    Ok(512)
                }
                Err(error) => Err(error),
            }
        }
        _ => return STATUS_INVALID_DEVICE_REQUEST,
    };
    match result {
        Ok(bytes) => { unsafe { bytes_transferred.write(bytes); } STATUS_SUCCESS }
        Err(error) => cognacos_storage::error_to_ntstatus(error),
    }
}

unsafe fn send_port_control(lower: *mut DEVICE_OBJECT, code: u32, input: *const c_void) -> i32 {
    if lower.is_null() { return STATUS_DEVICE_NOT_READY; }
    let irp = unsafe { IoAllocateIrp((*lower).stack_size as i8, 0) };
    if irp.is_null() { return STATUS_INSUFFICIENT_RESOURCES; }
    let stack = unsafe { next_stack_location(irp) };
    if stack.is_null() { unsafe { IoFreeIrp(irp); } return STATUS_INVALID_PARAMETER; }
    unsafe {
        (*stack).major_function = IRP_MJ_INTERNAL_DEVICE_CONTROL;
        (*stack).parameters.fill(0);
        core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<u32>(), code);
        core::ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(16).cast::<*const c_void>(), input);
    }
    let status = unsafe { IoCallDriver(lower, irp) };
    unsafe { IoFreeIrp(irp); }
    status
}

unsafe fn register_ataport_miniport(extension: *mut PciIdeFdoExtension) -> i32 {
    let table = cognacos_storage::ata_port::AtaPortMiniportInterface {
        size: core::mem::size_of::<cognacos_storage::ata_port::AtaPortMiniportInterface>() as u32,
        version: cognacos_storage::ata_port::ATA_PORT_INTERFACE_VERSION,
        miniport_context: extension.cast(),
        initialize: None,
        start: None,
        submit: Some(miniport_submit),
        reset: None,
        stop: None,
        unload: None,
    };
    unsafe { send_port_control((*extension).lower_device, IOCTL_ATA_PORT_REGISTER_MINIPORT, (&table as *const cognacos_storage::ata_port::AtaPortMiniportInterface).cast::<c_void>()) }
}

unsafe fn unregister_ataport_miniport(extension: *mut PciIdeFdoExtension) -> i32 {
    unsafe { send_port_control((*extension).lower_device, IOCTL_ATA_PORT_UNREGISTER_MINIPORT, extension.cast()) }
}

unsafe fn submit_test_request(
    extension: *mut PciIdeFdoExtension,
    operation: AtaPortOperation,
    lba: u64,
    sectors: u32,
    flags: u32,
    buffer: *mut u8,
) -> i32 {
    let request = AtaPortRequest {
        size: core::mem::size_of::<AtaPortRequest>() as u32,
        operation,
        request_id: 0x5155_454d_5550,
        lba,
        sector_count: sectors,
        flags,
        system_buffer: buffer,
        sg_list: core::ptr::null(),
        sg_count: 0,
        timeout_ms: 30_000,
        packet_cdb: core::ptr::null(),
        packet_cdb_length: 0,
        reserved: [0; 7],
    };
    unsafe {
        send_port_control(
            (*extension).lower_device,
            IOCTL_ATA_PORT_SUBMIT_MINIPORT,
            (&request as *const AtaPortRequest).cast(),
        )
    }
}

/// QEMU integration test against the dedicated primary-slave test disk.
/// It snapshots two sectors, writes a deterministic pattern, flushes, reads
/// them back, compares, and restores the original bytes before reporting pass.
unsafe fn qemu_pio_round_trip(extension: *mut PciIdeFdoExtension) -> i32 {
    if unsafe { (*extension).channels[1].is_none() } { return STATUS_SUCCESS; }
    let mut original = [0u8; 1024];
    let mut verify = [0u8; 1024];
    let mut pattern = [0u8; 1024];
    for (index, byte) in pattern.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(37).wrapping_add(0xa5);
    }
    let read_original = unsafe {
        submit_test_request(extension, AtaPortOperation::Read, 1, 2, 1, original.as_mut_ptr())
    };
    if read_original != STATUS_SUCCESS { return read_original; }

    let write_status = unsafe {
        submit_test_request(extension, AtaPortOperation::Write, 1, 2, 1, pattern.as_mut_ptr())
    };
    if write_status != STATUS_SUCCESS {
        let _ = unsafe { submit_test_request(extension, AtaPortOperation::Write, 1, 2, 1, original.as_mut_ptr()) };
        let _ = unsafe { submit_test_request(extension, AtaPortOperation::Flush, 0, 0, 1, core::ptr::null_mut()) };
        return write_status;
    }
    let flush_status = unsafe {
        submit_test_request(extension, AtaPortOperation::Flush, 0, 0, 1, core::ptr::null_mut())
    };
    let read_status = if flush_status == STATUS_SUCCESS {
        unsafe { submit_test_request(extension, AtaPortOperation::Read, 1, 2, 1, verify.as_mut_ptr()) }
    } else { flush_status };
    let matched = read_status == STATUS_SUCCESS && verify == pattern;

    // Always restore the test disk's previous contents, even when verification
    // fails, so repeated QEMU runs do not leave a modified test image.
    let restore_status = unsafe {
        submit_test_request(extension, AtaPortOperation::Write, 1, 2, 1, original.as_mut_ptr())
    };
    let restore_flush = if restore_status == STATUS_SUCCESS {
        unsafe { submit_test_request(extension, AtaPortOperation::Flush, 0, 0, 1, core::ptr::null_mut()) }
    } else { restore_status };
    if restore_flush != STATUS_SUCCESS { return restore_flush; }
    if !matched { return if read_status == STATUS_SUCCESS { STATUS_DEVICE_DATA_ERROR } else { read_status }; }
    STATUS_SUCCESS
}

/// Resolve command/control register windows from PnP-translated resources.
/// Native-mode BARs are consumed in PCI IDE BAR order (command/control pairs);
/// compatibility mode uses the legacy ranges only when those exact ranges
/// were assigned by the bus. No BAR is guessed and no PCI config is modified.
fn initialize_compatibility_channels(extension: &mut PciIdeFdoExtension) -> i32 {
    let identity = extension.controller.identity;
    let resources = &extension.controller.resources;
    let mut active = 0u8;
    let mut drives = 0u8;
    let mut discovered = [None; 4];
    let mut io = X86PortIo;

    let definitions = [
        (false, 0x1f0u16, 0x3f6u16, 0x1f0u64, 8u64, 0x3f4u64, 4u64),
        (true,  0x170u16, 0x376u16, 0x170u64, 8u64, 0x374u64, 4u64),
    ];
    for (index, (secondary, legacy_command, legacy_control, legacy_command_resource,
                 command_len, legacy_control_resource, control_len)) in definitions.into_iter().enumerate() {
        let native = if secondary { identity.secondary_native() } else { identity.primary_native() };
        let (command, control) = if native {
            // BAR0/1 are primary command/control, BAR2/3 secondary.
            let command_ordinal = if secondary { 2 } else { 0 };
            let control_ordinal = command_ordinal + 1;
            let Some((command, command_size)) = assigned_port_resource(resources, command_ordinal) else { continue; };
            let Some((control_bar, control_size)) = assigned_port_resource(resources, control_ordinal) else { continue; };
            if command_size < 8 || control_size < 3 { continue; }
            let Some(control) = control_bar.checked_add(2) else { continue; };
            (command, control)
        } else {
            if !assigned_port_range(resources, legacy_command_resource, command_len)
                || !assigned_port_range(resources, legacy_control_resource, control_len) { continue; }
            (legacy_command, legacy_control)
        };
        active |= 1 << index;
        for device in 0..2u8 {
            let channel = Channel { command_base: command, control, device, sectors: MAX_LBA28, poll_budget: 1_000_000 };
            // A missing device commonly floats as 0x00/0xff. IDENTIFY is
            // attempted only after the assigned channel responds plausibly.
            let status = match io.read8(command + 7) { Ok(value) => value, Err(_) => continue };
            if status == 0 || status == 0xff { continue; }
            let mut probe = channel;
            if probe.identify(&mut io).is_ok() {
                let slot = index * 2 + device as usize;
                drives |= 1 << slot;
                discovered[slot] = Some(probe);
            }
        }
    }
    if active == 0 || drives == 0 { return STATUS_DEVICE_CONFIGURATION_ERROR; }
    extension.active_channels = active;
    extension.present_drives = drives;
    extension.channels = discovered;
    STATUS_SUCCESS
}

unsafe extern "win64" fn pnp_complete(
    _lower_device: *mut DEVICE_OBJECT, irp: *mut IRP, context: PVOID,
) -> i32 {
    let fdo = context.cast::<DEVICE_OBJECT>();
    if fdo.is_null() || irp.is_null() { return STATUS_SUCCESS; }
    let extension = unsafe { (*fdo).device_extension.cast::<PciIdeFdoExtension>() };
    if extension.is_null() || unsafe { (*extension).magic != FDO_EXTENSION_MAGIC } {
        return STATUS_SUCCESS;
    }
    let status = unsafe { (*irp).io_status.status.status };
    let stack = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
    let minor = if stack.is_null() { 0xff } else { unsafe { (*stack).minor_function } };
    match minor {
        IRP_MN_START_DEVICE => {
            let hardware_status = if status == STATUS_SUCCESS {
                let ext = unsafe { &mut *extension };
                initialize_compatibility_channels(ext)
            } else { status };
            if hardware_status == STATUS_SUCCESS {
                let _ = unsafe { (*extension).controller.start_succeeded() };
                let registration = unsafe { register_ataport_miniport(extension) };
                let test_status = if registration == STATUS_SUCCESS { unsafe { qemu_pio_round_trip(extension) } } else { registration };
                if test_status == STATUS_SUCCESS && unsafe { (*extension).channels[1].is_some() } {
                    unsafe { DbgPrint(b"pciide: ATA PIO read/write/flush round-trip passed\\r\\n\\0".as_ptr()); }
                }
                if test_status != STATUS_SUCCESS {
                    unsafe {
                        (*extension).controller.stop_succeeded();
                        (*extension).channels = [None; 4];
                        (*extension).active_channels = 0;
                        (*extension).present_drives = 0;
                        let _ = unregister_ataport_miniport(extension);
                        (*irp).io_status.status.status = test_status;
                    }
                }
            } else {
                unsafe {
                    (*irp).io_status.status.status = hardware_status;
                    (*extension).controller.start_failed((*extension).previous_state);
                }
            }
        }
        IRP_MN_STOP_DEVICE | IRP_MN_SURPRISE_REMOVAL => {
            if status == STATUS_SUCCESS {
                let ext = unsafe { &mut *extension };
                if ext.controller.state == ControllerState::Started {
                    let _ = ext.controller.stop_succeeded();
                }
                // The miniport was unregistered before forwarding STOP/REMOVE.
                ext.channels = [None; 4];
                ext.active_channels = 0;
                ext.present_drives = 0;
            }
        }
        IRP_MN_REMOVE_DEVICE => {
            let ext = unsafe { &mut *extension };
            let lower = ext.lower_device;
            unsafe { (*extension).magic = 0; }
            if !lower.is_null() { unsafe { IoDetachDevice(lower); } }
            unsafe { IoDeleteDevice(fdo); }
        }
        _ => {}
    }
    STATUS_SUCCESS
}

unsafe extern "win64" fn dispatch(device: *mut DEVICE_OBJECT, irp: *mut IRP) -> i32 {
    if device.is_null() || irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let extension = unsafe { (*device).device_extension.cast::<PciIdeFdoExtension>() };
    if extension.is_null() || unsafe { (*extension).magic != FDO_EXTENSION_MAGIC } {
        return STATUS_DELETE_PENDING;
    }
    let lower = unsafe { (*extension).lower_device };
    if lower.is_null() { return STATUS_DEVICE_NOT_READY; }
    let stack = unsafe { (*irp).tail[8] as *mut IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }

    if unsafe { (*stack).major_function } == IRP_MJ_PNP {
        match unsafe { (*stack).minor_function } {
            IRP_MN_START_DEVICE => {
                let resources = match unsafe { read_start_resources(stack) } {
                    Ok(value) => value,
                    Err(status) => return status,
                };
                let previous = unsafe { (*extension).controller.state };
                if let Err(status) = unsafe { (*extension).controller.assign_resources(resources) } {
                    return status;
                }
                unsafe { (*extension).previous_state = previous; }
            }
            IRP_MN_STOP_DEVICE | IRP_MN_REMOVE_DEVICE | IRP_MN_SURPRISE_REMOVAL => {
                let status = unsafe { unregister_ataport_miniport(extension) };
                if status != STATUS_SUCCESS && status != STATUS_DEVICE_NOT_READY {
                    return status;
                }
            }
            _ => {}
        }
        unsafe {
            copy_current_irp_stack_location_to_next(irp);
            set_completion_routine(irp, Some(pnp_complete), device.cast(), 1, 1, 1);
            return IoCallDriver(lower, irp);
        }
    }
    unsafe {
        skip_current_irp_stack_location(irp);
        IoCallDriver(lower, irp)
    }
}

unsafe extern "win64" fn add_device(driver: *mut DRIVER_OBJECT, pdo: *mut DEVICE_OBJECT) -> i32 {
    if driver.is_null() || pdo.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut interface = core::mem::MaybeUninit::<PCI_DEVICE_INTERFACE>::zeroed();
    let status = unsafe {
        query_pci_bus_interface(pdo, &GUID_PCI_DEVICE_INTERFACE,
            core::mem::size_of::<PCI_DEVICE_INTERFACE>() as u16,
            cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION as u16,
            interface.as_mut_ptr().cast())
    };
    if status != STATUS_SUCCESS { return STATUS_NOT_SUPPORTED; }
    let interface = unsafe { interface.assume_init() };
    if (interface.header.size as usize) < core::mem::size_of::<PCI_DEVICE_INTERFACE>()
        || interface.header.version as u32 != cognacos_nt_abi::bus::PCI_DEVICE_INTERFACE_VERSION {
        return 0xC000_0059u32 as i32;
    }
    let Some(query) = interface.query_information else { return STATUS_NOT_SUPPORTED; };
    let mut information = PCI_DEVICE_INFORMATION::default();
    let status = unsafe { query(interface.header.context, &mut information) };
    if status != STATUS_SUCCESS { return status; }
    if information.class_code != 0x01 || information.subclass != 0x01 {
        return STATUS_NOT_SUPPORTED;
    }
    let identity = PciIdeIdentity {
        vendor_id: information.vendor_id, device_id: information.device_id,
        base_class: information.class_code, sub_class: information.subclass,
        programming_interface: information.programming_interface,
        revision_id: information.revision_id,
    };
    let mut fdo = core::ptr::null_mut();
    let status = unsafe {
        IoCreateDevice(driver.cast(), core::mem::size_of::<PciIdeFdoExtension>() as u32,
            core::ptr::null_mut(), FILE_DEVICE_CONTROLLER, 0, 0, &mut fdo)
    };
    if status != STATUS_SUCCESS { return status; }
    if fdo.is_null() { return STATUS_INSUFFICIENT_RESOURCES; }
    let lower = unsafe { IoAttachDeviceToDeviceStack(fdo, pdo) };
    if lower.is_null() {
        unsafe { IoDeleteDevice(fdo); }
        return 0xC000_000Eu32 as i32;
    }
    let extension = unsafe { (*fdo).device_extension.cast::<PciIdeFdoExtension>() };
    let controller = PciIdeController::new(identity);
    unsafe {
        extension.write(PciIdeFdoExtension {
            magic: FDO_EXTENSION_MAGIC, lower_device: lower,
            physical_device: pdo, controller,
            previous_state: ControllerState::Created,
            active_channels: 0,
            present_drives: 0,
            channels: [None; 4],
            pio_lock: AtomicBool::new(false),
        });
        (*fdo).flags = ((*fdo).flags & !0x80) | ((*lower).flags & (0x02 | 0x04));
    }
    STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: *mut DRIVER_OBJECT, _context: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe {
        for index in 0..=IRP_MJ_MAXIMUM_FUNCTION {
            (*driver).major_function[index] = Some(dispatch);
        }
        (*driver).driver_unload = None;
    }
    {
 let extension=unsafe{(*driver).driver_extension.cast::<DRIVER_EXTENSION>()};
 if extension.is_null(){return STATUS_INVALID_PARAMETER}
 unsafe{(*extension).add_device=Some(add_device);}
 STATUS_SUCCESS
}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}

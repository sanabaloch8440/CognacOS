//! Partial Windows x64 WDM object/IRP ABI foundation.
//!
//! Layouts and dispatch conventions here follow the public x64 WDK ABI for the
//! fields used by this implementation. This is not a claim of complete WDM
//! compatibility: MDLs, cancellation ownership, remove locks, I/O stack
//! completion unwinding, security descriptors, VPBs, and many IRP variants are
//! still unsupported. Driver images must not be treated as generally loadable.
use core::{ffi::c_void, ptr};
use crate::hal::IrqSpinLock;

pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_PENDING: i32 = 0x0000_0103;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
pub const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009A_u32 as i32;
pub const STATUS_INVALID_DEVICE_REQUEST: i32 = 0xC000_0010_u32 as i32;
pub const STATUS_DELETE_PENDING: i32 = 0xC000_0056_u32 as i32;
pub const IRP_SIZE_X64: usize = 0xD0;
pub const IO_STACK_LOCATION_SIZE_X64: usize = 0x48;
const MAX_WDM_DEVICES: usize = 32;
const MAX_WDM_IRPS: usize = 64;
const MAX_DEVICE_EXTENSION: usize = 512;
const IRP_STACK_OFFSET: usize = 0xB8;
const IRP_STATUS_OFFSET: usize = 0x30;
const IRP_INFORMATION_OFFSET: usize = 0x38;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnicodeString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u16 }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IoStatusBlock { pub status: isize, pub information: usize }
pub type DriverDispatch = unsafe extern "system" fn(*mut DeviceObject, *mut Irp) -> i32;
pub type DriverUnload = unsafe extern "system" fn(*mut DriverObject);
pub type IoCompletionRoutine = unsafe extern "system" fn(*mut DeviceObject, *mut Irp, *mut c_void) -> i32;

#[repr(C)]
pub struct DriverObject {
    pub object_type: i16, pub size: i16, pub device_object: *mut DeviceObject,
    pub flags: u32, pub driver_start: *mut c_void, pub driver_size: u32,
    pub driver_section: *mut c_void, pub driver_extension: *mut c_void,
    pub driver_name: UnicodeString, pub hardware_database: *mut UnicodeString,
    pub fast_io_dispatch: *mut c_void, pub driver_init: *mut c_void,
    pub driver_start_io: *mut c_void, pub driver_unload: Option<DriverUnload>,
    pub major_function: [Option<DriverDispatch>; 28],
}
#[repr(C, align(16))]
pub struct DeviceObject {
    pub object_type: i16, pub size: u16, pub reference_count: i32,
    pub driver_object: *mut DriverObject, pub next_device: *mut DeviceObject,
    pub attached_device: *mut DeviceObject, pub current_irp: *mut Irp,
    pub timer: *mut c_void, pub flags: u32, pub characteristics: u32,
    pub vpb: *mut c_void, pub device_extension: *mut c_void,
    pub device_type: u32, pub stack_size: u8, pub queue_padding: [u8; 3],
    pub queue: [u8; 0x48], pub alignment_requirement: u32, pub device_queue_padding: [u8; 4],
    pub device_queue: [u8; 0x28], pub dpc: [u8; 0x40], pub active_thread_count: u32,
    pub security_descriptor: *mut c_void, pub device_lock: [u8; 0x18],
    pub sector_size: u16, pub spare1: u16, pub device_object_extension: *mut c_void,
    pub reserved: *mut c_void,
}
/// Opaque storage for the x64 WDK IRP. Fields used here are accessed by their
/// documented x64 offsets to avoid pretending that Rust's high-level fields
/// are the complete SDK definition.
#[repr(C, align(16))]
pub struct Irp { pub bytes: [u8; IRP_SIZE_X64] }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IoStackLocation {
    pub major_function: u8, pub minor_function: u8, pub flags: u8, pub control: u8,
    pub parameters: [u64; 4], pub device_object: *mut DeviceObject,
    pub file_object: *mut c_void, pub completion_routine: Option<IoCompletionRoutine>,
    pub context: *mut c_void,
}
#[repr(C)]
struct DeviceSlot { used: bool, deleting: bool, attached_to: *mut DeviceObject, object: DeviceObject, extension: [u8; MAX_DEVICE_EXTENSION] }
unsafe impl Send for DeviceSlot {}
#[repr(C)]
struct IrpSlot { used: bool, object: Irp, stack: [IoStackLocation;16] }
unsafe impl Send for IrpSlot {}
struct WdmState { devices: [DeviceSlot; MAX_WDM_DEVICES], irps: [IrpSlot; MAX_WDM_IRPS] }
impl WdmState {
 const fn new() -> Self {
  Self {
   devices: [const { DeviceSlot { used:false, deleting:false, attached_to:ptr::null_mut(), object: DeviceObject {
    object_type:3,size:core::mem::size_of::<DeviceObject>() as u16,reference_count:1,
    driver_object:ptr::null_mut(),next_device:ptr::null_mut(),attached_device:ptr::null_mut(),
    current_irp:ptr::null_mut(),timer:ptr::null_mut(),flags:0,characteristics:0,vpb:ptr::null_mut(),
    device_extension:ptr::null_mut(),device_type:0,stack_size:1,queue_padding:[0;3],queue:[0;0x48],
    alignment_requirement:0,device_queue_padding:[0;4],device_queue:[0;0x28],dpc:[0;0x40],active_thread_count:0,
    security_descriptor:ptr::null_mut(),device_lock:[0;0x18],sector_size:0,spare1:0,
    device_object_extension:ptr::null_mut(),reserved:ptr::null_mut()
   },extension:[0;MAX_DEVICE_EXTENSION] } }; MAX_WDM_DEVICES],
   irps: [const { IrpSlot { used:false, object: Irp { bytes:[0;IRP_SIZE_X64] }, stack:[IoStackLocation { major_function:0,minor_function:0,flags:0,control:0,parameters:[0;4],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut() };16] } }; MAX_WDM_IRPS],
  }
 }
}
static WDM: IrqSpinLock<WdmState> = IrqSpinLock::new(WdmState::new());
unsafe fn read_ptr(base:*const u8, offset:usize)->*mut c_void { unsafe { ptr::read_unaligned(base.add(offset).cast::<*mut c_void>()) } }
unsafe fn write_ptr(base:*mut u8, offset:usize, value:*mut c_void) { unsafe { ptr::write_unaligned(base.add(offset).cast::<*mut c_void>(),value); } }
unsafe fn write_usize(base:*mut u8, offset:usize, value:usize) { unsafe { ptr::write_unaligned(base.add(offset).cast::<usize>(),value); } }
unsafe fn write_isize(base:*mut u8, offset:usize, value:isize) { unsafe { ptr::write_unaligned(base.add(offset).cast::<isize>(),value); } }

/// Implements the common IoCreateDevice form. Device names are validated as
/// counted UTF-16 strings; the kernel owns the returned device object and its
/// zeroed extension for its lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_create_device(
 driver:*mut DriverObject, extension_size:u32, device_name:*mut UnicodeString,
 device_type:u32, characteristics:u32, exclusive:u8, output:*mut *mut DeviceObject
)->i32 {
 if driver.is_null() || output.is_null() || extension_size as usize>MAX_DEVICE_EXTENSION || exclusive>1 { return STATUS_INVALID_PARAMETER; }
 unsafe { output.write(ptr::null_mut()); }
 if !device_name.is_null() {
  let name=unsafe{&*device_name};
  if name.length>name.maximum_length || name.length%2!=0 || (name.length!=0&&name.buffer.is_null()) {return STATUS_INVALID_PARAMETER;}
 }
 let mut state=WDM.lock();
 let Some(slot)=state.devices.iter_mut().find(|s|!s.used) else {return STATUS_INSUFFICIENT_RESOURCES};
 slot.used=true;slot.deleting=false;
 slot.object.driver_object=driver;slot.object.device_type=device_type;slot.object.characteristics=characteristics;
 slot.object.size=(core::mem::size_of::<DeviceObject>()+extension_size as usize) as u16;
 slot.object.device_extension=if extension_size==0 {ptr::null_mut()} else {slot.extension.as_mut_ptr().cast()};
 slot.object.stack_size=1;
 slot.object.next_device=unsafe{(*driver).device_object};
 let object=&mut slot.object as *mut DeviceObject;
 unsafe { (*driver).device_object=object; output.write(object); }
 STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_delete_device(device:*mut DeviceObject) {
 if device.is_null(){return}
 let mut state=WDM.lock();
 let Some(index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,device)) else{return};
 state.devices[index].deleting=true;
 let lower=state.devices[index].attached_to;
 let upper=state.devices[index].object.attached_device;
 if !lower.is_null() {
  unsafe { if (*lower).attached_device==device { (*lower).attached_device=upper; } }
 }
 if !upper.is_null() {
  if let Some(upper_index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,upper)) {
   state.devices[upper_index].attached_to=lower;
  }
 }
 let driver=state.devices[index].object.driver_object;
 if !driver.is_null() {
  unsafe {
   let mut cursor=ptr::addr_of_mut!((*driver).device_object);
   while !(*cursor).is_null() {
    if *cursor==device { *cursor=(*device).next_device;break; }
    cursor=ptr::addr_of_mut!((**cursor).next_device);
   }
  }
 }
 let slot=&mut state.devices[index];
 slot.object.device_extension=ptr::null_mut();
 slot.object.driver_object=ptr::null_mut();
 slot.object.next_device=ptr::null_mut();
 slot.object.attached_device=ptr::null_mut();
 slot.attached_to=ptr::null_mut();
 slot.used=false;slot.deleting=false;
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_allocate_irp(stack_size:i16, charge_quota:u8)->*mut Irp {
 if stack_size<=0||stack_size>16||charge_quota>1{return ptr::null_mut()}
 let mut state=WDM.lock();
 let Some(slot)=state.irps.iter_mut().find(|s|!s.used) else{return ptr::null_mut()};
 slot.used=true;slot.object.bytes.fill(0);
 slot.object.bytes[0..2].copy_from_slice(&6i16.to_le_bytes());
 let allocation_size=IRP_SIZE_X64+stack_size as usize*IO_STACK_LOCATION_SIZE_X64;
 slot.object.bytes[2..4].copy_from_slice(&(allocation_size as u16).to_le_bytes());
 slot.object.bytes[0x42]=stack_size as u8;slot.object.bytes[0x43]=(stack_size+1) as u8;
 slot.stack.fill(IoStackLocation { major_function:0,minor_function:0,flags:0,control:0,parameters:[0;4],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut() });
 let object=&mut slot.object as *mut Irp;
 let one_past=unsafe{slot.stack.as_mut_ptr().add(stack_size as usize).cast::<c_void>()};
 unsafe { write_ptr((*object).bytes.as_mut_ptr(),IRP_STACK_OFFSET,one_past); }
 object
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_free_irp(irp:*mut Irp) {
 if irp.is_null(){return}
 let mut state=WDM.lock();
 if let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) {slot.object.bytes.fill(0);slot.used=false;}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_get_current_irp_stack_location(irp:*mut Irp)->*mut IoStackLocation {
 if irp.is_null(){return ptr::null_mut()}
 unsafe{read_ptr((*irp).bytes.as_ptr(),IRP_STACK_OFFSET).cast()}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_get_next_irp_stack_location(irp:*mut Irp)->*mut IoStackLocation {
 if irp.is_null(){return ptr::null_mut()}
 let current=unsafe{io_get_current_irp_stack_location(irp)};
 if current.is_null(){return ptr::null_mut()}
 let state=WDM.lock();
 let Some(slot)=state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return ptr::null_mut()};
 let count=slot.object.bytes[0x42] as usize;
 if count==0||count>slot.stack.len(){return ptr::null_mut()}
 let base=slot.stack.as_ptr() as usize;
 let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
 let cur=current as usize;
 if cur<=base||cur>end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return ptr::null_mut()}
 unsafe{current.cast::<u8>().sub(IO_STACK_LOCATION_SIZE_X64).cast()}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_call_driver(device:*mut DeviceObject, irp:*mut Irp)->i32 {
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let (driver, deleting)={
  let state=WDM.lock();
  match state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)) {
   Some(slot)=>(slot.object.driver_object,slot.deleting),None=>(unsafe{(*device).driver_object},true)
  }
 };
 if deleting||driver.is_null(){return STATUS_DELETE_PENDING}
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){return STATUS_INVALID_PARAMETER}
 let location=unsafe{(*irp).bytes[0x43]};
 if location<=1{return STATUS_INVALID_DEVICE_REQUEST}
 unsafe {
  write_ptr((*irp).bytes.as_mut_ptr(),IRP_STACK_OFFSET,stack.cast());
  (*irp).bytes[0x43]=location-1;
  (*stack).device_object=device;
 }
 let major=unsafe{(*stack).major_function as usize};
 if major>=28{return STATUS_INVALID_DEVICE_REQUEST}
 let dispatch=unsafe{(*driver).major_function[major]};
 let Some(dispatch)=dispatch else{return STATUS_INVALID_DEVICE_REQUEST};
 unsafe{dispatch(device,irp)}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_complete_request(irp:*mut Irp, _priority_boost:i8) {
 if irp.is_null(){return}
 // Run registered completion routines while unwinding toward the original caller.
 // APC delivery, event signaling, MDL cleanup and thread-owned IRP teardown remain TODO.
 for _ in 0..16 {
  let (location,stack_count)=unsafe{((*irp).bytes[0x43] as usize,(*irp).bytes[0x42] as usize)};
  if location==0 || location>stack_count || stack_count==0 || stack_count>16 {return}
  let stack=unsafe{io_get_current_irp_stack_location(irp)};
  if stack.is_null(){return}
  let control=unsafe{(*stack).control};
  let status=unsafe{ptr::read_unaligned((*irp).bytes.as_ptr().add(IRP_STATUS_OFFSET).cast::<i32>())};
  let cancelled=unsafe{(*irp).bytes[0x44]!=0};
  let invoke=(status>=0&&(control&0x40)!=0)||(status<0&&(control&0x80)!=0)||(cancelled&&(control&0x20)!=0);
  if invoke {
   if let Some(routine)=unsafe{(*stack).completion_routine} {
    let result=unsafe{routine((*stack).device_object,irp,(*stack).context)};
    if result==0xC0000016u32 as i32{return}
   }
  }
  let current=unsafe{(*irp).bytes[0x43]};
  if current==0xff{return}
  let next=unsafe{stack.cast::<u8>().add(IO_STACK_LOCATION_SIZE_X64).cast::<IoStackLocation>()};
  unsafe { (*irp).bytes[0x43]=current.saturating_add(1); write_ptr((*irp).bytes.as_mut_ptr(),IRP_STACK_OFFSET,next.cast()); }
  if unsafe{(*irp).bytes[0x43]>(*irp).bytes[0x42]} {return}
 }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_mark_irp_pending(irp:*mut Irp) {
 if !irp.is_null() { unsafe { (*irp).bytes[0x41]=1; } }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_set_completion_routine(
 irp:*mut Irp, routine:Option<IoCompletionRoutine>, context:*mut c_void,
 invoke_success:u8, invoke_error:u8, invoke_cancel:u8
) {
 if irp.is_null(){return}
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){return}
 unsafe {
  (*stack).completion_routine=routine;(*stack).context=context;
  (*stack).control=(if invoke_success!=0{0x40}else{0})|(if invoke_error!=0{0x80}else{0})|(if invoke_cancel!=0{0x20}else{0});
 }
}


/// Attach a filter/filesystem device above the current top of a device stack.
/// The returned object is the lower device that receives forwarded IRPs.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_attach_device_to_device_stack(
    source:*mut DeviceObject, target:*mut DeviceObject
)->*mut DeviceObject {
    if source.is_null() || target.is_null() || source==target { return ptr::null_mut(); }
    let mut state=WDM.lock();
    let Some(source_index)=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,source)) else { return ptr::null_mut(); };
    let Some(target_index)=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,target)) else { return ptr::null_mut(); };
    if !state.devices[source_index].attached_to.is_null() { return ptr::null_mut(); }
    let mut top=target;
    for _ in 0..MAX_WDM_DEVICES {
        let next=unsafe{(*top).attached_device};
        if next.is_null() { break; }
        if next==source { return ptr::null_mut(); }
        if !state.devices.iter().any(|s|s.used&&ptr::eq(&s.object,next)) { return ptr::null_mut(); }
        top=next;
    }
    if !unsafe{(*top).attached_device.is_null()} { return ptr::null_mut(); }
    let stack_size=unsafe{(*top).stack_size};
    if stack_size==u8::MAX { return ptr::null_mut(); }
    unsafe {
        (*top).attached_device=source;
        (*source).attached_device=ptr::null_mut();
        (*source).stack_size=stack_size+1;
    }
    state.devices[source_index].attached_to=top;
    let _=target_index;
    top
}

/// Detach the device attached above the supplied lower device, as returned by
/// IoAttachDeviceToDeviceStack. Existing unrelated stack links are preserved.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_detach_device(target:*mut DeviceObject) {
    if target.is_null(){return}
    let mut state=WDM.lock();
    let Some(index)=state.devices.iter().position(|s|s.used&&s.attached_to==target) else{return};
    let source=&mut state.devices[index].object as *mut DeviceObject;
    if unsafe{(*target).attached_device}!=source{return}
    unsafe{(*target).attached_device=ptr::null_mut();(*source).attached_device=ptr::null_mut();(*source).stack_size=1;}
    state.devices[index].attached_to=ptr::null_mut();
}

/// Initialize a caller-allocated WDM IRP and its trailing stack locations.
/// PacketSize must cover the fixed x64 IRP plus StackSize full stack locations.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_initialize_irp(
    irp:*mut Irp, packet_size:u16, stack_size:u8
) {
    if irp.is_null() || stack_size==0 || stack_size>16 {return}
    let required=IRP_SIZE_X64.checked_add(stack_size as usize*IO_STACK_LOCATION_SIZE_X64);
    if required.is_none_or(|n|n>packet_size as usize){return}
    unsafe {
        ptr::write_bytes(irp.cast::<u8>(),0,packet_size as usize);
        ptr::write_unaligned((*irp).bytes.as_mut_ptr().cast::<i16>(),6);
        ptr::write_unaligned((*irp).bytes.as_mut_ptr().add(2).cast::<u16>(),packet_size);
        (*irp).bytes[0x42]=stack_size;
        (*irp).bytes[0x43]=stack_size.saturating_add(1);
        let stack_base=irp.cast::<u8>().add(IRP_SIZE_X64);
        let current=stack_base.add(stack_size as usize*IO_STACK_LOCATION_SIZE_X64);
        write_ptr((*irp).bytes.as_mut_ptr(),IRP_STACK_OFFSET,current.cast());
    }
}

/// Copy the current stack parameters into the next lower driver's stack slot.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_copy_current_irp_stack_location_to_next(irp:*mut Irp) {
    if irp.is_null(){return}
    let state=WDM.lock();
    let Some(slot)=state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
    let count=slot.object.bytes[0x42] as usize;
    if count==0||count>slot.stack.len(){return}
    let current=unsafe{io_get_current_irp_stack_location(irp)};
    let base=slot.stack.as_ptr() as usize;
    let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
    let cur=current as usize;
    if cur<base||cur>=end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return}
    let next=cur.checked_sub(IO_STACK_LOCATION_SIZE_X64).unwrap_or(0);
    if next<base{return}
    unsafe{ptr::copy_nonoverlapping(current, next as *mut IoStackLocation, 1);}
}

/// Arrange for IoCallDriver to reuse the current stack slot instead of
/// consuming another one: advance the current pointer so the dispatch decrement
/// returns it to the same slot.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_skip_current_irp_stack_location(irp:*mut Irp) {
    if irp.is_null(){return}
    let state=WDM.lock();
    let Some(slot)=state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
    let count=slot.object.bytes[0x42] as usize;
    if count==0||count>slot.stack.len(){return}
    let current=unsafe{io_get_current_irp_stack_location(irp)};
    let base=slot.stack.as_ptr() as usize;
    let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
    let cur=current as usize;
    if cur<base||cur>=end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return}
    if cur+IO_STACK_LOCATION_SIZE_X64>end{return}
    unsafe{
        write_ptr((*irp).bytes.as_mut_ptr(),IRP_STACK_OFFSET,(cur+IO_STACK_LOCATION_SIZE_X64) as *mut c_void);
        (*irp).bytes[0x43]=(*irp).bytes[0x43].saturating_add(1);
    }
}


/// Return the highest device object currently attached above the input device.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_get_attached_device(device:*mut DeviceObject)->*mut DeviceObject {
    if device.is_null(){return ptr::null_mut()}
    let state=WDM.lock();
    if !state.devices.iter().any(|s|s.used&&ptr::eq(&s.object,device)){return ptr::null_mut()}
    let mut top=device;
    for _ in 0..MAX_WDM_DEVICES {
        let next=unsafe{(*top).attached_device};
        if next.is_null(){return top}
        if !state.devices.iter().any(|s|s.used&&ptr::eq(&s.object,next)){return ptr::null_mut()}
        top=next;
    }
    ptr::null_mut()
}

/// Return the bottom/base object of an attachment stack.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_get_device_attachment_base_ref(device:*mut DeviceObject)->*mut DeviceObject {
    if device.is_null(){return ptr::null_mut()}
    let state=WDM.lock();
    let Some(mut index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,device)) else{return ptr::null_mut()};
    let mut base=device;
    for _ in 0..MAX_WDM_DEVICES {
        let lower=state.devices[index].attached_to;
        if lower.is_null(){return base}
        let Some(lower_index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,lower)) else{return ptr::null_mut()};
        base=lower;index=lower_index;
    }
    ptr::null_mut()
}

/// Return the next lower object in the stack, or NULL for a base device.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_get_lower_device_object(device:*mut DeviceObject)->*mut DeviceObject {
    if device.is_null(){return ptr::null_mut()}
    let state=WDM.lock();
    let Some(slot)=state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)) else{return ptr::null_mut()};
    slot.attached_to
}

/// Reset an allocated IRP for reuse without releasing its allocation.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_reuse_irp(irp:*mut Irp, status:i32) {
    if irp.is_null(){return}
    let mut state=WDM.lock();
    let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
    let count=slot.object.bytes[0x42] as usize;
    if count==0||count>slot.stack.len(){return}
    slot.stack.fill(IoStackLocation { major_function:0,minor_function:0,flags:0,control:0,parameters:[0;4],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut() });
    slot.object.bytes[0x30..0x34].copy_from_slice(&status.to_le_bytes());
    slot.object.bytes[0x38..0x40].fill(0);
    slot.object.bytes[0x41]=0;
    slot.object.bytes[0x44]=0;
    slot.object.bytes[0x43]=(count+1) as u8;
    let current=unsafe{slot.stack.as_mut_ptr().add(count).cast::<c_void>()};
    unsafe{write_ptr(slot.object.bytes.as_mut_ptr(),IRP_STACK_OFFSET,current);}
}


const MAX_REGISTERED_FILESYSTEMS:usize=16;
struct FileSystemRegistry { devices:[*mut DeviceObject;MAX_REGISTERED_FILESYSTEMS], count:usize }
unsafe impl Send for FileSystemRegistry {}
impl FileSystemRegistry { const fn new()->Self{Self{devices:[ptr::null_mut();MAX_REGISTERED_FILESYSTEMS],count:0}} }
static FILESYSTEMS:IrqSpinLock<FileSystemRegistry>=IrqSpinLock::new(FileSystemRegistry::new());

/// Register a filesystem control device so the I/O manager can discover it.
/// Mount dispatch, VPB allocation and mount-volume arbitration are separate
/// features and are not implied by registration alone.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_register_file_system(device:*mut DeviceObject) {
    if device.is_null(){return}
    let valid={let state=WDM.lock();state.devices.iter().any(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device))};
    if !valid{return}
    let mut registry=FILESYSTEMS.lock();
    if registry.devices[..registry.count].contains(&device){return}
    if registry.count<MAX_REGISTERED_FILESYSTEMS {
        let index=registry.count;
        registry.devices[index]=device;
        registry.count+=1;
    }
}

/// Remove a filesystem from discovery. The caller must first prevent new mount
/// requests and drain existing volumes; this routine only removes registration.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_unregister_file_system(device:*mut DeviceObject) {
    if device.is_null(){return}
    let mut registry=FILESYSTEMS.lock();
    let Some(index)=registry.devices[..registry.count].iter().position(|d|*d==device) else{return};
    for i in index..registry.count-1 {registry.devices[i]=registry.devices[i+1];}
    registry.count-=1;
    registry.devices[registry.count]=ptr::null_mut();
}

pub fn is_file_system_registered(device:*mut DeviceObject)->bool {
    if device.is_null(){return false}
    let registry=FILESYSTEMS.lock();
    registry.devices[..registry.count].contains(&device)
}


#[cfg(test)]
mod wdm_tests {
 use super::*;
 #[test]
 fn x64_wdm_prefix_layouts_match_expected_offsets() {
  assert_eq!(core::mem::size_of::<UnicodeString>(),16);
  assert_eq!(core::mem::size_of::<IoStatusBlock>(),16);
  assert_eq!(core::mem::size_of::<IoStackLocation>(),IO_STACK_LOCATION_SIZE_X64);
  assert_eq!(core::mem::offset_of!(DriverObject,major_function),112);
  assert_eq!(core::mem::offset_of!(DeviceObject,driver_object),8);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_extension),64);
  assert_eq!(core::mem::offset_of!(DeviceObject,stack_size),76);
  assert_eq!(core::mem::offset_of!(DeviceObject,queue),0x50);
  assert_eq!(core::mem::offset_of!(DeviceObject,alignment_requirement),0x98);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_queue),0xA0);
  assert_eq!(core::mem::offset_of!(DeviceObject,dpc),0xC8);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_object_extension),0x138);
  assert_eq!(core::mem::size_of::<DeviceObject>(),0x150);
  assert_eq!(core::mem::size_of::<Irp>(),IRP_SIZE_X64);
  assert_eq!(core::mem::size_of::<DeviceObject>(),0x150);
  assert_eq!(IRP_STACK_OFFSET,0xB8);
  assert_eq!(IRP_STACK_OFFSET,0xB8);
  assert_eq!(IRP_STATUS_OFFSET,0x30);
  assert_eq!(IRP_INFORMATION_OFFSET,0x38);
 }
 unsafe extern "system" fn test_dispatch(device:*mut DeviceObject, irp:*mut Irp)->i32 {
  if device.is_null() || irp.is_null() { return STATUS_INVALID_PARAMETER; }
  unsafe { (*irp).bytes[IRP_STATUS_OFFSET..IRP_STATUS_OFFSET+4].copy_from_slice(&STATUS_SUCCESS.to_le_bytes()); }
  STATUS_SUCCESS
 }
 #[test]
 fn device_dispatch_uses_wdm_device_object_signature() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device: *mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0]=Some(test_dispatch);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  let next=unsafe{io_get_next_irp_stack_location(irp)};
  assert!(!next.is_null());
  unsafe { (*next).major_function=0; }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert_eq!(unsafe{(*irp).bytes[IRP_STATUS_OFFSET]},0);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }
 #[test]
 fn create_delete_device_and_allocate_irp_stack() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device: *mut DeviceObject=ptr::null_mut();
  let status=unsafe{io_create_device(&mut driver,32,ptr::null_mut(),7,0,0,&mut device)};
  assert_eq!(status,STATUS_SUCCESS);
  assert!(!device.is_null());
  assert_eq!(unsafe{(*device).device_extension.is_null()},false);
  assert_eq!(unsafe{(*device).size},(core::mem::size_of::<DeviceObject>()+32) as u16);
  assert_eq!(driver.device_object,device);
  unsafe{io_delete_device(device);}
  assert!(driver.device_object.is_null());

  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  assert!(!unsafe{io_get_current_irp_stack_location(irp)}.is_null());
  let next=unsafe{io_get_next_irp_stack_location(irp)};
  assert!(!next.is_null());
  assert_eq!(next as usize,irp as usize+IRP_SIZE_X64+IO_STACK_LOCATION_SIZE_X64);
  unsafe{io_free_irp(irp);}
 }
 #[test]
 fn attach_detach_tracks_filter_stack_and_stack_depth() {
  let mut lower_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut upper_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut lower:*mut DeviceObject=ptr::null_mut();
  let mut upper:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut lower_driver,0,ptr::null_mut(),7,0,0,&mut lower)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut upper_driver,0,ptr::null_mut(),7,0,0,&mut upper)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_attach_device_to_device_stack(upper,lower)},lower);
  assert_eq!(unsafe{(*lower).attached_device},upper);
  assert_eq!(unsafe{(*upper).stack_size},2);
  unsafe{io_detach_device(lower);}
  assert!(unsafe{(*lower).attached_device.is_null()});
  assert_eq!(unsafe{(*upper).stack_size},1);
  unsafe{io_delete_device(upper);io_delete_device(lower);}
 }

 #[test]
 fn initialize_irp_rejects_short_packet_and_sets_stack_bounds() {
  let mut storage=[0u8;IRP_SIZE_X64+2*IO_STACK_LOCATION_SIZE_X64];
  let irp=storage.as_mut_ptr().cast::<Irp>();
  unsafe{io_initialize_irp(irp, (storage.len()-1) as u16, 2);}
  assert_eq!(unsafe{(*irp).bytes[0x42]},0);
  unsafe{io_initialize_irp(irp, storage.len() as u16, 2);}
  assert_eq!(unsafe{(*irp).bytes[0x42]},2);
  assert_eq!(unsafe{io_get_current_irp_stack_location(irp)} as usize,storage.as_ptr() as usize+storage.len());
 }

 #[test]
 fn attachment_queries_and_irp_reuse_preserve_allocation() {
  let mut lower_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut upper_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut lower:*mut DeviceObject=ptr::null_mut();
  let mut upper:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut lower_driver,0,ptr::null_mut(),7,0,0,&mut lower)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut upper_driver,0,ptr::null_mut(),7,0,0,&mut upper)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_attach_device_to_device_stack(upper,lower)},lower);
  assert_eq!(unsafe{io_get_attached_device(lower)},upper);
  assert_eq!(unsafe{io_get_device_attachment_base_ref(upper)},lower);
  assert_eq!(unsafe{io_get_lower_device_object(upper)},lower);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe{io_reuse_irp(irp,STATUS_SUCCESS);}
  assert_eq!(unsafe{(*irp).bytes[0x43]},3);
  assert_eq!(unsafe{io_get_current_irp_stack_location(irp)} as usize,irp as usize+IRP_SIZE_X64+2*IO_STACK_LOCATION_SIZE_X64);
  unsafe{io_free_irp(irp);io_detach_device(lower);io_delete_device(upper);io_delete_device(lower);}
 }

 #[test]
 fn file_system_registration_is_bounded_idempotent_and_removable() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  unsafe{io_register_file_system(device);io_register_file_system(device);}
  assert!(is_file_system_registered(device));
  unsafe{io_unregister_file_system(device);}
  assert!(!is_file_system_registered(device));
  unsafe{io_delete_device(device);}
 }

}

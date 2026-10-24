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
const IRP_STACK_OFFSET: usize = 0xB0;
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
#[repr(C)]
pub struct DeviceObject {
    pub object_type: i16, pub size: u16, pub reference_count: i32,
    pub driver_object: *mut DriverObject, pub next_device: *mut DeviceObject,
    pub attached_device: *mut DeviceObject, pub current_irp: *mut Irp,
    pub timer: *mut c_void, pub flags: u32, pub characteristics: u32,
    pub vpb: *mut c_void, pub device_extension: *mut c_void,
    pub device_type: u32, pub stack_size: u8, pub queue_padding: [u8; 7],
    pub queue: [usize; 4], pub alignment_requirement: u32,
    pub device_queue: [usize; 6], pub dpc: [usize; 8],
    pub device_object_extension: *mut c_void, pub reserved: *mut c_void, pub reserved_tail: [u8; 80],
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
struct DeviceSlot { used: bool, deleting: bool, object: DeviceObject, extension: [u8; MAX_DEVICE_EXTENSION] }
unsafe impl Send for DeviceSlot {}
struct IrpSlot { used: bool, object: Irp, stack: [IoStackLocation;16] }
unsafe impl Send for IrpSlot {}
struct WdmState { devices: [DeviceSlot; MAX_WDM_DEVICES], irps: [IrpSlot; MAX_WDM_IRPS] }
impl WdmState {
 const fn new() -> Self {
  Self {
   devices: [const { DeviceSlot { used:false, deleting:false, object: DeviceObject {
    object_type:3,size:core::mem::size_of::<DeviceObject>() as u16,reference_count:1,
    driver_object:ptr::null_mut(),next_device:ptr::null_mut(),attached_device:ptr::null_mut(),
    current_irp:ptr::null_mut(),timer:ptr::null_mut(),flags:0,characteristics:0,vpb:ptr::null_mut(),
    device_extension:ptr::null_mut(),device_type:0,stack_size:1,queue_padding:[0;7],queue:[0;4],
    alignment_requirement:0,device_queue:[0;6],dpc:[0;8],device_object_extension:ptr::null_mut(),reserved:ptr::null_mut(),reserved_tail:[0;80]
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
 slot.object.size=core::mem::size_of::<DeviceObject>() as u16;
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
 let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return};
 slot.deleting=true;
 let driver=slot.object.driver_object;
 if !driver.is_null() {
  unsafe {
   let mut cursor=ptr::addr_of_mut!((*driver).device_object);
   while !(*cursor).is_null() {
    if *cursor==device { *cursor=(*device).next_device;break; }
    cursor=ptr::addr_of_mut!((**cursor).next_device);
   }
  }
 }
 slot.object.device_extension=ptr::null_mut();slot.object.driver_object=ptr::null_mut();
 slot.object.next_device=ptr::null_mut();slot.used=false;slot.deleting=false;
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_allocate_irp(stack_size:i16, charge_quota:u8)->*mut Irp {
 if stack_size<=0||stack_size>16||charge_quota>1{return ptr::null_mut()}
 let mut state=WDM.lock();
 let Some(slot)=state.irps.iter_mut().find(|s|!s.used) else{return ptr::null_mut()};
 slot.used=true;slot.object.bytes.fill(0);
 slot.object.bytes[0..2].copy_from_slice(&6i16.to_le_bytes());
 slot.object.bytes[2..4].copy_from_slice(&(IRP_SIZE_X64 as u16).to_le_bytes());
 slot.object.bytes[0x42]=stack_size as u8;slot.object.bytes[0x43]=(stack_size+1) as u8;
 slot.stack.fill(IoStackLocation { major_function:0,minor_function:0,flags:0,control:0,parameters:[0;4],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut() });
 let object=&mut slot.object as *mut Irp;
 let one_past=unsafe{slot.stack.as_mut_ptr().add(stack_size as usize).cast::<c_void>()};
 unsafe { write_ptr(object.bytes.as_mut_ptr(),IRP_STACK_OFFSET,one_past); }
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
  assert_eq!(core::mem::size_of::<DeviceObject>(),0x150);
  assert_eq!(core::mem::size_of::<Irp>(),IRP_SIZE_X64);
  assert_eq!(IRP_STACK_OFFSET,0xB0);
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
  assert_eq!(driver.device_object,device);
  unsafe{io_delete_device(device);}
  assert!(driver.device_object.is_null());

  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  assert!(!unsafe{io_get_current_irp_stack_location(irp)}.is_null());
  assert!(!unsafe{io_get_next_irp_stack_location(irp)}.is_null());
  unsafe{io_free_irp(irp);}
 }
}

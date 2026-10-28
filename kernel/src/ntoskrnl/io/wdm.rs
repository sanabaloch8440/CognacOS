//! Partial Windows x64 WDM object/IRP ABI foundation.
//!
//! Layouts and dispatch conventions here follow the public x64 WDK ABI for the
//! fields used by this implementation. This is not a claim of complete WDM
//! compatibility: dynamic IRP records/system buffers and the WDM cancel-safe
//! queue API are implemented, but cancel-spin-lock parity, all driver-created IRP
//! ownership variants, security descriptors, VPBs, and many IRP variants remain
//! incomplete. Driver images must not be treated as generally loadable.
use core::{ffi::c_void, ptr, ops::{Index, IndexMut}};
use crate::hal::IrqSpinLock;

pub use crate::nt_abi::ntstatus::{
    STATUS_DELETE_PENDING, STATUS_DEVICE_BUSY, STATUS_INSUFFICIENT_RESOURCES,
    STATUS_INVALID_DEVICE_REQUEST, STATUS_INVALID_PARAMETER, STATUS_OBJECT_NAME_COLLISION,
    STATUS_OBJECT_NAME_NOT_FOUND, STATUS_PENDING, STATUS_SUCCESS,
};
pub const IRP_SIZE_X64: usize = 0xD0;
pub const IO_STACK_LOCATION_SIZE_X64: usize = 0x48;
const MAX_WDM_DEVICES: usize = 32;
const IRP_CANCEL_BATCH: usize = 64;
const MAX_SYNC_IRP_THREADS:usize=128;
const MAX_WDM_STACK_LOCATIONS: usize = 127;
const MAX_DEVICE_EXTENSION: usize = 512;
const IRP_STACK_OFFSET: usize = 0xB8;
const IRP_STATUS_OFFSET: usize = 0x30;
const IRP_INFORMATION_OFFSET: usize = 0x38;

/// Canonical counted UTF-16 string ABI shared with NTDLL and driver headers.
pub type UnicodeString = crate::nt_abi::object::UNICODE_STRING;
/// Use the canonical Windows IO_STATUS_BLOCK union layout shared with NTDLL and drivers.
pub type IoStatusBlock = crate::nt_abi::io::IO_STATUS_BLOCK;
pub type DriverDispatch = unsafe extern "system" fn(*mut DeviceObject, *mut Irp) -> i32;
pub type DriverUnload = unsafe extern "system" fn(*mut DriverObject);
pub type IoCompletionRoutine = unsafe extern "system" fn(*mut DeviceObject, *mut Irp, *mut c_void) -> i32;
pub type DriverCancelRoutine = unsafe extern "system" fn(*mut DeviceObject, *mut Irp);

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
    pub device_type: u32, pub stack_size: i8, pub queue_padding: [u8; 3],
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
    pub parameters: [u8; 0x20], pub device_object: *mut DeviceObject,
    pub file_object: *mut c_void, pub completion_routine: Option<IoCompletionRoutine>,
    pub context: *mut c_void,
}
#[repr(C)]
struct DeviceSlot { used: bool, deleting: bool, active_dispatches:u32, outstanding_irps:u32, remove_lock_count: u32, power_state: u8, power_target: u8, power_transition: bool, pnp_state: u8, pnp_pending: u8, pnp_transition_minor: u8, pnp_transition: bool, attached_to: *mut DeviceObject, name_len: usize, name: [u16; 260], object: DeviceObject, extension: [u8; MAX_DEVICE_EXTENSION] }
unsafe impl Send for DeviceSlot {}
#[repr(C)]
const PAGE_SIZE:usize=4096;
struct IrpSlot {
 next:*mut IrpSlot,
 used: bool, owned_mdl: *mut c_void, completion_state:u8, suspended_completion:bool, free_requested:bool, active_ops:u32,
 outstanding_count:usize, outstanding_devices:[*mut DeviceObject;MAX_WDM_STACK_LOCATIONS],
 object: Irp, stack: [IoStackLocation;MAX_WDM_STACK_LOCATIONS],
 system_buffer:*mut u8, system_buffer_pages:usize, system_buffer_len:usize,
 ioctl_output_buffer:*mut c_void, ioctl_output_len:usize, built_sync_ioctl:bool,
 sync_thread_id:u64, sync_event:*mut c_void, sync_next:*mut Irp, sync_prev:*mut Irp, sync_queued:bool,
}
impl IrpSlot {
 const fn empty()->Self { Self {
  next:ptr::null_mut(),used:false,owned_mdl:ptr::null_mut(),completion_state:0,
  suspended_completion:false,free_requested:false,active_ops:0,outstanding_count:0,
  outstanding_devices:[ptr::null_mut();MAX_WDM_STACK_LOCATIONS],
  object:Irp{bytes:[0;IRP_SIZE_X64]},
  stack:[IoStackLocation{major_function:0,minor_function:0,flags:0,control:0,parameters:[0;0x20],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut()};MAX_WDM_STACK_LOCATIONS],
  system_buffer:ptr::null_mut(),system_buffer_pages:0,system_buffer_len:0,
  ioctl_output_buffer:ptr::null_mut(),ioctl_output_len:0,built_sync_ioctl:false,
  sync_thread_id:0,sync_event:ptr::null_mut(),sync_next:ptr::null_mut(),sync_prev:ptr::null_mut(),sync_queued:false,
 }}
}
unsafe impl Send for IrpSlot {}
struct IrpIter<'a>{next:*mut IrpSlot,_marker:core::marker::PhantomData<&'a IrpSlot>}
impl<'a> Iterator for IrpIter<'a>{type Item=&'a IrpSlot;fn next(&mut self)->Option<Self::Item>{if self.next.is_null(){return None}let p=self.next;self.next=unsafe{(*p).next};Some(unsafe{&*p})}}
struct IrpIterMut<'a>{next:*mut IrpSlot,_marker:core::marker::PhantomData<&'a mut IrpSlot>}
impl<'a> Iterator for IrpIterMut<'a>{type Item=&'a mut IrpSlot;fn next(&mut self)->Option<Self::Item>{if self.next.is_null(){return None}let p=self.next;self.next=unsafe{(*p).next};Some(unsafe{&mut *p})}}
struct IrpTable{head:*mut IrpSlot,len:usize}
impl IrpTable{
 const fn new()->Self{Self{head:ptr::null_mut(),len:0}}
 fn iter(&self)->IrpIter<'_>{IrpIter{next:self.head,_marker:core::marker::PhantomData}}
 fn iter_mut(&mut self)->IrpIterMut<'_>{IrpIterMut{next:self.head,_marker:core::marker::PhantomData}}
 fn len(&self)->usize{self.len}
 fn push_front(&mut self,slot:*mut IrpSlot){unsafe{(*slot).next=self.head;}self.head=slot;self.len+=1;}
}
unsafe impl Send for IrpTable {}
unsafe impl Sync for IrpTable {}
impl Index<usize> for IrpTable{type Output=IrpSlot;fn index(&self,index:usize)->&Self::Output{assert!(index<self.len);let mut p=self.head;for _ in 0..index{p=unsafe{(*p).next};}unsafe{&*p}}}
impl IndexMut<usize> for IrpTable{fn index_mut(&mut self,index:usize)->&mut Self::Output{assert!(index<self.len);let mut p=self.head;for _ in 0..index{p=unsafe{(*p).next};}unsafe{&mut *p}}}
#[derive(Clone,Copy)]
struct ThreadIrpQueue { used:bool,thread_id:u64,head:*mut Irp,tail:*mut Irp }
impl ThreadIrpQueue { const EMPTY:Self=Self{used:false,thread_id:0,head:ptr::null_mut(),tail:ptr::null_mut()}; }
struct WdmState { devices: [DeviceSlot; MAX_WDM_DEVICES], irps: IrpTable, thread_irps:[ThreadIrpQueue;MAX_SYNC_IRP_THREADS] }
impl WdmState {
 const fn new() -> Self {
  Self {
   devices: [const { DeviceSlot { used:false, deleting:false, active_dispatches:0, outstanding_irps:0, remove_lock_count:0, power_state:0, power_target:0, power_transition:false, pnp_state:0, pnp_pending:0, pnp_transition_minor:0, pnp_transition:false, attached_to:ptr::null_mut(), name_len:0, name:[0;260], object: DeviceObject {
    object_type:3,size:core::mem::size_of::<DeviceObject>() as u16,reference_count:1,
    driver_object:ptr::null_mut(),next_device:ptr::null_mut(),attached_device:ptr::null_mut(),
    current_irp:ptr::null_mut(),timer:ptr::null_mut(),flags:0,characteristics:0,vpb:ptr::null_mut(),
    device_extension:ptr::null_mut(),device_type:0,stack_size:1,queue_padding:[0;3],queue:[0;0x48],
    alignment_requirement:0,device_queue_padding:[0;4],device_queue:[0;0x28],dpc:[0;0x40],active_thread_count:0,
    security_descriptor:ptr::null_mut(),device_lock:[0;0x18],sector_size:0,spare1:0,
    device_object_extension:ptr::null_mut(),reserved:ptr::null_mut()
   },extension:[0;MAX_DEVICE_EXTENSION] } }; MAX_WDM_DEVICES],
   irps: IrpTable::new(),
   thread_irps:[const{ThreadIrpQueue::EMPTY};MAX_SYNC_IRP_THREADS],
  }
 }
}
static WDM: IrqSpinLock<WdmState> = IrqSpinLock::new(WdmState::new());
/// Link/unlink builder-created synchronous IRPs on a per-thread intrusive
/// queue. The WDM lock serializes queue membership with completion and exit.
fn sync_queue_attach_locked(state:&mut WdmState,irp:*mut Irp,thread_id:u64)->bool {
 if irp.is_null(){return false}
 let Some(index)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,irp)) else{return false};
 if thread_id==0 {state.irps[index].sync_thread_id=0;state.irps[index].sync_queued=false;return true}
 if state.irps[index].sync_queued{return state.irps[index].sync_thread_id==thread_id}
 let queue_index=state.thread_irps.iter().position(|q|q.used&&q.thread_id==thread_id)
  .or_else(||state.thread_irps.iter().position(|q|!q.used));
 let Some(qi)=queue_index else{return false};
 if !state.thread_irps[qi].used {
  state.thread_irps[qi]=ThreadIrpQueue{used:true,thread_id,head:ptr::null_mut(),tail:ptr::null_mut()};
 }
 let tail=state.thread_irps[qi].tail;
 if !tail.is_null(){
  let Some(tail_index)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,tail)) else{return false};
  state.irps[tail_index].sync_next=irp;
 }
 state.irps[index].sync_thread_id=thread_id;
 state.irps[index].sync_prev=tail;
 state.irps[index].sync_next=ptr::null_mut();
 state.irps[index].sync_queued=true;
 if state.thread_irps[qi].head.is_null(){state.thread_irps[qi].head=irp;}
 state.thread_irps[qi].tail=irp;
 true
}
/// Detach a synchronous IRP and return its completion event. Caller owns WDM lock.
fn sync_queue_detach_locked(state:&mut WdmState,irp:*mut Irp)->*mut c_void {
 let Some(index)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,irp)) else{return ptr::null_mut()};
 let thread_id=state.irps[index].sync_thread_id;
 let prev=state.irps[index].sync_prev;
 let next=state.irps[index].sync_next;
 let queued=state.irps[index].sync_queued;
 let event=state.irps[index].sync_event;
 if queued {
  if let Some(qi)=state.thread_irps.iter().position(|q|q.used&&q.thread_id==thread_id) {
   if prev.is_null(){state.thread_irps[qi].head=next;}else if let Some(pi)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,prev)){state.irps[pi].sync_next=next;}
   if next.is_null(){state.thread_irps[qi].tail=prev;}else if let Some(ni)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,next)){state.irps[ni].sync_prev=prev;}
   if state.thread_irps[qi].head.is_null(){state.thread_irps[qi]=ThreadIrpQueue::EMPTY;}
  }
 }
 state.irps[index].sync_thread_id=0;
 state.irps[index].sync_event=ptr::null_mut();
 state.irps[index].sync_next=ptr::null_mut();
 state.irps[index].sync_prev=ptr::null_mut();
 state.irps[index].sync_queued=false;
 event
}

// Private high bits prevent new device creation after unload begins.
const DRIVER_UNLOADING_FLAG:u32=1<<31;
const DRIVER_UNLOADED_FLAG:u32=1<<30;

pub const DEVICE_POWER_D0:u8=0; pub const DEVICE_POWER_D1:u8=1; pub const DEVICE_POWER_D2:u8=2; pub const DEVICE_POWER_D3:u8=3;




#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_reference_device_object(device:*mut DeviceObject)->*mut DeviceObject {
 if device.is_null(){return ptr::null_mut()}
 let mut state=WDM.lock();
 let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)&&!s.deleting) else{return ptr::null_mut()};
 let Some(next)=slot.object.reference_count.checked_add(1) else{return ptr::null_mut()};
 slot.object.reference_count=next;device
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_dereference_device_object(device:*mut DeviceObject) {
 if device.is_null(){return}
 let finalize={
  let mut state=WDM.lock();
  let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return};
  if s.object.reference_count>1{s.object.reference_count-=1;}
  s.deleting&&s.object.reference_count<=1&&s.remove_lock_count==0&&s.active_dispatches==0&&!s.power_transition
 };
 if finalize{unsafe{io_delete_device(device)}}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_acquire_device_remove_lock(device:*mut DeviceObject)->i32 {
 if device.is_null(){return STATUS_INVALID_PARAMETER}
 let mut state=WDM.lock();
 let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
 if s.deleting{return STATUS_DELETE_PENDING}
 let Some(n)=s.remove_lock_count.checked_add(1) else{return STATUS_INSUFFICIENT_RESOURCES};
 s.remove_lock_count=n;STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_release_device_remove_lock(device:*mut DeviceObject) {
 if device.is_null(){return}
 let finalize={
  let mut state=WDM.lock();
  let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return};
  if s.remove_lock_count==0{return}
  s.remove_lock_count-=1;s.deleting&&s.remove_lock_count==0&&s.active_dispatches==0&&s.object.reference_count<=1&&!s.power_transition
 };
 if finalize{unsafe{io_delete_device(device)}}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_begin_device_power_transition(device:*mut DeviceObject,target:u8)->i32 {
 if device.is_null()||target>DEVICE_POWER_D3{return STATUS_INVALID_PARAMETER}
 let mut state=WDM.lock();
 let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
 if s.deleting{return STATUS_DELETE_PENDING}
 if s.power_transition{return STATUS_DEVICE_BUSY}
 s.power_target=target;s.power_transition=true;STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_complete_device_power_transition(device:*mut DeviceObject,target:u8,status:i32)->i32 {
 if device.is_null()||target>DEVICE_POWER_D3{return STATUS_INVALID_PARAMETER}
 let finalize={
  let mut state=WDM.lock();
  let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
  if !s.power_transition||s.power_target!=target{return STATUS_INVALID_DEVICE_REQUEST}
  if status==STATUS_SUCCESS{s.power_state=target}
  s.power_transition=false;s.deleting&&s.remove_lock_count==0&&s.active_dispatches==0&&s.object.reference_count<=1
 };
 if finalize{unsafe{io_delete_device(device)}}status
}

pub(crate) const PNP_STATE_NOT_STARTED:u8=0;
pub(crate) const PNP_STATE_STARTED:u8=1;
pub(crate) const PNP_STATE_STOPPED:u8=2;
pub(crate) const PNP_STATE_REMOVED:u8=3;
pub(crate) const PNP_STATE_SURPRISE_REMOVED:u8=4;
/// Only lifecycle-changing minors reserve a transition. Other PnP queries
/// are dispatched without mutating the device's lifecycle state.
fn pnp_minor_changes_lifecycle(minor:u8)->bool { matches!(minor,0x00..=0x06|0x17) }
pub(crate) unsafe fn io_begin_pnp_transition(device:*mut DeviceObject,minor:u8)->i32 {
 if device.is_null(){return STATUS_INVALID_PARAMETER}
 if !pnp_minor_changes_lifecycle(minor){return STATUS_SUCCESS}
 let mut state=WDM.lock();
 let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
 if s.deleting||s.pnp_state==PNP_STATE_REMOVED{return STATUS_DELETE_PENDING}
 if s.pnp_transition{return STATUS_DEVICE_BUSY}
 let valid=match minor {
  0x00=>s.pnp_state==PNP_STATE_NOT_STARTED||s.pnp_state==PNP_STATE_STOPPED,
  0x01|0x05=>s.pnp_state==PNP_STATE_STARTED&&s.pnp_pending==0,
  0x02|0x17=>s.pnp_state!=PNP_STATE_REMOVED,
  0x03=>s.pnp_state==PNP_STATE_STARTED&&s.pnp_pending==0x01,
  0x04=>s.pnp_state==PNP_STATE_STARTED&&s.pnp_pending==0,
  0x06=>s.pnp_state==PNP_STATE_STARTED&&s.pnp_pending==0x05,
  _=>true
 };
 if !valid{return STATUS_INVALID_DEVICE_REQUEST}
 s.pnp_transition=true;s.pnp_transition_minor=minor;STATUS_SUCCESS
}
pub(crate) unsafe fn io_complete_pnp_transition(device:*mut DeviceObject,minor:u8,status:i32)->i32 {
 if device.is_null(){return STATUS_INVALID_PARAMETER}
 if !pnp_minor_changes_lifecycle(minor){return STATUS_SUCCESS}
 let mut state=WDM.lock();
 let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
 if !s.pnp_transition||s.pnp_transition_minor!=minor{return STATUS_INVALID_DEVICE_REQUEST}
 // Only a terminal successful IRP completion may publish the new state.
 if status==STATUS_SUCCESS && (!s.deleting||matches!(minor,0x02|0x17)) {match minor {
  0x00=>{s.pnp_state=PNP_STATE_STARTED;s.pnp_pending=0;},
  0x01|0x05=>s.pnp_pending=minor,
  0x02=>{s.pnp_state=PNP_STATE_REMOVED;s.pnp_pending=0;},
  0x17=>{s.pnp_state=PNP_STATE_SURPRISE_REMOVED;s.pnp_pending=0;},
  0x03|0x06=>s.pnp_pending=0,
  0x04=>{s.pnp_state=PNP_STATE_STOPPED;s.pnp_pending=0;},
  _=>{}
 }}
 s.pnp_transition=false;STATUS_SUCCESS
}

/// Bounded CM_RESOURCE_LIST parser and resource arbiter for WDM start IRPs.
/// The caller must pass kernel-captured CM_RESOURCE_LIST pointers, never raw
/// user pointers. This reserves translated I/O ports, memory ranges and IRQs;
/// it does not program the underlying bus or replace a hardware bus driver.
const MAX_PNP_RESOURCE_CLAIMS:usize=256;
const MAX_RESOURCES_PER_DEVICE:usize=32;
const MAX_PNP_FULL_DESCRIPTORS:usize=16;
const MAX_PNP_PARTIAL_DESCRIPTORS:usize=64;
const IRP_MN_START_DEVICE:u8=0x00;
const IRP_MN_STOP_DEVICE:u8=0x04;
const IRP_MN_REMOVE_DEVICE:u8=0x02;
const IRP_MN_SURPRISE_REMOVAL:u8=0x17;
const CM_RESOURCE_PORT:u8=1;
const CM_RESOURCE_INTERRUPT:u8=2;
const CM_RESOURCE_MEMORY:u8=3;
#[derive(Clone,Copy)]
struct ResourceClaim { used:bool, owner:*mut DeviceObject, irp:*mut Irp, kind:u8, start:u64, length:u64, committed:bool }
unsafe impl Send for ResourceClaim {}
impl ResourceClaim { const EMPTY:Self=Self{used:false,owner:ptr::null_mut(),irp:ptr::null_mut(),kind:0,start:0,length:0,committed:false}; }
struct ResourceArbiter { claims:[ResourceClaim;MAX_PNP_RESOURCE_CLAIMS] }
impl ResourceArbiter { const fn new()->Self{Self{claims:[ResourceClaim::EMPTY;MAX_PNP_RESOURCE_CLAIMS]}} }
static RESOURCE_ARBITER:IrqSpinLock<ResourceArbiter>=IrqSpinLock::new(ResourceArbiter::new());
#[derive(Clone,Copy)]
struct ParsedResource { kind:u8, start:u64, length:u64 }
impl ParsedResource { const EMPTY:Self=Self{kind:0,start:0,length:0}; }

fn read_u16_at(base:*const u8,off:usize)->u16 { unsafe{ptr::read_unaligned(base.add(off).cast::<u16>())} }
fn read_u32_at(base:*const u8,off:usize)->u32 { unsafe{ptr::read_unaligned(base.add(off).cast::<u32>())} }
fn read_u64_at(base:*const u8,off:usize)->u64 { unsafe{ptr::read_unaligned(base.add(off).cast::<u64>())} }

/// Validate a WDK CM_RESOURCE_LIST and extract resources that participate in
/// exclusive range arbitration. Structure bounds/counts are capped to avoid
/// unbounded traversal. As with the WDK ABI, the pointer must reference a
/// readable kernel allocation for the duration of this call.
unsafe fn parse_cm_resource_list(list:*const c_void,out:&mut[ParsedResource;MAX_RESOURCES_PER_DEVICE])->Result<usize,i32>{
 if list.is_null(){return Ok(0)}
 let base=list.cast::<u8>();
 let full_count=unsafe{read_u32_at(base,0)} as usize;
 if full_count>MAX_PNP_FULL_DESCRIPTORS{return Err(STATUS_INVALID_PARAMETER)}
 let mut cursor=4usize;let mut count=0usize;
 for _ in 0..full_count {
  let partial_count=unsafe{read_u32_at(base,cursor+12)} as usize;
  if partial_count>MAX_PNP_PARTIAL_DESCRIPTORS{return Err(STATUS_INVALID_PARAMETER)}
  let descriptor_start=cursor.checked_add(16).ok_or(STATUS_INVALID_PARAMETER)?;
  let bytes=partial_count.checked_mul(16).ok_or(STATUS_INVALID_PARAMETER)?;
  let end=descriptor_start.checked_add(bytes).ok_or(STATUS_INVALID_PARAMETER)?;
  for i in 0..partial_count {
   let off=descriptor_start+i*16;
   let kind=unsafe{*base.add(off)};
   match kind {
    0|5|6=>{}, // Null, device-specific, and bus-number resources are not global ranges.
    4=>{
     let channel=unsafe{read_u32_at(base,off+4)} as u64;
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind:4,start:channel,length:1};count+=1;
    }
    CM_RESOURCE_PORT|CM_RESOURCE_MEMORY=>{
     let start=unsafe{read_u64_at(base,off+4)};
     let length=unsafe{read_u32_at(base,off+12)} as u64;
     if length==0||start.checked_add(length).is_none(){return Err(STATUS_INVALID_PARAMETER)}
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind,start,length};count+=1;
    }
    CM_RESOURCE_INTERRUPT=>{
     let vector=unsafe{read_u32_at(base,off+8)} as u64;
     if vector==0{return Err(STATUS_INVALID_PARAMETER)}
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind,start:vector,length:1};count+=1;
    }
    7=>return Err(STATUS_INVALID_DEVICE_REQUEST), // Large-memory encoding needs architecture-specific decoding.
    _=>return Err(STATUS_INVALID_PARAMETER),
   }
  }
  cursor=end;
 }
 Ok(count)
}
fn resources_overlap(a:&ParsedResource,b:&ParsedResource)->bool{
 if a.kind!=b.kind{return false}
 if a.kind==CM_RESOURCE_INTERRUPT{return a.start==b.start}
 let a_end=a.start.saturating_add(a.length);
 let b_end=b.start.saturating_add(b.length);
 a.start<b_end&&b.start<a_end
}

/// Reserve translated resources for an IRP_MN_START_DEVICE transaction.
/// Reservations are tentative until the IRP's terminal completion succeeds.
pub(crate) unsafe fn io_prepare_pnp_resources(device:*mut DeviceObject,irp:*mut Irp,minor:u8,allocated:*const c_void,translated:*const c_void)->i32{
 if minor!=IRP_MN_START_DEVICE{return STATUS_SUCCESS}
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 // Resource arbitration is performed once at the top of the stack and claims
 // belong to the attachment base (PDO), even when a filter receives START first.
 if unsafe{io_get_attached_device(device)}!=device{return STATUS_SUCCESS}
 let resource_owner=unsafe{io_get_device_attachment_base_ref(device)};
 if resource_owner.is_null(){return STATUS_INVALID_PARAMETER}
 let mut raw=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
 let mut resources=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
 let raw_count=match unsafe{parse_cm_resource_list(allocated,&mut raw)}{Ok(n)=>n,Err(e)=>return e};
 let count=match unsafe{parse_cm_resource_list(translated,&mut resources)}{Ok(n)=>n,Err(e)=>return e};
 if allocated.is_null()!=translated.is_null()||raw_count!=count{return STATUS_INVALID_PARAMETER}
 // Require the bus-provided raw and translated lists to describe the same
 // number of reservable resources; translation may change addresses, not count.
 for i in 0..count { for j in i+1..count { if resources_overlap(&resources[i],&resources[j]) { return STATUS_INVALID_PARAMETER; } } }
 let mut arbiter=RESOURCE_ARBITER.lock();
 if arbiter.claims.iter().any(|c|c.used&&c.owner==resource_owner&&c.irp==irp&&!c.committed){return STATUS_SUCCESS}
 if arbiter.claims.iter().any(|c|c.used&&c.owner==resource_owner&&!c.committed){return STATUS_DEVICE_BUSY}
 for r in resources[..count].iter() {
  for claim in arbiter.claims.iter().filter(|c|c.used&&c.owner!=device) {
   let existing=ParsedResource{kind:claim.kind,start:claim.start,length:claim.length};
   if resources_overlap(r,&existing){return STATUS_DEVICE_BUSY}
  }
 }
 let free_count=arbiter.claims.iter().filter(|c|!c.used).count();
 if free_count<count{return STATUS_INSUFFICIENT_RESOURCES}
 for r in resources[..count].iter() {
  let Some(slot)=arbiter.claims.iter_mut().find(|c|!c.used) else{return STATUS_INSUFFICIENT_RESOURCES};
  *slot=ResourceClaim{used:true,owner:resource_owner,irp,kind:r.kind,start:r.start,length:r.length,committed:false};
 }
 STATUS_SUCCESS
}

/// Resolve a PnP resource transaction only when IoCompleteRequest reaches the
/// terminal IRP unwind. Failed/cancelled starts drop tentative claims; successful
/// starts replace prior claims. Stop/remove release committed reservations.
pub(crate) fn io_finish_pnp_resources(irp:*mut Irp,device:*mut DeviceObject,minor:u8,status:i32){
 if irp.is_null()||device.is_null(){return}
 // Only the top-of-stack completion owns the transaction. Claims are attached
 // to the stack base so filters cannot double-reserve or prematurely release them.
 if unsafe{io_get_attached_device(device)}!=device{return}
 let resource_owner=unsafe{io_get_device_attachment_base_ref(device)};
 if resource_owner.is_null(){return}
 let device_deleting={let state=WDM.lock();state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)).is_none_or(|s|s.deleting)};
 let effective_status=if minor==IRP_MN_START_DEVICE&&device_deleting{STATUS_DELETE_PENDING}else{status};
 let mut arbiter=RESOURCE_ARBITER.lock();
 if minor==IRP_MN_START_DEVICE {
  let has_pending=arbiter.claims.iter().any(|c|c.used&&c.irp==irp&&c.owner==resource_owner&&!c.committed);
  if effective_status==STATUS_SUCCESS {
   for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==resource_owner&&c.committed){*c=ResourceClaim::EMPTY;}
   for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.irp==irp&&c.owner==resource_owner&&!c.committed){c.committed=true;c.irp=ptr::null_mut();}
  } else if has_pending {
   for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.irp==irp&&c.owner==device&&!c.committed){*c=ResourceClaim::EMPTY;}
  }
 } else if status==STATUS_SUCCESS&&(minor==IRP_MN_STOP_DEVICE||minor==IRP_MN_REMOVE_DEVICE||minor==IRP_MN_SURPRISE_REMOVAL) {
  for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==resource_owner){*c=ResourceClaim::EMPTY;}
 }
}

/// Explicitly release a device's committed resources after a successful
/// STOP_DEVICE/REMOVE_DEVICE completion. Safe to call repeatedly.
pub(crate) fn io_release_pnp_resources(device:*mut DeviceObject){
 if device.is_null(){return}
 let mut arbiter=RESOURCE_ARBITER.lock();
 for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==device){*c=ResourceClaim::EMPTY;}
}

/// PnP manager bookkeeping for bus-enumerated device relations. The bus driver
/// remains responsible for constructing a valid DEVICE_RELATIONS array; this
/// manager validates every returned pointer against the I/O manager's live
/// device-object table before publishing the relation.
const MAX_PNP_RELATIONS:usize=64;
#[repr(C)]
pub struct DeviceRelations { pub count:u32, pub objects:[*mut DeviceObject;MAX_PNP_RELATIONS] }
#[derive(Clone,Copy)]
struct PnpRelation { used:bool, bus:*mut DeviceObject, child:*mut DeviceObject }
unsafe impl Send for PnpRelation {}
impl PnpRelation { const EMPTY:Self=Self{used:false,bus:ptr::null_mut(),child:ptr::null_mut()}; }
const MAX_PNP_MANAGER_IRPS:usize=64;
#[derive(Clone,Copy)]
struct ManagedPnpIrp { used:bool, irp:*mut Irp, device:*mut DeviceObject, returned:bool, completed:bool, status:i32, remove_after_surprise:bool, enumerate_bus:bool }
unsafe impl Send for ManagedPnpIrp {}
impl ManagedPnpIrp { const EMPTY:Self=Self{used:false,irp:ptr::null_mut(),device:ptr::null_mut(),returned:false,completed:false,status:STATUS_PENDING,remove_after_surprise:false,enumerate_bus:false}; }
struct PnpManagerState { relations:[PnpRelation;MAX_PNP_DEVICES], managed_irps:[ManagedPnpIrp;MAX_PNP_MANAGER_IRPS], enumerations:u64 }
const MAX_PNP_DEVICES:usize=128;
impl PnpManagerState { const fn new()->Self{Self{relations:[PnpRelation::EMPTY;MAX_PNP_DEVICES],managed_irps:[ManagedPnpIrp::EMPTY;MAX_PNP_MANAGER_IRPS],enumerations:0}} }
static PNP_MANAGER:IrqSpinLock<PnpManagerState>=IrqSpinLock::new(PnpManagerState::new());

/// Publish bus relations after a successful IRP_MN_QUERY_DEVICE_RELATIONS
/// completion. The relations object and every child pointer must be kernel-owned
/// and readable; unknown/deleting objects are rejected before any state changes.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn IoPnPProcessBusRelations(bus:*mut DeviceObject,relations:*const DeviceRelations)->i32{
 if bus.is_null()||relations.is_null(){return STATUS_INVALID_PARAMETER}
 let count=unsafe{ptr::read_unaligned(ptr::addr_of!((*relations).count))} as usize;
 if count>MAX_PNP_RELATIONS{return STATUS_INVALID_PARAMETER}
 let mut children=[ptr::null_mut::<DeviceObject>();MAX_PNP_RELATIONS];
 for i in 0..count {
  let child=unsafe{ptr::read_unaligned(ptr::addr_of!((*relations).objects[i]))};
  if child.is_null()||child==bus{return STATUS_INVALID_PARAMETER}
  let valid={let state=WDM.lock();state.devices.iter().any(|s|s.used&&!s.deleting&&ptr::eq(&s.object,child))};
  if !valid{return STATUS_INVALID_PARAMETER}
  if children[..i].contains(&child){return STATUS_INVALID_PARAMETER}
  children[i]=child;
 }
 let mut removed=[ptr::null_mut::<DeviceObject>();MAX_PNP_DEVICES];
 let mut removed_count=0usize;
 let mut manager=PNP_MANAGER.lock();
 let free=manager.relations.iter().filter(|r|!r.used||r.bus==bus).count();
 if free<count{return STATUS_INSUFFICIENT_RESOURCES}
 for relation in manager.relations.iter().filter(|r|r.used&&r.bus==bus&&!children[..count].contains(&r.child)) {
  if removed_count<removed.len(){removed[removed_count]=relation.child;removed_count+=1;}
 }
 for relation in manager.relations.iter_mut().filter(|r|r.used&&r.bus==bus&&!children[..count].contains(&r.child)){*relation=PnpRelation::EMPTY;}
 for child in children[..count].iter().copied(){
  if !manager.relations.iter().any(|r|r.used&&r.bus==bus&&r.child==child){
   if let Some(slot)=manager.relations.iter_mut().find(|r|!r.used||r.bus==bus&&!children[..count].contains(&r.child)){
    *slot=PnpRelation{used:true,bus,child};
   }
  }
 }
 manager.enumerations=manager.enumerations.wrapping_add(1);
 drop(manager);
 // Children removed from BusRelations must go through real PnP teardown rather
 // than simply disappearing from the enumeration table.
 for child in removed[..removed_count].iter().copied(){
  let _=unsafe{IoPnPRemoveDevice(child)};
 }
 STATUS_SUCCESS
}

/// Dispatch one real IRP_MJ_PNP lifecycle request under PnP-manager ownership.
/// START_DEVICE is validated/resourced by the normal Po/I/O dispatch path.
/// Pending IRPs remain owned until terminal IoCompleteRequest.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn IoPnPDispatchLifecycle(device:*mut DeviceObject,minor:u8,allocated:*const c_void,translated:*const c_void)->i32 {
 unsafe{io_pnp_dispatch_lifecycle_internal(device,minor,allocated,translated,false)}
}
unsafe fn io_pnp_dispatch_lifecycle_internal(device:*mut DeviceObject,minor:u8,allocated:*const c_void,translated:*const c_void,remove_after_surprise:bool)->i32 {
 if device.is_null()||!matches!(minor,0x00|0x01|0x02|0x03|0x04|0x05|0x06|0x07|0x17){return STATUS_INVALID_PARAMETER}
 let dispatch_device=unsafe{io_get_attached_device(device)};
 if dispatch_device.is_null(){return STATUS_DELETE_PENDING}
 let referenced=unsafe{io_reference_device_object(dispatch_device)};
 if referenced.is_null(){return STATUS_DELETE_PENDING}
 let stack_size=unsafe{(*dispatch_device).stack_size.max(1)};
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){unsafe{io_dereference_device_object(dispatch_device)};return STATUS_INSUFFICIENT_RESOURCES}
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){unsafe{io_free_irp(irp);io_dereference_device_object(device)};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=0x1b;(*stack).minor_function=minor;
  ptr::write_unaligned(stack.cast::<u8>().add(4).cast::<*mut c_void>(),allocated);
  ptr::write_unaligned(stack.cast::<u8>().add(12).cast::<*mut c_void>(),translated);
 }
 {
  let mut manager=PNP_MANAGER.lock();
  let Some(slot)=manager.managed_irps.iter_mut().find(|s|!s.used) else {
   drop(manager);unsafe{io_free_irp(irp);io_dereference_device_object(device)};
   return STATUS_INSUFFICIENT_RESOURCES
  };
  *slot=ManagedPnpIrp{used:true,irp,device:dispatch_device,returned:false,completed:false,status:STATUS_PENDING,remove_after_surprise,enumerate_bus:minor==0x07};
 }
 let status=unsafe{io_call_driver(dispatch_device,irp)};
 let terminal={let state=WDM.lock();state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp)).is_some_and(|s|s.completion_state==2)};
 if status!=STATUS_PENDING&&!terminal {
  unsafe{
   let status_bytes=status.to_le_bytes();
   ptr::copy_nonoverlapping(status_bytes.as_ptr(),ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET),status_bytes.len());
   io_complete_request(irp,0);
  }
 }
 let completed={
  let mut manager=PNP_MANAGER.lock();
  if let Some(slot)=manager.managed_irps.iter_mut().find(|s|s.used&&s.irp==irp){
   slot.returned=true;
   if slot.completed{let result=*slot;*slot=ManagedPnpIrp::EMPTY;Some(result)}else{None}
  }else{None}
 };
 let mut final_status=status;
 if let Some(request)=completed {
  if request.enumerate_bus&&request.status==STATUS_SUCCESS {
   let relations=unsafe{read_ptr((*request.irp).bytes.as_ptr(),IRP_INFORMATION_OFFSET)};
   let _=unsafe{IoPnPProcessBusRelations(request.device,relations.cast::<DeviceRelations>())};
  }
  unsafe{io_free_irp(request.irp);}
  if request.remove_after_surprise&&request.status==STATUS_SUCCESS {
   let remove_status=unsafe{io_pnp_dispatch_lifecycle_internal(request.device,IRP_MN_REMOVE_DEVICE,ptr::null(),ptr::null(),false)};
   if status!=STATUS_PENDING {final_status=remove_status;}
  }
  unsafe{io_dereference_device_object(request.device);}
 }
 final_status
}
fn io_pnp_manager_irp_completed(irp:*mut Irp){
 if irp.is_null(){return}
 let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
 let completed={
  let mut manager=PNP_MANAGER.lock();
  let Some(slot)=manager.managed_irps.iter_mut().find(|s|s.used&&s.irp==irp) else{return};
  slot.completed=true;slot.status=status;
  if slot.returned{let result=*slot;*slot=ManagedPnpIrp::EMPTY;Some(result)}else{None}
 };
 if let Some(request)=completed {
  if request.enumerate_bus&&status==STATUS_SUCCESS {
   let relations=unsafe{read_ptr((*request.irp).bytes.as_ptr(),IRP_INFORMATION_OFFSET)};
   let _=unsafe{IoPnPProcessBusRelations(request.device,relations.cast::<DeviceRelations>())};
  }
  unsafe{io_free_irp(request.irp);}
  if request.remove_after_surprise&&status==STATUS_SUCCESS {
   let _=unsafe{io_pnp_dispatch_lifecycle_internal(request.device,IRP_MN_REMOVE_DEVICE,ptr::null(),ptr::null(),false)};
  }
  unsafe{io_dereference_device_object(request.device);}
 }
}

/// Ask a bus driver for IRP_MN_QUERY_DEVICE_RELATIONS / BusRelations and
/// publish the returned children when the IRP completes. STATUS_PENDING is
/// supported; the manager retains the IRP and processes relations on completion.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn IoPnPEnumerateBus(bus:*mut DeviceObject)->i32{
 unsafe{IoPnPDispatchLifecycle(bus,0x07,ptr::null(),ptr::null())}
}
/// Start a PnP device by sending its real START_DEVICE IRP with the
/// allocated and translated resource lists supplied by the bus/resource arbiter.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn IoPnPStartDevice(device:*mut DeviceObject,allocated:*const c_void,translated:*const c_void)->i32{
 unsafe{IoPnPDispatchLifecycle(device,IRP_MN_START_DEVICE,allocated,translated)}
}
/// Stop a started device through its driver's IRP_MN_STOP_DEVICE dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn IoPnPStopDevice(device:*mut DeviceObject)->i32{
 unsafe{IoPnPDispatchLifecycle(device,IRP_MN_STOP_DEVICE,ptr::null(),ptr::null())}
}
/// Remove a device using the required surprise-removal-before-remove sequence.
/// If surprise removal completes asynchronously, the manager automatically
/// dispatches REMOVE_DEVICE after successful terminal completion.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn IoPnPRemoveDevice(device:*mut DeviceObject)->i32{
 if device.is_null(){return STATUS_INVALID_PARAMETER}
 let state=io_pnp_state(device);
 if state<0{return STATUS_INVALID_PARAMETER}
 if state==PNP_STATE_REMOVED as i32{return STATUS_SUCCESS}
 if state==PNP_STATE_STARTED as i32 {
  // The manager owns the continuation: successful surprise-removal completion
  // automatically queues REMOVE_DEVICE, including when the first IRP pends.
  return unsafe{io_pnp_dispatch_lifecycle_internal(device,IRP_MN_SURPRISE_REMOVAL,ptr::null(),ptr::null(),true)}
 }
 if state!=PNP_STATE_SURPRISE_REMOVED as i32&&state!=PNP_STATE_STOPPED as i32&&state!=PNP_STATE_NOT_STARTED as i32{return STATUS_DEVICE_BUSY}
 unsafe{IoPnPDispatchLifecycle(device,IRP_MN_REMOVE_DEVICE,ptr::null(),ptr::null())}
}

/// Return the number of currently published children for a bus. This is an
/// enumeration snapshot count, not a claim that those children have started.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn IoPnPQueryChildCount(bus:*mut DeviceObject)->u32{
 if bus.is_null(){return 0}
 let manager=PNP_MANAGER.lock();
 manager.relations.iter().filter(|r|r.used&&r.bus==bus).count() as u32
}

pub(crate) fn io_pnp_state(device:*mut DeviceObject)->i32 {
 if device.is_null(){return -1}
 let state=WDM.lock();state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)).map(|s|s.pnp_state as i32).unwrap_or(-1)
}
/// Atomically publish a committed D-state while the device is still live.
/// The Po layer owns the WDK ABI; this helper only protects I/O device metadata.
pub(crate) fn io_set_device_power_state(device:*mut DeviceObject,next:u8)->Option<u8> {
 if device.is_null() || next>DEVICE_POWER_D3 { return None; }
 let mut state=WDM.lock();
 let slot=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)&&!s.deleting)?;
 let old=slot.power_state;
 slot.power_state=next;
 Some(old)
}

pub fn device_power_state(device:*mut DeviceObject)->i32 {
 if device.is_null(){return -1}
 let state=WDM.lock();
 state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)).map(|s|s.power_state as i32).unwrap_or(-1)
}

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
 let mut counted_name=[0u16;260];
 let name_len=if !device_name.is_null() {
  let name=unsafe{&*device_name};
  let bytes=name.length as usize;
  if name.length>name.maximum_length || name.length%2!=0 || bytes>counted_name.len()*2 || (bytes!=0&&name.buffer.is_null()) {return STATUS_INVALID_PARAMETER;}
  let units=bytes/2;
  if units!=0 {unsafe{ptr::copy_nonoverlapping(name.buffer,counted_name.as_mut_ptr(),units);}}
  units
 } else {0};
 let mut state=WDM.lock();
 if unsafe{(*driver).flags}&(DRIVER_UNLOADING_FLAG|DRIVER_UNLOADED_FLAG)!=0{return STATUS_DELETE_PENDING}
 if name_len!=0&&state.devices.iter().any(|s|s.used&&s.name_len==name_len&&unicode_equal_folded(&s.name[..s.name_len],&counted_name[..name_len])) {return STATUS_OBJECT_NAME_COLLISION}
 let Some(slot)=state.devices.iter_mut().find(|s|!s.used) else {return STATUS_INSUFFICIENT_RESOURCES};
 slot.used=true;slot.deleting=false;slot.active_dispatches=0;slot.outstanding_irps=0;slot.remove_lock_count=0;slot.power_state=0;slot.power_target=0;slot.power_transition=false;slot.pnp_state=0;slot.pnp_pending=0;slot.pnp_transition_minor=0;slot.pnp_transition=false;slot.attached_to=ptr::null_mut();
 slot.name_len=name_len;slot.name=[0;260];slot.name[..name_len].copy_from_slice(&counted_name[..name_len]);
 slot.object.driver_object=driver;slot.object.device_type=device_type;slot.object.characteristics=characteristics;
 slot.object.size=(core::mem::size_of::<DeviceObject>()+extension_size as usize) as u16;
 slot.object.device_extension=if extension_size==0 {ptr::null_mut()} else {slot.extension.as_mut_ptr().cast()};
 slot.object.stack_size=1;
 slot.object.next_device=unsafe{(*driver).device_object};
 let object=&mut slot.object as *mut DeviceObject;
 unsafe { (*driver).device_object=object; output.write(object); }
 drop(state);
 let mut ob_name=[0u16;128];
 if name_len>ob_name.len() { unsafe{io_delete_device(object)}; return STATUS_INVALID_PARAMETER; }
 ob_name[..name_len].copy_from_slice(&counted_name[..name_len]);
 let type_name=[b'D' as u16,b'e' as u16,b'v' as u16,b'i' as u16,b'c' as u16,b'e' as u16];
 if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(object.cast(),&type_name,&ob_name[..name_len],false,ptr::null(),0,io_object_manager_finalizer)}.is_err() {
  unsafe{io_delete_device(object)}; return STATUS_INSUFFICIENT_RESOURCES;
 }
 STATUS_SUCCESS
}

unsafe extern "win64" fn io_object_manager_finalizer(object:*mut c_void){unsafe{io_delete_device(object.cast::<DeviceObject>());}}

/// Begin driver unload only after all owned devices have completed active
/// dispatch, remove-lock, reference and power-transition rundown. Repeated calls
/// are safe: the unload callback is atomically claimed by clearing its pointer.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_unload_driver(driver:*mut DriverObject)->i32 {
 if driver.is_null(){return STATUS_INVALID_PARAMETER}
 let mut devices=[ptr::null_mut::<DeviceObject>();MAX_WDM_DEVICES];
 let count={
  let mut state=WDM.lock();
  let flags=unsafe{(*driver).flags};
  if flags&DRIVER_UNLOADED_FLAG!=0{return STATUS_SUCCESS}
  unsafe{(*driver).flags=flags|DRIVER_UNLOADING_FLAG;}
  let mut n=0usize;
  for slot in state.devices.iter_mut().filter(|s|s.used&&s.object.driver_object==driver){
   slot.deleting=true;
   if n<devices.len(){devices[n]=ptr::addr_of_mut!(slot.object);n+=1;}
  }
  n
 };
 for device in devices[..count].iter().copied(){unsafe{io_delete_device(device);}}
 let callback={
  let state=WDM.lock();
  if state.devices.iter().any(|s|s.used&&s.object.driver_object==driver){return STATUS_PENDING}
  drop(state);
  let _state=WDM.lock();
  unsafe{
   let callback=(*driver).driver_unload;
   (*driver).driver_unload=None;
   (*driver).flags|=DRIVER_UNLOADED_FLAG|DRIVER_UNLOADING_FLAG;
   callback
  }
 };
 if let Some(callback)=callback{unsafe{callback(driver)}}
 STATUS_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_delete_device(device:*mut DeviceObject) {
 if device.is_null(){return}
 let _ = crate::ntoskrnl::ob::runtime::unregister_object(device.cast());
 let mut state=WDM.lock();
 let Some(index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,device)) else{return};
 state.devices[index].deleting=true;
 // Defer reclamation while external references, remove-lock owners, or power transitions remain.
 if state.devices[index].object.reference_count > 1 || state.devices[index].remove_lock_count != 0 || state.devices[index].active_dispatches != 0 || state.devices[index].outstanding_irps != 0 || state.devices[index].power_transition { return; }
 // Ob references and handles may outlive Io callers; only reclaim the backing slot after Ob has removed its header record.
 if !crate::ntoskrnl::ob::runtime::object_rundown_complete(device.cast()) { return; }
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
 slot.name_len=0;slot.name.fill(0);
 slot.used=false;slot.deleting=false;
 // Drop stale bus/child links before making the backing device slot reusable.
 {
  let mut manager=PNP_MANAGER.lock();
  for relation in manager.relations.iter_mut().filter(|r|r.used&&(r.bus==device||r.child==device)){*relation=PnpRelation::EMPTY;}
 }
 drop(state);
 io_release_pnp_resources(device);
 unsafe{io_unregister_file_system(device);}
}
fn initialize_irp_slot(slot:&mut IrpSlot,stack_size:i8)->*mut Irp {
 let next=slot.next;
 slot.used=true;slot.owned_mdl=ptr::null_mut();slot.completion_state=0;
 slot.suspended_completion=false;slot.free_requested=false;slot.active_ops=0;
 slot.outstanding_count=0;slot.outstanding_devices.fill(ptr::null_mut());
 slot.system_buffer=ptr::null_mut();slot.system_buffer_pages=0;slot.system_buffer_len=0;
 slot.ioctl_output_buffer=ptr::null_mut();slot.ioctl_output_len=0;slot.built_sync_ioctl=false;
 slot.sync_thread_id=0;slot.sync_event=ptr::null_mut();slot.sync_next=ptr::null_mut();slot.sync_prev=ptr::null_mut();slot.sync_queued=false;
 slot.object.bytes.fill(0);
 slot.object.bytes[0..2].copy_from_slice(&6i16.to_le_bytes());
 slot.object.bytes[2..4].copy_from_slice(&(IRP_SIZE_X64 as u16).to_le_bytes());
 slot.object.bytes[0x42]=stack_size as u8;slot.object.bytes[0x43]=(stack_size as u8).wrapping_add(1);
 slot.stack.fill(IoStackLocation{major_function:0,minor_function:0,flags:0,control:0,parameters:[0;0x20],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut()});
 slot.next=next;
 let object=ptr::addr_of_mut!(slot.object);
 let one_past=unsafe{slot.stack.as_mut_ptr().add(stack_size as usize).cast::<c_void>()};
 unsafe{write_ptr(ptr::addr_of_mut!((*object).bytes).cast::<u8>(),IRP_STACK_OFFSET,one_past);}
 object
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_allocate_irp(stack_size:i8, charge_quota:u8)->*mut Irp {
 if stack_size<=0||stack_size as usize>MAX_WDM_STACK_LOCATIONS||charge_quota>1{return ptr::null_mut()}
 {
  let mut state=WDM.lock();
  if let Some(slot)=state.irps.iter_mut().find(|s|!s.used){return initialize_irp_slot(slot,stack_size)}
 }
 // Grow in slabs, not one VM extent per IRP. This avoids turning the VM
 // extent-descriptor count into a hidden small IRP limit. Slots remain reusable
 // after IoFreeIrp; backing pages are retained as a nonpaged high-water pool.
 const IRP_SLAB_PAGES:usize=256;
 let slab_bytes=IRP_SLAB_PAGES*PAGE_SIZE;
 let slot_bytes=core::mem::size_of::<IrpSlot>();
 let count=slab_bytes/slot_bytes;
 if count==0{return ptr::null_mut()}
 let memory=match crate::ntoskrnl::mm::kernel_space::allocate_pages(IRP_SLAB_PAGES){Ok(p)=>p,Err(_)=>return ptr::null_mut()};
 let mut state=WDM.lock();
 for index in 0..count {
  let candidate=unsafe{memory.add(index*slot_bytes).cast::<IrpSlot>()};
  unsafe{ptr::write(candidate,IrpSlot::empty());}
  state.irps.push_front(candidate);
 }
 let Some(slot)=state.irps.iter_mut().find(|s|!s.used) else{return ptr::null_mut()};
 initialize_irp_slot(slot,stack_size)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_free_irp(irp:*mut Irp) {
 if irp.is_null(){return}
 {
  let mut state=WDM.lock();
  let Some(index)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,irp)) else{return};
  if state.irps[index].completion_state==1 {state.irps[index].free_requested=true;return}
  if state.irps[index].completion_state==3||state.irps[index].active_ops!=0||(state.irps[index].completion_state==0&&state.irps[index].outstanding_count!=0){return}
  state.irps[index].free_requested=false;state.irps[index].suspended_completion=false;state.irps[index].completion_state=2;
  let _=sync_queue_detach_locked(&mut state,irp);
 }
 release_irp_device_rundown(irp);
 let (owned_mdl,system_buffer,system_buffer_pages)={
  let mut state=WDM.lock();
  let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  let mdl=slot.owned_mdl;slot.owned_mdl=ptr::null_mut();
  let buffer=slot.system_buffer;let pages=slot.system_buffer_pages;
  slot.object.bytes.fill(0);slot.system_buffer=ptr::null_mut();slot.system_buffer_pages=0;slot.system_buffer_len=0;
  slot.ioctl_output_buffer=ptr::null_mut();slot.ioctl_output_len=0;slot.built_sync_ioctl=false;
  slot.sync_thread_id=0;slot.sync_event=ptr::null_mut();slot.used=false;
  (mdl,buffer,pages)
 };
 crate::dma::release_irp_mappings(irp.cast());
 if !owned_mdl.is_null(){unsafe{crate::ntoskrnl::mm::runtime::mm_free_mdl(owned_mdl);}}
 if !system_buffer.is_null()&&system_buffer_pages!=0{let _=crate::ntoskrnl::mm::kernel_space::free_pages(system_buffer,system_buffer_pages);}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_get_current_irp_stack_location(irp:*mut Irp)->*mut IoStackLocation {
 if irp.is_null(){return ptr::null_mut()}
 unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET).cast()}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_get_next_irp_stack_location(irp:*mut Irp)->*mut IoStackLocation {
 if irp.is_null(){return ptr::null_mut()}
 let current=unsafe{io_get_current_irp_stack_location(irp)};
 if current.is_null(){return ptr::null_mut()}
 // The WDM ABI places stack locations immediately after the fixed IRP.
 let count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize};
 if count==0||count>0x7f{return ptr::null_mut()}
 let base=(irp as usize).saturating_add(IRP_SIZE_X64);
 let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
 let cur=current as usize;
 if cur<=base||cur>end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return ptr::null_mut()}
 unsafe{current.cast::<u8>().sub(IO_STACK_LOCATION_SIZE_X64).cast()}
}
#[unsafe(no_mangle)]

/// Pin a device for the lifetime of an IRP, not merely for IoCallDriver.
/// This bounded list prevents unload from reclaiming devices with callbacks
/// still in flight or an asynchronous request still outstanding.
fn acquire_irp_active_op(irp:*mut Irp)->bool {
 let mut state=WDM.lock();
 let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return false};
 if slot.completion_state==2 || slot.active_ops==u32::MAX {return false}
 slot.active_ops+=1;true
}

/// Pin a device for the lifetime of an IRP, not merely for IoCallDriver.
fn track_irp_device(irp:*mut Irp, device:*mut DeviceObject)->bool {
 let mut state=WDM.lock();
 let Some(ir)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return false};
 if state.irps[ir].completion_state!=0 || state.irps[ir].outstanding_count>=state.irps[ir].outstanding_devices.len(){return false}
 if state.irps[ir].outstanding_devices[..state.irps[ir].outstanding_count].contains(&device){return true}
 let Some(di)=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)) else{return false};
 if state.devices[di].outstanding_irps==u32::MAX{return false}
 state.devices[di].outstanding_irps+=1;
 let n=state.irps[ir].outstanding_count;
 state.irps[ir].outstanding_devices[n]=device;
 state.irps[ir].outstanding_count=n+1;
 true
}

/// Release the IRP's device references once. The caller must keep the IRP
/// allocated until this function returns.
fn release_irp_active_op(irp:*mut Irp) {
 let should_release={
  let mut state=WDM.lock();
  let Some(index)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  if state.irps[index].active_ops!=0 {state.irps[index].active_ops-=1;}
  state.irps[index].active_ops==0
   && state.irps[index].completion_state==2
   && state.irps[index].outstanding_count!=0
 };
 // Completion may finish while a cancel/dispatch callback is still pinned.
 // Keep device rundown until the final callback pin is dropped.
 if should_release {release_irp_device_rundown(irp);}
}

/// Drop every device rundown reference attached to an IRP exactly once.
fn try_finalize_deleting_device(device:*mut DeviceObject) {
 if device.is_null(){return}
 let finalize={
  let state=WDM.lock();
  state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device))
   .is_some_and(|s|s.deleting&&s.object.reference_count<=1&&s.remove_lock_count==0
    &&s.active_dispatches==0&&s.outstanding_irps==0&&!s.power_transition)
 };
 if finalize {unsafe{io_delete_device(device);}}
}

fn release_irp_device_rundown(irp:*mut Irp) {
 let devices={
  let mut state=WDM.lock();
  let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  let count=state.irps[i].outstanding_count.min(state.irps[i].outstanding_devices.len());
  let devices=state.irps[i].outstanding_devices;
  state.irps[i].outstanding_count=0;
  state.irps[i].outstanding_devices.fill(ptr::null_mut());
  for device in devices[..count].iter().copied() {
   if let Some(d)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) {
    if d.outstanding_irps!=0 {d.outstanding_irps-=1;}
   }
  }
  devices
 };
 // Releasing an IRP's rundown pin must never request device deletion. Only
 // finalize objects whose owner has already called IoDeleteDevice.
 for device in devices.into_iter().filter(|d|!d.is_null()) {try_finalize_deleting_device(device);}
}

fn release_device_dispatch_rundown(device:*mut DeviceObject){
 let finalize={
  let mut state=WDM.lock();
  let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return};
  if slot.active_dispatches!=0{slot.active_dispatches-=1;}
  slot.deleting&&slot.active_dispatches==0&&slot.object.reference_count<=1&&slot.remove_lock_count==0&&!slot.power_transition
 };
 if finalize{unsafe{io_delete_device(device)}}
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_call_driver(device:*mut DeviceObject, irp:*mut Irp)->i32 {
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let (driver, deleting)={
  let mut state=WDM.lock();
  match state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) {
   Some(slot) if !slot.deleting&&slot.pnp_state!=PNP_STATE_REMOVED&&slot.object.driver_object!=ptr::null_mut() => {
    if slot.active_dispatches==u32::MAX{return STATUS_DEVICE_BUSY}
    slot.active_dispatches+=1;(slot.object.driver_object,false)
   },
   Some(_)=>(ptr::null_mut(),true),None=>(ptr::null_mut(),true)
  }
 };
 if deleting||driver.is_null(){return STATUS_DELETE_PENDING}
 // Windows requires a completion routine when a synchronous builder IRP has
 // no Event to signal. Validate before mutating the IRP stack or taking rundown.
 let needs_completion={
  let state=WDM.lock();
  state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp))
   .is_some_and(|s|s.built_sync_ioctl&&s.sync_event.is_null())
 };
 if needs_completion {
  let next=unsafe{io_get_next_irp_stack_location(irp)};
  if next.is_null()||unsafe{(*next).completion_routine.is_none()} {
   release_device_dispatch_rundown(device);
   return STATUS_INVALID_PARAMETER;
  }
 }
 if !track_irp_device(irp,device) { release_device_dispatch_rundown(device); return STATUS_DELETE_PENDING; }
 if !acquire_irp_active_op(irp) { release_device_dispatch_rundown(device); return STATUS_DELETE_PENDING; }
 let irp_stack_count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize};
 let required_stack_size=unsafe{(*device).stack_size};
 if required_stack_size<=0 {release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_DEVICE_REQUEST;}
 let required_stack_count=required_stack_size as usize;
 if irp_stack_count<required_stack_count {
  release_irp_active_op(irp);release_device_dispatch_rundown(device);
  return STATUS_INVALID_DEVICE_REQUEST;
 }
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_PARAMETER}
 let location=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)};
 if location<=1{release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_DEVICE_REQUEST}
 unsafe {
  write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,stack.cast());
  *ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=location-1;
  (*stack).device_object=device;
 }
 let major=unsafe{(*stack).major_function as usize};
 if major>=28{release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_DEVICE_REQUEST}
 let dispatch=unsafe{(*driver).major_function[major]};
 let Some(dispatch)=dispatch else{release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_DEVICE_REQUEST};
 let parameters=unsafe{ptr::read_unaligned((*stack).parameters.as_ptr().cast::<[u64;4]>())};
 let prepare=unsafe{crate::ntoskrnl::po::runtime::po_prepare_irp_dispatch(device,irp,major as u8,(*stack).minor_function,parameters)};
 if prepare!=STATUS_SUCCESS{
  unsafe{write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,stack.cast::<u8>().add(IO_STACK_LOCATION_SIZE_X64).cast());*ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=location;}
  release_irp_active_op(irp);release_device_dispatch_rundown(device);return prepare
 }
 let status=unsafe{dispatch(device,irp)};
 // STATUS_PENDING is not a substitute for IoMarkIrpPending. The lower
 // driver marks its own stack location; completion unwind propagates it.
 crate::ntoskrnl::po::runtime::po_finish_dispatch_return(irp,device,status);
 release_irp_active_op(irp);
 try_free_completed_sync_ioctl_irp(irp);
 release_device_dispatch_rundown(device);
 status
}
/// Build a synchronous device-control IRP. METHOD_BUFFERED and the two
/// direct methods are supported; METHOD_NEITHER preserves the caller's kernel
/// pointers. A supplied event is signaled after the IOSB is finalized and the
/// IRP reaches terminal completion. Event == NULL requires the caller to install
/// a completion routine before calling IoCallDriver.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_build_device_io_control_request(
 ioctl_code:u32, device:*mut DeviceObject, input_buffer:*mut c_void,
 input_length:u32, output_buffer:*mut c_void, output_length:u32,
 internal_device_control:u8, event:*mut c_void, io_status:*mut IoStatusBlock
)->*mut Irp {
 let creator_thread=crate::thread::current_thread_id().unwrap_or(0);
 // IoBuildDeviceIoControlRequest is a synchronous, thread-associated DDI and
 // Windows documents it for IRQL <= APC_LEVEL. Host-side tests can validate
 // packet construction without a live scheduler thread.
 if unsafe{crate::ntoskrnl::ke::runtime::ke_get_current_irql()}>1{return ptr::null_mut()}
 #[cfg(not(test))]
 if creator_thread==0 {return ptr::null_mut()}
 if device.is_null()||io_status.is_null()||internal_device_control>1
  ||(input_length!=0&&input_buffer.is_null())||(output_length!=0&&output_buffer.is_null()) {
  return ptr::null_mut()
 }
 let stack_size={
  let state=WDM.lock();
  let Some(slot)=state.devices.iter().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)) else{return ptr::null_mut()};
  slot.object.stack_size
 };
 if stack_size<=0{return ptr::null_mut()}
 let method=(ioctl_code&3) as u8;
 let required_buffer=match method {0=>input_length.max(output_length) as usize,1|2=>input_length as usize,_=>0};
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){return ptr::null_mut()}
 let next=unsafe{io_get_next_irp_stack_location(irp)};
 if next.is_null(){unsafe{io_free_irp(irp)};return ptr::null_mut()}
 let (system_buffer,system_buffer_pages)=if required_buffer==0 {(ptr::null_mut(),0)} else {
  let pages=match required_buffer.checked_add(PAGE_SIZE-1){Some(n)=>n/PAGE_SIZE,None=>{unsafe{io_free_irp(irp)};return ptr::null_mut()}};
  match crate::ntoskrnl::mm::kernel_space::allocate_pages(pages) {
   Ok(p)=>{unsafe{ptr::write_bytes(p,0,pages*PAGE_SIZE);} (p,pages)},
   Err(_)=>{unsafe{io_free_irp(irp)};return ptr::null_mut()}
  }
 };
 {
  let mut state=WDM.lock();
  let Some(index)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{drop(state);unsafe{io_free_irp(irp)};return ptr::null_mut()};
  let slot=&mut state.irps[index];
  if method!=3 {
   slot.system_buffer=system_buffer;slot.system_buffer_pages=system_buffer_pages;
   slot.system_buffer_len=if method==0 {required_buffer}else{input_length as usize};
   if input_length!=0 {unsafe{ptr::copy_nonoverlapping(input_buffer.cast::<u8>(),slot.system_buffer,input_length as usize);}}
   write_ptr(slot.object.bytes.as_mut_ptr(),0x18,slot.system_buffer.cast());
   if method==0 {
    slot.ioctl_output_buffer=output_buffer;
    slot.ioctl_output_len=output_length as usize;
    slot.object.bytes[0x10..0x14].copy_from_slice(&(0x04u32|0x10u32|0x20u32).to_le_bytes());
   } else {
    slot.object.bytes[0x10..0x14].copy_from_slice(&(0x04u32|0x10u32|0x20u32).to_le_bytes());
   }
   slot.built_sync_ioctl=true;
   slot.sync_thread_id=creator_thread;
   slot.sync_event=event;
  } else {
   // METHOD_NEITHER's Type3InputBuffer and Irp.UserBuffer are kernel pointers
   // supplied by this kernel-mode caller, as required by this DDI contract.
   slot.object.bytes[0x10..0x14].copy_from_slice(&0x04u32.to_le_bytes());
   slot.built_sync_ioctl=true;
   slot.sync_thread_id=crate::thread::current_thread_id().unwrap_or(0);
   slot.sync_event=event;
  }
  // UserBuffer retains the caller's output pointer for all transfer methods;
  // drivers use SystemBuffer or MdlAddress according to the IOCTL method.
  write_ptr(slot.object.bytes.as_mut_ptr(),0x70,output_buffer);
  write_ptr(slot.object.bytes.as_mut_ptr(),0x48,io_status.cast());
  ptr::write_unaligned(slot.object.bytes.as_mut_ptr().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_PENDING);
  ptr::write_unaligned(slot.object.bytes.as_mut_ptr().add(IRP_INFORMATION_OFFSET).cast::<usize>(),0);
 }
 let attached={let mut state=WDM.lock();sync_queue_attach_locked(&mut state,irp,creator_thread)};
 if !attached {unsafe{io_free_irp(irp)};return ptr::null_mut()}
 unsafe {
  (*next).major_function=if internal_device_control!=0{0x0f}else{0x0e};
  // WDK marks InputBufferLength and IoControlCode POINTER_ALIGNMENT
  // fields: on x64 their offsets in Parameters.DeviceIoControl are 8 and 16.
  ptr::write_unaligned(next.cast::<u8>().add(4).cast::<u32>(),output_length);
  ptr::write_unaligned(next.cast::<u8>().add(12).cast::<u32>(),input_length);
  ptr::write_unaligned(next.cast::<u8>().add(20).cast::<u32>(),ioctl_code);
  if method==3 {
   ptr::write_unaligned(next.cast::<u8>().add(28).cast::<*mut c_void>(),input_buffer);
  }
 }
 if method==1||method==2 {
  if output_length!=0 {
   let mdl=unsafe{io_allocate_mdl(output_buffer,output_length,0,0,irp)};
   if mdl.is_null(){unsafe{io_free_irp(irp)};return ptr::null_mut()}
   // METHOD_IN_DIRECT grants the driver read access to the second buffer;
   // METHOD_OUT_DIRECT grants write access from the driver's perspective.
   unsafe{crate::ntoskrnl::mm::runtime::mm_probe_and_lock_pages(mdl,0,if method==1{0}else{1});}
   let flags=unsafe{ptr::read_unaligned(mdl.cast::<u8>().add(0x0a).cast::<i16>())};
   if flags&0x0002==0{unsafe{io_free_mdl(mdl);io_free_irp(irp);}return ptr::null_mut()}
   let mut state=WDM.lock();
   if let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)){slot.owned_mdl=mdl;}
  }
 }
 irp
}

fn try_free_completed_sync_ioctl_irp(irp:*mut Irp) {
 if irp.is_null(){return}
 let ready={
  let state=WDM.lock();
  state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp))
   .is_some_and(|s|(s.built_sync_ioctl||s.free_requested)&&s.completion_state==2&&s.active_ops==0)
 };
 if ready {unsafe{io_free_irp(irp);}}
}

fn finalize_irp_status(irp:*mut Irp) {
 if irp.is_null(){return}
 let base=unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()};
 let iosb=unsafe{read_ptr(base,0x48)};
 if iosb.is_null(){return}
 let status=unsafe{ptr::read_unaligned(base.add(IRP_STATUS_OFFSET).cast::<i32>())};
 let information=unsafe{ptr::read_unaligned(base.add(IRP_INFORMATION_OFFSET).cast::<usize>())};
 let copy_out={
  let mut state=WDM.lock();
  let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  if slot.built_sync_ioctl && (unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x10)&0x10})!=0 && !slot.ioctl_output_buffer.is_null() {
   let n=information.min(slot.ioctl_output_len).min(slot.system_buffer_len);
   if n!=0 {unsafe{ptr::copy_nonoverlapping(slot.system_buffer,slot.ioctl_output_buffer.cast::<u8>(),n);}}
  }
  slot.built_sync_ioctl
 };
 unsafe {
  ptr::write_unaligned(iosb.cast::<u8>().cast::<i32>(),status);
  ptr::write_unaligned(iosb.cast::<u8>().add(8).cast::<usize>(),information);
 }
 let _=copy_out;
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_complete_request(irp:*mut Irp, _priority_boost:i8) {
 if irp.is_null(){return}
 // Claim completion under WDM's lock. A competing completion cannot execute
 // the same completion routine twice or publish the IOSB twice.
 {
  let mut state=WDM.lock();
  let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  if state.irps[i].completion_state!=0{return}
  state.irps[i].suspended_completion=false;
  state.irps[i].completion_state=1;
 }
 let mut finished=false;
 for _ in 0..(MAX_WDM_STACK_LOCATIONS+1) {
  let (location,stack_count)=unsafe{(*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43) as usize,*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize)};
  if stack_count==0||stack_count>MAX_WDM_STACK_LOCATIONS||location==0 {break}
  if location>stack_count {
   finalize_irp_status(irp);
   let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
   crate::dma::release_irp_mappings(irp.cast());
   crate::ntoskrnl::po::runtime::po_complete_irp(irp,status);
   finished=true;
   break
  }
  let stack=unsafe{io_get_current_irp_stack_location(irp)};
  if stack.is_null(){break}
  let control=unsafe{(*stack).control};
  // SL_PENDING_RETURNED is converted into IRP.PendingReturned during unwind.
  if control&0x01!=0 {unsafe{*ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x41)=1;}}
  let routine=unsafe{(*stack).completion_routine};
  let context=unsafe{(*stack).context};
  let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
  let cancelled=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x44)!=0};
  let invoke=(status>=0&&(control&0x40)!=0)||(status<0&&(control&0x80)!=0)||(cancelled&&(control&0x20)!=0);
  // The I/O manager advances the current stack location before invoking the
  // completion callback. If it returns STATUS_MORE_PROCESSING_REQUIRED, that
  // advanced location is retained so a later IoCompleteRequest resumes above
  // this callback instead of invoking the same callback twice.
  let next=unsafe{stack.cast::<u8>().add(IO_STACK_LOCATION_SIZE_X64).cast::<IoStackLocation>()};
  // The completion callback belongs to the next-higher driver. Windows passes
  // that driver's DeviceObject, not the lower target stored in this stack slot.
  let completion_device_object=if location<stack_count {unsafe{(*next).device_object}} else {ptr::null_mut()};
  unsafe {
   *ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=(location+1).min(0xff) as u8;
   write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,next.cast());
   // Pending state must be visible to each higher driver's completion frame.
   if *ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x41)!=0 && (location+1)<=stack_count {
    (*next).control|=0x01;
   }
  }
  if invoke {
   if let Some(routine)=routine {
    let result=unsafe{routine(completion_device_object,irp,context)};
    if result==0xC0000016u32 as i32 {
     {
      let mut state=WDM.lock();
      if let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) {
       if state.irps[i].free_requested {
        state.irps[i].completion_state=2;
        state.irps[i].suspended_completion=false;
       } else {
        state.irps[i].completion_state=0;
        state.irps[i].suspended_completion=true;
       }
      } else {return}
     }
     try_free_completed_sync_ioctl_irp(irp);
     return
    } else {
     // Freeing an IRP is only legal from a completion callback when that
     // callback stops completion; otherwise retain the packet.
     let mut state=WDM.lock();
     if let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) {
      state.irps[i].free_requested=false;
     }
    }
   }
  }
  if unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)>*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42)} {
   finalize_irp_status(irp);
   let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
   crate::dma::release_irp_mappings(irp.cast());
   crate::ntoskrnl::po::runtime::po_complete_irp(irp,status);
   finished=true;
   break
  }
 }
 {
  let mut state=WDM.lock();
  if let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) {
   // A malformed stack is retryable; a terminal unwind permanently closes it.
   state.irps[i].completion_state=if finished {2}else{0};
  }
 }
 if finished {
  // Remove the request from the creator's queue before waking it. finalize_irp_status
  // has published the IOSB, and terminal state is now visible.
  let completion_event={
   let mut state=WDM.lock();
   if state.irps.iter().any(|slot|slot.used&&ptr::eq(&slot.object,irp)){
    sync_queue_detach_locked(&mut state,irp)
   }else{ptr::null_mut()}
  };
  if !completion_event.is_null(){unsafe{crate::ntoskrnl::ke::runtime::ke_set_event(completion_event.cast(),0,0);}}
  let callbacks_drained={
   let state=WDM.lock();
   state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp))
    .is_some_and(|s|s.active_ops==0)
  };
  if callbacks_drained {release_irp_device_rundown(irp);}
  io_pnp_manager_irp_completed(irp);
  try_free_completed_sync_ioctl_irp(irp);
 }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_mark_irp_pending(irp:*mut Irp) {
 if irp.is_null(){return}
 // Windows sets SL_PENDING_RETURNED on the current stack location; the I/O
 // manager propagates this into IRP.PendingReturned while unwinding.
 let stack=unsafe{io_get_current_irp_stack_location(irp)};
 if !stack.is_null(){unsafe{(*stack).control|=0x01;}}
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
    if stack_size>=127 { return ptr::null_mut(); }
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
    // A lower filter cannot be detached while another filter is still above
    // it; doing so would orphan that upper attachment and corrupt stack depth.
    if !unsafe{(*source).attached_device.is_null()} {return}
    unsafe{(*target).attached_device=ptr::null_mut();(*source).attached_device=ptr::null_mut();(*source).stack_size=1;}
    state.devices[index].attached_to=ptr::null_mut();
}

/// Initialize a caller-allocated WDM IRP and its trailing stack locations.
/// PacketSize must cover the fixed x64 IRP plus StackSize full stack locations.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_initialize_irp(
    irp:*mut Irp, packet_size:u16, stack_size:i8
) {
    if irp.is_null() || stack_size<=0 {return}
    let required=IRP_SIZE_X64.checked_add(stack_size as usize*IO_STACK_LOCATION_SIZE_X64);
    if required.is_none_or(|n|n>packet_size as usize){return}
    unsafe {
        ptr::write_bytes(irp.cast::<u8>(),0,packet_size as usize);
        ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().cast::<i16>(),6);
        ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(2).cast::<u16>(),IRP_SIZE_X64 as u16);
        *ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x42)=stack_size as u8;
        *ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=(stack_size as u8).wrapping_add(1);
        let stack_base=irp.cast::<u8>().add(IRP_SIZE_X64);
        let current=stack_base.add(stack_size as usize*IO_STACK_LOCATION_SIZE_X64);
        write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,current.cast());
    }
}

/// Copy the current stack parameters into the next lower driver's stack slot.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_copy_current_irp_stack_location_to_next(irp:*mut Irp) {
 if irp.is_null(){return}
 let count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize};
 if count==0||count>0x7f{return}
 let current=unsafe{io_get_current_irp_stack_location(irp)};
 if current.is_null(){return}
 let base=(irp as usize).saturating_add(IRP_SIZE_X64);
 let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
 let cur=current as usize;
 if cur<base||cur>=end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return}
 let next=cur.checked_sub(IO_STACK_LOCATION_SIZE_X64).unwrap_or(0);
 if next<base{return}
 unsafe{ptr::copy_nonoverlapping(current,next as *mut IoStackLocation,1);}
}

/// Arrange for IoCallDriver to reuse the current stack slot instead of
/// consuming another one: advance the current pointer so the dispatch decrement
/// returns it to the same slot.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_skip_current_irp_stack_location(irp:*mut Irp) {
 if irp.is_null(){return}
 let count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize};
 if count==0||count>0x7f{return}
 let current=unsafe{io_get_current_irp_stack_location(irp)};
 if current.is_null(){return}
 let base=(irp as usize).saturating_add(IRP_SIZE_X64);
 let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
 let cur=current as usize;
 if cur<base||cur>=end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return}
 if cur+IO_STACK_LOCATION_SIZE_X64>end{return}
 unsafe {
  write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,(cur+IO_STACK_LOCATION_SIZE_X64) as *mut c_void);
  {let location=*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43);*ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=location.saturating_add(1);}
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
 let release_old_refs={
  let mut state=WDM.lock();
  let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  let slot=&mut state.irps[i];
  let count=slot.object.bytes[0x42] as usize;
  if count==0||count>slot.stack.len()||slot.completion_state==1||slot.completion_state==3
    ||slot.active_ops!=0||slot.built_sync_ioctl||!slot.owned_mdl.is_null()
    || !(unsafe{read_ptr(slot.object.bytes.as_ptr(),0x08)}).is_null()
    ||(slot.outstanding_count!=0&&!slot.suspended_completion) {return}
  if slot.suspended_completion {slot.completion_state=3;slot.suspended_completion=false;true}else{false}
 };
 if release_old_refs {release_irp_device_rundown(irp);}
 let mut state=WDM.lock();
 let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
 let slot=&mut state.irps[i];
 let count=slot.object.bytes[0x42] as usize;
 if count==0||count>slot.stack.len()||slot.active_ops!=0||slot.outstanding_count!=0{return}
 slot.completion_state=0;slot.suspended_completion=false;slot.free_requested=false;
 slot.stack.fill(IoStackLocation { major_function:0,minor_function:0,flags:0,control:0,parameters:[0;0x20],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut() });
 slot.object.bytes[0x08..0x30].fill(0);
 slot.object.bytes[IRP_STATUS_OFFSET..IRP_STATUS_OFFSET+4].copy_from_slice(&status.to_le_bytes());
 slot.object.bytes[IRP_INFORMATION_OFFSET..IRP_INFORMATION_OFFSET+8].fill(0);
 slot.object.bytes[0x41]=0;
 slot.object.bytes[0x44..0x48].fill(0);
 slot.object.bytes[0x48..0x78].fill(0);
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
    let new_count=registry.count;
    registry.devices[new_count]=ptr::null_mut();
}

pub fn is_file_system_registered(device:*mut DeviceObject)->bool {
    if device.is_null(){return false}
    let registry=FILESYSTEMS.lock();
    registry.devices[..registry.count].contains(&device)
}


const MAX_SYMBOLIC_LINKS:usize=32;
const MAX_SYMBOLIC_LINK_CHARS:usize=260;
#[derive(Clone,Copy)]
struct SymbolicLink { used:bool, name_len:usize, target_len:usize, name:[u16;MAX_SYMBOLIC_LINK_CHARS], target:[u16;MAX_SYMBOLIC_LINK_CHARS] }
impl SymbolicLink { const EMPTY:Self=Self{used:false,name_len:0,target_len:0,name:[0;MAX_SYMBOLIC_LINK_CHARS],target:[0;MAX_SYMBOLIC_LINK_CHARS]}; }
struct NamespaceState { links:[SymbolicLink;MAX_SYMBOLIC_LINKS] }
impl NamespaceState { const fn new()->Self{Self{links:[SymbolicLink::EMPTY;MAX_SYMBOLIC_LINKS]}} }
static NAMESPACE:IrqSpinLock<NamespaceState>=IrqSpinLock::new(NamespaceState::new());

fn valid_unicode_name(name:*mut UnicodeString)->Option<(usize,*const u16)> {
 if name.is_null(){return None}
 let value=unsafe{&*name};
 let len=value.length as usize;
 if len==0||len%2!=0||len>MAX_SYMBOLIC_LINK_CHARS*2||len>value.maximum_length as usize||value.buffer.is_null(){return None}
 Some((len/2,value.buffer.cast_const()))
}
fn unicode_equal_folded(a:&[u16],b:&[u16])->bool {
 a.len()==b.len()&&a.iter().zip(b).all(|(x,y)| {
  let fold=|c:u16| if (b'A' as u16..=b'Z' as u16).contains(&c){c+32}else{c};
  fold(*x)==fold(*y)
 })
}

/// Create a bounded object-manager symbolic-link mapping. Resolution through
/// this namespace is intentionally separate from this storage primitive.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_create_symbolic_link(link_name:*mut UnicodeString,target_name:*mut UnicodeString)->i32 {
 let Some((name_len,name_ptr))=valid_unicode_name(link_name) else{return STATUS_INVALID_PARAMETER};
 let Some((target_len,target_ptr))=valid_unicode_name(target_name) else{return STATUS_INVALID_PARAMETER};
 let mut name=[0u16;MAX_SYMBOLIC_LINK_CHARS];
 let mut target=[0u16;MAX_SYMBOLIC_LINK_CHARS];
 unsafe{ptr::copy_nonoverlapping(name_ptr,name.as_mut_ptr(),name_len);ptr::copy_nonoverlapping(target_ptr,target.as_mut_ptr(),target_len);}
 let mut state=NAMESPACE.lock();
 if state.links.iter().any(|l|l.used&&unicode_equal_folded(&l.name[..l.name_len],&name[..name_len])) {return STATUS_OBJECT_NAME_COLLISION}
 let Some(slot)=state.links.iter_mut().find(|l|!l.used) else{return STATUS_INSUFFICIENT_RESOURCES};
 *slot=SymbolicLink::EMPTY;slot.used=true;slot.name_len=name_len;slot.target_len=target_len;
 slot.name[..name_len].copy_from_slice(&name[..name_len]);slot.target[..target_len].copy_from_slice(&target[..target_len]);
 STATUS_SUCCESS
}

/// Delete a previously created symbolic-link mapping by its counted name.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_delete_symbolic_link(link_name:*mut UnicodeString)->i32 {
 let Some((name_len,name_ptr))=valid_unicode_name(link_name) else{return STATUS_INVALID_PARAMETER};
 let mut name=[0u16;MAX_SYMBOLIC_LINK_CHARS];
 unsafe{ptr::copy_nonoverlapping(name_ptr,name.as_mut_ptr(),name_len);}
 let mut state=NAMESPACE.lock();
 let Some(slot)=state.links.iter_mut().find(|l|l.used&&unicode_equal_folded(&l.name[..l.name_len],&name[..name_len])) else{return STATUS_OBJECT_NAME_NOT_FOUND};
 *slot=SymbolicLink::EMPTY;
 STATUS_SUCCESS
}


/// Windows x64 IO_CSQ public storage layout. The queue remains driver-owned;
/// all operations are routed through its callbacks as required by WDM.
#[repr(C)]
pub struct IoCsq {
 pub type_:u32,pub padding:u32,
 pub insert_irp:*mut c_void,pub remove_irp:*mut c_void,pub peek_next_irp:*mut c_void,
 pub acquire_lock:*mut c_void,pub release_lock:*mut c_void,pub complete_canceled_irp:*mut c_void,
 pub reserve_pointer:*mut c_void,
}
#[repr(C)]
pub struct IoCsqIrpContext { pub type_:u32,pub padding:u32,pub irp:*mut Irp,pub csq:*mut IoCsq }
type CsqInsert=unsafe extern "system" fn(*mut IoCsq,*mut Irp);
type CsqInsertEx=unsafe extern "system" fn(*mut IoCsq,*mut Irp,*mut c_void)->i32;
type CsqRemove=unsafe extern "system" fn(*mut IoCsq,*mut Irp);
type CsqPeek=unsafe extern "system" fn(*mut IoCsq,*mut Irp,*mut c_void)->*mut Irp;
type CsqAcquire=unsafe extern "system" fn(*mut IoCsq,*mut u8);
type CsqRelease=unsafe extern "system" fn(*mut IoCsq,u8);
type CsqCompleteCanceled=unsafe extern "system" fn(*mut IoCsq,*mut Irp);
const CSQ_IRP_CONTEXT_OFFSET:usize=0x90;
const IO_TYPE_CSQ:u32=2;
const IO_TYPE_CSQ_EX:u32=3;
const CSQ_CONTEXT_TYPE:u32=1;

unsafe fn csq_callback<T:Copy>(csq:*mut IoCsq,offset:usize)->T {
 unsafe{ptr::read_unaligned((csq.cast::<u8>().add(offset)).cast::<T>())}
}
unsafe fn csq_lock(csq:*mut IoCsq)->u8 {
 let acquire:CsqAcquire=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,32))};
 let mut irql=0u8;unsafe{acquire(csq,&mut irql)};irql
}
unsafe fn csq_unlock(csq:*mut IoCsq,irql:u8) {
 let release:CsqRelease=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,40))};
 unsafe{release(csq,irql)}
}
unsafe fn csq_set_cancel_routine(irp:*mut Irp,routine:*mut c_void)->*mut c_void {
 let field=unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_CANCEL_ROUTINE_OFFSET).cast::<core::sync::atomic::AtomicPtr<c_void>>()};
 unsafe{(&*field).swap(routine,core::sync::atomic::Ordering::AcqRel)}
}
unsafe extern "system" fn csq_cancel_routine(_device:*mut DeviceObject,irp:*mut Irp) {
 if irp.is_null(){return}
 let csq=unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),CSQ_IRP_CONTEXT_OFFSET).cast::<IoCsq>()};
 if csq.is_null(){return}
 let mut old_irql=0u8;unsafe{super::io_acquire_cancel_spin_lock(&mut old_irql);}
 let queue_irql=unsafe{csq_lock(csq)};
 let remove:CsqRemove=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,16))};
 unsafe{remove(csq,irp);}
 write_ptr(unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,ptr::null_mut());
 unsafe{csq_unlock(csq,queue_irql);}
 unsafe{super::io_release_cancel_spin_lock(old_irql);}
 let complete:CsqCompleteCanceled=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,48))};
 unsafe{complete(csq,irp);}
}
unsafe fn csq_initialize_common(csq:*mut IoCsq,insert:*mut c_void,extended:bool,remove:*mut c_void,peek:*mut c_void,acquire:*mut c_void,release:*mut c_void,complete:*mut c_void)->i32 {
 if csq.is_null()||insert.is_null()||remove.is_null()||peek.is_null()||acquire.is_null()||release.is_null()||complete.is_null(){return STATUS_INVALID_PARAMETER}
 unsafe{
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).type_),if extended {IO_TYPE_CSQ_EX}else{IO_TYPE_CSQ});
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).padding),0);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).insert_irp),insert);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).remove_irp),remove);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).peek_next_irp),peek);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).acquire_lock),acquire);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).release_lock),release);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).complete_canceled_irp),complete);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).reserve_pointer),ptr::null_mut());
 }
 STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_csq_initialize(csq:*mut IoCsq,insert:Option<CsqInsert>,remove:Option<CsqRemove>,peek:Option<CsqPeek>,acquire:Option<CsqAcquire>,release:Option<CsqRelease>,complete:Option<CsqCompleteCanceled>)->i32 {
 let (Some(insert),Some(remove),Some(peek),Some(acquire),Some(release),Some(complete))=(insert,remove,peek,acquire,release,complete) else{return STATUS_INVALID_PARAMETER};
 unsafe{csq_initialize_common(csq,insert as *const () as *mut c_void,false,remove as *const () as *mut c_void,peek as *const () as *mut c_void,acquire as *const () as *mut c_void,release as *const () as *mut c_void,complete as *const () as *mut c_void)}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_csq_initialize_ex(csq:*mut IoCsq,insert:Option<CsqInsertEx>,remove:Option<CsqRemove>,peek:Option<CsqPeek>,acquire:Option<CsqAcquire>,release:Option<CsqRelease>,complete:Option<CsqCompleteCanceled>)->i32 {
 let (Some(insert),Some(remove),Some(peek),Some(acquire),Some(release),Some(complete))=(insert,remove,peek,acquire,release,complete) else{return STATUS_INVALID_PARAMETER};
 unsafe{csq_initialize_common(csq,insert as *const () as *mut c_void,true,remove as *const () as *mut c_void,peek as *const () as *mut c_void,acquire as *const () as *mut c_void,release as *const () as *mut c_void,complete as *const () as *mut c_void)}
}
unsafe fn csq_insert_impl(csq:*mut IoCsq,irp:*mut Irp,context:*mut IoCsqIrpContext,insert_context:*mut c_void,extended_api:bool)->i32 {
 if csq.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let extended=unsafe{ptr::read_unaligned(csq.cast::<u8>().cast::<u32>())}==IO_TYPE_CSQ_EX;
 let mut old_irql=0u8;unsafe{super::io_acquire_cancel_spin_lock(&mut old_irql);}
 let queue_irql=unsafe{csq_lock(csq)};
 let cancelled=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x44)!=0};
 let mut status=STATUS_SUCCESS;
 if !cancelled {
  write_ptr(unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,csq.cast());
  let routine=csq_cancel_routine as *const () as *mut c_void;
  unsafe{csq_set_cancel_routine(irp,routine);}
  if extended {
   let insert:CsqInsertEx=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,8))};
   status=unsafe{insert(csq,irp,insert_context)};
  } else {
   let insert:CsqInsert=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,8))};
   unsafe{insert(csq,irp);}
  }
  if status!=STATUS_SUCCESS {
   unsafe{csq_set_cancel_routine(irp,ptr::null_mut());}
   write_ptr(unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,ptr::null_mut());
  } else if !context.is_null() {
   unsafe{ptr::write_unaligned(ptr::addr_of_mut!((*context).type_),CSQ_CONTEXT_TYPE);}
   unsafe{ptr::write_unaligned(ptr::addr_of_mut!((*context).irp),irp);}
   unsafe{ptr::write_unaligned(ptr::addr_of_mut!((*context).csq),csq);}
  }
 }
 unsafe{csq_unlock(csq,queue_irql);}
 unsafe{super::io_release_cancel_spin_lock(old_irql);}
 if cancelled {
  let complete:CsqCompleteCanceled=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,48))};
  unsafe{complete(csq,irp);}
  return 0xC0000120u32 as i32;
 }
 if status==STATUS_SUCCESS {unsafe{io_mark_irp_pending(irp);}}
 if extended_api {status}else{STATUS_SUCCESS}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_csq_insert_irp(csq:*mut IoCsq,irp:*mut Irp,context:*mut IoCsqIrpContext) {
 let _=unsafe{csq_insert_impl(csq,irp,context,ptr::null_mut(),false)};
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_csq_insert_irp_ex(csq:*mut IoCsq,irp:*mut Irp,context:*mut IoCsqIrpContext,insert_context:*mut c_void)->i32 {
 unsafe{csq_insert_impl(csq,irp,context,insert_context,true)}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_csq_remove_irp(csq:*mut IoCsq,context:*mut IoCsqIrpContext)->*mut Irp {
 if csq.is_null()||context.is_null(){return ptr::null_mut()}
 let irp=unsafe{ptr::read_unaligned(ptr::addr_of!((*context).irp))};
 if irp.is_null()||unsafe{ptr::read_unaligned(ptr::addr_of!((*context).csq))}!=csq{return ptr::null_mut()}
 let mut old_irql=0u8;unsafe{super::io_acquire_cancel_spin_lock(&mut old_irql);}
 let queue_irql=unsafe{csq_lock(csq)};
 let cancel=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x44)!=0};
 let routine=unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),IRP_CANCEL_ROUTINE_OFFSET)};
 let queue=unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),CSQ_IRP_CONTEXT_OFFSET)};
 if cancel||routine!=(csq_cancel_routine as *const () as *mut c_void)||queue!=csq.cast(){
  unsafe{csq_unlock(csq,queue_irql);}unsafe{super::io_release_cancel_spin_lock(old_irql);}
  return ptr::null_mut()
 }
 unsafe{csq_set_cancel_routine(irp,ptr::null_mut());}
 let remove:CsqRemove=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,16))};
 unsafe{remove(csq,irp);}
 write_ptr(unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,ptr::null_mut());
 unsafe{csq_unlock(csq,queue_irql);}unsafe{super::io_release_cancel_spin_lock(old_irql);}
 unsafe{
  ptr::write_unaligned(ptr::addr_of_mut!((*context).type_),0);
  ptr::write_unaligned(ptr::addr_of_mut!((*context).irp),ptr::null_mut());
  ptr::write_unaligned(ptr::addr_of_mut!((*context).csq),ptr::null_mut());
 }
 irp
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_csq_remove_next_irp(csq:*mut IoCsq,peek_context:*mut c_void)->*mut Irp {
 if csq.is_null(){return ptr::null_mut()}
 let mut old_irql=0u8;unsafe{super::io_acquire_cancel_spin_lock(&mut old_irql);}
 let queue_irql=unsafe{csq_lock(csq)};
 let peek:CsqPeek=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,24))};
 let candidate=unsafe{peek(csq,ptr::null_mut(),peek_context)};
 if candidate.is_null()||unsafe{*ptr::addr_of!((*candidate).bytes).cast::<u8>().add(0x44)!=0} {
  unsafe{csq_unlock(csq,queue_irql);}unsafe{super::io_release_cancel_spin_lock(old_irql);}
  return ptr::null_mut()
 }
 let routine=unsafe{read_ptr(ptr::addr_of!((*candidate).bytes).cast::<u8>(),IRP_CANCEL_ROUTINE_OFFSET)};
 let queue=unsafe{read_ptr(ptr::addr_of!((*candidate).bytes).cast::<u8>(),CSQ_IRP_CONTEXT_OFFSET)};
 if routine!=(csq_cancel_routine as *const () as *mut c_void)||queue!=csq.cast(){
  unsafe{csq_unlock(csq,queue_irql);}unsafe{super::io_release_cancel_spin_lock(old_irql);}
  return ptr::null_mut()
 }
 unsafe{csq_set_cancel_routine(candidate,ptr::null_mut());}
 let remove:CsqRemove=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,16))};
 unsafe{remove(csq,candidate);}
 write_ptr(unsafe{ptr::addr_of_mut!((*candidate).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,ptr::null_mut());
 unsafe{csq_unlock(csq,queue_irql);}unsafe{super::io_release_cancel_spin_lock(old_irql);}
 candidate
}

const IRP_CANCEL_ROUTINE_OFFSET:usize=0x68;
/// Atomically replace the IRP cancel callback and return the previous callback.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_set_cancel_routine(
 irp:*mut Irp, routine:Option<DriverCancelRoutine>
)->Option<DriverCancelRoutine> {
 if irp.is_null()||!acquire_irp_active_op(irp){return None}
 let slot=unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_CANCEL_ROUTINE_OFFSET).cast::<core::sync::atomic::AtomicPtr<c_void>>()};
 let replacement=routine.map_or(ptr::null_mut(),|r|r as *const () as *mut c_void);
 let previous=unsafe{&*slot}.swap(replacement,core::sync::atomic::Ordering::AcqRel);
 release_irp_active_op(irp);
 if previous.is_null(){None}else{Some(unsafe{core::mem::transmute::<*mut c_void,DriverCancelRoutine>(previous)})}
}

/// Cancel all builder-owned synchronous IRPs associated with an exiting thread.
/// Detach stack-backed IOSB/event pointers before cancellation: a lower driver
/// may finish later if it has no cancel routine or cannot stop I/O immediately.
pub fn io_cancel_thread_irps(thread_id:u64)->usize {
 if thread_id==0{return 0}
 let mut total=0usize;
 loop {
  let mut pending=[(ptr::null_mut::<Irp>(),false);IRP_CANCEL_BATCH];
  let mut count=0usize;
  let mut completion_in_progress=false;
  let mut more=false;
  {
   let mut state=WDM.lock();
   let Some(qi)=state.thread_irps.iter().position(|q|q.used&&q.thread_id==thread_id) else{break};
   let mut cursor=state.thread_irps[qi].head;
   while !cursor.is_null(){
    let Some(index)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,cursor)) else{break};
    let next=state.irps[index].sync_next;
    if state.irps[index].completion_state==1||state.irps[index].active_ops!=0 {
     completion_in_progress=true;cursor=next;continue
    }
    if count==pending.len(){more=true;break}
    let never_dispatched=state.irps[index].outstanding_count==0&&state.irps[index].completion_state==0;
    let _=sync_queue_detach_locked(&mut state,cursor);
    write_ptr(unsafe{ptr::addr_of_mut!((*cursor).bytes).cast::<u8>()},0x48,ptr::null_mut());
    pending[count]=(cursor,never_dispatched);count+=1;cursor=next;
   }
   more|=state.thread_irps.iter().any(|q|q.used&&q.thread_id==thread_id);
  }
  for (irp,never_dispatched) in pending[..count].iter().copied(){
   let cancel_routine_called=unsafe{io_cancel_irp(irp)}!=0;
   if never_dispatched&&!cancel_routine_called {
    unsafe{
     ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),0xC0000120u32 as i32);
     ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),0);
     io_complete_request(irp,0);
    }
   }
  }
  total+=count;
  if more {
   if count==0&&completion_in_progress{core::hint::spin_loop();}
   continue
  }
  if !completion_in_progress {break}
  core::hint::spin_loop();
 }
 total
}

/// Request cancellation using the global cancel-spin-lock protocol. A driver
/// callback is invoked only after the lock and raised IRQL have been released.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_cancel_irp(irp:*mut Irp)->u8 {
 if irp.is_null(){return 0}
 if !acquire_irp_active_op(irp){return 0}
 let mut old_irql=0u8;
 unsafe{super::io_acquire_cancel_spin_lock(&mut old_irql);}
 unsafe{*ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x44)=1;}
 let cancel_slot=unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_CANCEL_ROUTINE_OFFSET).cast::<core::sync::atomic::AtomicPtr<c_void>>()};
 let callback=unsafe{&*cancel_slot}.swap(ptr::null_mut(),core::sync::atomic::Ordering::AcqRel);
 unsafe{super::io_release_cancel_spin_lock(old_irql);}
 if callback.is_null(){release_irp_active_op(irp);try_free_completed_sync_ioctl_irp(irp);return 0}
 let routine:DriverCancelRoutine=unsafe{core::mem::transmute(callback)};
 let device={
  let state=WDM.lock();
  if let Some(slot)=state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp)) {
   let count=slot.object.bytes[0x42] as usize;
   let stack=unsafe{io_get_current_irp_stack_location(irp)};
   let base=slot.stack.as_ptr() as usize;
   let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
   let cur=stack as usize;
   if count>0&&count<=slot.stack.len()&&cur>=base&&cur<end&&(cur-base)%IO_STACK_LOCATION_SIZE_X64==0 {
    unsafe{(*stack).device_object}
   } else {ptr::null_mut()}
  } else {ptr::null_mut()}
 };
 unsafe{routine(device,irp);}
 // The cancel routine owns stopping the device; once it returns, drop any
 // remaining IRP-bound IOMMU mappings. Completion performs the same idempotent rundown.
 crate::dma::release_irp_mappings(irp.cast());
 release_irp_active_op(irp);
 try_free_completed_sync_ioctl_irp(irp);
 1
}


/// Resolve a counted device name to a registered device object. This is the
/// bounded name lookup primitive used by future IoGetDeviceObjectPointer and
/// mount-manager integration; it does not create a FILE_OBJECT.
pub unsafe fn lookup_device_object(name:*mut UnicodeString)->*mut DeviceObject {
    let Some((len,buffer))=valid_unicode_name(name) else{return ptr::null_mut()};
    let mut copy=[0u16;MAX_SYMBOLIC_LINK_CHARS];
    unsafe{ptr::copy_nonoverlapping(buffer,copy.as_mut_ptr(),len);}
    let state=WDM.lock();
    state.devices.iter().find(|s|s.used&&!s.deleting&&s.name_len==len&&unicode_equal_folded(&s.name[..s.name_len],&copy[..len])).map_or(ptr::null_mut(),|s|ptr::addr_of!(s.object) as *mut DeviceObject)
}


/// Build a bounded asynchronous READ/WRITE IRP for a device that uses neither
/// buffered nor direct I/O. Buffered-I/O system buffers and MDL-backed direct
/// I/O are rejected until their full lifetime/locking contracts are available.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_build_asynchronous_fsd_request(
 major:u32, device:*mut DeviceObject, buffer:*mut c_void, length:u32,
 starting_offset:*const i64, io_status:*mut IoStatusBlock
)->*mut Irp {
 if device.is_null()||io_status.is_null()||(major!=3&&major!=4)||(length!=0&&buffer.is_null()){return ptr::null_mut()}
 let (stack_size,flags)={let state=WDM.lock();let Some(slot)=state.devices.iter().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)) else{return ptr::null_mut()};(slot.object.stack_size,slot.object.flags)};
 // Buffered I/O requires a dedicated system-buffer pool, so fail closed.
 if flags&0x4!=0{return ptr::null_mut()}
 let offset=if starting_offset.is_null(){0}else{unsafe{starting_offset.read_unaligned()}};
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){return ptr::null_mut()}
 let next=unsafe{io_get_next_irp_stack_location(irp)};
 if next.is_null(){unsafe{io_free_irp(irp)};return ptr::null_mut()}
 let mut owned_mdl=ptr::null_mut();
 if flags&0x2!=0 {
  owned_mdl=unsafe{io_allocate_mdl(buffer,length,0,0,irp)};
  if owned_mdl.is_null(){unsafe{io_free_irp(irp)};return ptr::null_mut()}
  // A READ writes into the caller's buffer; a WRITE reads from it.
  unsafe{crate::ntoskrnl::mm::runtime::mm_probe_and_lock_pages(owned_mdl,0,if major==3{1}else{0});}
  let mdl_flags=unsafe{ptr::read_unaligned(owned_mdl.cast::<u8>().add(0x0a).cast::<i16>())};
  if mdl_flags&0x0002==0 {unsafe{io_free_mdl(owned_mdl);io_free_irp(irp);}return ptr::null_mut()}
  let mut state=WDM.lock();
  if let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)){slot.owned_mdl=owned_mdl;}
 }
 unsafe{
  (*next).major_function=major as u8;
  // READ/WRITE layout uses ULONG Length at union offset 0 and
  // LARGE_INTEGER ByteOffset at union offset 0x10 on Windows x64.
  ptr::write_unaligned(next.cast::<u8>().add(4).cast::<u32>(),length);
  ptr::write_unaligned(next.cast::<u8>().add(20).cast::<i64>(),offset);
  write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),0x70,buffer);
  write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),0x48,io_status.cast());
  ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_PENDING);
  ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),0);
 }
 irp
}


/// Allocate an MDL descriptor and optionally link it into an IRP's MDL chain.
/// Page locking is deliberately separate, matching MmProbeAndLockPages.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_allocate_mdl(
 base:*mut c_void,length:u32,secondary_buffer:u8,charge_quota:u8,irp:*mut Irp
)->*mut c_void {
 if base.is_null()||length==0||secondary_buffer>1||charge_quota!=0{return ptr::null_mut()}
 if !irp.is_null() {
  let valid={let state=WDM.lock();state.irps.iter().any(|s|s.used&&ptr::eq(&s.object,irp))};
  if !valid{return ptr::null_mut()}
 }
 let mdl=unsafe{crate::ntoskrnl::mm::runtime::mm_create_mdl(ptr::null_mut(),base,length as u64)};
 if mdl.is_null(){return ptr::null_mut()}
 if !irp.is_null() {
  let mut state=WDM.lock();
  let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else {
   unsafe{crate::ntoskrnl::mm::runtime::mm_free_mdl(mdl);}
   return ptr::null_mut()
  };
  let head=unsafe{read_ptr(slot.object.bytes.as_ptr(),0x08)};
  if secondary_buffer==0 {
   unsafe{write_ptr(mdl.cast(),0,head);}
   unsafe{write_ptr(slot.object.bytes.as_mut_ptr(),0x08,mdl);}
  } else if head.is_null() {
   unsafe{write_ptr(slot.object.bytes.as_mut_ptr(),0x08,mdl);}
  } else {
   let mut tail=head;
   let mut found=false;
   for _ in 0..64 {
    let next=unsafe{read_ptr(tail.cast(),0)};
    if next.is_null(){unsafe{write_ptr(tail.cast(),0,mdl);}found=true;break}
    tail=next;
   }
   if !found {
    drop(state);
    unsafe{crate::ntoskrnl::mm::runtime::mm_free_mdl(mdl);}
    return ptr::null_mut()
   }
  }
 }
 mdl
}

/// Unlink and free an MDL allocated by IoAllocateMdl.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn io_free_mdl(mdl:*mut c_void) {
 if mdl.is_null(){return}
 {
  let mut state=WDM.lock();
  for slot in state.irps.iter_mut().filter(|s|s.used) {
   if slot.owned_mdl==mdl {slot.owned_mdl=ptr::null_mut();}
   let mut head=unsafe{read_ptr(slot.object.bytes.as_ptr(),0x08)};
   if head==mdl {
    head=unsafe{read_ptr(mdl.cast(),0)};
    unsafe{write_ptr(slot.object.bytes.as_mut_ptr(),0x08,head);}
    continue
   }
   let mut cursor=head;
   for _ in 0..64 {
    if cursor.is_null(){break}
    let next=unsafe{read_ptr(cursor.cast(),0)};
    if next==mdl {
     let after=unsafe{read_ptr(mdl.cast(),0)};
     unsafe{write_ptr(cursor.cast(),0,after);}
     break
    }
    cursor=next;
   }
  }
 }
 unsafe{crate::ntoskrnl::mm::runtime::mm_free_mdl(mdl);}
}



static RUNDOWN_SELFTEST_COMPLETIONS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static RUNDOWN_SELFTEST_UNLOADS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
unsafe extern "system" fn rundown_selftest_pending(_device:*mut DeviceObject,_irp:*mut Irp)->i32 { STATUS_PENDING }
unsafe extern "system" fn rundown_selftest_completion(_device:*mut DeviceObject,_irp:*mut Irp,_context:*mut c_void)->i32 {
 RUNDOWN_SELFTEST_COMPLETIONS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);STATUS_SUCCESS
}
unsafe extern "system" fn rundown_selftest_unload(_driver:*mut DriverObject) {
 RUNDOWN_SELFTEST_UNLOADS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
}

/// Boot-time regression for the actual WDM implementation: a pending IRP must
/// pin its device across dispatch return and unload; concurrent/repeated
/// completion must run its callback and the unload callback exactly once.
pub(crate) fn rundown_self_test()->bool {
 RUNDOWN_SELFTEST_COMPLETIONS.store(0,core::sync::atomic::Ordering::Release);
 RUNDOWN_SELFTEST_UNLOADS.store(0,core::sync::atomic::Ordering::Release);
 let mut driver:DriverObject=unsafe{core::mem::zeroed()};
 driver.driver_unload=Some(rundown_selftest_unload);
 driver.major_function[0]=Some(rundown_selftest_pending);
 let mut device: *mut DeviceObject=ptr::null_mut();
 if unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)}!=STATUS_SUCCESS{return false}
 let irp=unsafe{io_allocate_irp(2,0)};
 if irp.is_null(){unsafe{io_delete_device(device)};return false}
 unsafe {
  let next=io_get_next_irp_stack_location(irp);
  if next.is_null(){io_free_irp(irp);io_delete_device(device);return false}
  (*next).major_function=0;
  io_set_completion_routine(irp,Some(rundown_selftest_completion),ptr::null_mut(),1,1,1);
 }
 if unsafe{io_call_driver(device,irp)}!=STATUS_PENDING {
  unsafe{io_free_irp(irp);io_delete_device(device);}
  return false
 }
 if unsafe{io_unload_driver(&mut driver)}!=STATUS_PENDING
  || RUNDOWN_SELFTEST_UNLOADS.load(core::sync::atomic::Ordering::Acquire)!=0 {
  unsafe{io_free_irp(irp);}
  return false
 }
 unsafe{io_complete_request(irp,0);io_complete_request(irp,0);}
 let completion_ok=RUNDOWN_SELFTEST_COMPLETIONS.load(core::sync::atomic::Ordering::Acquire)==1;
 let unload_status=unsafe{io_unload_driver(&mut driver)};
 let unload_ok=unload_status==STATUS_SUCCESS
  && RUNDOWN_SELFTEST_UNLOADS.load(core::sync::atomic::Ordering::Acquire)==1;
 unsafe{io_free_irp(irp);}
 completion_ok&&unload_ok
}

#[cfg(test)]
mod wdm_tests {
 use super::*;
 #[test]
 fn x64_wdm_prefix_layouts_match_expected_offsets() {
  assert_eq!(core::mem::size_of::<UnicodeString>(),16);
  assert_eq!(core::mem::size_of::<IoStatusBlock>(),16);
  assert_eq!(core::mem::offset_of!(IoStatusBlock,status),0x00);
  assert_eq!(core::mem::offset_of!(IoStatusBlock,information),0x08);
  assert_eq!(core::mem::size_of::<IoStackLocation>(),IO_STACK_LOCATION_SIZE_X64);
  assert_eq!(core::mem::offset_of!(IoStackLocation,major_function),0x00);
  assert_eq!(core::mem::offset_of!(IoStackLocation,parameters),0x04);
  assert_eq!(core::mem::offset_of!(IoStackLocation,device_object),0x28);
  assert_eq!(core::mem::offset_of!(IoStackLocation,file_object),0x30);
  assert_eq!(core::mem::offset_of!(IoStackLocation,completion_routine),0x38);
  assert_eq!(core::mem::offset_of!(IoStackLocation,context),0x40);
  assert_eq!(core::mem::offset_of!(DriverObject,device_object),0x08);
  assert_eq!(core::mem::offset_of!(DriverObject,flags),0x10);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_start),0x18);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_size),0x20);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_section),0x28);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_extension),0x30);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_name),0x38);
  assert_eq!(core::mem::offset_of!(DriverObject,hardware_database),0x48);
  assert_eq!(core::mem::offset_of!(DriverObject,fast_io_dispatch),0x50);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_init),0x58);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_start_io),0x60);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_unload),0x68);
  assert_eq!(core::mem::offset_of!(DriverObject,major_function),0x70);
  assert_eq!(core::mem::size_of::<DriverObject>(),0x150);
  assert_eq!(core::mem::offset_of!(DeviceObject,object_type),0x00);
  assert_eq!(core::mem::offset_of!(DeviceObject,size),0x02);
  assert_eq!(core::mem::offset_of!(DeviceObject,reference_count),0x04);
  assert_eq!(core::mem::offset_of!(DeviceObject,driver_object),0x08);
  assert_eq!(core::mem::offset_of!(DeviceObject,next_device),0x10);
  assert_eq!(core::mem::offset_of!(DeviceObject,attached_device),0x18);
  assert_eq!(core::mem::offset_of!(DeviceObject,current_irp),0x20);
  assert_eq!(core::mem::offset_of!(DeviceObject,timer),0x28);
  assert_eq!(core::mem::offset_of!(DeviceObject,flags),0x30);
  assert_eq!(core::mem::offset_of!(DeviceObject,characteristics),0x34);
  assert_eq!(core::mem::offset_of!(DeviceObject,vpb),0x38);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_extension),0x40);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_type),0x48);
  assert_eq!(core::mem::offset_of!(DeviceObject,stack_size),0x4c);
  assert_eq!(core::mem::offset_of!(DeviceObject,queue),0x50);
  assert_eq!(core::mem::offset_of!(DeviceObject,alignment_requirement),0x98);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_queue),0xa0);
  assert_eq!(core::mem::offset_of!(DeviceObject,dpc),0xc8);
  assert_eq!(core::mem::offset_of!(DeviceObject,active_thread_count),0x108);
  assert_eq!(core::mem::offset_of!(DeviceObject,security_descriptor),0x110);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_lock),0x118);
  assert_eq!(core::mem::offset_of!(DeviceObject,sector_size),0x130);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_object_extension),0x138);
  assert_eq!(core::mem::offset_of!(DeviceObject,reserved),0x140);
  assert_eq!(core::mem::size_of::<DeviceObject>(),0x150);
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
  unsafe {
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   io_complete_request(irp,0);
  }
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
  assert_eq!(unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET))},0);
  unsafe{io_free_irp(irp);}
  // Completion and IRP free release rundown; they must not implicitly delete
  // a device whose owner has not called IoDeleteDevice.
  assert_eq!(driver.device_object,device);
  unsafe{io_delete_device(device);}
 }
 #[test]
 fn removal_waits_for_remove_lock_and_reference_rundown() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};let mut d:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut d)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_reference_device_object(d)},d);
  assert_eq!(unsafe{io_acquire_device_remove_lock(d)},STATUS_SUCCESS);
  unsafe{io_delete_device(d);}
  assert_eq!(unsafe{io_acquire_device_remove_lock(d)},STATUS_DELETE_PENDING);
  unsafe{io_dereference_device_object(d);}
  assert!(!driver.device_object.is_null());
  unsafe{io_release_device_remove_lock(d);}
  assert!(driver.device_object.is_null());
 }
 #[test]
 fn power_transition_serializes_and_commits_only_success() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};let mut d:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut d)},STATUS_SUCCESS);
  assert_eq!(device_power_state(d),0);
  assert_eq!(unsafe{io_begin_device_power_transition(d,DEVICE_POWER_D3)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_begin_device_power_transition(d,DEVICE_POWER_D1)},STATUS_DEVICE_BUSY);
  assert_eq!(unsafe{io_complete_device_power_transition(d,DEVICE_POWER_D3,STATUS_INVALID_DEVICE_REQUEST)},STATUS_INVALID_DEVICE_REQUEST);
  assert_eq!(device_power_state(d),0);
  assert_eq!(unsafe{io_begin_device_power_transition(d,DEVICE_POWER_D3)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_complete_device_power_transition(d,DEVICE_POWER_D3,STATUS_SUCCESS)},STATUS_SUCCESS);
  assert_eq!(device_power_state(d),3);unsafe{io_delete_device(d);}
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

  let irp=unsafe{io_allocate_irp(32,0)};
  assert!(!irp.is_null());
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42)},32);
  assert_eq!(unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(2).cast::<u16>())},IRP_SIZE_X64 as u16);
  assert_eq!(unsafe{io_get_current_irp_stack_location(irp)} as usize,irp as usize+IRP_SIZE_X64+32*IO_STACK_LOCATION_SIZE_X64);
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
 fn three_level_filter_stack_detaches_only_from_the_top_down() {
  let mut base_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut filter1_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut filter2_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut base:*mut DeviceObject=ptr::null_mut();
  let mut filter1:*mut DeviceObject=ptr::null_mut();
  let mut filter2:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut base_driver,0,ptr::null_mut(),7,0,0,&mut base)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut filter1_driver,0,ptr::null_mut(),7,0,0,&mut filter1)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut filter2_driver,0,ptr::null_mut(),7,0,0,&mut filter2)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_attach_device_to_device_stack(filter1,base)},base);
  assert_eq!(unsafe{io_attach_device_to_device_stack(filter2,base)},filter1);
  assert_eq!(unsafe{(*filter1).stack_size},2);
  assert_eq!(unsafe{(*filter2).stack_size},3);
  unsafe{io_detach_device(base);}
  assert_eq!(unsafe{(*base).attached_device},filter1,"lower filter must not be detached under an attached upper filter");
  unsafe{io_detach_device(filter1);io_detach_device(base);}
  assert!(unsafe{(*filter1).attached_device.is_null()});
  assert!(unsafe{(*base).attached_device.is_null()});
  unsafe{io_delete_device(filter2);io_delete_device(filter1);io_delete_device(base);}
 }

 #[test]
 fn initialize_irp_rejects_short_packet_and_sets_stack_bounds() {
  let mut storage=[0u8;IRP_SIZE_X64+2*IO_STACK_LOCATION_SIZE_X64];
  let irp=storage.as_mut_ptr().cast::<Irp>();
  unsafe{io_initialize_irp(irp, (storage.len()-1) as u16, 2);}
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42)},0);
  unsafe{io_initialize_irp(irp, storage.len() as u16, 2);}
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42)},2);
  assert_eq!(unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(2).cast::<u16>())},IRP_SIZE_X64 as u16);
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
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)},3);
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

 #[test]
 fn symbolic_links_validate_names_collide_case_insensitively_and_delete() {
  let mut link=[b'\\' as u16,b'?' as u16,b'?' as u16,b'\\' as u16,b'X' as u16];
  let mut target=[b'\\' as u16,b'D' as u16,b'e' as u16,b'v' as u16,b'i' as u16,b'c' as u16,b'e' as u16,b'\\' as u16,b'E' as u16,b'x' as u16,b't' as u16,b'4' as u16];
  let mut link_name=UnicodeString{length:(link.len()*2) as u16,maximum_length:(link.len()*2) as u16,buffer:link.as_mut_ptr()};
  let mut target_name=UnicodeString{length:(target.len()*2) as u16,maximum_length:(target.len()*2) as u16,buffer:target.as_mut_ptr()};
  assert_eq!(unsafe{io_create_symbolic_link(&mut link_name,&mut target_name)},STATUS_SUCCESS);
  let mut lower_link=[b'\\' as u16,b'?' as u16,b'?' as u16,b'\\' as u16,b'x' as u16];
  let mut lower_name=UnicodeString{length:(lower_link.len()*2) as u16,maximum_length:(lower_link.len()*2) as u16,buffer:lower_link.as_mut_ptr()};
  assert_eq!(unsafe{io_create_symbolic_link(&mut lower_name,&mut target_name)},STATUS_OBJECT_NAME_COLLISION);
  assert_eq!(unsafe{io_delete_symbolic_link(&mut lower_name)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_delete_symbolic_link(&mut lower_name)},STATUS_OBJECT_NAME_NOT_FOUND);
 }

 static COMPLETION_CALLBACKS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "system" fn counted_completion(_device:*mut DeviceObject,_irp:*mut Irp,_context:*mut c_void)->i32 {
  COMPLETION_CALLBACKS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);STATUS_SUCCESS
 }
 unsafe extern "system" fn pending_dispatch(_device:*mut DeviceObject,irp:*mut Irp)->i32 {
  unsafe{io_mark_irp_pending(irp);}
  STATUS_PENDING
 }
 static CANCEL_CALLED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
 unsafe extern "system" fn test_cancel(device:*mut DeviceObject,irp:*mut Irp) {
  if !irp.is_null() { CANCEL_CALLED.store(true,core::sync::atomic::Ordering::Release); }
  let _=device;
 }
 #[test]
 fn completion_claim_and_driver_unload_wait_for_outstanding_irp() {
  COMPLETION_CALLBACKS.store(0,core::sync::atomic::Ordering::Release);
  UNLOAD_CALLS.store(0,core::sync::atomic::Ordering::Release);
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.driver_unload=Some(counted_driver_unload);
  driver.major_function[0]=Some(pending_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe {
   (*io_get_next_irp_stack_location(irp)).major_function=0;
   io_set_completion_routine(irp,Some(counted_completion),ptr::null_mut(),1,1,1);
  }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
  // A pending IRP pins its device beyond the return from the dispatch routine.
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_PENDING);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),0);
  unsafe{io_complete_request(irp,0);io_complete_request(irp,0);}
  assert_eq!(COMPLETION_CALLBACKS.load(core::sync::atomic::Ordering::Acquire),1);
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_SUCCESS);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),1);
  unsafe{io_free_irp(irp);}
 }

 static IOCTL_COMPLETIONS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "system" fn ioctl_buffered_dispatch(_device:*mut DeviceObject,irp:*mut Irp)->i32 {
  let stack=unsafe{io_get_current_irp_stack_location(irp)};
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(4).cast::<u32>())},8);
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(12).cast::<u32>())},2);
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(20).cast::<u32>())},0x00222000);
  let system_buffer=unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),0x18).cast::<u8>()};
  if !system_buffer.is_null(){unsafe{ptr::copy_nonoverlapping(b"OK".as_ptr(),system_buffer,2);}}
  unsafe{
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),2);
   io_complete_request(irp,0);
  }
  STATUS_SUCCESS
 }
 unsafe extern "system" fn ioctl_counted_completion(_device:*mut DeviceObject,_irp:*mut Irp,_context:*mut c_void)->i32 {
  IOCTL_COMPLETIONS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
  STATUS_SUCCESS
 }
 #[test]
 fn build_device_io_control_request_copies_buffered_output_and_auto_frees() {
  IOCTL_COMPLETIONS.store(0,core::sync::atomic::Ordering::Release);
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0x0e]=Some(ioctl_buffered_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let input=*b"IN";
  let mut output=[0u8;8];
  let mut iosb:IoStatusBlock=unsafe{core::mem::zeroed()};
  let mut event=core::mem::MaybeUninit::<crate::ntoskrnl::ke::runtime::DispatcherHeader>::uninit();
  unsafe{crate::ntoskrnl::ke::runtime::ke_initialize_event(event.as_mut_ptr(),0,0);}
  // METHOD_BUFFERED = 0; the notification event must signal only after the
  // output copy and IOSB publication are complete.
  let irp=unsafe{io_build_device_io_control_request(0x00222000,device,input.as_ptr() as *mut c_void,2,output.as_mut_ptr().cast(),output.len() as u32,0,event.as_mut_ptr().cast(),&mut iosb)};
  assert!(!irp.is_null());
  unsafe{io_set_completion_routine(irp,Some(ioctl_counted_completion),ptr::null_mut(),1,1,1);}
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert_eq!(unsafe{iosb.status.status},STATUS_SUCCESS);
  assert_eq!(iosb.information,2);
  assert_eq!(unsafe{crate::ntoskrnl::ke::runtime::ke_read_state_event(event.as_ptr())},1);
  assert_eq!(&output[..2],b"OK");
  assert_eq!(IOCTL_COMPLETIONS.load(core::sync::atomic::Ordering::Acquire),1);
  // The builder-owned IRP is reclaimed at terminal completion.
  assert!(!WDM.lock().irps.iter().any(|s|s.used&&ptr::eq(&s.object,irp)));
  unsafe{io_delete_device(device);}
 }

 #[test]
 fn thread_exit_detaches_and_cancels_builder_owned_pending_irps() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0x0e]=Some(pending_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let mut iosb:IoStatusBlock=unsafe{core::mem::zeroed()};
  let irp=unsafe{io_build_device_io_control_request(0x00222003,device,ptr::null_mut(),0,ptr::null_mut(),0,0,ptr::null_mut(),&mut iosb)};
  assert!(!irp.is_null());
  // Simulate the current-thread queue entry without requiring a live scheduler
  // thread in this host-side unit test.
  {
   let mut state=WDM.lock();
   let _=sync_queue_detach_locked(&mut state,irp);
   assert!(sync_queue_attach_locked(&mut state,irp,0xBEEF));
  }
  unsafe{io_set_completion_routine(irp,Some(counted_completion),ptr::null_mut(),1,1,1);}
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
  assert_eq!(io_cancel_thread_irps(0xBEEF),1);
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x44)},1);
  assert!(unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),0x48)}.is_null());
  // With no lower cancel routine, the IRP stays alive until the driver completes
  // it; late completion must not touch the exiting thread's IOSB.
  unsafe{io_complete_request(irp,0);}
  assert!(!WDM.lock().irps.iter().any(|s|s.used&&ptr::eq(&s.object,irp)));
  unsafe{io_delete_device(device);}
 }

 #[test]
 fn free_irp_does_not_reclaim_a_pending_request() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0]=Some(pending_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe{(*io_get_next_irp_stack_location(irp)).major_function=0;}
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x41)},0);
  assert_ne!(unsafe{(*io_get_current_irp_stack_location(irp)).control}&0x01,0);
  // A premature free is rejected; the same IRP remains registered and
  // its outstanding completion can still finish safely.
  unsafe{io_free_irp(irp);}
  assert!(!unsafe{io_get_current_irp_stack_location(irp)}.is_null());
  unsafe{io_complete_request(irp,0);}
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }

 static MORE_PROCESSING_CALLBACKS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "system" fn stop_completion_once(_device:*mut DeviceObject,_irp:*mut Irp,_context:*mut c_void)->i32 {
  MORE_PROCESSING_CALLBACKS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
  0xC0000016u32 as i32
 }
 unsafe extern "system" fn free_then_stop_completion(_device:*mut DeviceObject,irp:*mut Irp,_context:*mut c_void)->i32 {
  unsafe{io_free_irp(irp);}
  0xC0000016u32 as i32
 }
 #[test]
 fn free_irp_inside_more_processing_callback_is_deferred_until_safe() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0]=Some(completing_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe{
   (*io_get_next_irp_stack_location(irp)).major_function=0;
   io_set_completion_routine(irp,Some(free_then_stop_completion),ptr::null_mut(),1,1,1);
  }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert!(!WDM.lock().irps.iter().any(|s|s.used&&ptr::eq(&s.object,irp)));
  unsafe{io_delete_device(device);}
 }

 #[test]
 fn reuse_after_more_processing_required_releases_old_stack_references() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0]=Some(completing_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe{
   (*io_get_next_irp_stack_location(irp)).major_function=0;
   io_set_completion_routine(irp,Some(stop_completion_once),ptr::null_mut(),1,1,1);
  }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  unsafe{io_reuse_irp(irp,STATUS_SUCCESS);}
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)},3);
  assert!(!unsafe{io_get_next_irp_stack_location(irp)}.is_null());
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }

 #[test]
 fn more_processing_required_advances_past_the_callback_before_resume() {
  MORE_PROCESSING_CALLBACKS.store(0,core::sync::atomic::Ordering::Release);
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0]=Some(completing_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe {
   (*io_get_next_irp_stack_location(irp)).major_function=0;
   io_set_completion_routine(irp,Some(stop_completion_once),ptr::null_mut(),1,1,1);
  }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert_eq!(MORE_PROCESSING_CALLBACKS.load(core::sync::atomic::Ordering::Acquire),1);
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)},3);
  unsafe{io_complete_request(irp,0);}
  assert_eq!(MORE_PROCESSING_CALLBACKS.load(core::sync::atomic::Ordering::Acquire),1);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }

 #[test]
 fn cancel_routine_is_atomically_claimed_and_called_once() {
  CANCEL_CALLED.store(false,core::sync::atomic::Ordering::Release);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  assert!(unsafe{io_set_cancel_routine(irp,Some(test_cancel))}.is_none());
  assert_eq!(unsafe{io_cancel_irp(irp)},1);
  assert!(CANCEL_CALLED.load(core::sync::atomic::Ordering::Acquire));
  assert_eq!(unsafe{io_cancel_irp(irp)},0);
  assert!(unsafe{io_set_cancel_routine(irp,None)}.is_none());
  unsafe{io_free_irp(irp);}
 }

 #[test]
 fn named_device_lookup_and_duplicate_rejection() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut duplicate_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut name=std::string::String::from("\\Device\\Ext4");
  let mut utf16=name.encode_utf16().collect::<std::vec::Vec<u16>>();
  let mut unicode=UnicodeString{length:(utf16.len()*2) as u16,maximum_length:(utf16.len()*2) as u16,buffer:utf16.as_mut_ptr()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,&mut unicode,7,0,0,&mut device)},STATUS_SUCCESS);
  assert_eq!(unsafe{lookup_device_object(&mut unicode)},device);
  let mut duplicate:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut duplicate_driver,0,&mut unicode,7,0,0,&mut duplicate)},STATUS_OBJECT_NAME_COLLISION);
  unsafe{io_delete_device(device);}
  assert!(unsafe{lookup_device_object(&mut unicode)}.is_null());
 }

 #[test]
 fn asynchronous_fsd_request_sets_read_parameters_and_iosb() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let mut status=IoStatusBlock { status: crate::nt_abi::io::IO_STATUS_BLOCK_STATUS { status: STATUS_PENDING }, information: 0 };
  let mut buffer=[0u8;512];
  let offset=4096i64;
  let irp=unsafe{io_build_asynchronous_fsd_request(3,device,buffer.as_mut_ptr().cast(),buffer.len() as u32,&offset,&mut status)};
  assert!(!irp.is_null());
  let stack=unsafe{io_get_next_irp_stack_location(irp)};
  assert_eq!(unsafe{(*stack).major_function},3);
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(4).cast::<u32>())},512);
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(20).cast::<i64>())},4096);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }

 unsafe extern "system" fn completing_dispatch(device:*mut DeviceObject,irp:*mut Irp)->i32 {
  if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
  unsafe{
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),123);
   io_complete_request(irp,0);
  }
  STATUS_SUCCESS
 }
 #[test]
 fn completed_async_request_publishes_iosb_status_and_information() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[3]=Some(completing_dispatch);
  let mut status=IoStatusBlock { status: crate::nt_abi::io::IO_STATUS_BLOCK_STATUS { status: STATUS_PENDING }, information: 0 };
  let mut buffer=[0u8;512];
  let irp=unsafe{io_build_asynchronous_fsd_request(3,device,buffer.as_mut_ptr().cast(),buffer.len() as u32,ptr::null(),&mut status)};
  assert!(!irp.is_null());
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert_eq!(unsafe { status.status.status }, STATUS_SUCCESS);
  assert_eq!(status.information,123);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }


 static UNLOAD_CALLS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "system" fn counted_driver_unload(_driver:*mut DriverObject){
  UNLOAD_CALLS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
 }
 #[test]
 fn driver_unload_waits_for_remove_lock_and_calls_callback_once(){
  UNLOAD_CALLS.store(0,core::sync::atomic::Ordering::Release);
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.driver_unload=Some(counted_driver_unload);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_acquire_device_remove_lock(device)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_PENDING);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),0);
  unsafe{io_release_device_remove_lock(device);}
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_SUCCESS);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),1);
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_SUCCESS);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),1);
  let mut rejected:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut rejected)},STATUS_DELETE_PENDING);
  assert!(rejected.is_null());
 }
 unsafe extern "system" fn pnp_start_pending(_device:*mut DeviceObject,_irp:*mut Irp)->i32 { STATUS_PENDING }
 #[test]
 fn pnp_pending_start_does_not_publish_started_until_completion() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0x1b]=Some(pnp_start_pending);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  let stack=unsafe{io_get_next_irp_stack_location(irp)};
  unsafe{(*stack).major_function=0x1b;(*stack).minor_function=0x00;}
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
  assert_eq!(io_pnp_state(device),PNP_STATE_NOT_STARTED);
  unsafe{
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   io_complete_request(irp,0);
  }
  assert_eq!(io_pnp_state(device),PNP_STATE_STARTED);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }
 unsafe extern "system" fn pnp_start_complete_success(_device:*mut DeviceObject,irp:*mut Irp)->i32 {
  unsafe {
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   io_complete_request(irp,0);
  }
  STATUS_SUCCESS
 }
 unsafe extern "system" fn pnp_start_complete_failure(_device:*mut DeviceObject,irp:*mut Irp)->i32 {
  unsafe {
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_INVALID_DEVICE_REQUEST);
   io_complete_request(irp,0);
  }
  STATUS_INVALID_DEVICE_REQUEST
 }
 #[test]
 fn pnp_state_changes_only_after_terminal_completion() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0x1b]=Some(pnp_start_complete_failure);
  let failed=unsafe{io_allocate_irp(2,0)};
  let stack=unsafe{io_get_next_irp_stack_location(failed)};
  unsafe{(*stack).major_function=0x1b;(*stack).minor_function=0x00;}
  assert_eq!(unsafe{io_call_driver(device,failed)},STATUS_INVALID_DEVICE_REQUEST);
  assert_eq!(io_pnp_state(device),PNP_STATE_NOT_STARTED);
  unsafe{io_free_irp(failed);}
  driver.major_function[0x1b]=Some(pnp_start_complete_success);
  let started=unsafe{io_allocate_irp(2,0)};
  let stack=unsafe{io_get_next_irp_stack_location(started)};
  unsafe{(*stack).major_function=0x1b;(*stack).minor_function=0x00;}
  assert_eq!(unsafe{io_call_driver(device,started)},STATUS_SUCCESS);
  assert_eq!(io_pnp_state(device),PNP_STATE_STARTED);
  unsafe{io_free_irp(started);io_delete_device(device);}
 }

 #[test]
 fn pnp_resource_lists_are_validated_and_conflicts_are_reserved() {
  fn memory_list(start:u64,length:u32)->[u8;36] {
   let mut list=[0u8;36];
   list[0..4].copy_from_slice(&1u32.to_le_bytes()); // one CM_FULL_RESOURCE_DESCRIPTOR
   list[12..14].copy_from_slice(&1u16.to_le_bytes()); // version
   list[14..16].copy_from_slice(&1u16.to_le_bytes()); // revision
   list[16..20].copy_from_slice(&1u32.to_le_bytes()); // one partial descriptor
   list[20]=CM_RESOURCE_MEMORY;
   list[24..32].copy_from_slice(&start.to_le_bytes());
   list[32..36].copy_from_slice(&length.to_le_bytes());
   list
  }
  let mut list=memory_list(0x100000,0x1000);
  let mut driver1:DriverObject=unsafe{core::mem::zeroed()};
  let mut driver2:DriverObject=unsafe{core::mem::zeroed()};
  let mut dev1: *mut DeviceObject=ptr::null_mut();
  let mut dev2: *mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver1,0,ptr::null_mut(),7,0,0,&mut dev1)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut driver2,0,ptr::null_mut(),7,0,0,&mut dev2)},STATUS_SUCCESS);
  let irp1=unsafe{io_allocate_irp(2,0)};
  let irp2=unsafe{io_allocate_irp(2,0)};
  assert!(!irp1.is_null()&&!irp2.is_null());
  let list_ptr=list.as_ptr().cast::<c_void>();
  assert_eq!(unsafe{io_prepare_pnp_resources(dev1,irp1,IRP_MN_START_DEVICE,list_ptr,list_ptr)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_prepare_pnp_resources(dev2,irp2,IRP_MN_START_DEVICE,list_ptr,list_ptr)},STATUS_DEVICE_BUSY);
  io_finish_pnp_resources(irp1,dev1,IRP_MN_START_DEVICE,STATUS_SUCCESS);
  assert_eq!(unsafe{io_prepare_pnp_resources(dev2,irp2,IRP_MN_START_DEVICE,list_ptr,list_ptr)},STATUS_DEVICE_BUSY);
  io_release_pnp_resources(dev1);
  assert_eq!(unsafe{io_prepare_pnp_resources(dev2,irp2,IRP_MN_START_DEVICE,list_ptr,list_ptr)},STATUS_SUCCESS);
  io_finish_pnp_resources(irp2,dev2,IRP_MN_START_DEVICE,STATUS_INVALID_DEVICE_REQUEST);
  unsafe{io_free_irp(irp1);io_free_irp(irp2);io_delete_device(dev1);io_delete_device(dev2);}
  // Overflowing ranges are rejected before they can enter the arbiter.
  list=memory_list(u64::MAX-3,8);
  let mut parsed=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
  assert_eq!(unsafe{parse_cm_resource_list(list.as_ptr().cast(),&mut parsed)},Err(STATUS_INVALID_PARAMETER));
 }

 #[test]
 fn pnp_manager_sends_real_start_stop_and_ordered_remove_irps() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0x1b]=Some(pnp_start_complete_success);
  assert_eq!(unsafe{IoPnPStartDevice(device,ptr::null(),ptr::null())},STATUS_SUCCESS);
  assert_eq!(io_pnp_state(device),PNP_STATE_STARTED as i32);
  assert_eq!(unsafe{IoPnPStopDevice(device)},STATUS_SUCCESS);
  assert_eq!(io_pnp_state(device),PNP_STATE_STOPPED as i32);
  assert_eq!(unsafe{IoPnPRemoveDevice(device)},STATUS_SUCCESS);
  assert_eq!(io_pnp_state(device),PNP_STATE_REMOVED as i32);
  unsafe{io_delete_device(device);}
 }

}

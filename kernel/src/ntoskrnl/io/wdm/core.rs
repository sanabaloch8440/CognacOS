//! Partial Windows x64 WDM object/IRP ABI foundation.
//!
//! Layouts and dispatch conventions here follow the public x64 WDK ABI for the
//! fields used by this implementation. This is not a claim of complete WDM
//! compatibility: dynamic IRP records/system buffers and the WDM cancel-safe
//! queue API are implemented, but cancel-spin-lock parity, all driver-created IRP
//! ownership variants, security descriptors, and many IRP variants remain
//! incomplete. VPB storage and mount arbitration are implemented below. Driver images must not be treated as generally loadable.
use core::{ffi::c_void, ptr, ops::{Index, IndexMut}};
use crate::hal::IrqSpinLock;

static VPB_SPIN_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// WDK global VPB lock. Callers must keep the critical section short; this
/// raises to DISPATCH_LEVEL and restores the caller's prior IRQL on release.
#[unsafe(export_name="IoAcquireVpbSpinLock")]
pub unsafe extern "win64" fn io_acquire_vpb_spin_lock(old_irql:*mut u8) {
 if old_irql.is_null(){return}
 #[cfg(test)] { unsafe{old_irql.write(0);} return; }
 #[cfg(not(test))] {
  let old=unsafe{crate::ntoskrnl::ke::runtime::ke_raise_irql_to_dpc_level()};
  while VPB_SPIN_LOCK.compare_exchange_weak(false,true,core::sync::atomic::Ordering::Acquire,core::sync::atomic::Ordering::Relaxed).is_err(){core::hint::spin_loop();}
  unsafe{old_irql.write(old);}
 }
}

#[unsafe(export_name="IoReleaseVpbSpinLock")]
pub unsafe extern "win64" fn io_release_vpb_spin_lock(old_irql:u8) {
 #[cfg(test)] { let _=old_irql; return; }
 #[cfg(not(test))] {
  VPB_SPIN_LOCK.store(false,core::sync::atomic::Ordering::Release);
  unsafe{crate::ntoskrnl::ke::runtime::ke_lower_irql(old_irql);}
 }
}

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
pub type Vpb = crate::nt_abi::io::VPB;
pub type FileObject = crate::nt_abi::io::FILE_OBJECT;
const IRP_MJ_FILE_SYSTEM_CONTROL: u8 = 0x0d;
const IRP_MN_MOUNT_VOLUME: u8 = 0x01;
const IRP_MN_VERIFY_VOLUME: u8 = 0x02;
const FILE_DEVICE_DISK: u32 = 0x0000_0007;
const FILE_DEVICE_CD_ROM: u32 = 0x0000_0002;
const FILE_DEVICE_TAPE: u32 = 0x0000_001f;
const FILE_DEVICE_VIRTUAL_DISK: u32 = 0x0000_0024;
pub type DriverDispatch = unsafe extern "win64" fn(*mut DeviceObject, *mut Irp) -> i32;
pub type DriverUnload = unsafe extern "win64" fn(*mut DriverObject);
pub type IoCompletionRoutine = unsafe extern "win64" fn(*mut DeviceObject, *mut Irp, *mut c_void) -> i32;
pub type DriverCancelRoutine = unsafe extern "win64" fn(*mut DeviceObject, *mut Irp);

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
/// x64 WDK KDEVICE_QUEUE_ENTRY: LIST_ENTRY, sort key, and insertion state.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KDeviceQueueEntry {
    pub flink: *mut KDeviceQueueEntry,
    pub blink: *mut KDeviceQueueEntry,
    pub sort_key: u32,
    pub inserted: u8,
    pub padding: [u8; 3],
}

/// x64 WDK KDEVICE_QUEUE. DEVICE_OBJECT embeds this at offset 0xA0.
#[repr(C)]
pub struct KDeviceQueue {
    pub type_: i16,
    pub size: i16,
    pub device_list_head: [u8; 0x10],
    pub lock: usize,
    pub busy: u8,
    pub padding: [u8; 7],
}

#[repr(C, align(16))]
pub struct DeviceObject {
    pub object_type: i16, pub size: u16, pub reference_count: i32,
    pub driver_object: *mut DriverObject, pub next_device: *mut DeviceObject,
    pub attached_device: *mut DeviceObject, pub current_irp: *mut Irp,
    pub timer: *mut c_void, pub flags: u32, pub characteristics: u32,
    pub vpb: *mut c_void, pub device_extension: *mut c_void,
    pub device_type: u32, pub stack_size: i8, pub queue_padding: [u8; 3],
    pub queue: [u8; 0x48],
    pub alignment_requirement: u32, pub device_queue_padding: [u8; 4],
    pub device_queue: KDeviceQueue, pub dpc: [u8; 0x40], pub active_thread_count: u32,
    pub security_descriptor: *mut c_void, pub device_lock: [u8; 0x18],
    pub sector_size: u16, pub spare1: u16, pub device_object_extension: *mut c_void,
    pub reserved: *mut c_void,
}
const _: () = assert!(core::mem::size_of::<DeviceObject>() == 0x150);
const _: () = assert!(core::mem::offset_of!(DeviceObject, device_queue) == 0xA0);
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
const _: () = assert!(core::mem::size_of::<IoStackLocation>() == IO_STACK_LOCATION_SIZE_X64);
#[repr(C)]
struct DeviceSlot { used: bool, deleting: bool, active_dispatches:u32, outstanding_irps:u32, remove_lock_count: u32, power_state: u8, power_target: u8, power_transition: bool, pnp_state: u8, pnp_pending: u8, pnp_transition_minor: u8, pnp_transition: bool, mount_in_progress: bool, attached_to: *mut DeviceObject, name_len: usize, name: [u16; 260], vpb: Vpb, object: DeviceObject, extension: [u8; MAX_DEVICE_EXTENSION] }
unsafe impl Send for DeviceSlot {}
const PAGE_SIZE:usize=4096;
#[repr(C)]
struct IrpSlot {
 next:*mut IrpSlot,
 used: bool, owned_mdl: *mut c_void, completion_state:u8, suspended_completion:bool, free_requested:bool, active_ops:u32,
 outstanding_count:usize, outstanding_devices:[*mut DeviceObject;MAX_WDM_STACK_LOCATIONS],
 object: Irp, stack: [IoStackLocation;MAX_WDM_STACK_LOCATIONS],
 system_buffer:*mut u8, system_buffer_pages:usize, system_buffer_len:usize,
 ioctl_output_buffer:*mut c_void, ioctl_output_len:usize, built_sync_ioctl:bool,
 sync_thread_id:u64, requestor_pid:u64, sync_event:*mut c_void, sync_next:*mut Irp, sync_prev:*mut Irp, sync_queued:bool,
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
  sync_thread_id:0,requestor_pid:0,sync_event:ptr::null_mut(),sync_next:ptr::null_mut(),sync_prev:ptr::null_mut(),sync_queued:false,
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
unsafe impl Send for ThreadIrpQueue {}
impl ThreadIrpQueue { const EMPTY:Self=Self{used:false,thread_id:0,head:ptr::null_mut(),tail:ptr::null_mut()}; }
struct WdmState { devices: [DeviceSlot; MAX_WDM_DEVICES], irps: IrpTable, thread_irps:[ThreadIrpQueue;MAX_SYNC_IRP_THREADS] }
impl WdmState {
 const fn new() -> Self {
  Self {
   devices: [const { DeviceSlot { used:false, deleting:false, active_dispatches:0, outstanding_irps:0, remove_lock_count:0, power_state:0, power_target:0, power_transition:false, pnp_state:0, pnp_pending:0, pnp_transition_minor:0, pnp_transition:false, mount_in_progress:false, attached_to:ptr::null_mut(), name_len:0, name:[0;260], vpb:Vpb::empty(), object: DeviceObject {
    object_type:3,size:core::mem::size_of::<DeviceObject>() as u16,reference_count:1,
    driver_object:ptr::null_mut(),next_device:ptr::null_mut(),attached_device:ptr::null_mut(),
    current_irp:ptr::null_mut(),timer:ptr::null_mut(),flags:0,characteristics:0,vpb:ptr::null_mut(),
    device_extension:ptr::null_mut(),device_type:0,stack_size:1,queue_padding:[0;3],queue:[0;0x48],alignment_requirement:0,device_queue_padding:[0;4],
    device_queue:KDeviceQueue{type_:0,size:core::mem::size_of::<KDeviceQueue>() as i16,device_list_head:[0;0x10],lock:0,busy:0,padding:[0;7]},dpc:[0;0x40],active_thread_count:0,
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

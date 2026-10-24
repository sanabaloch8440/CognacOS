//! Initial Windows-NT-inspired I/O manager contracts: device dispatch, bounded
//! IRPs, single-dispatch ownership, cancellation before dispatch, and completion.
//! This is not yet a full NT IRP stack or hardware DMA implementation.
use crate::hal::IrqSpinLock;
pub const MAX_DEVICES:usize=16;pub const MAX_IRPS:usize=64;pub const MAX_IRP_BYTES:usize=4096; 
pub const MAX_COMPLETION_PORTS:usize=16; 
pub const MAX_COMPLETION_PACKETS:usize=64;
pub const IRP_MJ_CREATE:u8=0;pub const IRP_MJ_CLOSE:u8=2;pub const IRP_MJ_READ:u8=3;pub const IRP_MJ_WRITE:u8=4;pub const IRP_MJ_DEVICE_CONTROL:u8=14;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IoStatus{Free,Pending,Success,InvalidRequest,NoSuchDevice,BufferTooLarge,DeviceError,Cancelled,TimedOut}
#[derive(Clone,Copy,Debug)]
pub struct Irp{pub id:u64,pub major:u8,pub device_id:u32,pub user_buffer:u64,pub length:usize,pub information:usize,pub status:IoStatus,pub completed:bool,pub in_flight:bool,pub deadline_tick:u64}
impl Irp{const EMPTY:Self=Self{id:0,major:0,device_id:0,user_buffer:0,length:0,information:0,status:IoStatus::Free,completed:false,in_flight:false,deadline_tick:0};}
pub type DispatchFn=fn(&mut Irp)->IoStatus;
#[derive(Clone,Copy)]
pub struct Device{pub id:u32,pub name:[u8;32],pub name_len:u8,pub dispatch:[Option<DispatchFn>;16],pub used:bool}
impl Device{const EMPTY:Self=Self{id:0,name:[0;32],name_len:0,dispatch:[None;16],used:false};}
struct State{devices:[Device;MAX_DEVICES],irps:[Irp;MAX_IRPS],next_device:u32,next_irp:u64}
impl State{const fn new()->Self{Self{devices:[Device::EMPTY;MAX_DEVICES],irps:[Irp::EMPTY;MAX_IRPS],next_device:1,next_irp:1}}}
static STATE:IrqSpinLock<State>=IrqSpinLock::new(State::new());
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IoError{InvalidName,NameTooLong,TooManyDevices,TooManyRequests,TooManyCompletionPorts,CompletionQueueFull,UnknownCompletionPort,UnknownDevice,InvalidMajor,BufferTooLarge,UnknownIrp,AlreadyCompleted,AlreadyInFlight,NotCancellable}

pub fn register_device(name:&[u8],dispatch:[Option<DispatchFn>;16])->Result<u32,IoError>{
 if name.is_empty(){return Err(IoError::InvalidName);}if name.len()>32{return Err(IoError::NameTooLong);}
 let mut s=STATE.lock();
 if s.devices.iter().any(|d|d.used&&d.name_len as usize==name.len()&&d.name[..name.len()]==*name){return Err(IoError::InvalidName);}
 let index=s.devices.iter().position(|d|!d.used).ok_or(IoError::TooManyDevices)?;
 let id=s.next_device;s.next_device=s.next_device.wrapping_add(1).max(1);
 let mut device=Device::EMPTY;device.id=id;device.name[..name.len()].copy_from_slice(name);device.name_len=name.len() as u8;device.dispatch=dispatch;device.used=true;s.devices[index]=device;Ok(id)
}
pub fn submit(device_id:u32,major:u8,user_buffer:u64,length:usize)->Result<u64,IoError>{
 submit_with_timeout(device_id,major,user_buffer,length,None)
}
/// Submit an IRP with an optional relative deadline in monotonic clock ticks.
/// Expiry is enforced before dispatch and when an asynchronous completion arrives.
pub fn submit_with_timeout(device_id:u32,major:u8,user_buffer:u64,length:usize,timeout_ticks:Option<u64>)->Result<u64,IoError>{
 if major as usize>=16{return Err(IoError::InvalidMajor);}if length>MAX_IRP_BYTES{return Err(IoError::BufferTooLarge);}
 let now=crate::hal::query_performance_counter();
 let deadline=match timeout_ticks{Some(0)=>now,Some(t)=>now.checked_add(t).ok_or(IoError::BufferTooLarge)?,None=>0};
 let mut s=STATE.lock();
 if !s.devices.iter().any(|d|d.used&&d.id==device_id){return Err(IoError::UnknownDevice);}
 let index=s.irps.iter().position(|i|i.status==IoStatus::Free).ok_or(IoError::TooManyRequests)?;
 let id=s.next_irp;s.next_irp=s.next_irp.wrapping_add(1).max(1);
 s.irps[index]=Irp{id,major,device_id,user_buffer,length,information:0,status:if timeout_ticks==Some(0){IoStatus::TimedOut}else{IoStatus::Pending},completed:timeout_ticks==Some(0),in_flight:false,deadline_tick:deadline};Ok(id)
}
/// Expire requests that have not yet been claimed by a driver. In-flight
/// hardware operations require a driver-specific cancellation protocol.
pub fn expire_timeouts()->usize{
 let now=crate::hal::query_performance_counter();let mut count=0;
 let mut s=STATE.lock();
 for irp in s.irps.iter_mut(){
  if irp.status==IoStatus::Pending&&!irp.completed&&!irp.in_flight&&irp.deadline_tick!=0&&now>=irp.deadline_tick{
   irp.status=IoStatus::TimedOut;irp.completed=true;irp.information=0;count+=1;
  }
 }
 count
}
/// Cancel only before dispatch begins. Once a callback owns the request, callers
/// must use a future driver-specific cancellation protocol rather than race it.
pub fn cancel_irp(irp_id:u64)->Result<(),IoError>{
 let mut s=STATE.lock();
 let irp=s.irps.iter_mut().find(|i|i.id==irp_id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
 if irp.completed{return Err(IoError::AlreadyCompleted);}
 if irp.in_flight{return Err(IoError::NotCancellable);}
 irp.status=IoStatus::Cancelled;irp.completed=true;irp.in_flight=false;Ok(())
}
/// Claim an IRP under lock, invoke its callback outside the lock, then publish
/// completion. in_flight prevents a second CPU from dispatching it concurrently.
pub fn dispatch_one(irp_id:u64)->Result<IoStatus,IoError>{
 let (mut irp,callback)={
  let mut s=STATE.lock();
  let index=s.irps.iter().position(|i|i.id==irp_id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
  if s.irps[index].completed{return Err(IoError::AlreadyCompleted);}
  if s.irps[index].in_flight{return Err(IoError::AlreadyInFlight);}
  if s.irps[index].deadline_tick!=0&&crate::hal::query_performance_counter()>=s.irps[index].deadline_tick{
   s.irps[index].status=IoStatus::TimedOut;s.irps[index].completed=true;s.irps[index].information=0;return Ok(IoStatus::TimedOut);
  }
  let d=s.devices.iter().find(|d|d.used&&d.id==s.irps[index].device_id).ok_or(IoError::UnknownDevice)?;
  let cb=d.dispatch[s.irps[index].major as usize];
  s.irps[index].in_flight=true;
  (s.irps[index],cb)
 };
 let status=if let Some(cb)=callback{cb(&mut irp)}else{IoStatus::InvalidRequest};
 irp.status=status;irp.completed=status!=IoStatus::Pending;irp.in_flight=status==IoStatus::Pending;
 let mut s=STATE.lock();
 let index=s.irps.iter().position(|i|i.id==irp_id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
 s.irps[index]=irp;Ok(status)
}
/// Complete an asynchronously dispatched IRP. Only the callback-owned
/// in-flight request may be completed; terminal states cannot be overwritten.
pub fn complete_irp(id:u64,status:IoStatus,information:usize)->Result<(),IoError>{
 if status==IoStatus::Free||status==IoStatus::Pending{return Err(IoError::AlreadyCompleted);}
 let mut s=STATE.lock();
 let irp=s.irps.iter_mut().find(|i|i.id==id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
 if irp.completed{return Err(IoError::AlreadyCompleted);}
 if !irp.in_flight{return Err(IoError::NotCancellable);}
 if irp.deadline_tick!=0&&crate::hal::query_performance_counter()>=irp.deadline_tick{
  irp.information=0;irp.status=IoStatus::TimedOut;irp.completed=true;irp.in_flight=false;return Ok(())
 }
 if information>irp.length{return Err(IoError::BufferTooLarge);}
 irp.information=information;irp.status=status;irp.completed=true;irp.in_flight=false;Ok(())
}

/// I/O completion packet. Values are copied, so no caller-owned pointer is
/// dereferenced while the completion queue lock is held.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct CompletionPacket{pub key:u64,pub apc_context:u64,pub irp_id:u64,pub status:IoStatus,pub information:usize,pub completed_at:u64}
impl CompletionPacket{const EMPTY:Self=Self{key:0,apc_context:0,irp_id:0,status:IoStatus::Free,information:0,completed_at:0};}
#[derive(Clone,Copy)]
struct CompletionPort{ id:u32,used:bool,head:usize,count:usize,packets:[CompletionPacket;MAX_COMPLETION_PACKETS] }
impl CompletionPort{const EMPTY:Self=Self{id:0,used:false,head:0,count:0,packets:[CompletionPacket::EMPTY;MAX_COMPLETION_PACKETS]};}
struct CompletionState{ports:[CompletionPort;MAX_COMPLETION_PORTS],next_id:u32}
impl CompletionState{const fn new()->Self{Self{ports:[CompletionPort::EMPTY;MAX_COMPLETION_PORTS],next_id:1}}}
static COMPLETIONS:IrqSpinLock<CompletionState>=IrqSpinLock::new(CompletionState::new());
pub fn create_completion_port()->Result<u32,IoError>{
 let mut s=COMPLETIONS.lock();let slot=s.ports.iter().position(|p|!p.used).ok_or(IoError::TooManyCompletionPorts)?;
 let id=s.next_id;s.next_id=s.next_id.wrapping_add(1).max(1);s.ports[slot]=CompletionPort{ id,used:true,..CompletionPort::EMPTY };Ok(id)
}
pub fn post_completion_packet(port_id:u32,packet:CompletionPacket)->Result<(),IoError>{
 let mut s=COMPLETIONS.lock();let port=s.ports.iter_mut().find(|p|p.used&&p.id==port_id).ok_or(IoError::UnknownCompletionPort)?;
 if port.count==MAX_COMPLETION_PACKETS{return Err(IoError::CompletionQueueFull);}
 let tail=(port.head+port.count)%MAX_COMPLETION_PACKETS;port.packets[tail]=packet;port.count+=1;Ok(())
}
pub fn try_remove_completion_packet(port_id:u32)->Result<Option<CompletionPacket>,IoError>{
 let mut s=COMPLETIONS.lock();let port=s.ports.iter_mut().find(|p|p.used&&p.id==port_id).ok_or(IoError::UnknownCompletionPort)?;
 if port.count==0{return Ok(None)}
 let packet=port.packets[port.head];port.packets[port.head]=CompletionPacket::EMPTY;port.head=(port.head+1)%MAX_COMPLETION_PACKETS;port.count-=1;Ok(Some(packet))
}
/// Nonblocking timed dequeue: returns no packet when the queue is empty and
/// the optional absolute monotonic deadline has elapsed. Callers integrate
/// retries with dispatcher waits instead of busy-spinning in kernel context.
pub fn remove_completion_packet_until(port_id:u32,deadline_tick:Option<u64>)->Result<Option<CompletionPacket>,IoError>{
 if let Some(packet)=try_remove_completion_packet(port_id)?{return Ok(Some(packet))}
 if deadline_tick.is_some_and(|deadline|crate::hal::query_performance_counter()>=deadline){return Ok(None)}
 Ok(None)
}

/// Complete an IRP and publish one packet to a completion port. If posting
/// fails because the bounded queue is full, the IRP remains completed and the
/// caller receives the queue error so it can recover without losing status.
pub fn complete_irp_to_port(id:u64,status:IoStatus,information:usize,port_id:u32,key:u64,apc_context:u64)->Result<(),IoError>{
 complete_irp(id,status,information)?;
 let irp=query_irp(id).ok_or(IoError::UnknownIrp)?;
 post_completion_packet(port_id,CompletionPacket{key,apc_context,irp_id:id,status:irp.status,information:irp.information,completed_at:crate::hal::query_performance_counter()})
}

pub fn query_irp(id:u64)->Option<Irp>{
 let s=STATE.lock();s.irps.iter().find(|i|i.id==id&&i.status!=IoStatus::Free).copied()
}
pub fn release_irp(id:u64)->Result<(),IoError>{
 let mut s=STATE.lock();
 let index=s.irps.iter().position(|i|i.id==id&&i.status!=IoStatus::Free).ok_or(IoError::UnknownIrp)?;
 if !s.irps[index].completed||s.irps[index].in_flight{return Err(IoError::AlreadyCompleted);}
 s.irps[index]=Irp::EMPTY;Ok(())
}


#[cfg(test)]
mod completion_tests {
    use super::*;
    #[test]
    fn completion_port_is_fifo_and_bounded() {
        let port=create_completion_port().expect("completion port");
        let first=CompletionPacket{key:11,apc_context:12,irp_id:13,status:IoStatus::Success,information:14,completed_at:15};
        let second=CompletionPacket{key:21,apc_context:22,irp_id:23,status:IoStatus::Cancelled,information:0,completed_at:25};
        post_completion_packet(port,first).expect("post first");
        post_completion_packet(port,second).expect("post second");
        assert_eq!(try_remove_completion_packet(port).unwrap(),Some(first));
        assert_eq!(try_remove_completion_packet(port).unwrap(),Some(second));
        for n in 0..MAX_COMPLETION_PACKETS {
            post_completion_packet(port,CompletionPacket{key:n as u64,apc_context:0,irp_id:n as u64,status:IoStatus::Success,information:0,completed_at:0}).expect("fill queue");
        }
        assert_eq!(post_completion_packet(port,first),Err(IoError::CompletionQueueFull));
        for n in 0..MAX_COMPLETION_PACKETS { assert_eq!(try_remove_completion_packet(port).unwrap().unwrap().key,n as u64); }
        assert_eq!(try_remove_completion_packet(port).unwrap(),None);
    }
    #[test]
    fn zero_deadline_is_terminal_before_dispatch() {
        let mut dispatch:[Option<DispatchFn>;16]=[None;16];
        dispatch[IRP_MJ_READ as usize]=Some(|_|IoStatus::Success);
        let device=register_device(b"timeout-zero-test",dispatch).expect("register test device");
        let id=submit_with_timeout(device,IRP_MJ_READ,0,16,Some(0)).expect("submit timed out IRP");
        let irp=query_irp(id).expect("query timed out IRP");
        assert_eq!(irp.status,IoStatus::TimedOut);
        assert!(irp.completed);
        assert_eq!(dispatch_one(id),Err(IoError::AlreadyCompleted));
        release_irp(id).expect("release terminal IRP");
    }
}


/// Windows WDM version gate. RdotOS currently implements the WDM 1.0
/// baseline only; newer requested minor versions are rejected rather than
/// claiming capabilities that are not present.
#[unsafe(no_mangle)]
pub extern "system" fn io_is_wdm_version_available(major_version: u16, minor_version: u16) -> u8 {
    u8::from(major_version == 0 || (major_version == 1 && minor_version == 0))
}

/// Public NT configuration counters returned by IoGetConfigurationInformation.
/// The device enumerator is not yet wired to these counters, so they remain zero
/// until the relevant bus/device classes are registered.
#[repr(C)]
pub struct ConfigurationInformation {
    pub disk_count: u32,
    pub floppy_count: u32,
    pub cd_rom_count: u32,
    pub tape_count: u32,
    pub scsi_port_count: u32,
    pub serial_count: u32,
    pub parallel_count: u32,
}
static mut CONFIGURATION_INFORMATION: ConfigurationInformation = ConfigurationInformation {
    disk_count: 0, floppy_count: 0, cd_rom_count: 0, tape_count: 0,
    scsi_port_count: 0, serial_count: 0, parallel_count: 0,
};

#[unsafe(no_mangle)]
pub extern "system" fn io_get_configuration_information() -> *mut ConfigurationInformation {
    core::ptr::addr_of_mut!(CONFIGURATION_INFORMATION)
}


#[cfg(test)]
mod io_export_tests {
    use super::*;

    #[test]
    fn wdm_version_gate_only_advertises_implemented_baseline() {
        assert_eq!(io_is_wdm_version_available(1, 0), 1);
        assert_eq!(io_is_wdm_version_available(1, 1), 0);
        assert_eq!(io_is_wdm_version_available(2, 0), 0);
    }

    #[test]
    fn configuration_information_has_stable_public_layout() {
        assert_eq!(core::mem::size_of::<ConfigurationInformation>(), 28);
        let ptr = io_get_configuration_information();
        assert!(!ptr.is_null());
    }
}


static IO_CANCEL_SPIN_LOCK: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Acquire the global NT I/O cancel spin lock and raise to DISPATCH_LEVEL.
/// The caller must supply a writable KIRQL slot, as required by the WDK ABI.
#[unsafe(no_mangle)]
pub extern "system" fn io_acquire_cancel_spin_lock(old_irql: *mut u8) {
    if old_irql.is_null() {
        return;
    }
    let previous = unsafe { crate::ntoskrnl::ke::runtime::ke_raise_irql_to_dpc_level() };
    while IO_CANCEL_SPIN_LOCK
        .compare_exchange_weak(
            false,
            true,
            core::sync::atomic::Ordering::Acquire,
            core::sync::atomic::Ordering::Relaxed,
        )
        .is_err()
    {
        core::hint::spin_loop();
    }
    unsafe { old_irql.write(previous); }
}

/// Release the global cancel spin lock, then restore the caller's previous IRQL.
#[unsafe(no_mangle)]
pub extern "system" fn io_release_cancel_spin_lock(old_irql: u8) {
    IO_CANCEL_SPIN_LOCK.store(false, core::sync::atomic::Ordering::Release);
    unsafe { crate::ntoskrnl::ke::runtime::ke_lower_irql(old_irql); }
}

#[path = "wdm.rs"] mod wdm;
pub use wdm::{
    io_allocate_irp, io_call_driver, io_complete_request, io_create_device,
    io_delete_device, io_detach_device, io_attach_device_to_device_stack,
    io_get_attached_device, io_get_device_attachment_base_ref, io_get_lower_device_object,
    io_initialize_irp, io_copy_current_irp_stack_location_to_next,
    io_skip_current_irp_stack_location, io_reuse_irp, lookup_device_object, io_register_file_system,
    io_unregister_file_system, io_create_symbolic_link, io_delete_symbolic_link,
    io_reference_device_object, io_dereference_device_object, io_acquire_device_remove_lock,
    io_release_device_remove_lock, io_begin_device_power_transition, io_complete_device_power_transition,
    device_power_state,
    io_set_cancel_routine, io_cancel_irp, io_build_asynchronous_fsd_request,
    io_allocate_mdl, io_free_mdl, DriverCancelRoutine,
    io_free_irp, io_get_current_irp_stack_location,
    io_get_next_irp_stack_location, io_mark_irp_pending, io_set_completion_routine,
    DeviceObject, DriverObject, IoCompletionRoutine, IoStackLocation, IoStatusBlock,
    UnicodeString,
};
pub(crate) use wdm::io_set_device_power_state;
pub use wdm::Irp as WdmIrp;

//! NT power-manager runtime and ABI entry points.
//!
//! Power-specific ABI and system power state live here. Device D-state metadata
//! remains owned by the I/O manager's device-object lifetime records.
use core::sync::atomic::{AtomicU32, Ordering};
use crate::io::{self, WdmIrp};
use crate::io::DeviceObject;

#[repr(C)]
#[derive(Clone, Copy)]
pub union PowerState {
    pub system_state: u32,
    pub device_state: u32,
}

static SYSTEM_POWER_STATE: AtomicU32 = AtomicU32::new(1);

/// Report a committed system/device power state and return the previous state.
/// This bookkeeping entry point does not perform a hardware transition or
/// replace IRP_MJ_POWER dispatch, PnP sequencing, or asynchronous completion.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoSetPowerState(
    device: *mut DeviceObject,
    state_type: u32,
    new_state: PowerState,
) -> PowerState {
    match state_type {
        0 => {
            let next = unsafe { new_state.system_state };
            // SYSTEM_POWER_STATE values S0..S5 plus the internal shutdown slot.
            if next > 6 {
                return PowerState { system_state: 0 };
            }
            let old = SYSTEM_POWER_STATE.swap(next, Ordering::AcqRel);
            PowerState { system_state: old }
        }
        1 => {
            if device.is_null() {
                return PowerState { device_state: 0 };
            }
            // WDM DEVICE_POWER_STATE values are Unspecified=0, D0=1..D3=4.
            let raw = unsafe { new_state.device_state };
            if !(1..=4).contains(&raw) {
                return PowerState { device_state: 0 };
            }
            match io::io_set_device_power_state(device, (raw - 1) as u8) {
                Some(previous) => PowerState { device_state: previous as u32 + 1 },
                None => PowerState { device_state: 0 },
            }
        }
        _ => PowerState { device_state: 0 },
    }
}


const IRP_MJ_POWER:u8=0x16;
const IRP_MJ_PNP:u8=0x1b;
const IRP_MN_SET_POWER:u8=0x02;
const IRP_MN_QUERY_POWER:u8=0x03;
const STATUS_SUCCESS:i32=0;
const STATUS_PENDING:i32=0x0000_0103;
const STATUS_INVALID_DEVICE_REQUEST:i32=0xC000_0010u32 as i32;
const STATUS_DELETE_PENDING:i32=0xC000_0056u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES:i32=0xC000_009Au32 as i32;
const MAX_POWER_TRANSITIONS:usize=64;

#[derive(Clone,Copy)]
struct Transition { used:bool, irp:*mut WdmIrp, device:*mut DeviceObject, kind:u8, target:u8, minor:u8 }
unsafe impl Send for Transition {}
impl Transition { const EMPTY:Self=Self{used:false,irp:core::ptr::null_mut(),device:core::ptr::null_mut(),kind:0,target:0,minor:0}; }
struct TransitionState { slots:[Transition;MAX_POWER_TRANSITIONS] }
impl TransitionState { const fn new()->Self{Self{slots:[Transition::EMPTY;MAX_POWER_TRANSITIONS]}} }
static TRANSITIONS:crate::hal::IrqSpinLock<TransitionState>=crate::hal::IrqSpinLock::new(TransitionState::new());
static SYSTEM_POWER_TRANSITION:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);

fn add_transition(t:Transition)->Result<(),i32>{
 let mut s=TRANSITIONS.lock();
 if s.slots.iter().any(|v|v.used&&v.irp==t.irp&&v.device==t.device){return Err(STATUS_INVALID_DEVICE_REQUEST)}
 let Some(slot)=s.slots.iter_mut().find(|v|!v.used) else{return Err(STATUS_INSUFFICIENT_RESOURCES)};
 *slot=t;Ok(())
}

/// Called by IoCallDriver before power/PnP dispatch. Device references and
/// transition reservations survive STATUS_PENDING until IoCompleteRequest.
pub(crate) unsafe fn po_prepare_irp_dispatch(device:*mut DeviceObject,irp:*mut WdmIrp,major:u8,minor:u8,parameters:[u64;4])->i32{
 if major!=IRP_MJ_POWER&&major!=IRP_MJ_PNP{return STATUS_SUCCESS}
 let (kind,target)=if major==IRP_MJ_POWER {
  if minor==IRP_MN_QUERY_POWER{return STATUS_SUCCESS}
  if minor!=IRP_MN_SET_POWER{return STATUS_SUCCESS}
  let state_type=parameters[0] as u32;
  let raw=(parameters[0]>>32) as u32;
  match state_type {
   0 if (1..=6).contains(&raw)=>{
    if SYSTEM_POWER_TRANSITION.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return STATUS_PENDING}
    (3,raw as u8)
   }
   1 if (1..=4).contains(&raw)=>{
    let refd=unsafe{io::io_reference_device_object(device)};
    if refd.is_null(){return STATUS_DELETE_PENDING}
    let status=unsafe{io::io_begin_device_power_transition(device,(raw-1) as u8)};
    if status!=STATUS_SUCCESS{unsafe{io::io_dereference_device_object(device)};return status}
    (1,(raw-1) as u8)
   }
   _=>return STATUS_INVALID_DEVICE_REQUEST,
  }
 } else {
  if !matches!(minor,0x00|0x01|0x02|0x03|0x04|0x05|0x06|0x17){return STATUS_SUCCESS}
  let refd=unsafe{io::io_reference_device_object(device)};
  if refd.is_null(){return STATUS_DELETE_PENDING}
  let status=unsafe{io::io_begin_pnp_transition(device,minor)};
  if status!=STATUS_SUCCESS{unsafe{io::io_dereference_device_object(device)};return status}
  (2,0)
 };
 if let Err(status)=add_transition(Transition{used:true,irp,device,kind,target,minor}) {
  match kind {
   1=>unsafe{let _=io::io_complete_device_power_transition(device,target,STATUS_INVALID_DEVICE_REQUEST);io::io_dereference_device_object(device)},
   2=>unsafe{let _=io::io_complete_pnp_transition(device,minor,STATUS_INVALID_DEVICE_REQUEST);io::io_dereference_device_object(device)},
   3=>SYSTEM_POWER_TRANSITION.store(false,Ordering::Release),
   _=>{}
  }
  return status
 }
 STATUS_SUCCESS
}
pub(crate) fn po_finish_dispatch_return(irp:*mut WdmIrp,device:*mut DeviceObject,status:i32){
 if status==STATUS_PENDING{return}
 finish_matching(irp,Some(device),status);
}
fn finish_matching(irp:*mut WdmIrp,device:Option<*mut DeviceObject>,status:i32){
 let mut ready=[Transition::EMPTY;MAX_POWER_TRANSITIONS];let mut n=0usize;
 {
  let mut s=TRANSITIONS.lock();
  for slot in s.slots.iter_mut(){
   if slot.used&&slot.irp==irp&&device.is_none_or(|d|slot.device==d){ready[n]=*slot;n+=1;*slot=Transition::EMPTY;}
  }
 }
 for t in ready.iter().take(n){
  match t.kind{
   1=>unsafe{let _=io::io_complete_device_power_transition(t.device,t.target,status);io::io_dereference_device_object(t.device)},
   2=>unsafe{let _=io::io_complete_pnp_transition(t.device,t.minor,status);io::io_dereference_device_object(t.device)},
   3=>{if status==STATUS_SUCCESS{SYSTEM_POWER_STATE.store(t.target as u32,Ordering::Release)}SYSTEM_POWER_TRANSITION.store(false,Ordering::Release);},
   _=>{}
  }
 }
}
pub(crate) fn po_complete_irp(irp:*mut WdmIrp,status:i32){finish_matching(irp,None,status);finish_power_request(irp,status);}

pub type PowerRequestComplete=unsafe extern "system" fn(*mut DeviceObject,u8,PowerState,*mut core::ffi::c_void,*mut WdmIrp);
const MAX_POWER_REQUESTS:usize=32;
#[derive(Clone,Copy)]
struct PowerRequest{used:bool,irp:*mut WdmIrp,device:*mut DeviceObject,minor:u8,state:u32,context:*mut core::ffi::c_void,callback:Option<PowerRequestComplete>,returned:bool,completed:bool}
unsafe impl Send for PowerRequest{}
impl PowerRequest{const EMPTY:Self=Self{used:false,irp:core::ptr::null_mut(),device:core::ptr::null_mut(),minor:0,state:0,context:core::ptr::null_mut(),callback:None,returned:false,completed:false};}
struct RequestState{slots:[PowerRequest;MAX_POWER_REQUESTS]}
impl RequestState{const fn new()->Self{Self{slots:[PowerRequest::EMPTY;MAX_POWER_REQUESTS]}}}
static POWER_REQUESTS:crate::hal::IrqSpinLock<RequestState>=crate::hal::IrqSpinLock::new(RequestState::new());

/// Request a device power transition through its driver's IRP_MJ_POWER routine.
/// The manager owns the IRP and invokes the optional completion callback once.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoRequestPowerIrp(device:*mut DeviceObject,minor:u8,state:PowerState,callback:Option<PowerRequestComplete>,context:*mut core::ffi::c_void,output_irp:*mut *mut WdmIrp)->i32{
 if device.is_null()||minor!=IRP_MN_SET_POWER{return STATUS_INVALID_DEVICE_REQUEST}
 if !output_irp.is_null(){unsafe{output_irp.write(core::ptr::null_mut())}}
 let stack_size=unsafe{(*device).stack_size.max(1) as i16};
 let irp=unsafe{io::io_allocate_irp(stack_size,0)};
 if irp.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let target=unsafe{state.device_state};
 if !(1..=4).contains(&target){unsafe{io::io_free_irp(irp)};return STATUS_INVALID_DEVICE_REQUEST}
 let stack=unsafe{io::io_get_next_irp_stack_location(irp)};
 if stack.is_null(){unsafe{io::io_free_irp(irp)};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{(*stack).major_function=IRP_MJ_POWER;(*stack).minor_function=minor;(*stack).parameters[0]=1u64|((target as u64)<<32);}
 {
  let mut requests=POWER_REQUESTS.lock();
  let Some(slot)=requests.slots.iter_mut().find(|r|!r.used) else{drop(requests);unsafe{io::io_free_irp(irp)};return STATUS_INSUFFICIENT_RESOURCES};
  *slot=PowerRequest{used:true,irp,device,minor,state:target,context,callback,returned:false,completed:false};
 }
 if !output_irp.is_null(){unsafe{output_irp.write(irp)}}
 let status=unsafe{io::io_call_driver(device,irp)};
 let synth={
  let requests=POWER_REQUESTS.lock();
  requests.slots.iter().find(|r|r.used&&r.irp==irp).is_some_and(|r|!r.completed&&status!=STATUS_PENDING)
 };
 if synth{unsafe{(*irp).bytes[0x30..0x34].copy_from_slice(&status.to_le_bytes());io::io_complete_request(irp,0);}}
 let free={
  let mut requests=POWER_REQUESTS.lock();
  if let Some(r)=requests.slots.iter_mut().find(|r|r.used&&r.irp==irp){r.returned=true;if r.completed{*r=PowerRequest::EMPTY;true}else{false}}else{false}
 };
 if free{unsafe{io::io_free_irp(irp)}}
 status
}
fn finish_power_request(irp:*mut WdmIrp,_status:i32){
 let snapshot={
  let mut requests=POWER_REQUESTS.lock();
  let Some(r)=requests.slots.iter_mut().find(|r|r.used&&r.irp==irp)else{return};
  if r.completed{return}
  r.completed=true;let snap=*r;if r.returned{*r=PowerRequest::EMPTY;}snap
 };
 if let Some(callback)=snapshot.callback{unsafe{callback(snapshot.device,snapshot.minor,PowerState{device_state:snapshot.state},snapshot.context,irp)}}
 if snapshot.returned{unsafe{io::io_free_irp(irp)}}
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoStartNextPowerIrp(_irp:*mut WdmIrp){}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::{io_create_device, io_delete_device, DriverObject};
    use core::ptr;

    #[test]
    fn power_state_reports_previous_device_and_system_states() {
        let mut driver: DriverObject = unsafe { core::mem::zeroed() };
        let mut device: *mut DeviceObject = ptr::null_mut();
        assert_eq!(unsafe { io_create_device(&mut driver, 0, ptr::null_mut(), 7, 0, 0, &mut device) }, 0);
        let old = unsafe { PoSetPowerState(device, 1, PowerState { device_state: 4 }) };
        assert_eq!(unsafe { old.device_state }, 1);
        assert_eq!(crate::io::device_power_state(device), 3);
        let previous = unsafe { PoSetPowerState(ptr::null_mut(), 0, PowerState { system_state: 3 }) };
        assert_eq!(unsafe { previous.system_state }, 1);
        unsafe { io_delete_device(device); }
    }

    #[test]
    fn invalid_power_state_does_not_modify_device() {
        let mut driver: DriverObject = unsafe { core::mem::zeroed() };
        let mut device: *mut DeviceObject = ptr::null_mut();
        assert_eq!(unsafe { io_create_device(&mut driver, 0, ptr::null_mut(), 7, 0, 0, &mut device) }, 0);
        let result = unsafe { PoSetPowerState(device, 1, PowerState { device_state: 5 }) };
        assert_eq!(unsafe { result.device_state }, 0);
        assert_eq!(crate::io::device_power_state(device), 0);
        unsafe { io_delete_device(device); }
    }
}


#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use core::ptr;
    use core::sync::atomic::{AtomicU32, Ordering};
    use crate::io::{io_allocate_irp,io_call_driver,io_complete_request,io_create_device,io_delete_device,io_free_irp,io_get_next_irp_stack_location,io_pnp_state,device_power_state,DriverObject};

    unsafe extern "system" fn success_dispatch(_device:*mut DeviceObject,_irp:*mut WdmIrp)->i32 { 0 }
    unsafe extern "system" fn pending_dispatch(_device:*mut DeviceObject,_irp:*mut WdmIrp)->i32 { STATUS_PENDING }
    static CALLBACK_COUNT:AtomicU32=AtomicU32::new(0);
    unsafe extern "system" fn request_callback(_device:*mut DeviceObject,_minor:u8,_state:PowerState,_context:*mut core::ffi::c_void,_irp:*mut WdmIrp) {
        CALLBACK_COUNT.fetch_add(1,Ordering::SeqCst);
    }

    #[test]
    fn pnp_start_is_committed_only_after_dispatch_success() {
        let mut driver:DriverObject=unsafe{core::mem::zeroed()};
        let mut device:*mut DeviceObject=ptr::null_mut();
        assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},0);
        driver.major_function[IRP_MJ_PNP as usize]=Some(success_dispatch);
        let irp=unsafe{io_allocate_irp(1,0)};
        let stack=unsafe{io_get_next_irp_stack_location(irp)};
        unsafe{(*stack).major_function=IRP_MJ_PNP;(*stack).minor_function=0x00;}
        assert_eq!(unsafe{io_call_driver(device,irp)},0);
        assert_eq!(io_pnp_state(device),1);
        unsafe{io_free_irp(irp);io_delete_device(device);}
    }

    #[test]
    fn pending_device_power_irp_commits_only_when_completed() {
        let mut driver:DriverObject=unsafe{core::mem::zeroed()};
        let mut device:*mut DeviceObject=ptr::null_mut();
        assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},0);
        driver.major_function[IRP_MJ_POWER as usize]=Some(pending_dispatch);
        let irp=unsafe{io_allocate_irp(1,0)};
        let stack=unsafe{io_get_next_irp_stack_location(irp)};
        unsafe{(*stack).major_function=IRP_MJ_POWER;(*stack).minor_function=IRP_MN_SET_POWER;(*stack).parameters[0]=1u64|(4u64<<32);}
        assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
        assert_eq!(device_power_state(device),0);
        unsafe{(*irp).bytes[0x30..0x34].copy_from_slice(&0i32.to_le_bytes());io_complete_request(irp,0);}
        assert_eq!(device_power_state(device),3);
        unsafe{io_free_irp(irp);io_delete_device(device);}
    }

    #[test]
    fn po_request_power_irp_dispatches_and_calls_completion_once() {
        CALLBACK_COUNT.store(0,Ordering::SeqCst);
        let mut driver:DriverObject=unsafe{core::mem::zeroed()};
        let mut device:*mut DeviceObject=ptr::null_mut();
        assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},0);
        driver.major_function[IRP_MJ_POWER as usize]=Some(success_dispatch);
        let status=unsafe{PoRequestPowerIrp(device,IRP_MN_SET_POWER,PowerState{device_state:4},Some(request_callback),ptr::null_mut(),ptr::null_mut())};
        assert_eq!(status,0);
        assert_eq!(CALLBACK_COUNT.load(Ordering::SeqCst),1);
        assert_eq!(device_power_state(device),3);
        unsafe{io_delete_device(device);}
    }
}

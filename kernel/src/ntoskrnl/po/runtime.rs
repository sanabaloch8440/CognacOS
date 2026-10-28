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


/// Register the platform-specific storage, memory, device, and CPU restore backend.
/// This RdotOS-private hook is not a Windows WDK export.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RdotPoRegisterHibernationBackend(ops:*mut super::hibernate::HiberPlatformOps)->i32 {
    unsafe { super::hibernate::register_platform(ops) }
}
/// Request S4. Returns only if the platform's enter-S4 routine returns.
#[unsafe(no_mangle)]
pub extern "system" fn RdotPoInitiateS4()->i32 { super::hibernate::initiate_s4() }
/// Boot/resume-loader hook; must be invoked from a resume-safe execution environment.
#[unsafe(no_mangle)]
pub extern "system" fn RdotPoResumeFromHibernation()->i32 { super::hibernate::resume_from_hibernation() }
#[unsafe(no_mangle)]
pub extern "system" fn RdotPoValidateHibernationImages()->i32 { super::hibernate::validate_hibernation_images() }

/// Report the committed state after a driver has completed its power transition.
/// Hardware sequencing remains owned by the driver's IRP_MJ_POWER path.
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
const IRP_MN_WAIT_WAKE:u8=0x00;
const IRP_MN_SET_POWER:u8=0x02;
const IRP_MN_QUERY_POWER:u8=0x03;
const STATUS_CANCELLED:i32=0xC000_0120u32 as i32;
const MAX_IDLE_DEVICES:usize=32;
fn performance_ticks_per_second()->u64 { crate::hal::query_performance_frequency_hz().unwrap_or(1).max(1) }
const STATUS_SUCCESS:i32=0;
const STATUS_PENDING:i32=0x0000_0103;
const STATUS_DEVICE_BUSY:i32=0x8000_0011u32 as i32;
const STATUS_INVALID_DEVICE_REQUEST:i32=0xC000_0010u32 as i32;
const STATUS_DELETE_PENDING:i32=0xC000_0056u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES:i32=0xC000_009Au32 as i32;
const MAX_POWER_TRANSITIONS:usize=64;

#[derive(Clone,Copy)]
struct Transition { used:bool, irp:*mut WdmIrp, device:*mut DeviceObject, kind:u8, target:u8, minor:u8 }
#[derive(Clone,Copy)]
struct IdleDevice {
 used:bool, device:*mut DeviceObject, token:u32, conservation_seconds:u32,
 performance_seconds:u32, target_state:u8, busy_count:u32, last_busy:u64,
}
unsafe impl Send for IdleDevice {}
impl IdleDevice { const EMPTY:Self=Self{used:false,device:core::ptr::null_mut(),token:0,conservation_seconds:0,performance_seconds:0,target_state:0,busy_count:0,last_busy:0}; }
struct IdleState { slots:[IdleDevice;MAX_IDLE_DEVICES], next_token:u32, conserve_policy:bool }
impl IdleState { const fn new()->Self{Self{slots:[IdleDevice::EMPTY;MAX_IDLE_DEVICES],next_token:1,conserve_policy:false}} }
static IDLE_DEVICES:crate::hal::IrqSpinLock<IdleState>=crate::hal::IrqSpinLock::new(IdleState::new());
unsafe impl Send for Transition {}
impl Transition { const EMPTY:Self=Self{used:false,irp:core::ptr::null_mut(),device:core::ptr::null_mut(),kind:0,target:0,minor:0}; }
struct TransitionState { slots:[Transition;MAX_POWER_TRANSITIONS] }
impl TransitionState { const fn new()->Self{Self{slots:[Transition::EMPTY;MAX_POWER_TRANSITIONS]}} }
static TRANSITIONS:crate::hal::IrqSpinLock<TransitionState>=crate::hal::IrqSpinLock::new(TransitionState::new());
static SYSTEM_POWER_IRP:core::sync::atomic::AtomicPtr<WdmIrp>=core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

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
 if major==IRP_MJ_POWER {
  if minor==IRP_MN_WAIT_WAKE {
   let system_wake=(parameters[0] as u32) as u8;
   if !(1..=6).contains(&system_wake){return STATUS_INVALID_DEVICE_REQUEST}
   let refd=unsafe{io::io_reference_device_object(device)};
   if refd.is_null(){return STATUS_DELETE_PENDING}
   let duplicate={let s=TRANSITIONS.lock();s.slots.iter().any(|t|t.used&&t.kind==4&&t.device==device)};
   if duplicate {unsafe{io::io_dereference_device_object(device)};return STATUS_DEVICE_BUSY}
   if let Err(status)=add_transition(Transition{used:true,irp,device,kind:4,target:system_wake,minor}) {
    unsafe{io::io_dereference_device_object(device)};return status
   }
   return STATUS_SUCCESS
  }
 }
 let (kind,target)=if major==IRP_MJ_POWER {
  if minor==IRP_MN_QUERY_POWER{return STATUS_SUCCESS}
  if minor!=IRP_MN_SET_POWER{return STATUS_SUCCESS}
  let state_type=parameters[0] as u32;
  let raw=(parameters[0]>>32) as u32;
  match state_type {
   0 if (1..=6).contains(&raw)=>{
    let owner=SYSTEM_POWER_IRP.load(Ordering::Acquire);
    if owner!=irp {
     match SYSTEM_POWER_IRP.compare_exchange(core::ptr::null_mut(),irp,Ordering::AcqRel,Ordering::Acquire) {
      Ok(_)=>{},
      Err(existing) if existing==irp=>{},
      Err(_)=>return STATUS_DEVICE_BUSY,
     }
    }
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
  // START_DEVICE is not allowed to reach the driver until both bus-provided
  // resource lists have passed bounded validation and translated resources
  // have been reserved atomically by the kernel resource arbiter.
  let resource_status=unsafe{io::io_prepare_pnp_resources(device,irp,minor,parameters[0] as *const core::ffi::c_void,parameters[1] as *const core::ffi::c_void)};
  if resource_status!=STATUS_SUCCESS {
   unsafe{let _=io::io_complete_pnp_transition(device,minor,resource_status);io::io_dereference_device_object(device);}
   return resource_status
  }
  (2,0)
 };
 if let Err(status)=add_transition(Transition{used:true,irp,device,kind,target,minor}) {
  match kind {
   1=>unsafe{let _=io::io_complete_device_power_transition(device,target,STATUS_INVALID_DEVICE_REQUEST);io::io_dereference_device_object(device)},
   2=>unsafe{io::io_finish_pnp_resources(irp,device,minor,STATUS_INVALID_DEVICE_REQUEST);let _=io::io_complete_pnp_transition(device,minor,STATUS_INVALID_DEVICE_REQUEST);io::io_dereference_device_object(device)},
   3=>release_system_power_irp(irp),
   _=>{}
  }
  return status
 }
 STATUS_SUCCESS
}
fn release_system_power_irp(irp:*mut WdmIrp) {
 let still_active={let s=TRANSITIONS.lock();s.slots.iter().any(|t|t.used&&t.irp==irp&&t.kind==3)};
 if !still_active {let _=SYSTEM_POWER_IRP.compare_exchange(irp,core::ptr::null_mut(),Ordering::AcqRel,Ordering::Acquire);}
}
pub(crate) fn po_finish_dispatch_return(irp:*mut WdmIrp,device:*mut DeviceObject,status:i32){
 if status==STATUS_PENDING{return}
 // Do not publish PnP state on dispatch return. The PnP reservation and its
 // device reference survive until IoCompleteRequest reaches terminal unwind.
 finish_matching(irp,Some(device),status,false);
}
fn finish_matching(irp:*mut WdmIrp,device:Option<*mut DeviceObject>,status:i32,include_pnp:bool){
 let mut ready=[Transition::EMPTY;MAX_POWER_TRANSITIONS];let mut n=0usize;
 {
  let mut s=TRANSITIONS.lock();
  for slot in s.slots.iter_mut(){
   if slot.used&&slot.irp==irp&&device.is_none_or(|d|slot.device==d)&&(include_pnp||slot.kind!=2){
    ready[n]=*slot;n+=1;*slot=Transition::EMPTY;
   }
  }
 }
 for t in ready.iter().take(n){
  match t.kind{
   1=>unsafe{let _=io::io_complete_device_power_transition(t.device,t.target,status);io::io_dereference_device_object(t.device)},
   2=>unsafe{io::io_finish_pnp_resources(irp,t.device,t.minor,status);let _=io::io_complete_pnp_transition(t.device,t.minor,status);io::io_dereference_device_object(t.device)},
   3=>{if status==STATUS_SUCCESS{SYSTEM_POWER_STATE.store(t.target as u32,Ordering::Release)}release_system_power_irp(irp);},
   4=>unsafe{io::io_dereference_device_object(t.device)},
   _=>{}
  }
 }
}
pub(crate) fn po_complete_irp(irp:*mut WdmIrp,status:i32){finish_matching(irp,None,status,true);finish_power_request(irp,status);}

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
 if device.is_null()||!matches!(minor,IRP_MN_WAIT_WAKE|IRP_MN_SET_POWER){return STATUS_INVALID_DEVICE_REQUEST}
 if !output_irp.is_null(){unsafe{output_irp.write(core::ptr::null_mut())}}
 let stack_size=unsafe{(*device).stack_size.max(1) as i16};
 let irp=unsafe{io::io_allocate_irp(stack_size,0)};
 if irp.is_null(){return STATUS_INSUFFICIENT_RESOURCES}
 let target=unsafe{if minor==IRP_MN_WAIT_WAKE{state.system_state}else{state.device_state}};
 if (minor==IRP_MN_WAIT_WAKE&&!(1..=6).contains(&target))||(minor==IRP_MN_SET_POWER&&!(1..=4).contains(&target)){unsafe{io::io_free_irp(irp)};return STATUS_INVALID_DEVICE_REQUEST}
 let stack=unsafe{io::io_get_next_irp_stack_location(irp)};
 if stack.is_null(){unsafe{io::io_free_irp(irp)};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{(*stack).major_function=IRP_MJ_POWER;(*stack).minor_function=minor;(*stack).parameters[0]=if minor==IRP_MN_WAIT_WAKE{target as u64}else{1u64|((target as u64)<<32)};}
 {
  let mut requests=POWER_REQUESTS.lock();
  let Some(slot)=requests.slots.iter_mut().find(|r|!r.used) else{drop(requests);unsafe{io::io_free_irp(irp)};return STATUS_INSUFFICIENT_RESOURCES};
  *slot=PowerRequest{used:true,irp,device,minor,state:target,context,callback,returned:false,completed:false};
 }
 unsafe{let _=io::io_set_cancel_routine(irp,Some(power_request_cancel));}
 if !output_irp.is_null(){unsafe{output_irp.write(irp)}}
 let status=unsafe{io::io_call_driver(device,irp)};
 let synth={
  let requests=POWER_REQUESTS.lock();
  requests.slots.iter().find(|r|r.used&&r.irp==irp).is_some_and(|r|!r.completed&&status!=STATUS_PENDING)
 };
 if synth{unsafe{(&mut (*irp).bytes)[0x30..0x34].copy_from_slice(&status.to_le_bytes());}finish_power_request(irp,status);}
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
 let _=unsafe{io::io_set_cancel_routine(irp,None)};
 if let Some(callback)=snapshot.callback{
  let state=if snapshot.minor==IRP_MN_WAIT_WAKE{PowerState{system_state:snapshot.state}}else{PowerState{device_state:snapshot.state}};
  unsafe{callback(snapshot.device,snapshot.minor,state,snapshot.context,irp)}
 }
 if snapshot.returned{unsafe{io::io_free_irp(irp)}}
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoCallDriver(device:*mut DeviceObject,irp:*mut WdmIrp)->i32 { unsafe{io::io_call_driver(device,irp)} }


unsafe extern "system" fn power_request_cancel(device:*mut DeviceObject,irp:*mut WdmIrp) {
 if irp.is_null(){return}
 unsafe{(&mut (*irp).bytes)[0x30..0x34].copy_from_slice(&STATUS_CANCELLED.to_le_bytes());}
 let _=device;
 unsafe{io::io_complete_request(irp,0);}
}

/// Register or update per-device idle detection. The returned ULONG pointer is
/// a stable token, not a writable timer; idle countdowns use the monotonic clock.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoRegisterDeviceForIdleDetection(
 device:*mut DeviceObject, conservation_idle_time:u32, performance_idle_time:u32, state:u32
)->*mut u32 {
 if device.is_null()||!(1..=4).contains(&state){return core::ptr::null_mut()}
 if conservation_idle_time==0&&performance_idle_time==0 {
  let removed={
   let mut idle=IDLE_DEVICES.lock();
   if let Some(i)=idle.slots.iter().position(|s|s.used&&s.device==device){let old=idle.slots[i];idle.slots[i]=IdleDevice::EMPTY;Some(old)}else{None}
  };
  if let Some(old)=removed{unsafe{io::io_dereference_device_object(old.device)}}
  return core::ptr::null_mut()
 }
 if conservation_idle_time==u32::MAX||performance_idle_time==u32::MAX {
  let kind=unsafe{(*device).device_type};
  if kind!=7&&kind!=0x2d{return core::ptr::null_mut()}
 }
 let existing={let idle=IDLE_DEVICES.lock();idle.slots.iter().position(|s|s.used&&s.device==device)};
 if existing.is_none()&&unsafe{io::io_reference_device_object(device)}.is_null(){return core::ptr::null_mut()}
 let now=crate::hal::query_performance_counter();
 let mut idle=IDLE_DEVICES.lock();
 let index=if let Some(i)=existing{i}else if let Some(i)=idle.slots.iter().position(|s|!s.used){i}else{
  drop(idle);unsafe{io::io_dereference_device_object(device)};return core::ptr::null_mut()
 };
 let token=if idle.slots[index].used{idle.slots[index].token}else{let t=idle.next_token;idle.next_token=idle.next_token.wrapping_add(1).max(1);t};
 idle.slots[index]=IdleDevice{used:true,device,token,conservation_seconds:conservation_idle_time,performance_seconds:performance_idle_time,target_state:(state-1) as u8,busy_count:0,last_busy:now};
 core::ptr::addr_of_mut!(idle.slots[index].token)
}

fn find_idle_token(token:*mut u32)->Option<usize>{
 if token.is_null(){return None}
 let idle=IDLE_DEVICES.lock();
 idle.slots.iter().position(|s|s.used&&core::ptr::addr_of!(s.token) as *mut u32==token)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoSetDeviceBusyEx(token:*mut u32) {
 let Some(index)=find_idle_token(token) else{return};
 let now=crate::hal::query_performance_counter();
 let mut idle=IDLE_DEVICES.lock();
 if let Some(s)=idle.slots.get_mut(index).filter(|s|s.used&&core::ptr::addr_of!(s.token) as *mut u32==token){s.last_busy=now;}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoStartDeviceBusy(token:*mut u32) {
 let Some(index)=find_idle_token(token) else{return};
 let now=crate::hal::query_performance_counter();
 let mut idle=IDLE_DEVICES.lock();
 if let Some(s)=idle.slots.get_mut(index).filter(|s|s.used&&core::ptr::addr_of!(s.token) as *mut u32==token){s.busy_count=s.busy_count.saturating_add(1);s.last_busy=now;}
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoEndDeviceBusy(token:*mut u32) {
 let Some(index)=find_idle_token(token) else{return};
 let now=crate::hal::query_performance_counter();
 let mut idle=IDLE_DEVICES.lock();
 if let Some(s)=idle.slots.get_mut(index).filter(|s|s.used&&core::ptr::addr_of!(s.token) as *mut u32==token){if s.busy_count>0{s.busy_count-=1;if s.busy_count==0{s.last_busy=now;}}}
}

/// Set which timeout policy applies; a passive-level worker can call
/// PoPollIdleDevices to issue the resulting power IRPs outside the idle lock.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoSetIdlePowerPolicy(conservation: u8) {
 IDLE_DEVICES.lock().conserve_policy=conservation!=0;
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn PoPollIdleDevices()->u32 {
 let now=crate::hal::query_performance_counter();
 let mut requests=[(core::ptr::null_mut::<DeviceObject>(),0u32);MAX_IDLE_DEVICES];
 let mut count=0usize;
 {
  let mut idle=IDLE_DEVICES.lock();
  let conserve=idle.conserve_policy;
  for s in idle.slots.iter_mut(){
   if !s.used||s.busy_count!=0{continue}
   let seconds=if conserve{s.conservation_seconds}else{s.performance_seconds};
   if seconds==0||seconds==u32::MAX{continue}
   let ticks=(seconds as u64).saturating_mul(performance_ticks_per_second());
   if now.saturating_sub(s.last_busy)>=ticks{
    requests[count]=(s.device,s.target_state as u32+1);count+=1;s.last_busy=now;
   }
  }
 }
 let mut issued=0u32;
 for (device,state) in requests.iter().take(count){
  let _=unsafe{PoRequestPowerIrp(*device,IRP_MN_SET_POWER,PowerState{device_state:*state},None,core::ptr::null_mut(),core::ptr::null_mut())};
  issued=issued.saturating_add(1);
 }
 issued
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
        unsafe{(&mut (*irp).bytes)[0x30..0x34].copy_from_slice(&0i32.to_le_bytes());io_complete_request(irp,0);}
        assert_eq!(device_power_state(device),3);
        unsafe{io_free_irp(irp);io_delete_device(device);}
    }

    #[test]
    fn wait_wake_irp_can_be_cancelled_and_completes_once() {
        CALLBACK_COUNT.store(0,Ordering::SeqCst);
        let mut driver:DriverObject=unsafe{core::mem::zeroed()};
        let mut device:*mut DeviceObject=ptr::null_mut();
        assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},0);
        driver.major_function[IRP_MJ_POWER as usize]=Some(pending_dispatch);
        let mut irp:*mut WdmIrp=ptr::null_mut();
        let status=unsafe{PoRequestPowerIrp(device,IRP_MN_WAIT_WAKE,PowerState{system_state:3},Some(request_callback),ptr::null_mut(),&mut irp)};
        assert_eq!(status,STATUS_PENDING);
        assert!(!irp.is_null());
        assert_eq!(unsafe{io::io_cancel_irp(irp)},1);
        assert_eq!(CALLBACK_COUNT.load(Ordering::SeqCst),1);
        unsafe{io_delete_device(device);}
    }

    #[test]
    fn idle_detection_register_busy_and_unregister_are_bounded() {
        let mut driver:DriverObject=unsafe{core::mem::zeroed()};
        let mut device:*mut DeviceObject=ptr::null_mut();
        assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},0);
        let token=unsafe{PoRegisterDeviceForIdleDetection(device,10,20,4)};
        assert!(!token.is_null());
        unsafe{PoStartDeviceBusy(token);PoSetDeviceBusyEx(token);PoEndDeviceBusy(token);}
        assert!(unsafe{PoRegisterDeviceForIdleDetection(device,0,0,4)}.is_null());
        unsafe{io_delete_device(device);}
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

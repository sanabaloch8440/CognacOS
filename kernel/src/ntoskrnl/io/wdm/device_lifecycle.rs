// Private high bits prevent new device creation after unload begins.
const DRIVER_UNLOADING_FLAG:u32=1<<31;
const DRIVER_UNLOADED_FLAG:u32=1<<30;

pub const DEVICE_POWER_D0:u8=0; pub const DEVICE_POWER_D1:u8=1; pub const DEVICE_POWER_D2:u8=2; pub const DEVICE_POWER_D3:u8=3;




#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_reference_device_object(device:*mut DeviceObject)->*mut DeviceObject {
 if device.is_null(){return ptr::null_mut()}
 let mut state=WDM.lock();
 let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)&&!s.deleting) else{return ptr::null_mut()};
 let Some(next)=slot.object.reference_count.checked_add(1) else{return ptr::null_mut()};
 slot.object.reference_count=next;device
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_dereference_device_object(device:*mut DeviceObject) {
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
pub unsafe extern "win64" fn io_acquire_device_remove_lock(device:*mut DeviceObject)->i32 {
 if device.is_null(){return STATUS_INVALID_PARAMETER}
 let mut state=WDM.lock();
 let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
 if s.deleting{return STATUS_DELETE_PENDING}
 let Some(n)=s.remove_lock_count.checked_add(1) else{return STATUS_INSUFFICIENT_RESOURCES};
 s.remove_lock_count=n;STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_release_device_remove_lock(device:*mut DeviceObject) {
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
pub unsafe extern "win64" fn io_begin_device_power_transition(device:*mut DeviceObject,target:u8)->i32 {
 if device.is_null()||target>DEVICE_POWER_D3{return STATUS_INVALID_PARAMETER}
 let mut state=WDM.lock();
 let Some(s)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
 if s.deleting{return STATUS_DELETE_PENDING}
 if s.power_transition{return STATUS_DEVICE_BUSY}
 s.power_target=target;s.power_transition=true;STATUS_SUCCESS
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_complete_device_power_transition(device:*mut DeviceObject,target:u8,status:i32)->i32 {
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
 s.pnp_transition=false;
 drop(state);
 if status==STATUS_SUCCESS&&matches!(minor,0x02|0x17){crate::ntoskrnl::po::device_power::unregister(device);}
 STATUS_SUCCESS
}

/// Bounded CM_RESOURCE_LIST parser and resource arbiter for WDM start IRPs.
/// The caller must pass kernel-captured CM_RESOURCE_LIST pointers, never raw
/// user pointers. This reserves translated I/O ports, memory ranges and IRQs;
/// it does not program the underlying bus or replace a hardware bus driver.
//! WDM device-power capability facade.
//! Capability state is stored in the I/O manager's stable DEVICE_OBJECT record;
//! it therefore shares the same reference, removal, and nonpaged lifetime rules.
use crate::io::DeviceObject;

const D0:u8=1; const D1:u8=2; const D2:u8=4; const D3:u8=8;
const DEFAULT:u8=D0|D3;
const OK:i32=0; const INVALID:i32=0xC000_000Du32 as i32;

pub(crate) fn supports_wdm_state(device:*mut DeviceObject,state:u32)->bool{
 crate::io::io_device_supports_power_state(device,state)
}
pub(crate) fn supports_wdm_state_stack(device:*mut DeviceObject,state:u32)->bool{
 crate::io::io_device_supports_power_state_stack(device,state)
}
pub(crate) fn supports_wake(device:*mut DeviceObject)->bool{
 crate::io::io_device_supports_wake(device)
}
pub(crate) fn supports_wake_stack(device:*mut DeviceObject)->bool{
 crate::io::io_device_supports_wake_stack(device)
}
pub(crate) fn register_default(device:*mut DeviceObject)->i32{
 register(device,DEFAULT,false)
}
pub(crate) fn register(device:*mut DeviceObject,states:u8,wake:bool)->i32{
 if device.is_null()||states&D0==0||states&!(D0|D1|D2|D3)!=0||(wake&&states&D3!=0&&states&(D1|D2)==0){return INVALID}
 crate::io::io_set_device_power_capabilities(device,states,wake)
}
pub(crate) fn unregister(device:*mut DeviceObject){
 crate::io::io_clear_device_power_capabilities(device)
}

#[cfg(test)]
mod tests{
 use super::*;
 use crate::io::{io_create_device,io_delete_device,io_attach_device_to_device_stack,io_detach_device,DriverObject};
 #[test]
 fn defaults_are_d0_d3_without_wake(){
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device=core::ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,core::ptr::null_mut(),7,0,0,&mut device)},OK);
  assert!(supports_wdm_state(device,1));assert!(!supports_wdm_state(device,2));
  assert!(supports_wdm_state(device,4));assert!(!supports_wake(device));
  unsafe{io_delete_device(device);}
 }
 #[test]
 fn registered_capabilities_are_enforced(){
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device=core::ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,core::ptr::null_mut(),7,0,0,&mut device)},OK);
  assert_eq!(register(device,D0|D1|D3,true),OK);
  assert!(supports_wdm_state(device,2));assert!(supports_wake(device));
  unregister(device);assert!(!supports_wake(device));
  unsafe{io_delete_device(device);}
 }
 #[test]
 fn invalid_capability_sets_are_rejected(){
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device=core::ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,core::ptr::null_mut(),7,0,0,&mut device)},OK);
  assert_eq!(register(device,D3,false),INVALID);
  assert_eq!(register(device,D0|D3,true),INVALID);
  unsafe{io_delete_device(device);}
 }
 #[test]
 fn wake_requires_every_attached_stack_device(){
  let mut lower_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut upper_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut lower=core::ptr::null_mut();let mut upper=core::ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut lower_driver,0,core::ptr::null_mut(),7,0,0,&mut lower)},OK);
  assert_eq!(unsafe{io_create_device(&mut upper_driver,0,core::ptr::null_mut(),7,0,0,&mut upper)},OK);
  assert_eq!(unsafe{io_attach_device_to_device_stack(upper,lower)},lower);
  assert_eq!(register(lower,D0|D1|D2|D3,true),OK);
  assert_eq!(register(upper,D0|D1|D2|D3,true),OK);
  assert!(supports_wdm_state_stack(upper,2));assert!(supports_wake_stack(upper));
  assert_eq!(register(lower,D0|D3,false),OK);
  assert!(!supports_wdm_state_stack(upper,2));assert!(!supports_wake_stack(upper));
  unsafe{io_detach_device(lower);io_delete_device(upper);io_delete_device(lower);}
 }
}

//! Conservative WDM device-power capability registry.
//! Drivers must still implement IRP_MJ_POWER hardware operations.
use crate::io::DeviceObject;
const MAX_DEVICES:usize=64;
const D0:u8=1; const D1:u8=2; const D2:u8=4; const D3:u8=8;
const DEFAULT:u8=D0|D3;
const OK:i32=0; const INVALID:i32=0xC000_000Du32 as i32; const FULL:i32=0xC000_009Au32 as i32;
#[derive(Clone,Copy)] struct Entry{device:usize,states:u8,wake:bool}
impl Entry{const EMPTY:Self=Self{device:0,states:0,wake:false};}
struct Registry{entries:[Entry;MAX_DEVICES]}
impl Registry{const fn new()->Self{Self{entries:[Entry::EMPTY;MAX_DEVICES]}}}
static REGISTRY:crate::hal::IrqSpinLock<Registry>=crate::hal::IrqSpinLock::new(Registry::new());
fn find(device:*mut DeviceObject)->Option<Entry>{if device.is_null(){return None} REGISTRY.lock().entries.iter().copied().find(|e|e.device==device as usize)}
pub(crate) fn supports_wdm_state(device:*mut DeviceObject,state:u32)->bool{if !(1..=4).contains(&state){return false}let bit=1u8<<(state-1);find(device).map_or(bit&DEFAULT!=0,|e|e.states&bit!=0)}
pub(crate) fn supports_wake(device:*mut DeviceObject)->bool{find(device).is_some_and(|e|e.wake)}
pub(crate) fn register(device:*mut DeviceObject,states:u8,wake:bool)->i32{
 if device.is_null()||states&D0==0||states&!(D0|D1|D2|D3)!=0||(wake&&states&D3!=0&&states&(D1|D2)==0){return INVALID}
 let mut r=REGISTRY.lock();if let Some(e)=r.entries.iter_mut().find(|e|e.device==device as usize){e.states=states;e.wake=wake;return OK}
 let Some(e)=r.entries.iter_mut().find(|e|e.device==0)else{return FULL};*e=Entry{device:device as usize,states,wake};OK
}
pub(crate) fn unregister(device:*mut DeviceObject){if device.is_null(){return}if let Some(e)=REGISTRY.lock().entries.iter_mut().find(|e|e.device==device as usize){*e=Entry::EMPTY}}

#[cfg(test)] mod tests{use super::*;#[test]fn defaults_are_d0_d3_without_wake(){let d=0x1000usize as *mut DeviceObject;assert!(supports_wdm_state(d,1));assert!(!supports_wdm_state(d,2));assert!(supports_wdm_state(d,4));assert!(!supports_wake(d));}
#[test]fn registered_capabilities_are_enforced(){let d=0x2000usize as *mut DeviceObject;assert_eq!(register(d,D0|D1|D3,true),OK);assert!(supports_wdm_state(d,2));assert!(supports_wake(d));unregister(d);assert!(!supports_wake(d));}
#[test]fn invalid_capability_sets_rejected(){let d=0x3000usize as *mut DeviceObject;assert_eq!(register(d,D3,false),INVALID);assert_eq!(register(d,D0|D3,true),INVALID);}}

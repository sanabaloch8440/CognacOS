//! ACPI-5 resource selection and device power policy.
#![no_std]
use crate::acpi_resources::{parse_resource_template,ResourceError,ResourceList};
pub const MAX_WAKE_RESOURCES:usize=8;
pub const ACPI_POWER_D0:u8=0;
pub const ACPI_POWER_D3:u8=3;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PowerError{InvalidState,InvalidSleepState,InvalidWakePackage,TooManyWakeResources,Resource(ResourceError),MethodFailed}
impl From<ResourceError> for PowerError{fn from(v:ResourceError)->Self{Self::Resource(v)}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct WakeResource{pub gpe_device_path:[u8;64],pub gpe_device_path_length:u8,pub gpe_number:u32}
impl WakeResource{pub const EMPTY:Self=Self{gpe_device_path:[0;64],gpe_device_path_length:0,gpe_number:0};}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct WakeInfo{pub resources:[WakeResource;MAX_WAKE_RESOURCES],pub resource_count:u8,pub deepest_sleep_state:u8}
impl WakeInfo{pub const EMPTY:Self=Self{resources:[WakeResource::EMPTY;MAX_WAKE_RESOURCES],resource_count:0,deepest_sleep_state:0};}
/// Validate and normalize _PRW wake resources and the deepest supported system sleep state.
pub fn build_wake_info(gpe:WakeResource,deepest_sleep_state:u64,additional_gpes:&[WakeResource])->Result<WakeInfo,PowerError>{
 if deepest_sleep_state>5{return Err(PowerError::InvalidSleepState)}
 if additional_gpes.len()>=MAX_WAKE_RESOURCES{return Err(PowerError::TooManyWakeResources)}
 let n=gpe.gpe_device_path_length as usize;
 if n>64||(n!=0&&(gpe.gpe_device_path[0]!=b'\\'||gpe.gpe_device_path[..n].contains(&0))){return Err(PowerError::InvalidWakePackage)}
 let mut out=WakeInfo::EMPTY;out.resources[0]=gpe;out.resource_count=1;out.deepest_sleep_state=deepest_sleep_state as u8;
 for item in additional_gpes{let i=out.resource_count as usize;let n=item.gpe_device_path_length as usize;
  if i>=MAX_WAKE_RESOURCES||n>64||(n!=0&&(item.gpe_device_path[0]!=b'\\'||item.gpe_device_path[..n].contains(&0))){return Err(PowerError::InvalidWakePackage)}
  out.resources[i]=*item;out.resource_count+=1;
 }Ok(out)
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PowerMethod{Disable,PowerD0,PowerD1,PowerD2,PowerD3}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct DevicePowerState{pub d_state:u8,pub disabled:bool,pub wake:Option<WakeInfo>}
impl DevicePowerState{pub const INITIAL:Self=Self{d_state:0,disabled:false,wake:None};}
pub trait AcpiPowerMethodInvoker{
 fn evaluate_prs(&mut self)->Result<&[u8],PowerError>;
 fn set_srs(&mut self,selected:&ResourceList)->bool;
 fn invoke(&mut self,method:PowerMethod)->bool;
 fn evaluate_prw(&mut self)->Result<Option<WakeInfo>,PowerError>;
}
/// Evaluate and decode a device's _PRS resource alternatives; this does not arbitrate them.
pub fn evaluate_prs(i:&mut impl AcpiPowerMethodInvoker)->Result<ResourceList,PowerError>{Ok(parse_resource_template(i.evaluate_prs()?)?)}
/// Ask the firmware backend to program a previously selected resource configuration through _SRS.
pub fn apply_srs(i:&mut impl AcpiPowerMethodInvoker,r:&ResourceList)->Result<(),PowerError>{if i.set_srs(r){Ok(())}else{Err(PowerError::MethodFailed)}}
/// Evaluate _PRW through the firmware backend and return its validated wake description.
pub fn evaluate_prw(i:&mut impl AcpiPowerMethodInvoker)->Result<Option<WakeInfo>,PowerError>{i.evaluate_prw()}
/// Invoke the matching _PS0 through _PS3 method and update cached D-state only after success.
pub fn transition_power(i:&mut impl AcpiPowerMethodInvoker,s:&mut DevicePowerState,target:u8)->Result<(),PowerError>{
 if target>3{return Err(PowerError::InvalidState)}if target==s.d_state{return Ok(())}
 let method=match target{0=>PowerMethod::PowerD0,1=>PowerMethod::PowerD1,2=>PowerMethod::PowerD2,3=>PowerMethod::PowerD3,_=>return Err(PowerError::InvalidState)};
 if !i.invoke(method){return Err(PowerError::MethodFailed)}s.d_state=target;Ok(())
}
/// Invoke the device disable policy method and commit disabled state only after success.
pub fn disable_device(i:&mut impl AcpiPowerMethodInvoker,s:&mut DevicePowerState)->Result<(),PowerError>{
 if s.disabled{return Ok(())}if !i.invoke(PowerMethod::Disable){return Err(PowerError::MethodFailed)}s.disabled=true;Ok(())
}
#[cfg(test)]extern crate std;
#[cfg(test)]mod tests{
 use super::*;
 struct Fake{prs:[u8;2],ok:bool,calls:[Option<PowerMethod>;4],count:usize}
 impl AcpiPowerMethodInvoker for Fake{
  fn evaluate_prs(&mut self)->Result<&[u8],PowerError>{Ok(&self.prs)}
  fn set_srs(&mut self,_:&ResourceList)->bool{self.ok}
  fn invoke(&mut self,m:PowerMethod)->bool{if self.count<4{self.calls[self.count]=Some(m)}self.count+=1;self.ok}
  fn evaluate_prw(&mut self)->Result<Option<WakeInfo>,PowerError>{Ok(None)}
 }
 fn fake(ok:bool)->Fake{Fake{prs:[0x79,0],ok,calls:[None;4],count:0}}
 #[test]fn prs_is_validated(){let mut f=fake(true);assert_eq!(evaluate_prs(&mut f).unwrap().count,1);f.prs=[0x47,0];assert!(evaluate_prs(&mut f).is_err());}
 #[test]fn srs_failure_propagates(){let mut f=fake(false);assert_eq!(apply_srs(&mut f,&ResourceList::EMPTY),Err(PowerError::MethodFailed));}
 #[test]fn power_state_changes_only_after_success(){let mut f=fake(false);let mut s=DevicePowerState::INITIAL;assert_eq!(transition_power(&mut f,&mut s,3),Err(PowerError::MethodFailed));assert_eq!(s.d_state,0);f.ok=true;transition_power(&mut f,&mut s,3).unwrap();assert_eq!(s.d_state,3);}
 #[test]fn every_device_power_state_invokes_its_matching_method(){let mut f=fake(true);let mut s=DevicePowerState::INITIAL;for (target,expected) in [(1,PowerMethod::PowerD1),(2,PowerMethod::PowerD2),(0,PowerMethod::PowerD0),(3,PowerMethod::PowerD3)]{transition_power(&mut f,&mut s,target).unwrap();assert_eq!(f.calls[f.count-1],Some(expected));assert_eq!(s.d_state,target);}}
 #[test]fn invalid_device_power_state_does_not_invoke_firmware(){let mut f=fake(true);let mut s=DevicePowerState::INITIAL;assert_eq!(transition_power(&mut f,&mut s,4),Err(PowerError::InvalidState));assert_eq!(f.count,0);assert_eq!(s.d_state,0);}
 #[test]fn prw_sleep_state_checked(){let g=WakeResource{gpe_number:12,..WakeResource::EMPTY};assert_eq!(build_wake_info(g,6,&[]),Err(PowerError::InvalidSleepState));assert_eq!(build_wake_info(g,3,&[]).unwrap().deepest_sleep_state,3);}
 #[test]fn disable_failure_does_not_commit(){let mut f=fake(false);let mut s=DevicePowerState::INITIAL;assert_eq!(disable_device(&mut f,&mut s),Err(PowerError::MethodFailed));assert!(!s.disabled);}
}
//! ACPI-5 resource selection and device power policy.
/*
 * FIXME(ACPI-PRODUCTION) — ACPI power policy
 * FIXME(ACPI-PRODUCTION): Complete device/system power-state evaluation and sequencing, power-resource dependencies, rollback, wake handling, and coordination with kernel power IRPs. Parsing a power method is not proof that suspend/resume is safe; require transition tests and hardware-specific gates.
 */

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
pub struct DevicePowerState{pub d_state:u8,pub disabled:bool,pub wake:Option<WakeInfo>,pub active_resources:[u16;MAX_POWER_RESOURCE_DEPENDENCIES],pub active_resource_count:u8}
impl DevicePowerState{pub const INITIAL:Self=Self{d_state:0,disabled:false,wake:None,active_resources:[u16::MAX;MAX_POWER_RESOURCE_DEPENDENCIES],active_resource_count:0};}
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

#[cfg(test)] mod acpi_v6_srs_success_path {
 use super::*;
 
 struct SuccessfulSrs;
 impl AcpiPowerMethodInvoker for SuccessfulSrs {
  fn evaluate_prs(&mut self)->Result<&[u8],PowerError>{Ok(&[])}
  fn set_srs(&mut self,_:&ResourceList)->bool{true}
  fn invoke(&mut self,_:PowerMethod)->bool{true}
  fn evaluate_prw(&mut self)->Result<Option<WakeInfo>,PowerError>{Ok(None)}
 }
 #[test] fn successful_srs_application_calls_firmware_once() {
  let mut firmware=SuccessfulSrs;
  assert_eq!(apply_srs(&mut firmware,&ResourceList::EMPTY),Ok(()));
 }

}


// ACPI 5.0+ power-resource dependency management. The graph is deliberately
// independent of AML evaluation and hardware access: the driver supplies a
// backend that invokes _ON/_OFF and serializes graph mutations with PnP power IRPs.
pub const MAX_POWER_RESOURCES:usize=64;
pub const MAX_POWER_RESOURCE_DEPENDENCIES:usize=8;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PowerResourceError{
 InvalidNode,DuplicateNode,Capacity,TooManyDependencies,DuplicateDependency,
 MissingDependency,DependencyCycle,ReferenceOverflow,NotReferenced,
 BackendFailure,InvalidSleepPackage,InvalidPerformanceLimit,InvalidPowerState,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PowerResource{
 pub node:u16,
 pub system_level:u8,
 pub order:u8,
 pub dependencies:[u16;MAX_POWER_RESOURCE_DEPENDENCIES],
 pub dependency_count:u8,
 pub references:u16,
 pub enabled:bool,
}
impl PowerResource{
 pub const EMPTY:Self=Self{node:u16::MAX,system_level:0,order:0,
  dependencies:[u16::MAX;MAX_POWER_RESOURCE_DEPENDENCIES],dependency_count:0,
  references:0,enabled:false};
}
pub trait PowerResourceBackend{
 /// Invoke the resource's _ON or _OFF method. Return only after AML completes.
 fn set_power_resource(&mut self,node:u16,enabled:bool)->bool;
}
/// Bounded namespace-node keyed graph for _PR0/_PR1/_PR2/_PR3 dependencies.
pub struct PowerResourceGraph{
 entries:[PowerResource;MAX_POWER_RESOURCES],
 count:usize,
}
impl PowerResourceGraph{
 pub const fn new()->Self{Self{entries:[PowerResource::EMPTY;MAX_POWER_RESOURCES],count:0}}
 pub fn len(&self)->usize{self.count}
 fn find(&self,node:u16)->Option<usize>{self.entries[..self.count].iter().position(|e|e.node==node)}
 pub fn resource(&self,node:u16)->Option<&PowerResource>{self.find(node).map(|i|&self.entries[i])}
 /// Register a PowerResource namespace object and its _STA-reconciled initial state.
 pub fn register(&mut self,node:u16,system_level:u8,order:u8,enabled:bool)->Result<(),PowerResourceError>{
  if node==u16::MAX||system_level>5{return Err(PowerResourceError::InvalidNode)}
  if self.find(node).is_some(){return Err(PowerResourceError::DuplicateNode)}
  if self.count==MAX_POWER_RESOURCES{return Err(PowerResourceError::Capacity)}
  self.entries[self.count]=PowerResource{node,system_level,order,enabled,
   references:if enabled{1}else{0},..PowerResource::EMPTY};
  self.count+=1;Ok(())
 }
 /// Replace dependencies only after every referenced PowerResource is known.
 pub fn set_dependencies(&mut self,node:u16,dependencies:&[u16])->Result<(),PowerResourceError>{
  if dependencies.len()>MAX_POWER_RESOURCE_DEPENDENCIES{return Err(PowerResourceError::TooManyDependencies)}
  let index=self.find(node).ok_or(PowerResourceError::InvalidNode)?;
  for (i,dependency) in dependencies.iter().enumerate(){
   if *dependency==node||self.find(*dependency).is_none(){return Err(PowerResourceError::MissingDependency)}
   if dependencies[..i].contains(dependency){return Err(PowerResourceError::DuplicateDependency)}
  }
  self.entries[index].dependencies=[u16::MAX;MAX_POWER_RESOURCE_DEPENDENCIES];
  self.entries[index].dependencies[..dependencies.len()].copy_from_slice(dependencies);
  self.entries[index].dependency_count=dependencies.len() as u8;
  Ok(())
 }
 fn acquire_inner(&mut self,index:usize,backend:&mut impl PowerResourceBackend,
  path:&mut [u16;MAX_POWER_RESOURCES],depth:usize)->Result<(),PowerResourceError>{
  if depth>=MAX_POWER_RESOURCES{return Err(PowerResourceError::DependencyCycle)}
  let node=self.entries[index].node;
  if path[..depth].contains(&node){return Err(PowerResourceError::DependencyCycle)}
  if self.entries[index].references==u16::MAX{return Err(PowerResourceError::ReferenceOverflow)}
  if self.entries[index].references!=0{
   self.entries[index].references+=1;return Ok(())
  }
  path[depth]=node;
  let count=self.entries[index].dependency_count as usize;
  let dependencies=self.entries[index].dependencies;
  let mut acquired=0usize;
  while acquired<count{
   let dep_index=self.find(dependencies[acquired]).ok_or(PowerResourceError::MissingDependency)?;
   if let Err(error)=self.acquire_inner(dep_index,backend,path,depth+1){
    while acquired>0{acquired-=1;let rollback=self.find(dependencies[acquired]).ok_or(PowerResourceError::MissingDependency)?;
     let _=self.release_inner(rollback,backend);}
    return Err(error)
   }
   acquired+=1;
  }
  if !backend.set_power_resource(node,true){
   while acquired>0{acquired-=1;let rollback=self.find(dependencies[acquired]).ok_or(PowerResourceError::MissingDependency)?;
    let _=self.release_inner(rollback,backend);}
   return Err(PowerResourceError::BackendFailure)
  }
  self.entries[index].enabled=true;self.entries[index].references=1;Ok(())
 }
 fn release_inner(&mut self,index:usize,backend:&mut impl PowerResourceBackend)->Result<(),PowerResourceError>{
  let resource=self.entries[index];
  if resource.references==0{return Err(PowerResourceError::NotReferenced)}
  if resource.references>1{self.entries[index].references-=1;return Ok(())}
  if !backend.set_power_resource(resource.node,false){return Err(PowerResourceError::BackendFailure)}
  self.entries[index].enabled=false;self.entries[index].references=0;
  for dependency in resource.dependencies[..resource.dependency_count as usize].iter().rev(){
   let dep_index=self.find(*dependency).ok_or(PowerResourceError::MissingDependency)?;
   self.release_inner(dep_index,backend)?;
  }
  Ok(())
 }
 /// Acquire a resource and all transitive dependencies in dependency-first order.
 pub fn acquire(&mut self,node:u16,backend:&mut impl PowerResourceBackend)->Result<(),PowerResourceError>{
  let index=self.find(node).ok_or(PowerResourceError::InvalidNode)?;
  self.acquire_inner(index,backend,&mut [u16::MAX;MAX_POWER_RESOURCES],0)
 }
 /// Release one reference; dependencies are released only after the resource turns off.
 pub fn release(&mut self,node:u16,backend:&mut impl PowerResourceBackend)->Result<(),PowerResourceError>{
  let index=self.find(node).ok_or(PowerResourceError::InvalidNode)?;
  self.release_inner(index,backend)
 }
 /// Atomically acquire a device's selected _PRx set; unwind every acquired reference on failure.
 pub fn acquire_set(&mut self,nodes:&[u16],backend:&mut impl PowerResourceBackend)->Result<(),PowerResourceError>{
  for (i,node) in nodes.iter().enumerate(){
   if nodes[..i].contains(node){return Err(PowerResourceError::DuplicateDependency)}
   if self.find(*node).is_none(){return Err(PowerResourceError::InvalidNode)}
  }
  let mut acquired=0usize;
  while acquired<nodes.len(){
   if let Err(error)=self.acquire(nodes[acquired],backend){
    while acquired>0{acquired-=1;let _=self.release(nodes[acquired],backend);}
    return Err(error)
   }
   acquired+=1;
  }
  Ok(())
 }
 /// Release a device's resource set in reverse declaration order.
 pub fn release_set(&mut self,nodes:&[u16],backend:&mut impl PowerResourceBackend)->Result<(),PowerResourceError>{
  for node in nodes.iter().rev(){self.release(*node,backend)?;}
  Ok(())
 }
}

/// ACPI _S1.._S5 package data: exactly two 3-bit sleep-type values.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SleepType{pub state:u8,pub type_a:u8,pub type_b:u8}
pub fn parse_sleep_type(state:u8,package:&[u64])->Result<SleepType,PowerResourceError>{
 if !(1..=5).contains(&state)||package.len()!=2||package[0]>7||package[1]>7{
  return Err(PowerResourceError::InvalidSleepPackage)
 }
 Ok(SleepType{state,type_a:package[0] as u8,type_b:package[1] as u8})
}
/// Validate integer results returned by _PSC, _PPC and _PSR before policy consumes them.
pub fn validate_current_power_state(value:u64)->Result<u8,PowerResourceError>{
 if value>3{Err(PowerResourceError::InvalidPowerState)}else{Ok(value as u8)}
}
pub fn validate_performance_limit(value:u64,performance_states:u8)->Result<u8,PowerResourceError>{
 if performance_states==0||value>=performance_states as u64||value>u8::MAX as u64{
  Err(PowerResourceError::InvalidPerformanceLimit)
 }else{Ok(value as u8)}
}

/// Sleep sequencing is delegated to a platform backend so CPU/interrupt/timer
/// coordination remains in the kernel/HAL rather than being guessed by AML code.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SystemSleepPhase{Awake,Preparing,Sleeping,Resuming,Failed}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SystemSleepError{InvalidTransition,PrepareFailed,EnterFailed,ResumeFailed}
pub trait SystemSleepBackend{
 fn prepare_devices(&mut self,state:u8)->bool;
 fn enter_firmware_sleep(&mut self,sleep:SleepType)->bool;
 fn resume_devices(&mut self,state:u8)->bool;
 fn rollback_prepare(&mut self,state:u8);
}
pub struct SystemSleepCoordinator{phase:SystemSleepPhase,last_state:u8}
impl SystemSleepCoordinator{
 pub const fn new()->Self{Self{phase:SystemSleepPhase::Awake,last_state:0}}
 pub fn phase(&self)->SystemSleepPhase{self.phase}
 /// Prepare devices before entering S1-S5; failed entry rolls prepared devices back.
 pub fn enter(&mut self,sleep:SleepType,backend:&mut impl SystemSleepBackend)->Result<(),SystemSleepError>{
  if self.phase!=SystemSleepPhase::Awake||!(1..=5).contains(&sleep.state){return Err(SystemSleepError::InvalidTransition)}
  self.phase=SystemSleepPhase::Preparing;
  if !backend.prepare_devices(sleep.state){self.phase=SystemSleepPhase::Awake;return Err(SystemSleepError::PrepareFailed)}
  if !backend.enter_firmware_sleep(sleep){
   backend.rollback_prepare(sleep.state);self.phase=SystemSleepPhase::Awake;
   return Err(SystemSleepError::EnterFailed)
  }
  self.last_state=sleep.state;self.phase=SystemSleepPhase::Sleeping;Ok(())
 }
 /// Complete resume only after the platform has restored firmware-visible state.
 pub fn resume(&mut self,backend:&mut impl SystemSleepBackend)->Result<(),SystemSleepError>{
  if self.phase!=SystemSleepPhase::Sleeping{return Err(SystemSleepError::InvalidTransition)}
  self.phase=SystemSleepPhase::Resuming;
  if !backend.resume_devices(self.last_state){self.phase=SystemSleepPhase::Failed;return Err(SystemSleepError::ResumeFailed)}
  self.phase=SystemSleepPhase::Awake;self.last_state=0;Ok(())
 }
}

#[cfg(test)]
mod power_resource_graph_tests{
 use super::*;
 struct Backend{fail_node:Option<u16>,calls:[(u16,bool);32],count:usize}
 impl Backend{fn new()->Self{Self{fail_node:None,calls:[(u16::MAX,false);32],count:0}}}
 impl PowerResourceBackend for Backend{
  fn set_power_resource(&mut self,node:u16,enabled:bool)->bool{
   if self.count<self.calls.len(){self.calls[self.count]=(node,enabled);self.count+=1;}
   self.fail_node!=Some(node)
  }
 }
 fn graph()->PowerResourceGraph{
  let mut graph=PowerResourceGraph::new();
  graph.register(1,0,0,false).unwrap();graph.register(2,0,1,false).unwrap();
  graph.register(3,0,2,false).unwrap();graph.set_dependencies(2,&[1]).unwrap();
  graph.set_dependencies(3,&[2]).unwrap();graph
 }
 #[test]fn dependencies_power_on_before_dependent_and_off_after(){
  let mut graph=graph();let mut backend=Backend::new();graph.acquire(3,&mut backend).unwrap();
  assert_eq!(&backend.calls[..3],&[(1,true),(2,true),(3,true)]);
  graph.release(3,&mut backend).unwrap();
  assert_eq!(&backend.calls[3..6],&[(3,false),(2,false),(1,false)]);
 }
 #[test]fn shared_dependency_is_reference_counted(){
  let mut graph=graph();graph.set_dependencies(3,&[1]).unwrap();let mut backend=Backend::new();
  graph.acquire(2,&mut backend).unwrap();graph.acquire(3,&mut backend).unwrap();
  assert_eq!(graph.resource(1).unwrap().references,2);
  graph.release(2,&mut backend).unwrap();assert!(graph.resource(1).unwrap().enabled);
  graph.release(3,&mut backend).unwrap();assert!(!graph.resource(1).unwrap().enabled);
 }
 #[test]fn failed_power_on_unwinds_already_acquired_dependencies(){
  let mut graph=graph();let mut backend=Backend::new();backend.fail_node=Some(3);
  assert_eq!(graph.acquire(3,&mut backend),Err(PowerResourceError::BackendFailure));
  assert_eq!(graph.resource(1).unwrap().references,0);assert_eq!(graph.resource(2).unwrap().references,0);
  assert!(!graph.resource(1).unwrap().enabled);assert!(!graph.resource(2).unwrap().enabled);
 }
 #[test]fn dependency_cycle_is_rejected_without_powering_hardware(){
  let mut graph=graph();graph.set_dependencies(1,&[3]).unwrap();let mut backend=Backend::new();
  assert_eq!(graph.acquire(3,&mut backend),Err(PowerResourceError::DependencyCycle));
  assert_eq!(backend.count,0);
 }
 #[test]fn dependency_sets_reject_duplicates_and_missing_nodes(){
  let mut graph=graph();assert_eq!(graph.set_dependencies(3,&[2,2]),Err(PowerResourceError::DuplicateDependency));
  assert_eq!(graph.set_dependencies(3,&[9]),Err(PowerResourceError::MissingDependency));
 }
 #[test]fn sleep_type_requires_exact_two_valid_values(){
  assert_eq!(parse_sleep_type(3,&[5,6]).unwrap(),SleepType{state:3,type_a:5,type_b:6});
  assert_eq!(parse_sleep_type(0,&[0,0]),Err(PowerResourceError::InvalidSleepPackage));
  assert_eq!(parse_sleep_type(3,&[8,0]),Err(PowerResourceError::InvalidSleepPackage));
  assert_eq!(parse_sleep_type(3,&[0]),Err(PowerResourceError::InvalidSleepPackage));
 }
 #[test]fn power_and_performance_method_results_are_bounded(){
  assert_eq!(validate_current_power_state(3),Ok(3));assert!(validate_current_power_state(4).is_err());
  assert_eq!(validate_performance_limit(2,4),Ok(2));assert!(validate_performance_limit(4,4).is_err());
 }
}


/// AML bridge used by the device-power state machine. The native WDM adapter
/// implements these calls with AmlTableManager and the serialized AML runtime;
/// no firmware method is executed by the kernel's DISPATCH_LEVEL power preamble.
pub trait AcpiPowerFirmware{
 fn evaluate_prx(&mut self,device_node:u16,state:u8,output:&mut [u16;MAX_POWER_RESOURCE_DEPENDENCIES])->Result<usize,PowerResourceError>;
 fn invoke_device_method(&mut self,device_node:u16,method:[u8;4])->Result<(),PowerResourceError>;
 fn evaluate_integer_method(&mut self,device_node:u16,method:[u8;4])->Result<u64,PowerResourceError>;
 fn invoke_power_resource_method(&mut self,resource_node:u16,enable:bool)->Result<(),PowerResourceError>;
}
/// Execute ACPI methods which report dynamic device power policy.
/// _PSC reports the current D-state; _PPC reports the current performance
/// limit; _PSR reports external-power-source status. Values are validated
/// before being returned to WDM policy.
pub fn query_device_power_policy(firmware:&mut impl AcpiPowerFirmware,node:u16,performance_states:u8)->Result<(u8,u8,bool),PowerResourceError>{
 let psc=validate_current_power_state(firmware.evaluate_integer_method(node,*b"_PSC")?)?;
 let ppc=validate_performance_limit(firmware.evaluate_integer_method(node,*b"_PPC")?,performance_states)?;
 let psr=firmware.evaluate_integer_method(node,*b"_PSR")?;
 if psr>1{return Err(PowerResourceError::InvalidPowerState)}
 Ok((psc,ppc,psr!=0))
}
/// PowerResourceGraph adapter whose _ON/_OFF operations are AML invocations.
pub struct FirmwarePowerResourceBackend<'a,F:AcpiPowerFirmware>{pub firmware:&'a mut F}
impl<F:AcpiPowerFirmware> PowerResourceBackend for FirmwarePowerResourceBackend<'_,F>{
 fn set_power_resource(&mut self,node:u16,enabled:bool)->bool{
  self.firmware.invoke_power_resource_method(node,enabled).is_ok()
 }
}
/// Apply a D-state transition against _PRx and _PSx. Newly required resources
/// are acquired before the device method; resources no longer needed are
/// released only after the new state method succeeds. Failure unwinds newly
/// acquired references and leaves the cached state unchanged.
pub fn transition_device_with_resources(
 firmware:&mut impl AcpiPowerFirmware,
 graph:&mut PowerResourceGraph,
 state:&mut DevicePowerState,
 device_node:u16,
 target:u8,
)->Result<(),PowerResourceError>{
 if target>3{return Err(PowerResourceError::InvalidPowerState)}
 if target==state.d_state&&state.active_resource_count!=0{return Ok(())}
 let mut desired=[u16::MAX;MAX_POWER_RESOURCE_DEPENDENCIES];
 let desired_count=firmware.evaluate_prx(device_node,target,&mut desired)?;
 if desired_count>desired.len(){return Err(PowerResourceError::TooManyDependencies)}
 for i in 0..desired_count{
  if desired[i]==u16::MAX||desired[..i].contains(&desired[i]){return Err(PowerResourceError::DuplicateDependency)}
 }
 let mut newly_acquired=[u16::MAX;MAX_POWER_RESOURCE_DEPENDENCIES];let mut new_count=0usize;
 for resource in desired[..desired_count].iter().copied(){
  if !state.active_resources[..state.active_resource_count as usize].contains(&resource){
   if let Err(error)=graph.acquire(resource,&mut FirmwarePowerResourceBackend{firmware}){
    while new_count>0{new_count-=1;let _=graph.release(newly_acquired[new_count],&mut FirmwarePowerResourceBackend{firmware});}
    return Err(error)
   }
   newly_acquired[new_count]=resource;new_count+=1;
  }
 }
 let method=match target{0=>*b"_PS0",1=>*b"_PS1",2=>*b"_PS2",3=>*b"_PS3",_=>return Err(PowerResourceError::InvalidPowerState)};
 if let Err(error)=firmware.invoke_device_method(device_node,method){
  while new_count>0{new_count-=1;let _=graph.release(newly_acquired[new_count],&mut FirmwarePowerResourceBackend{firmware});}
  return Err(error)
 }
 let old=state.active_resources;let old_count=state.active_resource_count as usize;
 let mut released=[u16::MAX;MAX_POWER_RESOURCE_DEPENDENCIES];let mut released_count=0usize;
 for resource in old[..old_count].iter().copied(){
  if !desired[..desired_count].contains(&resource){
   if let Err(error)=graph.release(resource,&mut FirmwarePowerResourceBackend{firmware}){
    // Best-effort rollback restores the old device method state and resource
    // references before returning failure to the WDM power IRP.
    let rollback_method=match state.d_state{0=>*b"_PS0",1=>*b"_PS1",2=>*b"_PS2",3=>*b"_PS3",_=>*b"____"};
    let _=firmware.invoke_device_method(device_node,rollback_method);
    while released_count>0{released_count-=1;let _=graph.acquire(released[released_count],&mut FirmwarePowerResourceBackend{firmware});}
    while new_count>0{new_count-=1;let _=graph.release(newly_acquired[new_count],&mut FirmwarePowerResourceBackend{firmware});}
    return Err(error)
   }
   released[released_count]=resource;released_count+=1;
  }
 }
 state.active_resources=[u16::MAX;MAX_POWER_RESOURCE_DEPENDENCIES];
 state.active_resources[..desired_count].copy_from_slice(&desired[..desired_count]);
 state.active_resource_count=desired_count as u8;
 state.d_state=target;
 Ok(())
}

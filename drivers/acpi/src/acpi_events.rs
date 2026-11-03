//! ACPI fixed-hardware power registers, GPEs, wake, EC query and thermal events.
//! Hardware access and AML invocation are injected by the WDM/HAL adapter. No AML
//! method is ever executed in the SCI interrupt path.
#![no_std]

use crate::{AcpiGas,FadtInfo};

pub const MAX_GPE_EVENTS:usize=128;
pub const MAX_DEFERRED_EVENTS:usize=128;
pub const MAX_EC_QUERIES:usize=32;
pub const MAX_THERMAL_ZONES:usize=16;
pub const ACPI_GPE_EDGE:u8=0;
pub const ACPI_GPE_LEVEL:u8=1;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EventError{InvalidFadt,UnsupportedHardwareModel,InvalidGas,InvalidEvent,Capacity,QueueFull,IoFailure,MethodFailure,InvalidWake,InvalidThermalData,InvalidSleepState}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AcpiHardwareModel{FixedHardware,HardwareReduced}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AcpiPlatformRegisters{
 pub model:AcpiHardwareModel,
 pub pm1a_control:Option<AcpiGas>,pub pm1b_control:Option<AcpiGas>,
 pub sleep_control:Option<AcpiGas>,pub sleep_status:Option<AcpiGas>,
 pub gpe0:Option<(AcpiGas,u8,u8)>,pub gpe1:Option<(AcpiGas,u8,u8)>,
}
pub fn platform_registers(fadt:&FadtInfo)->Result<AcpiPlatformRegisters,EventError>{
 let model=if fadt.hardware_reduced(){AcpiHardwareModel::HardwareReduced}else{AcpiHardwareModel::FixedHardware};
 let pm1a=fadt.x_pm1a_control_block.filter(|g|g.address!=0).or_else(||(fadt.pm1a_control_legacy!=0).then_some(AcpiGas{address_space_id:1,bit_width:fadt.pm1_control_length.saturating_mul(8),bit_offset:0,access_size:1,address:fadt.pm1a_control_legacy as u64}));
 let pm1b=fadt.x_pm1b_control_block.filter(|g|g.address!=0).or_else(||(fadt.pm1b_control_legacy!=0).then_some(AcpiGas{address_space_id:1,bit_width:fadt.pm1_control_length.saturating_mul(8),bit_offset:0,access_size:1,address:fadt.pm1b_control_legacy as u64}));
 let sleep_control=fadt.sleep_control_register.filter(|g|g.address!=0);
 let sleep_status=fadt.sleep_status_register.filter(|g|g.address!=0);
 if model==AcpiHardwareModel::HardwareReduced{
  if sleep_control.is_none(){return Err(EventError::InvalidFadt)}
  // GPE0/GPE1 blocks are not part of the hardware-reduced programming model.
  return Ok(AcpiPlatformRegisters{model,pm1a_control:None,pm1b_control:None,sleep_control,sleep_status,gpe0:None,gpe1:None})
 }
 if pm1a.is_none()||fadt.pm1_control_length<2{return Err(EventError::InvalidFadt)}
 Ok(AcpiPlatformRegisters{model,pm1a_control:pm1a,pm1b_control:pm1b,sleep_control,sleep_status,
  gpe0:fadt.effective_gpe0().map(|(g,l)|(g,l,0)),gpe1:fadt.effective_gpe1().map(|(g,l,b)|(g,l,b))})
}

/// GAS access contract. Implementations must honor address_space_id and access_size;
/// SystemMemory and SystemIO are not interchangeable and FunctionalFixedHardware
/// requires a platform-specific backend.
pub trait GasIo{
 fn read(&mut self,gas:AcpiGas,width:u8)->Result<u64,EventError>;
 fn write(&mut self,gas:AcpiGas,width:u8,value:u64)->Result<(),EventError>;
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SleepRegisterWrite{pub gas:AcpiGas,pub width:u8,pub value:u64}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SleepRegisterPlan{pub writes:[Option<SleepRegisterWrite>;2],pub count:u8,pub state:u8}
/// Construct (but do not execute) a spec-shaped S1-S5 register write sequence.
/// Traditional ACPI writes SLP_TYP into PM1 control and then sets SLP_EN;
/// hardware-reduced ACPI writes SLP_TYP/SLP_EN to SLEEP_CONTROL_REG.
pub fn build_sleep_register_plan(registers:&AcpiPlatformRegisters,state:u8,type_a:u8,type_b:u8)->Result<SleepRegisterPlan,EventError>{
 if !(1..=5).contains(&state)||type_a>7||type_b>7{return Err(EventError::InvalidSleepState)}
 let mut plan=SleepRegisterPlan{writes:[None,None],count:0,state};
 match registers.model{
  AcpiHardwareModel::HardwareReduced=>{
   let gas=registers.sleep_control.ok_or(EventError::InvalidFadt)?;
   // HW-reduced SLEEP_CONTROL: SLP_TYP[4:2], SLP_EN[5].
   plan.writes[0]=Some(SleepRegisterWrite{gas,width:1,value:((type_a as u64)<<2)|(1<<5)});plan.count=1;
  }
  AcpiHardwareModel::FixedHardware=>{
   let a=registers.pm1a_control.ok_or(EventError::InvalidFadt)?;
   // PM1 control: SLP_TYP[12:10], SLP_EN[13]. Preserve unrelated bits.
   plan.writes[0]=Some(SleepRegisterWrite{gas:a,width:2,value:((type_a as u64)<<10)|(1<<13)});plan.count=1;
   if let Some(b)=registers.pm1b_control{
    plan.writes[1]=Some(SleepRegisterWrite{gas:b,width:2,value:((type_b as u64)<<10)|(1<<13)});plan.count=2;
   }
  }
 }
 Ok(plan)
}
pub fn execute_sleep_register_plan(io:&mut impl GasIo,plan:&SleepRegisterPlan)->Result<(),EventError>{
 for write in plan.writes.iter().take(plan.count as usize).flatten(){
  io.write(write.gas,write.width,write.value)?;
 }
 Ok(())
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct GpeRegisterBlock{pub gas:AcpiGas,pub length:u8,pub event_base:u8}
impl GpeRegisterBlock{
 pub fn event_count(&self)->Result<u16,EventError>{
  if self.length<2||self.length%2!=0||self.gas.address==0{return Err(EventError::InvalidGas)}
  Ok((self.length as u16/2)*8)
 }
 fn register_gas(&self,enable:bool,index:u16)->AcpiGas{
  let half=(self.length/2) as u64;
  AcpiGas{address:self.gas.address+(if enable{half}else{0})+index as u64,
   address_space_id:self.gas.address_space_id,bit_width:8,bit_offset:0,access_size:1}
 }
 fn bit(&self,event:u16)->Result<(u16,u8),EventError>{
  if event>=self.event_count()?{return Err(EventError::InvalidEvent)}
  Ok((event/8,1u8<<(event%8)))
 }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct GpeEvent{pub number:u16,pub method_node:u16,pub trigger:u8,pub wake_capable:bool,pub enabled:bool}
impl GpeEvent{pub const EMPTY:Self=Self{number:u16::MAX,method_node:u16::MAX,trigger:ACPI_GPE_EDGE,wake_capable:false,enabled:false};}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct DeferredGpe{pub number:u16,pub method_node:u16,pub trigger:u8}
impl DeferredGpe{pub const EMPTY:Self=Self{number:u16::MAX,method_node:u16::MAX,trigger:0};}
pub trait GpeIo:GasIo{
 /// Write-one-to-clear a GPE status bit. Implementations must issue a byte write.
 fn clear_status_w1c(&mut self,gas:AcpiGas,mask:u8)->Result<(),EventError>{self.write(gas,1,mask as u64)}
}
pub trait GpeMethodExecutor{
 /// Execute the associated _Lxx or _Exx AML method at PASSIVE_LEVEL.
 fn execute_gpe_method(&mut self,method_node:u16)->Result<(),EventError>;
}
pub struct GpeController{
 blocks:[Option<GpeRegisterBlock>;2],events:[GpeEvent;MAX_GPE_EVENTS],event_count:usize,
 queue:[DeferredGpe;MAX_DEFERRED_EVENTS],head:usize,tail:usize,queued:usize,
}
impl GpeController{
 pub const fn new()->Self{Self{blocks:[None,None],events:[GpeEvent::EMPTY;MAX_GPE_EVENTS],event_count:0,
  queue:[DeferredGpe::EMPTY;MAX_DEFERRED_EVENTS],head:0,tail:0,queued:0}}
 pub fn configure_block(&mut self,slot:usize,block:GpeRegisterBlock)->Result<(),EventError>{
  if slot>=2||block.event_count()?==0{return Err(EventError::InvalidGas)}self.blocks[slot]=Some(block);Ok(())
 }
 pub fn register_event(&mut self,event:GpeEvent)->Result<(),EventError>{
  if event.trigger>ACPI_GPE_LEVEL||event.method_node==u16::MAX{return Err(EventError::InvalidEvent)}
  if self.event_count==MAX_GPE_EVENTS{return Err(EventError::Capacity)}
  if self.events[..self.event_count].iter().any(|e|e.number==event.number){return Err(EventError::InvalidEvent)}
  let found=self.blocks.iter().flatten().any(|b|{
   let base=b.event_base as u16;event.number>=base&&event.number<base+b.event_count().unwrap_or(0)
  });
  if !found{return Err(EventError::InvalidEvent)}
  self.events[self.event_count]=event;self.event_count+=1;Ok(())
 }
 fn locate(&self,event:u16)->Result<(GpeRegisterBlock,u16,u8),EventError>{
  let block=self.blocks.iter().flatten().find(|b|{
   let base=b.event_base as u16;event>=base&&event<base+b.event_count().unwrap_or(0)
  }).copied().ok_or(EventError::InvalidEvent)?;
  let local=event-block.event_base as u16;let (index,mask)=block.bit(local)?;Ok((block,index,mask))
 }
 pub fn set_enabled(&mut self,io:&mut impl GpeIo,event:u16,enabled:bool)->Result<(),EventError>{
  let (block,index,mask)=self.locate(event)?;
  let gas=block.register_gas(true,index);
  let current=io.read(gas,1)? as u8;
  io.write(gas,1,if enabled{current|mask}else{current&!mask} as u64)?;
  if let Some(entry)=self.events[..self.event_count].iter_mut().find(|e|e.number==event){entry.enabled=enabled;}
  Ok(())
 }
 /// SCI top-half: snapshot enabled status, mask each event, clear status W1C,
 /// and enqueue only. It never enters AML or blocks on a method mutex.
 pub fn capture_sci(&mut self,io:&mut impl GpeIo)->Result<usize,EventError>{
  let mut captured=0usize;
  for slot in 0..2{
   let Some(block)=self.blocks[slot] else{continue};
   let bytes=(block.length/2) as u16;
   for index in 0..bytes{
    let status=io.read(block.register_gas(false,index),1)? as u8;
    let enable_gas=block.register_gas(true,index);
    let enabled=io.read(enable_gas,1)? as u8;
    let mut pending=status&enabled;
    while pending!=0{
     let bit=pending.trailing_zeros() as u8;let mask=1u8<<bit;pending&=!mask;
     let event=block.event_base as u16+index*8+bit as u16;
     let Some(registration)=self.events[..self.event_count].iter().find(|e|e.number==event).copied() else{
      io.write(enable_gas,1,(enabled&!mask) as u64)?;
      io.clear_status_w1c(block.register_gas(false,index),mask)?;
      continue
     };
     if self.queued==MAX_DEFERRED_EVENTS{return Err(EventError::QueueFull)}
     io.write(enable_gas,1,(enabled&!mask) as u64)?;
     io.clear_status_w1c(block.register_gas(false,index),mask)?;
     self.queue[self.tail]=DeferredGpe{number:event,method_node:registration.method_node,trigger:registration.trigger};
     self.tail=(self.tail+1)%MAX_DEFERRED_EVENTS;self.queued+=1;captured+=1;
     if let Some(entry)=self.events[..self.event_count].iter_mut().find(|e|e.number==event){entry.enabled=false;}
    }
   }
  }
  Ok(captured)
 }
 /// Drain at PASSIVE_LEVEL. Level-triggered events are re-enabled only after
 /// successful AML completion; edge events are likewise re-enabled afterwards.
 pub fn dispatch_deferred(&mut self,io:&mut impl GpeIo,executor:&mut impl GpeMethodExecutor,budget:usize)->Result<usize,EventError>{
  let mut completed=0usize;
  while completed<budget&&self.queued!=0{
   let event=self.queue[self.head];self.queue[self.head]=DeferredGpe::EMPTY;
   self.head=(self.head+1)%MAX_DEFERRED_EVENTS;self.queued-=1;
   executor.execute_gpe_method(event.method_node)?;
   self.set_enabled(io,event.number,true)?;
   completed+=1;
  }
  Ok(completed)
 }
 pub fn pending(&self)->usize{self.queued}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct WakeConfiguration{pub gpe:u16,pub deepest_system_state:u8,pub device_sleep_state:u8,pub armed:bool}
pub trait WakePlatform{
 fn enable_wake_interrupt(&mut self,gpe:u16,enable:bool)->Result<(),EventError>;
 fn set_wake_power_resources(&mut self,device_node:u16,enable:bool)->Result<(),EventError>;
 fn invoke_wake_method(&mut self,device_node:u16,enable:bool,system_state:u8,device_state:u8)->Result<(),EventError>;
}
pub fn configure_device_wake(platform:&mut impl WakePlatform,device_node:u16,config:&mut WakeConfiguration,
 enable:bool,system_state:u8,device_state:u8)->Result<(),EventError>{
 if device_node==u16::MAX||config.gpe==u16::MAX||!(1..=5).contains(&config.deepest_system_state)
  ||!(1..=5).contains(&system_state)||system_state>config.deepest_system_state||device_state>3{return Err(EventError::InvalidWake)}
 if enable{
  platform.set_wake_power_resources(device_node,true)?;
  platform.invoke_wake_method(device_node,true,system_state,device_state)?; // _DSW preferred, _PSW legacy
  platform.enable_wake_interrupt(config.gpe,true)?;
  config.armed=true;config.device_sleep_state=device_state;
 }else{
  platform.enable_wake_interrupt(config.gpe,false)?;
  platform.invoke_wake_method(device_node,false,system_state,device_state)?;
  platform.set_wake_power_resources(device_node,false)?;
  config.armed=false;
 }
 Ok(())
}

pub trait EmbeddedControllerBackend{
 fn read_query_value(&mut self)->Result<u8,EventError>;
 fn execute_query_method(&mut self,method_node:u16,query:u8)->Result<(),EventError>;
}
pub struct EmbeddedControllerEvents{queue:[u8;MAX_EC_QUERIES],head:usize,tail:usize,count:usize}
impl EmbeddedControllerEvents{
 pub const fn new()->Self{Self{queue:[0;MAX_EC_QUERIES],head:0,tail:0,count:0}}
 /// SCI/EC GPE deferred worker reads one query byte and queues it; AML _Qxx
 /// methods are executed only by dispatch_deferred.
 pub fn capture_query(&mut self,ec:&mut impl EmbeddedControllerBackend)->Result<(),EventError>{
  if self.count==MAX_EC_QUERIES{return Err(EventError::QueueFull)}
  let value=ec.read_query_value()?;self.queue[self.tail]=value;self.tail=(self.tail+1)%MAX_EC_QUERIES;self.count+=1;Ok(())
 }
 pub fn dispatch_deferred(&mut self,ec:&mut impl EmbeddedControllerBackend,namespace:&[u16;256],budget:usize)->Result<usize,EventError>{
  let mut n=0;
  while n<budget&&self.count!=0{
   let query=self.queue[self.head];self.head=(self.head+1)%MAX_EC_QUERIES;self.count-=1;
   let node=namespace[query as usize];if node==u16::MAX{return Err(EventError::InvalidEvent)}
   ec.execute_query_method(node,query)?;n+=1;
  }
  Ok(n)
 }
 pub fn pending(&self)->usize{self.count}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ThermalZone{pub node:u16,pub temperature_deci_kelvin:u32,pub passive_trip:Option<u32>,pub hot_trip:Option<u32>,pub critical_trip:Option<u32>,pub active_trip:[Option<u32>;8],pub active_count:u8}
impl ThermalZone{pub const EMPTY:Self=Self{node:u16::MAX,temperature_deci_kelvin:0,passive_trip:None,hot_trip:None,critical_trip:None,active_trip:[None;8],active_count:0};}
pub trait ThermalPolicyBackend{
 fn set_passive_cooling(&mut self,zone:u16,active:bool)->Result<(),EventError>;
 fn set_active_cooling(&mut self,zone:u16,trip:u8,active:bool)->Result<(),EventError>;
 fn notify_critical_temperature(&mut self,zone:u16)->Result<(),EventError>;
 fn notify_hot_temperature(&mut self,zone:u16)->Result<(),EventError>;
}
/// Consume _TMP and trip-point values (all in 0.1 Kelvin) and apply hysteresis-free
/// ACPI trip policy. Platform policy may layer hysteresis and user thermal limits above.
pub fn update_thermal_zone(zone:&mut ThermalZone,temp:u32,backend:&mut impl ThermalPolicyBackend)->Result<(),EventError>{
 if zone.node==u16::MAX||temp<1000||temp>20000||zone.active_count as usize>zone.active_trip.len()
  ||[zone.passive_trip,zone.hot_trip,zone.critical_trip].iter().flatten().any(|v|*v<1000||*v>20000){return Err(EventError::InvalidThermalData)}
 zone.temperature_deci_kelvin=temp;
 if let Some(trip)=zone.passive_trip{backend.set_passive_cooling(zone.node,temp>=trip)?;}
 for i in 0..zone.active_count as usize{if let Some(trip)=zone.active_trip[i]{if trip<1000||trip>20000{return Err(EventError::InvalidThermalData)}backend.set_active_cooling(zone.node,i as u8,temp>=trip)?;}}
 if zone.hot_trip.is_some_and(|trip|temp>=trip){backend.notify_hot_temperature(zone.node)?;}
 if zone.critical_trip.is_some_and(|trip|temp>=trip){backend.notify_critical_temperature(zone.node)?;}
 Ok(())
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests{
 use super::*;
 fn gas(address:u64)->AcpiGas{AcpiGas{address_space_id:1,bit_width:8,bit_offset:0,access_size:1,address}}
 #[test]fn hardware_reduced_ignores_legacy_gpe_and_requires_sleep_control(){
  let mut f=FadtInfo::default();f.flags=FadtInfo::HARDWARE_REDUCED_ACPI;f.sleep_control_register=Some(gas(0x100));
  let r=platform_registers(&f).unwrap();assert_eq!(r.model,AcpiHardwareModel::HardwareReduced);assert!(r.gpe0.is_none());
  let p=build_sleep_register_plan(&r,3,5,0).unwrap();assert_eq!(p.writes[0].unwrap().value,(5<<2)|(1<<5));
 }
 #[test]fn fixed_sleep_plan_sets_both_pm1_blocks(){
  let r=AcpiPlatformRegisters{model:AcpiHardwareModel::FixedHardware,pm1a_control:Some(gas(0x404)),pm1b_control:Some(gas(0x408)),sleep_control:None,sleep_status:None,gpe0:None,gpe1:None};
  let p=build_sleep_register_plan(&r,3,5,6).unwrap();assert_eq!(p.count,2);assert_eq!(p.writes[1].unwrap().value,(6<<10)|(1<<13));
 }
 struct Io{regs:[u8;32],writes:usize}
 impl GasIo for Io{fn read(&mut self,g:AcpiGas,_:u8)->Result<u64,EventError>{Ok(self.regs[g.address as usize] as u64)}fn write(&mut self,g:AcpiGas,_:u8,v:u64)->Result<(),EventError>{self.regs[g.address as usize]=v as u8;self.writes+=1;Ok(())}}
 impl GpeIo for Io{}
 struct Exec{calls:usize}impl GpeMethodExecutor for Exec{fn execute_gpe_method(&mut self,_:u16)->Result<(),EventError>{self.calls+=1;Ok(())}}
 #[test]fn gpe_capture_masks_and_clears_then_deferred_exec_reenables(){
  let mut g=GpeController::new();g.configure_block(0,GpeRegisterBlock{gas:gas(4),length:4,event_base:0}).unwrap();
  g.register_event(GpeEvent{number:1,method_node:77,trigger:ACPI_GPE_LEVEL,wake_capable:true,enabled:true}).unwrap();
  let mut io=Io{regs:[0;32],writes:0};io.regs[4]=2;io.regs[6]=2;
  assert_eq!(g.capture_sci(&mut io).unwrap(),1);assert_eq!(g.pending(),1);assert_eq!(io.regs[4],0);assert_eq!(io.regs[6],0);
  let mut exec=Exec{calls:0};assert_eq!(g.dispatch_deferred(&mut io,&mut exec,4).unwrap(),1);
  assert_eq!(exec.calls,1);assert_eq!(io.regs[6],2);
 }
 struct Ec{query:u8,called:u8}impl EmbeddedControllerBackend for Ec{fn read_query_value(&mut self)->Result<u8,EventError>{Ok(self.query)}fn execute_query_method(&mut self,_:u16,q:u8)->Result<(),EventError>{self.called=q;Ok(())}}
 #[test]fn ec_queries_are_deferred(){let mut ec=Ec{query:0x42,called:0};let mut q=EmbeddedControllerEvents::new();q.capture_query(&mut ec).unwrap();let mut map=[u16::MAX;256];map[0x42]=12;q.dispatch_deferred(&mut ec,&map,1).unwrap();assert_eq!(ec.called,0x42);}
 struct Thermal{passive:bool,active:bool,critical:bool}impl ThermalPolicyBackend for Thermal{fn set_passive_cooling(&mut self,_:u16,a:bool)->Result<(),EventError>{self.passive=a;Ok(())}fn set_active_cooling(&mut self,_:u16,_:u8,a:bool)->Result<(),EventError>{self.active=a;Ok(())}fn notify_critical_temperature(&mut self,_:u16)->Result<(),EventError>{self.critical=true;Ok(())}fn notify_hot_temperature(&mut self,_:u16)->Result<(),EventError>{Ok(())}}
 #[test]fn thermal_trip_updates_cooling_and_critical_notification(){let mut z=ThermalZone{node:2,passive_trip:Some(3000),critical_trip:Some(3300),active_trip:[Some(3100),None,None,None,None,None,None,None],active_count:1,..ThermalZone::EMPTY};let mut b=Thermal{passive:false,active:false,critical:false};update_thermal_zone(&mut z,3400,&mut b).unwrap();assert!(b.passive&&b.active&&b.critical);}
}

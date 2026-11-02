//! PCI IDE controller resource decoding for the native pciide/ataport stack.
//!
//! Native-mode channels MUST use their assigned PCI BARs. Legacy compatibility
//! addresses are exposed only for channels whose programming-interface bit says
//! that channel is in compatibility mode. BAR4 is optional and is never treated
//! as proof that bus mastering/DMA is enabled.
#![no_std]

use crate::resources::BarKind;
use crate::resource_manager::{AllocationPlan, BarAssignment};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdeResourceError { InvalidProgrammingInterface, MissingBar, WrongBarType, TooSmall, AddressOverflow, PortRangeOverflow }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IdeChannelPorts {
    pub command_base: u16,
    pub control_base: u16,
    pub native: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IdeControllerPorts {
    pub primary: IdeChannelPorts,
    pub secondary: IdeChannelPorts,
    pub bus_master_base: Option<u16>,
    pub bus_master_capable: bool,
}

fn find_bar(plan:&AllocationPlan,index:u8)->Option<BarAssignment>{
    plan.assignments[..plan.count].iter().flatten().find(|a|a.index==index).copied()
}
fn io_bar(plan:&AllocationPlan,index:u8,minimum:u64)->Result<u16,IdeResourceError>{
    let bar=find_bar(plan,index).ok_or(IdeResourceError::MissingBar)?;
    if bar.kind!=BarKind::Io{return Err(IdeResourceError::WrongBarType)}
    if bar.length<minimum{return Err(IdeResourceError::TooSmall)}
    let end=bar.base.checked_add(bar.length).ok_or(IdeResourceError::AddressOverflow)?;
    if end>0x1_0000||bar.base>u16::MAX as u64{return Err(IdeResourceError::PortRangeOverflow)}
    u16::try_from(bar.base).map_err(|_|IdeResourceError::PortRangeOverflow)
}

/// Resolve command/control port ranges from the PCI IDE programming interface
/// and the PnP-assigned BAR plan. Compatibility-mode channels use the standard
/// legacy ports; native-mode channels fail closed if their BARs are absent.
pub fn resolve_ide_ports(
    programming_interface:u8,
    plan:&AllocationPlan,
)->Result<IdeControllerPorts,IdeResourceError>{
    let primary_native=programming_interface&0x01!=0;
    let secondary_native=programming_interface&0x04!=0;
    let bus_master_capable=programming_interface&0x80!=0;
    // Bits 1 and 3 are reserved in the IDE programming interface.
    if programming_interface&0x0a!=0{return Err(IdeResourceError::InvalidProgrammingInterface)}
    let primary=if primary_native{
        IdeChannelPorts{command_base:io_bar(plan,0,8)?,control_base:io_bar(plan,1,4)?.checked_add(2).ok_or(IdeResourceError::AddressOverflow)?,native:true}
    }else{IdeChannelPorts{command_base:0x1f0,control_base:0x3f6,native:false}};
    let secondary=if secondary_native{
        IdeChannelPorts{command_base:io_bar(plan,2,8)?,control_base:io_bar(plan,3,4)?.checked_add(2).ok_or(IdeResourceError::AddressOverflow)?,native:true}
    }else{IdeChannelPorts{command_base:0x170,control_base:0x376,native:false}};
    let bus_master_base=if bus_master_capable{
        Some(io_bar(plan,4,16)?)
    }else{None};
    if let Some(base)=bus_master_base{
        base.checked_add(15).ok_or(IdeResourceError::AddressOverflow)?;
    }
    Ok(IdeControllerPorts{primary,secondary,bus_master_base,bus_master_capable})
}

#[cfg(test)]
mod tests{
    use super::*;
    fn assignment(index:u8,base:u64,length:u64)->Option<BarAssignment>{
        Some(BarAssignment{index,kind:BarKind::Io,base,length,prefetchable:false})
    }
    #[test]
    fn compatibility_mode_does_not_require_or_invent_native_bars(){
        let plan=AllocationPlan::empty();
        let ports=resolve_ide_ports(0,&plan).unwrap();
        assert_eq!(ports.primary.command_base,0x1f0);
        assert_eq!(ports.primary.control_base,0x3f6);
        assert!(!ports.primary.native);
        assert_eq!(ports.secondary.command_base,0x170);
    }
    #[test]
    fn native_channels_use_assigned_bars_and_busmaster_window(){
        let plan=AllocationPlan{assignments:[assignment(0,0x2000,8),assignment(1,0x2100,4),assignment(2,0x2200,8),assignment(3,0x2300,4),assignment(4,0x2400,16),None],count:5};
        let ports=resolve_ide_ports(0x85,&plan).unwrap();
        assert_eq!(ports.primary.command_base,0x2000);
        assert_eq!(ports.primary.control_base,0x2102);
        assert_eq!(ports.secondary.command_base,0x2200);
        assert_eq!(ports.secondary.control_base,0x2302);
        assert_eq!(ports.bus_master_base,Some(0x2400));
    }
    #[test]
    fn native_mode_fails_closed_without_valid_assigned_bar(){
        assert_eq!(resolve_ide_ports(0x01,&AllocationPlan::empty()),Err(IdeResourceError::MissingBar));
        let wrong=AllocationPlan{assignments:[Some(BarAssignment{index:0,kind:BarKind::Memory32,base:0x8000_0000,length:8,prefetchable:false}),None,None,None,None,None],count:1};
        assert_eq!(resolve_ide_ports(0x01,&wrong),Err(IdeResourceError::WrongBarType));
    }
    #[test]
    fn busmaster_window_is_not_assumed_when_controller_lacks_capability(){
        let plan=AllocationPlan::empty();
        let ports=resolve_ide_ports(0,&plan).unwrap();
        assert_eq!(ports.bus_master_base,None);
    }
}

//! Transactional PCI PnP resource manager primitives.
//!
//! Apertures must be firmware/bridge-provided windows already intersected
//! across the endpoint's complete bridge path. No host aperture is guessed.
//! The caller serializes configuration access and publishes PDO resources only
//! after programming succeeds. Bus mastering is never enabled here.
#![no_std]

use crate::resources::{BarKind, BarRequirement, ConfigAccess, ResourceError,
    PCI_BAR0, PCI_COMMAND, PCI_COMMAND_BUS_MASTER, PCI_COMMAND_IO, PCI_COMMAND_MEMORY};

pub const MAX_ASSIGNMENTS: usize = 6;
pub const IO_RESOURCE_DESCRIPTOR_SIZE: usize = 32;
pub const CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aperture { pub start:u64, pub length:u64, pub kind:BarKind, pub prefetchable:bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OccupiedRange { pub start:u64, pub length:u64, pub kind:BarKind }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarAssignment { pub index:u8, pub kind:BarKind, pub base:u64, pub length:u64, pub prefetchable:bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationPlan { pub assignments:[Option<BarAssignment>;MAX_ASSIGNMENTS], pub count:usize }
impl AllocationPlan { pub const fn empty()->Self { Self{assignments:[None;MAX_ASSIGNMENTS],count:0} } }

/// Plan all BAR allocations in scratch state; output is unchanged on failure.
/// Apertures represent the intersection of the host and every ancestor bridge
/// window for this function, not merely the endpoint's local bridge window.
pub fn allocate_all(
    requirements:&[Option<BarRequirement>;MAX_ASSIGNMENTS], count:usize,
    apertures:&[Aperture], occupied:&[OccupiedRange], output:&mut AllocationPlan,
)->Result<(),ResourceError>{
    if count>MAX_ASSIGNMENTS||apertures.is_empty(){return Err(ResourceError::NoWindow)}
    let mut plan=AllocationPlan::empty();
    let mut used_bars=0u8;
    for requirement in requirements.iter().take(count).flatten(){
        let index=requirement.index as usize;
        if index>=MAX_ASSIGNMENTS{return Err(ResourceError::InvalidBar)}
        let mut mask=1u8<<index;
        if requirement.kind==BarKind::Memory64 {
            if index+1>=MAX_ASSIGNMENTS{return Err(ResourceError::InvalidBar)}
            mask|=1u8<<(index+1);
        }
        if used_bars&mask!=0{return Err(ResourceError::InvalidBar)}
        used_bars|=mask;
        if requirement.length==0||requirement.alignment==0||!requirement.alignment.is_power_of_two(){return Err(ResourceError::InvalidAlignment)}
        let mut selected=None;
        for aperture in apertures.iter().copied(){
            if aperture.kind!=requirement.kind||(requirement.kind!=BarKind::Io&&aperture.prefetchable!=requirement.prefetchable){continue}
            let end=aperture.start.checked_add(aperture.length).ok_or(ResourceError::AddressOverflow)?;
            let mut candidate=aperture.start.checked_add(requirement.alignment-1).ok_or(ResourceError::AddressOverflow)?&!(requirement.alignment-1);
            loop{
                let candidate_end=candidate.checked_add(requirement.length).ok_or(ResourceError::AddressOverflow)?;
                if candidate_end>end||(requirement.kind==BarKind::Memory32&&candidate_end>(1u64<<32))||(requirement.kind==BarKind::Io&&candidate_end>0x1_0000){break}
                let mut conflict=None;
                for range in occupied.iter().copied().chain(plan.assignments[..plan.count].iter().flatten().map(|a:&BarAssignment|OccupiedRange{start:a.base,length:a.length,kind:a.kind})){
                    if range.kind!=requirement.kind{continue}
                    let range_end=range.start.checked_add(range.length).ok_or(ResourceError::AddressOverflow)?;
                    if candidate<range_end&&range.start<candidate_end{conflict=Some(conflict.map_or(range_end,|old:u64|old.max(range_end)))}
                }
                match conflict{
                    None=>{selected=Some(candidate);break}
                    Some(next)=>candidate=next.checked_add(requirement.alignment-1).ok_or(ResourceError::AddressOverflow)?&!(requirement.alignment-1),
                }
            }
            if selected.is_some(){break}
        }
        let base=selected.ok_or(ResourceError::NoSpace)?;
        plan.assignments[plan.count]=Some(BarAssignment{index:requirement.index,kind:requirement.kind,base,length:requirement.length,prefetchable:requirement.prefetchable});
        plan.count+=1;
    }
    if plan.count!=count{return Err(ResourceError::InvalidBar)}
    *output=plan;
    Ok(())
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AddressTranslation{pub bus_start:u64,pub system_start:u64,pub length:u64}
impl AddressTranslation{
    pub fn translate(self,start:u64,length:u64)->Result<u64,ResourceError>{
        if length==0{return Err(ResourceError::ZeroLength)}
        let end=start.checked_add(length).ok_or(ResourceError::AddressOverflow)?;
        let limit=self.bus_start.checked_add(self.length).ok_or(ResourceError::AddressOverflow)?;
        if start<self.bus_start||end>limit{return Err(ResourceError::NoWindow)}
        self.system_start.checked_add(start-self.bus_start).ok_or(ResourceError::AddressOverflow)
    }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PublishedResource{pub kind:BarKind,pub raw_start:u64,pub translated_start:u64,pub length:u32,pub prefetchable:bool}

/// Translate every assignment before publishing any descriptor.
pub fn translate_plan(plan:&AllocationPlan,windows:&[AddressTranslation],output:&mut[Option<PublishedResource>;MAX_ASSIGNMENTS])->Result<(),ResourceError>{
    if plan.count>MAX_ASSIGNMENTS{return Err(ResourceError::Capacity)}
    let mut staged=[None;MAX_ASSIGNMENTS];
    for(i,a)in plan.assignments[..plan.count].iter().flatten().enumerate(){
        let w=windows.iter().copied().find(|w|a.base>=w.bus_start&&a.base.checked_add(a.length).is_some_and(|e|w.bus_start.checked_add(w.length).is_some_and(|limit|e<=limit))).ok_or(ResourceError::NoWindow)?;
        staged[i]=Some(PublishedResource{kind:a.kind,raw_start:a.base,translated_start:w.translate(a.base,a.length)?,length:u32::try_from(a.length).map_err(|_|ResourceError::AddressOverflow)?,prefetchable:a.prefetchable});
    }
    *output=staged;Ok(())
}

/// Program every BAR with decode disabled. A failed write restores the saved
/// BAR image; rollback failure leaves decode and bus mastering disabled.
pub fn program_assignments<C:ConfigAccess>(config:&mut C,header_type:u8,plan:&AllocationPlan)->Result<(),ResourceError>{
    let bars=match header_type&0x7f{0=>6,1=>2,_=>return Err(ResourceError::UnsupportedHeader)};
    if plan.count>MAX_ASSIGNMENTS{return Err(ResourceError::Capacity)}
    let command=config.read_u16(PCI_COMMAND)?;
    let mut saved=[0u32;MAX_ASSIGNMENTS];let mut touched=[false;MAX_ASSIGNMENTS];
    let mut used_bars=0u8;
    for a in plan.assignments[..plan.count].iter().flatten(){
        let i=a.index as usize;if i>=bars{return Err(ResourceError::InvalidBar)}
        let mut mask=1u8<<i;
        if a.kind==BarKind::Memory64{if i+1>=bars{return Err(ResourceError::InvalidBar)}mask|=1u8<<(i+1)}
        if used_bars&mask!=0{return Err(ResourceError::InvalidBar)}used_bars|=mask;
    }
    let operation=(||{
        config.write_u16(PCI_COMMAND,command&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER))?;
        for i in 0..bars{saved[i]=config.read_u32(PCI_BAR0+i as u16*4)?;touched[i]=true}
        for a in plan.assignments[..plan.count].iter().flatten(){
            let i=a.index as usize;if i>=bars{return Err(ResourceError::InvalidBar)}
            let old=saved[i];
            let end=a.base.checked_add(a.length).ok_or(ResourceError::AddressOverflow)?;
            let low=match a.kind{
                BarKind::Io=>{if end>0x1_0000{return Err(ResourceError::AddressOverflow)}(a.base as u32&!3)|(old&3)},
                BarKind::Memory32=>{if end>(1u64<<32){return Err(ResourceError::AddressOverflow)}(a.base as u32&!0xf)|(old&0xf)},
                BarKind::Memory64=>(a.base as u32&!0xf)|(old&0xf),
            };
            config.write_u32(PCI_BAR0+i as u16*4,low)?;
            if a.kind==BarKind::Memory64{if i+1>=bars{return Err(ResourceError::InvalidBar)}config.write_u32(PCI_BAR0+(i as u16+1)*4,(a.base>>32)as u32)?}
        }
        Ok(())
    })();
    if let Err(error)=operation{
        let mut failed=false;for i in(0..bars).rev(){if touched[i]&&config.write_u32(PCI_BAR0+i as u16*4,saved[i]).is_err(){failed=true}}
        let safe=command&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER);
        if config.write_u16(PCI_COMMAND,if failed{safe}else{command&!PCI_COMMAND_BUS_MASTER}).is_err(){failed=true}
        return Err(if failed{ResourceError::Config}else{error})
    }
    let mut io=false;let mut memory=false;
    for a in plan.assignments[..plan.count].iter().flatten(){if a.kind==BarKind::Io{io=true}else{memory=true}}
    let mut next=command&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER);if io{next|=PCI_COMMAND_IO}if memory{next|=PCI_COMMAND_MEMORY}
    if config.write_u16(PCI_COMMAND,next).is_err(){
        let mut failed=false;for i in(0..bars).rev(){if touched[i]&&config.write_u32(PCI_BAR0+i as u16*4,saved[i]).is_err(){failed=true}}
        let safe=command&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER);
        if config.write_u16(PCI_COMMAND,if failed{safe}else{command&!PCI_COMMAND_BUS_MASTER}).is_err(){failed=true}
        return Err(ResourceError::Config)
    }
    Ok(())
}

/// One alternative set offered to the PnP arbiter.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct RequirementAlternative {
    pub requirements:[Option<BarRequirement>;MAX_ASSIGNMENTS],
    pub count:usize,
}

/// Encode all alternatives in one x64 IO_RESOURCE_REQUIREMENTS_LIST. The list
/// header is followed by one 8-byte IO_RESOURCE_LIST header and its descriptors
/// for each alternative. Address maxima are constrained by the BAR type.
pub fn encode_requirements_list(alternatives:&[RequirementAlternative],bus:u32,out:&mut[u8])->Result<usize,ResourceError>{
    if alternatives.is_empty()||alternatives.len()>16{return Err(ResourceError::Capacity)}
    let mut size=32usize;
    for alt in alternatives{
        if alt.count>MAX_ASSIGNMENTS{return Err(ResourceError::Capacity)}
        size=size.checked_add(8).and_then(|v|v.checked_add(alt.count.checked_mul(IO_RESOURCE_DESCRIPTOR_SIZE)?)).ok_or(ResourceError::AddressOverflow)?;
    }
    if out.len()<size{return Err(ResourceError::Capacity)}
    out[..size].fill(0);
    out[0..4].copy_from_slice(&(size as u32).to_le_bytes());
    out[4..8].copy_from_slice(&15i32.to_le_bytes()); // PNPBus
    out[8..12].copy_from_slice(&bus.to_le_bytes());
    let mut cursor=32usize;
    for alt in alternatives{
        out[cursor..cursor+2].copy_from_slice(&1u16.to_le_bytes());
        out[cursor+2..cursor+4].copy_from_slice(&1u16.to_le_bytes());
        out[cursor+4..cursor+8].copy_from_slice(&(alt.count as u32).to_le_bytes());
        cursor+=8;
        let mut emitted=0usize;
        for r in alt.requirements.iter().take(alt.count).flatten(){
            if r.length==0||r.alignment==0||!r.alignment.is_power_of_two(){return Err(ResourceError::InvalidAlignment)}
            let p=cursor+emitted*IO_RESOURCE_DESCRIPTOR_SIZE;
            out[p]=0;
            out[p+1]=if r.kind==BarKind::Io{1}else{3};
            out[p+2]=1;
            out[p+4..p+6].copy_from_slice(&(if r.kind==BarKind::Io{1u16}else if r.prefetchable{4}else{0}).to_le_bytes());
            out[p+8..p+12].copy_from_slice(&u32::try_from(r.length).map_err(|_|ResourceError::AddressOverflow)?.to_le_bytes());
            out[p+12..p+16].copy_from_slice(&u32::try_from(r.alignment).map_err(|_|ResourceError::AddressOverflow)?.to_le_bytes());
            let maximum=match r.kind{BarKind::Io=>0xffffu64,BarKind::Memory32=>0xffff_ffffu64,BarKind::Memory64=>u64::MAX};
            out[p+24..p+32].copy_from_slice(&maximum.to_le_bytes());
            emitted+=1;
        }
        if emitted!=alt.count{return Err(ResourceError::InvalidBar)}
        cursor+=emitted*IO_RESOURCE_DESCRIPTOR_SIZE;
    }
    Ok(size)
}

/// Convenience wrapper for the common single-alternative case.
pub fn encode_requirements_alternative(reqs:&[Option<BarRequirement>;MAX_ASSIGNMENTS],count:usize,bus:u32,out:&mut[u8])->Result<usize,ResourceError>{
    let alternative=RequirementAlternative{requirements:*reqs,count};
    encode_requirements_list(core::slice::from_ref(&alternative),bus,out)
}

/// Emit raw or translated CM_RESOURCE_LIST data for a single full descriptor.
pub fn encode_cm_resource_list(resources:&[Option<PublishedResource>;MAX_ASSIGNMENTS],count:usize,bus:u32,translated:bool,out:&mut[u8])->Result<usize,ResourceError>{
    let size=20usize.checked_add(count.checked_mul(CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE).ok_or(ResourceError::AddressOverflow)?).ok_or(ResourceError::AddressOverflow)?;
    if count>MAX_ASSIGNMENTS||out.len()<size{return Err(ResourceError::Capacity)}
    out[..size].fill(0);out[0..4].copy_from_slice(&1u32.to_le_bytes());out[4..8].copy_from_slice(&15i32.to_le_bytes());out[8..12].copy_from_slice(&bus.to_le_bytes());out[12..14].copy_from_slice(&1u16.to_le_bytes());out[14..16].copy_from_slice(&1u16.to_le_bytes());out[16..20].copy_from_slice(&(count as u32).to_le_bytes());
    let mut n=0;for r in resources.iter().take(count).flatten(){let p=20+n*CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE;out[p]=if r.kind==BarKind::Io{1}else{3};out[p+1]=1;out[p+2..p+4].copy_from_slice(&(if r.kind==BarKind::Io{1u16}else if r.prefetchable{4}else{0}).to_le_bytes());let start=if translated{r.translated_start}else{r.raw_start};out[p+4..p+12].copy_from_slice(&start.to_le_bytes());out[p+12..p+16].copy_from_slice(&r.length.to_le_bytes());n+=1}
    if n!=count{return Err(ResourceError::InvalidBar)}Ok(size)
}

#[cfg(test)]
mod tests{
 use super::*;
 fn r(i:u8,k:BarKind,l:u64,p:bool)->Option<BarRequirement>{Some(BarRequirement{index:i,kind:k,length:l,alignment:l,prefetchable:p,assigned:0})}
 #[test]fn allocation_is_atomic_and_obeys_apertures(){let req=[r(0,BarKind::Memory32,0x1000,false),r(1,BarKind::Io,0x100,false),None,None,None,None];let windows=[Aperture{start:0x8000_0000,length:0x3000,kind:BarKind::Memory32,prefetchable:false},Aperture{start:0xc000,length:0x400,kind:BarKind::Io,prefetchable:false}];let occupied=[OccupiedRange{start:0x8000_0000,length:0x1000,kind:BarKind::Memory32}];let mut plan=AllocationPlan::empty();allocate_all(&req,2,&windows,&occupied,&mut plan).unwrap();assert_eq!(plan.assignments[0].unwrap().base,0x8000_1000);assert_eq!(plan.assignments[1].unwrap().base,0xc000);let old=plan;let big=[r(0,BarKind::Memory32,0x8000,false),None,None,None,None,None];assert_eq!(allocate_all(&big,1,&windows,&occupied,&mut plan),Err(ResourceError::NoSpace));assert_eq!(plan,old)}
 #[test]fn translation_checks_full_range(){let plan=AllocationPlan{assignments:[Some(BarAssignment{index:0,kind:BarKind::Memory32,base:0x9000,length:0x1000,prefetchable:false}),None,None,None,None,None],count:1};let windows=[AddressTranslation{bus_start:0x8000,system_start:0xf000_0000,length:0x2000}];let mut out=[None;MAX_ASSIGNMENTS];translate_plan(&plan,&windows,&mut out).unwrap();assert_eq!(out[0].unwrap().translated_start,0xf000_1000);let short=[AddressTranslation{bus_start:0x9000,system_start:0xf000_0000,length:0x800}];assert_eq!(translate_plan(&plan,&short,&mut out),Err(ResourceError::NoWindow))}
 #[test]fn programs_bars_and_never_enables_bus_master(){struct Fake{regs:[u32;64]}impl ConfigAccess for Fake{fn read_u16(&mut self,o:u16)->Result<u16,ResourceError>{Ok(self.regs[o as usize/4]as u16)}fn write_u16(&mut self,o:u16,v:u16)->Result<(),ResourceError>{self.regs[o as usize/4]=(self.regs[o as usize/4]&0xffff_0000)|v as u32;Ok(())}fn read_u32(&mut self,o:u16)->Result<u32,ResourceError>{Ok(self.regs[o as usize/4])}fn write_u32(&mut self,o:u16,v:u32)->Result<(),ResourceError>{self.regs[o as usize/4]=v;Ok(())}}let mut f=Fake{regs:[0;64]};f.regs[1]=7;f.regs[4]=0x0000_0008;let plan=AllocationPlan{assignments:[Some(BarAssignment{index:0,kind:BarKind::Memory32,base:0x8000_0000,length:0x1000,prefetchable:false}),None,None,None,None,None],count:1};program_assignments(&mut f,0,&plan).unwrap();assert_eq!(f.regs[4],0x8000_0008);assert_eq!(f.regs[1]as u16,2)}
 #[test]fn rejects_duplicate_bar_assignments_before_writing(){struct Fake{regs:[u32;64]}impl ConfigAccess for Fake{fn read_u16(&mut self,o:u16)->Result<u16,ResourceError>{Ok(self.regs[o as usize/4]as u16)}fn write_u16(&mut self,o:u16,v:u16)->Result<(),ResourceError>{self.regs[o as usize/4]=(self.regs[o as usize/4]&0xffff_0000)|v as u32;Ok(())}fn read_u32(&mut self,o:u16)->Result<u32,ResourceError>{Ok(self.regs[o as usize/4])}fn write_u32(&mut self,o:u16,v:u32)->Result<(),ResourceError>{self.regs[o as usize/4]=v;Ok(())}}let mut f=Fake{regs:[0;64]};let plan=AllocationPlan{assignments:[Some(BarAssignment{index:0,kind:BarKind::Memory32,base:0x8000_0000,length:0x1000,prefetchable:false}),Some(BarAssignment{index:0,kind:BarKind::Memory32,base:0x8000_1000,length:0x1000,prefetchable:false}),None,None,None,None],count:2};assert_eq!(program_assignments(&mut f,0,&plan),Err(ResourceError::InvalidBar));assert_eq!(f.regs[4],0)}
 #[test]fn emits_wdk_requirements_and_cm_wire_layout(){let req=[r(0,BarKind::Memory64,0x2000,true),None,None,None,None,None];let mut io=[0u8;40+6*32];let n=encode_requirements_alternative(&req,1,3,&mut io).unwrap();assert_eq!(n,72);assert_eq!(u32::from_le_bytes(io[36..40].try_into().unwrap()),1);let resources=[Some(PublishedResource{kind:BarKind::Memory64,raw_start:0x1000,translated_start:0x8000_1000,length:0x2000,prefetchable:true}),None,None,None,None,None];let mut cm=[0u8;20+6*16];assert_eq!(encode_cm_resource_list(&resources,1,3,true,&mut cm).unwrap(),36);assert_eq!(u64::from_le_bytes(cm[24..32].try_into().unwrap()),0x8000_1000)}
}


/// Probe one quiesced function and encode its native PnP resource-requirements
/// list as a single operation. The caller must be in passive PnP context and
/// must not call this for a started function. The exclusive config transaction
/// covers the destructive BAR probe; no assignment or bus mastering is enabled.
/// The caller supplies storage suitable for the returned requirements list and
/// transfers/copies it using the kernel's pool-allocation contract.
pub fn probe_and_encode_requirements<C: ConfigAccess>(
    config: &mut C,
    header_type: u8,
    bus: u32,
    output: &mut [u8],
) -> Result<usize, ResourceError> {
    let mut requirements: [Option<BarRequirement>; MAX_ASSIGNMENTS] = [None; MAX_ASSIGNMENTS];
    let count = crate::size_bars_exclusive(config, header_type, &mut requirements)?;
    let alternative = RequirementAlternative { requirements, count };
    encode_requirements_list(core::slice::from_ref(&alternative), bus, output)
}


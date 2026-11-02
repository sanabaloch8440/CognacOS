//! Bounded decoder and selector for Windows x64 IO_RESOURCE_REQUIREMENTS_LIST.
//!
//! Each IO_RESOURCE_LIST is expanded into complete alternatives. A descriptor
//! with IO_RESOURCE_ALTERNATIVE is a mutually-exclusive choice for the
//! immediately preceding requirement group. Expansion is bounded to prevent
//! hostile firmware/driver input from causing combinatorial memory growth.
#![no_std]

use super::resource_arbiter::{
    ArbiterError, ResourceArbiter, ResourceAssignment, ResourceKind, ResourceRequest,
    SYSTEM_RESOURCE_ARBITER,
};

pub const REQUIREMENTS_HEADER_SIZE: usize = 32;
pub const IO_RESOURCE_LIST_HEADER_SIZE: usize = 8;
pub const IO_RESOURCE_DESCRIPTOR_SIZE: usize = 32;
pub const MAX_ALTERNATIVE_LISTS: usize = 8;
pub const MAX_DESCRIPTORS_PER_LIST: usize = 16;
pub const MAX_CHOICES_PER_REQUIREMENT: usize = 8;
pub const MAX_EXPANDED_ALTERNATIVES: usize = 8;

const OPTION_PREFERRED: u8 = 0x01;
const OPTION_ALTERNATIVE: u8 = 0x08;
const OPTION_MASK: u8 = OPTION_PREFERRED | OPTION_ALTERNATIVE;
const INTERRUPT_LEVEL_SENSITIVE: u16 = 0x0001;
const SHARE_EXCLUSIVE: u8 = 1;
const SHARE_SHARED: u8 = 3;
const TYPE_PORT: u8 = 1;
const TYPE_INTERRUPT: u8 = 2;
const TYPE_MEMORY: u8 = 3;
const TYPE_DMA: u8 = 4;
const TYPE_BUS_NUMBER: u8 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequirementsError {
    NullBuffer, Truncated, InvalidSize, TooManyAlternatives, TooManyDescriptors,
    TooManyChoices, UnsupportedDescriptor, InvalidRange, InvalidAlignment,
    OutputTooSmall, Arbitration(ArbiterError),
}
impl From<ArbiterError> for RequirementsError {
    fn from(value: ArbiterError) -> Self { Self::Arbitration(value) }
}

#[derive(Clone, Copy)]
struct ParsedAlternative {
    requests: [ResourceRequest; MAX_DESCRIPTORS_PER_LIST],
    count: usize,
}
impl ParsedAlternative {
    const EMPTY_REQUEST: ResourceRequest = ResourceRequest {
        kind: ResourceKind::Port, minimum: 0, maximum: 0, length: 0, alignment: 1,
    };
    const fn new() -> Self {
        Self { requests: [Self::EMPTY_REQUEST; MAX_DESCRIPTORS_PER_LIST], count: 0 }
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, RequirementsError> {
    let s=bytes.get(offset..offset.checked_add(2).ok_or(RequirementsError::InvalidSize)?)
        .ok_or(RequirementsError::Truncated)?;
    Ok(u16::from_le_bytes([s[0],s[1]]))
}
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, RequirementsError> {
    let s=bytes.get(offset..offset.checked_add(4).ok_or(RequirementsError::InvalidSize)?)
        .ok_or(RequirementsError::Truncated)?;
    Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))
}
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, RequirementsError> {
    let s=bytes.get(offset..offset.checked_add(8).ok_or(RequirementsError::InvalidSize)?)
        .ok_or(RequirementsError::Truncated)?;
    Ok(u64::from_le_bytes(s.try_into().map_err(|_|RequirementsError::Truncated)?))
}

fn parse_descriptor(bytes:&[u8],offset:usize)->Result<ResourceRequest,RequirementsError>{
    let kind=*bytes.get(offset+1).ok_or(RequirementsError::Truncated)?;
    let share=*bytes.get(offset+2).ok_or(RequirementsError::Truncated)?;
    let (resource_kind,minimum,maximum,length,alignment)=match kind{
        TYPE_PORT|TYPE_MEMORY=>{
            if share!=SHARE_EXCLUSIVE{return Err(RequirementsError::UnsupportedDescriptor)}
            let len=read_u32(bytes,offset+8)? as u64;
            let align=read_u32(bytes,offset+12)? as u64;
            let min=read_u64(bytes,offset+16)?;
            let max=read_u64(bytes,offset+24)?;
            (if kind==TYPE_PORT{ResourceKind::Port}else{ResourceKind::Memory},min,max,len,align)
        }
        TYPE_INTERRUPT=>{
            let min=read_u32(bytes,offset+8)? as u64;
            let max=read_u32(bytes,offset+12)? as u64;
            let flags=read_u16(bytes,offset+4)?;
            let rk=match share{
                SHARE_EXCLUSIVE=>ResourceKind::Interrupt,
                SHARE_SHARED=>{
                    // Only level-sensitive line interrupts are shareable in
                    // this initial arbiter. Edge/MSI/policy variants fail closed.
                    if flags&INTERRUPT_LEVEL_SENSITIVE==0 || flags&!INTERRUPT_LEVEL_SENSITIVE!=0 {
                        return Err(RequirementsError::UnsupportedDescriptor)
                    }
                    ResourceKind::SharedInterrupt
                },
                _=>return Err(RequirementsError::UnsupportedDescriptor)
            };
            (rk,min,max,1,1)
        }
        TYPE_DMA=>{
            if share!=SHARE_EXCLUSIVE{return Err(RequirementsError::UnsupportedDescriptor)}
            let min=read_u32(bytes,offset+8)? as u64;
            let max=read_u32(bytes,offset+12)? as u64;
            (ResourceKind::Dma,min,max,1,1)
        }
        TYPE_BUS_NUMBER=>{
            if share!=SHARE_EXCLUSIVE{return Err(RequirementsError::UnsupportedDescriptor)}
            let len=read_u32(bytes,offset+8)? as u64;
            let min=read_u32(bytes,offset+12)? as u64;
            let max=read_u32(bytes,offset+16)? as u64;
            (ResourceKind::BusNumber,min,max,len,1)
        }
        _=>return Err(RequirementsError::UnsupportedDescriptor),
    };
    if length==0||minimum>maximum{return Err(RequirementsError::InvalidRange)}
    if alignment==0||!alignment.is_power_of_two(){return Err(RequirementsError::InvalidAlignment)}
    maximum.checked_add(length-1).ok_or(RequirementsError::InvalidRange)?;
    Ok(ResourceRequest{kind:resource_kind,minimum,maximum,length,alignment})
}

#[derive(Clone,Copy)]
struct ChoiceGroup {
    choices:[ResourceRequest;MAX_CHOICES_PER_REQUIREMENT],
    preferred:[bool;MAX_CHOICES_PER_REQUIREMENT],
    count:usize,
}
impl ChoiceGroup {
    const fn new()->Self{Self{choices:[ParsedAlternative::EMPTY_REQUEST;MAX_CHOICES_PER_REQUIREMENT],preferred:[false;MAX_CHOICES_PER_REQUIREMENT],count:0}}
}

/// Expand one IO_RESOURCE_LIST's descriptor choice groups into full resource
/// sets. Option=0 begins a requirement; Option=IO_RESOURCE_ALTERNATIVE adds an
/// exclusive choice to the immediately preceding requirement.
fn expand_io_resource_list(
    bytes:&[u8],descriptor_start:usize,descriptor_count:usize,
    output:&mut [ParsedAlternative;MAX_EXPANDED_ALTERNATIVES],
)->Result<usize,RequirementsError>{
    if descriptor_count==0{return Err(RequirementsError::InvalidSize)}
    let mut groups=[ChoiceGroup::new();MAX_DESCRIPTORS_PER_LIST];
    let mut group_count=0usize;
    for i in 0..descriptor_count{
        let offset=descriptor_start+i*IO_RESOURCE_DESCRIPTOR_SIZE;
        let option=*bytes.get(offset).ok_or(RequirementsError::Truncated)?;
        if option&!OPTION_MASK!=0{return Err(RequirementsError::UnsupportedDescriptor)}
        let request=parse_descriptor(bytes,offset)?;
        if option&OPTION_ALTERNATIVE==0{
            if group_count>=MAX_DESCRIPTORS_PER_LIST{return Err(RequirementsError::TooManyDescriptors)}
            groups[group_count].choices[0]=request;
            groups[group_count].count=1;
            group_count+=1;
            if option&OPTION_PREFERRED!=0 { groups[group_count-1].preferred[0]=true; }
        }else{
            if group_count==0{return Err(RequirementsError::InvalidSize)}
            let group=&mut groups[group_count-1];
            if group.count>=MAX_CHOICES_PER_REQUIREMENT{return Err(RequirementsError::TooManyChoices)}
            // A choice group must retain one resource class and compatible
            // sharing disposition; otherwise it cannot be safely arbitrated.
            if group.choices[0].kind!=request.kind{return Err(RequirementsError::UnsupportedDescriptor)}
            group.choices[group.count]=request;
            group.preferred[group.count]=option&OPTION_PREFERRED!=0;
            group.count+=1;
        }
    }
    // Move explicitly preferred choices ahead of ordinary choices while
    // retaining descriptor order within each preference class.
    for group in groups[..group_count].iter_mut(){
        for i in 0..group.count {
            if group.preferred[i] {
                let choice=group.choices[i];
                let mut j=i;
                while j>0 && !group.preferred[j-1] {
                    group.choices[j]=group.choices[j-1];
                    group.preferred[j]=group.preferred[j-1];
                    j-=1;
                }
                group.choices[j]=choice;
                group.preferred[j]=true;
            }
        }
    }
    let mut expanded=1usize;
    for group in groups[..group_count].iter(){
        expanded=expanded.checked_mul(group.count).ok_or(RequirementsError::TooManyAlternatives)?;
        if expanded>MAX_EXPANDED_ALTERNATIVES{return Err(RequirementsError::TooManyAlternatives)}
    }
    for slot in output.iter_mut().take(expanded){*slot=ParsedAlternative::new();}
    // Bounded mixed-radix enumeration of the Cartesian product.
    for candidate in 0..expanded{
        let mut divisor=1usize;
        let mut count=0usize;
        for group in groups[..group_count].iter(){
            let choice=(candidate/divisor)%group.count;
            output[candidate].requests[count]=group.choices[choice];
            count+=1;
            divisor*=group.count;
        }
        output[candidate].count=count;
    }
    Ok(expanded)
}

fn parse_requirements(bytes:&[u8])->Result<([ParsedAlternative;MAX_EXPANDED_ALTERNATIVES],usize),RequirementsError>{
    if bytes.is_empty(){return Err(RequirementsError::NullBuffer)}
    if bytes.len()<REQUIREMENTS_HEADER_SIZE{return Err(RequirementsError::Truncated)}
    let list_size=read_u32(bytes,0)? as usize;
    if list_size<REQUIREMENTS_HEADER_SIZE||list_size>bytes.len(){return Err(RequirementsError::InvalidSize)}
    let list_count=read_u32(bytes,28)? as usize;
    if list_count==0{return Err(RequirementsError::InvalidSize)}
    if list_count>MAX_ALTERNATIVE_LISTS{return Err(RequirementsError::TooManyAlternatives)}
    let mut alternatives=[ParsedAlternative::new();MAX_EXPANDED_ALTERNATIVES];
    let mut total=0usize;
    let mut cursor=REQUIREMENTS_HEADER_SIZE;
    for _ in 0..list_count{
        let header_end=cursor.checked_add(IO_RESOURCE_LIST_HEADER_SIZE).ok_or(RequirementsError::InvalidSize)?;
        if header_end>list_size{return Err(RequirementsError::Truncated)}
        if read_u16(bytes,cursor)?!=1||read_u16(bytes,cursor+2)?!=1{return Err(RequirementsError::UnsupportedDescriptor)}
        let descriptors=read_u32(bytes,cursor+4)? as usize;
        if descriptors>MAX_DESCRIPTORS_PER_LIST{return Err(RequirementsError::TooManyDescriptors)}
        let descriptor_bytes=descriptors.checked_mul(IO_RESOURCE_DESCRIPTOR_SIZE).ok_or(RequirementsError::InvalidSize)?;
        let end=header_end.checked_add(descriptor_bytes).ok_or(RequirementsError::InvalidSize)?;
        if end>list_size{return Err(RequirementsError::Truncated)}
        let mut local=[ParsedAlternative::new();MAX_EXPANDED_ALTERNATIVES];
        let expanded=expand_io_resource_list(bytes,header_end,descriptors,&mut local)?;
        if total+expanded>MAX_EXPANDED_ALTERNATIVES{return Err(RequirementsError::TooManyAlternatives)}
        for item in local[..expanded].iter(){alternatives[total]=*item;total+=1;}
        cursor=end;
    }
    if cursor!=list_size{return Err(RequirementsError::InvalidSize)}
    Ok((alternatives,total))
}

pub fn reserve_wdm_requirements(owner:u64,bytes:&[u8],output:&mut [ResourceAssignment])->Result<(usize,usize),RequirementsError>{
    reserve_wdm_requirements_with(&SYSTEM_RESOURCE_ARBITER,owner,bytes,output)
}
pub fn reserve_wdm_requirements_with(
    arbiter:&ResourceArbiter,owner:u64,bytes:&[u8],output:&mut [ResourceAssignment],
)->Result<(usize,usize),RequirementsError>{
    let(parsed,count)=parse_requirements(bytes)?;
    let max_count=parsed[..count].iter().map(|a|a.count).max().unwrap_or(0);
    if output.len()<max_count{return Err(RequirementsError::OutputTooSmall)}
    let mut refs:[&[ResourceRequest];MAX_EXPANDED_ALTERNATIVES]=[&[];MAX_EXPANDED_ALTERNATIVES];
    for i in 0..count{refs[i]=&parsed[i].requests[..parsed[i].count];}
    let selected_count=arbiter.reserve_alternative(owner,&refs[..count],output)?;
    let index=parsed[..count].iter().position(|alternative|{
        alternative.count==selected_count
        &&alternative.requests[..selected_count].iter().zip(output[..selected_count].iter()).all(|(request,assignment)|
            assignment.kind==request.kind&&assignment.length==request.length
            &&assignment.start>=request.minimum&&assignment.start<=request.maximum
            &&assignment.start%request.alignment==0)
    }).ok_or(RequirementsError::InvalidSize)?;
    Ok((index,selected_count))
}

#[cfg(test)] extern crate std;
#[cfg(test)] mod tests{
 use super::*;use std::vec;
 fn descriptor(bytes:&mut [u8],offset:usize,option:u8,kind:u8,share:u8,min:u32,max:u32){
  bytes[offset]=option;bytes[offset+1]=kind;bytes[offset+2]=share;
  if kind==TYPE_PORT||kind==TYPE_MEMORY{
   bytes[offset+8..offset+12].copy_from_slice(&8u32.to_le_bytes());
   bytes[offset+12..offset+16].copy_from_slice(&8u32.to_le_bytes());
   bytes[offset+16..offset+24].copy_from_slice(&(min as u64).to_le_bytes());
   bytes[offset+24..offset+32].copy_from_slice(&(max as u64).to_le_bytes());
  }else{
   bytes[offset+8..offset+12].copy_from_slice(&min.to_le_bytes());
   bytes[offset+12..offset+16].copy_from_slice(&max.to_le_bytes());
  }
 }
 fn make_list(desc:&[(u8,u8,u8,u32,u32)])->std::vec::Vec<u8>{
  let size=REQUIREMENTS_HEADER_SIZE+IO_RESOURCE_LIST_HEADER_SIZE+desc.len()*IO_RESOURCE_DESCRIPTOR_SIZE;
  let mut b=vec![0u8;size];b[0..4].copy_from_slice(&(size as u32).to_le_bytes());
  b[4..8].copy_from_slice(&15u32.to_le_bytes());b[28..32].copy_from_slice(&1u32.to_le_bytes());
  b[32..34].copy_from_slice(&1u16.to_le_bytes());b[34..36].copy_from_slice(&1u16.to_le_bytes());
  b[36..40].copy_from_slice(&(desc.len() as u32).to_le_bytes());
  for(i,(o,k,s,min,max))in desc.iter().enumerate(){descriptor(&mut b,40+i*32,*o,*k,*s,*min,*max);}
  b
 }
 #[test]fn expands_descriptor_choices_as_complete_alternatives(){
  let b=make_list(&[(0,TYPE_PORT,SHARE_EXCLUSIVE,0x300,0x300),(OPTION_ALTERNATIVE,TYPE_PORT,SHARE_EXCLUSIVE,0x400,0x400)]);
  let(parsed,count)=parse_requirements(&b).unwrap();assert_eq!(count,2);
  assert_eq!(parsed[0].requests[0].minimum,0x300);assert_eq!(parsed[1].requests[0].minimum,0x400);
 }
 #[test]fn expands_cartesian_product_for_independent_choice_groups(){
  let b=make_list(&[(0,TYPE_PORT,SHARE_EXCLUSIVE,0x300,0x300),(8,TYPE_PORT,SHARE_EXCLUSIVE,0x400,0x400),(0,TYPE_INTERRUPT,SHARE_EXCLUSIVE,5,5),(8,TYPE_INTERRUPT,SHARE_EXCLUSIVE,7,7)]);
  let(parsed,count)=parse_requirements(&b).unwrap();assert_eq!(count,4);
  assert_eq!(parsed[0].requests[0].minimum,0x300);assert_eq!(parsed[0].requests[1].minimum,5);
  assert_eq!(parsed[1].requests[0].minimum,0x400);assert_eq!(parsed[1].requests[1].minimum,5);
  assert_eq!(parsed[2].requests[0].minimum,0x300);assert_eq!(parsed[2].requests[1].minimum,7);
 }
 #[test]fn allows_two_devices_to_share_a_shared_interrupt_but_not_exclusive_irq(){
  let arb=ResourceArbiter::new();
  let shared=[ResourceRequest{kind:ResourceKind::SharedInterrupt,minimum:9,maximum:9,length:1,alignment:1}];
  let exclusive=[ResourceRequest{kind:ResourceKind::Interrupt,minimum:9,maximum:9,length:1,alignment:1}];
  let mut out=[ResourceAssignment{kind:ResourceKind::Port,start:0,length:0};1];
  arb.reserve_alternative(1,&[&shared],&mut out).unwrap();
  assert_eq!(arb.reserve_alternative(2,&[&shared],&mut out),Ok(1));
  assert_eq!(arb.reserve_alternative(3,&[&exclusive],&mut out),Err(ArbiterError::Conflict));
 }
 #[test]fn preferred_alternative_is_tried_before_unpreferred_choice(){
  let b=make_list(&[(0,TYPE_PORT,SHARE_EXCLUSIVE,0x300,0x300),(9,TYPE_PORT,SHARE_EXCLUSIVE,0x400,0x400)]);
  let(parsed,count)=parse_requirements(&b).unwrap();assert_eq!(count,2);
  assert_eq!(parsed[0].requests[0].minimum,0x400);assert_eq!(parsed[1].requests[0].minimum,0x300);
 }
 #[test]fn shared_irq_requires_level_sensitive_flag(){
  let mut b=make_list(&[(0,TYPE_INTERRUPT,SHARE_SHARED,9,9)]);
  let flags_offset=40+4;b[flags_offset..flags_offset+2].copy_from_slice(&INTERRUPT_LEVEL_SENSITIVE.to_le_bytes());
  assert!(parse_requirements(&b).is_ok());
  b[flags_offset..flags_offset+2].copy_from_slice(&0u16.to_le_bytes());
  assert_eq!(parse_requirements(&b),Err(RequirementsError::UnsupportedDescriptor));
 }
 #[test]fn rejects_choice_group_class_mismatch(){
  let b=make_list(&[(0,TYPE_PORT,SHARE_EXCLUSIVE,0x300,0x300),(8,TYPE_MEMORY,SHARE_EXCLUSIVE,0x400,0x400)]);
  assert_eq!(parse_requirements(&b),Err(RequirementsError::UnsupportedDescriptor));
 }
 #[test]fn rejects_truncated_and_preserves_output(){
  let mut b=make_list(&[(0,TYPE_PORT,SHARE_EXCLUSIVE,0x300,0x300)]);b.pop();
  let mut out=[ResourceAssignment{kind:ResourceKind::Port,start:9,length:9};1];
  assert!(reserve_wdm_requirements_with(&ResourceArbiter::new(),1,&b,&mut out).is_err());assert_eq!(out[0].start,9);
 }
}

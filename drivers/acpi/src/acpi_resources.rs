//! Bounded ACPI resource-template decoding for `_CRS`.
//! This parser validates firmware requirements; it does not allocate or translate them.
/*
 * FIXME(ACPI-PRODUCTION) — ACPI resource-template parser
 * FIXME(ACPI-PRODUCTION): Complete descriptor coverage and exact semantics for _CRS/_PRS templates, including IRQ/DMA/address-space/vendor descriptors, alternatives, priorities, ranges, alignment, and end-tag/checksum rules where applicable. Use checked arithmetic and preserve unsupported descriptors as explicit failures rather than silently dropping resource constraints.
 */

#![no_std]

pub const MAX_RESOURCES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError { MissingEndTag, Truncated, InvalidLength, InvalidDescriptor, TooManyResources, TooManyAlternatives, IntegerOverflow, UnsupportedDescriptor }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Io { minimum:u64, maximum:u64, alignment:u64, length:u64, decode_16_bit:bool },
    Memory { minimum:u64, maximum:u64, alignment:u64, length:u64, writeable:bool, cacheable:bool, kind:u8 },
    Irq { mask:u16, level_triggered:bool, active_low:bool, shared:bool, wake_capable:bool },
    ExtendedIrq { flags:u8, interrupts:[u32;8], count:u8 },
    Dma { channels:u8, transfer_size:u8, bus_master:bool },
    BusNumber { minimum:u64, maximum:u64, length:u64 },
    EndTag { checksum:u8 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceList { pub entries:[Option<Resource>;MAX_RESOURCES], pub count:usize }
impl ResourceList {
    pub const EMPTY:Self=Self{entries:[None;MAX_RESOURCES],count:0};
    fn push(&mut self,item:Resource)->Result<(),ResourceError>{
        if self.count==MAX_RESOURCES{return Err(ResourceError::TooManyResources)}
        self.entries[self.count]=Some(item);self.count+=1;Ok(())
    }
}
fn slice(b:&[u8],o:usize,n:usize)->Result<&[u8],ResourceError>{
    let e=o.checked_add(n).ok_or(ResourceError::IntegerOverflow)?;
    b.get(o..e).ok_or(ResourceError::Truncated)
}
fn u16le(b:&[u8],o:usize)->Result<u16,ResourceError>{let s=slice(b,o,2)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn u32le(b:&[u8],o:usize)->Result<u32,ResourceError>{let s=slice(b,o,4)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
fn u64le(b:&[u8],o:usize)->Result<u64,ResourceError>{let s=slice(b,o,8)?;Ok(u64::from_le_bytes(s.try_into().map_err(|_|ResourceError::Truncated)?))}
fn validate_range(min:u64,max:u64,len:u64)->Result<(),ResourceError>{
    // AML address descriptors encode a range of permitted base addresses;
    // Length is the span at each candidate base, not an upper bound for the base.
    if len==0||min>max||min.checked_add(len-1).is_none()||max.checked_add(len-1).is_none(){return Err(ResourceError::InvalidDescriptor)} Ok(())
}
fn address_space(p:&[u8],width:u8)->Result<Resource,ResourceError>{
    let expected=match width{2=>13,4=>23,8=>43,_=>return Err(ResourceError::UnsupportedDescriptor)};
    if p.len()!=expected{return Err(ResourceError::InvalidLength)}
    let space=p[0];let flags=p[1];let type_flags=p[2];
    let (min,max,align,len)=match width{
        2=>(u16le(p,5)? as u64,u16le(p,7)? as u64,u16le(p,3)? as u64,u16le(p,11)? as u64),
        4=>(u32le(p,7)? as u64,u32le(p,11)? as u64,u32le(p,3)? as u64,u32le(p,19)? as u64),
        _=>(u64le(p,11)?,u64le(p,19)?,u64le(p,3)?,u64le(p,35)?),
    };
    validate_range(min,max,len)?;
    match space{
        0=>Ok(Resource::Memory{minimum:min,maximum:max,alignment:align,length:len,writeable:type_flags&1!=0,cacheable:(type_flags>>1)&3!=0,kind:match width{2=>0,4=>1,8=>2,_=>0}}),
        1=>Ok(Resource::Io{minimum:min,maximum:max,alignment:align,length:len,decode_16_bit:flags&1!=0}),
        2=>Ok(Resource::BusNumber{minimum:min,maximum:max,length:len}),
        _=>Err(ResourceError::UnsupportedDescriptor),
    }
}

/// Parse one complete `_CRS` resource template. No result escapes on failure.
/// Unknown descriptor kinds fail closed; EndTag must be present and final.
/// A zero EndTag checksum means checksum validation is not required by firmware.
pub fn parse_resource_template(bytes:&[u8])->Result<ResourceList,ResourceError>{
    let mut out=ResourceList::EMPTY;let mut pos=0usize;let mut ended=false;
    while pos<bytes.len(){
        let tag=bytes[pos];pos+=1;
        if tag&0x80==0{
            let kind=(tag>>3)&0x0f;let len=(tag&7) as usize;let p=slice(bytes,pos,len)?;let end=pos+len;
            let item=match kind{
                0x04 if len==2||len==3=>{let mask=u16le(p,0)?;let flags=if len==3{p[2]}else{0};
                    Resource::Irq{mask,level_triggered:flags&1!=0,active_low:flags&8!=0,shared:flags&0x10!=0,wake_capable:flags&0x20!=0}},
                0x05 if len==2=>Resource::Dma{channels:p[0],transfer_size:p[1]&3,bus_master:p[1]&4!=0},
                0x08 if len==7=>{let min=u16le(p,1)? as u64;let max=u16le(p,3)? as u64;let align=p[5] as u64;let size=p[6] as u64;
                    validate_range(min,max,size)?;Resource::Io{minimum:min,maximum:max,alignment:align,length:size,decode_16_bit:p[0]&1!=0}},
                0x09 if len==3=>{let base=u16le(p,0)? as u64;let size=p[2] as u64;
                    validate_range(base,base,size)?;Resource::Io{minimum:base,maximum:base,alignment:1,length:size,decode_16_bit:true}},
                0x0f if len==1=>{out.push(Resource::EndTag{checksum:p[0]})?;pos=end;ended=true;if pos!=bytes.len(){return Err(ResourceError::InvalidLength)}
                    if p[0]!=0 && bytes.iter().fold(0u8,|sum,b|sum.wrapping_add(*b))!=0{return Err(ResourceError::InvalidDescriptor)} break;},
                _=>return Err(ResourceError::UnsupportedDescriptor),
            };
            out.push(item)?;pos=end;
        }else{
            let kind=tag&0x7f;let len=u16le(bytes,pos)? as usize;pos=pos.checked_add(2).ok_or(ResourceError::IntegerOverflow)?;
            let p=slice(bytes,pos,len)?;let end=pos.checked_add(len).ok_or(ResourceError::IntegerOverflow)?;
            let item=match kind{
                0x01 if len==9=>{let min=u16le(p,1)? as u64;let max=u16le(p,3)? as u64;let align=u16le(p,5)? as u64;let size=u16le(p,7)? as u64;
                    validate_range(min,max,size)?;Resource::Memory{minimum:min,maximum:max,alignment:align,length:size,writeable:p[0]&1!=0,cacheable:false,kind:0}},
                0x05 if len==17=>{let min=u32le(p,1)? as u64;let max=u32le(p,5)? as u64;let align=u32le(p,9)? as u64;let size=u32le(p,13)? as u64;
                    validate_range(min,max,size)?;Resource::Memory{minimum:min,maximum:max,alignment:align,length:size,writeable:p[0]&1!=0,cacheable:false,kind:1}},
                0x06 if len==9=>{let base=u32le(p,1)? as u64;let size=u32le(p,5)? as u64;
                    validate_range(base,base,size)?;Resource::Memory{minimum:base,maximum:base,alignment:0,length:size,writeable:p[0]&1!=0,cacheable:false,kind:2}},
                0x07=>address_space(p,4)?,
                0x08=>address_space(p,2)?,
                0x09 if len>=6=>{let flags=p[0];let count=p[1] as usize;if count==0||count>8||len!=2+count*4{return Err(ResourceError::InvalidLength)}
                    let mut ints=[0u32;8];for i in 0..count{ints[i]=u32le(p,2+i*4)?;}
                    Resource::ExtendedIrq{flags,interrupts:ints,count:count as u8}},
                0x0a=>address_space(p,8)?,
                0x0b if len==53=>{ // Extended address-space descriptor; fields follow QWord layout plus type-specific attributes.
                    let space=p[0];let flags=p[1];let type_flags=p[2];let min=u64le(p,11)?;let max=u64le(p,19)?;
                    let align=u64le(p,27)?;let size=u64le(p,35)?;validate_range(min,max,size)?;
                    match space{0=>Resource::Memory{minimum:min,maximum:max,alignment:align,length:size,writeable:type_flags&1!=0,cacheable:(type_flags>>1)&3!=0,kind:8},
                    1=>Resource::Io{minimum:min,maximum:max,alignment:align,length:size,decode_16_bit:flags&1!=0},
                    2=>Resource::BusNumber{minimum:min,maximum:max,length:size},_=>return Err(ResourceError::UnsupportedDescriptor)}},
                0x0c if len==3=>{let base=u16le(p,0)? as u64;let size=p[2] as u64;
                    validate_range(base,base,size)?;Resource::Io{minimum:base,maximum:base,alignment:1,length:size,decode_16_bit:true}},
                _=>return Err(ResourceError::UnsupportedDescriptor),
            };
            out.push(item)?;pos=end;
        }
    }
    if !ended{return Err(ResourceError::MissingEndTag)} Ok(out)
}

/// Return true when an allocated bus-relative resource satisfies one
/// descriptor from an _PRS alternative. The caller supplies resources
/// selected by the PnP arbiter, not translated HAL addresses.
fn allocation_satisfies(requirement:Resource,allocated:Resource)->bool{
    match (requirement,allocated){
        (Resource::Io{minimum,maximum,alignment,length,..},
         Resource::Io{minimum:base,length:actual,..}) =>
            actual==length&&base>=minimum&&base<=maximum
                &&(alignment<=1||base%alignment==0),
        (Resource::Memory{minimum,maximum,alignment,length,..},
         Resource::Memory{minimum:base,length:actual,..}) =>
            actual==length&&base>=minimum&&base<=maximum
                &&(alignment<=1||base%alignment==0),
        (Resource::BusNumber{minimum,maximum,length},
         Resource::BusNumber{minimum:base,length:actual,..}) =>
            actual==length&&base>=minimum&&base<=maximum,
        (Resource::Irq{mask,level_triggered,active_low,shared,..},
         Resource::Irq{mask:allocated_mask,level_triggered:a_level,active_low:a_low,shared:a_shared,..}) =>
            allocated_mask.count_ones()==1&&mask&allocated_mask!=0
                &&level_triggered==a_level&&active_low==a_low&&shared==a_shared,
        (Resource::Irq{mask,level_triggered,active_low,shared,..},
         Resource::ExtendedIrq{flags,interrupts,count}) =>
            count==1&&mask&(1u16.checked_shl(interrupts[0]).unwrap_or(0) as u16)!=0
                &&level_triggered==(flags&2!=0)&&active_low==(flags&4!=0)&&shared==(flags&8!=0),
        (Resource::ExtendedIrq{flags,interrupts,count,..},
         Resource::ExtendedIrq{flags:actual_flags,interrupts:actual,count:actual_count}) =>
            count>0&&actual_count>0&&flags&0x1e==actual_flags&0x1e
                &&actual[..actual_count as usize].iter().all(|gsi|interrupts[..count as usize].contains(gsi)),
        (Resource::Dma{channels,transfer_size,bus_master},
         Resource::Dma{channels:allocated,transfer_size:actual_size,bus_master:actual_master}) =>
            allocated.count_ones()==1&&channels&allocated!=0
                &&transfer_size==actual_size&&bus_master==actual_master,
        _=>false,
    }
}

/// Verify that a PnP arbiter allocation satisfies one complete _PRS
/// alternative. Resources are matched as a multiset (order-independent),
/// and the returned list is the arbiter's bus-relative assignment.
pub fn select_assigned_resources(
    alternatives:&ResourceAlternatives,
    allocated:&ResourceList,
)->Result<ResourceList,ResourceError>{
    if alternatives.count==0||alternatives.count>alternatives.entries.len()
        ||allocated.count>allocated.entries.len(){return Err(ResourceError::InvalidDescriptor)}
    let allocated_count=allocated.entries[..allocated.count].iter()
        .flatten().filter(|r|!matches!(r,Resource::EndTag{..})).count();
    for alternative in alternatives.entries[..alternatives.count].iter(){
        if alternative.count>alternative.entries.len(){return Err(ResourceError::InvalidDescriptor)}
        let required: [Option<Resource>;MAX_RESOURCES]=alternative.entries;
        let required_count=required[..alternative.count].iter().flatten()
            .filter(|r|!matches!(r,Resource::EndTag{..})).count();
        if required_count!=allocated_count{continue}
        let mut used=[false;MAX_RESOURCES];
        let mut matched=true;
        for req in required[..alternative.count].iter().flatten(){
            if matches!(req,Resource::EndTag{..}){continue}
            let mut found=None;
            for (index,candidate) in allocated.entries[..allocated.count].iter().enumerate(){
                if used[index]{continue}
                if let Some(candidate)=candidate{
                    if allocation_satisfies(*req,*candidate){found=Some(index);break}
                }
            }
            if let Some(index)=found{used[index]=true}else{matched=false;break}
        }
        if matched{return Ok(*allocated)}
    }
    Err(ResourceError::InvalidDescriptor)
}

/// Verify that firmware's post-_SRS _CRS describes the exact allocation
/// selected by the PnP arbiter. Matching is order-independent and rejects
/// missing, duplicated, or additional non-EndTag resources.
pub fn verify_current_resources(
    allocated:&ResourceList,
    current:&ResourceList,
)->Result<(),ResourceError>{
    if allocated.count>allocated.entries.len()||current.count>current.entries.len(){
        return Err(ResourceError::InvalidDescriptor)
    }
    let expected_count=allocated.entries[..allocated.count].iter().flatten()
        .filter(|r|!matches!(r,Resource::EndTag{..})).count();
    let current_count=current.entries[..current.count].iter().flatten()
        .filter(|r|!matches!(r,Resource::EndTag{..})).count();
    if expected_count!=current_count{return Err(ResourceError::InvalidDescriptor)}
    let mut used=[false;MAX_RESOURCES];
    for expected in allocated.entries[..allocated.count].iter().flatten(){
        if matches!(expected,Resource::EndTag{..}){continue}
        let mut found=None;
        for (index,candidate) in current.entries[..current.count].iter().enumerate(){
            if used[index]{continue}
            if let Some(candidate)=candidate{
                if allocation_satisfies(*candidate,*expected){found=Some(index);break}
            }
        }
        if let Some(index)=found{used[index]=true}else{return Err(ResourceError::InvalidDescriptor)}
    }
    Ok(())
}

/// Encode an allocated resource list as an ACPI resource template suitable
/// for _SRS. Address descriptors use QWord form to avoid truncating 64-bit
/// physical apertures; interrupts use Extended IRQ descriptors.
pub fn encode_resource_template(resources:&ResourceList,output:&mut [u8])->Result<usize,ResourceError>{
    if resources.count>resources.entries.len(){return Err(ResourceError::InvalidDescriptor)}
    let mut cursor=0usize;
    for resource in resources.entries[..resources.count].iter().flatten(){
        match *resource{
            Resource::Io{minimum,maximum,alignment,length,decode_16_bit:_}=>{
                encode_qword_address(output,&mut cursor,1,0,0,alignment,minimum,maximum,length)?;
            }
            Resource::Memory{minimum,maximum,alignment,length,writeable,cacheable,..}=>{
                encode_qword_address(output,&mut cursor,0,0,(if writeable{1}else{0})|(if cacheable{2}else{0}),alignment,minimum,maximum,length)?;
            }
            Resource::BusNumber{minimum,maximum,length}=>{
                encode_qword_address(output,&mut cursor,2,0,0,1,minimum,maximum,length)?;
            }
            Resource::Irq{mask,level_triggered,active_low,shared,wake_capable}=>{
                let mut interrupts=[0u32;8];let mut count=0usize;
                for irq in 0..16u32{if mask&(1u16<<irq)!=0{interrupts[count]=irq;count+=1;}}
                if count==0||count>8{return Err(ResourceError::UnsupportedDescriptor)}
                let flags=(if level_triggered{2}else{0})|(if active_low{4}else{0})|(if shared{8}else{0})|(if wake_capable{0x10}else{0});
                encode_extended_irq(output,&mut cursor,flags,&interrupts[..count])?;
            }
            Resource::ExtendedIrq{flags,interrupts,count}=>{
                if count==0||count>8{return Err(ResourceError::InvalidDescriptor)}
                encode_extended_irq(output,&mut cursor,flags,&interrupts[..count as usize])?;
            }
            Resource::Dma{channels,transfer_size,bus_master}=>{
                let flags=(transfer_size&3)|if bus_master{4}else{0};
                put_resource_bytes(output,&mut cursor,&[0x2a,channels,flags])?;
            }
            Resource::EndTag{..}=>{},
        }
    }
    let end=cursor.checked_add(2).ok_or(ResourceError::IntegerOverflow)?;
    if end>output.len(){return Err(ResourceError::Truncated)}
    output[cursor]=0x79;output[cursor+1]=0;
    let checksum=output[..end].iter().fold(0u8,|sum,b|sum.wrapping_add(*b));
    output[cursor+1]=0u8.wrapping_sub(checksum);
    Ok(end)
}

fn put_resource_bytes(output:&mut [u8],cursor:&mut usize,bytes:&[u8])->Result<(),ResourceError>{
    let end=cursor.checked_add(bytes.len()).ok_or(ResourceError::IntegerOverflow)?;
    if end>output.len(){return Err(ResourceError::Truncated)}
    output[*cursor..end].copy_from_slice(bytes);*cursor=end;Ok(())
}
fn encode_qword_address(output:&mut [u8],cursor:&mut usize,space:u8,flags:u8,type_flags:u8,
    alignment:u64,minimum:u64,maximum:u64,length:u64)->Result<(),ResourceError>{
    validate_range(minimum,maximum,length)?;
    let mut descriptor=[0u8;46];
    descriptor[0]=0x8a;descriptor[1]=43;descriptor[2]=space;
    descriptor[3]=flags;descriptor[4]=type_flags;
    descriptor[5..13].copy_from_slice(&alignment.to_le_bytes());
    descriptor[13..21].copy_from_slice(&minimum.to_le_bytes());
    descriptor[21..29].copy_from_slice(&maximum.to_le_bytes());
    descriptor[29..37].copy_from_slice(&0u64.to_le_bytes());
    descriptor[37..45].copy_from_slice(&length.to_le_bytes());
    put_resource_bytes(output,cursor,&descriptor)
}
fn encode_extended_irq(output:&mut [u8],cursor:&mut usize,flags:u8,interrupts:&[u32])->Result<(),ResourceError>{
    if interrupts.is_empty()||interrupts.len()>8{return Err(ResourceError::InvalidDescriptor)}
    let payload=2usize.checked_add(interrupts.len().checked_mul(4).ok_or(ResourceError::IntegerOverflow)?)
        .ok_or(ResourceError::IntegerOverflow)?;
    let mut descriptor=[0u8;37];descriptor[0]=0x89;
    descriptor[1]=payload as u8;descriptor[2]=0;descriptor[3]=flags;
    descriptor[4]=interrupts.len() as u8;
    for (index,gsi) in interrupts.iter().enumerate(){
        let offset=5+index*4;descriptor[offset..offset+4].copy_from_slice(&gsi.to_le_bytes());
    }
    put_resource_bytes(output,cursor,&descriptor[..3+payload])
}

#[cfg(test)]
mod assigned_resource_encoding_tests{
 use super::*;
 #[test]
 fn selects_arbiter_allocation_and_round_trips_srs_template(){
  let mut requirement=ResourceList::EMPTY;
  requirement.entries[0]=Some(Resource::Io{minimum:0x1000,maximum:0x1fff,alignment:0x100,length:0x100,decode_16_bit:true});
  requirement.entries[1]=Some(Resource::ExtendedIrq{flags:0x0a,interrupts:[5,9,0,0,0,0,0,0],count:2});
  requirement.count=2;
  let mut alternatives=ResourceAlternatives::EMPTY;
  alternatives.entries[0]=requirement;alternatives.count=1;

  let mut allocated=ResourceList::EMPTY;
  allocated.entries[0]=Some(Resource::Io{minimum:0x1100,maximum:0x1100,alignment:1,length:0x100,decode_16_bit:true});
  allocated.entries[1]=Some(Resource::ExtendedIrq{flags:0x0a,interrupts:[9,0,0,0,0,0,0,0],count:1});
  allocated.count=2;
  let selected=select_assigned_resources(&alternatives,&allocated).unwrap();
  assert_eq!(selected,allocated);

  let mut bytes=[0u8;128];
  let length=encode_resource_template(&selected,&mut bytes).unwrap();
  let parsed=parse_resource_template(&bytes[..length]).unwrap();
  assert!(matches!(parsed.entries[0],Some(Resource::Io{minimum:0x1100,maximum:0x1100,length:0x100,..})));
  assert!(matches!(parsed.entries[1],Some(Resource::ExtendedIrq{flags:0x0a,count:1,interrupts,..}) if interrupts[0]==9));
 }
 #[test]
 fn rejects_allocations_outside_every_prs_alternative(){
  let mut required=ResourceList::EMPTY;
  required.entries[0]=Some(Resource::Io{minimum:0x1000,maximum:0x1fff,alignment:0x100,length:0x100,decode_16_bit:true});
  required.count=1;
  let mut alternatives=ResourceAlternatives::EMPTY;alternatives.entries[0]=required;alternatives.count=1;
  let mut allocated=ResourceList::EMPTY;
  allocated.entries[0]=Some(Resource::Io{minimum:0x1081,maximum:0x1081,alignment:1,length:0x100,decode_16_bit:true});
  allocated.count=1;
  assert_eq!(select_assigned_resources(&alternatives,&allocated),Err(ResourceError::InvalidDescriptor));
 }
}

#[cfg(test)] extern crate std;
#[cfg(test)] mod tests{
 use super::*;
 #[test]fn parses_io_irq_and_end_tag(){let data=[0x47,0x01,0x00,0x01,0x00,0x01,0x01,0x08,0x22,0x0b,0x00,0x79,0x00];let r=parse_resource_template(&data).unwrap();
  assert_eq!(r.count,3);assert!(matches!(r.entries[0],Some(Resource::Io{minimum:0x100,maximum:0x100,length:8,..})));
  assert!(matches!(r.entries[1],Some(Resource::Irq{mask:0x0b,..})));}
 #[test]fn parses_fixed_io_descriptor(){let parsed=parse_resource_template(&[0x4b,0x78,0x03,0x03,0x79,0]);assert!(matches!(parsed.unwrap().entries[0],Some(Resource::Io{minimum:0x378,maximum:0x378,length:3,..})));}
 #[test]fn parses_fixed_memory_and_io_as_exact_assigned_bases(){
  let memory=parse_resource_template(&[0x86,9,0,0,0x00,0x10,0,0,0x20,0,0,0,0x79,0]).unwrap();
  assert!(matches!(memory.entries[0],Some(Resource::Memory{minimum:0x1000,maximum:0x1000,length:0x20,..})));
  let io=parse_resource_template(&[0x4b,0x78,0x03,4,0x79,0]).unwrap();
  assert!(matches!(io.entries[0],Some(Resource::Io{minimum:0x378,maximum:0x378,length:4,..})));
 }
 #[test]fn rejects_missing_end_tag(){assert_eq!(parse_resource_template(&[0x47,1,0,1,0,1,1,8]),Err(ResourceError::MissingEndTag));}
 #[test]fn rejects_truncated_descriptor(){assert_eq!(parse_resource_template(&[0x47,1]),Err(ResourceError::Truncated));}
 #[test]fn rejects_zero_length_range(){assert_eq!(parse_resource_template(&[0x47,1,0,1,1,0,0x79,0]),Err(ResourceError::InvalidDescriptor));}
 #[test]fn parses_qword_memory_range(){
  let mut d=[0u8;48];d[0]=0x8a;d[1]=43;d[2]=0;d[3]=0; // large QWord address-space descriptor, memory
  d[14..22].copy_from_slice(&0x100000u64.to_le_bytes());
  d[22..30].copy_from_slice(&0x10ffffu64.to_le_bytes());
  d[30..38].copy_from_slice(&1u64.to_le_bytes());
  d[38..46].copy_from_slice(&0x10000u64.to_le_bytes());
  d[46]=0x79;d[47]=0;
  let parsed=parse_resource_template(&d).unwrap();
  assert!(matches!(parsed.entries[0],Some(Resource::Memory{minimum:0x100000,maximum:0x10ffff,length:0x10000,..})));
 }
 #[test]fn rejects_trailing_data_after_end_tag(){assert_eq!(parse_resource_template(&[0x79,0,0]),Err(ResourceError::InvalidLength));}
}


/// Maximum number of complete configurations represented by one _PRS template.
pub const MAX_RESOURCE_ALTERNATIVES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceAlternatives {
    pub entries: [ResourceList; MAX_RESOURCE_ALTERNATIVES],
    pub count: usize,
}
impl ResourceAlternatives {
    pub const EMPTY: Self = Self {
        entries: [ResourceList::EMPTY; MAX_RESOURCE_ALTERNATIVES],
        count: 0,
    };
    fn push(&mut self, list: ResourceList) -> Result<(), ResourceError> {
        if self.count == MAX_RESOURCE_ALTERNATIVES {
            return Err(ResourceError::TooManyAlternatives);
        }
        self.entries[self.count] = list;
        self.count += 1;
        Ok(())
    }
}

/// Decode a single ordinary descriptor by appending a synthetic EndTag.
/// Keeping the existing descriptor decoder as the sole source of semantics
/// prevents _CRS and _PRS from drifting apart.
fn parse_one_requirement_descriptor(bytes: &[u8]) -> Result<Resource, ResourceError> {
    if bytes.len() > 62 { return Err(ResourceError::UnsupportedDescriptor); }
    let mut bounded = [0u8; 64];
    bounded[..bytes.len()].copy_from_slice(bytes);
    bounded[bytes.len()] = 0x79;
    bounded[bytes.len() + 1] = 0;
    let parsed = parse_resource_template(&bounded[..bytes.len() + 2])?;
    if parsed.count != 2 { return Err(ResourceError::InvalidDescriptor); }
    parsed.entries[0].ok_or(ResourceError::InvalidDescriptor)
}

/// Parse _PRS including StartDependentFn/EndDependentFn groups.
///
/// Resources outside dependent-function groups are common to every
/// configuration. Each complete dependent-function group contributes one
/// alternative, in firmware order. A template without dependent groups
/// produces one alternative containing all ordinary resources.
pub fn parse_resource_requirements_template(bytes: &[u8]) -> Result<ResourceAlternatives, ResourceError> {
    let mut common = ResourceList::EMPTY;
    let mut alternatives = ResourceAlternatives::EMPTY;
    let mut dependent = ResourceList::EMPTY;
    let mut in_dependent = false;
    let mut pos = 0usize;
    let mut ended = false;

    while pos < bytes.len() {
        let start = pos;
        let tag = bytes[pos];
        pos += 1;
        let (kind, length, header_length) = if tag & 0x80 == 0 {
            (((tag >> 3) & 0x0f), (tag & 7) as usize, 1usize)
        } else {
            let length = u16le(bytes, pos)? as usize;
            pos = pos.checked_add(2).ok_or(ResourceError::IntegerOverflow)?;
            ((tag & 0x7f), length, 3usize)
        };
        let payload = slice(bytes, pos, length)?;
        pos = pos.checked_add(length).ok_or(ResourceError::IntegerOverflow)?;
        let descriptor = &bytes[start..pos];

        if header_length == 1 && kind == 0x06 {
            if in_dependent || length > 1 { return Err(ResourceError::InvalidDescriptor); }
            in_dependent = true;
            dependent = ResourceList::EMPTY;
            continue;
        }
        if header_length == 1 && kind == 0x07 {
            if !in_dependent || length != 0 || dependent.count == 0 {
                return Err(ResourceError::InvalidDescriptor);
            }
            alternatives.push(dependent)?;
            in_dependent = false;
            dependent = ResourceList::EMPTY;
            continue;
        }
        if header_length == 1 && kind == 0x0f {
            if length != 1 || pos != bytes.len() {
                return Err(ResourceError::InvalidLength);
            }
            if in_dependent { return Err(ResourceError::InvalidDescriptor); }
            if payload[0] != 0 && bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) != 0 {
                return Err(ResourceError::InvalidDescriptor);
            }
            ended = true;
            break;
        }

        let resource = parse_one_requirement_descriptor(descriptor)?;
        if matches!(resource, Resource::EndTag { .. }) {
            return Err(ResourceError::InvalidDescriptor);
        }
        if in_dependent { dependent.push(resource)?; } else { common.push(resource)?; }
    }

    if !ended { return Err(ResourceError::MissingEndTag); }
    if in_dependent { return Err(ResourceError::InvalidDescriptor); }

    if alternatives.count == 0 {
        alternatives.push(common)?;
    } else {
        for index in 0..alternatives.count {
            let dependent_list = alternatives.entries[index];
            let mut combined = ResourceList::EMPTY;
            for item in common.entries[..common.count].iter().flatten() { combined.push(*item)?; }
            for item in dependent_list.entries[..dependent_list.count].iter().flatten() { combined.push(*item)?; }
            alternatives.entries[index] = combined;
        }
    }
    Ok(alternatives)
}

#[cfg(test)]
mod dependent_requirement_tests {
    use super::*;

    #[test]
    fn dependent_groups_become_full_alternatives_with_common_resources() {
        // A common IRQ plus two dependent DMA choices, each with priority 1.
        let bytes = [0x22, 0x10, 0x00, 0x31, 0x01, 0x2a, 0x01, 0x00,
                     0x38, 0x31, 0x01, 0x2a, 0x04, 0x00, 0x38, 0x79, 0x00];
        let parsed = parse_resource_requirements_template(&bytes).unwrap();
        assert_eq!(parsed.count, 2);
        assert_eq!(parsed.entries[0].count, 2);
        assert_eq!(parsed.entries[1].count, 2);
        assert!(matches!(parsed.entries[0].entries[0], Some(Resource::Irq { mask: 0x10, .. })));
        assert!(matches!(parsed.entries[0].entries[1], Some(Resource::Dma { channels: 1, .. })));
        assert!(matches!(parsed.entries[1].entries[1], Some(Resource::Dma { channels: 4, .. })));
    }

    #[test]
    fn rejects_unbalanced_dependent_groups() {
        assert_eq!(parse_resource_requirements_template(&[0x38, 0x79, 0]), Err(ResourceError::InvalidDescriptor));
        assert_eq!(parse_resource_requirements_template(&[0x31, 1, 0x79, 0]), Err(ResourceError::InvalidDescriptor));
    }
}


#[cfg(test)] mod stability_21_end_tag_checksum {
 use super::*;
 #[test] fn rejects_bad_nonzero_end_tag_checksum() {
  assert_eq!(parse_resource_template(&[0x79, 1]), Err(ResourceError::InvalidDescriptor));
 }
}


#[cfg(test)] mod stability_22_unknown_small_descriptor {
 use super::*;
 #[test] fn rejects_unknown_small_descriptor_without_skipping_it() {
  assert_eq!(parse_resource_template(&[0x30, 0x79, 0]), Err(ResourceError::UnsupportedDescriptor));
 }
}


#[cfg(test)] mod stability_23_truncated_large_header {
 use super::*;
 #[test] fn rejects_large_header_missing_length_byte() {
  assert_eq!(parse_resource_template(&[0x8a, 0x2b]), Err(ResourceError::Truncated));
 }
}


#[cfg(test)] mod stability_24_extended_irq_length {
 use super::*;
 #[test] fn rejects_extended_irq_count_that_exceeds_payload() {
  assert_eq!(parse_resource_template(&[0x89, 7, 0, 0, 1, 1, 0, 0, 0, 0x79, 0]), Err(ResourceError::InvalidLength));
 }
}


#[cfg(test)] mod stability_25_empty_extended_irq {
 use super::*;
 #[test] fn rejects_extended_irq_with_zero_interrupts() {
  assert_eq!(parse_resource_template(&[0x89, 6, 0, 0, 0, 0, 0, 0, 0x79, 0]), Err(ResourceError::InvalidLength));
 }
}


#[cfg(test)] mod stability_26_extended_irq_bound {
 use super::*;
 #[test] fn rejects_more_than_eight_extended_interrupts() {
  let mut bytes = std::vec![0x89, 38, 0, 0, 9];
  bytes.extend_from_slice(&[0; 36]);
  bytes.extend_from_slice(&[0x79, 0]);
  assert_eq!(parse_resource_template(&bytes), Err(ResourceError::InvalidLength));
 }
}


#[cfg(test)] mod stability_27_dword_address_range {
 use super::*;
 #[test] fn parses_dword_memory_aperture_at_exact_boundaries() {
  let mut bytes = [0u8; 28];
  bytes[0] = 0x87; bytes[1] = 23;
  bytes[10..14].copy_from_slice(&0x1000u32.to_le_bytes());
  bytes[14..18].copy_from_slice(&0x1fffu32.to_le_bytes());
  bytes[22..26].copy_from_slice(&0x1000u32.to_le_bytes());
  bytes[26] = 0x79; bytes[27] = 0;
  let parsed = parse_resource_template(&bytes).unwrap();
  assert!(matches!(parsed.entries[0], Some(Resource::Memory { minimum: 0x1000, maximum: 0x1fff, length: 0x1000, kind: 1, .. })));
 }
}


#[cfg(test)] mod stability_28_qword_overflow {
 use super::*;
 #[test] fn rejects_qword_range_whose_end_wraps() {
  let mut bytes = [0u8; 48];
  bytes[0] = 0x8a; bytes[1] = 43;
  bytes[14..22].copy_from_slice(&(u64::MAX - 1).to_le_bytes());
  bytes[22..30].copy_from_slice(&u64::MAX.to_le_bytes());
  bytes[38..46].copy_from_slice(&4u64.to_le_bytes());
  bytes[46] = 0x79; bytes[47] = 0;
  assert_eq!(parse_resource_template(&bytes), Err(ResourceError::InvalidDescriptor));
 }
}


#[cfg(test)] mod stability_29_resource_capacity {
 use super::*;
 #[test] fn rejects_more_than_max_resources() {
  let mut bytes = std::vec::Vec::new();
  for _ in 0..=MAX_RESOURCES { bytes.extend_from_slice(&[0x4b, 0x78, 0x03, 3]); }
  bytes.extend_from_slice(&[0x79, 0]);
  assert_eq!(parse_resource_template(&bytes), Err(ResourceError::TooManyResources));
 }
}


#[cfg(test)] mod stability_30_end_tag_trailing_data {
 use super::*;
 #[test] fn rejects_bytes_after_end_tag_even_when_checksum_is_zero() {
  assert_eq!(parse_resource_template(&[0x79, 0, 0]), Err(ResourceError::InvalidLength));
 }
}


#[cfg(test)] mod acpi_v6_extended_irq_fixture {
 use super::*;
 
 #[test] fn parses_extended_irq_gsi_and_flags() {
  let bytes=[0x89,6,0,0,1,24,0,0,0,0x79,0];
  let parsed=parse_resource_template(&bytes).unwrap();
  assert_eq!(parsed.count,2);
  assert!(matches!(parsed.entries[0],Some(Resource::ExtendedIrq{flags:0,count:1,interrupts,..}) if interrupts[0]==24));
 }

}


#[cfg(test)] mod acpi_v6_qword_bus_number {
 use super::*;
 
 #[test] fn parses_qword_bus_number_window() {
  let mut bytes=[0u8;48]; bytes[0]=0x8a; bytes[1]=43; bytes[2]=2;
  bytes[14..22].copy_from_slice(&0x20u64.to_le_bytes());
  bytes[22..30].copy_from_slice(&0x2fu64.to_le_bytes());
  bytes[30..38].copy_from_slice(&1u64.to_le_bytes());
  bytes[38..46].copy_from_slice(&0x10u64.to_le_bytes());
  bytes[46]=0x79;
  let parsed=parse_resource_template(&bytes).unwrap();
  assert!(matches!(parsed.entries[0],Some(Resource::BusNumber{minimum:0x20,maximum:0x2f,length:0x10})));
 }

}

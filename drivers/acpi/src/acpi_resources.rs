//! Bounded ACPI resource-template decoding for `_CRS`.
//! This parser validates firmware requirements; it does not allocate or translate them.
#![no_std]

pub const MAX_RESOURCES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError { MissingEndTag, Truncated, InvalidLength, InvalidDescriptor, TooManyResources, IntegerOverflow, UnsupportedDescriptor }

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
    if len==0||min>max||min.checked_add(len-1).filter(|end|*end<=max).is_none(){return Err(ResourceError::InvalidDescriptor)} Ok(())
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
        0=>Ok(Resource::Memory{minimum:min,maximum:max,alignment:align,length:len,writeable:type_flags&1!=0,cacheable:(type_flags>>1)&3!=0,kind:width}),
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
                    validate_range(base,base,size)?;Resource::Io{minimum:base,maximum:base,alignment:1,length:size,decode_16_bit:p[2]&1!=0}},
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
                0x06 if len==9=>{let base=u32le(p,1)? as u64;let size=u32le(p,5)? as u64;let max=base.checked_add(size.checked_sub(1).ok_or(ResourceError::InvalidDescriptor)?).ok_or(ResourceError::IntegerOverflow)?;
                    validate_range(base,max,size)?;Resource::Memory{minimum:base,maximum:max,alignment:0,length:size,writeable:p[0]&1!=0,cacheable:false,kind:2}},
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
                0x0c if len==3=>{let base=u16le(p,0)? as u64;let size=p[2] as u64;let max=base.checked_add(size.checked_sub(1).ok_or(ResourceError::InvalidDescriptor)?).ok_or(ResourceError::IntegerOverflow)?;
                    validate_range(base,max,size)?;Resource::Io{minimum:base,maximum:max,alignment:1,length:size,decode_16_bit:true}},
                _=>return Err(ResourceError::UnsupportedDescriptor),
            };
            out.push(item)?;pos=end;
        }
    }
    if !ended{return Err(ResourceError::MissingEndTag)} Ok(out)
}

#[cfg(test)] extern crate std;
#[cfg(test)] mod tests{
 use super::*;
 #[test]fn parses_io_irq_and_end_tag(){let data=[0x47,0x01,0x00,0x01,0x01,0x08,0x0b,0x22,0x00];let r=parse_resource_template(&data).unwrap();
  assert_eq!(r.count,3);assert!(matches!(r.entries[0],Some(Resource::Io{minimum:0x100,maximum:0x100,length:8,..})));
  assert!(matches!(r.entries[1],Some(Resource::Irq{mask:0x0b,..})));}
 #[test]fn parses_fixed_io_descriptor(){let parsed=parse_resource_template(&[0x4b,0x78,0x03,0x01,0x79,0]);assert!(matches!(parsed.unwrap().entries[0],Some(Resource::Io{minimum:0x378,maximum:0x378,length:3,..})));}
 #[test]fn rejects_missing_end_tag(){assert_eq!(parse_resource_template(&[0x47,1,0,1,1,8,0]),Err(ResourceError::MissingEndTag));}
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

//! ACPI-2 resource-template decoding.
//!
//! Decodes the resource descriptors used by _CRS into a bounded, typed list.
//! This module does not allocate resources: ownership/arbitration remains with
//! the parent bus (notably PCI) and the platform resource manager.
#![no_std]

pub const MAX_RESOURCES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    MissingEndTag, Truncated, InvalidLength, InvalidDescriptor,
    TooManyResources, IntegerOverflow, UnsupportedDescriptor,
}

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
pub struct ResourceList {
    pub entries:[Option<Resource>;MAX_RESOURCES],
    pub count:usize,
}
impl ResourceList {
    pub const EMPTY:Self=Self{entries:[None;MAX_RESOURCES],count:0};
    fn push(&mut self, item:Resource)->Result<(),ResourceError>{
        if self.count==MAX_RESOURCES{return Err(ResourceError::TooManyResources)}
        self.entries[self.count]=Some(item);self.count+=1;Ok(())
    }
}

fn u16le(b:&[u8],o:usize)->Result<u16,ResourceError>{
    let s=b.get(o..o.checked_add(2).ok_or(ResourceError::IntegerOverflow)?).ok_or(ResourceError::Truncated)?;
    Ok(u16::from_le_bytes([s[0],s[1]]))
}
fn u32le(b:&[u8],o:usize)->Result<u32,ResourceError>{
    let s=b.get(o..o.checked_add(4).ok_or(ResourceError::IntegerOverflow)?).ok_or(ResourceError::Truncated)?;
    Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))
}
fn u64le(b:&[u8],o:usize)->Result<u64,ResourceError>{
    let s=b.get(o..o.checked_add(8).ok_or(ResourceError::IntegerOverflow)?).ok_or(ResourceError::Truncated)?;
    Ok(u64::from_le_bytes([s[0],s[1],s[2],s[3],s[4],s[5],s[6],s[7]]))
}
fn add_range(min:u64,max:u64,len:u64)->Result<(),ResourceError>{
    if len==0 || min>max || min.checked_add(len-1).filter(|end|*end<=max).is_none(){return Err(ResourceError::InvalidDescriptor)}
    Ok(())
}

/// Parse a complete ACPI resource template. Descriptor payloads are validated
/// before any result is published; an EndTag is mandatory and must be final.
/// Unknown descriptors are rejected rather than being misrepresented as empty.
pub fn parse_resource_template(bytes:&[u8])->Result<ResourceList,ResourceError>{
    let mut result=ResourceList::EMPTY;
    let mut pos=0usize;
    let mut ended=false;
    while pos<bytes.len(){
        let tag=bytes[pos];pos+=1;
        if tag&0x80==0 {
            let kind=(tag>>3)&0x0f;
            let len=(tag&7) as usize;
            let end=pos.checked_add(len).filter(|v|*v<=bytes.len()).ok_or(ResourceError::Truncated)?;
            let p=&bytes[pos..end];
            let item=match kind {
                0x04 if len==2 || len==3 => {
                    let mask=u16le(p,0)?;
                    let flags=if len==3{p[2]}else{0};
                    Resource::Irq{mask,level_triggered:flags&1!=0,active_low:flags&8!=0,shared:flags&0x10!=0,wake_capable:flags&0x20!=0}
                }
                0x05 if len==2 => Resource::Dma{channels:p[0],transfer_size:p[1]&3,bus_master:p[1]&4!=0},
                0x08 if len==7 => {
                    let min=u16le(p,1)? as u64;let max=u16le(p,3)? as u64;
                    let align=p[5] as u64;let size=p[6] as u64;
                    add_range(min,max,size)?;
                    Resource::Io{minimum:min,maximum:max,alignment:align,length:size,decode_16_bit:p[0]&1!=0}
                }
                0x0f if len==1 => {result.push(Resource::EndTag{checksum:p[0]})?;pos=end;ended=true;if pos!=bytes.len(){return Err(ResourceError::InvalidLength)}break;}
                _=>return Err(ResourceError::UnsupportedDescriptor),
            };
            result.push(item)?;pos=end;
        } else {
            let kind=tag&0x7f;
            let len=u16le(bytes,pos)? as usize;pos=pos.checked_add(2).ok_or(ResourceError::IntegerOverflow)?;
            let end=pos.checked_add(len).filter(|v|*v<=bytes.len()).ok_or(ResourceError::Truncated)?;
            let p=&bytes[pos..end];
            let item=match kind {
                0x01 if len==9 => { // 24-bit memory range
                    let min=u16le(p,1)? as u64;let max=u16le(p,3)? as u64;
                    let align=u16le(p,5)? as u64;let size=u16le(p,7)? as u64;
                    add_range(min,max,size)?;
                    Resource::Memory{minimum:min,maximum:max,alignment:align,length:size,writeable:p[0]&1!=0,cacheable:false,kind:0}
                }
                0x05 if len==17 => { // 32-bit memory range
                    let min=u32le(p,1)? as u64;let max=u32le(p,5)? as u64;
                    let align=u32le(p,9)? as u64;let size=u32le(p,13)? as u64;
                    add_range(min,max,size)?;
                    Resource::Memory{minimum:min,maximum:max,alignment:align,length:size,writeable:p[0]&1!=0,cacheable:false,kind:0}
                }
                0x06 if len==9 => { // 32-bit fixed memory
                    let base=u32le(p,1)? as u64;let size=u32le(p,5)? as u64;
                    let max=base.checked_add(size.saturating_sub(1)).ok_or(ResourceError::IntegerOverflow)?;
                    add_range(base,max,size)?;
                    Resource::Memory{minimum:base,maximum:max,alignment:0,length:size,writeable:p[0]&1!=0,cacheable:false,kind:1}
                }
                0x07 if len==23 => { // 64-bit memory range
                    let min=u64le(p,3)?;let max=u64le(p,11)?;let align=u64le(p,19)?;
                    // The descriptor has no length field; derive it from its range.
                    if min>max{return Err(ResourceError::InvalidDescriptor)}
                    let size=max.checked_sub(min).and_then(|v|v.checked_add(1)).ok_or(ResourceError::IntegerOverflow)?;
                    Resource::Memory{minimum:min,maximum:max,alignment:align,length:size,writeable:p[0]&1!=0,cacheable:false,kind:2}
                }
                0x08 if len==12 => { // 64-bit address-space descriptor
                    let space=p[0];let flags=p[1];let granularity=u64le(p,2)?;
                    let min=u64le(p,10.min(p.len().saturating_sub(8)))?; // replaced below by strict layout parsing
                    let _=(space,flags,granularity,min);
                    return Err(ResourceError::UnsupportedDescriptor)
                }
                0x09 if len==3 => { // Extended IRQ
                    let flags=p[0];let count=p[1] as usize;
                    if count==0||count>8||len!=2+count*4{return Err(ResourceError::InvalidLength)}
                    let mut ints=[0u32;8];for i in 0..count{ints[i]=u32le(p,2+i*4)?;}
                    Resource::ExtendedIrq{flags,interrupts:ints,count:count as u8}
                }
                0x0a if len==6 => { // Generic Register: not a bus allocation resource
                    return Err(ResourceError::UnsupportedDescriptor)
                }
                0x0c if len==4 => { // Fixed I/O port range
                    let base=u16le(p,0)? as u64;let size=p[2] as u64;
                    let max=base.checked_add(size.saturating_sub(1)).ok_or(ResourceError::IntegerOverflow)?;
                    add_range(base,max,size)?;
                    Resource::Io{minimum:base,maximum:max,alignment:1,length:size,decode_16_bit:true}
                }
                0x0d if len==5 => { // Fixed DMA, conservatively expose channels
                    Resource::Dma{channels:p[0],transfer_size:p[1]&3,bus_master:p[1]&4!=0}
                }
                0x0e if len==2 => { // Word Address Space: bus-number range only
                    return Err(ResourceError::UnsupportedDescriptor)
                }
                0x0f if len==9 => { // Extended Address Space: bus-number resource when space=2
                    let space=p[0];
                    if space!=2{return Err(ResourceError::UnsupportedDescriptor)}
                    let min=u64le(p,3)?;let max=u64le(p,11)?;
                    if min>max{return Err(ResourceError::InvalidDescriptor)}
                    let length=max.checked_sub(min).and_then(|v|v.checked_add(1)).ok_or(ResourceError::IntegerOverflow)?;
                    Resource::BusNumber{minimum:min,maximum:max,length}
                }
                _=>return Err(ResourceError::UnsupportedDescriptor),
            };
            result.push(item)?;pos=end;
        }
    }
    if !ended{return Err(ResourceError::MissingEndTag)}
    Ok(result)
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parses_io_irq_and_end_tag(){
        let data=[0x47,0x01,0x00,0x01,0x01,0x08,0x0b,0x22,0x00];
        let parsed=parse_resource_template(&data).unwrap();
        assert_eq!(parsed.count,3);
        assert!(matches!(parsed.entries[0],Some(Resource::Io{minimum:0x100,maximum:0x100,length:8,..})));
        assert!(matches!(parsed.entries[1],Some(Resource::Irq{mask:0x0b,..})));
    }
    #[test] fn rejects_missing_end_tag(){assert_eq!(parse_resource_template(&[0x47,1,0,1,1,8,0]),Err(ResourceError::MissingEndTag));}
    #[test] fn rejects_truncated_descriptor(){assert_eq!(parse_resource_template(&[0x47,1]),Err(ResourceError::Truncated));}
    #[test] fn rejects_zero_length_range(){assert_eq!(parse_resource_template(&[0x47,1,0,1,1,0,0x79,0]),Err(ResourceError::InvalidDescriptor));}
}

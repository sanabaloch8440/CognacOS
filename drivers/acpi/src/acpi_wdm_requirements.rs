//! WDM IO_RESOURCE_REQUIREMENTS_LIST serialization for ACPI _PRS templates.
//! One parsed template becomes one alternative; arbitration and _SRS programming
//! remain owned by kernel PnP/resource-arbiter layers.
/*
 * FIXME(ACPI-PRODUCTION) — WDM resource requirements serialization
 * FIXME(ACPI-PRODUCTION): Complete IO_RESOURCE_REQUIREMENTS_LIST serialization for every supported ACPI alternative and descriptor, with correct option/priority flags, alignment, ABI layout, and checked sizing. Test empty, maximal, conflicting, and malformed alternatives against kernel consumers.
 */

#![no_std]
use crate::acpi_resources::{Resource,ResourceList};
pub const IO_RESOURCE_DESCRIPTOR_SIZE:usize=32;
pub const REQUIREMENTS_HEADER_SIZE:usize=32;
pub const ALTERNATIVE_HEADER_SIZE:usize=8;
const TYPE_PORT:u8=1;const TYPE_INTERRUPT:u8=2;const TYPE_MEMORY:u8=3;const TYPE_DMA:u8=4;const TYPE_BUS_NUMBER:u8=6;
const SHARE_EXCLUSIVE:u8=1;const SHARE_SHARED:u8=3;
const PORT_IO:u16=1;const PORT_16_BIT_DECODE:u16=2;const PORT_10_BIT_DECODE:u16=4;
const MEMORY_READ_ONLY:u16=1;const MEMORY_CACHEABLE:u16=0x20;
const INTERRUPT_LEVEL_SENSITIVE:u16=1;const IO_RESOURCE_ALTERNATIVE:u8=0x08;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RequirementsError{SizeOverflow,OutputTooSmall,InvalidResourceList,UnsupportedRange}
/// Count WDM IO_RESOURCE_DESCRIPTOR records after validating the ACPI resource list shape.
pub fn descriptor_count(r:&ResourceList)->Result<usize,RequirementsError>{
 if r.count>r.entries.len(){return Err(RequirementsError::InvalidResourceList)}
 let mut n=0usize;for item in r.entries[..r.count].iter().flatten(){let add=match item{
 Resource::Io{..}|Resource::Memory{..}|Resource::BusNumber{..}=>1,
 Resource::Irq{mask,..}=>mask.count_ones() as usize,
 Resource::ExtendedIrq{count,..}=>{if *count>8{return Err(RequirementsError::InvalidResourceList)}*count as usize},
 Resource::Dma{channels,..}=>channels.count_ones() as usize,Resource::EndTag{..}=>0,
 };n=n.checked_add(add).ok_or(RequirementsError::SizeOverflow)?;}Ok(n)
}
/// Compute the exact serialized IO_RESOURCE_REQUIREMENTS_LIST size with checked arithmetic.
pub fn required_size(r:&ResourceList)->Result<usize,RequirementsError>{
 let count=descriptor_count(r)?;
 REQUIREMENTS_HEADER_SIZE.checked_add(ALTERNATIVE_HEADER_SIZE).and_then(|n|n.checked_add(count.checked_mul(IO_RESOURCE_DESCRIPTOR_SIZE)?)).ok_or(RequirementsError::SizeOverflow)
}
unsafe fn w16(p:*mut u8,o:usize,v:u16){unsafe{p.add(o).cast::<u16>().write_unaligned(v)}}
unsafe fn w32(p:*mut u8,o:usize,v:u32){unsafe{p.add(o).cast::<u32>().write_unaligned(v)}}
unsafe fn w64(p:*mut u8,o:usize,v:u64){unsafe{p.add(o).cast::<u64>().write_unaligned(v)}}
unsafe fn header(p:*mut u8,i:usize,kind:u8,share:u8,flags:u16,option:u8){let d=unsafe{p.add(i*IO_RESOURCE_DESCRIPTOR_SIZE)};unsafe{d.write(option);d.add(1).write(kind);d.add(2).write(share);d.add(3).write(0);w16(d,4,flags);w16(d,6,0);}}
unsafe fn address(p:*mut u8,i:usize,kind:u8,flags:u16,len:u64,align:u64,min:u64,max:u64){let d=unsafe{p.add(i*IO_RESOURCE_DESCRIPTOR_SIZE)};unsafe{header(p,i,kind,SHARE_EXCLUSIVE,flags,0);w32(d,8,len as u32);w32(d,12,align.max(1) as u32);w64(d,16,min);w64(d,24,max);}}
unsafe fn pair(p:*mut u8,i:usize,kind:u8,share:u8,flags:u16,min:u32,max:u32,alternative:bool){let d=unsafe{p.add(i*IO_RESOURCE_DESCRIPTOR_SIZE)};unsafe{header(p,i,kind,share,flags,if alternative{IO_RESOURCE_ALTERNATIVE}else{0});w32(d,8,min);w32(d,12,max);}}
/// Serialize one _PRS template as one IO_RESOURCE_LIST alternative.
pub unsafe fn serialize(r:&ResourceList,output:*mut u8,capacity:usize)->Result<usize,RequirementsError>{
 let needed=required_size(r)?;if output.is_null()||capacity<needed{return Err(RequirementsError::OutputTooSmall)}let count=descriptor_count(r)?;
 for item in r.entries[..r.count].iter().flatten(){match *item{
 Resource::Io{minimum,maximum,length,alignment,..}|Resource::Memory{minimum,maximum,length,alignment,..}=>if minimum>maximum||length==0||length>u32::MAX as u64||alignment>u32::MAX as u64||maximum.checked_add(length-1).is_none(){return Err(RequirementsError::UnsupportedRange)},
 Resource::BusNumber{minimum,maximum,length}=>if minimum>maximum||minimum>u32::MAX as u64||maximum>u32::MAX as u64||length==0||length>u32::MAX as u64{return Err(RequirementsError::UnsupportedRange)},
 Resource::ExtendedIrq{count,..} if count>8=>return Err(RequirementsError::InvalidResourceList),_=>{}
 }}
 unsafe{core::ptr::write_bytes(output,0,needed);w32(output,0,needed as u32);w32(output,4,15);w32(output,8,0);w32(output,12,0);w32(output,16,0);w32(output,20,0);w32(output,24,0);w32(output,28,1);}
 let alt=unsafe{output.add(REQUIREMENTS_HEADER_SIZE)};unsafe{w16(alt,0,1);w16(alt,2,1);w32(alt,4,count as u32);}
 let p=unsafe{alt.add(ALTERNATIVE_HEADER_SIZE)};let mut i=0usize;
 for item in r.entries[..r.count].iter().flatten(){match *item{
 Resource::Io{minimum,maximum,length,alignment,decode_16_bit}=>unsafe{address(p,i,TYPE_PORT,PORT_IO|if decode_16_bit{PORT_16_BIT_DECODE}else{PORT_10_BIT_DECODE},length,alignment,minimum,maximum);i+=1;},
 Resource::Memory{minimum,maximum,length,alignment,writeable,cacheable,..}=>unsafe{address(p,i,TYPE_MEMORY,(if writeable{0}else{MEMORY_READ_ONLY})|(if cacheable{MEMORY_CACHEABLE}else{0}),length,alignment,minimum,maximum);i+=1;},
 Resource::BusNumber{minimum,maximum,length}=>{let d=unsafe{p.add(i*IO_RESOURCE_DESCRIPTOR_SIZE)};unsafe{header(p,i,TYPE_BUS_NUMBER,SHARE_EXCLUSIVE,0,0);w32(d,8,length as u32);w32(d,12,minimum as u32);w32(d,16,maximum as u32);}i+=1;},
 Resource::Irq{mask,level_triggered,shared,..}=>{let mut choice=0usize;for vector in 0..16u32{if mask&(1u16<<vector)!=0{unsafe{pair(p,i,TYPE_INTERRUPT,if shared{SHARE_SHARED}else{SHARE_EXCLUSIVE},if level_triggered{INTERRUPT_LEVEL_SENSITIVE}else{0},vector,vector,choice!=0);}i+=1;choice+=1;}}},
 Resource::ExtendedIrq{flags,interrupts,count}=>{let level=flags&2!=0;let shared=flags&8!=0;for (choice,vector) in interrupts[..count as usize].iter().enumerate(){unsafe{pair(p,i,TYPE_INTERRUPT,if shared{SHARE_SHARED}else{SHARE_EXCLUSIVE},if level{INTERRUPT_LEVEL_SENSITIVE}else{0},*vector,*vector,choice!=0);}i+=1;}},
 Resource::Dma{channels,..}=>{let mut choice=0usize;for channel in 0..8u32{if channels&(1u8<<channel)!=0{unsafe{pair(p,i,TYPE_DMA,SHARE_EXCLUSIVE,0,channel,channel,choice!=0);}i+=1;choice+=1;}}},
 Resource::EndTag{..}=>{}
 }}
 if i!=count{return Err(RequirementsError::InvalidResourceList)}Ok(needed)
}
#[cfg(test)]extern crate std;
#[cfg(test)]mod tests{use super::*;use crate::acpi_resources::Resource;
#[test]fn serializes_ranges_and_irq_alternative(){let mut r=ResourceList::EMPTY;r.entries[0]=Some(Resource::Io{minimum:0x300,maximum:0x3f8,alignment:8,length:8,decode_16_bit:true});r.entries[1]=Some(Resource::Memory{minimum:0xfed00000,maximum:0xfed01000,alignment:0x1000,length:0x1000,writeable:true,cacheable:false,kind:8});r.entries[2]=Some(Resource::Irq{mask:(1<<4)|(1<<7),level_triggered:true,active_low:false,shared:true,wake_capable:false});r.count=3;let mut b=[0u8;168];assert_eq!(unsafe{serialize(&r,b.as_mut_ptr(),b.len())},Ok(b.len()));assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()),b.len() as u32);assert_eq!(u32::from_le_bytes(b[28..32].try_into().unwrap()),1);assert_eq!(u32::from_le_bytes(b[36..40].try_into().unwrap()),4);assert_eq!(b[41],TYPE_PORT);assert_eq!(u64::from_le_bytes(b[56..64].try_into().unwrap()),0x300);assert_eq!(u64::from_le_bytes(b[64..72].try_into().unwrap()),0x3f8);assert_eq!(b[73],TYPE_MEMORY);assert_eq!(b[105],TYPE_INTERRUPT);assert_eq!(b[106],SHARE_SHARED);assert_eq!(b[104],0);assert_eq!(b[136],IO_RESOURCE_ALTERNATIVE);assert_eq!(u32::from_le_bytes(b[112..116].try_into().unwrap()),4);}
#[test]fn rejects_short_output_without_mutating(){let r=ResourceList::EMPTY;let mut b=[0xa5;32];assert_eq!(unsafe{serialize(&r,b.as_mut_ptr(),b.len())},Err(RequirementsError::OutputTooSmall));assert_eq!(b,[0xa5;32]);}
#[test]fn rejects_address_overflow(){let mut r=ResourceList::EMPTY;r.entries[0]=Some(Resource::Memory{minimum:u64::MAX,maximum:u64::MAX,alignment:1,length:2,writeable:true,cacheable:false,kind:8});r.count=1;let mut b=[0u8;72];assert_eq!(unsafe{serialize(&r,b.as_mut_ptr(),b.len())},Err(RequirementsError::UnsupportedRange));}
}

/// Size of an IO_RESOURCE_REQUIREMENTS_LIST containing every complete
/// firmware configuration. Each configuration is a separate IO_RESOURCE_LIST.
pub fn required_size_alternatives(r:&crate::acpi_resources::ResourceAlternatives)->Result<usize,RequirementsError>{
 if r.count==0||r.count>r.entries.len(){return Err(RequirementsError::InvalidResourceList)}
 let mut total=REQUIREMENTS_HEADER_SIZE;
 for list in r.entries[..r.count].iter(){
  let n=descriptor_count(list)?;
  total=total.checked_add(ALTERNATIVE_HEADER_SIZE).and_then(|v|v.checked_add(n.checked_mul(IO_RESOURCE_DESCRIPTOR_SIZE)?)).ok_or(RequirementsError::SizeOverflow)?;
 }
 Ok(total)
}

/// Serialize all _PRS configurations into the variable-length WDM list.
/// The resource arbiter, not this driver, chooses one of these alternatives.
pub unsafe fn serialize_alternatives(
 r:&crate::acpi_resources::ResourceAlternatives,
 output:*mut u8,
 capacity:usize,
)->Result<usize,RequirementsError>{
 let needed=required_size_alternatives(r)?;
 if output.is_null()||capacity<needed{return Err(RequirementsError::OutputTooSmall)}
 // Validate every descriptor before mutating caller memory.
 for list in r.entries[..r.count].iter(){
  if list.count>list.entries.len(){return Err(RequirementsError::InvalidResourceList)}
  for item in list.entries[..list.count].iter().flatten(){match *item{
   Resource::Io{minimum,maximum,length,alignment,..}|Resource::Memory{minimum,maximum,length,alignment,..}=>if minimum>maximum||length==0||length>u32::MAX as u64||alignment>u32::MAX as u64||maximum.checked_add(length-1).is_none(){return Err(RequirementsError::UnsupportedRange)},
   Resource::BusNumber{minimum,maximum,length}=>if minimum>maximum||minimum>u32::MAX as u64||maximum>u32::MAX as u64||length==0||length>u32::MAX as u64{return Err(RequirementsError::UnsupportedRange)},
   Resource::ExtendedIrq{count,..} if count>8=>return Err(RequirementsError::InvalidResourceList),_=>{}
  }}
 }
 unsafe{core::ptr::write_bytes(output,0,needed);w32(output,0,needed as u32);w32(output,4,15);w32(output,8,0);w32(output,12,0);w32(output,16,0);w32(output,20,0);w32(output,24,0);w32(output,28,r.count as u32);}
 let mut offset=REQUIREMENTS_HEADER_SIZE;
 for list in r.entries[..r.count].iter(){
  let count=descriptor_count(list)?;
  let alt=unsafe{output.add(offset)};
  unsafe{w16(alt,0,1);w16(alt,2,1);w32(alt,4,count as u32);}
  let p=unsafe{alt.add(ALTERNATIVE_HEADER_SIZE)};
  let mut i=0usize;
  for item in list.entries[..list.count].iter().flatten(){match *item{
   Resource::Io{minimum,maximum,length,alignment,decode_16_bit}=>unsafe{address(p,i,TYPE_PORT,PORT_IO|if decode_16_bit{PORT_16_BIT_DECODE}else{PORT_10_BIT_DECODE},length,alignment,minimum,maximum);i+=1;},
   Resource::Memory{minimum,maximum,length,alignment,writeable,cacheable,..}=>unsafe{address(p,i,TYPE_MEMORY,(if writeable{0}else{MEMORY_READ_ONLY})|(if cacheable{MEMORY_CACHEABLE}else{0}),length,alignment,minimum,maximum);i+=1;},
   Resource::BusNumber{minimum,maximum,length}=>{let d=unsafe{p.add(i*IO_RESOURCE_DESCRIPTOR_SIZE)};unsafe{header(p,i,TYPE_BUS_NUMBER,SHARE_EXCLUSIVE,0,0);w32(d,8,length as u32);w32(d,12,minimum as u32);w32(d,16,maximum as u32);}i+=1;},
   Resource::Irq{mask,level_triggered,shared,..}=>{let mut choice=0usize;for vector in 0..16u32{if mask&(1u16<<vector)!=0{unsafe{pair(p,i,TYPE_INTERRUPT,if shared{SHARE_SHARED}else{SHARE_EXCLUSIVE},if level_triggered{INTERRUPT_LEVEL_SENSITIVE}else{0},vector,vector,choice!=0);}i+=1;choice+=1;}}},
   Resource::ExtendedIrq{flags,interrupts,count}=>{let level=flags&2!=0;let shared=flags&8!=0;for (choice,vector) in interrupts[..count as usize].iter().enumerate(){unsafe{pair(p,i,TYPE_INTERRUPT,if shared{SHARE_SHARED}else{SHARE_EXCLUSIVE},if level{INTERRUPT_LEVEL_SENSITIVE}else{0},*vector,*vector,choice!=0);}i+=1;}},
   Resource::Dma{channels,..}=>{let mut choice=0usize;for channel in 0..8u32{if channels&(1u8<<channel)!=0{unsafe{pair(p,i,TYPE_DMA,SHARE_EXCLUSIVE,0,channel,channel,choice!=0);}i+=1;choice+=1;}}},
   Resource::EndTag{..}=>{}
  }}
  if i!=count{return Err(RequirementsError::InvalidResourceList)}
  offset=offset.checked_add(ALTERNATIVE_HEADER_SIZE).and_then(|v|v.checked_add(count.checked_mul(IO_RESOURCE_DESCRIPTOR_SIZE)?)).ok_or(RequirementsError::SizeOverflow)?;
 }
 Ok(needed)
}

#[cfg(test)]
mod multiple_alternative_tests{
 use super::*;
 use crate::acpi_resources::{Resource,ResourceAlternatives,ResourceList};
 #[test]fn emits_multiple_complete_io_resource_lists(){
  let mut alternatives=ResourceAlternatives::EMPTY;
  for channel in [1u8,4u8]{
   let mut list=ResourceList::EMPTY;
   list.entries[0]=Some(Resource::Dma{channels:channel,transfer_size:0,bus_master:false});list.count=1;
   alternatives.entries[alternatives.count]=list;alternatives.count+=1;
  }
  let size=required_size_alternatives(&alternatives).unwrap();assert_eq!(size,112);
  let mut bytes=[0u8;112];
  assert_eq!(unsafe{serialize_alternatives(&alternatives,bytes.as_mut_ptr(),bytes.len())},Ok(112));
  assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()),2);
  assert_eq!(u32::from_le_bytes(bytes[36..40].try_into().unwrap()),1);
  assert_eq!(u32::from_le_bytes(bytes[76..80].try_into().unwrap()),1);
  assert_eq!(u32::from_le_bytes(bytes[48..52].try_into().unwrap()),1);
  assert_eq!(u32::from_le_bytes(bytes[88..92].try_into().unwrap()),4);
 }
 #[test]fn rejects_empty_alternative_set(){assert_eq!(required_size_alternatives(&ResourceAlternatives::EMPTY),Err(RequirementsError::InvalidResourceList));}
}

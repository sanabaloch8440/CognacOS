//! Windows x64 CM_RESOURCE_LIST serialization for ACPI raw bus resources.
//! ACPI reports bus-relative values; kernel PnP and HAL own arbitration and translation.
#![no_std]
use crate::acpi_resources::{Resource,ResourceList};
pub const PARTIAL_DESCRIPTOR_SIZE:usize=16;
const TYPE_PORT:u8=1;const TYPE_INTERRUPT:u8=2;const TYPE_MEMORY:u8=3;const TYPE_DMA:u8=4;const TYPE_BUS_NUMBER:u8=6;
const SHARE_EXCLUSIVE:u8=1;const SHARE_SHARED:u8=3;const PORT_IO:u16=1;const MEMORY_READ_ONLY:u16=1;const MEMORY_CACHEABLE:u16=0x20;const INTERRUPT_LATCHED:u16=1;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SerializeError{TooManyDescriptors,SizeOverflow,OutputTooSmall,InvalidResourceList}
pub fn descriptor_count(r:&ResourceList)->Result<usize,SerializeError>{
 if r.count>r.entries.len(){return Err(SerializeError::InvalidResourceList)}
 let mut n=0usize;for item in r.entries[..r.count].iter().flatten(){let add=match item{
 Resource::Io{..}|Resource::Memory{..}|Resource::BusNumber{..}=>1,Resource::Irq{mask,..}=>mask.count_ones() as usize,
 Resource::ExtendedIrq{count,..}=>*count as usize,Resource::Dma{channels,..}=>channels.count_ones() as usize,Resource::EndTag{..}=>0};
 n=n.checked_add(add).ok_or(SerializeError::SizeOverflow)?;}Ok(n)
}
pub fn required_size(r:&ResourceList)->Result<usize,SerializeError>{4usize.checked_add(16).and_then(|n|n.checked_add(descriptor_count(r).ok()?.checked_mul(16)?)).ok_or(SerializeError::SizeOverflow)}
unsafe fn w16(p:*mut u8,o:usize,v:u16){unsafe{p.add(o).cast::<u16>().write_unaligned(v)}}
unsafe fn w32(p:*mut u8,o:usize,v:u32){unsafe{p.add(o).cast::<u32>().write_unaligned(v)}}
unsafe fn w64(p:*mut u8,o:usize,v:u64){unsafe{p.add(o).cast::<u64>().write_unaligned(v)}}
unsafe fn desc(out:*mut u8,i:usize,t:u8,s:u8,f:u16,a:u64,b:u64,c:u64){let p=unsafe{out.add(i*16)};unsafe{p.write(t);p.add(1).write(s);w16(p,2,f);}
 match t{TYPE_INTERRUPT=>unsafe{w32(p,4,a as u32);w32(p,8,b as u32);w64(p,12,c);},TYPE_DMA=>unsafe{w32(p,4,a as u32);w32(p,8,b as u32);w32(p,12,c as u32);},_=>unsafe{w64(p,4,a);w32(p,12,b as u32);}}
}
/// Write a one-FULL_DESCRIPTOR CM_RESOURCE_LIST into caller-owned kernel storage.
pub unsafe fn serialize(r:&ResourceList,out:*mut u8,capacity:usize)->Result<usize,SerializeError>{
 let needed=required_size(r)?;if out.is_null()||capacity<needed{return Err(SerializeError::OutputTooSmall)}let count=descriptor_count(r)?;
 unsafe{core::ptr::write_bytes(out,0,needed);w32(out,0,1);}let full=unsafe{out.add(4)};
 unsafe{w32(full,0,15);w32(full,4,0);w16(full,8,1);w16(full,10,1);w32(full,12,count as u32);}
 let p=unsafe{full.add(16)};let mut i=0usize;
 // Validate all descriptors before publishing any partially serialized list.
 for item in r.entries[..r.count].iter().flatten(){match *item{
 Resource::Io{minimum,maximum,..}|Resource::Memory{minimum,maximum,..}|Resource::BusNumber{minimum,maximum,..} if minimum!=maximum=>return Err(SerializeError::InvalidResourceList),
 Resource::ExtendedIrq{count,..} if count as usize>8=>return Err(SerializeError::InvalidResourceList),
 _=>{}
 }}
 for item in r.entries[..r.count].iter().flatten(){match *item{
 Resource::Io{minimum,maximum,length,decode_16_bit,..}=>{if minimum!=maximum{return Err(SerializeError::InvalidResourceList)}unsafe{desc(p,i,TYPE_PORT,SHARE_EXCLUSIVE,if decode_16_bit{PORT_IO}else{0},minimum,length,0);i+=1;}},
 Resource::Memory{minimum,maximum,length,writeable,cacheable,..}=>{if minimum!=maximum{return Err(SerializeError::InvalidResourceList)}unsafe{desc(p,i,TYPE_MEMORY,SHARE_EXCLUSIVE,(if writeable{0}else{MEMORY_READ_ONLY})|(if cacheable{MEMORY_CACHEABLE}else{0}),minimum,length,0);i+=1;}},
 Resource::BusNumber{minimum,maximum,length,..}=>{if minimum!=maximum{return Err(SerializeError::InvalidResourceList)}unsafe{desc(p,i,TYPE_BUS_NUMBER,SHARE_EXCLUSIVE,0,minimum,length,0);i+=1;}},
 Resource::Irq{mask,level_triggered,shared,..}=>for gsi in 0..16u32{if mask&(1u16<<gsi)!=0{unsafe{desc(p,i,TYPE_INTERRUPT,if shared{SHARE_SHARED}else{SHARE_EXCLUSIVE},if level_triggered{0}else{INTERRUPT_LATCHED},gsi as u64,gsi as u64,u64::MAX);i+=1;}}},
 Resource::ExtendedIrq{flags,interrupts,count}=>{let level=flags&2!=0;let shared=flags&8!=0;for gsi in interrupts[..count as usize].iter(){unsafe{desc(p,i,TYPE_INTERRUPT,if shared{SHARE_SHARED}else{SHARE_EXCLUSIVE},if level{0}else{INTERRUPT_LATCHED},*gsi as u64,*gsi as u64,u64::MAX);i+=1;}}},
 Resource::Dma{channels,transfer_size,bus_master}=>{let flags=match transfer_size{0=>0,1=>1,2=>2,_=>4}|if bus_master{8}else{0};for channel in 0..8u32{if channels&(1u8<<channel)!=0{unsafe{desc(p,i,TYPE_DMA,SHARE_EXCLUSIVE,flags,channel as u64,0,0);i+=1;}}}},
 Resource::EndTag{..}=>{}
 }}
 if i!=count{return Err(SerializeError::InvalidResourceList)}Ok(needed)
}
#[cfg(test)]extern crate std;
#[cfg(test)]mod tests{use super::*;use crate::acpi_resources::Resource;
#[test]fn serializes_io_memory_and_irq(){let mut r=ResourceList::EMPTY;r.entries[0]=Some(Resource::Io{minimum:0x3f8,maximum:0x3ff,alignment:1,length:8,decode_16_bit:true});r.entries[1]=Some(Resource::Memory{minimum:0xfed00000,maximum:0xfed00fff,alignment:0x1000,length:0x1000,writeable:true,cacheable:false,kind:8});r.entries[2]=Some(Resource::Irq{mask:1<<4,level_triggered:false,active_low:false,shared:true,wake_capable:false});r.entries[3]=Some(Resource::EndTag{checksum:0});r.count=4;let mut b=[0u8;68];assert_eq!(unsafe{serialize(&r,b.as_mut_ptr(),b.len())},Ok(68));assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()),1);assert_eq!(u32::from_le_bytes(b[16..20].try_into().unwrap()),15);assert_eq!(u32::from_le_bytes(b[28..32].try_into().unwrap()),3);assert_eq!(b[20],TYPE_PORT);assert_eq!(u64::from_le_bytes(b[24..32].try_into().unwrap()),0x3f8);assert_eq!(b[36],TYPE_MEMORY);assert_eq!(u64::from_le_bytes(b[40..48].try_into().unwrap()),0xfed00000);assert_eq!(b[52],TYPE_INTERRUPT);assert_eq!(b[53],SHARE_SHARED);}

#[test]fn rejects_unassigned_address_ranges(){let mut r=ResourceList::EMPTY;r.entries[0]=Some(Resource::Io{minimum:0x100,maximum:0x1ff,alignment:1,length:8,decode_16_bit:true});r.count=1;let mut b=[0u8;36];assert_eq!(unsafe{serialize(&r,b.as_mut_ptr(),b.len())},Err(SerializeError::InvalidResourceList));}
#[test]fn short_output_is_not_modified(){let mut r=ResourceList::EMPTY;r.entries[0]=Some(Resource::Io{minimum:1,maximum:1,alignment:1,length:1,decode_16_bit:true});r.count=1;let mut b=[0xa5;20];assert_eq!(unsafe{serialize(&r,b.as_mut_ptr(),b.len())},Err(SerializeError::OutputTooSmall));assert_eq!(b,[0xa5;20]);}
}
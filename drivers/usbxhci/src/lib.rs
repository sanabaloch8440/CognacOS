//! Bounded xHCI controller primitives for CognacOS.
//! This is a controller/transfer-ring core, not a complete USB host stack.
//! MMIO, DMA allocation, interrupt routing, enumeration, and HID transport belong to HAL/kernel.
#![no_std]
#[cfg(test)] extern crate std;
pub const CAPLENGTH_MIN:u8=0x20;
pub const MAX_SLOTS:u8=64;
pub const MAX_PORTS:u8=255;
pub const TRB_TYPE_SHIFT:u32=10;
pub const TRB_TYPE_MASK:u32=0x3f<<TRB_TYPE_SHIFT;
pub const TRB_TYPE_NORMAL:u32=1;
pub const TRB_TYPE_SETUP_STAGE:u32=2;
pub const TRB_TYPE_DATA_STAGE:u32=3;
pub const TRB_TYPE_STATUS_STAGE:u32=4;
pub const TRB_TYPE_LINK:u32=6;
pub const TRB_TYPE_ENABLE_SLOT:u32=9;
pub const TRB_TYPE_ADDRESS_DEVICE:u32=11;
pub const TRB_TYPE_CONFIGURE_ENDPOINT:u32=12;
pub const TRB_TYPE_NOOP_COMMAND:u32=23;
pub const TRB_TYPE_TRANSFER_EVENT:u32=32;
pub const TRB_TYPE_COMMAND_COMPLETION:u32=33;
pub const TRB_TYPE_PORT_STATUS_CHANGE:u32=34;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum XhciError{TruncatedCapabilities,InvalidCapabilityLength,UnsupportedPageSize,InvalidRingSize,RingFull,InvalidDmaAddress,Timeout,ControllerHalted,InvalidPort,InvalidSlot,InvalidParameter,CompletionError(u8)}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
#[repr(C,align(16))]
pub struct Trb{pub parameter:u64,pub status:u32,pub control:u32}
impl Trb{
 pub const fn new(parameter:u64,status:u32,trb_type:u32,cycle:bool)->Self{Self{parameter,status,control:((trb_type<<TRB_TYPE_SHIFT)&TRB_TYPE_MASK)|if cycle{1}else{0}}}
 pub const fn trb_type(self)->u8{((self.control&TRB_TYPE_MASK)>>TRB_TYPE_SHIFT) as u8}
 pub const fn cycle(self)->bool{self.control&1!=0}
 pub const fn completion_code(self)->u8{(self.status>>24) as u8}
 pub const fn slot_id(self)->u8{(self.control>>24) as u8}
 pub const fn endpoint_id(self)->u8{((self.control>>16)&0x1f) as u8}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Capabilities{pub cap_length:u8,pub version:u16,pub max_slots:u8,pub max_interrupters:u16,pub max_ports:u8,pub doorbell_offset:u32,pub runtime_offset:u32,pub context_size:usize,pub page_size_mask:u32}
fn u16at(b:&[u8],o:usize)->Result<u16,XhciError>{Ok(u16::from_le_bytes([*b.get(o).ok_or(XhciError::TruncatedCapabilities)?,*b.get(o+1).ok_or(XhciError::TruncatedCapabilities)?]))}
fn u32at(b:&[u8],o:usize)->Result<u32,XhciError>{Ok(u32::from_le_bytes([*b.get(o).ok_or(XhciError::TruncatedCapabilities)?,*b.get(o+1).ok_or(XhciError::TruncatedCapabilities)?,*b.get(o+2).ok_or(XhciError::TruncatedCapabilities)?,*b.get(o+3).ok_or(XhciError::TruncatedCapabilities)?]))}
/// Parse the fixed xHCI capability register prefix from an already validated MMIO snapshot.
pub fn parse_capabilities(b:&[u8])->Result<Capabilities,XhciError>{
 if b.len()<0x20{return Err(XhciError::TruncatedCapabilities)}
 let cap_length=b[0];if cap_length<CAPLENGTH_MIN||cap_length as usize>b.len(){return Err(XhciError::InvalidCapabilityLength)}
 let hcs1=u32at(b,4)?;let hcc1=u32at(b,0x10)?;let page_size_mask=if b.len()>=0x2c{u32at(b,0x28)?}else{0};
 let max_slots=(hcs1&0xff) as u8;let max_interrupters=((hcs1>>8)&0x7ff) as u16;let max_ports=(hcs1>>24) as u8;
 let doorbell_offset=u32at(b,0x14)?&!3;let runtime_offset=u32at(b,0x18)?&!0x1f;
 if max_slots==0||max_slots>MAX_SLOTS||max_ports==0||doorbell_offset<cap_length as u32||runtime_offset<cap_length as u32{return Err(XhciError::InvalidCapabilityLength)}
 Ok(Capabilities{cap_length,version:u16at(b,2)?,max_slots,max_interrupters,max_ports,doorbell_offset,runtime_offset,context_size:if hcc1&(1<<2)!=0{64}else{32},page_size_mask})
}
/// Fixed-capacity producer ring. Its backing memory must be DMA-safe, pinned and coherent.
pub struct ProducerRing<const N:usize>{entries:[Trb;N],producer_index:usize,cycle:bool}
impl<const N:usize> ProducerRing<N>{
 pub const fn new()->Self{Self{entries:[Trb{parameter:0,status:0,control:0};N],producer_index:0,cycle:true}}
 pub const fn len(&self)->usize{N} pub const fn producer_index(&self)->usize{self.producer_index} pub const fn cycle_state(&self)->bool{self.cycle}
 pub fn push(&mut self,mut trb:Trb)->Result<usize,XhciError>{
  if N<2||!N.is_power_of_two(){return Err(XhciError::InvalidRingSize)}
  if self.producer_index+1>=N{return Err(XhciError::RingFull)}
  trb.control=(trb.control&!1)|if self.cycle{1}else{0};let idx=self.producer_index;self.entries[idx]=trb;self.producer_index+=1;Ok(idx)
 }
 pub fn get(&self,index:usize)->Option<Trb>{self.entries.get(index).copied()}
}
impl<const N:usize> Default for ProducerRing<N>{fn default()->Self{Self::new()}}
/// DMA bus addresses are never treated as CPU virtual pointers.
pub const fn validate_dma_address(address:u64,alignment:u64)->Result<u64,XhciError>{
 if alignment==0||!alignment.is_power_of_two()||address&(alignment-1)!=0||address==0{Err(XhciError::InvalidDmaAddress)}else{Ok(address)}
}
/// Bounded register polling. The callback performs exactly one register read per iteration.
pub fn wait_register<F:FnMut()->u32>(mut read:F,mask:u32,expected:u32,budget:u32)->Result<(),XhciError>{
 for _ in 0..budget{if read()&mask==expected&mask{return Ok(())}core::hint::spin_loop()}Err(XhciError::Timeout)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn rejects_truncated_or_invalid_caps(){let mut b=[0u8;0x30];assert_eq!(parse_capabilities(&b),Err(XhciError::InvalidCapabilityLength));b[0]=0x40;assert_eq!(parse_capabilities(&b),Err(XhciError::InvalidCapabilityLength));}
 #[test]fn parses_capability_prefix(){let mut b=[0u8;0x30];b[0]=0x20;b[2]=0x10;b[4]=8;b[5]=1;b[7]=4;b[0x14..0x18].copy_from_slice(&0x100u32.to_le_bytes());b[0x18..0x1c].copy_from_slice(&0x200u32.to_le_bytes());let c=parse_capabilities(&b).unwrap();assert_eq!(c.max_slots,8);assert_eq!(c.max_ports,4);}
 #[test]fn trb_fields_round_trip(){let t=Trb::new(0x1234,0,TRB_TYPE_ENABLE_SLOT,true);assert_eq!(t.trb_type(),TRB_TYPE_ENABLE_SLOT as u8);assert!(t.cycle());}
 #[test]fn rejects_bad_dma_alignment(){assert_eq!(validate_dma_address(0x1003,16),Err(XhciError::InvalidDmaAddress));assert_eq!(validate_dma_address(0x1000,16),Ok(0x1000));}
 #[test]fn ring_does_not_overwrite_reserved_slot(){let mut r:ProducerRing<4>=ProducerRing::new();assert!(r.push(Trb::default()).is_ok());assert!(r.push(Trb::default()).is_ok());assert_eq!(r.push(Trb::default()),Err(XhciError::RingFull));}
 #[test]fn wait_is_bounded(){let mut n=0;assert_eq!(wait_register(||{n+=1;0},1,1,4),Err(XhciError::Timeout));assert_eq!(n,4);}
}

/// Internal version marker used by the PE entry point to retain the controller core.
pub const fn core_abi_version()->u32{1}

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
 let hcs1=u32at(b,4)?;let hcc1=u32at(b,0x10)?;
 // PAGESIZE lives in the operational register block at CAPLENGTH + 0x08,
 // not in the capability-register prefix; query it only after MMIO mapping.
 let page_size_mask=0;
 let max_slots=(hcs1&0xff) as u8;let max_interrupters=((hcs1>>8)&0x7ff) as u16;let max_ports=(hcs1>>24) as u8;
 let doorbell_offset=u32at(b,0x14)?&!3;let runtime_offset=u32at(b,0x18)?&!0x1f;
 if max_slots==0||max_slots>MAX_SLOTS||max_ports==0||doorbell_offset<cap_length as u32||runtime_offset<cap_length as u32{return Err(XhciError::InvalidCapabilityLength)}
 Ok(Capabilities{cap_length,version:u16at(b,2)?,max_slots,max_interrupters,max_ports,doorbell_offset,runtime_offset,context_size:if hcc1&(1<<2)!=0{64}else{32},page_size_mask})
}
/// Single-segment command/transfer ring model. The last TRB is a Link TRB,
/// and the backing allocation must be contiguous, DMA-coherent and pinned.
pub struct ProducerRing<const N:usize>{entries:[Trb;N],producer_index:usize,cycle:bool,dma_base:u64}
impl<const N:usize> ProducerRing<N>{
 pub const fn new()->Self{Self{entries:[Trb{parameter:0,status:0,control:0};N],producer_index:0,cycle:true,dma_base:0}}
 pub const fn new_with_dma_base(dma_base:u64)->Self{Self{entries:[Trb{parameter:0,status:0,control:0};N],producer_index:0,cycle:true,dma_base}}
 pub const fn len(&self)->usize{N}
 pub const fn producer_index(&self)->usize{self.producer_index}
 pub const fn cycle_state(&self)->bool{self.cycle}
 /// Enqueue one TRB and follow the Link TRB when reaching the segment end.
 /// Full-ring accounting is the caller's responsibility because hardware owns the dequeue pointer.
 pub fn push(&mut self,mut trb:Trb)->Result<usize,XhciError>{
  if N<4||!N.is_power_of_two(){return Err(XhciError::InvalidRingSize)}
  if self.producer_index>=N-1{return Err(XhciError::InvalidParameter)}
  if self.producer_index+1==N-1{validate_dma_address(self.dma_base,64)?;}
  trb.control=(trb.control&!1)|if self.cycle{1}else{0};
  let idx=self.producer_index;self.entries[idx]=trb;self.producer_index+=1;
  if self.producer_index==N-1{
   self.entries[N-1]=Trb{parameter:self.dma_base,status:0,control:(TRB_TYPE_LINK<<TRB_TYPE_SHIFT)|2|if self.cycle{1}else{0}};
   self.producer_index=0;self.cycle=!self.cycle;
  }
  Ok(idx)
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

/// xHCI event-ring consumer. The event storage must be a DMA-coherent segment
/// with a separately allocated ERST; this type models one segment and its CCS.
pub struct EventRingConsumer<const N:usize>{entries:[Trb;N],dequeue:usize,cycle:bool}
impl<const N:usize> EventRingConsumer<N>{
 pub const fn new()->Self{Self{entries:[Trb{parameter:0,status:0,control:0};N],dequeue:0,cycle:true}}
 pub const fn dequeue_index(&self)->usize{self.dequeue}
 pub const fn consumer_cycle(&self)->bool{self.cycle}
 /// Snapshot a TRB only when its cycle bit matches the consumer cycle state.
 pub fn peek(&self)->Option<Trb>{
  if N==0{return None}
  let t=self.entries[self.dequeue];
  if t.cycle()==self.cycle{Some(t)}else{None}
 }
 /// Advance after the caller has fully handled the current event.
 pub fn advance(&mut self)->Result<(),XhciError>{
  if N==0||!N.is_power_of_two(){return Err(XhciError::InvalidRingSize)}
  if self.peek().is_none(){return Err(XhciError::InvalidParameter)}
  self.dequeue+=1;
  if self.dequeue==N{self.dequeue=0;self.cycle=!self.cycle;}
  Ok(())
 }
 /// Test/hardware-adapter helper: publish a synthetic event with an explicit cycle bit.
 pub fn set_snapshot_for_test(&mut self,index:usize,event:Trb)->Result<(),XhciError>{
  if index>=N{return Err(XhciError::InvalidParameter)} self.entries[index]=event;Ok(())
 }
}
impl<const N:usize> Default for EventRingConsumer<N>{fn default()->Self{Self::new()}}

pub const USB_DESC_DEVICE:u8=1;
pub const USB_DESC_CONFIGURATION:u8=2;
pub const USB_DESC_INTERFACE:u8=4;
pub const USB_DESC_ENDPOINT:u8=5;
pub const USB_DESC_HID:u8=0x21;
pub const USB_DESC_REPORT:u8=0x22;
pub const USB_CLASS_HID:u8=3;
pub const USB_SUBCLASS_BOOT:u8=1;
pub const USB_PROTOCOL_KEYBOARD:u8=1;
pub const USB_ENDPOINT_IN:u8=0x80;
pub const USB_TRANSFER_INTERRUPT:u8=3;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct UsbDeviceDescriptor{pub usb_version:u16,pub vendor_id:u16,pub product_id:u16,pub max_packet_size0:u8,pub num_configurations:u8}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct HidKeyboardEndpoint{pub interface_number:u8,pub alternate_setting:u8,pub endpoint_address:u8,pub max_packet_size:u16,pub interval:u8}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum UsbDescriptorError{Truncated,InvalidLength,WrongDescriptor,NoBootKeyboard,InvalidEndpoint}
/// Parse the first 18 bytes of a standard USB device descriptor.
pub fn parse_usb_device_descriptor(b:&[u8])->Result<UsbDeviceDescriptor,UsbDescriptorError>{
 if b.len()<18{return Err(UsbDescriptorError::Truncated)}
 if b[0]<18||b[1]!=USB_DESC_DEVICE{return Err(UsbDescriptorError::WrongDescriptor)}
 Ok(UsbDeviceDescriptor{usb_version:u16::from_le_bytes([b[2],b[3]]),max_packet_size0:b[7],vendor_id:u16::from_le_bytes([b[8],b[9]]),product_id:u16::from_le_bytes([b[10],b[11]]),num_configurations:b[17]})
}
/// Locate a boot-protocol keyboard interrupt-IN endpoint in a configuration blob.
/// The input must be the complete configuration descriptor (wTotalLength bytes).
pub fn find_boot_keyboard_endpoint(b:&[u8])->Result<HidKeyboardEndpoint,UsbDescriptorError>{
 if b.len()<9{return Err(UsbDescriptorError::Truncated)}
 if b[1]!=USB_DESC_CONFIGURATION||b[0]<9{return Err(UsbDescriptorError::WrongDescriptor)}
 let total=u16::from_le_bytes([b[2],b[3]]) as usize;
 if total>b.len()||total<9{return Err(UsbDescriptorError::Truncated)}
 let mut off=0usize;let mut iface:Option<(u8,u8)>=None;let mut found=None;
 while off<total{
  if off+2>total{return Err(UsbDescriptorError::Truncated)}
  let len=b[off] as usize;let ty=b[off+1];
  if len<2||off+len>total{return Err(UsbDescriptorError::InvalidLength)}
  if ty==USB_DESC_INTERFACE{
   if len<9{return Err(UsbDescriptorError::InvalidLength)}
   iface=if b[off+5]==USB_CLASS_HID&&b[off+6]==USB_SUBCLASS_BOOT&&b[off+7]==USB_PROTOCOL_KEYBOARD{Some((b[off+2],b[off+3]))}else{None};
  }else if ty==USB_DESC_ENDPOINT{
   if len<7{return Err(UsbDescriptorError::InvalidLength)}
   if let Some((interface_number,alternate_setting))=iface{
    let address=b[off+2];let attributes=b[off+3]&3;let packet=u16::from_le_bytes([b[off+4],b[off+5]])&0x07ff;
    if address&USB_ENDPOINT_IN!=0&&attributes==USB_TRANSFER_INTERRUPT{
     if packet==0{return Err(UsbDescriptorError::InvalidEndpoint)}
     found=Some(HidKeyboardEndpoint{interface_number,alternate_setting,endpoint_address:address,max_packet_size:packet,interval:b[off+6]});
    }
   }
  }
  off+=len;
 }
 found.ok_or(UsbDescriptorError::NoBootKeyboard)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EnumerationState{Offline,Connected,ResetComplete,SlotEnabled,Addressed,Configured,HidKeyboardReady,Failed}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EnumerationEvent{PortConnected,PortReset,SlotEnabled,Addressed,Configured,BootKeyboardFound,Disconnected,Failure}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EnumerationError{InvalidTransition}
/// Pure USB enumeration state machine. Hardware actions (port reset, command submission,
/// control transfers and endpoint configuration) are issued by the platform adapter.
pub struct UsbEnumeration{state:EnumerationState}
impl UsbEnumeration{
 pub const fn new()->Self{Self{state:EnumerationState::Offline}}
 pub const fn state(&self)->EnumerationState{self.state}
 pub fn transition(&mut self,event:EnumerationEvent)->Result<EnumerationState,EnumerationError>{
  use EnumerationEvent as E;use EnumerationState as S;
  let next=match (self.state,event){
   (_,E::Disconnected)=>(S::Offline),
   (_,E::Failure)=>S::Failed,
   (S::Offline,E::PortConnected)=>S::Connected,
   (S::Connected,E::PortReset)=>S::ResetComplete,
   (S::ResetComplete,E::SlotEnabled)=>S::SlotEnabled,
   (S::SlotEnabled,E::Addressed)=>S::Addressed,
   (S::Addressed,E::Configured)=>S::Configured,
   (S::Configured,E::BootKeyboardFound)=>S::HidKeyboardReady,
   _=>return Err(EnumerationError::InvalidTransition),
  };
  self.state=next;Ok(next)
 }
}
impl Default for UsbEnumeration{fn default()->Self{Self::new()}}

#[cfg(test)]
mod milestone_tests{
 use super::*;
 #[test]fn event_ring_honors_cycle_and_wrap(){
  let mut r:EventRingConsumer<2>=EventRingConsumer::new();
  assert!(r.peek().is_none());
  r.set_snapshot_for_test(0,Trb::new(0,0,TRB_TYPE_PORT_STATUS_CHANGE,true)).unwrap();
  assert_eq!(r.peek().unwrap().trb_type(),TRB_TYPE_PORT_STATUS_CHANGE as u8);
  r.advance().unwrap();assert!(r.peek().is_none());
  r.set_snapshot_for_test(1,Trb::new(0,0,TRB_TYPE_TRANSFER_EVENT,true)).unwrap();
  r.advance().unwrap();assert_eq!(r.dequeue_index(),0);assert!(!r.consumer_cycle());
  r.set_snapshot_for_test(0,Trb::new(0,0,TRB_TYPE_COMMAND_COMPLETION,false)).unwrap();
  assert_eq!(r.peek().unwrap().trb_type(),TRB_TYPE_COMMAND_COMPLETION as u8);
 }
 #[test]fn parses_usb_device_descriptor(){
  let b=[18,1,0x00,0x02,0,0,0,8,0x34,0x12,0x78,0x56,0,1,1,2,3,1];
  let d=parse_usb_device_descriptor(&b).unwrap();assert_eq!(d.vendor_id,0x1234);assert_eq!(d.product_id,0x5678);assert_eq!(d.num_configurations,1);
 }
 #[test]fn finds_boot_keyboard_interrupt_in_endpoint(){
  let b=[9,2,34,0,1,1,0,0x80,50,
   9,4,0,0,1,3,1,1,0,
   9,0x21,0x11,0x01,0,1,0x22,63,0,
   7,5,0x81,3,8,0,10];
  let e=find_boot_keyboard_endpoint(&b).unwrap();assert_eq!(e.interface_number,0);assert_eq!(e.endpoint_address,0x81);assert_eq!(e.max_packet_size,8);
 }
 #[test]fn rejects_malformed_usb_descriptor_lengths(){
  let b=[9,2,11,0,1,1,0,0x80,50,1,4];
  assert_eq!(find_boot_keyboard_endpoint(&b),Err(UsbDescriptorError::InvalidLength));
 }
 #[test]fn enumeration_state_machine_rejects_skipped_hardware_steps(){
  let mut e=UsbEnumeration::new();assert_eq!(e.transition(EnumerationEvent::Addressed),Err(EnumerationError::InvalidTransition));
  for event in [EnumerationEvent::PortConnected,EnumerationEvent::PortReset,EnumerationEvent::SlotEnabled,EnumerationEvent::Addressed,EnumerationEvent::Configured,EnumerationEvent::BootKeyboardFound]{e.transition(event).unwrap();}
  assert_eq!(e.state(),EnumerationState::HidKeyboardReady);
  e.transition(EnumerationEvent::Disconnected).unwrap();assert_eq!(e.state(),EnumerationState::Offline);
 }
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn rejects_truncated_or_invalid_caps(){let mut b=[0u8;0x30];assert_eq!(parse_capabilities(&b),Err(XhciError::InvalidCapabilityLength));b[0]=0x40;assert_eq!(parse_capabilities(&b),Err(XhciError::InvalidCapabilityLength));}
 #[test]fn parses_capability_prefix(){let mut b=[0u8;0x30];b[0]=0x20;b[2]=0x10;b[4]=8;b[5]=1;b[7]=4;b[0x14..0x18].copy_from_slice(&0x100u32.to_le_bytes());b[0x18..0x1c].copy_from_slice(&0x200u32.to_le_bytes());let c=parse_capabilities(&b).unwrap();assert_eq!(c.max_slots,8);assert_eq!(c.max_ports,4);}
 #[test]fn trb_fields_round_trip(){let t=Trb::new(0x1234,0,TRB_TYPE_ENABLE_SLOT,true);assert_eq!(t.trb_type(),TRB_TYPE_ENABLE_SLOT as u8);assert!(t.cycle());}
 #[test]fn rejects_bad_dma_alignment(){assert_eq!(validate_dma_address(0x1003,16),Err(XhciError::InvalidDmaAddress));assert_eq!(validate_dma_address(0x1000,16),Ok(0x1000));}
 #[test]fn producer_ring_links_and_toggles_cycle_on_wrap(){let mut r:ProducerRing<4>=ProducerRing::new_with_dma_base(0x1000);assert_eq!(r.push(Trb::default()),Ok(0));assert_eq!(r.push(Trb::default()),Ok(1));assert_eq!(r.push(Trb::default()),Ok(2));let link=r.get(3).unwrap();assert_eq!(link.trb_type(),TRB_TYPE_LINK as u8);assert_eq!(link.parameter,0x1000);assert_eq!(r.producer_index(),0);assert!(!r.cycle_state());assert_eq!(r.push(Trb::default()),Ok(0));assert!(!r.get(0).unwrap().cycle());}
 #[test]fn wait_is_bounded(){let mut n=0;assert_eq!(wait_register(||{n+=1;0},1,1,4),Err(XhciError::Timeout));assert_eq!(n,4);}
}

/// Internal version marker used by the PE entry point to retain the controller core.
pub const fn core_abi_version()->u32{1}


pub const USBCMD_RUN:u32=1;
pub const USBCMD_RESET:u32=1<<1;
pub const USBCMD_INTERRUPT_ENABLE:u32=1<<2;
pub const USBSTS_HALTED:u32=1;
pub const USBSTS_NOT_READY:u32=1<<11;
pub const IMAN_INTERRUPT_ENABLE:u32=1<<1;
pub const CONFIG_MAX_SLOTS_MASK:u32=0xff;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ControllerDmaConfig{pub dcbaa:u64,pub command_ring:u64,pub erst:u64,pub event_ring:u64,pub event_ring_entries:u16,pub max_slots:u8}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ControllerInitError{InvalidCapabilities,InvalidDmaConfiguration,UnsupportedPageSize,InterruptNotRegistered,RegisterIoFailure,ResetTimeout,ControllerDidNotHalt,ControllerDidNotStart}
/// Implemented by a kernel/HAL adapter over a validated, uncached BAR mapping.
/// Register offsets are relative to the controller's MMIO BAR base.
pub trait XhciRegisterIo{
 fn read32(&mut self,offset:u32)->Result<u32,()>;
 fn write32(&mut self,offset:u32,value:u32)->Result<(),()>;
}
fn reg_read<I:XhciRegisterIo>(io:&mut I,offset:u32)->Result<u32,ControllerInitError>{io.read32(offset).map_err(|_|ControllerInitError::RegisterIoFailure)}
fn reg_write<I:XhciRegisterIo>(io:&mut I,offset:u32,value:u32)->Result<(),ControllerInitError>{io.write32(offset,value).map_err(|_|ControllerInitError::RegisterIoFailure)}
fn poll_reg<I:XhciRegisterIo>(io:&mut I,offset:u32,mask:u32,expected:u32,budget:u32)->Result<(),ControllerInitError>{
 for _ in 0..budget{if reg_read(io,offset)?&mask==expected&mask{return Ok(())}core::hint::spin_loop()}Err(ControllerInitError::ResetTimeout)
}
pub fn validate_controller_dma_config(caps:&Capabilities,c:&ControllerDmaConfig)->Result<(),ControllerInitError>{
 if c.max_slots==0||c.max_slots>caps.max_slots||c.event_ring_entries<16||c.event_ring_entries>4096||!c.event_ring_entries.is_power_of_two(){return Err(ControllerInitError::InvalidDmaConfiguration)}
 if validate_dma_address(c.dcbaa,64).is_err()||validate_dma_address(c.command_ring,64).is_err()||validate_dma_address(c.erst,64).is_err()||validate_dma_address(c.event_ring,64).is_err(){return Err(ControllerInitError::InvalidDmaConfiguration)}
 Ok(())
}
/// Reset and program the controller's command ring, DCBAA, one ERST segment,
/// event ring and slot limit, then start it. The caller must have already acquired
/// PCI resources, created DMA-coherent allocations, populated the ERST's first entry
/// with event_ring + event_ring_entries, and registered the interrupt.
pub fn initialize_controller<I:XhciRegisterIo>(io:&mut I,caps:&Capabilities,dma:ControllerDmaConfig,interrupt_registered:bool,poll_budget:u32)->Result<(),ControllerInitError>{
 if caps.cap_length<CAPLENGTH_MIN||caps.doorbell_offset<caps.cap_length as u32||caps.runtime_offset<caps.cap_length as u32{return Err(ControllerInitError::InvalidCapabilities)}
 validate_controller_dma_config(caps,&dma)?;
 if !interrupt_registered{return Err(ControllerInitError::InterruptNotRegistered)}
 let op=caps.cap_length as u32;
 let cmd=reg_read(io,op).map_err(|_|ControllerInitError::RegisterIoFailure)?;
 poll_reg(io,op+4,USBSTS_NOT_READY,0,poll_budget)?;
 if reg_read(io,op+8)?&1==0{return Err(ControllerInitError::UnsupportedPageSize)}
 reg_write(io,op,cmd&!(USBCMD_RUN|USBCMD_INTERRUPT_ENABLE))?;
 poll_reg(io,op+4,USBSTS_HALTED,USBSTS_HALTED,poll_budget)?;
 reg_write(io,op,(cmd&!(USBCMD_RUN|USBCMD_INTERRUPT_ENABLE))|USBCMD_RESET)?;
 poll_reg(io,op,USBCMD_RESET,0,poll_budget)?;
 poll_reg(io,op+4,USBSTS_NOT_READY,0,poll_budget)?;
 if reg_read(io,op+4)?&USBSTS_HALTED==0{return Err(ControllerInitError::ControllerDidNotHalt)}
 reg_write(io,op+0x18,(dma.command_ring as u32)|1)?;
 reg_write(io,op+0x1c,(dma.command_ring>>32) as u32)?;
 reg_write(io,op+0x30,dma.dcbaa as u32)?;
 reg_write(io,op+0x34,(dma.dcbaa>>32) as u32)?;
 reg_write(io,op+0x38,(dma.max_slots as u32)&CONFIG_MAX_SLOTS_MASK)?;
 let rt=caps.runtime_offset;
 let iman=rt+0x20;let erstsz=rt+0x28;let erstba=rt+0x30;let erdp=rt+0x38;
 // ERST memory must already contain one { event_ring DMA base, segment size } entry.
 reg_write(io,erstsz,1)?;
 reg_write(io,erstba,dma.erst as u32)?;
 reg_write(io,erstba+4,(dma.erst>>32) as u32)?;
 reg_write(io,erdp,dma.event_ring as u32)?;
 reg_write(io,erdp+4,(dma.event_ring>>32) as u32)?;
 reg_write(io,iman,IMAN_INTERRUPT_ENABLE)?;
 reg_write(io,op,USBCMD_RUN|USBCMD_INTERRUPT_ENABLE)?;
 for _ in 0..poll_budget{if reg_read(io,op+4)?&USBSTS_HALTED==0{return Ok(())}core::hint::spin_loop()}
 Err(ControllerInitError::ControllerDidNotStart)
}


#[cfg(test)]
mod controller_init_tests{
 use super::*;
 struct FakeMmio{regs:[u32;256]}
 impl FakeMmio{fn new()->Self{let mut x=Self{regs:[0;256]};x.regs[0x44/4]=USBSTS_HALTED;x.regs[0x48/4]=1;x}}
 impl XhciRegisterIo for FakeMmio{
  fn read32(&mut self,o:u32)->Result<u32,()>{self.regs.get((o/4) as usize).copied().ok_or(())}
  fn write32(&mut self,o:u32,v:u32)->Result<(),()>{
   let i=(o/4) as usize;if i>=self.regs.len(){return Err(())}
   if o==0x40{
    if v&USBCMD_RESET!=0{self.regs[i]=v&!USBCMD_RESET;self.regs[0x44/4]=USBSTS_HALTED;}
    else if v&USBCMD_RUN!=0{self.regs[i]=v;self.regs[0x44/4]=0;}
    else{self.regs[i]=v;self.regs[0x44/4]=USBSTS_HALTED;}
   }else{self.regs[i]=v;}Ok(())
  }
 }
 fn caps()->Capabilities{Capabilities{cap_length:0x40,version:0x100,max_slots:8,max_interrupters:1,max_ports:4,doorbell_offset:0x1000,runtime_offset:0x200,context_size:32,page_size_mask:1}}
 fn dma()->ControllerDmaConfig{ControllerDmaConfig{dcbaa:0x10000,command_ring:0x11000,erst:0x12000,event_ring:0x13000,event_ring_entries:256,max_slots:8}}
 #[test]fn validates_all_dma_alignments_and_slot_bounds(){
  let c=caps();assert_eq!(validate_controller_dma_config(&c,&dma()),Ok(()));
  let mut d=dma();d.event_ring=0x13008;assert_eq!(validate_controller_dma_config(&c,&d),Err(ControllerInitError::InvalidDmaConfiguration));
  let mut d=dma();d.max_slots=9;assert_eq!(validate_controller_dma_config(&c,&d),Err(ControllerInitError::InvalidDmaConfiguration));
 }
 #[test]fn controller_init_requires_registered_interrupt(){
  let mut io=FakeMmio::new();assert_eq!(initialize_controller(&mut io,&caps(),dma(),false,8),Err(ControllerInitError::InterruptNotRegistered));
 }
 #[test]fn bounded_controller_reset_and_start_programs_registers(){
  let mut io=FakeMmio::new();assert_eq!(initialize_controller(&mut io,&caps(),dma(),true,8),Ok(()));
  assert_eq!(io.regs[0x40/4]&USBCMD_RUN,USBCMD_RUN);
  assert_eq!(io.regs[(0x40+0x18)/4],0x11001);
  assert_eq!(io.regs[(0x40+0x30)/4],0x10000);
  assert_eq!(io.regs[(0x40+0x38)/4],8);
  assert_eq!(io.regs[(0x200+0x28)/4],1);
  assert_eq!(io.regs[(0x200+0x30)/4],0x12000);
  assert_eq!(io.regs[(0x200+0x38)/4],0x13000);
 }
}


#[repr(C,align(16))]
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct ErstEntry{pub ring_segment_base:u64,pub ring_segment_size:u16,pub reserved:u16,pub reserved2:u32}
pub fn make_erst_entry(event_ring_dma:u64,entries:u16)->Result<ErstEntry,ControllerInitError>{
 if validate_dma_address(event_ring_dma,64).is_err()||entries<16||entries>4096||!entries.is_power_of_two(){return Err(ControllerInitError::InvalidDmaConfiguration)}
 Ok(ErstEntry{ring_segment_base:event_ring_dma,ring_segment_size:entries,reserved:0,reserved2:0})
}

#[cfg(test)]
mod erst_tests{
 use super::*;
 #[test]fn erst_entry_is_spec_sized_and_validated(){
  assert_eq!(core::mem::size_of::<ErstEntry>(),16);
  let e=make_erst_entry(0x4000,256).unwrap();assert_eq!(e.ring_segment_base,0x4000);assert_eq!(e.ring_segment_size,256);
  assert_eq!(make_erst_entry(0x4008,256),Err(ControllerInitError::InvalidDmaConfiguration));
  assert_eq!(make_erst_entry(0x4000,3),Err(ControllerInitError::InvalidDmaConfiguration));
 }
}


/// Concrete runtime boundary used by the xHCI controller owner. Implementations
/// must return pinned, DMA-coherent memory and must not expose a device address
/// that is merely a CPU virtual address. Interrupt registration must install an
/// ISR-safe handler before the controller is started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaAllocation {
    pub cpu_address: u64,
    pub device_address: u64,
    pub size: u64,
}
pub trait XhciPlatform: XhciRegisterIo {
    fn allocate_dma(&mut self, size: u64, alignment: u64, max_device_address: u64) -> Result<DmaAllocation, ()>;
    fn free_dma(&mut self, allocation: DmaAllocation);
    fn register_interrupt(&mut self) -> Result<(), ()>;
    fn unregister_interrupt(&mut self);
    fn set_interrupt_enabled(&mut self, enabled: bool) -> Result<(), ()>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeStartError {
    AllocationFailed,
    InvalidAllocation,
    InterruptRegistrationFailed,
    Controller(ControllerInitError),
    InterruptEnableFailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XhciRuntimeResources {
    pub dcbaa: DmaAllocation,
    pub command_ring: DmaAllocation,
    pub event_ring: DmaAllocation,
    pub erst: DmaAllocation,
    pub event_entries: u16,
    pub dequeue_index: u16,
    pub consumer_cycle: bool,
    pub interrupt_registered: bool,
    pub suspended: bool,
}
const RUNTIME_EVENT_ENTRIES: u16 = 256;
const RUNTIME_RING_BYTES: u64 = RUNTIME_EVENT_ENTRIES as u64 * core::mem::size_of::<Trb>() as u64;
fn valid_allocation(a: DmaAllocation, size: u64, alignment: u64) -> bool {
    a.cpu_address != 0 && a.cpu_address & (alignment - 1) == 0
        && a.size >= size && validate_dma_address(a.device_address, alignment).is_ok()
        && a.device_address.checked_add(size).is_some()
}
unsafe fn zero_dma(a: DmaAllocation, size: usize) {
    // SAFETY: XhciPlatform promises a writable CPU mapping covering a.size bytes.
    unsafe { core::ptr::write_bytes(a.cpu_address as *mut u8, 0, size); }
}
fn free_allocations<P: XhciPlatform>(p: &mut P, a: [Option<DmaAllocation>; 4]) {
    for allocation in a.into_iter().flatten().rev() { p.free_dma(allocation); }
}
/// Allocate and initialize the controller-owned DMA structures, register the
/// interrupt before enabling the controller, then perform the bounded reset/run
/// sequence. Every failure path frees acquired memory and unregisters the IRQ.
pub fn start_controller<P: XhciPlatform>(
    platform: &mut P, caps: &Capabilities, max_slots: u8, poll_budget: u32,
) -> Result<XhciRuntimeResources, RuntimeStartError> {
    if max_slots == 0 || max_slots > caps.max_slots || caps.max_slots > MAX_SLOTS { return Err(RuntimeStartError::InvalidAllocation); }
    let dcbaa_size = (max_slots as u64 + 1).checked_mul(8).ok_or(RuntimeStartError::InvalidAllocation)?;
    let sizes = [dcbaa_size, RUNTIME_RING_BYTES, RUNTIME_RING_BYTES, core::mem::size_of::<ErstEntry>() as u64];
    let aligns = [64u64, 64, 64, 64];
    let mut allocations: [Option<DmaAllocation>; 4] = [None, None, None, None];
    for i in 0..4 {
        let a = match platform.allocate_dma(sizes[i], aligns[i], u64::MAX) {
            Ok(a) if valid_allocation(a, sizes[i], aligns[i]) => a,
            Ok(a) => { platform.free_dma(a); free_allocations(platform, allocations); return Err(RuntimeStartError::InvalidAllocation); }
            Err(()) => { free_allocations(platform, allocations); return Err(RuntimeStartError::AllocationFailed); }
        };
        allocations[i] = Some(a);
        // SAFETY: allocation contract guarantees the requested mapped size.
        unsafe { zero_dma(a, sizes[i] as usize); }
    }
    let dcbaa = allocations[0].unwrap();
    let command_ring = allocations[1].unwrap();
    let event_ring = allocations[2].unwrap();
    let erst = allocations[3].unwrap();
    let entry = match make_erst_entry(event_ring.device_address, RUNTIME_EVENT_ENTRIES) {
        Ok(e) => e, Err(_) => { free_allocations(platform, allocations); return Err(RuntimeStartError::InvalidAllocation); }
    };
    // SAFETY: ERST is an aligned, writable 16-byte DMA allocation and the entry
    // layout is repr(C, align(16)) as required by xHCI.
    unsafe { core::ptr::write_volatile(erst.cpu_address as *mut ErstEntry, entry); }
    // Seed command ring segment's final TRB as a Link TRB pointing back to base.
    let mut link = Trb::new(command_ring.device_address, 0, TRB_TYPE_LINK, true);
    link.control |= 1 << 1; // Toggle Cycle (TC) on the segment Link TRB.
    // SAFETY: the command ring allocation covers RUNTIME_RING_BYTES.
    unsafe { core::ptr::write_volatile((command_ring.cpu_address as *mut Trb).add(RUNTIME_EVENT_ENTRIES as usize - 1), link); }
    if platform.register_interrupt().is_err() {
        free_allocations(platform, allocations);
        return Err(RuntimeStartError::InterruptRegistrationFailed);
    }
    let dma = ControllerDmaConfig {
        dcbaa: dcbaa.device_address, command_ring: command_ring.device_address,
        erst: erst.device_address, event_ring: event_ring.device_address,
        event_ring_entries: RUNTIME_EVENT_ENTRIES, max_slots,
    };
    if let Err(e) = initialize_controller(platform, caps, dma, true, poll_budget) {
        platform.unregister_interrupt();
        free_allocations(platform, allocations);
        return Err(RuntimeStartError::Controller(e));
    }
    if platform.set_interrupt_enabled(true).is_err() {
        // Stop the controller before releasing any memory it may still access.
        let op = caps.cap_length as u32;
        if let Ok(cmd) = platform.read32(op) {
            let _ = platform.write32(op, cmd & !(USBCMD_RUN | USBCMD_INTERRUPT_ENABLE));
            let _ = wait_register(|| platform.read32(op + 4).unwrap_or(0), USBSTS_HALTED, USBSTS_HALTED, poll_budget);
        }
        platform.unregister_interrupt();
        free_allocations(platform, allocations);
        return Err(RuntimeStartError::InterruptEnableFailed);
    }
    Ok(XhciRuntimeResources {
        dcbaa, command_ring, event_ring, erst, event_entries: RUNTIME_EVENT_ENTRIES,
        dequeue_index: 0, consumer_cycle: true, interrupt_registered: true, suspended: false,
    })
}
/// Drain live DMA event TRBs up to a caller-supplied budget. The callback must
/// be bounded and ISR/DPC-safe; this routine advances ERDP only after handling
/// each event. A single segment is used, so the consumer cycle toggles on wrap.
pub fn drain_event_ring<P, F>(
    platform: &mut P, caps: &Capabilities, resources: &mut XhciRuntimeResources,
    budget: u16, mut handle: F,
) -> Result<u16, XhciError>
where P: XhciRegisterIo, F: FnMut(Trb) {
    if resources.event_entries < 16 || !resources.event_entries.is_power_of_two()
        || resources.dequeue_index >= resources.event_entries {
        return Err(XhciError::InvalidRingSize);
    }
    let mut consumed = 0u16;
    while consumed < budget {
        let slot = resources.event_ring.cpu_address as *const Trb;
        // SAFETY: the platform keeps the DMA allocation mapped for this resource's lifetime.
        let event_ptr = unsafe { slot.add(resources.dequeue_index as usize) };
        // xHCI publishes the TRB payload before publishing the Cycle bit. Read
        // control first, then acquire-order the payload reads after ownership.
        let published_control = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*event_ptr).control)) };
        if (published_control & 1 != 0) != resources.consumer_cycle { break; }
        core::sync::atomic::fence(core::sync::atomic::Ordering::Acquire);
        let event = unsafe { core::ptr::read_volatile(event_ptr) };
        handle(Trb { parameter: event.parameter, status: event.status, control: published_control });
        resources.dequeue_index += 1;
        if resources.dequeue_index == resources.event_entries {
            resources.dequeue_index = 0;
            resources.consumer_cycle = !resources.consumer_cycle;
        }
        let dequeue_dma = resources.event_ring.device_address
            + resources.dequeue_index as u64 * core::mem::size_of::<Trb>() as u64;
        let erdp = caps.runtime_offset + 0x38;
        // EHB (bit 3) acknowledges the event-handler-busy state while the low
        // pointer bits remain naturally aligned and available for xHCI flags.
        platform.write32(erdp, (dequeue_dma as u32) | (1 << 3)).map_err(|_| XhciError::InvalidParameter)?;
        platform.write32(erdp + 4, (dequeue_dma >> 32) as u32).map_err(|_| XhciError::InvalidParameter)?;
        consumed += 1;
    }
    Ok(consumed)
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeStopError { InterruptDisableFailed, RegisterIoFailure, ControllerDidNotHalt }
/// Quiesce the controller before releasing DMA memory. On timeout, allocations
/// intentionally remain owned by the caller because hardware may still DMA into them.
pub fn stop_controller<P: XhciPlatform>(
    platform: &mut P, caps: &Capabilities, resources: XhciRuntimeResources, poll_budget: u32,
) -> Result<(), RuntimeStopError> {
    if resources.interrupt_registered && platform.set_interrupt_enabled(false).is_err() {
        return Err(RuntimeStopError::InterruptDisableFailed);
    }
    let op = caps.cap_length as u32;
    let command = platform.read32(op).map_err(|_| RuntimeStopError::RegisterIoFailure)?;
    platform.write32(op, command & !(USBCMD_RUN | USBCMD_INTERRUPT_ENABLE))
        .map_err(|_| RuntimeStopError::RegisterIoFailure)?;
    let mut halted = false;
    for _ in 0..poll_budget {
        let status = platform.read32(op + 4).map_err(|_| RuntimeStopError::RegisterIoFailure)?;
        if status & USBSTS_HALTED != 0 { halted = true; break; }
        core::hint::spin_loop();
    }
    if !halted { return Err(RuntimeStopError::ControllerDidNotHalt); }
    if resources.interrupt_registered { platform.unregister_interrupt(); }
    platform.free_dma(resources.erst);
    platform.free_dma(resources.event_ring);
    platform.free_dma(resources.command_ring);
    platform.free_dma(resources.dcbaa);
    Ok(())
}


pub const PORTSC_CCS:u32=1<<0;
pub const PORTSC_PLS_MASK:u32=0xf<<5;
pub const PORTSC_PP:u32=1<<9;
pub const PORTSC_LWS:u32=1<<16;
pub const PORTSC_WAKE_ENABLE_MASK:u32=0x7<<25;
pub const PORTSC_LINK_U0:u8=0;
pub const PORTSC_LINK_U3:u8=3;
pub const PORTSC_LINK_RESUME:u8=15;
const PORTSC_CHANGE_BITS:u32=0x7f<<17;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum XhciPowerError { InvalidPort, RegisterIoFailure, PortDisconnected, PortDidNotSuspend, PortDidNotResume, InterruptDisableFailed, ControllerDidNotHalt, ControllerDidNotStart, AlreadySuspended, NotSuspended }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PortPowerSnapshot { pub port:u8, pub connected:bool, pub prior_link_state:u8 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct XhciPowerSnapshot {
 command:u32, config:u32, dcbaap:u64, crcr:u64, iman:u32,
 erst_size:u32, erst_base:u64, erdp:u64,
}
fn portsc_offset(caps:&Capabilities,port:u8)->Result<u32,XhciPowerError>{
 if port==0||port>caps.max_ports{return Err(XhciPowerError::InvalidPort)}
 (caps.cap_length as u32).checked_add(0x400).and_then(|v|v.checked_add((port as u32-1)*0x10)).ok_or(XhciPowerError::InvalidPort)
}
fn write_port_link_state<I:XhciRegisterIo>(io:&mut I,offset:u32,current:u32,pls:u8)->Result<(),XhciPowerError>{
 let writable=(current&(PORTSC_PP|PORTSC_WAKE_ENABLE_MASK))&!PORTSC_CHANGE_BITS;
 io.write32(offset,writable|(((pls as u32)<<5)&PORTSC_PLS_MASK)|PORTSC_LWS).map_err(|_|XhciPowerError::RegisterIoFailure)
}
/// Suspend a connected port to U3. Parent-stack wait/wake policy remains the caller's responsibility.
pub fn suspend_port<I:XhciRegisterIo>(io:&mut I,caps:&Capabilities,port:u8,budget:u32)->Result<PortPowerSnapshot,XhciPowerError>{
 let off=portsc_offset(caps,port)?;
 let current=io.read32(off).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 if current&PORTSC_CCS==0{return Err(XhciPowerError::PortDisconnected)}
 let prior=((current&PORTSC_PLS_MASK)>>5) as u8;
 if prior!=PORTSC_LINK_U3 { write_port_link_state(io,off,current,PORTSC_LINK_U3)?; }
 for _ in 0..budget {
  let now=io.read32(off).map_err(|_|XhciPowerError::RegisterIoFailure)?;
  if ((now&PORTSC_PLS_MASK)>>5) as u8==PORTSC_LINK_U3 { return Ok(PortPowerSnapshot{port,connected:true,prior_link_state:prior}); }
  core::hint::spin_loop();
 }
 Err(XhciPowerError::PortDidNotSuspend)
}
/// Resume a suspended port; delay_us must provide the USB resume signalling interval.
pub fn resume_port<I:XhciRegisterIo,F:FnMut(u32)>(io:&mut I,caps:&Capabilities,snapshot:PortPowerSnapshot,budget:u32,mut delay_us:F)->Result<(),XhciPowerError>{
 if !snapshot.connected{return Ok(())}
 let off=portsc_offset(caps,snapshot.port)?;
 let current=io.read32(off).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 if current&PORTSC_CCS==0{return Err(XhciPowerError::PortDisconnected)}
 if ((current&PORTSC_PLS_MASK)>>5) as u8==PORTSC_LINK_U3 {
  write_port_link_state(io,off,current,PORTSC_LINK_RESUME)?;
  delay_us(20_000);
 }
 for _ in 0..budget {
  let now=io.read32(off).map_err(|_|XhciPowerError::RegisterIoFailure)?;
  if ((now&PORTSC_PLS_MASK)>>5) as u8==PORTSC_LINK_U0{return Ok(())}
  core::hint::spin_loop();
 }
 Err(XhciPowerError::PortDidNotResume)
}
/// Halt the xHC without freeing DMA rings or resetting slot contexts. Caller must quiesce transfers first.
pub fn suspend_controller_for_power<P:XhciPlatform>(platform:&mut P,caps:&Capabilities,resources:&mut XhciRuntimeResources,budget:u32)->Result<XhciPowerSnapshot,XhciPowerError>{
 if resources.suspended{return Err(XhciPowerError::AlreadySuspended)}
 let op=caps.cap_length as u32; let rt=caps.runtime_offset; let iman_off=rt+0x20;
 let command=platform.read32(op).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 let config=platform.read32(op+0x38).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 let dcbaap=((platform.read32(op+0x34).map_err(|_|XhciPowerError::RegisterIoFailure)? as u64)<<32)|platform.read32(op+0x30).map_err(|_|XhciPowerError::RegisterIoFailure)? as u64;
 let crcr=((platform.read32(op+0x1c).map_err(|_|XhciPowerError::RegisterIoFailure)? as u64)<<32)|platform.read32(op+0x18).map_err(|_|XhciPowerError::RegisterIoFailure)? as u64;
 let iman=platform.read32(iman_off).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 let erst_size=platform.read32(rt+0x28).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 let erst_base=((platform.read32(rt+0x34).map_err(|_|XhciPowerError::RegisterIoFailure)? as u64)<<32)|platform.read32(rt+0x30).map_err(|_|XhciPowerError::RegisterIoFailure)? as u64;
 let erdp=((platform.read32(rt+0x3c).map_err(|_|XhciPowerError::RegisterIoFailure)? as u64)<<32)|(platform.read32(rt+0x38).map_err(|_|XhciPowerError::RegisterIoFailure)? as u64&!0xf);
 if resources.interrupt_registered&&platform.set_interrupt_enabled(false).is_err(){return Err(XhciPowerError::InterruptDisableFailed)}
 platform.write32(iman_off,iman&!IMAN_INTERRUPT_ENABLE).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 platform.write32(op,command&!(USBCMD_RUN|USBCMD_INTERRUPT_ENABLE)).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 for _ in 0..budget {
  if platform.read32(op+4).map_err(|_|XhciPowerError::RegisterIoFailure)?&USBSTS_HALTED!=0 {
   resources.suspended=true;
   return Ok(XhciPowerSnapshot{command,config,dcbaap,crcr,iman,erst_size,erst_base,erdp});
  }
  core::hint::spin_loop();
 }
 Err(XhciPowerError::ControllerDidNotHalt)
}
/// Restore controller registers and existing DMA context before reenabling interrupts.
pub fn resume_controller_from_power<P:XhciPlatform>(platform:&mut P,caps:&Capabilities,resources:&mut XhciRuntimeResources,snapshot:XhciPowerSnapshot,budget:u32)->Result<(),XhciPowerError>{
 if !resources.suspended{return Err(XhciPowerError::NotSuspended)}
 let op=caps.cap_length as u32;let rt=caps.runtime_offset;
 let writes=[
  (op+0x18,snapshot.crcr as u32),(op+0x1c,(snapshot.crcr>>32) as u32),
  (op+0x30,snapshot.dcbaap as u32),(op+0x34,(snapshot.dcbaap>>32) as u32),
  (op+0x38,snapshot.config),(rt+0x28,snapshot.erst_size),
  (rt+0x30,snapshot.erst_base as u32),(rt+0x34,(snapshot.erst_base>>32) as u32),
  (rt+0x38,snapshot.erdp as u32),(rt+0x3c,(snapshot.erdp>>32) as u32),
  (rt+0x20,snapshot.iman|IMAN_INTERRUPT_ENABLE),
 ];
 for (offset,value) in writes {platform.write32(offset,value).map_err(|_|XhciPowerError::RegisterIoFailure)?;}
 platform.write32(op,(snapshot.command&!USBCMD_RESET)|USBCMD_RUN|USBCMD_INTERRUPT_ENABLE).map_err(|_|XhciPowerError::RegisterIoFailure)?;
 let mut running=false;
 for _ in 0..budget {if platform.read32(op+4).map_err(|_|XhciPowerError::RegisterIoFailure)?&USBSTS_HALTED==0{running=true;break}core::hint::spin_loop();}
 if !running{return Err(XhciPowerError::ControllerDidNotStart)}
 if resources.interrupt_registered&&platform.set_interrupt_enabled(true).is_err(){
  let _=platform.write32(op,snapshot.command&!(USBCMD_RUN|USBCMD_INTERRUPT_ENABLE));
  return Err(XhciPowerError::InterruptDisableFailed)
 }
 resources.suspended=false;Ok(())
}

/// Flags for the HAL DMA allocator. A successful allocation must provide a
/// CPU mapping and a device-visible address for the same pinned contiguous
/// range; COHERENT means CPU/device cache visibility is handled by the HAL.
pub const HAL_DMA_CONTIGUOUS: u32 = 1 << 0;
pub const HAL_DMA_COHERENT: u32 = 1 << 1;
pub const HAL_DMA_ZEROED: u32 = 1 << 2;

/// Adapter from the versioned CognacOS HAL service table to the xHCI core.
/// It does not invent physical addresses or implement DMA/interrupts itself.
pub struct HalBackedXhciPlatform {
    services: *const cognacos_hal_contract::DeviceServiceTable,
    mmio_base: u64,
    mmio_length: u64,
    interrupt_vector: u32,
    interrupt_context: u64,
    isr: Option<cognacos_hal_contract::InterruptIsr>,
    dpc: Option<cognacos_hal_contract::InterruptDpc>,
    registered: bool,
}
impl HalBackedXhciPlatform {
    /// # Safety
    /// services must remain readable and immutable for this adapter's lifetime.
    /// mmio_base must be a valid kernel virtual mapping of the assigned xHCI BAR
    /// with device memory attributes. Callback functions and context must remain valid.
    pub unsafe fn new(
        services: *const cognacos_hal_contract::DeviceServiceTable,
        mmio_base: u64, mmio_length: u64, interrupt_vector: u32,
        interrupt_context: u64,
        isr: Option<cognacos_hal_contract::InterruptIsr>,
        dpc: Option<cognacos_hal_contract::InterruptDpc>,
    ) -> Result<Self, ()> {
        if services.is_null() || mmio_base == 0 || mmio_length < 0x1000
            || mmio_base.checked_add(mmio_length).is_none()
            || interrupt_vector == 0 || isr.is_none() { return Err(()); }
        // SAFETY: caller guarantees the table pointer is readable.
        let table = unsafe { &*services };
        if !table.compatible() || !table.has_dma() || !table.has_interrupts() { return Err(()); }
        Ok(Self { services, mmio_base, mmio_length, interrupt_vector,
            interrupt_context, isr, dpc, registered: false })
    }
    fn table(&self) -> &cognacos_hal_contract::DeviceServiceTable {
        // SAFETY: upheld by constructor contract.
        unsafe { &*self.services }
    }
}
impl XhciRegisterIo for HalBackedXhciPlatform {
    fn read32(&mut self, offset: u32) -> Result<u32, ()> {
        if offset & 3 != 0 || (offset as u64).checked_add(4).filter(|end| *end <= self.mmio_length).is_none() { return Err(()); }
        let address = self.mmio_base.checked_add(offset as u64).ok_or(())?;
        // SAFETY: constructor guarantees a mapped MMIO span; offset is checked/aligned.
        Ok(unsafe { core::ptr::read_volatile(address as *const u32) })
    }
    fn write32(&mut self, offset: u32, value: u32) -> Result<(), ()> {
        if offset & 3 != 0 || (offset as u64).checked_add(4).filter(|end| *end <= self.mmio_length).is_none() { return Err(()); }
        let address = self.mmio_base.checked_add(offset as u64).ok_or(())?;
        // SAFETY: constructor guarantees a mapped MMIO span; offset is checked/aligned.
        unsafe { core::ptr::write_volatile(address as *mut u32, value); }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}
impl XhciPlatform for HalBackedXhciPlatform {
    fn allocate_dma(&mut self, size: u64, alignment: u64, max_device_address: u64) -> Result<DmaAllocation, ()> {
        if size == 0 || alignment < 16 || !alignment.is_power_of_two() { return Err(()); }
        let allocate = self.table().allocate_dma.ok_or(())?;
        let mut cpu = 0u64; let mut device = 0u64;
        let status = unsafe { allocate(size, alignment, max_device_address,
            HAL_DMA_CONTIGUOUS | HAL_DMA_COHERENT | HAL_DMA_ZEROED, &mut cpu, &mut device) };
        if status.code != 0 { return Err(()); }
        Ok(DmaAllocation { cpu_address: cpu, device_address: device, size })
    }
    fn free_dma(&mut self, allocation: DmaAllocation) {
        if let Some(free) = self.table().free_dma {
            let _ = unsafe { free(allocation.cpu_address, allocation.device_address, allocation.size) };
        }
    }
    fn register_interrupt(&mut self) -> Result<(), ()> {
        if self.registered { return Err(()); }
        let register = self.table().register_interrupt.ok_or(())?;
        let status = unsafe { register(self.interrupt_vector, self.interrupt_context, self.isr, self.dpc) };
        if status.code != 0 { return Err(()); }
        self.registered = true;
        Ok(())
    }
    fn unregister_interrupt(&mut self) {
        if self.registered {
            if let Some(unregister) = self.table().unregister_interrupt {
                let _ = unsafe { unregister(self.interrupt_vector, self.interrupt_context) };
            }
            self.registered = false;
        }
    }
    fn set_interrupt_enabled(&mut self, enabled: bool) -> Result<(), ()> {
        if !self.registered { return Err(()); }
        let set = self.table().set_interrupt_enabled.ok_or(())?;
        let status = unsafe { set(self.interrupt_vector, enabled) };
        if status.code == 0 { Ok(()) } else { Err(()) }
    }
}
#[cfg(test)]
mod hal_adapter_tests {
    use super::*;
    #[test]
    fn rejects_invalid_mmio_accesses_without_touching_hardware() {
        let mut words = [0u32; 4];
        let mut platform = HalBackedXhciPlatform {
            services: core::ptr::null(), mmio_base: words.as_mut_ptr() as u64,
            mmio_length: core::mem::size_of_val(&words) as u64, interrupt_vector: 1,
            interrupt_context: 0, isr: None, dpc: None, registered: false,
        };
        assert_eq!(platform.read32(1), Err(()));
        assert_eq!(platform.read32(16), Err(()));
        assert_eq!(platform.write32(2, 1), Err(()));
        assert_eq!(platform.read32(0), Ok(0));
        assert_eq!(platform.write32(4, 0x1234), Ok(()));
        assert_eq!(words[1], 0x1234);
    }
}


/// Producer state for the single-segment xHCI command ring. One TRB slot is
/// reserved for the Link TRB; the command ring is single-producer and must be
/// serialized by the caller (normally the controller's command lock).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandRingCursor {
    pub enqueue_index: u16,
    pub producer_cycle: bool,
}
impl CommandRingCursor {
    pub const fn new() -> Self { Self { enqueue_index: 0, producer_cycle: true } }
}
impl Default for CommandRingCursor { fn default() -> Self { Self::new() } }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandSubmitError {
    InvalidRing,
    InvalidCommand,
    TrackingFailed,
    RegisterIoFailure,
}

/// Publish one command TRB into a DMA-backed command ring and ring Doorbell 0.
/// entry_count includes the reserved Link TRB. The command buffer must remain
/// pinned/coherent and cursor must be protected against concurrent producers.
/// A successful return means submitted, not completed; completion is reported
/// separately through the event ring's Command Completion Event.
pub fn submit_command<I: XhciRegisterIo>(
    io: &mut I,
    caps: &Capabilities,
    ring_cpu: u64,
    ring_dma: u64,
    entry_count: u16,
    cursor: &mut CommandRingCursor,
    mut command: Trb,
) -> Result<u64, CommandSubmitError> {
    if entry_count < 4 || !entry_count.is_power_of_two()
        || ring_cpu == 0 || ring_cpu & 0xf != 0
        || validate_dma_address(ring_dma, 64).is_err()
        || cursor.enqueue_index >= entry_count - 1
        || caps.doorbell_offset < caps.cap_length as u32
        || caps.doorbell_offset & 3 != 0 {
        return Err(CommandSubmitError::InvalidRing);
    }
    let ty = command.trb_type();
    if ty == 0 || ty == TRB_TYPE_LINK as u8 || ty >= TRB_TYPE_TRANSFER_EVENT as u8 {
        return Err(CommandSubmitError::InvalidCommand);
    }
    let index = cursor.enqueue_index as usize;
    let address = ring_cpu.checked_add((index * core::mem::size_of::<Trb>()) as u64)
        .ok_or(CommandSubmitError::InvalidRing)?;
    command.control = (command.control & !1) | u32::from(cursor.producer_cycle);
    // SAFETY: caller supplies a writable, pinned DMA ring with entry_count slots.
    unsafe { core::ptr::write_volatile(address as *mut Trb, command); }
    core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
    let submitted_dma = ring_dma.checked_add((index * core::mem::size_of::<Trb>()) as u64)
        .ok_or(CommandSubmitError::InvalidRing)?;
    cursor.enqueue_index += 1;
    if cursor.enqueue_index == entry_count - 1 {
        let link = Trb {
            parameter: ring_dma,
            status: 0,
            control: (TRB_TYPE_LINK << TRB_TYPE_SHIFT) | (1 << 1)
                | u32::from(cursor.producer_cycle),
        };
        let link_address = ring_cpu.checked_add((entry_count as u64 - 1)
            * core::mem::size_of::<Trb>() as u64).ok_or(CommandSubmitError::InvalidRing)?;
        // SAFETY: last ring slot is reserved for the Link TRB.
        unsafe { core::ptr::write_volatile(link_address as *mut Trb, link); }
        cursor.enqueue_index = 0;
        cursor.producer_cycle = !cursor.producer_cycle;
    }
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    // Doorbell 0 is the command ring doorbell; its target is always zero.
    io.write32(caps.doorbell_offset, 0).map_err(|_| CommandSubmitError::RegisterIoFailure)?;
    Ok(submitted_dma)
}

#[cfg(test)]
mod command_submission_tests {
    use super::*;
    struct FakeIo { regs: [u32; 128], writes: [u32; 8], count: usize }
    impl FakeIo { fn new() -> Self { Self { regs: [0;128], writes: [u32::MAX;8], count: 0 } } }
    impl XhciRegisterIo for FakeIo {
        fn read32(&mut self, o:u32)->Result<u32,()> { self.regs.get((o/4) as usize).copied().ok_or(()) }
        fn write32(&mut self,o:u32,v:u32)->Result<(),()> {
            let i=(o/4) as usize; if i>=self.regs.len(){return Err(())}
            self.regs[i]=v;
            if self.count < self.writes.len() { self.writes[self.count]=o; }
            self.count+=1; Ok(())
        }
    }
    fn caps()->Capabilities { Capabilities { cap_length:0x40,version:0x100,max_slots:8,
        max_interrupters:1,max_ports:4,doorbell_offset:0x100,runtime_offset:0x200,
        context_size:32,page_size_mask:1 } }
    #[test]
    fn submits_command_and_rings_command_doorbell() {
        let mut ring=[Trb::default();8]; let mut io=FakeIo::new();
        let mut cursor=CommandRingCursor::new();
        let cmd=Trb::new(0x1234,0,TRB_TYPE_ENABLE_SLOT,true);
        assert_eq!(submit_command(&mut io,&caps(),ring.as_mut_ptr() as u64,0x1000,8,&mut cursor,cmd),Ok(0x1000));
        assert_eq!(unsafe { core::ptr::read_volatile(ring.as_ptr()) }.trb_type(),TRB_TYPE_ENABLE_SLOT as u8);
        assert!(unsafe { core::ptr::read_volatile(ring.as_ptr()) }.cycle());
        assert_eq!(io.writes[0],0x100);
        assert_eq!(io.regs[0x100/4],0);
        assert_eq!(cursor.enqueue_index,1);
    }
    #[test]
    fn writes_link_and_toggles_cycle_at_segment_end() {
        let mut ring=[Trb::default();4]; let mut io=FakeIo::new();
        let mut cursor=CommandRingCursor::new();
        for _ in 0..3 {
            submit_command(&mut io,&caps(),ring.as_mut_ptr() as u64,0x1000,4,&mut cursor,
                Trb::new(0,0,TRB_TYPE_ENABLE_SLOT,true)).unwrap();
        }
        let link=unsafe { core::ptr::read_volatile(ring.as_ptr().add(3)) };
        assert_eq!(link.trb_type(),TRB_TYPE_LINK as u8);
        assert_eq!(link.parameter,0x1000);
        assert_ne!(link.control & (1<<1),0);
        assert_eq!(cursor.enqueue_index,0);
        assert!(!cursor.producer_cycle);
    }
    #[test]
    fn rejects_bad_ring_and_event_trbs() {
        let mut ring=[Trb::default();8]; let mut io=FakeIo::new();
        let mut cursor=CommandRingCursor::new();
        assert_eq!(submit_command(&mut io,&caps(),ring.as_mut_ptr() as u64,0x1000,3,&mut cursor,
            Trb::new(0,0,TRB_TYPE_ENABLE_SLOT,true)),Err(CommandSubmitError::InvalidRing));
        assert_eq!(submit_command(&mut io,&caps(),ring.as_mut_ptr() as u64,0x1000,8,&mut cursor,
            Trb::new(0,0,TRB_TYPE_COMMAND_COMPLETION,true)),Err(CommandSubmitError::InvalidCommand));
        assert_eq!(io.count,0);
    }
}


/// Command kinds tracked until the controller reports a Command Completion Event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind { EnableSlot, AddressDevice, ConfigureEndpoint, Noop }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingCommand {
    pub command_trb_dma: u64,
    pub kind: CommandKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandCompletion {
    pub command: PendingCommand,
    pub completion_code: u8,
    pub slot_id: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionTrackError { Full, InvalidEvent, UnknownCommand }

/// Fixed-capacity command correlation table. Insert immediately after publishing
/// a command and before allowing the controller's interrupt path to process events.
pub struct CommandCompletionTracker<const N: usize> {
    entries: [Option<PendingCommand>; N],
}
impl<const N: usize> CommandCompletionTracker<N> {
    pub const fn new() -> Self { Self { entries: [None; N] } }
    pub fn track(&mut self, command: PendingCommand) -> Result<(), CompletionTrackError> {
        if command.command_trb_dma == 0 || command.command_trb_dma & 0xf != 0 {
            return Err(CompletionTrackError::InvalidEvent);
        }
        if self.entries.iter().flatten().any(|p| p.command_trb_dma == command.command_trb_dma) {
            return Err(CompletionTrackError::InvalidEvent);
        }
        let slot = self.entries.iter_mut().find(|e| e.is_none()).ok_or(CompletionTrackError::Full)?;
        *slot = Some(command);
        Ok(())
    }
    /// Correlate an already cycle-validated event TRB. Unknown completions are
    /// surfaced rather than silently discarded; callers should log them.
    pub fn complete(&mut self, event: Trb) -> Result<CommandCompletion, CompletionTrackError> {
        if event.trb_type() != TRB_TYPE_COMMAND_COMPLETION as u8 {
            return Err(CompletionTrackError::InvalidEvent);
        }
        let command_dma = event.parameter & !0xf;
        let index = self.entries.iter().position(|e|
            e.map(|p| p.command_trb_dma == command_dma).unwrap_or(false)
        ).ok_or(CompletionTrackError::UnknownCommand)?;
        let command = self.entries[index].take().ok_or(CompletionTrackError::UnknownCommand)?;
        Ok(CommandCompletion {
            command,
            completion_code: event.completion_code(),
            slot_id: event.slot_id(),
        })
    }
    pub fn cancel(&mut self, command_trb_dma: u64) -> bool {
        if let Some(slot) = self.entries.iter_mut().find(|slot| slot.as_ref().is_some_and(|p| p.command_trb_dma == command_trb_dma)) { *slot = None; true } else { false }
    }
    pub fn pending_count(&self) -> usize { self.entries.iter().filter(|e| e.is_some()).count() }
}
impl<const N: usize> Default for CommandCompletionTracker<N> {
    fn default() -> Self { Self::new() }
}

/// Build an Enable Slot command. The controller chooses the slot ID on completion.
pub const fn enable_slot_command() -> Trb {
    Trb::new(0, 0, TRB_TYPE_ENABLE_SLOT, true)
}

/// Build an Address Device command. input_context_dma must refer to a valid,
/// aligned, DMA-visible Input Context owned by the caller.
pub fn address_device_command(input_context_dma: u64, slot_id: u8) -> Result<Trb, XhciError> {
    if slot_id == 0 || validate_dma_address(input_context_dma, 16).is_err() {
        return Err(XhciError::InvalidParameter);
    }
    let mut trb = Trb::new(input_context_dma, 0, TRB_TYPE_ADDRESS_DEVICE, true);
    trb.control = (trb.control & 0x00ff_ffff) | ((slot_id as u32) << 24);
    Ok(trb)
}

/// Build a Configure Endpoint command. Input context contents and endpoint
/// descriptors must already be validated by the USB/device layer.
pub fn configure_endpoint_command(input_context_dma: u64, slot_id: u8) -> Result<Trb, XhciError> {
    if slot_id == 0 || validate_dma_address(input_context_dma, 16).is_err() {
        return Err(XhciError::InvalidParameter);
    }
    let mut trb = Trb::new(input_context_dma, 0, TRB_TYPE_CONFIGURE_ENDPOINT, true);
    trb.control = (trb.control & 0x00ff_ffff) | ((slot_id as u32) << 24);
    Ok(trb)
}

#[cfg(test)]
mod command_completion_tests {
    use super::*;
    #[test]
    fn correlates_completion_by_command_trb_pointer() {
        let mut tracker = CommandCompletionTracker::<2>::new();
        let pending = PendingCommand { command_trb_dma: 0x1040, kind: CommandKind::EnableSlot };
        tracker.track(pending).unwrap();
        let mut event = Trb::new(0x1040, 1 << 24, TRB_TYPE_COMMAND_COMPLETION, true);
        event.control |= 7 << 24;
        let done = tracker.complete(event).unwrap();
        assert_eq!(done.command, pending);
        assert_eq!(done.completion_code, 1);
        assert_eq!(done.slot_id, 7);
        assert_eq!(tracker.pending_count(), 0);
    }
    #[test]
    fn rejects_unknown_and_non_completion_events() {
        let mut tracker = CommandCompletionTracker::<1>::new();
        assert_eq!(tracker.complete(Trb::new(0,0,TRB_TYPE_TRANSFER_EVENT,true)),
            Err(CompletionTrackError::InvalidEvent));
        assert_eq!(tracker.complete(Trb::new(0x1000,1<<24,TRB_TYPE_COMMAND_COMPLETION,true)),
            Err(CompletionTrackError::UnknownCommand));
    }
    #[test]
    fn validates_slot_and_input_context_before_building_commands() {
        assert_eq!(address_device_command(0x1000,0).unwrap_err(),XhciError::InvalidParameter);
        assert_eq!(configure_endpoint_command(0x1001,1).unwrap_err(),XhciError::InvalidParameter);
        assert_eq!(address_device_command(0x1000,3).unwrap().slot_id(),3);
        assert_eq!(configure_endpoint_command(0x2000,4).unwrap().slot_id(),4);
        assert_eq!(enable_slot_command().trb_type(),TRB_TYPE_ENABLE_SLOT as u8);
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerEvent { CommandCompleted(CommandCompletion), TransferEvent(Trb), PortStatusChanged(Trb), Other(Trb) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerEventError { Completion(CompletionTrackError) }

/// Install completion tracking before the doorbell can make a command visible.
pub fn submit_tracked_command<I: XhciRegisterIo, const N: usize>(
 io: &mut I, caps: &Capabilities, ring_cpu: u64, ring_dma: u64, entry_count: u16,
 cursor: &mut CommandRingCursor, tracker: &mut CommandCompletionTracker<N>,
 kind: CommandKind, command: Trb,
) -> Result<u64, CommandSubmitError> {
 if entry_count < 4 || !entry_count.is_power_of_two() || cursor.enqueue_index >= entry_count - 1 || ring_dma == 0 || ring_dma & 0x3f != 0 {
  return Err(CommandSubmitError::InvalidRing);
 }
 let dma=ring_dma.checked_add(cursor.enqueue_index as u64 * core::mem::size_of::<Trb>() as u64).ok_or(CommandSubmitError::InvalidRing)?;
 tracker.track(PendingCommand{command_trb_dma:dma,kind}).map_err(|_|CommandSubmitError::TrackingFailed)?;
 match submit_command(io,caps,ring_cpu,ring_dma,entry_count,cursor,command) {
  Ok(actual) if actual==dma=>Ok(actual),
  Ok(_)=>{tracker.cancel(dma);Err(CommandSubmitError::InvalidRing)},
  Err(e)=>{tracker.cancel(dma);Err(e)}
 }
}
/// Route an event already accepted by drain_event_ring; the caller handles transfer and port events.
pub fn route_controller_event<const N:usize>(tracker:&mut CommandCompletionTracker<N>,event:Trb)->Result<ControllerEvent,ControllerEventError>{
 match event.trb_type() as u32 {
  TRB_TYPE_COMMAND_COMPLETION=>tracker.complete(event).map(ControllerEvent::CommandCompleted).map_err(ControllerEventError::Completion),
  TRB_TYPE_TRANSFER_EVENT=>Ok(ControllerEvent::TransferEvent(event)),
  TRB_TYPE_PORT_STATUS_CHANGE=>Ok(ControllerEvent::PortStatusChanged(event)),
  _=>Ok(ControllerEvent::Other(event)),
 }
}


#[cfg(test)]
mod controller_event_routing_tests {
 use super::*;
 #[test]
 fn live_command_completion_is_correlated_to_pending_command() {
  let mut tracker=CommandCompletionTracker::<2>::new();
  let command_dma=0x4000;
  assert_eq!(tracker.track(PendingCommand{command_trb_dma:command_dma,kind:CommandKind::EnableSlot}),Ok(()));
  let mut event=Trb::new(command_dma,(1u32)<<24,TRB_TYPE_COMMAND_COMPLETION,true);
  event.control|=3u32<<24;
  assert_eq!(route_controller_event(&mut tracker,event),Ok(ControllerEvent::CommandCompleted(CommandCompletion{command:PendingCommand{command_trb_dma:command_dma,kind:CommandKind::EnableSlot},completion_code:1,slot_id:3})));
  assert_eq!(tracker.pending_count(),0);
 }
 #[test]
 fn port_and_transfer_events_are_routed_without_consuming_command_reservations() {
  let mut tracker=CommandCompletionTracker::<2>::new();
  assert!(matches!(route_controller_event(&mut tracker,Trb::new(0,0,TRB_TYPE_PORT_STATUS_CHANGE,true)),Ok(ControllerEvent::PortStatusChanged(_))));
  assert!(matches!(route_controller_event(&mut tracker,Trb::new(0,0,TRB_TYPE_TRANSFER_EVENT,true)),Ok(ControllerEvent::TransferEvent(_))));
  assert_eq!(tracker.pending_count(),0);
 }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerDrainError { Ring(XhciError), Event(ControllerEventError) }

/// Consume live DMA event-ring entries, advance ERDP through drain_event_ring,
/// and route command completions into the pending tracker in the same path.
/// The callback should enqueue transfer/port work to a DPC or worker; keep it bounded.
pub fn drain_controller_events<P, F, const N: usize>(
 platform: &mut P, caps: &Capabilities, resources: &mut XhciRuntimeResources,
 budget: u16, tracker: &mut CommandCompletionTracker<N>, mut handle: F,
) -> Result<u16, ControllerDrainError>
where P: XhciRegisterIo, F: FnMut(ControllerEvent) {
 let mut routing_error=None;
 let count=drain_event_ring(platform,caps,resources,budget,|event| {
  match route_controller_event(tracker,event) {
   Ok(routed)=>handle(routed),
   Err(e)=>routing_error=Some(e),
  }
 }).map_err(ControllerDrainError::Ring)?;
 if let Some(e)=routing_error { Err(ControllerDrainError::Event(e)) } else { Ok(count) }
}

#[cfg(test)]
mod power_state_tests {
 use super::*;
 struct PortMmio { regs:[u32;512], delayed:u32 }
 impl PortMmio { fn new()->Self { let mut s=Self{regs:[0;512],delayed:0};s.regs[0x440/4]=PORTSC_CCS|PORTSC_PP; s } }
 impl XhciRegisterIo for PortMmio {
  fn read32(&mut self,o:u32)->Result<u32,()>{self.regs.get((o/4) as usize).copied().ok_or(())}
  fn write32(&mut self,o:u32,v:u32)->Result<(),()>{
   let i=(o/4) as usize;if i>=self.regs.len(){return Err(())}
   if o==0x440 {
    let pls=(v&PORTSC_PLS_MASK)>>5;
    let old=self.regs[i];
    let state=if pls==PORTSC_LINK_RESUME as u32 {PORTSC_LINK_U0 as u32}else{pls};
    self.regs[i]=(old&!(PORTSC_PLS_MASK|PORTSC_LWS|PORTSC_CHANGE_BITS))|(state<<5)|(v&PORTSC_PP);
   } else {self.regs[i]=v}
   Ok(())
  }
 }
 fn caps()->Capabilities{Capabilities{cap_length:0x40,version:0x100,max_slots:8,max_interrupters:1,max_ports:4,doorbell_offset:0x1000,runtime_offset:0x200,context_size:32,page_size_mask:1}}
 #[test] fn connected_port_enters_u3_and_resumes_to_u0(){
  let mut mmio=PortMmio::new();
  let snapshot=suspend_port(&mut mmio,&caps(),1,8).unwrap();
  assert_eq!(snapshot.port,1);assert_eq!((mmio.regs[0x440/4]&PORTSC_PLS_MASK)>>5,PORTSC_LINK_U3 as u32);
  let mut delay=0;
  resume_port(&mut mmio,&caps(),snapshot,8,|us|delay=us).unwrap();
  assert_eq!(delay,20_000);
  assert_eq!((mmio.regs[0x440/4]&PORTSC_PLS_MASK)>>5,PORTSC_LINK_U0 as u32);
 }
 #[test] fn disconnected_port_is_not_claimed_suspended(){
  let mut mmio=PortMmio{regs:[0;512],delayed:0};
  assert_eq!(suspend_port(&mut mmio,&caps(),1,8),Err(XhciPowerError::PortDisconnected));
  assert_eq!(suspend_port(&mut mmio,&caps(),0,8),Err(XhciPowerError::InvalidPort));
 }
}

#[cfg(test)]
mod controller_power_tests {
 use super::*;
 #[repr(align(64))] struct Dma([u8;32768]);
 struct FakePlatform { regs:[u32;1024], dma:Dma, next:usize, irq_registered:bool, irq_enabled:bool }
 impl FakePlatform { fn new()->Self{let mut p=Self{regs:[0;1024],dma:Dma([0;32768]),next:0,irq_registered:false,irq_enabled:false};p.regs[0x44/4]=USBSTS_HALTED;p.regs[0x48/4]=1;p} }
 impl XhciRegisterIo for FakePlatform {
  fn read32(&mut self,o:u32)->Result<u32,()>{self.regs.get((o/4) as usize).copied().ok_or(())}
  fn write32(&mut self,o:u32,v:u32)->Result<(),()>{
   let i=(o/4) as usize;if i>=self.regs.len(){return Err(())}
   if o==0x40 {
    if v&USBCMD_RESET!=0 {self.regs[i]=v&!USBCMD_RESET;self.regs[0x44/4]=USBSTS_HALTED;}
    else if v&USBCMD_RUN!=0 {self.regs[i]=v;self.regs[0x44/4]=0;}
    else {self.regs[i]=v;self.regs[0x44/4]=USBSTS_HALTED;}
   } else {self.regs[i]=v}
   Ok(())
  }
 }
 impl XhciPlatform for FakePlatform {
  fn allocate_dma(&mut self,size:u64,alignment:u64,_max:u64)->Result<DmaAllocation,()>{
   let mask=alignment as usize-1;self.next=(self.next+mask)&!mask;
   let end=self.next.checked_add(size as usize).ok_or(())?;if end>self.dma.0.len(){return Err(())}
   let a=DmaAllocation{cpu_address:unsafe{self.dma.0.as_mut_ptr().add(self.next)} as u64,device_address:0x10000+self.next as u64,size};
   self.next=end;Ok(a)
  }
  fn free_dma(&mut self,_:DmaAllocation){}
  fn register_interrupt(&mut self)->Result<(),()>{self.irq_registered=true;Ok(())}
  fn unregister_interrupt(&mut self){self.irq_registered=false;self.irq_enabled=false;}
  fn set_interrupt_enabled(&mut self,e:bool)->Result<(),()>{if e&&!self.irq_registered{return Err(())}self.irq_enabled=e;Ok(())}
 }
 fn caps()->Capabilities{Capabilities{cap_length:0x40,version:0x100,max_slots:8,max_interrupters:1,max_ports:4,doorbell_offset:0x1000,runtime_offset:0x200,context_size:32,page_size_mask:1}}
 #[test] fn d0_exit_and_entry_preserve_dma_context_and_restore_interrupts(){
  let mut platform=FakePlatform::new();
  let mut resources=start_controller(&mut platform,&caps(),4,16).unwrap();
  let dcbaa=resources.dcbaa.device_address;
  assert!(platform.irq_enabled);assert!(!resources.suspended);
  let snapshot=suspend_controller_for_power(&mut platform,&caps(),&mut resources,16).unwrap();
  assert!(resources.suspended);assert!(!platform.irq_enabled);assert!(platform.irq_registered);
  assert_eq!(resources.dcbaa.device_address,dcbaa);
  resume_controller_from_power(&mut platform,&caps(),&mut resources,snapshot,16).unwrap();
  assert!(!resources.suspended);assert!(platform.irq_enabled);
  assert_eq!(platform.regs[(0x40+0x30)/4],dcbaa as u32);
  assert_eq!(platform.regs[0x40/4]&USBCMD_RUN,USBCMD_RUN);
 }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct UsbWakePath { pub port_connected:bool, pub device_supports_remote_wake:bool, pub hub_supports_wake:bool, pub controller_supports_wake:bool }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum UsbWakeError { UnsupportedPath, ControllerFailure, HubFailure, DeviceFailure }
/// Backend operations are supplied by the real USB bus/function driver. Arming
/// is ordered host-controller -> hub -> device; failures roll back in reverse.
pub trait UsbWakeBackend {
 fn arm_controller_wake(&mut self)->Result<(),()>;
 fn arm_hub_wake(&mut self)->Result<(),()>;
 fn enable_device_remote_wakeup(&mut self)->Result<(),()>;
 fn disable_device_remote_wakeup(&mut self)->Result<(),()>;
 fn disarm_hub_wake(&mut self)->Result<(),()>;
 fn disarm_controller_wake(&mut self)->Result<(),()>;
}
pub fn arm_usb_wait_wake<B:UsbWakeBackend>(path:UsbWakePath,backend:&mut B)->Result<(),UsbWakeError>{
 if !path.port_connected||!path.device_supports_remote_wake||!path.hub_supports_wake||!path.controller_supports_wake{return Err(UsbWakeError::UnsupportedPath)}
 backend.arm_controller_wake().map_err(|_|UsbWakeError::ControllerFailure)?;
 if backend.arm_hub_wake().is_err(){let _=backend.disarm_controller_wake();return Err(UsbWakeError::HubFailure)}
 if backend.enable_device_remote_wakeup().is_err(){let _=backend.disarm_hub_wake();let _=backend.disarm_controller_wake();return Err(UsbWakeError::DeviceFailure)}
 Ok(())
}
pub fn cancel_usb_wait_wake<B:UsbWakeBackend>(backend:&mut B)->Result<(),UsbWakeError>{
 backend.disable_device_remote_wakeup().map_err(|_|UsbWakeError::DeviceFailure)?;
 backend.disarm_hub_wake().map_err(|_|UsbWakeError::HubFailure)?;
 backend.disarm_controller_wake().map_err(|_|UsbWakeError::ControllerFailure)?;
 Ok(())
}

#[cfg(test)]
mod usb_wake_chain_tests {
 use super::*;
 struct Backend { order:[u8;8], len:usize, fail:u8 }
 impl Backend { fn new(fail:u8)->Self{Self{order:[0;8],len:0,fail}} fn push(&mut self,v:u8)->Result<(),()>{self.order[self.len]=v;self.len+=1;if self.fail==v{Err(())}else{Ok(())}} }
 impl UsbWakeBackend for Backend {
  fn arm_controller_wake(&mut self)->Result<(),()>{self.push(1)}
  fn arm_hub_wake(&mut self)->Result<(),()>{self.push(2)}
  fn enable_device_remote_wakeup(&mut self)->Result<(),()>{self.push(3)}
  fn disable_device_remote_wakeup(&mut self)->Result<(),()>{self.push(4)}
  fn disarm_hub_wake(&mut self)->Result<(),()>{self.push(5)}
  fn disarm_controller_wake(&mut self)->Result<(),()>{self.push(6)}
 }
 fn path()->UsbWakePath{UsbWakePath{port_connected:true,device_supports_remote_wake:true,hub_supports_wake:true,controller_supports_wake:true}}
 #[test] fn wake_is_armed_controller_to_hub_to_device_and_cancelled_in_reverse(){
  let mut b=Backend::new(0);assert_eq!(arm_usb_wait_wake(path(),&mut b),Ok(()));assert_eq!(&b.order[..3],&[1,2,3]);
  assert_eq!(cancel_usb_wait_wake(&mut b),Ok(()));assert_eq!(&b.order[3..6],&[4,5,6]);
 }
 #[test] fn unsupported_path_is_rejected_before_touching_hardware(){
  let mut p=path();p.hub_supports_wake=false;let mut b=Backend::new(0);
  assert_eq!(arm_usb_wait_wake(p,&mut b),Err(UsbWakeError::UnsupportedPath));assert_eq!(b.len,0);
 }
 #[test] fn device_failure_rolls_back_hub_and_controller_wake(){
  let mut b=Backend::new(3);assert_eq!(arm_usb_wait_wake(path(),&mut b),Err(UsbWakeError::DeviceFailure));assert_eq!(&b.order[..5],&[1,2,3,5,6]);
 }
}

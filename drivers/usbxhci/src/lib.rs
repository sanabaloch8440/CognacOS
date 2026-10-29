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
 let hcs1=u32at(b,4)?;let hcc1=u32at(b,0x10)?;\n // PAGESIZE lives in the operational register block at CAPLENGTH + 0x08,\n // not in the capability-register prefix; query it only after MMIO mapping.\n let page_size_mask=0;
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
  let b=[9,2,20,0,1,1,0,0x80,50,1,4];
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
pub enum ControllerInitError{InvalidCapabilities,InvalidDmaConfiguration,InterruptNotRegistered,RegisterIoFailure,ResetTimeout,ControllerDidNotHalt,ControllerDidNotStart}
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
 impl FakeMmio{fn new()->Self{let mut x=Self{regs:[0;256]};x.regs[0x44/4]=USBSTS_HALTED;x}}
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

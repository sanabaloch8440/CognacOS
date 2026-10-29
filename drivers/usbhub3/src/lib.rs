//! USB hub-layer logic for CognacOS. This crate owns hub/port state and
//! descriptor-driven enumeration policy; xHCI register/ring mechanics live in
//! cognacos-usbxhci and HID report interpretation belongs to HIDClass/KbdHid.
#![no_std]
#[cfg(test)] extern crate std;

pub const USB_DESC_DEVICE:u8=1;
pub const USB_DESC_CONFIGURATION:u8=2;
pub const USB_CLASS_HUB:u8=9;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct UsbDeviceDescriptor {
    pub usb_version:u16, pub device_class:u8, pub vendor_id:u16, pub product_id:u16,
    pub max_packet_size0:u8, pub num_configurations:u8,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DescriptorError { Truncated, InvalidLength, WrongDescriptor, InvalidConfigurationLength }

/// Parses the fixed standard device descriptor only; the caller must first
/// fetch the first 8 bytes to learn EP0 packet sizing, then fetch all 18 bytes.
pub fn parse_device_descriptor(b:&[u8])->Result<UsbDeviceDescriptor,DescriptorError>{
    if b.len()<18 { return Err(DescriptorError::Truncated); }
    if b[0]<18 || b[1]!=USB_DESC_DEVICE { return Err(DescriptorError::WrongDescriptor); }
    Ok(UsbDeviceDescriptor {
        usb_version:u16::from_le_bytes([b[2],b[3]]), device_class:b[4],
        max_packet_size0:b[7], vendor_id:u16::from_le_bytes([b[8],b[9]]),
        product_id:u16::from_le_bytes([b[10],b[11]]), num_configurations:b[17],
    })
}

/// Validates a complete configuration descriptor chain and returns its
/// declared length. Every descriptor is bounds-checked before it is consumed.
pub fn validate_configuration_descriptor(b:&[u8])->Result<usize,DescriptorError>{
    if b.len()<9 { return Err(DescriptorError::Truncated); }
    if b[0]<9 || b[1]!=USB_DESC_CONFIGURATION { return Err(DescriptorError::WrongDescriptor); }
    let total=u16::from_le_bytes([b[2],b[3]]) as usize;
    if total<9 || total>b.len() { return Err(DescriptorError::InvalidConfigurationLength); }
    let mut off=0usize;
    while off<total {
        if off+2>total { return Err(DescriptorError::Truncated); }
        let len=b[off] as usize;
        if len<2 || off.checked_add(len).filter(|end|*end<=total).is_none() {
            return Err(DescriptorError::InvalidLength);
        }
        off+=len;
    }
    Ok(total)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PortState { Disconnected, Connected, Resetting, Enabled, Enumerating, Active, Removing, Failed }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PortEvent { Connect, BeginReset, ResetComplete, BeginEnumeration, EnumerationComplete, Disconnect, Failure }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum HubError { InvalidPort, InvalidTransition, PortCountOutOfRange, Descriptor(DescriptorError) }

pub struct HubPort { pub port_number:u8, state:PortState, generation:u64 }
impl HubPort {
    pub const fn new(port_number:u8)->Self { Self { port_number, state:PortState::Disconnected, generation:0 } }
    pub const fn state(&self)->PortState { self.state }
    /// Generation changes on every disconnect/failure to invalidate late transfer completions.
    pub const fn generation(&self)->u64 { self.generation }
    pub fn transition(&mut self,event:PortEvent)->Result<PortState,HubError>{
        use PortEvent as E; use PortState as S;
        let next=match (self.state,event) {
            (S::Disconnected,E::Connect)=>S::Connected,
            (S::Connected,E::BeginReset)=>S::Resetting,
            (S::Resetting,E::ResetComplete)=>S::Enabled,
            (S::Enabled,E::BeginEnumeration)=>S::Enumerating,
            (S::Enumerating,E::EnumerationComplete)=>S::Active,
            (S::Active,E::Disconnect)|(S::Enumerating,E::Disconnect)|(S::Enabled,E::Disconnect)|
            (S::Resetting,E::Disconnect)|(S::Connected,E::Disconnect)=>S::Removing,
            (S::Removing,E::Disconnect)=>S::Disconnected,
            (_,E::Failure)=>S::Failed,
            (S::Failed,E::Disconnect)|(S::Removing,E::Failure)=>S::Disconnected,
            _=>return Err(HubError::InvalidTransition),
        };
        if next==S::Removing || next==S::Disconnected || next==S::Failed {
            self.generation=self.generation.wrapping_add(1);
        }
        self.state=next; Ok(next)
    }
}
pub struct Hub {
    ports:[HubPort;255],
    count:u8,
}
impl Hub {
    pub const fn new()->Self { Self { ports:[const { HubPort::new(0) };255], count:0 } }
    pub fn configure_ports(&mut self,count:u8)->Result<(),HubError>{
        if count==0 || count>255 { return Err(HubError::PortCountOutOfRange); }
        self.count=count;
        let mut i=0; while i<count as usize { self.ports[i]=HubPort::new((i+1) as u8); i+=1; }
        Ok(())
    }
    pub fn port(&self,port:u8)->Result<&HubPort,HubError>{
        if port==0 || port>self.count { Err(HubError::InvalidPort) } else { Ok(&self.ports[(port-1) as usize]) }
    }
    pub fn port_mut(&mut self,port:u8)->Result<&mut HubPort,HubError>{
        if port==0 || port>self.count { Err(HubError::InvalidPort) } else { Ok(&mut self.ports[(port-1) as usize]) }
    }
}
impl Default for Hub { fn default()->Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn descriptor_parser_checks_lengths_and_fields(){
        let b=[18,1,0,2,0,0,0,8,0x34,0x12,0x78,0x56,0,1,1,2,3,1];
        let d=parse_device_descriptor(&b).unwrap();
        assert_eq!(d.vendor_id,0x1234); assert_eq!(d.product_id,0x5678); assert_eq!(d.num_configurations,1);
        assert_eq!(parse_device_descriptor(&b[..8]),Err(DescriptorError::Truncated));
    }
    #[test] fn configuration_chain_rejects_bad_lengths(){
        let good=[9,2,9,0,0,1,0,0x80,50]; assert_eq!(validate_configuration_descriptor(&good),Ok(9));
        let bad=[9,2,11,0,1,1,0,0x80,50,1,4]; assert_eq!(validate_configuration_descriptor(&bad),Err(DescriptorError::InvalidLength));
    }
    #[test] fn port_lifecycle_rejects_skips_and_invalidates_late_completions(){
        let mut h=Hub::new(); h.configure_ports(2).unwrap();
        assert_eq!(h.port_mut(1).unwrap().transition(PortEvent::BeginReset),Err(HubError::InvalidTransition));
        for e in [PortEvent::Connect,PortEvent::BeginReset,PortEvent::ResetComplete,PortEvent::BeginEnumeration,PortEvent::EnumerationComplete] { h.port_mut(1).unwrap().transition(e).unwrap(); }
        assert_eq!(h.port(1).unwrap().state(),PortState::Active);
        let generation=h.port(1).unwrap().generation();
        h.port_mut(1).unwrap().transition(PortEvent::Disconnect).unwrap();
        assert!(h.port(1).unwrap().generation()>generation);
        assert_eq!(h.port_mut(3),Err(HubError::InvalidPort));
    }
}

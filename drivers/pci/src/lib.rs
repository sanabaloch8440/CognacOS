//! PCIe Enhanced Configuration Access Mechanism (ECAM) primitives.
//!
//! The caller must map the firmware-described ECAM aperture with uncached,
//! device-memory attributes before calling these routines. This module does not
//! assume a universal ECAM base or silently identity-map arbitrary physical RAM.
#![no_std]

/// Canonical Windows NT ABI declarations shared with ntoskrnl and NTDLL.
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)]
extern crate std;
use core::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PciError { InvalidWindow, InvalidBdf, InvalidOffset, AddressOverflow, UnsupportedRange }

/// Convert PCI discovery/configuration failures to the shared NTSTATUS ABI.
pub const fn error_to_ntstatus(error: PciError) -> nt_abi::ntstatus::NTSTATUS {
    use nt_abi::ntstatus::*;
    match error {
        PciError::UnsupportedRange => STATUS_INVALID_DEVICE_REQUEST,
        PciError::InvalidWindow | PciError::InvalidBdf | PciError::InvalidOffset
        | PciError::AddressOverflow => STATUS_INVALID_PARAMETER,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EcamWindow {
    /// Virtual address corresponding to bus_start/device 0/function 0/offset 0.
    pub virtual_base: u64,
    pub physical_base: u64,
    pub segment: u16,
    pub bus_start: u8,
    pub bus_end: u8,
}

impl EcamWindow {
    pub fn validate(&self) -> Result<(), PciError> {
        if self.virtual_base == 0 || self.physical_base == 0 || self.bus_end < self.bus_start
            || self.virtual_base & 0xfffff != 0 || self.physical_base & 0xfffff != 0 {
            return Err(PciError::InvalidWindow);
        }
        let span = ((self.bus_end as u64 - self.bus_start as u64) + 1)
            .checked_mul(1 << 20).ok_or(PciError::AddressOverflow)?;
        self.virtual_base.checked_add(span).ok_or(PciError::AddressOverflow)?;
        self.physical_base.checked_add(span).ok_or(PciError::AddressOverflow)?;
        Ok(())
    }

    /// Calculate the mapped virtual address for one ECAM register. ECAM uses
    /// 1 MiB per bus, 32 devices, 8 functions and 4096 bytes per function.
    pub fn register_address(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16) -> Result<*mut u8, PciError> {
        self.validate()?;
        if segment != self.segment || bus < self.bus_start || bus > self.bus_end || device >= 32 || function >= 8 {
            return Err(PciError::InvalidBdf);
        }
        if offset >= 4096 { return Err(PciError::InvalidOffset); }
        let bus_offset = (bus as u64 - self.bus_start as u64) << 20;
        let bdf_offset = ((device as u64) << 15) | ((function as u64) << 12) | offset as u64;
        let address = self.virtual_base.checked_add(bus_offset)
            .and_then(|v| v.checked_add(bdf_offset)).ok_or(PciError::AddressOverflow)?;
        Ok(address as *mut u8)
    }

    pub unsafe fn read_u8(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16) -> Result<u8, PciError> {
        let p = self.register_address(segment, bus, device, function, offset)?;
        Ok(unsafe { core::ptr::read_volatile(p) })
    }
    pub unsafe fn read_u16(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16) -> Result<u16, PciError> {
        if offset & 1 != 0 { return Err(PciError::InvalidOffset); }
        let p = self.register_address(segment, bus, device, function, offset)?;
        Ok(unsafe { core::ptr::read_volatile(p.cast::<u16>()) })
    }
    pub unsafe fn read_u32(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16) -> Result<u32, PciError> {
        if offset & 3 != 0 { return Err(PciError::InvalidOffset); }
        let p = self.register_address(segment, bus, device, function, offset)?;
        Ok(unsafe { core::ptr::read_volatile(p.cast::<u32>()) })
    }

    /// Write one naturally aligned dword. Callers must apply PCI register
    /// semantics (W1C status bits, BAR sizing, command bits) before writing.
    pub unsafe fn write_u32(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16, value: u32) -> Result<(), PciError> {
        if offset & 3 != 0 { return Err(PciError::InvalidOffset); }
        let p = self.register_address(segment, bus, device, function, offset)?;
        unsafe { core::ptr::write_volatile(p.cast::<u32>(), value); }
        core::sync::atomic::fence(Ordering::SeqCst);
        Ok(())
    }
}

/// A checked translation window used by bus/resource code. It only translates
/// addresses inside an explicitly supplied firmware/bridge window; identity
/// translation is never assumed for arbitrary PCI resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressWindow { pub bus_start: u64, pub physical_start: u64, pub length: u64 }
impl AddressWindow {
    pub fn translate(&self, bus_address: u64, length: u64) -> Result<u64, PciError> {
        if length == 0 || self.length == 0 { return Err(PciError::UnsupportedRange); }
        let end = bus_address.checked_add(length).ok_or(PciError::AddressOverflow)?;
        let window_end = self.bus_start.checked_add(self.length).ok_or(PciError::AddressOverflow)?;
        if bus_address < self.bus_start || end > window_end { return Err(PciError::UnsupportedRange); }
        self.physical_start.checked_add(bus_address - self.bus_start).ok_or(PciError::AddressOverflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ecam_bdf_offsets_are_correct() {
        let w = EcamWindow { virtual_base: 0x1000_0000, physical_base: 0x8000_0000, segment: 0, bus_start: 0, bus_end: 255 };
        assert_eq!(w.register_address(0, 0, 0, 0, 0x100).unwrap() as u64, 0x1000_0100);
        assert_eq!(w.register_address(0, 2, 3, 1, 0x20).unwrap() as u64, 0x1021_9020);
        assert!(w.register_address(0, 0, 32, 0, 0).is_err());
        assert!(w.register_address(0, 0, 0, 8, 0).is_err());
        assert!(w.register_address(0, 0, 0, 0, 4096).is_err());
    }
    #[test]
    fn resource_translation_checks_entire_range() {
        let w = AddressWindow { bus_start: 0x1000, physical_start: 0x8000_0000, length: 0x1000 };
        assert_eq!(w.translate(0x1100, 0x100), Ok(0x8000_0100));
        assert!(w.translate(0x1fff, 2).is_err());
        assert!(w.translate(0x0fff, 1).is_err());
    }
}

#[cfg(test)]
mod ntstatus_adapter_tests {
    use super::*;
    #[test]
    fn pci_errors_map_to_shared_ntstatus() {
        use nt_abi::ntstatus::*;
        assert_eq!(error_to_ntstatus(PciError::UnsupportedRange), STATUS_INVALID_DEVICE_REQUEST);
        assert_eq!(error_to_ntstatus(PciError::InvalidBdf), STATUS_INVALID_PARAMETER);
        assert_eq!(error_to_ntstatus(PciError::AddressOverflow), STATUS_INVALID_PARAMETER);
    }
}


/// PCI Command-register policy for safe BAR sizing. Probe with I/O decode,
/// memory decode and bus mastering all disabled; restore decode bits afterward
/// but never restore bus mastering before an IOMMU domain is active.
pub const fn command_for_bar_probe(command: u16) -> u16 { command & !0x0007 }
pub const fn command_after_bar_probe(command: u16) -> u16 { command & !0x0004 }

#[cfg(test)]
mod pci_command_policy_tests {
    use super::*;
    #[test]
    fn bar_probe_disables_decode_and_bus_mastering() {
        assert_eq!(command_for_bar_probe(0x0007), 0);
        assert_eq!(command_for_bar_probe(0x0147), 0x0140);
    }
    #[test]
    fn post_probe_policy_never_reenables_bus_mastering() {
        assert_eq!(command_after_bar_probe(0x0007), 0x0003);
        assert_eq!(command_after_bar_probe(0x0147), 0x0143);
    }
}

/// PCI class-code matcher for xHCI (Base Class 0x0C, Subclass 0x03, ProgIF 0x30).
pub const fn is_xhci_controller(class_code:u8,subclass:u8,programming_interface:u8)->bool{
 class_code==0x0c&&subclass==0x03&&programming_interface==0x30
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum BarError{IoBar,ReservedMemoryType,MissingUpperDword,Unassigned,AddressOverflow}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct MemoryBar{pub base:u64,pub is_64_bit:bool,pub prefetchable:bool}
/// Decode already-read BAR dwords without probing by writing all ones (which would
/// mutate live hardware). Resource sizing and mapping are separate privileged steps.
pub fn decode_memory_bar(low:u32,high:Option<u32>)->Result<MemoryBar,BarError>{
 if low&1!=0{return Err(BarError::IoBar)}
 let kind=(low>>1)&3;
 if kind==1||kind==3{return Err(BarError::ReservedMemoryType)}
 let is_64_bit=kind==2;
 if is_64_bit&&high.is_none(){return Err(BarError::MissingUpperDword)}
 let base=((high.unwrap_or(0) as u64)<<32)|(low as u64&!0xf);
 if base==0{return Err(BarError::Unassigned)}
 Ok(MemoryBar{base,is_64_bit,prefetchable:low&(1<<3)!=0})
}

#[cfg(test)]
mod xhci_pci_tests{
 use super::*;
 #[test]fn recognizes_only_xhci_class_code(){assert!(is_xhci_controller(0x0c,0x03,0x30));assert!(!is_xhci_controller(0x0c,0x03,0x20));assert!(!is_xhci_controller(0x01,0x06,0x01));}
 #[test]fn decodes_64_bit_mmio_bar(){let b=decode_memory_bar(0x0000_000c,Some(0x0000_0001)).unwrap();assert_eq!(b.base,0x1_0000_0000);assert!(b.is_64_bit);assert!(b.prefetchable);}
 #[test]fn rejects_io_reserved_and_unassigned_bars(){assert_eq!(decode_memory_bar(1,None),Err(BarError::IoBar));assert_eq!(decode_memory_bar(0x2,None),Err(BarError::ReservedMemoryType));assert_eq!(decode_memory_bar(0,None),Err(BarError::Unassigned));assert_eq!(decode_memory_bar(0x4,None),Err(BarError::MissingUpperDword));}
}


// Read-only PCI function discovery. Enumeration never probes BAR sizes by writing
// all-ones values: BAR assignment and mapping belong to the kernel PnP/resource layer.
pub const PCI_VENDOR_ID_OFFSET:u16=0x00;
pub const PCI_DEVICE_ID_OFFSET:u16=0x02;
pub const PCI_COMMAND_OFFSET:u16=0x04;
pub const PCI_CLASS_REVISION_OFFSET:u16=0x08;
pub const PCI_HEADER_TYPE_OFFSET:u16=0x0e;
pub const PCI_BAR0_OFFSET:u16=0x10;
pub const PCI_COMMAND_MEMORY:u16=1<<1;
pub const PCI_COMMAND_BUS_MASTER:u16=1<<2;

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct PciFunctionHeader {
    pub vendor_id:u16,
    pub device_id:u16,
    pub command:u16,
    pub revision_id:u8,
    pub programming_interface:u8,
    pub subclass:u8,
    pub class_code:u8,
    pub header_type:u8,
}
impl PciFunctionHeader {
    /// Parse the common 16-byte PCI configuration header without interpreting
    /// untrusted/reserved fields as resource addresses.
    pub fn parse(bytes:&[u8])->Result<Self,PciError>{
        if bytes.len()<16{return Err(PciError::InvalidOffset)}
        let vendor_id=u16::from_le_bytes([bytes[0],bytes[1]]);
        let device_id=u16::from_le_bytes([bytes[2],bytes[3]]);
        if vendor_id==0xffff || vendor_id==0{return Err(PciError::InvalidBdf)}
        Ok(Self{
            vendor_id,device_id,
            command:u16::from_le_bytes([bytes[4],bytes[5]]),
            revision_id:bytes[8],
            programming_interface:bytes[9],
            subclass:bytes[10],
            class_code:bytes[11],
            header_type:bytes[14]&0x7f,
        })
    }
    pub const fn is_xhci(&self)->bool{
        is_xhci_controller(self.class_code,self.subclass,self.programming_interface)
    }
    pub const fn is_ahci(&self)->bool {
        self.class_code == 0x01 && self.subclass == 0x06 && self.programming_interface == 0x01
    }
    pub const fn is_nvme(&self)->bool {
        self.class_code == 0x01 && self.subclass == 0x08 && self.programming_interface == 0x02
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PciFunction {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub header: PciFunctionHeader,
}
pub const MAX_PCI_FUNCTIONS: usize = 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PciFunctionScanError { Window(PciError), Capacity }
impl From<PciError> for PciFunctionScanError {
    fn from(value: PciError) -> Self { Self::Window(value) }
}

/// Enumerate present PCI functions using only read-only ECAM configuration
/// accesses. The caller supplies the bounded output array; no BAR sizing,
/// command-register writes, bus mastering, or device activation occurs here.
pub unsafe fn scan_functions(
    window: &EcamWindow,
    output: &mut [Option<PciFunction>],
) -> Result<usize, PciFunctionScanError> {
    window.validate().map_err(PciFunctionScanError::Window)?;
    output.fill(None);
    let mut count = 0usize;
    for bus in window.bus_start..=window.bus_end {
        for device in 0u8..32 {
            let mut multifunction = false;
            for function in 0u8..8 {
                if function > 0 && !multifunction { break; }
                let vendor = unsafe { window.read_u16(window.segment, bus, device, function, PCI_VENDOR_ID_OFFSET) }
                    .map_err(PciFunctionScanError::Window)?;
                if vendor == 0xffff || vendor == 0 {
                    if function == 0 { break; }
                    continue;
                }
                let header_type = unsafe { window.read_u8(window.segment, bus, device, function, PCI_HEADER_TYPE_OFFSET) }
                    .map_err(PciFunctionScanError::Window)?;
                if function == 0 { multifunction = header_type & 0x80 != 0; }
                let mut bytes = [0u8; 16];
                for offset in [0u16, 4, 8, 12] {
                    let value = unsafe { window.read_u32(window.segment, bus, device, function, offset) }
                        .map_err(PciFunctionScanError::Window)?;
                    let at = offset as usize;
                    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
                }
                let header = PciFunctionHeader::parse(&bytes).map_err(PciFunctionScanError::Window)?;
                if count == output.len() { return Err(PciFunctionScanError::Capacity); }
                output[count] = Some(PciFunction { segment: window.segment, bus, device, function, header });
                count += 1;
            }
        }
    }
    Ok(count)
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct XhciPciFunction {
    pub segment:u16,pub bus:u8,pub device:u8,pub function:u8,
    pub vendor_id:u16,pub device_id:u16,pub command:u16,pub bar:MemoryBar,
}
pub const MAX_XHCI_FUNCTIONS:usize=32;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PciScanError { Window(PciError), Bar(BarError), ResultCapacity }
impl From<PciError> for PciScanError {fn from(e:PciError)->Self{Self::Window(e)}}
impl From<BarError> for PciScanError {fn from(e:BarError)->Self{Self::Bar(e)}}

/// Discover xHCI functions in one firmware-described ECAM window. This only
/// reads configuration space and reports the assigned BAR; it intentionally
/// does not enable bus mastering, size BARs, map MMIO, or start the controller.
/// Those privileged operations require kernel resource arbitration and HAL DMA
/// / interrupt services. Multi-function devices are walked according to the
/// header-type bit; absent functions are skipped.
pub unsafe fn scan_xhci(
    window:&EcamWindow,
)->Result<([Option<XhciPciFunction>;MAX_XHCI_FUNCTIONS],usize),PciScanError>{
    window.validate().map_err(PciScanError::Window)?;
    let mut found=[None;MAX_XHCI_FUNCTIONS];
    let mut count=0usize;
    for bus in window.bus_start..=window.bus_end {
        for device in 0u8..32 {
            let mut multifunction=false;
            for function in 0u8..8 {
                if function>0 && !multifunction {break}
                let vendor=unsafe{window.read_u16(window.segment,bus,device,function,PCI_VENDOR_ID_OFFSET)}
                    .map_err(PciScanError::Window)?;
                if vendor==0xffff || vendor==0 {if function==0{break}else{continue}}
                let header_type=unsafe{window.read_u8(window.segment,bus,device,function,PCI_HEADER_TYPE_OFFSET)}
                    .map_err(PciScanError::Window)?;
                if function==0 {multifunction=header_type&0x80!=0}
                let class=unsafe{window.read_u32(window.segment,bus,device,function,PCI_CLASS_REVISION_OFFSET)}
                    .map_err(PciScanError::Window)?;
                let revision=class as u8;
                let prog_if=(class>>8) as u8;
                let subclass=(class>>16) as u8;
                let class_code=(class>>24) as u8;
                if !is_xhci_controller(class_code,subclass,prog_if){continue}
                // Type-0 endpoint headers expose BAR0 at 0x10. Do not treat
                // bridge/CardBus layouts as controller endpoint resources.
                if header_type&0x7f!=0 {continue}
                let low=unsafe{window.read_u32(window.segment,bus,device,function,PCI_BAR0_OFFSET)}
                    .map_err(PciScanError::Window)?;
                let kind=(low>>1)&3;
                let high=if kind==2 {Some(unsafe{window.read_u32(window.segment,bus,device,function,PCI_BAR0_OFFSET+4)}.map_err(PciScanError::Window)?)}else{None};
                let bar=decode_memory_bar(low,high).map_err(PciScanError::Bar)?;
                let device_id=unsafe{window.read_u16(window.segment,bus,device,function,PCI_DEVICE_ID_OFFSET)}
                    .map_err(PciScanError::Window)?;
                let command=unsafe{window.read_u16(window.segment,bus,device,function,PCI_COMMAND_OFFSET)}
                    .map_err(PciScanError::Window)?;
                if count==MAX_XHCI_FUNCTIONS{return Err(PciScanError::ResultCapacity)}
                found[count]=Some(XhciPciFunction{segment:window.segment,bus,device,function,vendor_id:vendor,device_id,command,bar});
                count+=1;
                let _=revision; // retained in the common header parser for callers that need it
            }
        }
    }
    Ok((found,count))
}

#[cfg(test)]
mod function_header_tests {
    use super::*;
    #[test]
    fn parses_function_header_and_xhci_class(){
        let mut b=[0u8;16];b[0..2].copy_from_slice(&0x8086u16.to_le_bytes());
        b[2..4].copy_from_slice(&0x1234u16.to_le_bytes());b[4..6].copy_from_slice(&0x0006u16.to_le_bytes());
        b[8]=1;b[9]=0x30;b[10]=0x03;b[11]=0x0c;b[14]=0;
        let h=PciFunctionHeader::parse(&b).unwrap();
        assert_eq!(h.vendor_id,0x8086);assert_eq!(h.device_id,0x1234);
        assert!(h.is_xhci());assert_eq!(h.command,6);
    }
    #[test]
    fn common_header_rejects_absent_and_truncated_functions(){
        assert_eq!(PciFunctionHeader::parse(&[0;8]),Err(PciError::InvalidOffset));
        let mut b=[0u8;16];b[0..2].copy_from_slice(&0xffffu16.to_le_bytes());
        assert_eq!(PciFunctionHeader::parse(&b),Err(PciError::InvalidBdf));
    }
}


/// One PCI-to-PCI bridge's bus-number window, captured from the Type-1 header.
/// Segment is part of the identity: bus numbers are not globally unique.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct PciBridgeRange {
    pub segment:u16,pub primary_bus:u8,pub secondary_bus:u8,pub subordinate_bus:u8,
    pub device:u8,pub function:u8,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct PciPathElement { pub device:u8,pub function:u8 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PciRequesterPath {
    pub segment:u16,pub root_bus:u8,pub endpoint_bus:u8,pub endpoint_device:u8,
    pub endpoint_function:u8,pub path:[PciPathElement;8],pub path_count:u8,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PciTopologyError { InvalidBdf, InvalidBridgeRange, MissingParentBridge, AmbiguousParentBridge, PathTooDeep, Cycle }

/// Resolve an endpoint's complete bridge path using validated Type-1 primary,
/// secondary and subordinate bus numbers. The most-specific containing bridge
/// is selected at each hop; segment mismatches and ambiguous ranges fail closed.
/// The root_bus argument is the root bus of the ECAM/host bridge hierarchy.
pub fn resolve_requester_path(segment:u16,root_bus:u8,endpoint_bus:u8,
    endpoint_device:u8,endpoint_function:u8,bridges:&[PciBridgeRange])
    ->Result<PciRequesterPath,PciTopologyError> {
    if endpoint_device>=32||endpoint_function>=8{return Err(PciTopologyError::InvalidBdf)}
    let mut reverse=[PciPathElement::default();8];
    let mut count=0usize;
    let mut bus=endpoint_bus;
    let mut visited=[false;256];
    while bus!=root_bus {
        if visited[bus as usize]{return Err(PciTopologyError::Cycle)}
        visited[bus as usize]=true;
        let mut best:Option<PciBridgeRange>=None;
        for b in bridges.iter().filter(|b|b.segment==segment) {
            if b.device>=32||b.function>=8||b.secondary_bus<=b.primary_bus
                ||b.subordinate_bus<b.secondary_bus {
                return Err(PciTopologyError::InvalidBridgeRange)
            }
            if b.secondary_bus<=bus&&bus<=b.subordinate_bus {
                match best {
                    None=>best=Some(*b),
                    Some(old)=>{
                        let old_width=old.subordinate_bus as u16-old.secondary_bus as u16;
                        let new_width=b.subordinate_bus as u16-b.secondary_bus as u16;
                        if new_width<old_width {best=Some(*b)}
                        else if new_width==old_width{return Err(PciTopologyError::AmbiguousParentBridge)}
                    }
                }
            }
        }
        let bridge=best.ok_or(PciTopologyError::MissingParentBridge)?;
        if count==reverse.len(){return Err(PciTopologyError::PathTooDeep)}
        reverse[count]=PciPathElement{device:bridge.device,function:bridge.function};
        count+=1;
        bus=bridge.primary_bus;
    }
    let mut path=[PciPathElement::default();8];
    for i in 0..count {path[i]=reverse[count-1-i];}
    if count==path.len(){return Err(PciTopologyError::PathTooDeep)}
    path[count]=PciPathElement{device:endpoint_device,function:endpoint_function};
    Ok(PciRequesterPath{segment,root_bus,endpoint_bus,endpoint_device,endpoint_function,
        path,path_count:(count+1) as u8})
}

#[cfg(test)]
mod requester_topology_tests {
    use super::*;
    #[test]
    fn resolves_nested_bridge_path_and_keeps_segment() {
        let bridges=[
            PciBridgeRange{segment:4,primary_bus:0,secondary_bus:2,subordinate_bus:20,device:1,function:0},
            PciBridgeRange{segment:4,primary_bus:2,secondary_bus:7,subordinate_bus:9,device:3,function:1},
        ];
        let p=resolve_requester_path(4,0,8,5,0,&bridges).unwrap();
        assert_eq!(p.segment,4);assert_eq!(p.path_count,3);
        assert_eq!(p.path[0],PciPathElement{device:1,function:0});
        assert_eq!(p.path[1],PciPathElement{device:3,function:1});
        assert_eq!(p.path[2],PciPathElement{device:5,function:0});
        assert_eq!(resolve_requester_path(0,0,8,5,0,&bridges),Err(PciTopologyError::MissingParentBridge));
    }
    #[test]
    fn rejects_missing_or_ambiguous_bridge_routes() {
        let none:[PciBridgeRange;0]=[];
        assert_eq!(resolve_requester_path(0,0,2,1,0,&none),Err(PciTopologyError::MissingParentBridge));
        let overlap=[
            PciBridgeRange{segment:0,primary_bus:0,secondary_bus:1,subordinate_bus:8,device:1,function:0},
            PciBridgeRange{segment:0,primary_bus:0,secondary_bus:1,subordinate_bus:8,device:2,function:0},
        ];
        assert_eq!(resolve_requester_path(0,0,4,1,0,&overlap),Err(PciTopologyError::AmbiguousParentBridge));
    }
}


/// Decode the Type-1 bridge bus-number dword at configuration offset 0x18.
pub fn decode_bridge_range(segment:u16,current_bus:u8,device:u8,function:u8,
    buses:u32)->Result<PciBridgeRange,PciTopologyError> {
    let primary=buses as u8;
    let secondary=(buses>>8) as u8;
    let subordinate=(buses>>16) as u8;
    if device>=32||function>=8||primary!=current_bus||secondary<=primary
        ||subordinate<secondary {
        return Err(PciTopologyError::InvalidBridgeRange);
    }
    Ok(PciBridgeRange{segment,primary_bus:primary,secondary_bus:secondary,
        subordinate_bus:subordinate,device,function})
}

pub const MAX_PCI_BRIDGES:usize=256;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PciBridgeScanError { Window(PciError), InvalidBridgeTopology, Capacity }
/// Discover Type-1 PCI bridges and their primary/secondary/subordinate bus
/// windows in one MCFG segment. This is read-only and never changes bus numbers.
pub unsafe fn scan_bridge_ranges(window:&EcamWindow)
    ->Result<([Option<PciBridgeRange>;MAX_PCI_BRIDGES],usize),PciBridgeScanError> {
    window.validate().map_err(PciBridgeScanError::Window)?;
    let mut found=[None;MAX_PCI_BRIDGES];
    let mut count=0usize;
    for bus in window.bus_start..=window.bus_end {
        for device in 0u8..32 {
            let mut multifunction=false;
            for function in 0u8..8 {
                if function>0&&!multifunction{break}
                let vendor=unsafe{window.read_u16(window.segment,bus,device,function,PCI_VENDOR_ID_OFFSET)}
                    .map_err(PciBridgeScanError::Window)?;
                if vendor==0xffff||vendor==0 {if function==0{break}else{continue}}
                let header=unsafe{window.read_u8(window.segment,bus,device,function,PCI_HEADER_TYPE_OFFSET)}
                    .map_err(PciBridgeScanError::Window)?;
                if function==0{multifunction=header&0x80!=0}
                if header&0x7f!=1{continue}
                let buses=unsafe{window.read_u32(window.segment,bus,device,function,0x18)}
                    .map_err(PciBridgeScanError::Window)?;
                let bridge=decode_bridge_range(window.segment,bus,device,function,buses)
                    .map_err(|_|PciBridgeScanError::InvalidBridgeTopology)?;
                if bridge.subordinate_bus>window.bus_end{return Err(PciBridgeScanError::InvalidBridgeTopology)}
                if count==found.len(){return Err(PciBridgeScanError::Capacity)}
                found[count]=Some(bridge);count+=1;
            }
        }
    }
    Ok((found,count))
}

#[cfg(test)]
mod bridge_range_decode_tests {
    use super::*;
    #[test]
    fn bridge_bus_numbers_must_match_the_enumerated_primary_bus() {
        let buses=0x0008_0702u32;
        assert_eq!(decode_bridge_range(2,2,3,0,buses),Ok(PciBridgeRange{
            segment:2,primary_bus:2,secondary_bus:7,subordinate_bus:8,device:3,function:0
        }));
        assert_eq!(decode_bridge_range(2,1,3,0,buses),Err(PciTopologyError::InvalidBridgeRange));
        assert_eq!(decode_bridge_range(2,2,3,0,0x0006_0702),Err(PciTopologyError::InvalidBridgeRange));
    }
}

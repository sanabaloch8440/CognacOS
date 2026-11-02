//! Hardware-independent PCI IDE controller framework (pciidex.sys model).
//!
//! This module deliberately owns policy and resource validation, not PCI
//! configuration-space writes or register access. The PCI bus driver remains
//! responsible for PDO identity and PnP resource assignment; pciide supplies
//! the hardware-specific controller operations; ataport owns channel I/O.
//! This is a CognacOS implementation of the documented layering, not Microsoft's
//! private binary interface.

use core::mem::size_of;

pub const PCI_CLASS_MASS_STORAGE: u8 = 0x01;
pub const PCI_SUBCLASS_IDE: u8 = 0x01;
pub const PROGIF_PRIMARY_NATIVE: u8 = 1 << 0;
pub const PROGIF_SECONDARY_NATIVE: u8 = 1 << 2;
pub const PROGIF_BUS_MASTER: u8 = 1 << 7;

pub const CM_RESOURCE_PORT: u8 = 1;
pub const CM_RESOURCE_INTERRUPT: u8 = 2;
pub const CM_RESOURCE_MEMORY: u8 = 3;
pub const MAX_IDE_RESOURCES: usize = 32;
pub const MAX_FULL_DESCRIPTORS: usize = 16;
pub const MAX_PARTIAL_DESCRIPTORS: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PciIdeIdentity {
    pub vendor_id: u16,
    pub device_id: u16,
    pub base_class: u8,
    pub sub_class: u8,
    pub programming_interface: u8,
    pub revision_id: u8,
}
impl PciIdeIdentity {
    pub const fn is_ide_controller(self) -> bool {
        self.vendor_id != 0 && self.vendor_id != 0xffff
            && self.device_id != 0xffff
            && self.base_class == PCI_CLASS_MASS_STORAGE
            && self.sub_class == PCI_SUBCLASS_IDE
    }
    pub const fn primary_native(self) -> bool { self.programming_interface & PROGIF_PRIMARY_NATIVE != 0 }
    pub const fn secondary_native(self) -> bool { self.programming_interface & PROGIF_SECONDARY_NATIVE != 0 }
    pub const fn bus_master_capable(self) -> bool { self.programming_interface & PROGIF_BUS_MASTER != 0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind { Port, Interrupt, Memory }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignedResource {
    pub kind: ResourceKind,
    /// Translated system physical address or I/O port address.
    pub start: u64,
    pub length: u64,
    pub flags: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdeResourceSet {
    pub entries: [Option<AssignedResource>; MAX_IDE_RESOURCES],
    pub count: usize,
}
impl IdeResourceSet {
    pub const fn empty() -> Self { Self { entries: [None; MAX_IDE_RESOURCES], count: 0 } }
    pub fn get(&self, index: usize) -> Option<AssignedResource> {
        if index >= self.count { None } else { self.entries[index] }
    }
    pub fn contains_kind(&self, kind: ResourceKind) -> bool {
        self.entries[..self.count].iter().flatten().any(|r| r.kind == kind)
    }
}

/// Parse the translated CM_RESOURCE_LIST supplied in IRP_MN_START_DEVICE.
/// Resource descriptors use the x64 WDK layout (16-byte partial descriptor).
/// The returned ranges are only descriptions: mapping/port claiming is done
/// by the PnP/I/O manager and the hardware minidriver consumes these assignments.
pub fn parse_translated_resources(bytes: &[u8]) -> Result<IdeResourceSet, i32> {
    const INVALID: i32 = 0xC000_000Du32 as i32;
    const RESOURCES: i32 = 0xC000_009Au32 as i32;
    let mut out = IdeResourceSet::empty();
    if bytes.is_empty() { return Ok(out); }
    if bytes.len() < 4 { return Err(INVALID); }
    let full_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    if full_count > MAX_FULL_DESCRIPTORS { return Err(INVALID); }
    let mut cursor = 4usize;
    for _ in 0..full_count {
        let header_end = cursor.checked_add(16).ok_or(INVALID)?;
        if header_end > bytes.len() { return Err(INVALID); }
        let partial_count = u32::from_le_bytes(bytes[cursor+12..cursor+16].try_into().unwrap()) as usize;
        if partial_count > MAX_PARTIAL_DESCRIPTORS { return Err(INVALID); }
        let descriptors_end = header_end.checked_add(partial_count.checked_mul(16).ok_or(INVALID)?).ok_or(INVALID)?;
        if descriptors_end > bytes.len() { return Err(INVALID); }
        for n in 0..partial_count {
            let d = header_end + n * 16;
            let kind = bytes[d];
            let flags = u16::from_le_bytes([bytes[d+2], bytes[d+3]]);
            let parsed = match kind {
                0 | 5 | 6 => None, // Null, device-specific, bus-number
                CM_RESOURCE_PORT | CM_RESOURCE_MEMORY => {
                    let start = u64::from_le_bytes(bytes[d+4..d+12].try_into().unwrap());
                    let length = u32::from_le_bytes(bytes[d+12..d+16].try_into().unwrap()) as u64;
                    if length == 0 || start.checked_add(length).is_none() { return Err(INVALID); }
                    Some(AssignedResource {
                        kind: if kind == CM_RESOURCE_PORT { ResourceKind::Port } else { ResourceKind::Memory },
                        start, length, flags,
                    })
                }
                CM_RESOURCE_INTERRUPT => {
                    // Vector is in the second union member at offset 8.
                    let vector = u32::from_le_bytes(bytes[d+8..d+12].try_into().unwrap()) as u64;
                    if vector == 0 { return Err(INVALID); }
                    Some(AssignedResource { kind: ResourceKind::Interrupt, start: vector, length: 1, flags })
                }
                _ => return Err(INVALID),
            };
            if let Some(resource) = parsed {
                if out.count == MAX_IDE_RESOURCES { return Err(RESOURCES); }
                if out.entries[..out.count].iter().flatten().any(|old| overlaps(*old, resource)) {
                    return Err(INVALID);
                }
                out.entries[out.count] = Some(resource);
                out.count += 1;
            }
        }
        cursor = descriptors_end;
    }
    if cursor != bytes.len() { return Err(INVALID); }
    Ok(out)
}
fn overlaps(a: AssignedResource, b: AssignedResource) -> bool {
    if a.kind != b.kind { return false; }
    if a.kind == ResourceKind::Interrupt { return a.start == b.start; }
    a.start < b.start.saturating_add(b.length) && b.start < a.start.saturating_add(a.length)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelMode { Compatibility, Native }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelAssignment {
    pub mode: ChannelMode,
    pub command: Option<AssignedResource>,
    pub control: Option<AssignedResource>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerState { Created, ResourcesAssigned, Started, Stopped, Removed }

/// Per-controller state owned by the controller framework. It never programs
/// PCI command bits; StartDevice resource activation is a separate minidriver
/// operation after the PnP manager has committed translated resources.
pub struct PciIdeController {
    pub identity: PciIdeIdentity,
    pub state: ControllerState,
    pub resources: IdeResourceSet,
}
impl PciIdeController {
    pub const fn new(identity: PciIdeIdentity) -> Self {
        Self { identity, state: ControllerState::Created, resources: IdeResourceSet::empty() }
    }
    pub fn assign_resources(&mut self, resources: IdeResourceSet) -> Result<(), i32> {
        const INVALID: i32 = 0xC000_000Du32 as i32;
        const STATE: i32 = 0xC000_0184u32 as i32;
        if !self.identity.is_ide_controller() { return Err(INVALID); }
        if !matches!(self.state, ControllerState::Created | ControllerState::Stopped) { return Err(STATE); }
        // A native-mode channel must be backed by assigned resources. Compatibility
        // mode can use legacy decode, but the minidriver still owns safe activation.
        if (self.identity.primary_native() || self.identity.secondary_native()) && resources.count == 0 {
            return Err(INVALID);
        }
        self.resources = resources;
        self.state = ControllerState::ResourcesAssigned;
        Ok(())
    }
    pub fn start_succeeded(&mut self) -> Result<(), i32> {
        const STATE: i32 = 0xC000_0184u32 as i32;
        if self.state != ControllerState::ResourcesAssigned { return Err(STATE); }
        self.state = ControllerState::Started; Ok(())
    }
    pub fn stop_succeeded(&mut self) -> Result<(), i32> {
        const STATE: i32 = 0xC000_0184u32 as i32;
        if self.state != ControllerState::Started { return Err(STATE); }
        self.state = ControllerState::Stopped; Ok(())
    }
    pub fn remove_succeeded(&mut self) -> Result<(), i32> {
        const STATE: i32 = 0xC000_0184u32 as i32;
        if self.state == ControllerState::Started || self.state == ControllerState::Removed { return Err(STATE); }
        self.resources = IdeResourceSet::empty();
        self.state = ControllerState::Removed; Ok(())
    }
    pub fn channel_mode(&self, secondary: bool) -> ChannelMode {
        let native = if secondary { self.identity.secondary_native() } else { self.identity.primary_native() };
        if native { ChannelMode::Native } else { ChannelMode::Compatibility }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity(progif: u8) -> PciIdeIdentity {
        PciIdeIdentity { vendor_id: 0x8086, device_id: 0x7010, base_class: 1, sub_class: 1, programming_interface: progif, revision_id: 0 }
    }
    fn resource_list(kind: u8, start: u64, length: u32) -> std::vec::Vec<u8> {
        let mut b = std::vec![0u8; 4 + 16 + 16];
        b[0..4].copy_from_slice(&1u32.to_le_bytes());
        b[16..20].copy_from_slice(&1u32.to_le_bytes());
        b[20] = kind;
        b[24..32].copy_from_slice(&start.to_le_bytes());
        b[32..36].copy_from_slice(&length.to_le_bytes());
        b
    }
    #[test]
    fn matches_only_pci_ide_class() {
        assert!(identity(0x05).is_ide_controller());
        let mut ahci = identity(0);
        ahci.sub_class = 6;
        assert!(!ahci.is_ide_controller());
    }
    #[test]
    fn translated_resource_list_is_bounds_checked() {
        let parsed = parse_translated_resources(&resource_list(CM_RESOURCE_PORT, 0x1f0, 8)).unwrap();
        assert_eq!(parsed.count, 1);
        assert_eq!(parsed.get(0).unwrap().start, 0x1f0);
        assert!(parse_translated_resources(&[1, 0, 0]).is_err());
    }
    #[test]
    fn controller_lifecycle_is_ordered() {
        let mut controller = PciIdeController::new(identity(0));
        assert!(controller.start_succeeded().is_err());
        controller.assign_resources(IdeResourceSet::empty()).unwrap();
        controller.start_succeeded().unwrap();
        controller.stop_succeeded().unwrap();
        controller.remove_succeeded().unwrap();
        assert_eq!(controller.state, ControllerState::Removed);
    }
}

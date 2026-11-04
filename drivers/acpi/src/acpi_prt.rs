//! ACPI-3 PCI interrupt routing table validation.
//!
//! Consumes four-element entries after AML evaluates _PRT to a Package.
//! AML evaluation and link-device resource selection remain separate; this
//! module never guesses a GSI or programs an interrupt controller.
/*
 * FIXME(ACPI-PRODUCTION) — PCI interrupt routing via _PRT
 * Intended functionality still to complete: Complete _PRT package evaluation, source/link-device resolution, index validation, direct GSI routing, polarity/trigger conversion, and kernel interrupt integration. Test missing links, malformed packages, shared interrupts, and bridge hierarchies.
 */

#![no_std]
pub const MAX_PRT_ENTRIES: usize = 128;
pub const MAX_SOURCE_PATH: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrtError { TooManyEntries, InvalidPin, InvalidAddress, InvalidSource, InvalidSourceIndex, DuplicateRoute, MissingRoute, LinkResolutionFailed }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptSource {
    DirectGsi { gsi: u32 },
    LinkDevice { path: [u8; MAX_SOURCE_PATH], length: u8, source_index: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrtEntry { pub address: u32, pub pin: u8, pub source: InterruptSource }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrtTable { pub entries: [Option<PrtEntry>; MAX_PRT_ENTRIES], pub count: usize }
impl PrtTable {
    pub const EMPTY: Self = Self { entries: [None; MAX_PRT_ENTRIES], count: 0 };
    fn push(&mut self, entry: PrtEntry) -> Result<(), PrtError> {
        if self.count == self.entries.len() { return Err(PrtError::TooManyEntries); }
        if self.entries[..self.count].iter().flatten().any(|old| old.address == entry.address && old.pin == entry.pin) { return Err(PrtError::DuplicateRoute); }
        self.entries[self.count] = Some(entry); self.count += 1; Ok(())
    }
/// Find the route for a device address and PCI interrupt pin in the validated _PRT table.
    pub fn find(&self, address: u32, pin: u8) -> Option<PrtEntry> {
        self.entries[..self.count].iter().flatten().find(|e| e.pin == pin &&
            (e.address == address || (e.address >> 16 == address >> 16 && e.address & 0xffff == 0xffff))).copied()
    }
}
/// Typed element produced by an AML evaluator for a _PRT package entry.
/// None source_path means AML integer zero (direct GSI); Some is a canonical
/// link-device namespace path. SourceIndex is retained for link resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluatedPrtElement<'a> { pub address: u64, pub pin: u64, pub source_path: Option<&'a [u8]>, pub source_index: u64 }
/// Validate evaluated _PRT package elements and build a bounded, duplicate-free route table.
pub fn build_prt_table(elements: &[EvaluatedPrtElement<'_>]) -> Result<PrtTable, PrtError> {
    if elements.len() > MAX_PRT_ENTRIES { return Err(PrtError::TooManyEntries); }
    let mut out = PrtTable::EMPTY;
    for e in elements {
        if e.pin > 3 { return Err(PrtError::InvalidPin); }
        if e.address > u32::MAX as u64 { return Err(PrtError::InvalidAddress); }
        if e.source_index > u32::MAX as u64 { return Err(PrtError::InvalidSourceIndex); }
        let source = match e.source_path {
            None => InterruptSource::DirectGsi { gsi: e.source_index as u32 },
            Some(path) => {
                if path.is_empty() || path.len() > MAX_SOURCE_PATH || path.iter().any(|b| *b == 0) { return Err(PrtError::InvalidSource); }
                let mut stored = [0u8; MAX_SOURCE_PATH]; stored[..path.len()].copy_from_slice(path);
                InterruptSource::LinkDevice { path: stored, length: path.len() as u8, source_index: e.source_index as u32 }
            }
        };
        out.push(PrtEntry { address: e.address as u32, pin: e.pin as u8, source })?;
    }
    Ok(out)
}
/// The platform resolver must consult selected link-device resources and the
/// resource arbiter before returning a concrete GSI.
pub trait LinkDeviceResolver { fn resolve_gsi(&self, canonical_path: &[u8], source_index: u32) -> Result<u32, PrtError>; }
/// Resolve a validated _PRT entry through its link-device source and return the selected GSI.
pub fn resolve_route<R: LinkDeviceResolver>(table: &PrtTable, address: u32, pin: u8, resolver: &R) -> Result<u32, PrtError> {
    let route = table.find(address, pin).ok_or(PrtError::MissingRoute)?;
    match route.source {
        InterruptSource::DirectGsi { gsi } => Ok(gsi),
        InterruptSource::LinkDevice { path, length, source_index } =>
            resolver.resolve_gsi(&path[..length as usize], source_index).map_err(|_| PrtError::LinkResolutionFailed),
    }
}
#[cfg(test)] extern crate std;
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn builds_direct_and_link_routes() {
        let entries = [
            EvaluatedPrtElement { address: 0x0002_ffff, pin: 0, source_path: None, source_index: 17 },
            EvaluatedPrtElement { address: 0x0003_0000, pin: 1, source_path: Some(b"\\_SB.PCI0.LNKA"), source_index: 0 },
        ];
        let table = build_prt_table(&entries).unwrap(); assert_eq!(table.count, 2);
        assert!(matches!(table.find(0x0002_0001, 0).unwrap().source, InterruptSource::DirectGsi { gsi: 17 }));
    }
    #[test] fn rejects_bad_pin_and_duplicate_routes() {
        let bad = [EvaluatedPrtElement { address: 0, pin: 4, source_path: None, source_index: 1 }];
        assert_eq!(build_prt_table(&bad), Err(PrtError::InvalidPin));
        let duplicate = [
            EvaluatedPrtElement { address: 0x0001_ffff, pin: 0, source_path: None, source_index: 1 },
            EvaluatedPrtElement { address: 0x0001_ffff, pin: 0, source_path: None, source_index: 2 },
        ];
        assert_eq!(build_prt_table(&duplicate), Err(PrtError::DuplicateRoute));
    }
    struct Resolver;
    impl LinkDeviceResolver for Resolver {
        fn resolve_gsi(&self, path: &[u8], index: u32) -> Result<u32, PrtError> {
            if path == b"\\_SB.PCI0.LNKA" && index == 0 { Ok(23) } else { Err(PrtError::LinkResolutionFailed) }
        }
    }
    #[test] fn resolves_link_device_through_platform_contract() {
        let entries = [EvaluatedPrtElement { address: 0x0003_0000, pin: 1, source_path: Some(b"\\_SB.PCI0.LNKA"), source_index: 0 }];
        let table = build_prt_table(&entries).unwrap();
        assert_eq!(resolve_route(&table, 0x0003_0000, 1, &Resolver), Ok(23));
    }
}


#[cfg(test)] mod acpi_v6_prt_function_wildcard {
 use super::*;
 
 #[test] fn function_wildcard_matches_only_its_device_and_pin() {
  let entries=[EvaluatedPrtElement{address:0x0004_ffff,pin:2,source_path:None,source_index:19}];
  let table=build_prt_table(&entries).unwrap();
  assert!(matches!(table.find(0x0004_0007,2).unwrap().source,InterruptSource::DirectGsi{gsi:19}));
  assert!(table.find(0x0005_0007,2).is_none());
  assert!(table.find(0x0004_0007,1).is_none());
 }

}


/// A PCI-function-specific route result. Quirk polarity/trigger values are
/// present only when an exact DMI policy override matched; otherwise the
/// caller must retain the electrical attributes obtained from the firmware
/// link-device resource or platform interrupt description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PolicyResolvedRoute {
    pub gsi: u32,
    pub active_low: Option<bool>,
    pub level_triggered: Option<bool>,
    pub quirk_applied: bool,
}

/// Resolve a route for one concrete PCI function. The policy lookup is done
/// against segment + bus + device + function + pin before consulting the
/// firmware _PRT entry. This lets a narrowly-scoped platform override replace
/// either a direct-GSI or link-device route without affecting sibling
/// functions, other segments, or other interrupt pins.
pub fn resolve_route_for_pci_function<R: LinkDeviceResolver>(
    table: &PrtTable,
    segment: u16,
    bus: u8,
    device: u8,
    function: u8,
    pin: u8,
    policy: Option<&crate::quirks::QuirkPolicy>,
    resolver: &R,
) -> Result<PolicyResolvedRoute, PrtError> {
    if device >= 32 || function >= 8 || pin > 3 {
        return Err(PrtError::InvalidPin);
    }
    if let Some(route) = policy.and_then(|p| {
        p.override_prt_gsi(segment, bus, device, function, pin)
    }) {
        return Ok(PolicyResolvedRoute {
            gsi: route.gsi,
            active_low: Some(route.active_low),
            level_triggered: Some(route.level_triggered),
            quirk_applied: true,
        });
    }
    let address = ((device as u32) << 16) | function as u32;
    let gsi = resolve_route(table, address, pin, resolver)?;
    Ok(PolicyResolvedRoute {
        gsi,
        active_low: None,
        level_triggered: None,
        quirk_applied: false,
    })
}

#[cfg(test)]
mod platform_prt_policy_tests {
    use super::*;
    use crate::quirks::{PrtOverride, QuirkPolicy};

    struct NoLink;
    impl LinkDeviceResolver for NoLink {
        fn resolve_gsi(&self, _: &[u8], _: u32) -> Result<u32, PrtError> {
            Err(PrtError::LinkResolutionFailed)
        }
    }

    #[test]
    fn exact_platform_route_override_precedes_firmware_link_resolution() {
        let entries = [EvaluatedPrtElement {
            address: 0x0005_ffff,
            pin: 1,
            source_path: Some(b"\\_SB.PCI0.LNKA"),
            source_index: 0,
        }];
        let table = build_prt_table(&entries).unwrap();
        let mut policy = QuirkPolicy::EMPTY;
        policy.prt[0] = Some(PrtOverride {
            segment: 0, bus: 2, device: 5, function: 0, pin: 1,
            gsi: 19, active_low: true, level_triggered: true,
        });
        policy.prt_count = 1;
        let route = resolve_route_for_pci_function(
            &table, 0, 2, 5, 0, 1, Some(&policy), &NoLink,
        ).unwrap();
        assert_eq!(route.gsi, 19);
        assert_eq!(route.active_low, Some(true));
        assert_eq!(route.level_triggered, Some(true));
        assert!(route.quirk_applied);
    }

    #[test]
    fn platform_route_override_does_not_match_sibling_functions_or_segments() {
        let entries = [EvaluatedPrtElement {
            address: 0x0005_ffff, pin: 1, source_path: None, source_index: 11,
        }];
        let table = build_prt_table(&entries).unwrap();
        let mut policy = QuirkPolicy::EMPTY;
        policy.prt[0] = Some(PrtOverride {
            segment: 0, bus: 2, device: 5, function: 0, pin: 1,
            gsi: 19, active_low: true, level_triggered: true,
        });
        policy.prt_count = 1;
        let sibling = resolve_route_for_pci_function(
            &table, 0, 2, 5, 1, 1, Some(&policy), &NoLink,
        ).unwrap();
        assert_eq!(sibling.gsi, 11);
        assert!(!sibling.quirk_applied);
        let other_segment = resolve_route_for_pci_function(
            &table, 1, 2, 5, 0, 1, Some(&policy), &NoLink,
        ).unwrap();
        assert_eq!(other_segment.gsi, 11);
        assert!(!other_segment.quirk_applied);
    }
}


/// Errors from decoding the firmware _PRT Name(Package(...)) object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrtDecodeError {
    MissingObject, WrongObjectType, BadTableIndex, Truncated, BadPackage,
    BadInteger, BadSource, UnresolvedLink, InvalidPath, TooManyEntries,
}

/// Construct the canonical rooted namespace path for a resolved link device.
fn canonical_namespace_path(
    namespace: &crate::aml::AmlNamespace,
    node: u16,
    output: &mut [u8; MAX_SOURCE_PATH],
) -> Result<usize, PrtDecodeError> {
    if node as usize >= namespace.count { return Err(PrtDecodeError::InvalidPath); }
    let mut reverse = [0u16; 32];
    let mut count = 0usize;
    let mut current = node;
    while current != 0 {
        if count == reverse.len() { return Err(PrtDecodeError::InvalidPath); }
        reverse[count] = current;
        count += 1;
        current = namespace.nodes[current as usize].parent;
        if current as usize >= namespace.count { return Err(PrtDecodeError::InvalidPath); }
    }
    let mut used = 0usize;
    output[used] = b'\\';
    used += 1;
    for index in (0..count).rev() {
        if index != count - 1 {
            if used == output.len() { return Err(PrtDecodeError::InvalidPath); }
            output[used] = b'.';
            used += 1;
        }
        if used + 4 > output.len() { return Err(PrtDecodeError::InvalidPath); }
        output[used..used + 4].copy_from_slice(&namespace.nodes[reverse[index] as usize].name);
        used += 4;
    }
    Ok(used)
}

fn read_prt_integer(bytes: &[u8], cursor: &mut usize, end: usize) -> Result<u64, PrtDecodeError> {
    let (value, next) = crate::aml::decode_integer_constant(bytes, *cursor, end, 2)
        .map_err(|_| PrtDecodeError::BadInteger)?;
    *cursor = next;
    Ok(value.value)
}

/// Evaluate a static firmware _PRT package attached to a PCI bridge namespace
/// node. Link-device entries are resolved through the supplied resource-aware
/// resolver; direct-GSI entries are decoded without executing AML in interrupt
/// context. A method-valued _PRT is deliberately rejected until the runtime
/// supports returning nested packages without lossy object flattening.
pub fn evaluate_prt_name_object<R: LinkDeviceResolver>(
    namespace: &crate::aml::AmlNamespace,
    aml_tables: &[&[u8]],
    bridge_node: u16,
    resolver: &R,
) -> Result<PrtTable, PrtDecodeError> {
    let object = namespace.find_child(bridge_node, *b"_PRT")
        .ok_or(PrtDecodeError::MissingObject)?;
    let entry = namespace.nodes.get(object as usize)
        .filter(|_| (object as usize) < namespace.count)
        .ok_or(PrtDecodeError::MissingObject)?;
    if entry.object_type != crate::aml::AmlObjectType::Name {
        return Err(PrtDecodeError::WrongObjectType);
    }
    let aml = *aml_tables.get(entry.source_table as usize).ok_or(PrtDecodeError::BadTableIndex)?;
    let start = entry.body_start as usize;
    let end = start.checked_add(entry.body_length as usize)
        .filter(|v| *v <= aml.len()).ok_or(PrtDecodeError::Truncated)?;
    let body = &aml[start..end];
    if body.first() != Some(&0x12) { return Err(PrtDecodeError::BadPackage); }
    let (_, package_end) = crate::aml::decode_package_length(body, 1, body.len())
        .map_err(|_| PrtDecodeError::BadPackage)?;
    let length_bytes = 1 + ((body[1] >> 6) as usize);
    let mut cursor = 1 + length_bytes;
    let count = *body.get(cursor).ok_or(PrtDecodeError::Truncated)? as usize;
    cursor += 1;
    if count > MAX_PRT_ENTRIES { return Err(PrtDecodeError::TooManyEntries); }
    let mut elements = [EvaluatedPrtElement { address: 0, pin: 0, source_path: None, source_index: 0 }; MAX_PRT_ENTRIES];
    let mut paths = [[0u8; MAX_SOURCE_PATH]; MAX_PRT_ENTRIES];
    let mut link_nodes = [u16::MAX; MAX_PRT_ENTRIES];
    for index in 0..count {
        if body.get(cursor) != Some(&0x12) { return Err(PrtDecodeError::BadPackage); }
        let (_, sub_end) = crate::aml::decode_package_length(body, cursor + 1, package_end)
            .map_err(|_| PrtDecodeError::BadPackage)?;
        let pkg_len_bytes = 1 + ((body[cursor + 1] >> 6) as usize);
        let mut field = cursor + 1 + pkg_len_bytes;
        let fields = *body.get(field).ok_or(PrtDecodeError::Truncated)?;
        field += 1;
        if fields != 4 { return Err(PrtDecodeError::BadPackage); }
        let address = read_prt_integer(body, &mut field, sub_end)?;
        let pin = read_prt_integer(body, &mut field, sub_end)?;
        let source_path = match body.get(field).copied() {
            Some(0x00) => { field += 1; None }
            Some(_) => {
                let (name, next) = crate::aml::parse_name_string(body, field, sub_end)
                    .map_err(|_| PrtDecodeError::BadSource)?;
                let link = crate::aml::resolve_name_string(namespace, bridge_node, &name)
                    .map_err(|_| PrtDecodeError::UnresolvedLink)?;
                let link = namespace.resolve_alias(link).map_err(|_| PrtDecodeError::UnresolvedLink)?;
                if namespace.nodes[link as usize].object_type != crate::aml::AmlObjectType::Device {
                    return Err(PrtDecodeError::UnresolvedLink);
                }
                link_nodes[index] = link;
                field = next;
                None
            }
            None => return Err(PrtDecodeError::Truncated),
        };
        let source_index = read_prt_integer(body, &mut field, sub_end)?;
        if field != sub_end { return Err(PrtDecodeError::BadPackage); }
        elements[index] = EvaluatedPrtElement { address, pin, source_path, source_index };
        cursor = sub_end;
    }
    if cursor != package_end { return Err(PrtDecodeError::BadPackage); }
    // Materialize link paths in a second pass so the table's borrowed path
    // slices are created only after every backing buffer has been written.
    for index in 0..count {
        if link_nodes[index] != u16::MAX {
            let length = canonical_namespace_path(namespace, link_nodes[index], &mut paths[index])?;
            elements[index].source_path = Some(&paths[index][..length]);
        }
    }
    let table = build_prt_table(&elements[..count]).map_err(|_| PrtDecodeError::BadPackage)?;
    // Validate every link route while the namespace and link-device resolver
    // are live. The route table remains symbolic so later resource changes can
    // be re-resolved instead of caching a stale GSI.
    for route in table.entries[..table.count].iter().flatten() {
        if let InterruptSource::LinkDevice { path, length, source_index } = route.source {
            resolver.resolve_gsi(&path[..length as usize], source_index)
                .map_err(|_| PrtDecodeError::UnresolvedLink)?;
        }
    }
    Ok(table)
}


#[cfg(test)]
mod firmware_prt_package_tests {
    use super::*;
    use crate::aml::{AmlNode,AmlObjectType,AmlNamespace};

    struct RejectLinks;
    impl LinkDeviceResolver for RejectLinks {
        fn resolve_gsi(&self, _: &[u8], _: u32) -> Result<u32, PrtError> {
            Err(PrtError::LinkResolutionFailed)
        }
    }

    #[test]
    fn evaluates_static_prt_package_with_direct_gsi_entry() {
        // Package(1) { Package(4) { 0x0005ffff, 0, Zero, 19 } }
        let aml = [
            0x12, 15, 1, 0x12, 12, 4,
            0x0c, 0xff, 0xff, 0x05, 0x00,
            0x00,
            0x00,
            0x0a, 19,
        ];
        let mut namespace = AmlNamespace::new();
        namespace.nodes[1] = AmlNode {
            parent: 0, name: *b"PCI0", object_type: AmlObjectType::Device,
            ..AmlNode::ROOT
        };
        namespace.nodes[2] = AmlNode {
            parent: 1, name: *b"_PRT", object_type: AmlObjectType::Name,
            source_table: 0, body_start: 0, body_length: aml.len() as u32,
            ..AmlNode::ROOT
        };
        namespace.count = 3;
        let table = evaluate_prt_name_object(&namespace, &[&aml], 1, &RejectLinks).unwrap();
        assert_eq!(table.count, 1);
        assert_eq!(table.find(0x0005_0000, 0).unwrap().source,
            InterruptSource::DirectGsi { gsi: 19 });
    }

    #[test]
    fn rejects_malformed_prt_package_without_partial_routes() {
        let aml = [0x12, 4, 1, 0x12, 1, 4];
        let mut namespace = AmlNamespace::new();
        namespace.nodes[1] = AmlNode {
            parent: 0, name: *b"PCI0", object_type: AmlObjectType::Device,
            ..AmlNode::ROOT
        };
        namespace.nodes[2] = AmlNode {
            parent: 1, name: *b"_PRT", object_type: AmlObjectType::Name,
            source_table: 0, body_start: 0, body_length: aml.len() as u32,
            ..AmlNode::ROOT
        };
        namespace.count = 3;
        assert_eq!(
            evaluate_prt_name_object(&namespace, &[&aml], 1, &RejectLinks),
            Err(PrtDecodeError::BadPackage)
        );
    }
}

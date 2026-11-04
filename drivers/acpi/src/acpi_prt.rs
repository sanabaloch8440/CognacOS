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
            source_path: Some(b"\\\\_SB.PCI0.LNKA"),
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

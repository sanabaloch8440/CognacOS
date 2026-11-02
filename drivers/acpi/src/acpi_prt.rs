//! ACPI-3 PCI interrupt routing table validation.
//!
//! Consumes four-element entries after AML evaluates _PRT to a Package.
//! AML evaluation and link-device resource selection remain separate; this
//! module never guesses a GSI or programs an interrupt controller.
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

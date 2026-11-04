//! Platform-specific ACPI quirk policy, isolated from table parsing and AML execution.
//! The caller supplies the SMBIOS structure-table bytes from the UEFI SMBIOS
//! configuration-table entry. The entry point and physical mapping must already
//! have been validated by the boot/platform layer.
#![allow(clippy::too_many_arguments)]

use core::cmp;

pub const MAX_DMI_STRING: usize = 64;
pub const MAX_QUIRKS: usize = 32;
pub const MAX_OSI_STRINGS: usize = 16;
pub const MAX_BYTE_PATCHES: usize = 16;
pub const MAX_IRQ_OVERRIDES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmiField { SystemVendor, ProductName, BoardVendor, BoardName, ProductSku, BiosVersion, BiosDate }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmiValue {
    bytes: [u8; MAX_DMI_STRING],
    len: u8,
}
impl DmiValue {
    pub const EMPTY: Self = Self { bytes: [0; MAX_DMI_STRING], len: 0 };
    pub fn as_bytes(&self) -> &[u8] { &self.bytes[..self.len as usize] }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    fn set(&mut self, value: &[u8]) {
        let n = cmp::min(value.len(), MAX_DMI_STRING);
        self.bytes = [0; MAX_DMI_STRING];
        self.bytes[..n].copy_from_slice(&value[..n]);
        self.len = n as u8;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformIdentity {
    pub system_vendor: DmiValue,
    pub product_name: DmiValue,
    pub board_vendor: DmiValue,
    pub board_name: DmiValue,
    pub product_sku: DmiValue,
    pub bios_version: DmiValue,
    pub bios_date: DmiValue,
    pub smbios_major: u8,
    pub smbios_minor: u8,
}
impl PlatformIdentity {
    pub const EMPTY: Self = Self {
        system_vendor: DmiValue::EMPTY, product_name: DmiValue::EMPTY,
        board_vendor: DmiValue::EMPTY, board_name: DmiValue::EMPTY, product_sku: DmiValue::EMPTY,
        bios_version: DmiValue::EMPTY, bios_date: DmiValue::EMPTY,
        smbios_major: 0, smbios_minor: 0,
    };
    pub fn get(&self, field: DmiField) -> &[u8] {
        match field {
            DmiField::SystemVendor => self.system_vendor.as_bytes(),
            DmiField::ProductName => self.product_name.as_bytes(),
            DmiField::BoardVendor => self.board_vendor.as_bytes(),
            DmiField::BoardName => self.board_name.as_bytes(),
            DmiField::ProductSku => self.product_sku.as_bytes(),
            DmiField::BiosVersion => self.bios_version.as_bytes(),
            DmiField::BiosDate => self.bios_date.as_bytes(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmbiosError { Truncated, MalformedStructure, MissingEndMarker, TooManyStructures }

fn smbios_string(strings: &[u8], index: u8) -> Result<&[u8], SmbiosError> {
    if index == 0 { return Ok(&[]); }
    let mut current = 1u8;
    let mut start = 0usize;
    let mut i = 0usize;
    while i + 1 < strings.len() {
        if strings[i] == 0 && strings[i + 1] == 0 { break; }
        if strings[i] == 0 {
            current = current.saturating_add(1);
            start = i + 1;
        }
        i += 1;
    }
    if current == index {
        let end = strings[start..].iter().position(|b| *b == 0)
            .map(|n| start + n).unwrap_or(strings.len());
        return Ok(&strings[start..end]);
    }
    Err(SmbiosError::MalformedStructure)
}

/// Parse SMBIOS Type 0/1/2 strings from a bounded, already-mapped structure
/// table. Every formatted length and string-set terminator is checked.
pub fn parse_smbios_table(table: &[u8], major: u8, minor: u8)
    -> Result<PlatformIdentity, SmbiosError>
{
    let mut id = PlatformIdentity { smbios_major: major, smbios_minor: minor, ..PlatformIdentity::EMPTY };
    let mut off = 0usize;
    let mut structures = 0usize;
    let mut ended = false;
    while off < table.len() {
        if table.len() - off < 4 { return Err(SmbiosError::Truncated); }
        let kind = table[off];
        let formatted_len = table[off + 1] as usize;
        if formatted_len < 4 || off.checked_add(formatted_len).filter(|v| *v <= table.len()).is_none() {
            return Err(SmbiosError::MalformedStructure);
        }
        let formatted_end = off + formatted_len;
        let strings_end = table[formatted_end..].windows(2).position(|w| w == [0, 0])
            .map(|n| formatted_end + n + 2).ok_or(SmbiosError::MalformedStructure)?;
        let formatted = &table[off..formatted_end];
        let strings = &table[formatted_end..strings_end];
        structures += 1;
        if structures > 4096 { return Err(SmbiosError::TooManyStructures); }
        match kind {
            0 if formatted_len >= 6 => {
                id.bios_version.set(smbios_string(strings, formatted[5])?);
                if formatted_len >= 9 { /* vendor is intentionally not a DMI match field */ }
                if formatted_len >= 9 { id.bios_date.set(smbios_string(strings, formatted[8])?); }
            }
            1 if formatted_len >= 6 => {
                id.system_vendor.set(smbios_string(strings, formatted[4])?);
                id.product_name.set(smbios_string(strings, formatted[5])?);
                if formatted_len >= 0x1b { id.product_sku.set(smbios_string(strings, formatted[0x19])?); }
            }
            2 if formatted_len >= 6 => {
                id.board_vendor.set(smbios_string(strings, formatted[4])?);
                id.board_name.set(smbios_string(strings, formatted[5])?);
            }
            127 => { ended = true; break; }
            _ => {}
        }
        off = strings_end;
    }
    if !ended { return Err(SmbiosError::MissingEndMarker); }
    Ok(id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchKind { Exact, Prefix, Contains }

#[derive(Clone, Copy, Debug)]
pub struct DmiMatch {
    pub field: DmiField,
    pub kind: MatchKind,
    pub value: &'static [u8],
}
impl DmiMatch {
    pub const fn exact(field: DmiField, value: &'static [u8]) -> Self {
        Self { field, kind: MatchKind::Exact, value }
    }
    pub const fn prefix(field: DmiField, value: &'static [u8]) -> Self {
        Self { field, kind: MatchKind::Prefix, value }
    }
    pub const fn contains(field: DmiField, value: &'static [u8]) -> Self {
        Self { field, kind: MatchKind::Contains, value }
    }
}

fn ascii_eq_fold(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x,y)| x.eq_ignore_ascii_case(y))
}
fn matches_one(actual: &[u8], rule: &DmiMatch) -> bool {
    match rule.kind {
        MatchKind::Exact => ascii_eq_fold(actual, rule.value),
        MatchKind::Prefix => actual.get(..rule.value.len())
            .is_some_and(|v| ascii_eq_fold(v, rule.value)),
        MatchKind::Contains => actual.windows(rule.value.len())
            .any(|v| ascii_eq_fold(v, rule.value)),
    }
}
pub fn matches_all(identity: &PlatformIdentity, rules: &[DmiMatch]) -> bool {
    !rules.is_empty() && rules.iter().all(|r| matches_one(identity.get(r.field), r))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OsiAction { Advertise, Suppress }
#[derive(Clone, Copy, Debug)]
pub struct OsiOverride { pub name: &'static str, pub action: OsiAction }

#[derive(Clone, Copy, Debug)]
pub struct BytePatch {
    pub table_signature: [u8; 4],
    pub offset: u32,
    pub expected: &'static [u8],
    pub replacement: &'static [u8],
}
#[derive(Clone, Copy, Debug)]
pub struct FadtOverride {
    /// Only explicitly permitted bits may be changed. Reserved bits are never
    /// writable through this policy structure.
    pub flags_set: u32,
    pub flags_clear: u32,
    pub flags_writable_mask: u32,
    pub boot_arch_set: u16,
    pub boot_arch_clear: u16,
    pub boot_arch_writable_mask: u16,
}
#[derive(Clone, Copy, Debug)]
pub struct AdrOverride { pub parent_path: &'static str, pub from: u64, pub to: u64 }

#[derive(Clone, Copy, Debug)]
pub struct PrtOverride {
    pub segment: u16, pub bus: u8, pub device: u8, pub function: u8,
    pub pin: u8, pub gsi: u32, pub active_low: bool, pub level_triggered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuirkFlags(pub u32);
impl QuirkFlags {
    pub const NONE: Self = Self(0);
    pub const FORCE_PIC: Self = Self(1 << 0);
    pub const FORCE_APIC: Self = Self(1 << 1);
    pub const PM_TIMER_32BIT: Self = Self(1 << 2);
    pub const DISABLE_BROKEN_LINK_DEVICE: Self = Self(1 << 3);
    pub const fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}

pub type QuirkHook = fn(&PlatformIdentity, &mut QuirkPolicy) -> Result<(), QuirkError>;
#[derive(Clone, Copy)]
pub struct QuirkDefinition {
    pub id: &'static str,
    pub matches: &'static [DmiMatch],
    pub flags: QuirkFlags,
    pub osi: &'static [OsiOverride],
    pub table_patches: &'static [BytePatch],
    pub fadt: Option<FadtOverride>,
    pub prt: &'static [PrtOverride],
    pub adr: &'static [AdrOverride],
    pub hook: Option<QuirkHook>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuirkError { Capacity, ConflictingOsiPolicy, InvalidPatch, PatchMismatch, UnsupportedTable, InvalidFadtMask }

#[derive(Clone, Copy)]
pub struct QuirkPolicy {
    pub flags: QuirkFlags,
    pub matched_ids: [&'static str; MAX_QUIRKS],
    pub matched_count: usize,
    pub osi: [Option<OsiOverride>; MAX_OSI_STRINGS],
    pub osi_count: usize,
    pub patches: [Option<BytePatch>; MAX_BYTE_PATCHES],
    pub patch_count: usize,
    pub fadt: Option<FadtOverride>,
    pub prt: [Option<PrtOverride>; MAX_IRQ_OVERRIDES],
    pub prt_count: usize,
    pub adr: [Option<AdrOverride>; MAX_IRQ_OVERRIDES],
    pub adr_count: usize,
}
impl QuirkPolicy {
    pub const EMPTY: Self = Self {
        flags: QuirkFlags::NONE, matched_ids: [""; MAX_QUIRKS], matched_count: 0,
        osi: [None; MAX_OSI_STRINGS], osi_count: 0,
        patches: [None; MAX_BYTE_PATCHES], patch_count: 0,
        fadt: None, prt: [None; MAX_IRQ_OVERRIDES], prt_count: 0,
        adr: [None; MAX_IRQ_OVERRIDES], adr_count: 0,
    };
    fn add(&mut self, q: &QuirkDefinition) -> Result<(), QuirkError> {
        if self.matched_count == MAX_QUIRKS { return Err(QuirkError::Capacity); }
        self.matched_ids[self.matched_count] = q.id;
        self.matched_count += 1;
        self.flags.0 |= q.flags.0;
        for item in q.osi {
            if let Some(old) = self.osi[..self.osi_count].iter().flatten().find(|v| v.name == item.name) {
                if old.action != item.action { return Err(QuirkError::ConflictingOsiPolicy); }
                continue;
            }
            if self.osi_count == MAX_OSI_STRINGS { return Err(QuirkError::Capacity); }
            self.osi[self.osi_count] = Some(*item);
            self.osi_count += 1;
        }
        for patch in q.table_patches {
            if self.patch_count == MAX_BYTE_PATCHES { return Err(QuirkError::Capacity); }
            self.patches[self.patch_count] = Some(*patch);
            self.patch_count += 1;
        }
        for route in q.prt {
            if self.prt_count == MAX_IRQ_OVERRIDES { return Err(QuirkError::Capacity); }
            self.prt[self.prt_count] = Some(*route);
            self.prt_count += 1;
        }
        for remap in q.adr {
            if self.adr_count == MAX_IRQ_OVERRIDES { return Err(QuirkError::Capacity); }
            self.adr[self.adr_count] = Some(*remap);
            self.adr_count += 1;
        }
        if let Some(fadt) = q.fadt {
            if self.fadt.is_some() { return Err(QuirkError::InvalidFadtMask); }
            self.fadt = Some(fadt);
        }
        Ok(())
    }
    pub fn osi_is_advertised(&self, name: &str, default: bool) -> bool {
        self.osi[..self.osi_count].iter().flatten()
            .find(|v| v.name == name)
            .map(|v| v.action == OsiAction::Advertise).unwrap_or(default)
    }
}

pub fn evaluate_registry(
    identity: &PlatformIdentity,
    registry: &[QuirkDefinition],
) -> Result<QuirkPolicy, QuirkError> {
    let mut policy = QuirkPolicy::EMPTY;
    for q in registry {
        if matches_all(identity, q.matches) {
            policy.add(q)?;
            if let Some(hook) = q.hook { hook(identity, &mut policy)?; }
        }
    }
    Ok(policy)
}

/// Apply only exact, expected-byte patches to a validated in-memory SDT.
/// Recompute the ACPI checksum after each patch. This function never changes
/// the firmware-owned mapping: the caller must pass a private writable copy.
pub fn apply_table_patches(
    table: &mut [u8], patches: &[Option<BytePatch>],
) -> Result<usize, QuirkError> {
    if table.len() < 36 { return Err(QuirkError::UnsupportedTable); }
    let signature: [u8;4] = table[..4].try_into().map_err(|_| QuirkError::UnsupportedTable)?;
    let declared = u32::from_le_bytes(table[4..8].try_into().map_err(|_| QuirkError::UnsupportedTable)?) as usize;
    if declared < 36 || declared > table.len() { return Err(QuirkError::UnsupportedTable); }
    let mut applied = 0;
    for patch in patches.iter().flatten() {
        if patch.table_signature != signature { continue; }
        let start = patch.offset as usize;
        let end = start.checked_add(patch.expected.len()).ok_or(QuirkError::InvalidPatch)?;
        if patch.expected.is_empty() || patch.expected.len() != patch.replacement.len() || end > declared {
            return Err(QuirkError::InvalidPatch);
        }
        if &table[start..end] != patch.expected { return Err(QuirkError::PatchMismatch); }
        table[start..end].copy_from_slice(patch.replacement);
        table[9] = 0;
        let sum = table[..declared].iter().fold(0u8, |a,b| a.wrapping_add(*b));
        table[9] = 0u8.wrapping_sub(sum);
        applied += 1;
    }
    Ok(applied)
}

pub fn apply_fadt_override(
    flags: u32, boot_arch: u16, override_: FadtOverride,
) -> Result<(u32,u16), QuirkError> {
    if (override_.flags_set | override_.flags_clear) & !override_.flags_writable_mask != 0
        || override_.flags_set & override_.flags_clear != 0
        || (override_.boot_arch_set | override_.boot_arch_clear) & !override_.boot_arch_writable_mask != 0
        || override_.boot_arch_set & override_.boot_arch_clear != 0
    { return Err(QuirkError::InvalidFadtMask); }
    let f = (flags & !override_.flags_clear) | override_.flags_set;
    let b = (boot_arch & !override_.boot_arch_clear) | override_.boot_arch_set;
    Ok((f,b))
}

// Reference patterns only. They are deliberately not enabled by default:
// a production registry must use exact, validated DMI identities and a
// firmware-specific expected-byte signature before enabling any mutation.
pub const EXAMPLE_ASUS_MATCH: &[DmiMatch] = &[
    DmiMatch::exact(DmiField::SystemVendor, b"ASUSTeK COMPUTER INC."),
    DmiMatch::prefix(DmiField::ProductName, b"ROG"),
];
pub const EXAMPLE_LENOVO_MATCH: &[DmiMatch] = &[
    DmiMatch::exact(DmiField::SystemVendor, b"LENOVO"),
    DmiMatch::prefix(DmiField::ProductName, b"ThinkPad"),
];

// Two source-backed quirk definitions, kept opt-in until the corresponding
// hardware paths have been validated on CognacOS. The DMI identifiers and
// replacement values are documented upstream compatibility cases; this file
// contains original Rust data and matching/dispatch code, not imported code.
//
// Fujitsu Siemens firmware historically used Windows-version _OSI probes.
// Suppress only the legacy Vista strings; do not disable all Windows strings.
pub const FUJITSU_SIEMENS_OSI: &[OsiOverride] = &[
    OsiOverride { name: "Windows 2006", action: OsiAction::Suppress },
    OsiOverride { name: "Windows 2006 SP1", action: OsiAction::Suppress },
    OsiOverride { name: "Windows 2006 SP2", action: OsiAction::Suppress },
];
pub const FUJITSU_SIEMENS_MATCH: &[DmiMatch] = &[
    DmiMatch::prefix(DmiField::SystemVendor, b"FUJITSU SIEMENS"),
];

// Dell SKU 0A3E has a documented SoundWire ACPI _ADR topology mismatch.
// These remaps are scoped to the SoundWire parent namespace and must be
// consumed only by the ACPI namespace _ADR resolution hook.
pub const DELL_0A3E_ADR: &[AdrOverride] = &[
    AdrOverride { parent_path: "\\\\_SB.SDWT", from: 0x00020025d071100, to: 0x000210025d071500 },
    AdrOverride { parent_path: "\\\\_SB.SDWT", from: 0x000120025d130800, to: 0x000120025d071100 },
    AdrOverride { parent_path: "\\\\_SB.SDWT", from: 0x000220025d071500, to: 0x000220025d130800 },
];
pub const DELL_0A3E_MATCH: &[DmiMatch] = &[
    DmiMatch::exact(DmiField::SystemVendor, b"Dell Inc"),
    DmiMatch::exact(DmiField::ProductSku, b"0A3E"),
];

pub const FUJITSU_SIEMENS_QUIRK: QuirkDefinition = QuirkDefinition {
    id: "fujitsu-siemens-legacy-osi",
    matches: FUJITSU_SIEMENS_MATCH, flags: QuirkFlags::NONE,
    osi: FUJITSU_SIEMENS_OSI, table_patches: &[], fadt: None, prt: &[], adr: &[], hook: None,
};
pub const DELL_0A3E_QUIRK: QuirkDefinition = QuirkDefinition {
    id: "dell-sku-0a3e-soundwire-adr",
    matches: DELL_0A3E_MATCH, flags: QuirkFlags::NONE,
    osi: &[], table_patches: &[], fadt: None, prt: &[], adr: DELL_0A3E_ADR, hook: None,
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dmi_match_is_case_insensitive_and_requires_all_fields() {
        let mut p = PlatformIdentity::EMPTY;
        p.system_vendor.set(b"ASUSTeK COMPUTER INC.");
        p.product_name.set(b"ROG Zephyrus");
        assert!(matches_all(&p, EXAMPLE_ASUS_MATCH));
        p.product_name.set(b"Vivobook");
        assert!(!matches_all(&p, EXAMPLE_ASUS_MATCH));
    }
    #[test]
    fn patch_requires_exact_original_bytes_and_repairs_checksum() {
        let mut table = [0u8; 40];
        table[..4].copy_from_slice(b"DSDT");
        table[4..8].copy_from_slice(&40u32.to_le_bytes());
        table[36..40].copy_from_slice(b"TEST");
        let patch = BytePatch { table_signature: *b"DSDT", offset: 36, expected: b"TEST", replacement: b"SAFE" };
        assert_eq!(apply_table_patches(&mut table, &[Some(patch)]), Ok(1));
        assert_eq!(&table[36..40], b"SAFE");
        assert_eq!(table.iter().fold(0u8, |a,b| a.wrapping_add(*b)), 0);
    }
    #[test]
    fn fadt_override_cannot_change_unapproved_bits() {
        let q = FadtOverride { flags_set: 1 << 8, flags_clear: 0, flags_writable_mask: 1 << 8,
            boot_arch_set: 0, boot_arch_clear: 0, boot_arch_writable_mask: 0 };
        assert_eq!(apply_fadt_override(0, 0, q), Ok((1 << 8, 0)));
        let bad = FadtOverride { flags_set: 1 << 20, flags_clear: 0, flags_writable_mask: 0,
            boot_arch_set: 0, boot_arch_clear: 0, boot_arch_writable_mask: 0 };
        assert_eq!(apply_fadt_override(0, 0, bad), Err(QuirkError::InvalidFadtMask));
    }
}

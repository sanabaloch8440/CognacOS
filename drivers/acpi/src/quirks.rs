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
pub enum DmiField { SystemVendor, ProductName, BoardVendor, BoardName, ProductSku, ProductVersion, BiosVersion, BiosDate }

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
    pub product_version: DmiValue,
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
            DmiField::ProductVersion => self.product_version.as_bytes(),
            DmiField::BiosVersion => self.bios_version.as_bytes(),
            DmiField::BiosDate => self.bios_date.as_bytes(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmbiosError { Truncated, MalformedStructure, MissingEndMarker, TooManyStructures }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmbiosTableDescriptor {
    pub major: u8, pub minor: u8, pub table_address: u64, pub table_length: u32,
    pub is_64_bit_entry_point: bool,
}
fn checksum_zero(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) == 0
}

/// Validate an SMBIOS 2.x (_SM_) or 3.x (_SM3_) entry point and return the
/// physical table descriptor. The caller must map exactly the returned
/// bounded range before calling parse_smbios_table().
pub fn parse_smbios_entry_point(bytes: &[u8]) -> Result<SmbiosTableDescriptor, SmbiosError> {
    if bytes.len() < 5 { return Err(SmbiosError::Truncated); }
    if bytes.get(..5) == Some(b"_SM3_") {
        if bytes.len() < 24 { return Err(SmbiosError::Truncated); }
        let len = bytes[6] as usize;
        if len < 24 || len > bytes.len() || !checksum_zero(&bytes[..len]) {
            return Err(SmbiosError::MalformedStructure);
        }
        let max_size = u32::from_le_bytes(bytes[12..16].try_into().map_err(|_| SmbiosError::Truncated)?);
        let address = u64::from_le_bytes(bytes[16..24].try_into().map_err(|_| SmbiosError::Truncated)?);
        if max_size == 0 || address == 0 || address.checked_add(max_size as u64).is_none() {
            return Err(SmbiosError::MalformedStructure);
        }
        return Ok(SmbiosTableDescriptor { major: bytes[7], minor: bytes[8],
            table_address: address, table_length: max_size, is_64_bit_entry_point: true });
    }
    if bytes.get(..4) == Some(b"_SM_") {
        if bytes.len() < 31 { return Err(SmbiosError::Truncated); }
        let len = bytes[5] as usize;
        if len < 31 || len > bytes.len() || !checksum_zero(&bytes[..len])
            || bytes.get(16..21) != Some(b"_DMI_") || !checksum_zero(&bytes[16..31]) {
            return Err(SmbiosError::MalformedStructure);
        }
        let table_len = u16::from_le_bytes([bytes[22], bytes[23]]) as u32;
        let address = u32::from_le_bytes(bytes[24..28].try_into().map_err(|_| SmbiosError::Truncated)?) as u64;
        if table_len == 0 || address == 0 || address.checked_add(table_len as u64).is_none() {
            return Err(SmbiosError::MalformedStructure);
        }
        return Ok(SmbiosTableDescriptor { major: bytes[6], minor: bytes[7],
            table_address: address, table_length: table_len, is_64_bit_entry_point: false });
    }
    Err(SmbiosError::MalformedStructure)
}

fn smbios_string(strings: &[u8], index: u8) -> Result<&[u8], SmbiosError> {
    if index == 0 { return Ok(&[]); }
    if strings.first() == Some(&0) { return Err(SmbiosError::MalformedStructure); }
    let mut current = 1u8;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < strings.len() {
        if strings[i] == 0 {
            if i + 1 < strings.len() && strings[i + 1] == 0 { break; }
            if current == index { return Ok(&strings[start..i]); }
            current = current.saturating_add(1);
            start = i + 1;
        }
        i += 1;
    }
    if current == index && start < strings.len() {
        let end = strings[start..].iter().position(|b| *b == 0)
            .map(|n| start + n).unwrap_or(strings.len());
        return Ok(&strings[start..end]);
    }
    Err(SmbiosError::MalformedStructure)
}

/// Parse SMBIOS Type 0/1/2 strings from a bounded, already-mapped structure
/// table. Every formatted length and string-set terminator is checked.
/// UEFI EFI_CONFIGURATION_TABLE GUID and vendor table pointer (x64 ABI).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EfiGuid { pub data1: u32, pub data2: u16, pub data3: u16, pub data4: [u8; 8] }
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EfiConfigurationTable { pub vendor_guid: EfiGuid, pub vendor_table: u64 }

pub const SMBIOS_TABLE_GUID: EfiGuid = EfiGuid {
    data1: 0xeb9d2d31, data2: 0x2d88, data3: 0x11d3,
    data4: [0x9a,0x16,0x00,0x90,0x27,0x3f,0xc1,0x4d],
};
pub const SMBIOS3_TABLE_GUID: EfiGuid = EfiGuid {
    data1: 0xf2fd1544, data2: 0x9794, data3: 0x4a2c,
    data4: [0x99,0x2e,0xe5,0xbb,0xcf,0x20,0xe3,0x94],
};

/// Prefer the SMBIOS 3.x entry point, falling back to SMBIOS 2.x. Returns
/// only the physical entry-point address; mapping and memory-range validation
/// remain the boot memory manager's responsibility.
pub fn find_smbios_entry_point(entries: &[EfiConfigurationTable]) -> Option<u64> {
    entries.iter().find(|e| e.vendor_guid == SMBIOS3_TABLE_GUID && e.vendor_table != 0)
        .or_else(|| entries.iter().find(|e| e.vendor_guid == SMBIOS_TABLE_GUID && e.vendor_table != 0))
        .map(|e| e.vendor_table)
}

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
                if formatted_len >= 7 { id.product_version.set(smbios_string(strings, formatted[6])?); }
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
    pub const IGNORE_IRQ0_OVERRIDE: Self = Self(1 << 4);
    pub const DISABLE_ACPI_PCI_IRQ_ROUTING: Self = Self(1 << 5);
    pub const DISABLE_ACPI_PCI_ENUMERATION: Self = Self(1 << 6);
    pub const PREFER_RSDT: Self = Self(1 << 7);
    pub const IGNORE_E820_FOR_PCI_CRS: Self = Self(1 << 8);
    pub const fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}

impl core::ops::BitOr for QuirkFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output { Self(self.0 | rhs.0) }
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
    /// Namespace hook: remap only the exact _ADR value under the exact
    /// caller-supplied parent path. A missing match preserves firmware data.
    pub fn override_adr(&self, parent_path: &str, firmware_adr: u64) -> u64 {
        self.adr[..self.adr_count].iter().flatten()
            .find(|v| (v.parent_path == "*" || v.parent_path == parent_path) && v.from == firmware_adr)
            .map(|v| v.to).unwrap_or(firmware_adr)
    }
    pub fn override_prt_gsi(&self, segment: u16, bus: u8, device: u8,
        function: u8, pin: u8) -> Option<PrtOverride> {
        self.prt[..self.prt_count].iter().flatten()
            .find(|v| v.segment == segment && v.bus == bus && v.device == device
                && v.function == function && v.pin == pin).copied()
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
    // Validate the entire patch set before changing a byte. A stale expected
    // value therefore leaves the private table copy completely untouched.
    let mut applied = 0usize;
    for patch in patches.iter().flatten() {
        if patch.table_signature != signature { continue; }
        let start = patch.offset as usize;
        let end = start.checked_add(patch.expected.len()).ok_or(QuirkError::InvalidPatch)?;
        if start < 36 || patch.expected.is_empty()
            || patch.expected.len() != patch.replacement.len() || end > declared {
            return Err(QuirkError::InvalidPatch);
        }
        if &table[start..end] != patch.expected { return Err(QuirkError::PatchMismatch); }
        applied += 1;
    }
    if applied == 0 { return Ok(0); }
    for patch in patches.iter().flatten() {
        if patch.table_signature != signature { continue; }
        let start = patch.offset as usize;
        let end = start + patch.expected.len();
        table[start..end].copy_from_slice(patch.replacement);
    }
    table[9] = 0;
    let sum = table[..declared].iter().fold(0u8, |a,b| a.wrapping_add(*b));
    table[9] = 0u8.wrapping_sub(sum);
    Ok(applied)
}

/// Apply the selected FADT policy to an already validated FADT model.
pub fn apply_policy_to_fadt(
    info: &mut crate::FadtInfo,
    policy: &QuirkPolicy,
) -> Result<(), QuirkError> {
    let timer_quirk = policy.flags.contains(QuirkFlags::PM_TIMER_32BIT);
    if policy.fadt.is_none() && !timer_quirk { return Ok(()); }
    let mut override_ = policy.fadt.unwrap_or(FadtOverride {
        flags_set: 0, flags_clear: 0, flags_writable_mask: 0,
        boot_arch_set: 0, boot_arch_clear: 0, boot_arch_writable_mask: 0,
    });
    if timer_quirk {
        override_.flags_set |= 1 << 8; // FADT.TMR_VAL_EXT
        override_.flags_writable_mask |= 1 << 8;
    }
    let (flags, boot_arch) = apply_fadt_override(
        info.flags, info.boot_arch_flags, override_,
    )?;
    info.flags = flags;
    info.boot_arch_flags = boot_arch;
    Ok(())
}

pub const QUIRK_FADT_FLAGS_WRITABLE_MASK: u32 = 1 << 8; // TMR_VAL_EXT only.
pub const QUIRK_FADT_BOOT_ARCH_WRITABLE_MASK: u16 = 0x001f;

/// Apply only explicitly safe FADT quirk bits. In particular, policy cannot
/// flip HARDWARE_REDUCED_ACPI or LOW_POWER_S0_IDLE_CAPABLE: those select a
/// different platform programming model and are not repairable guesses.
pub fn apply_fadt_override(
    flags: u32, boot_arch: u16, override_: FadtOverride,
) -> Result<(u32,u16), QuirkError> {
    if override_.flags_writable_mask & !QUIRK_FADT_FLAGS_WRITABLE_MASK != 0
        || override_.boot_arch_writable_mask & !QUIRK_FADT_BOOT_ARCH_WRITABLE_MASK != 0
        || (override_.flags_set | override_.flags_clear) & !override_.flags_writable_mask != 0
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
// The upstream compatibility case applies these exact 64-bit _ADR values
// at the SoundWire bus layer rather than under one fixed AML parent. The
// explicit "*" scope is safe only because the enclosing DMI match is exact
// on Dell Inc + product SKU 0A3E and each source _ADR must match exactly.
pub const DELL_0A3E_ADR: &[AdrOverride] = &[
    AdrOverride { parent_path: "*", from: 0x00020025d071100, to: 0x000210025d071500 },
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


/// DMI-scoped early ACPI/PCI compatibility cases documented by Linux.
pub const HP_IRQ0_NX6115_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::SystemVendor,b"Hewlett-Packard"),DmiMatch::exact(DmiField::ProductName,b"HP Compaq nx6115")];
pub const HP_IRQ0_NX6125_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::SystemVendor,b"Hewlett-Packard"),DmiMatch::exact(DmiField::ProductName,b"HP Compaq nx6125")];
pub const HP_IRQ0_NX6325_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::SystemVendor,b"Hewlett-Packard"),DmiMatch::exact(DmiField::ProductName,b"HP Compaq nx6325")];
pub const IBM_600_2645_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::BoardVendor,b"IBM"),DmiMatch::exact(DmiField::BoardName,b"2645")];
pub const IBM_600_2646_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::BoardVendor,b"IBM"),DmiMatch::exact(DmiField::BoardName,b"2646")];
pub const ASUS_PR_DLS_MATCH:&[DmiMatch]=&[
 DmiMatch::exact(DmiField::BoardVendor,b"ASUSTeK Computer INC."),DmiMatch::exact(DmiField::BoardName,b"PR-DLS"),
 DmiMatch::exact(DmiField::BiosVersion,b"ASUS PR-DLS ACPI BIOS Revision 1010"),DmiMatch::exact(DmiField::BiosDate,b"03/21/2003")];
pub const ACER_TRAVELMATE_360_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::SystemVendor,b"Acer"),DmiMatch::exact(DmiField::ProductName,b"TravelMate 360")];
pub const NEC_BEARLAKE_MATCH:&[DmiMatch]=&[
 DmiMatch::exact(DmiField::SystemVendor,b"NEC"),DmiMatch::exact(DmiField::ProductName,b"Bearlake CRB Board"),
 DmiMatch::exact(DmiField::BiosVersion,b"V1.12"),DmiMatch::exact(DmiField::BiosDate,b"02/01/2011")];
pub const LENOVO_IIL_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::SystemVendor,b"LENOVO"),DmiMatch::contains(DmiField::ProductVersion,b"IIL")];
pub const ACER_SPIN_5_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::SystemVendor,b"Acer"),DmiMatch::exact(DmiField::ProductName,b"Spin SP513-54N")];
pub const CLEVO_X170KM_G_MATCH:&[DmiMatch]=&[DmiMatch::exact(DmiField::BoardName,b"X170KM-G")];

macro_rules! quirk_def {
 ($name:ident,$id:literal,$matches:ident,$flags:expr) => {
  pub const $name:QuirkDefinition=QuirkDefinition{id:$id,matches:$matches,flags:$flags,osi:&[],table_patches:&[],fadt:None,prt:&[],adr:&[],hook:None};
 };
}
quirk_def!(HP_NX6115_IRQ0_QUIRK,"hp-compaq-nx6115-ignore-irq0-override",HP_IRQ0_NX6115_MATCH,QuirkFlags::IGNORE_IRQ0_OVERRIDE);
quirk_def!(HP_NX6125_IRQ0_QUIRK,"hp-compaq-nx6125-ignore-irq0-override",HP_IRQ0_NX6125_MATCH,QuirkFlags::IGNORE_IRQ0_OVERRIDE);
quirk_def!(HP_NX6325_IRQ0_QUIRK,"hp-compaq-nx6325-ignore-irq0-override",HP_IRQ0_NX6325_MATCH,QuirkFlags::IGNORE_IRQ0_OVERRIDE);
quirk_def!(IBM_600_2645_IRQ_QUIRK,"ibm-thinkpad-600-2645-disable-acpi-pci-irq",IBM_600_2645_MATCH,QuirkFlags::DISABLE_ACPI_PCI_IRQ_ROUTING);
quirk_def!(IBM_600_2646_IRQ_QUIRK,"ibm-thinkpad-600-2646-disable-acpi-pci-irq",IBM_600_2646_MATCH,QuirkFlags::DISABLE_ACPI_PCI_IRQ_ROUTING);
quirk_def!(ASUS_PR_DLS_PCI_QUIRK,"asus-pr-dls-disable-acpi-pci",ASUS_PR_DLS_MATCH,QuirkFlags::DISABLE_ACPI_PCI_ENUMERATION|QuirkFlags::DISABLE_ACPI_PCI_IRQ_ROUTING);
quirk_def!(ACER_TRAVELMATE_360_PCI_QUIRK,"acer-travelmate-360-disable-acpi-pci",ACER_TRAVELMATE_360_MATCH,QuirkFlags::DISABLE_ACPI_PCI_ENUMERATION|QuirkFlags::DISABLE_ACPI_PCI_IRQ_ROUTING);
quirk_def!(NEC_BEARLAKE_RSDT_QUIRK,"nec-bearlake-v1-12-prefer-rsdt",NEC_BEARLAKE_MATCH,QuirkFlags::PREFER_RSDT);
quirk_def!(LENOVO_IIL_PCI_CRS_QUIRK,"lenovo-iil-ignore-e820-for-pci-crs",LENOVO_IIL_MATCH,QuirkFlags::IGNORE_E820_FOR_PCI_CRS);
quirk_def!(ACER_SPIN_5_PCI_CRS_QUIRK,"acer-spin-sp513-54n-ignore-e820-for-pci-crs",ACER_SPIN_5_MATCH,QuirkFlags::IGNORE_E820_FOR_PCI_CRS);
quirk_def!(CLEVO_X170KM_G_PCI_CRS_QUIRK,"clevo-x170km-g-ignore-e820-for-pci-crs",CLEVO_X170KM_G_MATCH,QuirkFlags::IGNORE_E820_FOR_PCI_CRS);

pub const PLATFORM_QUIRK_REGISTRY:&[QuirkDefinition]=&[
 FUJITSU_SIEMENS_QUIRK,DELL_0A3E_QUIRK,
 HP_NX6115_IRQ0_QUIRK,HP_NX6125_IRQ0_QUIRK,HP_NX6325_IRQ0_QUIRK,
 IBM_600_2645_IRQ_QUIRK,IBM_600_2646_IRQ_QUIRK,ASUS_PR_DLS_PCI_QUIRK,
 ACER_TRAVELMATE_360_PCI_QUIRK,NEC_BEARLAKE_RSDT_QUIRK,
 LENOVO_IIL_PCI_CRS_QUIRK,ACER_SPIN_5_PCI_CRS_QUIRK,CLEVO_X170KM_G_PCI_CRS_QUIRK,
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smbios_string_indices_select_the_correct_string() {
        let strings = b"Vendor\0Product\0SKU\0\0";
        assert_eq!(smbios_string(strings, 1), Ok(&b"Vendor"[..]));
        assert_eq!(smbios_string(strings, 2), Ok(&b"Product"[..]));
        assert_eq!(smbios_string(strings, 3), Ok(&b"SKU"[..]));
    }
    #[test]
    fn smbios_guid_lookup_prefers_3x() {
        let entries = [
            EfiConfigurationTable { vendor_guid: SMBIOS_TABLE_GUID, vendor_table: 0x1000 },
            EfiConfigurationTable { vendor_guid: SMBIOS3_TABLE_GUID, vendor_table: 0x2000 },
        ];
        assert_eq!(find_smbios_entry_point(&entries), Some(0x2000));
    }
    #[test]
    fn dell_adr_override_is_exact_and_dmi_scoped() {
        let mut policy = QuirkPolicy::EMPTY;
        policy.adr[0] = Some(AdrOverride {
            parent_path: "*", from: 0x00020025d071100,
            to: 0x00021025d071500,
        });
        policy.adr_count = 1;
        assert_eq!(policy.override_adr("\\\\_SB.SDWT", 0x00020025d071100), 0x00021025d071500);
        assert_eq!(policy.override_adr("\\\\_SB.SDWT", 0x00020025d071101), 0x00020025d071101);
    }
    #[test]
    fn missing_smbios_string_index_is_not_accepted_as_empty() {
        assert_eq!(smbios_string(b"\0\0", 1), Err(SmbiosError::MalformedStructure));
    }
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
    fn fadt_override_cannot_change_hardware_reduced_mode() {
        let bad = FadtOverride {
            flags_set: 1 << 20, flags_clear: 0,
            flags_writable_mask: 1 << 20,
            boot_arch_set: 0, boot_arch_clear: 0, boot_arch_writable_mask: 0,
        };
        assert_eq!(apply_fadt_override(0, 0, bad), Err(QuirkError::InvalidFadtMask));
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

    #[test]
    fn stage1_dmi_registry_selects_only_the_documented_platform_actions() {
        let mut hp = PlatformIdentity::EMPTY;
        hp.system_vendor.set(b"Hewlett-Packard");
        hp.product_name.set(b"HP Compaq nx6115");
        let hp_policy = evaluate_registry(&hp, PLATFORM_QUIRK_REGISTRY).unwrap();
        assert!(hp_policy.flags.contains(QuirkFlags::IGNORE_IRQ0_OVERRIDE));
        assert!(!hp_policy.flags.contains(QuirkFlags::DISABLE_ACPI_PCI_ENUMERATION));

        let mut ibm = PlatformIdentity::EMPTY;
        ibm.board_vendor.set(b"IBM");
        ibm.board_name.set(b"2645");
        let ibm_policy = evaluate_registry(&ibm, PLATFORM_QUIRK_REGISTRY).unwrap();
        assert!(ibm_policy.flags.contains(QuirkFlags::DISABLE_ACPI_PCI_IRQ_ROUTING));
        assert!(!ibm_policy.flags.contains(QuirkFlags::DISABLE_ACPI_PCI_ENUMERATION));

        let mut asus = PlatformIdentity::EMPTY;
        asus.board_vendor.set(b"ASUSTeK Computer INC.");
        asus.board_name.set(b"PR-DLS");
        asus.bios_version.set(b"ASUS PR-DLS ACPI BIOS Revision 1010");
        asus.bios_date.set(b"03/21/2003");
        let asus_policy = evaluate_registry(&asus, PLATFORM_QUIRK_REGISTRY).unwrap();
        assert!(asus_policy.flags.contains(QuirkFlags::DISABLE_ACPI_PCI_ENUMERATION));
        assert!(asus_policy.flags.contains(QuirkFlags::DISABLE_ACPI_PCI_IRQ_ROUTING));
    }

    #[test]
    fn stage1_lenovo_iil_requires_product_version_match() {
        let mut identity = PlatformIdentity::EMPTY;
        identity.system_vendor.set(b"LENOVO");
        identity.product_version.set(b"Yoga IIL platform");
        let policy = evaluate_registry(&identity, PLATFORM_QUIRK_REGISTRY).unwrap();
        assert!(policy.flags.contains(QuirkFlags::IGNORE_E820_FOR_PCI_CRS));
        identity.product_version.set(b"Yoga platform");
        let policy = evaluate_registry(&identity, PLATFORM_QUIRK_REGISTRY).unwrap();
        assert!(!policy.flags.contains(QuirkFlags::IGNORE_E820_FOR_PCI_CRS));
    }
}

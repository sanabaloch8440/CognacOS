//! Bounded ACPI table parsing primitives for platform discovery.
//! Callers must pass a readable, mapped byte slice obtained from firmware data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpiError { Truncated, BadSignature, BadLength, BadChecksum, TooManyEntries, MalformedEntry, MissingRoot, MissingMadt, AddressOutOfRange }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuEntry { pub apic_id: u32, pub processor_uid: u32, pub enabled: bool, pub x2apic: bool }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoApicEntry { pub id: u8, pub address: u32, pub gsi_base: u32 }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrqOverride { pub bus: u8, pub source_irq: u8, pub gsi: u32, pub active_low: bool, pub level_triggered: bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MadtInfo {
    pub local_apic_address: u64,
    pub flags: u32,
    pub cpus: [CpuEntry; 64],
    pub cpu_count: usize,
    pub io_apics: [IoApicEntry; 8],
    pub io_apic_count: usize,
    pub irq_overrides: [IrqOverride; 16],
    pub irq_override_count: usize,
}
impl MadtInfo {
    pub const EMPTY: Self = Self { local_apic_address: 0, flags: 0, cpus: [CpuEntry { apic_id: 0, processor_uid: 0, enabled: false, x2apic: false }; 64], cpu_count: 0, io_apics: [IoApicEntry { id: 0, address: 0, gsi_base: 0 }; 8], io_apic_count: 0, irq_overrides: [IrqOverride { bus: 0, source_irq: 0, gsi: 0, active_low: false, level_triggered: false }; 16], irq_override_count: 0 };
}
fn u32le(b: &[u8], o: usize) -> Result<u32, AcpiError> {
    let s = b.get(o..o + 4).ok_or(AcpiError::Truncated)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn checksum_ok(bytes: &[u8]) -> bool { bytes.iter().fold(0u8, |sum, v| sum.wrapping_add(*v)) == 0 }
/// Parse a complete ACPI MADT (APIC) table including its 36-byte SDT header.
/// Unknown record types are skipped according to their validated record length.
pub fn parse_madt(bytes: &[u8]) -> Result<MadtInfo, AcpiError> {
    if bytes.len() < 44 { return Err(AcpiError::Truncated); }
    if bytes.get(0..4) != Some(b"APIC") { return Err(AcpiError::BadSignature); }
    let length = u32le(bytes, 4)? as usize;
    if length < 44 || length > bytes.len() { return Err(AcpiError::BadLength); }
    if !checksum_ok(&bytes[..length]) { return Err(AcpiError::BadChecksum); }
    let mut out = MadtInfo::EMPTY;
    out.local_apic_address = u32le(bytes, 36)? as u64;
    out.flags = u32le(bytes, 40)?;
    let mut offset = 44;
    while offset < length {
        if offset + 2 > length { return Err(AcpiError::MalformedEntry); }
        let kind = bytes[offset];
        let size = bytes[offset + 1] as usize;
        if size < 2 || offset.checked_add(size).filter(|end| *end <= length).is_none() { return Err(AcpiError::MalformedEntry); }
        let e = &bytes[offset..offset + size];
        match kind {
            0 if size >= 8 => {
                if out.cpu_count == out.cpus.len() { return Err(AcpiError::TooManyEntries); }
                let flags = u32le(e, 4)?;
                out.cpus[out.cpu_count] = CpuEntry { processor_uid: e[2] as u32, apic_id: e[3] as u32, enabled: flags & 3 != 0, x2apic: false };
                out.cpu_count += 1;
            }
            1 if size >= 12 => {
                if out.io_apic_count == out.io_apics.len() { return Err(AcpiError::TooManyEntries); }
                out.io_apics[out.io_apic_count] = IoApicEntry { id: e[2], address: u32le(e, 4)?, gsi_base: u32le(e, 8)? };
                out.io_apic_count += 1;
            }
            2 if size >= 10 => {
                if out.irq_override_count == out.irq_overrides.len() { return Err(AcpiError::TooManyEntries); }
                let flags = u16::from_le_bytes([e[8], e[9]]);
                let polarity = flags & 0x3;
                let trigger = (flags >> 2) & 0x3;
                if polarity == 2 || trigger == 2 { return Err(AcpiError::MalformedEntry); }
                out.irq_overrides[out.irq_override_count] = IrqOverride {
                    bus: e[2], source_irq: e[3], gsi: u32le(e, 4)?,
                    active_low: polarity == 3, level_triggered: trigger == 3,
                };
                out.irq_override_count += 1;
            }
            5 if size >= 12 => out.local_apic_address = u64::from_le_bytes([e[4],e[5],e[6],e[7],e[8],e[9],e[10],e[11]]),
            9 if size >= 16 => {
                if out.cpu_count == out.cpus.len() { return Err(AcpiError::TooManyEntries); }
                let flags = u32le(e, 8)?;
                out.cpus[out.cpu_count] = CpuEntry { processor_uid: u32le(e, 12)?, apic_id: u32le(e, 4)?, enabled: flags & 3 != 0, x2apic: true };
                out.cpu_count += 1;
            }
            _ => {}
        }
        offset += size;
    }
    if out.local_apic_address == 0 { return Err(AcpiError::MalformedEntry); }
    Ok(out)
}
/// Validate the ACPI RSDP v1/v2 checksums and report its revision.
/// The caller is responsible for discovering a candidate in a firmware-defined region.
pub fn validate_rsdp(bytes: &[u8]) -> Result<u8, AcpiError> {
    if bytes.len() < 20 { return Err(AcpiError::Truncated); }
    if bytes.get(0..8) != Some(b"RSD PTR ") { return Err(AcpiError::BadSignature); }
    if !checksum_ok(&bytes[..20]) { return Err(AcpiError::BadChecksum); }
    let revision = bytes[15];
    if revision >= 2 {
        if bytes.len() < 36 { return Err(AcpiError::Truncated); }
        let length = u32le(bytes, 20)? as usize;
        if length < 36 || length > bytes.len() { return Err(AcpiError::BadLength); }
        if !checksum_ok(&bytes[..length]) { return Err(AcpiError::BadChecksum); }
    }
    Ok(revision)
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RsdpRoots {
    pub revision: u8,
    pub rsdt_address: u32,
    pub xsdt_address: u64,
}

/// Parse and validate the ACPI RSDP, returning root-table physical addresses.
pub fn parse_rsdp(bytes: &[u8]) -> Result<RsdpRoots, AcpiError> {
    let revision = validate_rsdp(bytes)?;
    let rsdt_address = u32le(bytes, 16)?;
    let xsdt_address = if revision >= 2 {
        let s = bytes.get(24..32).ok_or(AcpiError::Truncated)?;
        u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
    } else { 0 };
    if rsdt_address == 0 && xsdt_address == 0 { return Err(AcpiError::MissingRoot); }
    Ok(RsdpRoots { revision, rsdt_address, xsdt_address })
}

const MAX_ACPI_TABLE_LENGTH: usize = 1024 * 1024;
const LOW_IDENTITY_LIMIT: u64 = 0x1_0000_0000;

/// Follow the firmware-provided RSDP to an RSDT/XSDT and locate the MADT.
/// The bootstrap identity-maps only 0..4 GiB, so physical ranges are checked
/// before dereferencing. This routine discovers topology but enables no devices.
pub unsafe fn discover_madt(rsdp_address: u64) -> Result<MadtInfo, AcpiError> {
    if rsdp_address == 0 || rsdp_address.checked_add(36).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
        return Err(AcpiError::AddressOutOfRange);
    }
    let rsdp_v1 = unsafe { core::slice::from_raw_parts(rsdp_address as *const u8, 20) };
    let revision = *rsdp_v1.get(15).ok_or(AcpiError::Truncated)?;
    let rsdp_len = if revision >= 2 {
        if rsdp_address.checked_add(36).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
            return Err(AcpiError::AddressOutOfRange);
        }
        let rsdp_v2_prefix = unsafe { core::slice::from_raw_parts(rsdp_address as *const u8, 36) };
        let n = u32le(rsdp_v2_prefix, 20)? as usize;
        if !(36..=4096).contains(&n) { return Err(AcpiError::BadLength); }
        n
    } else { 20 };
    let rsdp_end = rsdp_address.checked_add(rsdp_len as u64).ok_or(AcpiError::AddressOutOfRange)?;
    if rsdp_end > LOW_IDENTITY_LIMIT { return Err(AcpiError::AddressOutOfRange); }
    let rsdp = unsafe { core::slice::from_raw_parts(rsdp_address as *const u8, rsdp_len) };
    let roots = parse_rsdp(rsdp)?;
    let (root_address, is_xsdt) = if roots.revision >= 2 && roots.xsdt_address != 0
        && roots.xsdt_address < LOW_IDENTITY_LIMIT {
        (roots.xsdt_address, true)
    } else if roots.rsdt_address != 0 {
        (roots.rsdt_address as u64, false)
    } else { return Err(AcpiError::MissingRoot); };
    if root_address == 0 || root_address.checked_add(36).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
        return Err(AcpiError::AddressOutOfRange);
    }
    let root_header = unsafe { core::slice::from_raw_parts(root_address as *const u8, 36) };
    let root_length = u32le(root_header, 4)? as usize;
    if !(36..=MAX_ACPI_TABLE_LENGTH).contains(&root_length) {
        return Err(AcpiError::BadLength);
    }
    let root_end = root_address.checked_add(root_length as u64).ok_or(AcpiError::AddressOutOfRange)?;
    if root_end > LOW_IDENTITY_LIMIT { return Err(AcpiError::AddressOutOfRange); }
    let root = unsafe { core::slice::from_raw_parts(root_address as *const u8, root_length) };
    if !checksum_ok(root) { return Err(AcpiError::BadChecksum); }
    let expected: &[u8] = if is_xsdt { b"XSDT" } else { b"RSDT" };
    if root.get(0..4) != Some(expected) { return Err(AcpiError::BadSignature); }
    let width = if is_xsdt { 8usize } else { 4usize };
    let payload = root_length - 36;
    if payload % width != 0 { return Err(AcpiError::MalformedEntry); }
    for offset in (36..root_length).step_by(width) {
        let address = if is_xsdt {
            let s = &root[offset..offset + 8];
            u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
        } else { u32le(root, offset)? as u64 };
        if address == 0 || address >= LOW_IDENTITY_LIMIT || address.checked_add(36).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
            continue;
        }
        let header = unsafe { core::slice::from_raw_parts(address as *const u8, 36) };
        if header.get(0..4) != Some(b"APIC") { continue; }
        let length = u32le(header, 4)? as usize;
        if !(44..=MAX_ACPI_TABLE_LENGTH).contains(&length) { return Err(AcpiError::BadLength); }
        if address.checked_add(length as u64).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
            return Err(AcpiError::AddressOutOfRange);
        }
        let table = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
        return parse_madt(table);
    }
    Err(AcpiError::MissingMadt)
}


/// Locate the Nth ACPI system-description table matching a four-byte signature.
/// This validates the RSDP, root table, each candidate SDT length and checksum.
/// The current bootstrap identity map is limited to 4 GiB; XSDT entries above
/// that limit are skipped rather than dereferenced. The returned slice borrows
/// firmware-owned physical memory and is valid only while that mapping remains.
pub unsafe fn find_table(rsdp_address: u64, signature: [u8; 4], instance: usize) -> Result<&'static [u8], AcpiError> {
    if rsdp_address == 0 || rsdp_address.checked_add(36).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
        return Err(AcpiError::AddressOutOfRange);
    }
    let prefix = unsafe { core::slice::from_raw_parts(rsdp_address as *const u8, 36) };
    let revision = prefix[15];
    let rsdp_len = if revision >= 2 {
        let n = u32le(prefix, 20)? as usize;
        if !(36..=4096).contains(&n) { return Err(AcpiError::BadLength); }
        n
    } else { 20 };
    if rsdp_address.checked_add(rsdp_len as u64).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
        return Err(AcpiError::AddressOutOfRange);
    }
    let rsdp = unsafe { core::slice::from_raw_parts(rsdp_address as *const u8, rsdp_len) };
    let roots = parse_rsdp(rsdp)?;
    let (root_address, is_xsdt) = if revision >= 2 && roots.xsdt_address != 0
        && roots.xsdt_address < LOW_IDENTITY_LIMIT {
        (roots.xsdt_address, true)
    } else if roots.rsdt_address != 0 {
        (roots.rsdt_address as u64, false)
    } else { return Err(AcpiError::MissingRoot); };
    if root_address.checked_add(36).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
        return Err(AcpiError::AddressOutOfRange);
    }
    let header = unsafe { core::slice::from_raw_parts(root_address as *const u8, 36) };
    let root_len = u32le(header, 4)? as usize;
    if !(36..=MAX_ACPI_TABLE_LENGTH).contains(&root_len)
        || root_address.checked_add(root_len as u64).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
        return Err(AcpiError::BadLength);
    }
    let root = unsafe { core::slice::from_raw_parts(root_address as *const u8, root_len) };
    if !checksum_ok(root) { return Err(AcpiError::BadChecksum); }
    let expected_root: &[u8] = if is_xsdt { b"XSDT" } else { b"RSDT" };
    if root.get(0..4) != Some(expected_root) {
        return Err(AcpiError::BadSignature);
    }
    let width = if is_xsdt { 8usize } else { 4usize };
    if (root_len - 36) % width != 0 { return Err(AcpiError::MalformedEntry); }
    let mut seen = 0usize;
    for offset in (36..root_len).step_by(width) {
        let address = if is_xsdt {
            u64::from_le_bytes(root[offset..offset + 8].try_into().map_err(|_| AcpiError::Truncated)?)
        } else { u32le(root, offset)? as u64 };
        if address == 0 || address.checked_add(36).is_none_or(|end| end > LOW_IDENTITY_LIMIT) { continue; }
        let h = unsafe { core::slice::from_raw_parts(address as *const u8, 36) };
        if h.get(0..4) != Some(&signature[..]) { continue; }
        let length = u32le(h, 4)? as usize;
        if !(36..=MAX_ACPI_TABLE_LENGTH).contains(&length)
            || address.checked_add(length as u64).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
            return Err(AcpiError::BadLength);
        }
        let table = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
        if !checksum_ok(table) { return Err(AcpiError::BadChecksum); }
        if seen == instance { return Ok(table); }
        seen += 1;
    }
    Err(AcpiError::MissingRoot)
}


/// Resolve an ISA IRQ through MADT Interrupt Source Overrides. Without an
/// override, ISA IRQs use the architectural legacy defaults: active-high edge.
pub fn resolve_isa_irq(madt: &MadtInfo, irq: u8) -> (u32, bool, bool) {
    for entry in madt.irq_overrides.iter().take(madt.irq_override_count) {
        if entry.bus == 0 && entry.source_irq == irq {
            return (entry.gsi, entry.active_low, entry.level_triggered);
        }
    }
    (irq as u32, false, false)
}


#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct McfgAllocation {
    pub base_address: u64,
    pub segment: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McfgInfo { pub allocations: [McfgAllocation; 16], pub count: usize }
impl McfgInfo {
    pub const EMPTY: Self = Self { allocations: [McfgAllocation { base_address: 0, segment: 0, start_bus: 0, end_bus: 0 }; 16], count: 0 };
}

/// Parse ACPI MCFG allocations. This describes physical ECAM windows only; the
/// memory manager must establish an uncached device mapping before PCI ECAM reads.
pub fn parse_mcfg(bytes: &[u8]) -> Result<McfgInfo, AcpiError> {
    if bytes.len() < 44 { return Err(AcpiError::Truncated); }
    if bytes.get(0..4) != Some(b"MCFG") { return Err(AcpiError::BadSignature); }
    let length = u32le(bytes, 4)? as usize;
    if length < 44 || length > bytes.len() { return Err(AcpiError::BadLength); }
    if !checksum_ok(&bytes[..length]) { return Err(AcpiError::BadChecksum); }
    if (length - 44) % 16 != 0 { return Err(AcpiError::MalformedEntry); }
    let mut out = McfgInfo::EMPTY;
    for offset in (44..length).step_by(16) {
        if out.count == out.allocations.len() { return Err(AcpiError::TooManyEntries); }
        let base = u64::from_le_bytes(bytes[offset..offset + 8].try_into().map_err(|_| AcpiError::Truncated)?);
        let segment = u16::from_le_bytes([bytes[offset + 8], bytes[offset + 9]]);
        let start_bus = bytes[offset + 10];
        let end_bus = bytes[offset + 11];
        if base == 0 || base & 0xfffff != 0 || end_bus < start_bus {
            return Err(AcpiError::MalformedEntry);
        }
        let span = ((end_bus as u64 - start_bus as u64) + 1).checked_mul(1 << 20).ok_or(AcpiError::BadLength)?;
        base.checked_add(span).ok_or(AcpiError::AddressOutOfRange)?;
        out.allocations[out.count] = McfgAllocation { base_address: base, segment, start_bus, end_bus };
        out.count += 1;
    }
    Ok(out)
}

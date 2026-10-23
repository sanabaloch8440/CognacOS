//! Bounded ACPI table parsing primitives for platform discovery.
//! Callers must pass a readable, mapped byte slice obtained from firmware data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpiError { Truncated, BadSignature, BadLength, BadChecksum, TooManyEntries, MalformedEntry }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuEntry { pub apic_id: u32, pub processor_uid: u32, pub enabled: bool, pub x2apic: bool }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoApicEntry { pub id: u8, pub address: u32, pub gsi_base: u32 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MadtInfo {
    pub local_apic_address: u64,
    pub flags: u32,
    pub cpus: [CpuEntry; 64],
    pub cpu_count: usize,
    pub io_apics: [IoApicEntry; 8],
    pub io_apic_count: usize,
}
impl MadtInfo {
    pub const EMPTY: Self = Self { local_apic_address: 0, flags: 0, cpus: [CpuEntry { apic_id: 0, processor_uid: 0, enabled: false, x2apic: false }; 64], cpu_count: 0, io_apics: [IoApicEntry { id: 0, address: 0, gsi_base: 0 }; 8], io_apic_count: 0 };
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

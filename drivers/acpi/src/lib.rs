//! Bounded ACPI table parsing primitives for platform discovery.
//! Callers must pass a readable, mapped byte slice obtained from firmware data.
#![no_std]

pub mod aml;
pub mod aml_exec;
pub mod aml_objects;


/// Common 36-byte ACPI System Description Table header.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AcpiTableHeader {
    pub signature:[u8;4],pub length:u32,pub revision:u8,
    pub checksum:u8,pub oem_id:[u8;6],pub oem_table_id:[u8;8],
    pub oem_revision:u32,pub creator_id:u32,pub creator_revision:u32,
}
/// ACPI specification family inferred from the RSDP revision and table
/// revision. This is descriptive only; individual tables still require
/// revision/length-aware parsing.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RsdpFormat { V1,V2Plus }
pub fn rsdp_format(revision:u8)->RsdpFormat { if revision==0{RsdpFormat::V1}else{RsdpFormat::V2Plus} }
/// Parse and checksum-validate any complete SDT without assuming its payload.
pub fn parse_sdt_header(bytes:&[u8])->Result<AcpiTableHeader,AcpiError>{
    if bytes.len()<36{return Err(AcpiError::Truncated)}
    let mut signature=[0;4];signature.copy_from_slice(&bytes[0..4]);
    let length=u32le(bytes,4)?;
    if length<36||length as usize>bytes.len(){return Err(AcpiError::BadLength)}
    if !checksum_ok(&bytes[..length as usize]){return Err(AcpiError::BadChecksum)}
    let mut oem_id=[0;6];oem_id.copy_from_slice(&bytes[10..16]);
    let mut oem_table_id=[0;8];oem_table_id.copy_from_slice(&bytes[16..24]);
    Ok(AcpiTableHeader{signature,length,revision:bytes[8],checksum:bytes[9],oem_id,oem_table_id,
        oem_revision:u32le(bytes,24)?,creator_id:u32le(bytes,28)?,creator_revision:u32le(bytes,32)?})
}
/// Return a validated table's exact declared extent and metadata.
pub fn validate_sdt(bytes:&[u8],expected_signature:Option<[u8;4]>)->Result<AcpiTableHeader,AcpiError>{
    let header=parse_sdt_header(bytes)?;
    if expected_signature.is_some_and(|s|s!=header.signature){return Err(AcpiError::BadSignature)}
    Ok(header)
}

/// Revision-aware subset of FADT fields used during x86 platform bring-up.
/// Optional extended pointers are zero when the firmware table predates them.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct FadtInfo {
    pub revision:u8,pub preferred_pm_profile:u8,pub firmware_ctrl:u32,pub dsdt:u32,
    pub boot_arch_flags:u16,pub flags:u32,pub minor_version:u8,
    pub x_firmware_ctrl:u64,pub x_dsdt:u64,
}
impl FadtInfo {
    pub fn effective_firmware_ctrl(&self)->u64{if self.x_firmware_ctrl!=0{self.x_firmware_ctrl}else{self.firmware_ctrl as u64}}
    pub fn effective_dsdt(&self)->u64{if self.x_dsdt!=0{self.x_dsdt}else{self.dsdt as u64}}
}
/// Parse the stable ACPI 5.0+ FADT fields, checking each field against the
/// actual table length. Short legacy FADTs remain valid with absent fields zero.
pub fn parse_fadt(bytes:&[u8])->Result<FadtInfo,AcpiError>{
    let h=validate_sdt(bytes,Some(*b"FACP"))?;
    let n=h.length as usize;
    let mut out=FadtInfo{revision:h.revision,..FadtInfo::default()};
    if n>=46{out.preferred_pm_profile=bytes[45];}
    if n>=44{out.firmware_ctrl=u32le(bytes,36)?;out.dsdt=u32le(bytes,40)?;}
    if n>=111{out.boot_arch_flags=u16::from_le_bytes([bytes[109],bytes[110]]);}
    if n>=116{out.flags=u32le(bytes,112)?;}
    if n>=132{out.minor_version=bytes[131];}
    if n>=148{
        out.x_firmware_ctrl=u64::from_le_bytes(bytes[132..140].try_into().map_err(|_|AcpiError::Truncated)?);
        out.x_dsdt=u64::from_le_bytes(bytes[140..148].try_into().map_err(|_|AcpiError::Truncated)?);
    }
    if out.effective_dsdt()==0{return Err(AcpiError::MalformedEntry)}
    Ok(out)
}

/// Canonical Windows NT ABI declarations shared with ntoskrnl and NTDLL.
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)]
extern crate std;
static LAST_RSDP_ADDRESS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Remember the firmware RSDP discovered during boot for later HAL table queries.
pub fn rsdp_address() -> u64 { LAST_RSDP_ADDRESS.load(core::sync::atomic::Ordering::Acquire) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpiError { Truncated, BadSignature, BadLength, BadChecksum, TooManyEntries, MalformedEntry, MissingRoot, MissingMadt, AddressOutOfRange }

/// Convert ACPI discovery failures to the shared NTSTATUS ABI at the driver boundary.
pub const fn error_to_ntstatus(error: AcpiError) -> nt_abi::ntstatus::NTSTATUS {
    use nt_abi::ntstatus::*;
    match error {
        AcpiError::Truncated => STATUS_BUFFER_TOO_SMALL,
        AcpiError::TooManyEntries => STATUS_INSUFFICIENT_RESOURCES,
        AcpiError::MissingRoot | AcpiError::MissingMadt => STATUS_UNSUCCESSFUL,
        AcpiError::BadSignature | AcpiError::BadLength | AcpiError::BadChecksum
        | AcpiError::MalformedEntry | AcpiError::AddressOutOfRange => STATUS_INVALID_PARAMETER,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuEntry { pub apic_id: u32, pub processor_uid: u32, pub enabled: bool, pub x2apic: bool }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoApicEntry { pub id: u8, pub address: u32, pub gsi_base: u32 }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrqOverride { pub bus: u8, pub source_irq: u8, pub gsi: u32, pub active_low: bool, pub level_triggered: bool }
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct LocalApicNmi { pub processor_uid:u8,pub flags:u16,pub lint:u8 }
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct MadtWakeupMailbox { pub address:u64 }
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct X2ApicNmi { pub processor_uid:u32,pub flags:u16,pub lint:u8 }
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
    pub local_apic_nmis:[LocalApicNmi;16],pub local_apic_nmi_count:usize,
    pub x2apic_nmis:[X2ApicNmi;16],pub x2apic_nmi_count:usize,
    pub wakeup_mailbox:Option<MadtWakeupMailbox>,
}
impl MadtInfo {
    pub const EMPTY: Self = Self { local_apic_address: 0, flags: 0, cpus: [CpuEntry { apic_id: 0, processor_uid: 0, enabled: false, x2apic: false }; 64], cpu_count: 0, io_apics: [IoApicEntry { id: 0, address: 0, gsi_base: 0 }; 8], io_apic_count: 0, irq_overrides: [IrqOverride { bus: 0, source_irq: 0, gsi: 0, active_low: false, level_triggered: false }; 16], irq_override_count: 0, local_apic_nmis:[LocalApicNmi{processor_uid:0,flags:0,lint:0};16],local_apic_nmi_count:0, x2apic_nmis:[X2ApicNmi{processor_uid:0,flags:0,lint:0};16],x2apic_nmi_count:0,wakeup_mailbox:None };
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
            4 if size >= 6 => {
                if out.local_apic_nmi_count==out.local_apic_nmis.len(){return Err(AcpiError::TooManyEntries)}
                let flags=u16::from_le_bytes([e[3],e[4]]);
                if flags&0x000c==0x0008||flags&0x0003==0x0002{return Err(AcpiError::MalformedEntry)}
                out.local_apic_nmis[out.local_apic_nmi_count]=LocalApicNmi{processor_uid:e[2],flags,lint:e[5]};
                out.local_apic_nmi_count+=1;
            }
            5 if size >= 12 => out.local_apic_address = u64::from_le_bytes([e[4],e[5],e[6],e[7],e[8],e[9],e[10],e[11]]),
            9 if size >= 16 => {
                if out.cpu_count == out.cpus.len() { return Err(AcpiError::TooManyEntries); }
                let flags = u32le(e, 8)?;
                out.cpus[out.cpu_count] = CpuEntry { processor_uid: u32le(e, 12)?, apic_id: u32le(e, 4)?, enabled: flags & 3 != 0, x2apic: true };
                out.cpu_count += 1;
            }
            16 if size >= 16 => {
                let address=u64::from_le_bytes(e[4..12].try_into().map_err(|_|AcpiError::Truncated)?);
                if address==0||address&0x3f!=0{return Err(AcpiError::MalformedEntry)}
                out.wakeup_mailbox=Some(MadtWakeupMailbox{address});
            }
            10 if size >= 12 => {
                if out.x2apic_nmi_count==out.x2apic_nmis.len(){return Err(AcpiError::TooManyEntries)}
                let flags=u16::from_le_bytes([e[2],e[3]]);
                if flags&0x000c==0x0008||flags&0x0003==0x0002{return Err(AcpiError::MalformedEntry)}
                out.x2apic_nmis[out.x2apic_nmi_count]=X2ApicNmi{flags,processor_uid:u32le(e,4)?,lint:e[8]};
                out.x2apic_nmi_count+=1;
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
    LAST_RSDP_ADDRESS.store(rsdp_address, core::sync::atomic::Ordering::Release);
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
    if rsdp_address == 0 || rsdp_address.checked_add(20).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
        return Err(AcpiError::AddressOutOfRange);
    }
    let v1 = unsafe { core::slice::from_raw_parts(rsdp_address as *const u8, 20) };
    let revision = v1[15];
    let rsdp_len = if revision >= 2 {
        if rsdp_address.checked_add(36).is_none_or(|end| end > LOW_IDENTITY_LIMIT) {
            return Err(AcpiError::AddressOutOfRange);
        }
        let prefix = unsafe { core::slice::from_raw_parts(rsdp_address as *const u8, 36) };
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



/// Maximum NUMA proximity domains retained from SRAT/SLIT firmware tables.
pub const MAX_NUMA_DOMAINS:usize=64;
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct NumaCpuAffinity { pub proximity_domain:u32,pub apic_id:u32,pub enabled:bool,pub x2apic:bool }
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct NumaMemoryAffinity {
    pub proximity_domain:u32,pub base:u64,pub length:u64,
    pub enabled:bool,pub hot_pluggable:bool,pub non_volatile:bool,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SratInfo {
    pub cpus:[NumaCpuAffinity;128],pub cpu_count:usize,
    pub memory:[NumaMemoryAffinity;128],pub memory_count:usize,
}
impl SratInfo {
    pub const EMPTY:Self=Self{cpus:[NumaCpuAffinity{proximity_domain:0,apic_id:0,enabled:false,x2apic:false};128],cpu_count:0,
        memory:[NumaMemoryAffinity{proximity_domain:0,base:0,length:0,enabled:false,hot_pluggable:false,non_volatile:false};128],memory_count:0};
}
/// Parse ACPI SRAT processor and memory affinity records. Unknown newer
/// architecture-specific records are skipped by their validated record length.
pub fn parse_srat(bytes:&[u8])->Result<SratInfo,AcpiError>{
    if bytes.len()<48{return Err(AcpiError::Truncated)}
    let h=validate_sdt(bytes,Some(*b"SRAT"))?;let end=h.length as usize;
    if end<48{return Err(AcpiError::BadLength)}
    let mut out=SratInfo::EMPTY;let mut off=48;
    while off<end{
        if off+2>end{return Err(AcpiError::MalformedEntry)}
        let kind=bytes[off];let size=bytes[off+1] as usize;
        if size<2||off.checked_add(size).filter(|e|*e<=end).is_none(){return Err(AcpiError::MalformedEntry)}
        let e=&bytes[off..off+size];
        match kind{
            0 if size>=16=>{
                if out.cpu_count==out.cpus.len(){return Err(AcpiError::TooManyEntries)}
                let domain=(e[2] as u32)|((e[9] as u32)<<8)|((e[10] as u32)<<16)|((e[11] as u32)<<24);
                let flags=u32le(e,4)?;
                out.cpus[out.cpu_count]=NumaCpuAffinity{proximity_domain:domain,apic_id:e[3] as u32,enabled:flags&1!=0,x2apic:false};out.cpu_count+=1;
            }
            1 if size>=40=>{
                if out.memory_count==out.memory.len(){return Err(AcpiError::TooManyEntries)}
                let domain=u32le(e,2)?;
                let base=u64::from_le_bytes(e[8..16].try_into().map_err(|_|AcpiError::Truncated)?);
                let length=u64::from_le_bytes(e[16..24].try_into().map_err(|_|AcpiError::Truncated)?);
                let flags=u32le(e,28)?;
                if length==0||base.checked_add(length).is_none(){return Err(AcpiError::MalformedEntry)}
                out.memory[out.memory_count]=NumaMemoryAffinity{proximity_domain:domain,base,length,
                    enabled:flags&1!=0,hot_pluggable:flags&2!=0,non_volatile:flags&4!=0};out.memory_count+=1;
            }
            2 if size>=24=>{
                if out.cpu_count==out.cpus.len(){return Err(AcpiError::TooManyEntries)}
                let domain=u32le(e,4)?;let apic_id=u32le(e,8)?;let flags=u32le(e,12)?;
                out.cpus[out.cpu_count]=NumaCpuAffinity{proximity_domain:domain,apic_id,enabled:flags&1!=0,x2apic:true};out.cpu_count+=1;
            }
            _=>{}
        }
        off+=size;
    }
    Ok(out)
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SlitInfo { pub locality_count:usize,pub distances:[[u8;MAX_NUMA_DOMAINS];MAX_NUMA_DOMAINS] }
impl SlitInfo {
    pub const EMPTY:Self=Self{locality_count:0,distances:[[0;MAX_NUMA_DOMAINS];MAX_NUMA_DOMAINS]};
    pub fn distance(&self,from:usize,to:usize)->Option<u8>{if from<self.locality_count&&to<self.locality_count{Some(self.distances[from][to])}else{None}}
}
/// Parse the SLIT relative-distance matrix. The table is bounded to 64
/// proximity domains; asymmetric firmware matrices are preserved as supplied.
pub fn parse_slit(bytes:&[u8])->Result<SlitInfo,AcpiError>{
    if bytes.len()<44{return Err(AcpiError::Truncated)}
    let h=validate_sdt(bytes,Some(*b"SLIT"))?;let end=h.length as usize;
    let count=u64::from_le_bytes(bytes[36..44].try_into().map_err(|_|AcpiError::Truncated)?);
    if count==0||count as usize>MAX_NUMA_DOMAINS{return Err(AcpiError::TooManyEntries)}
    let n=count as usize;let matrix=n.checked_mul(n).ok_or(AcpiError::BadLength)?;
    if 44usize.checked_add(matrix).filter(|v|*v==end).is_none(){return Err(AcpiError::BadLength)}
    let mut out=SlitInfo::EMPTY;out.locality_count=n;
    for i in 0..n{for j in 0..n{
        let d=bytes[44+i*n+j];
        if i==j&&d!=10{return Err(AcpiError::MalformedEntry)}
        if i!=j&&d<10{return Err(AcpiError::MalformedEntry)}
        out.distances[i][j]=d;
    }}
    Ok(out)
}

/// PCI path element within an ACPI DMAR device scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DmarPath { pub device:u8, pub function:u8 }
/// Parsed DRHD device scope. Paths are bounded and retain the exact PCI
/// device/function chain; callers must not infer coverage from bus number alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmarDeviceScope {
    pub scope_type:u8,pub enumeration_id:u8,pub start_bus:u8,
    pub path:[DmarPath;8],pub path_count:u8,
}
impl DmarDeviceScope {
    pub const EMPTY:Self=Self{scope_type:0,enumeration_id:0,start_bus:0,path:[DmarPath{device:0,function:0};8],path_count:0};
}
/// Intel VT-d remapping hardware discovered from ACPI. A DRHD with include-all
/// covers the segment; otherwise only an explicitly matching endpoint or
/// PCI-subhierarchy device scope authorizes the requester.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmarUnit {
    pub register_base:u64,pub segment:u16,pub include_all:bool,
    pub scopes:[DmarDeviceScope;64],pub scope_count:u8,
}
impl DmarUnit {
    pub const EMPTY:Self=Self{register_base:0,segment:0,include_all:false,scopes:[DmarDeviceScope::EMPTY;64],scope_count:0};
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmarInfo {
    pub host_address_width:u8,pub interrupt_remapping_supported:bool,
    pub units:[DmarUnit;8],pub unit_count:usize,
    /// Parsed RMRRs from this exact, checksum-validated DMAR table.
    pub rmrr:DmarRmrrInfo,
}
impl DmarInfo {
    pub const EMPTY:Self=Self{host_address_width:0,interrupt_remapping_supported:false,
        units:[DmarUnit::EMPTY;8],unit_count:0,rmrr:DmarRmrrInfo::EMPTY};
}
/// Parse Intel VT-d DMAR DRHD structures and their bounded PCI device scopes.
pub fn parse_dmar(bytes:&[u8])->Result<DmarInfo,AcpiError>{
    if bytes.len()<48{return Err(AcpiError::Truncated)}
    if bytes.get(0..4)!=Some(b"DMAR"){return Err(AcpiError::BadSignature)}
    let length=u32le(bytes,4)? as usize;
    if length<48||length>bytes.len(){return Err(AcpiError::BadLength)}
    if !checksum_ok(&bytes[..length]){return Err(AcpiError::BadChecksum)}
    let mut out=DmarInfo::EMPTY;
    out.host_address_width=bytes[36].saturating_add(1);
    out.interrupt_remapping_supported=bytes[37]&1!=0;
    let mut off=48usize;
    while off<length{
        if off.checked_add(4).filter(|e|*e<=length).is_none(){return Err(AcpiError::MalformedEntry)}
        let kind=u16::from_le_bytes([bytes[off],bytes[off+1]]);
        let size=u16::from_le_bytes([bytes[off+2],bytes[off+3]]) as usize;
        if size<4||off.checked_add(size).filter(|e|*e<=length).is_none(){return Err(AcpiError::MalformedEntry)}
        if kind==0{
            if size<16{return Err(AcpiError::MalformedEntry)}
            if out.unit_count==out.units.len(){return Err(AcpiError::TooManyEntries)}
            let base=u64::from_le_bytes(bytes[off+8..off+16].try_into().map_err(|_|AcpiError::Truncated)?);
            if base==0||base&0xfff!=0{return Err(AcpiError::MalformedEntry)}
            let mut unit=DmarUnit::EMPTY;
            unit.register_base=base;unit.include_all=bytes[off+4]&1!=0;
            unit.segment=u16::from_le_bytes([bytes[off+6],bytes[off+7]]);
            let mut cursor=off+16;let unit_end=off+size;
            while cursor<unit_end{
                if cursor.checked_add(6).filter(|e|*e<=unit_end).is_none(){return Err(AcpiError::MalformedEntry)}
                let scope_len=bytes[cursor+1] as usize;
                if scope_len<6||cursor.checked_add(scope_len).filter(|e|*e<=unit_end).is_none()
                    ||(scope_len-6)%2!=0{return Err(AcpiError::MalformedEntry)}
                let path_count=(scope_len-6)/2;
                if path_count==0||path_count>8||unit.scope_count as usize==unit.scopes.len(){return Err(AcpiError::TooManyEntries)}
                let mut scope=DmarDeviceScope::EMPTY;
                scope.scope_type=bytes[cursor];scope.enumeration_id=bytes[cursor+4];scope.start_bus=bytes[cursor+5];
                scope.path_count=path_count as u8;
                for n in 0..path_count{
                    let p=cursor+6+n*2;let device=bytes[p];let function=bytes[p+1];
                    if device>=32||function>=8{return Err(AcpiError::MalformedEntry)}
                    scope.path[n]=DmarPath{device,function};
                }
                unit.scopes[unit.scope_count as usize]=scope;unit.scope_count+=1;
                cursor+=scope_len;
            }
            out.units[out.unit_count]=unit;out.unit_count+=1;
        }
        off+=size;
    }
    out.rmrr=parse_dmar_rmrr(bytes)?;
    Ok(out)
}
/// Determine whether a DRHD explicitly owns a requester. Scope type 1 is a
/// PCI endpoint; type 2 is a PCI sub-hierarchy. Other scope types (IOAPIC,
/// HPET, namespace) do not authorize an arbitrary endpoint.
pub fn dmar_unit_covers_requester(unit:&DmarUnit,segment:u16,bus:u8,device:u8,function:u8)->bool{
    if unit.segment!=segment{return false}
    if unit.include_all{return true}
    unit.scopes.iter().take(unit.scope_count as usize).any(|scope|{
        if scope.scope_type!=1||scope.path_count!=1{return false}
        let first=scope.path[0];
        if scope.start_bus==bus&&first.device==device&&first.function==function{return true}
            // Sub-hierarchy scopes require the full PCI parent path; this early
        // enumerator has no such path, so it deliberately does not guess.
        false
    })
}

#[cfg(test)]
mod dmar_tests {
 use super::*;
 fn finish_checksum(table:&mut [u8]){
  let sum=table.iter().fold(0u8,|a,b|a.wrapping_add(*b));table[9]=table[9].wrapping_sub(sum);
 }
 #[test]fn dmar_rejects_truncated_header_and_bad_signature(){
  assert_eq!(parse_dmar(&[0;47]),Err(AcpiError::Truncated));
  let mut t=[0u8;48];t[0..4].copy_from_slice(b"NOPE");t[4..8].copy_from_slice(&(48u32).to_le_bytes());
  assert_eq!(parse_dmar(&t),Err(AcpiError::BadSignature));
 }
 #[test]fn dmar_parses_include_all_drhd(){
  let mut t=[0u8;64];t[0..4].copy_from_slice(b"DMAR");t[4..8].copy_from_slice(&(64u32).to_le_bytes());t[36]=47;t[37]=1;
  t[48..50].copy_from_slice(&(0u16).to_le_bytes());t[50..52].copy_from_slice(&(16u16).to_le_bytes());t[52]=1;
  t[54..56].copy_from_slice(&(0u16).to_le_bytes());t[56..64].copy_from_slice(&(0xfed9_0000u64).to_le_bytes());
  finish_checksum(&mut t);let info=parse_dmar(&t).unwrap();assert_eq!(info.host_address_width,48);assert!(info.interrupt_remapping_supported);
  assert_eq!(info.unit_count,1);assert_eq!(info.units[0].register_base,0xfed9_0000);assert!(info.units[0].include_all);
 }
 #[test]fn dmar_parses_and_matches_explicit_endpoint_scope(){
  let mut t=[0u8;72];t[0..4].copy_from_slice(b"DMAR");t[4..8].copy_from_slice(&(72u32).to_le_bytes());t[36]=47;
  t[48..50].copy_from_slice(&(0u16).to_le_bytes());t[50..52].copy_from_slice(&(24u16).to_le_bytes());t[52]=0;
  t[54..56].copy_from_slice(&(0u16).to_le_bytes());t[56..64].copy_from_slice(&(0xfed9_0000u64).to_le_bytes());
  t[64]=1;t[65]=8;t[68]=0;t[69]=0;t[70]=3;t[71]=0;
  // The scope's length is 8 bytes: 6-byte header + one PCI path element.
  finish_checksum(&mut t);let info=parse_dmar(&t).unwrap();let unit=&info.units[0];
  assert!(!unit.include_all);assert_eq!(unit.scope_count,1);
  assert!(dmar_unit_covers_requester(unit,0,0,3,0));assert!(!dmar_unit_covers_requester(unit,0,0,4,0));
 }
}

#[cfg(test)]
mod ntstatus_adapter_tests {
    use super::*;
    #[test]
    fn acpi_errors_map_to_shared_ntstatus() {
        use nt_abi::ntstatus::*;
        assert_eq!(error_to_ntstatus(AcpiError::Truncated), STATUS_BUFFER_TOO_SMALL);
        assert_eq!(error_to_ntstatus(AcpiError::TooManyEntries), STATUS_INSUFFICIENT_RESOURCES);
        assert_eq!(error_to_ntstatus(AcpiError::MissingRoot), STATUS_UNSUCCESSFUL);
        assert_eq!(error_to_ntstatus(AcpiError::BadChecksum), STATUS_INVALID_PARAMETER);
    }
}


/// Firmware-reserved memory region (ACPI DMAR RMRR). The limit is inclusive,
/// as encoded by the DMAR table. This is metadata only: the platform must not
/// expose or reclaim the range until every scoped device has been quiesced and
/// firmware ownership has been transferred under the platform's RMRR policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmarRmrr {
    pub base:u64,
    pub limit:u64,
    pub segment:u16,
    pub scopes:[DmarDeviceScope;16],
    pub scope_count:u8,
}
impl DmarRmrr {
    pub const EMPTY:Self=Self{base:0,limit:0,segment:0,scopes:[DmarDeviceScope::EMPTY;16],scope_count:0};
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmarRmrrInfo { pub regions:[DmarRmrr;8], pub count:usize }
impl DmarRmrrInfo {
    pub const EMPTY:Self=Self{regions:[DmarRmrr::EMPTY;8],count:0};
}

/// Parse all RMRR structures in a checksum-validated DMAR table. The returned
/// ranges remain reserved; parsing never changes firmware ownership or maps RAM.
pub fn parse_dmar_rmrr(bytes:&[u8])->Result<DmarRmrrInfo,AcpiError>{
    if bytes.len()<48{return Err(AcpiError::Truncated)}
    if bytes.get(0..4)!=Some(b"DMAR"){return Err(AcpiError::BadSignature)}
    let length=u32le(bytes,4)? as usize;
    if length<48||length>bytes.len(){return Err(AcpiError::BadLength)}
    if !checksum_ok(&bytes[..length]){return Err(AcpiError::BadChecksum)}
    let mut out=DmarRmrrInfo::EMPTY;
    let mut off=48usize;
    while off<length {
        if off.checked_add(4).filter(|e|*e<=length).is_none(){return Err(AcpiError::MalformedEntry)}
        let kind=u16::from_le_bytes([bytes[off],bytes[off+1]]);
        let size=u16::from_le_bytes([bytes[off+2],bytes[off+3]]) as usize;
        if size<4||off.checked_add(size).filter(|e|*e<=length).is_none(){return Err(AcpiError::MalformedEntry)}
        if kind==1 {
            if size<24||out.count==out.regions.len(){return Err(AcpiError::MalformedEntry)}
            let base=u64::from_le_bytes(bytes[off+8..off+16].try_into().map_err(|_|AcpiError::Truncated)?);
            let limit=u64::from_le_bytes(bytes[off+16..off+24].try_into().map_err(|_|AcpiError::Truncated)?);
            if base==0||base&0xfff!=0||limit<base||limit&0xfff!=0x0fff||limit.checked_add(1).is_none(){
                return Err(AcpiError::MalformedEntry)
            }
            let mut region=DmarRmrr::EMPTY;
            region.base=base;region.limit=limit;
            region.segment=u16::from_le_bytes([bytes[off+6],bytes[off+7]]);
            let mut cursor=off+24;
            let end=off+size;
            while cursor<end {
                if cursor.checked_add(6).filter(|e|*e<=end).is_none(){return Err(AcpiError::MalformedEntry)}
                let scope_len=bytes[cursor+1] as usize;
                if scope_len<8||cursor.checked_add(scope_len).filter(|e|*e<=end).is_none()
                    ||(scope_len-6)%2!=0{return Err(AcpiError::MalformedEntry)}
                let count=(scope_len-6)/2;
                if count==0||count>8||region.scope_count as usize==region.scopes.len(){
                    return Err(AcpiError::TooManyEntries)
                }
                let mut scope=DmarDeviceScope::EMPTY;
                scope.scope_type=bytes[cursor];scope.enumeration_id=bytes[cursor+4];
                scope.start_bus=bytes[cursor+5];scope.path_count=count as u8;
                for n in 0..count {
                    let p=cursor+6+n*2;let device=bytes[p];let function=bytes[p+1];
                    if device>=32||function>=8{return Err(AcpiError::MalformedEntry)}
                    scope.path[n]=DmarPath{device,function};
                }
                region.scopes[region.scope_count as usize]=scope;
                region.scope_count+=1;cursor+=scope_len;
            }
            if region.scope_count==0{return Err(AcpiError::MalformedEntry)}
            out.regions[out.count]=region;out.count+=1;
        }
        off+=size;
    }
    Ok(out)
}

/// Match an endpoint's complete PCI path against a DMAR scope. A type-1
/// endpoint scope must match the entire path; a type-2 sub-hierarchy scope
/// matches only when its bridge path is a strict prefix of the endpoint path.
/// The caller supplies paths from the same segment and starting root bus.
pub fn dmar_scope_matches_path(scope:&DmarDeviceScope, endpoint_start_bus:u8,
    endpoint_path:&[DmarPath])->bool {
    let n=scope.path_count as usize;
    if scope.start_bus!=endpoint_start_bus||n==0||n>scope.path.len()||endpoint_path.len()<n{return false}
    if scope.scope_type==1&&endpoint_path.len()!=n{return false}
    if scope.scope_type==2&&endpoint_path.len()<=n{return false}
    if scope.scope_type!=1&&scope.scope_type!=2{return false}
    scope.path.iter().take(n).zip(endpoint_path.iter()).all(|(a,b)|a==b)
}

/// A DMAR DRHD is eligible only for its own segment and a matching full path.
/// Include-all applies to every requester in that segment, not other segments.
pub fn dmar_unit_covers_path(unit:&DmarUnit,segment:u16,endpoint_start_bus:u8,
    endpoint_path:&[DmarPath])->bool {
    unit.segment==segment&&(unit.include_all||unit.scopes.iter().take(unit.scope_count as usize)
        .any(|scope|dmar_scope_matches_path(scope,endpoint_start_bus,endpoint_path)))
}

#[cfg(test)]
mod rmrr_and_scope_tests {
    use super::*;
    fn finish(table:&mut [u8]) {
        let sum=table.iter().fold(0u8,|a,b|a.wrapping_add(*b));
        table[9]=table[9].wrapping_sub(sum);
    }
    #[test]
    fn rmrr_parser_preserves_inclusive_page_aligned_range_and_segment() {
        let mut t=[0u8;80];t[0..4].copy_from_slice(b"DMAR");
        t[4..8].copy_from_slice(&(80u32).to_le_bytes());t[36]=47;
        t[48..50].copy_from_slice(&(1u16).to_le_bytes());
        t[50..52].copy_from_slice(&(32u16).to_le_bytes());
        t[54..56].copy_from_slice(&(2u16).to_le_bytes());
        t[56..64].copy_from_slice(&(0x1000u64).to_le_bytes());
        t[64..72].copy_from_slice(&(0x2fffu64).to_le_bytes());
        t[72]=1;t[73]=8;t[76]=0;t[77]=0;t[78]=4;t[79]=0;
        finish(&mut t);
        let parsed=parse_dmar_rmrr(&t).unwrap();
        assert_eq!(parsed.count,1);
        assert_eq!(parsed.regions[0].base,0x1000);
        assert_eq!(parsed.regions[0].limit,0x2fff);
        assert_eq!(parsed.regions[0].segment,2);
        assert_eq!(parsed.regions[0].scope_count,1);
    }
    #[test]
    fn rmrr_rejects_unaligned_or_reversed_ranges() {
        let mut t=[0u8;80];t[0..4].copy_from_slice(b"DMAR");
        t[4..8].copy_from_slice(&(80u32).to_le_bytes());t[36]=47;
        t[48..50].copy_from_slice(&(1u16).to_le_bytes());
        t[50..52].copy_from_slice(&(32u16).to_le_bytes());
        t[56..64].copy_from_slice(&(0x1001u64).to_le_bytes());
        t[64..72].copy_from_slice(&(0x2fffu64).to_le_bytes());
        t[72]=1;t[73]=8;t[78]=1;finish(&mut t);
        assert_eq!(parse_dmar_rmrr(&t),Err(AcpiError::MalformedEntry));
    }
    #[test]
    fn bridge_subhierarchy_requires_full_matching_prefix_and_segment() {
        let mut scope=DmarDeviceScope::EMPTY;
        scope.scope_type=2;scope.start_bus=0;scope.path_count=1;
        scope.path[0]=DmarPath{device:3,function:0};
        let endpoint=[DmarPath{device:3,function:0},DmarPath{device:4,function:1}];
        assert!(dmar_scope_matches_path(&scope,0,&endpoint));
        assert!(!dmar_scope_matches_path(&scope,0,&endpoint[..1]));
        assert!(!dmar_scope_matches_path(&scope,1,&endpoint));
        assert!(!dmar_scope_matches_path(&scope,0,&[DmarPath{device:3,function:1},endpoint[1]]));
    }
}


/// Resolve the owning DRHD for a requester. An explicit endpoint/sub-hierarchy
/// scope takes precedence over the segment's include-all unit. Duplicate
/// explicit matches or duplicate include-all units are rejected as ambiguous.
pub fn select_dmar_unit<'a>(info:&'a DmarInfo,segment:u16,root_bus:u8,
    endpoint_path:&[DmarPath])->Result<&'a DmarUnit,AcpiError> {
    let mut explicit=None;
    let mut include_all=None;
    for unit in info.units.iter().take(info.unit_count) {
        if unit.segment!=segment {continue}
        if unit.include_all {
            if include_all.is_some(){return Err(AcpiError::MalformedEntry)}
            include_all=Some(unit);
        } else if unit.scopes.iter().take(unit.scope_count as usize)
            .any(|s|dmar_scope_matches_path(s,root_bus,endpoint_path)) {
            if explicit.is_some(){return Err(AcpiError::MalformedEntry)}
            explicit=Some(unit);
        }
    }
    explicit.or(include_all).ok_or(AcpiError::MissingRoot)
}

/// True when any RMRR in the same PCI segment scopes the endpoint. Callers
/// must retain the reserved physical range and must not start DMA unless the
/// platform has installed the required identity mapping after ownership handoff.
pub fn dmar_rmrr_covers_path(info:&DmarRmrrInfo,segment:u16,root_bus:u8,
    endpoint_path:&[DmarPath])->bool {
    info.regions.iter().take(info.count).any(|r|r.segment==segment
        &&r.scopes.iter().take(r.scope_count as usize)
            .any(|s|dmar_scope_matches_path(s,root_bus,endpoint_path)))
}


#[cfg(test)]
mod dmar_selection_tests {
    use super::*;
    #[test]
    fn explicit_drhd_scope_precedes_include_all_in_same_segment() {
        let mut info=DmarInfo::EMPTY;info.unit_count=2;
        info.units[0]=DmarUnit{register_base:0xfee0_0000,segment:0,include_all:true,
            scopes:[DmarDeviceScope::EMPTY;64],scope_count:0};
        let mut scope=DmarDeviceScope::EMPTY;scope.scope_type=1;scope.start_bus=0;
        scope.path_count=1;scope.path[0]=DmarPath{device:5,function:0};
        info.units[1]=DmarUnit{register_base:0xfed9_0000,segment:0,include_all:false,
            scopes:{let mut s=[DmarDeviceScope::EMPTY;64];s[0]=scope;s},scope_count:1};
        let path=[DmarPath{device:5,function:0}];
        assert_eq!(select_dmar_unit(&info,0,0,&path).unwrap().register_base,0xfed9_0000);
        assert_eq!(select_dmar_unit(&info,1,0,&path),Err(AcpiError::MissingRoot));
    }
    #[test]
    fn rmrr_applies_only_to_matching_segment_and_path() {
        let mut info=DmarRmrrInfo::EMPTY;info.count=1;
        let mut scope=DmarDeviceScope::EMPTY;scope.scope_type=1;scope.start_bus=0;
        scope.path_count=1;scope.path[0]=DmarPath{device:5,function:0};
        info.regions[0]=DmarRmrr{base:0x1000,limit:0x2fff,segment:3,
            scopes:{let mut s=[DmarDeviceScope::EMPTY;16];s[0]=scope;s},scope_count:1};
        let path=[DmarPath{device:5,function:0}];
        assert!(dmar_rmrr_covers_path(&info,3,0,&path));
        assert!(!dmar_rmrr_covers_path(&info,0,0,&path));
        assert!(!dmar_rmrr_covers_path(&info,3,0,&[DmarPath{device:5,function:1}]));
    }
}


/// Address-space aperture from an ACPI resource template (typically a
/// PCI root bridge's evaluated _CRS). The range is bus-visible and inclusive
/// in the AML descriptor; consumers must still reserve firmware-owned ranges.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct PciRootAperture {
    pub resource_type:u8, // 0 memory, 1 I/O, 2 bus numbers
    pub minimum:u64,
    pub maximum:u64,
    pub translation:u64,
    pub length:u64,
    pub minimum_fixed:bool,
    pub maximum_fixed:bool,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PciRootResources {
    pub apertures:[PciRootAperture;32],
    pub count:usize,
}
impl PciRootResources {
    pub const EMPTY:Self=Self{apertures:[PciRootAperture{resource_type:0,minimum:0,maximum:0,translation:0,length:0,minimum_fixed:false,maximum_fixed:false};32],count:0};
}
fn read_width(bytes:&[u8],off:usize,width:usize)->Result<u64,AcpiError>{
    let s=bytes.get(off..off.checked_add(width).ok_or(AcpiError::AddressOutOfRange)?).ok_or(AcpiError::Truncated)?;
    let mut v=0u64;
    for (i,b) in s.iter().enumerate(){v|=(*b as u64)<<(i*8);}
    Ok(v)
}

/// Parse the ACPI resource-template descriptors used by PCI root-bridge _CRS.
/// Supports Word/DWord/QWord address-space descriptors, fixed I/O, and
/// fixed 32-bit memory. Unknown descriptors are skipped using their encoded
/// lengths. EndTag is mandatory; malformed or overflowing ranges are rejected.
pub fn parse_pci_root_resources(bytes:&[u8])->Result<PciRootResources,AcpiError>{
    let mut out=PciRootResources::EMPTY;
    let mut off=0usize;
    let mut ended=false;
    while off<bytes.len(){
        let tag=*bytes.get(off).ok_or(AcpiError::Truncated)?;
        if tag&0x80==0 {
            let len=(tag&7) as usize;
            let end=off.checked_add(1+len).ok_or(AcpiError::BadLength)?;
            let d=bytes.get(off+1..end).ok_or(AcpiError::Truncated)?;
            match tag {
                0x79=>{ if len!=1{return Err(AcpiError::MalformedEntry)} ended=true;off=end;break; }
                0x27 if len==7=>{ // I/O port descriptor
                    let minimum=u16::from_le_bytes([d[1],d[2]]) as u64;
                    let maximum=u16::from_le_bytes([d[3],d[4]]) as u64;
                    if maximum<minimum{return Err(AcpiError::MalformedEntry)}
                    push_root_aperture(&mut out,PciRootAperture{resource_type:1,minimum,maximum,translation:0,length:maximum-minimum+1,minimum_fixed:true,maximum_fixed:true})?;
                }
                0x4b if len==3=>{ // Fixed I/O
                    let minimum=u16::from_le_bytes([d[0],d[1]]) as u64;
                    let length=d[2] as u64;
                    if length==0{return Err(AcpiError::MalformedEntry)}
                    let maximum=minimum.checked_add(length-1).ok_or(AcpiError::AddressOutOfRange)?;
                    push_root_aperture(&mut out,PciRootAperture{resource_type:1,minimum,maximum,translation:0,length,minimum_fixed:true,maximum_fixed:true})?;
                }
                _=>{}
            }
            off=end;
            continue;
        }
        let len=u16::from_le_bytes([*bytes.get(off+1).ok_or(AcpiError::Truncated)?,*bytes.get(off+2).ok_or(AcpiError::Truncated)?]) as usize;
        let end=off.checked_add(3).and_then(|v|v.checked_add(len)).ok_or(AcpiError::BadLength)?;
        let d=bytes.get(off+3..end).ok_or(AcpiError::Truncated)?;
        let parsed=match tag {
            0x88 if len>=13=>Some((0usize,1usize,3usize,5usize,7usize,9usize)), // Word address
            0x87 if len>=23=>Some((0usize,1usize,4usize,4usize,8usize,12usize)), // DWord address
            0x8a if len>=43=>Some((0usize,1usize,8usize,4usize,12usize,20usize)), // QWord address
            _=>None,
        };
        if let Some((typ_off,flags_off,width,gran_off,min_off,max_off))=parsed{
            let resource_type=d[typ_off];
            if resource_type<=2 {
                let minimum=read_width(d,min_off,width)?;
                let maximum=read_width(d,max_off,width)?;
                let translation=read_width(d,max_off+2*width,width)?;
                let length=read_width(d,max_off+3*width,width)?;
                if length==0||maximum<minimum{return Err(AcpiError::MalformedEntry)}
                let last=minimum.checked_add(length-1).ok_or(AcpiError::AddressOutOfRange)?;
                if last>maximum{return Err(AcpiError::MalformedEntry)}
                let _granularity=read_width(d,gran_off,width)?;
                let flags=d[flags_off];
                push_root_aperture(&mut out,PciRootAperture{resource_type,minimum,maximum,translation,length,minimum_fixed:flags&0x04!=0,maximum_fixed:flags&0x08!=0})?;
            }
        }
        off=end;
    }
    if !ended{return Err(AcpiError::MalformedEntry)}
    Ok(out)
}
fn push_root_aperture(out:&mut PciRootResources,a:PciRootAperture)->Result<(),AcpiError>{
    if out.count==out.apertures.len(){return Err(AcpiError::TooManyEntries)}
    if a.length==0||a.minimum.checked_add(a.length-1).is_none(){return Err(AcpiError::AddressOutOfRange)}
    out.apertures[out.count]=a;out.count+=1;Ok(())
}


#[cfg(test)]
mod pci_root_crs_tests {
    use super::*;
    use std::vec;
    #[test]
    fn parses_qword_memory_and_io_ranges(){
        // QWord address descriptor: memory, min=0x80000000,
        // max=0x8fffffff, translation=0, length=0x10000000.
        let mut b=vec![0x8a,43,0,
            0,0,0,0,
            0,0,0,0,0,0,0,0,
            0,0,0,0x80,0,0,0,0,
            0xff,0xff,0xff,0x8f,0,0,0,0,
            0,0,0,0,0,0,0,0,
            0,0,0,0,0,0,0,0,
            0x79,0];
        b[1..3].copy_from_slice(&43u16.to_le_bytes());
        b[39..47].copy_from_slice(&(0x1000_0000u64).to_le_bytes());
        let parsed=parse_pci_root_resources(&b).unwrap();
        assert_eq!(parsed.count,1);
        assert_eq!(parsed.apertures[0].resource_type,0);
        assert_eq!(parsed.apertures[0].minimum,0x8000_0000);
        assert_eq!(parsed.apertures[0].maximum,0x8fff_ffff);
        assert_eq!(parsed.apertures[0].length,0x1000_0000);
    }
    #[test]
    fn requires_end_tag_and_rejects_truncated_descriptor(){
        assert_eq!(parse_pci_root_resources(&[0x79]),Err(AcpiError::Truncated));
        assert_eq!(parse_pci_root_resources(&[0x8a,43,0]),Err(AcpiError::Truncated));
    }
}

#[cfg(test)]
mod acpi_5_6_revision_tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;
    fn checksum(table:&mut [u8]) {
        table[9]=0;
        let sum=table.iter().fold(0u8,|a,b|a.wrapping_add(*b));
        table[9]=0u8.wrapping_sub(sum);
    }
    fn header(signature:[u8;4],length:usize,revision:u8)->Vec<u8>{
        let mut b=vec![0u8;length];b[0..4].copy_from_slice(&signature);
        b[4..8].copy_from_slice(&(length as u32).to_le_bytes());b[8]=revision;b
    }
    #[test]
    fn validates_revision_aware_sdt_headers(){
        let mut table=header(*b"TEST",36,2);checksum(&mut table);
        let parsed=parse_sdt_header(&table).unwrap();
        assert_eq!(parsed.signature,*b"TEST");assert_eq!(parsed.revision,2);
        assert_eq!(validate_sdt(&table,Some(*b"TEST")),Ok(parsed));
        assert_eq!(validate_sdt(&table,Some(*b"FACP")),Err(AcpiError::BadSignature));
    }
    #[test]
    fn parses_fadt_extended_dsdt_and_legacy_fallback(){
        let mut table=header(*b"FACP",148,6);
        table[36..40].copy_from_slice(&0x1234u32.to_le_bytes());
        table[40..44].copy_from_slice(&0x5678u32.to_le_bytes());
        table[45]=2;table[109..111].copy_from_slice(&0x0003u16.to_le_bytes());
        table[112..116].copy_from_slice(&0x00000400u32.to_le_bytes());
        table[131]=1;table[140..148].copy_from_slice(&0x1_0000_5678u64.to_le_bytes());
        checksum(&mut table);
        let fadt=parse_fadt(&table).unwrap();
        assert_eq!(fadt.revision,6);assert_eq!(fadt.preferred_pm_profile,2);
        assert_eq!(fadt.effective_dsdt(),0x1_0000_5678);
        assert_eq!(fadt.effective_firmware_ctrl(),0x1234);
        assert_eq!(fadt.flags,0x400);
        let mut legacy=header(*b"FACP",116,3);
        legacy[40..44].copy_from_slice(&0x8765u32.to_le_bytes());checksum(&mut legacy);
        assert_eq!(parse_fadt(&legacy).unwrap().effective_dsdt(),0x8765);
    }
    #[test]
    fn parses_x86_local_apic_and_x2apic_nmi_records(){
        let mut table=header(*b"APIC",62,5);
        table[36..40].copy_from_slice(&0xfee00000u32.to_le_bytes());
        table[40..44].copy_from_slice(&1u32.to_le_bytes());
        table[44..50].copy_from_slice(&[4,6,0xff,0x0f,0,1]);
        table[50..62].copy_from_slice(&[10,12,0x0f,0,0xff,0xff,0xff,0xff,1,0,0,0]);
        checksum(&mut table);
        let madt=parse_madt(&table).unwrap();
        assert_eq!(madt.local_apic_nmi_count,1);
        assert_eq!(madt.local_apic_nmis[0].processor_uid,0xff);
        assert_eq!(madt.x2apic_nmi_count,1);
        assert_eq!(madt.x2apic_nmis[0].processor_uid,u32::MAX);
        assert_eq!(madt.x2apic_nmis[0].lint,1);
    }
    #[test]
    fn parses_acpi_65_multiprocessor_wakeup_mailbox(){
        let mut table=header(*b"APIC",60,6);
        table[36..40].copy_from_slice(&0xfee00000u32.to_le_bytes());
        table[44..60].copy_from_slice(&[16,16,0,0,0x00,0x10,0,0,0,0,0,0,0,0,0,0]);
        checksum(&mut table);
        let madt=parse_madt(&table).unwrap();
        assert_eq!(madt.wakeup_mailbox,Some(MadtWakeupMailbox{address:0x1000}));
    }
    #[test]
    fn rejects_bad_fadt_checksum_and_reserved_nmi_flags(){
        let mut table=header(*b"FACP",148,6);
        table[40..44].copy_from_slice(&1u32.to_le_bytes());checksum(&mut table);
        table[9]^=1;assert_eq!(parse_fadt(&table),Err(AcpiError::BadChecksum));
        let mut madt=header(*b"APIC",50,5);
        madt[36..40].copy_from_slice(&0xfee00000u32.to_le_bytes());
        madt[44..50].copy_from_slice(&[4,6,0,0x02,0,1]);checksum(&mut madt);
        assert_eq!(parse_madt(&madt),Err(AcpiError::MalformedEntry));
    }
}

#[cfg(test)]
mod numa_table_tests {
    use super::*;
    use std::vec;
    fn finish(b:&mut [u8]){b[9]=0;let s=b.iter().fold(0u8,|a,v|a.wrapping_add(*v));b[9]=0u8.wrapping_sub(s);}
    #[test]
    fn parses_srat_x2apic_and_hotplug_nonvolatile_memory(){
        let mut b=vec![0u8;48+24+40];b[0..4].copy_from_slice(b"SRAT");
        b[4..8].copy_from_slice(&(b.len() as u32).to_le_bytes());b[8]=3;
        b[48..72].copy_from_slice(&[2,24,0,0,3,0,0,0,0x44,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0]);
        b[72]=1;b[73]=40;b[74..78].copy_from_slice(&2u32.to_le_bytes());
        b[80..88].copy_from_slice(&0x1_0000_0000u64.to_le_bytes());
        b[88..96].copy_from_slice(&0x2000_0000u64.to_le_bytes());
        b[100..104].copy_from_slice(&7u32.to_le_bytes());finish(&mut b);
        let s=parse_srat(&b).unwrap();
        assert_eq!(s.cpu_count,1);assert_eq!(s.cpus[0].proximity_domain,3);assert!(s.cpus[0].x2apic);
        assert_eq!(s.memory_count,1);assert_eq!(s.memory[0].proximity_domain,2);
        assert!(s.memory[0].enabled&&s.memory[0].hot_pluggable&&s.memory[0].non_volatile);
    }
    #[test]
    fn parses_slit_and_checks_matrix_shape_and_local_diagonal(){
        let mut b=vec![0u8;48];b[0..4].copy_from_slice(b"SLIT");
        b[4..8].copy_from_slice(&(48u32).to_le_bytes());b[8]=1;
        b[36..44].copy_from_slice(&2u64.to_le_bytes());b[44..48].copy_from_slice(&[10,20,25,10]);finish(&mut b);
        let s=parse_slit(&b).unwrap();assert_eq!(s.locality_count,2);
        assert_eq!(s.distance(0,1),Some(20));assert_eq!(s.distance(1,0),Some(25));
        b[44]=9;finish(&mut b);assert_eq!(parse_slit(&b),Err(AcpiError::MalformedEntry));
    }
    #[test]
    fn rejects_srat_memory_range_overflow(){
        let mut b=vec![0u8;88];b[0..4].copy_from_slice(b"SRAT");b[4..8].copy_from_slice(&(88u32).to_le_bytes());
        b[48]=1;b[49]=40;b[80..88].copy_from_slice(&u64::MAX.to_le_bytes());finish(&mut b);
        assert_eq!(parse_srat(&b),Err(AcpiError::MalformedEntry));
    }
}

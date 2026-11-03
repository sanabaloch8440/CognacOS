//! Disk-class metadata and partition discovery primitives.
//!
//! The disk class owns logical-disk identity and geometry. Controller/port
//! drivers remain responsible for physical transport and never parse volumes.
//! This module is transport-independent and uses caller-supplied sector data;
//! it performs no device I/O and allocates no memory.
#![allow(clippy::too_many_arguments)]

use crate::{Geometry, StorageError};

pub const ATA_IDENTIFY_WORDS: usize = 256;
pub const MBR_PARTITION_COUNT: usize = 4;
pub const MAX_PARTITIONS: usize = 128;
pub const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
pub const MBR_SIGNATURE: u16 = 0xaa55;
pub const PARTITION_MBR_TYPE_EXTENDED: u8 = 0x05;
pub const PARTITION_MBR_TYPE_EXTENDED_LBA: u8 = 0x0f;
pub const PARTITION_MBR_TYPE_PROTECTIVE: u8 = 0xee;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskIdentity {
    /// ATA IDENTIFY serial number, decoded from words 10..=19.
    pub serial: [u8; 20],
    /// ATA IDENTIFY firmware revision, decoded from words 23..=26.
    pub firmware: [u8; 8],
    /// ATA IDENTIFY model, decoded from words 27..=46.
    pub model: [u8; 40],
    /// Stable identifier derived from the serial/model strings.
    pub stable_id: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskDescriptor {
    pub identity: DiskIdentity,
    pub geometry: Geometry,
    pub removable: bool,
    pub supports_flush: bool,
}

fn decode_ata_string(words: &[u16], output: &mut [u8]) {
    for (index, word) in words.iter().copied().enumerate() {
        let offset = index * 2;
        if offset < output.len() { output[offset] = (word >> 8) as u8; }
        if offset + 1 < output.len() { output[offset + 1] = word as u8; }
    }
    // ATA strings are space padded. Keep a deterministic, NUL-free identity.
    for byte in output.iter_mut() {
        if *byte == 0 { *byte = b' '; }
    }
    let mut start = 0;
    while start < output.len() && output[start] == b' ' { start += 1; }
    if start != 0 { output.copy_within(start.., 0); output[output.len() - start..].fill(b' '); }
    while output.last() == Some(&b' ') {
        let last = output.len() - 1;
        output[last] = 0;
    }
}

fn hash_identity(serial: &[u8], model: &[u8]) -> u64 {
    // FNV-1a, with a domain separator between serial and model. This is an
    // identifier, not a cryptographic authenticator.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |byte: u8| { hash ^= byte as u64; hash = hash.wrapping_mul(0x0000_0100_0000_01b3); };
    for byte in serial { feed(*byte); }
    feed(0xff);
    for byte in model { feed(*byte); }
    hash
}

/// Decode ATA IDENTIFY DEVICE data into a validated disk descriptor.
/// LBA48 capacity is preferred; LBA28 is used only when LBA48 is unavailable.
/// Logical-sector-size words are honored when the device advertises them.
pub fn parse_ata_identify(words: &[u16; ATA_IDENTIFY_WORDS]) -> Result<DiskDescriptor, StorageError> {
    let lba48 = words[83] & (1 << 10) != 0;
    let sectors = if lba48 {
        (words[100] as u64)
            | ((words[101] as u64) << 16)
            | ((words[102] as u64) << 32)
            | ((words[103] as u64) << 48)
    } else if words[49] & (1 << 9) != 0 {
        words[60] as u64 | ((words[61] as u64) << 16)
    } else {
        return Err(StorageError::Unsupported);
    };
    if sectors == 0 { return Err(StorageError::InvalidArgument); }

    let logical_sector_size = if words[106] & 0xc000 == 0x4000 && words[106] & (1 << 12) != 0 {
        let words_per_sector = words[117] as u32 | ((words[118] as u32) << 16);
        words_per_sector.checked_mul(2).ok_or(StorageError::Overflow)?
    } else { 512 };
    if logical_sector_size < 512 || !logical_sector_size.is_power_of_two() {
        return Err(StorageError::InvalidArgument);
    }
    let physical_sector_size = if words[106] & 0xc000 == 0x4000 && words[106] & (1 << 13) != 0 {
        let exponent = (words[106] >>  exponent_shift()) & 0x0f;
        logical_sector_size.checked_shl(exponent as u32).ok_or(StorageError::Overflow)?
    } else { logical_sector_size };
    let geometry = Geometry { sector_count: sectors, logical_sector_size, physical_sector_size }.validate()?;

    let mut serial = [0u8; 20];
    let mut firmware = [0u8; 8];
    let mut model = [0u8; 40];
    decode_ata_string(&words[10..20], &mut serial);
    decode_ata_string(&words[23..27], &mut firmware);
    decode_ata_string(&words[27..47], &mut model);
    if serial.iter().all(|byte| *byte == 0 || *byte == b' ')
        || model.iter().all(|byte| *byte == 0 || *byte == b' ') {
        return Err(StorageError::InvalidArgument);
    }
    let stable_id = hash_identity(&serial, &model);
    if stable_id == 0 { return Err(StorageError::InvalidState); }
    Ok(DiskDescriptor {
        identity: DiskIdentity { serial, firmware, model, stable_id },
        geometry,
        removable: words[0] & (1 << 7) != 0,
        supports_flush: words[82] & (1 << 12) != 0 || words[83] & (1 << 12) != 0,
    })
}

const fn exponent_shift() -> u16 { 13 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartitionStyle { Mbr, Gpt }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partition {
    pub number: u32,
    pub start_lba: u64,
    pub sector_count: u64,
    pub mbr_type: u8,
    pub type_guid: [u8; 16],
    pub unique_guid: [u8; 16],
    pub bootable: bool,
}

impl Partition {
    pub const EMPTY: Self = Self {
        number: 0, start_lba: 0, sector_count: 0, mbr_type: 0,
        type_guid: [0; 16], unique_guid: [0; 16], bootable: false,
    };
    pub fn end_lba_exclusive(self) -> Result<u64, StorageError> {
        self.start_lba.checked_add(self.sector_count).ok_or(StorageError::Overflow)
    }
    pub fn validate(self, disk: Geometry) -> Result<Self, StorageError> {
        disk.validate()?;
        if self.sector_count == 0 || self.start_lba == 0
            || self.end_lba_exclusive()? > disk.sector_count {
            return Err(StorageError::OutOfRange);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionTable {
    pub style: PartitionStyle,
    pub count: usize,
    pub entries: [Partition; MAX_PARTITIONS],
}
impl PartitionTable {
    pub const fn empty(style: PartitionStyle) -> Self {
        Self { style, count: 0, entries: [Partition::EMPTY; MAX_PARTITIONS] }
    }
    fn push(&mut self, partition: Partition, disk: Geometry) -> Result<(), StorageError> {
        partition.validate(disk)?;
        if self.count == self.entries.len() { return Err(StorageError::QueueFull); }
        let end = partition.end_lba_exclusive()?;
        for old in &self.entries[..self.count] {
            let old_end = old.end_lba_exclusive()?;
            if partition.start_lba < old_end && old.start_lba < end {
                return Err(StorageError::InvalidArgument);
            }
        }
        self.entries[self.count] = partition;
        self.count += 1;
        Ok(())
    }
}

/// Parse the four primary MBR entries. Extended/logical partitions are not
/// silently misrepresented as primary partitions; callers can detect them
/// from the returned MBR type and perform a separate EBR walk.
pub fn parse_mbr(sector: &[u8; 512], disk: Geometry) -> Result<PartitionTable, StorageError> {
    if u16::from_le_bytes([sector[510], sector[511]]) != MBR_SIGNATURE {
        return Err(StorageError::InvalidArgument);
    }
    let mut table = PartitionTable::empty(PartitionStyle::Mbr);
    for index in 0..MBR_PARTITION_COUNT {
        let offset = 446 + index * 16;
        let boot = sector[offset];
        let kind = sector[offset + 4];
        let start = u32::from_le_bytes(sector[offset + 8..offset + 12].try_into().map_err(|_| StorageError::InvalidArgument)?) as u64;
        let count = u32::from_le_bytes(sector[offset + 12..offset + 16].try_into().map_err(|_| StorageError::InvalidArgument)?) as u64;
        if kind == 0 && start == 0 && count == 0 { continue; }
        if boot != 0 && boot != 0x80 { return Err(StorageError::InvalidArgument); }
        if kind == 0 || kind == PARTITION_MBR_TYPE_EXTENDED || kind == PARTITION_MBR_TYPE_EXTENDED_LBA
            || kind == PARTITION_MBR_TYPE_PROTECTIVE { continue; }
        table.push(Partition {
            number: index as u32 + 1, start_lba: start, sector_count: count,
            mbr_type: kind, type_guid: [0; 16], unique_guid: [0; 16],
            bootable: boot == 0x80,
        }, disk)?;
    }
    Ok(table)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 { crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 }; }
    }
    !crc
}

/// Parse a GPT header and its already-read partition-entry array. The caller
/// supplies the header sector and entry bytes from the disk; CRCs and bounds
/// are checked before any partition is published.
pub fn parse_gpt(
    header_sector: &[u8; 512],
    entry_bytes: &[u8],
    disk: Geometry,
) -> Result<PartitionTable, StorageError> {
    if &header_sector[..8] != GPT_SIGNATURE { return Err(StorageError::InvalidArgument); }
    let header_size = u32::from_le_bytes(header_sector[12..16].try_into().map_err(|_| StorageError::InvalidArgument)?) as usize;
    if !(92..=512).contains(&header_size) { return Err(StorageError::InvalidArgument); }
    let expected_header_crc = u32::from_le_bytes(header_sector[16..20].try_into().map_err(|_| StorageError::InvalidArgument)?);
    let mut header = [0u8; 512];
    header[..header_size].copy_from_slice(&header_sector[..header_size]);
    header[16..20].fill(0);
    if crc32(&header[..header_size]) != expected_header_crc { return Err(StorageError::DeviceError); }
    let current_lba = u64::from_le_bytes(header_sector[24..32].try_into().map_err(|_| StorageError::InvalidArgument)?);
    let backup_lba = u64::from_le_bytes(header_sector[32..40].try_into().map_err(|_| StorageError::InvalidArgument)?);
    let first_usable = u64::from_le_bytes(header_sector[40..48].try_into().map_err(|_| StorageError::InvalidArgument)?);
    let last_usable = u64::from_le_bytes(header_sector[48..56].try_into().map_err(|_| StorageError::InvalidArgument)?);
    let entries_lba = u64::from_le_bytes(header_sector[72..80].try_into().map_err(|_| StorageError::InvalidArgument)?);
    let entry_count = u32::from_le_bytes(header_sector[80..84].try_into().map_err(|_| StorageError::InvalidArgument)?) as usize;
    let entry_size = u32::from_le_bytes(header_sector[84..88].try_into().map_err(|_| StorageError::InvalidArgument)?) as usize;
    let expected_entries_crc = u32::from_le_bytes(header_sector[88..92].try_into().map_err(|_| StorageError::InvalidArgument)?);
    if current_lba == backup_lba || current_lba >= disk.sector_count || backup_lba >= disk.sector_count
        || first_usable > last_usable || last_usable >= disk.sector_count
        || entries_lba >= disk.sector_count || entry_count == 0 || entry_count > MAX_PARTITIONS
        || entry_size < 128 || entry_size % 8 != 0 {
        return Err(StorageError::InvalidArgument);
    }
    let required = entry_count.checked_mul(entry_size).ok_or(StorageError::Overflow)?;
    if required > entry_bytes.len() { return Err(StorageError::InvalidArgument); }
    let entries = &entry_bytes[..required];
    if crc32(entries) != expected_entries_crc { return Err(StorageError::DeviceError); }

    let mut table = PartitionTable::empty(PartitionStyle::Gpt);
    for index in 0..entry_count {
        let entry = &entries[index * entry_size..(index + 1) * entry_size];
        let type_guid: [u8; 16] = entry[..16].try_into().map_err(|_| StorageError::InvalidArgument)?;
        if type_guid.iter().all(|byte| *byte == 0) { continue; }
        let unique_guid: [u8; 16] = entry[16..32].try_into().map_err(|_| StorageError::InvalidArgument)?;
        let start = u64::from_le_bytes(entry[32..40].try_into().map_err(|_| StorageError::InvalidArgument)?);
        let end_inclusive = u64::from_le_bytes(entry[40..48].try_into().map_err(|_| StorageError::InvalidArgument)?);
        if start < first_usable || end_inclusive > last_usable || end_inclusive < start
            || unique_guid.iter().all(|byte| *byte == 0) {
            return Err(StorageError::InvalidArgument);
        }
        let count = end_inclusive.checked_sub(start).and_then(|v| v.checked_add(1)).ok_or(StorageError::Overflow)?;
        table.push(Partition {
            number: index as u32 + 1, start_lba: start, sector_count: count,
            mbr_type: 0, type_guid, unique_guid, bootable: false,
        }, disk)?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Geometry {
        Geometry { sector_count: 100_000, logical_sector_size: 512, physical_sector_size: 4096 }
    }

    #[test]
    fn identify_prefers_lba48_and_decodes_stable_strings() {
        let mut words = [0u16; 256];
        words[49] = 1 << 9;
        words[83] = 1 << 10;
        words[100] = 0x5678; words[101] = 0x1234;
        let serial = b"SN123456";
        for (i, pair) in serial.chunks(2).enumerate() {
            words[10 + i] = ((pair[0] as u16) << 8) | pair.get(1).copied().unwrap_or(b' ') as u16;
        }
        let model = b"QEMU HARDDISK";
        for (i, pair) in model.chunks(2).enumerate() {
            words[27 + i] = ((pair[0] as u16) << 8) | pair.get(1).copied().unwrap_or(b' ') as u16;
        }
        let disk = parse_ata_identify(&words).unwrap();
        assert_eq!(disk.geometry.sector_count, 0x1234_5678);
        assert_eq!(&disk.identity.serial[..8], b"SN123456");
        assert_ne!(disk.identity.stable_id, 0);
    }

    #[test]
    fn identify_rejects_missing_stable_serial_or_model() {
        let mut words = [0u16; 256];
        words[49] = 1 << 9; words[60] = 100;
        assert_eq!(parse_ata_identify(&words), Err(StorageError::InvalidArgument));
    }

    #[test]
    fn mbr_parser_checks_signature_bounds_and_overlap() {
        let mut mbr = [0u8; 512];
        mbr[510..512].copy_from_slice(&MBR_SIGNATURE.to_le_bytes());
        mbr[446 + 4] = 0x0c;
        mbr[446 + 8..446 + 12].copy_from_slice(&2048u32.to_le_bytes());
        mbr[446 + 12..446 + 16].copy_from_slice(&4096u32.to_le_bytes());
        let table = parse_mbr(&mbr, geometry()).unwrap();
        assert_eq!(table.count, 1);
        assert_eq!(table.entries[0].start_lba, 2048);
        assert_eq!(table.entries[0].sector_count, 4096);
        mbr[510] = 0;
        assert!(parse_mbr(&mbr, geometry()).is_err());
    }

    #[test]
    fn partition_bounds_reject_out_of_disk_ranges() {
        let mut mbr = [0u8; 512];
        mbr[510..512].copy_from_slice(&MBR_SIGNATURE.to_le_bytes());
        mbr[450] = 0x0c;
        mbr[454..458].copy_from_slice(&99_999u32.to_le_bytes());
        mbr[458..462].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(parse_mbr(&mbr, geometry()), Err(StorageError::OutOfRange));
    }

    #[test]
    fn gpt_parser_checks_header_and_entry_crcs_and_publishes_partition() {
        let mut entries=[0u8;128];
        entries[0..16].copy_from_slice(&[0x28,0x73,0x2a,0xc1,0x1f,0xf8,0xd2,0x11,0xba,0x4b,0x00,0xa0,0xc9,0x3e,0xc9,0x3b]);
        entries[16..32].copy_from_slice(&[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]);
        entries[32..40].copy_from_slice(&2048u64.to_le_bytes());
        entries[40..48].copy_from_slice(&4095u64.to_le_bytes());
        let mut header=[0u8;512];
        header[..8].copy_from_slice(GPT_SIGNATURE);
        header[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        header[12..16].copy_from_slice(&92u32.to_le_bytes());
        header[24..32].copy_from_slice(&1u64.to_le_bytes());
        header[32..40].copy_from_slice(&99_999u64.to_le_bytes());
        header[40..48].copy_from_slice(&34u64.to_le_bytes());
        header[48..56].copy_from_slice(&99_966u64.to_le_bytes());
        header[72..80].copy_from_slice(&2u64.to_le_bytes());
        header[80..84].copy_from_slice(&1u32.to_le_bytes());
        header[84..88].copy_from_slice(&128u32.to_le_bytes());
        header[88..92].copy_from_slice(&crc32(&entries).to_le_bytes());
        header[16..20].fill(0);
        let checksum=crc32(&header[..92]);
        header[16..20].copy_from_slice(&checksum.to_le_bytes());
        let table=parse_gpt(&header,&entries,geometry()).unwrap();
        assert_eq!(table.style,PartitionStyle::Gpt);
        assert_eq!(table.count,1);
        assert_eq!(table.entries[0].start_lba,2048);
        assert_eq!(table.entries[0].sector_count,2048);
        header[16]^=1;
        assert_eq!(parse_gpt(&header,&entries,geometry()),Err(StorageError::DeviceError));
    }

    #[test]
    fn identify_honors_advertised_four_kib_logical_sectors() {
        let mut words=[0u16;256];
        words[49]=1<<9;words[60]=4096;
        words[83]=1<<10;
        words[100]=4096;
        words[106]=0x4000|(1<<12);
        words[117]=2048;
        for (base,text) in [(10,b"SERIAL-0001".as_slice()),(27,b"QEMU 4K DISK".as_slice())] {
            for (index,pair) in text.chunks(2).enumerate() {
                words[base+index]=((pair[0] as u16)<<8)|pair.get(1).copied().unwrap_or(b' ') as u16;
            }
        }
        let disk=parse_ata_identify(&words).unwrap();
        assert_eq!(disk.geometry.logical_sector_size,4096);
        assert_eq!(disk.geometry.physical_sector_size,4096);
    }

    #[test]
    fn partition_table_rejects_overlapping_entries() {
        let mut table = PartitionTable::empty(PartitionStyle::Mbr);
        table.push(Partition { number: 1, start_lba: 10, sector_count: 20, ..Partition::EMPTY }, geometry()).unwrap();
        assert_eq!(table.push(Partition { number: 2, start_lba: 15, sector_count: 20, ..Partition::EMPTY }, geometry()), Err(StorageError::InvalidArgument));
    }
}

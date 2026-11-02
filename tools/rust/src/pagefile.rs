use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const SECTOR: u64 = 512;
const HEADER_SIG: &[u8; 8] = b"EFI PART";
const PAGEFILE_MAGIC: &[u8; 8] = b"COGNACPF";
const PAGEFILE_VERSION: u32 = 1;
const PAGEFILE_HEADER_SIZE: u32 = 4096;
const PAGEFILE_TYPE_GUID_LE: [u8; 16] = [
    0xd1, 0xc8, 0xe4, 0xb7, 0x63, 0x2a, 0x8a, 0x4e,
    0x9f, 0x52, 0x43, 0x4f, 0x47, 0x4e, 0x41, 0x43,
];

struct Header {
    raw: Vec<u8>,
    header_size: usize,
    current_lba: u64,
    backup_lba: u64,
    first_usable: u64,
    last_usable: u64,
    entries_lba: u64,
    entry_count: usize,
    entry_size: usize,
    entries: Vec<u8>,
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn read_at(file: &mut File, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).map_err(|e| format!("short read at byte {offset}: {e}"))?;
    Ok(bytes)
}
fn write_at(file: &mut File, offset: u64, bytes: &[u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| format!("write at byte {offset}: {e}"))
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn parse_header(file: &mut File, sectors: u64, expected_lba: u64) -> Result<Header, String> {
    let raw = read_at(file, expected_lba * SECTOR, SECTOR as usize)?;
    if &raw[..8] != HEADER_SIG { return Err(format!("GPT signature missing at LBA {expected_lba}")); }
    let header_size = u32_at(&raw, 12) as usize;
    if !(92..=SECTOR as usize).contains(&header_size) { return Err("invalid GPT header size".into()); }
    let mut check = raw[..header_size].to_vec();
    let stored_crc = u32_at(&check, 16);
    put_u32(&mut check, 16, 0);
    if crc32(&check) != stored_crc { return Err(format!("GPT header CRC mismatch at LBA {expected_lba}")); }
    let current_lba = u64_at(&raw, 24);
    let backup_lba = u64_at(&raw, 32);
    let first_usable = u64_at(&raw, 40);
    let last_usable = u64_at(&raw, 48);
    let entries_lba = u64_at(&raw, 72);
    let entry_count = u32_at(&raw, 80) as usize;
    let entry_size = u32_at(&raw, 84) as usize;
    let entries_crc = u32_at(&raw, 88);
    if current_lba != expected_lba || current_lba >= sectors || backup_lba >= sectors
        || entry_count < 4 || entry_size < 128 || entry_size % 8 != 0 {
        return Err("invalid GPT geometry".into());
    }
    let array_len = entry_count.checked_mul(entry_size).ok_or("GPT array length overflow")?;
    if array_len > 16 * 1024 * 1024 { return Err("GPT entry array exceeds safety limit".into()); }
    let entries = read_at(file, entries_lba.checked_mul(SECTOR).ok_or("GPT entry offset overflow")?, array_len)?;
    if crc32(&entries) != entries_crc { return Err("GPT partition-entry array CRC mismatch".into()); }
    if first_usable > last_usable || last_usable >= sectors { return Err("invalid GPT usable range".into()); }
    Ok(Header { raw, header_size, current_lba, backup_lba, first_usable, last_usable, entries_lba, entry_count, entry_size, entries })
}
fn seal_header(mut raw: Vec<u8>, header_size: usize) -> Vec<u8> {
    put_u32(&mut raw, 16, 0);
    let crc = crc32(&raw[..header_size]);
    put_u32(&mut raw, 16, crc);
    raw
}
fn unique_guid_bytes_le() -> [u8; 16] {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let mixed = nanos ^ ((std::process::id() as u128) << 64);
    let mut bytes = mixed.to_le_bytes();
    bytes[7] = (bytes[7] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

/// Provision GPT entry 4 in an existing regular disk-image file. This modifies
/// the image in place; it refuses non-files, occupied entry 4, bad CRCs and
/// overlaps. The format is CognacOS-specific and is not Windows pagefile.sys.
pub fn provision(image: &Path, size_mib: u64) -> Result<(), String> {
    if size_mib < 16 { return Err("minimum paging area size is 16 MiB".into()); }
    let metadata = std::fs::metadata(image).map_err(|e| e.to_string())?;
    if !metadata.is_file() { return Err("--image must be a regular disk-image file, not a block device".into()); }
    let mut file = OpenOptions::new().read(true).write(true).open(image).map_err(|e| e.to_string())?;
    let length = metadata.len();
    if length % SECTOR != 0 || length < 8 * 1024 * 1024 { return Err("image must be sector-aligned and at least 8 MiB".into()); }
    let sectors = length / SECTOR;
    let mut primary = parse_header(&mut file, sectors, 1)?;
    let mut backup = parse_header(&mut file, sectors, sectors - 1)?;
    if primary.backup_lba != backup.current_lba || backup.backup_lba != primary.current_lba
        || primary.entry_count != backup.entry_count || primary.entry_size != backup.entry_size {
        return Err("primary and backup GPT headers disagree".into());
    }
    let entry_size = primary.entry_size;
    let offset = 3usize.checked_mul(entry_size).ok_or("entry offset overflow")?;
    if primary.entries[offset..offset + entry_size].iter().any(|b| *b != 0) {
        return Err("GPT partition entry 4 is occupied; refusing to overwrite it".into());
    }
    let needed = size_mib.checked_mul(1024 * 1024).ok_or("requested size overflow")?;
    let count = needed.checked_add(SECTOR - 1).ok_or("sector count overflow")? / SECTOR;
    let alignment = (1024 * 1024) / SECTOR;
    let start = primary.last_usable.checked_add(1).and_then(|v| v.checked_sub(count)).ok_or("not enough free GPT space")? / alignment * alignment;
    let end = start.checked_add(count - 1).ok_or("partition end overflow")?;
    if start < primary.first_usable || end > primary.last_usable {
        return Err("not enough unallocated GPT usable space".into());
    }
    for index in 0..primary.entry_count {
        let at = index * entry_size;
        let entry = &primary.entries[at..at + entry_size];
        if entry.iter().all(|b| *b == 0) { continue; }
        let first = u64_at(entry, 32);
        let last = u64_at(entry, 40);
        if !(end < first || start > last) {
            return Err(format!("new range overlaps existing GPT entry {}", index + 1));
        }
    }
    let mut entry = vec![0u8; entry_size];
    entry[..16].copy_from_slice(&PAGEFILE_TYPE_GUID_LE);
    entry[16..32].copy_from_slice(&unique_guid_bytes_le());
    put_u64(&mut entry, 32, start);
    put_u64(&mut entry, 40, end);
    let name: Vec<u8> = "CognacOS Paging".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let name_len = name.len().min(entry_size.saturating_sub(56));
    entry[56..56 + name_len].copy_from_slice(&name[..name_len]);
    primary.entries[offset..offset + entry_size].copy_from_slice(&entry);
    backup.entries[offset..offset + entry_size].copy_from_slice(&entry);
    let entries_crc = crc32(&primary.entries);
    put_u32(&mut primary.raw, 88, entries_crc);
    put_u32(&mut backup.raw, 88, entries_crc);
    write_at(&mut file, backup.entries_lba * SECTOR, &backup.entries)?;
    write_at(&mut file, primary.entries_lba * SECTOR, &primary.entries)?;
    write_at(&mut file, backup.current_lba * SECTOR, &seal_header(backup.raw, backup.header_size))?;
    write_at(&mut file, primary.current_lba * SECTOR, &seal_header(primary.raw, primary.header_size))?;
    let mut page_header = vec![0u8; PAGEFILE_HEADER_SIZE as usize];
    page_header[..8].copy_from_slice(PAGEFILE_MAGIC);
    put_u32(&mut page_header, 8, PAGEFILE_VERSION);
    put_u32(&mut page_header, 12, PAGEFILE_HEADER_SIZE);
    write_at(&mut file, start * SECTOR, &page_header)?;
    file.flush().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    println!("Provisioned CognacOS paging partition: start_lba={start}, sectors={count}, size_mib={size_mib}");
    println!("This is a CognacOS raw paging area, not a Windows pagefile.sys volume/file.");
    Ok(())
}

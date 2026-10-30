//! Bounded parser for ext4 external xattr blocks.
//!
//! This validates the on-disk header, entry table and value layout. A separate
//! checksum helper authenticates block bytes when given the filesystem seed,
//! physical block number and exclusive-owner identity where required. Neither
//! helper establishes inode ownership or authorizes journal replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XattrError {
    Truncated, BadMagic, InvalidReferenceCount, InvalidBlockCount,
    InvalidEntry, InvalidValue, DuplicateEntry, OverlappingValues,
    InvalidChecksum, MissingOwnerIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalXattrBlockInfo {
    pub reference_count: u32,
    pub hash: u32,
    pub entry_count: usize,
    pub entries_end: usize,
}

#[derive(Clone, Copy)]
struct Entry {
    name_index: u8, name_start: usize, name_len: usize,
    value_start: usize, value_len: usize,
}

fn u16le(block: &[u8], offset: usize) -> Result<u16, XattrError> {
    let end = offset.checked_add(2).ok_or(XattrError::Truncated)?;
    let s = block.get(offset..end).ok_or(XattrError::Truncated)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
fn u32le(block: &[u8], offset: usize) -> Result<u32, XattrError> {
    let end = offset.checked_add(4).ok_or(XattrError::Truncated)?;
    let s = block.get(offset..end).ok_or(XattrError::Truncated)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn entry_at(block: &[u8], offset: usize) -> Result<Entry, XattrError> {
    if offset.checked_add(16).ok_or(XattrError::InvalidEntry)? > block.len() {
        return Err(XattrError::Truncated);
    }
    let name_len = block[offset] as usize;
    let name_index = block[offset + 1];
    let value_offset = u16le(block, offset + 2)? as usize;
    let value_block = u32le(block, offset + 4)?;
    let value_len = usize::try_from(u32le(block, offset + 8)?)
        .map_err(|_| XattrError::InvalidValue)?;
    if name_len == 0 || name_index == 0 || value_block != 0 {
        return Err(XattrError::InvalidEntry);
    }
    let name_start = offset.checked_add(16).ok_or(XattrError::InvalidEntry)?;
    let name_end = name_start.checked_add(name_len).ok_or(XattrError::InvalidEntry)?;
    let padded_end = name_start.checked_add((name_len + 3) & !3)
        .ok_or(XattrError::InvalidEntry)?;
    if padded_end > block.len() || name_end > block.len()
        || block[name_start..name_end].iter().any(|byte| *byte == 0) {
        return Err(XattrError::InvalidEntry);
    }
    if value_len != 0 && (value_offset == 0 || value_offset & 3 != 0) {
        return Err(XattrError::InvalidValue);
    }
    let value_end = value_offset.checked_add(value_len).ok_or(XattrError::InvalidValue)?;
    if value_len != 0 && value_end > block.len() { return Err(XattrError::InvalidValue); }
    Ok(Entry { name_index, name_start, name_len, value_start: value_offset, value_len })
}

/// Validate one ext4 external xattr block's structural layout.
///
/// metadata_csum validation is deliberately separate: ext4 xattr checksums
/// depend on the filesystem checksum seed, physical block number, and (for
/// an exclusively referenced block) the owning inode identity. This parser
/// never treats a structurally valid block as checksum-authenticated.
pub fn validate_external_xattr_block(
    block: &[u8],
) -> Result<ExternalXattrBlockInfo, XattrError> {
    const HEADER_BYTES: usize = 32;
    const XATTR_MAGIC: u32 = 0xea02_0000;
    const MAX_REFCOUNT: u32 = 1024;
    if block.len() < HEADER_BYTES + 4 { return Err(XattrError::Truncated); }
    if u32le(block, 0)? != XATTR_MAGIC { return Err(XattrError::BadMagic); }
    let reference_count = u32le(block, 4)?;
    if reference_count == 0 || reference_count > MAX_REFCOUNT {
        return Err(XattrError::InvalidReferenceCount);
    }
    if u32le(block, 8)? != 1 { return Err(XattrError::InvalidBlockCount); }
    if u32le(block, 20)? != 0 || u32le(block, 24)? != 0 || u32le(block, 28)? != 0 {
        return Err(XattrError::InvalidEntry);
    }

    let mut cursor = HEADER_BYTES;
    let mut count = 0usize;
    loop {
        let end = cursor.checked_add(4).ok_or(XattrError::InvalidEntry)?;
        let first = block.get(cursor..end).ok_or(XattrError::Truncated)?;
        if first.iter().all(|byte| *byte == 0) {
            cursor = end;
            break;
        }
        let entry = entry_at(block, cursor)?;
        cursor = cursor.checked_add(16 + ((entry.name_len + 3) & !3))
            .ok_or(XattrError::InvalidEntry)?;
        if cursor > block.len() { return Err(XattrError::Truncated); }
        count = count.checked_add(1).ok_or(XattrError::InvalidEntry)?;
        if count > block.len() / 16 { return Err(XattrError::InvalidEntry); }
    }
    let entries_end = cursor;

    // Bounded rescans avoid allocating attacker-controlled entry vectors.
    let mut outer = HEADER_BYTES;
    while outer < entries_end - 4 {
        let a = entry_at(block, outer)?;
        let a_next = outer.checked_add(16 + ((a.name_len + 3) & !3))
            .ok_or(XattrError::InvalidEntry)?;
        let mut inner = a_next;
        while inner < entries_end - 4 {
            let b = entry_at(block, inner)?;
            if a.name_index == b.name_index && a.name_len == b.name_len
                && block[a.name_start..a.name_start + a.name_len]
                    == block[b.name_start..b.name_start + b.name_len] {
                return Err(XattrError::DuplicateEntry);
            }
            inner = inner.checked_add(16 + ((b.name_len + 3) & !3))
                .ok_or(XattrError::InvalidEntry)?;
        }
        if a.value_len != 0 {
            if a.value_start < entries_end { return Err(XattrError::InvalidValue); }
            let a_end = a.value_start.checked_add(a.value_len).ok_or(XattrError::InvalidValue)?;
            let mut prior = HEADER_BYTES;
            while prior < outer {
                let p = entry_at(block, prior)?;
                if p.value_len != 0 {
                    let p_end = p.value_start.checked_add(p.value_len).ok_or(XattrError::InvalidValue)?;
                    if a.value_start < p_end && p.value_start < a_end {
                        return Err(XattrError::OverlappingValues);
                    }
                }
                prior = prior.checked_add(16 + ((p.name_len + 3) & !3))
                    .ok_or(XattrError::InvalidEntry)?;
            }
        }
        outer = a_next;
    }

    let mut scan = HEADER_BYTES;
    while scan < entries_end - 4 {
        let entry = entry_at(block, scan)?;
        if entry.value_len != 0 {
            let end = entry.value_start.checked_add(entry.value_len).ok_or(XattrError::InvalidValue)?;
            if entry.value_start < entries_end || end > block.len() {
                return Err(XattrError::InvalidValue);
            }
        }
        scan = scan.checked_add(16 + ((entry.name_len + 3) & !3))
            .ok_or(XattrError::InvalidEntry)?;
    }
    Ok(ExternalXattrBlockInfo {
        reference_count, hash: u32le(block, 12)?, entry_count: count, entries_end,
    })
}



/// Derive the ext4 checksum seed used by external xattr blocks from a
/// checksum-validated superblock image. CSUM_SEED uses s_checksum_seed at
/// offset 0x270; otherwise the seed is CRC32C(~0, s_uuid).
pub fn external_xattr_checksum_seed(raw_superblock: &[u8]) -> Result<u32, XattrError> {
    const SUPERBLOCK_BYTES: usize = 1024;
    const FEATURE_INCOMPAT_CSUM_SEED: u32 = 0x2000;
    const FEATURE_INCOMPAT_OFFSET: usize = 96;
    const UUID_OFFSET: usize = 104;
    const UUID_END: usize = UUID_OFFSET + 16;
    const CHECKSUM_SEED_OFFSET: usize = 0x270;
    if raw_superblock.len() < SUPERBLOCK_BYTES {
        return Err(XattrError::Truncated);
    }
    crate::validate_superblock_checksum(raw_superblock)
        .map_err(|_| XattrError::InvalidChecksum)?;
    let incompat = u32le(raw_superblock, FEATURE_INCOMPAT_OFFSET)?;
    if incompat & FEATURE_INCOMPAT_CSUM_SEED != 0 {
        return u32le(raw_superblock, CHECKSUM_SEED_OFFSET);
    }
    let uuid = raw_superblock.get(UUID_OFFSET..UUID_END).ok_or(XattrError::Truncated)?;
    Ok(crate::checksum::crc32c_update(!0, uuid))
}

/// Validate an external xattr block's metadata_csum checksum.
///
/// `filesystem_seed` is the ext4 checksum seed (explicit s_checksum_seed
/// when CSUM_SEED is enabled, otherwise CRC32C(~0, s_uuid)). `physical_block`
/// is the absolute filesystem block number. Exclusively referenced blocks
/// (h_refcount == 1) additionally bind the owning inode number and generation.
/// Shared blocks bind the physical block number and contents only.
///
/// The checksum field at bytes 16..20 is treated as zero. This authenticates
/// bytes only; callers must separately prove inode ownership and reconcile
/// shared reference counts before authorizing replay of an xattr block.
pub fn validate_external_xattr_checksum(
    block: &[u8],
    filesystem_seed: u32,
    physical_block: u64,
    owner: Option<(u32, u32)>,
) -> Result<(), XattrError> {
    let info = validate_external_xattr_block(block)?;
    if info.reference_count == 1 && owner.is_none() {
        return Err(XattrError::MissingOwnerIdentity);
    }

    let mut crc = crate::checksum::crc32c_update(
        filesystem_seed,
        &physical_block.to_le_bytes(),
    );
    if info.reference_count == 1 {
        let (inode_number, generation) = owner.ok_or(XattrError::MissingOwnerIdentity)?;
        crc = crate::checksum::crc32c_update(crc, &inode_number.to_le_bytes());
        crc = crate::checksum::crc32c_update(crc, &generation.to_le_bytes());
    }
    crc = crate::checksum::crc32c_update(crc, &block[..16]);
    crc = crate::checksum::crc32c_update(crc, &[0; 4]);
    crc = crate::checksum::crc32c_update(crc, &block[20..]);
    if crc != u32le(block, 16)? {
        return Err(XattrError::InvalidChecksum);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn empty_block() -> [u8; 128] {
        let mut b = [0u8; 128];
        b[0..4].copy_from_slice(&0xea02_0000u32.to_le_bytes());
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        b
    }

    #[test]
    fn validates_exclusive_and_shared_block_checksums() {
        let seed = 0x1234_5678;
        let physical = 42u64;
        let owner = (7u32, 9u32);
        let mut exclusive = empty_block();
        let mut crc = crate::checksum::crc32c_update(seed, &physical.to_le_bytes());
        crc = crate::checksum::crc32c_update(crc, &owner.0.to_le_bytes());
        crc = crate::checksum::crc32c_update(crc, &owner.1.to_le_bytes());
        crc = crate::checksum::crc32c_update(crc, &exclusive[..16]);
        crc = crate::checksum::crc32c_update(crc, &[0; 4]);
        crc = crate::checksum::crc32c_update(crc, &exclusive[20..]);
        exclusive[16..20].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(validate_external_xattr_checksum(&exclusive, seed, physical, Some(owner)), Ok(()));
        assert_eq!(validate_external_xattr_checksum(&exclusive, seed, physical, None), Err(XattrError::MissingOwnerIdentity));
        exclusive[40] ^= 1;
        assert_eq!(validate_external_xattr_checksum(&exclusive, seed, physical, Some(owner)), Err(XattrError::InvalidChecksum));

        let mut shared = empty_block();
        shared[4..8].copy_from_slice(&2u32.to_le_bytes());
        let mut crc = crate::checksum::crc32c_update(seed, &physical.to_le_bytes());
        crc = crate::checksum::crc32c_update(crc, &shared[..16]);
        crc = crate::checksum::crc32c_update(crc, &[0; 4]);
        crc = crate::checksum::crc32c_update(crc, &shared[20..]);
        shared[16..20].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(validate_external_xattr_checksum(&shared, seed, physical, None), Ok(()));
    }

    #[test]
    fn accepts_empty_external_block() {
        let info = validate_external_xattr_block(&empty_block()).unwrap();
        assert_eq!(info.entry_count, 0);
        assert_eq!(info.entries_end, 36);
    }
    #[test]
    fn validates_entry_and_value_layout() {
        let mut b = empty_block();
        b[32] = 1; b[33] = 1;
        b[34..36].copy_from_slice(&60u16.to_le_bytes());
        b[40..44].copy_from_slice(&3u32.to_le_bytes());
        b[48] = b'x';
        b[60..63].copy_from_slice(b"abc");
        let info = validate_external_xattr_block(&b).unwrap();
        assert_eq!(info.entry_count, 1);
    }
    #[test]
    fn rejects_bad_magic_and_reference_count() {
        let mut b = empty_block(); b[0] = 0;
        assert_eq!(validate_external_xattr_block(&b), Err(XattrError::BadMagic));
        let mut b = empty_block(); b[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(validate_external_xattr_block(&b), Err(XattrError::InvalidReferenceCount));
    }
    #[test]
    fn rejects_value_inside_entry_table() {
        let mut b = empty_block();
        b[32] = 1; b[33] = 1;
        b[34..36].copy_from_slice(&52u16.to_le_bytes());
        b[40..44].copy_from_slice(&4u32.to_le_bytes());
        b[48] = b'x';
        assert_eq!(validate_external_xattr_block(&b), Err(XattrError::InvalidValue));
    }
}

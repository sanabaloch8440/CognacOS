//! Checksumming primitives shared by ext4 metadata and JBD2.
//! The update routines expose the raw reflected CRC state: callers choose the
//! initial seed and apply final inversion only when the on-disk format requires it.
#[inline]
fn update_reflected(mut crc: u32, bytes: &[u8], polynomial: u32) -> u32 {
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (polynomial & (0u32.wrapping_sub(crc & 1)));
        }
    }
    crc
}

/// IEEE CRC-32 (reflected polynomial 0xedb88320), raw-state update.
pub fn crc32_update(seed: u32, bytes: &[u8]) -> u32 {
    update_reflected(seed, bytes, 0xedb8_8320)
}

/// CRC-32C/Castagnoli (reflected polynomial 0x82f63b78), raw-state update.
pub fn crc32c_update(seed: u32, bytes: &[u8]) -> u32 {
    update_reflected(seed, bytes, 0x82f6_3b78)
}

/// Conventional finalized IEEE CRC-32.
pub fn crc32(bytes: &[u8]) -> u32 {
    !crc32_update(!0, bytes)
}

/// Conventional finalized CRC-32C.
pub fn crc32c(bytes: &[u8]) -> u32 {
    !crc32c_update(!0, bytes)
}

/// JBD2 checksum-v2/v3 seed: CRC32C(~0, journal UUID).
pub fn jbd2_csum_seed(journal_uuid: &[u8; 16]) -> u32 {
    crc32c_update(!0, journal_uuid)
}

/// JBD2 per-tag data checksum: CRC32C(seed, big-endian transaction
/// sequence || complete journaled data block). The tag stores the low
/// 16 bits for checksum-v2 and all 32 bits for checksum-v3.
pub fn jbd2_tag_checksum(seed: u32, sequence: u32, data: &[u8]) -> u32 {
    let sequence = sequence.to_be_bytes();
    crc32c_update(crc32c_update(seed, &sequence), data)
}

/// JBD2 descriptor/revoke tail checksum. The final four bytes are excluded
/// and hold the big-endian result.
pub fn jbd2_metadata_block_checksum(seed: u32, block: &[u8]) -> Option<u32> {
    if block.len() < 4 { return None; }
    let crc = crc32c_update(seed, &block[..block.len() - 4]);
    // JBD2 specifies the tail checksum field as zero during calculation.
    Some(crc32c_update(crc, &[0; 4]))
}

/// Validate a JBD2 descriptor/revoke block's trailing big-endian CRC32C.
pub fn verify_jbd2_metadata_block(seed: u32, block: &[u8]) -> bool {
    let Some(expected) = jbd2_metadata_block_checksum(seed, block) else {
        return false;
    };
    let Some(tail) = block.get(block.len() - 4..) else {
        return false;
    };
    expected == u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]])
}

/// JBD2 commit-block checksum-v2/v3: CRC32C(seed, complete commit block)
/// after the checksum field (bytes 16..20) has been zeroed.
pub fn jbd2_commit_checksum(seed: u32, block: &[u8]) -> Option<u32> {
    if block.len() < 20 { return None; }
    let mut crc = crc32c_update(seed, &block[..16]);
    crc = crc32c_update(crc, &[0; 4]);
    Some(crc32c_update(crc, &block[20..]))
}

/// ext4 inode checksum seed for filesystems without the csum_seed feature:
/// CRC32C(filesystem seed, inode number LE || inode generation LE).
pub fn ext4_inode_csum_seed(filesystem_seed: u32, inode_number: u32, generation: u32) -> u32 {
    let ino = inode_number.to_le_bytes();
    let generation = generation.to_le_bytes();
    crc32c_update(crc32c_update(filesystem_seed, &ino), &generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_crc_vectors() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32c(b"123456789"), 0xe306_9283);
    }

    #[test]
    fn raw_updates_are_streamable() {
        let a = crc32c_update(!0, b"1234");
        assert_eq!(!crc32c_update(a, b"56789"), crc32c(b"123456789"));
    }

    #[test]
    fn jbd2_tag_checksum_binds_sequence_and_payload() {
        let uuid = [0x5a; 16];
        let seed = jbd2_csum_seed(&uuid);
        let a = jbd2_tag_checksum(seed, 7, &[0x33; 1024]);
        assert_ne!(a, jbd2_tag_checksum(seed, 8, &[0x33; 1024]));
        assert_ne!(a, jbd2_tag_checksum(seed, 7, &[0x34; 1024]));
    }

    #[test]
    fn jbd2_metadata_tail_is_checked() {
        let seed = jbd2_csum_seed(&[0x11; 16]);
        let mut block = [0x22; 1024];
        let checksum = jbd2_metadata_block_checksum(seed, &block).unwrap();
        block[1020..].copy_from_slice(&checksum.to_be_bytes());
        assert!(verify_jbd2_metadata_block(seed, &block));
        block[40] ^= 1;
        assert!(!verify_jbd2_metadata_block(seed, &block));
    }

    #[test]
    fn commit_checksum_zeroes_checksum_slot() {
        let seed = jbd2_csum_seed(&[0x77; 16]);
        let mut block = [0x44; 1024];
        let checksum = jbd2_commit_checksum(seed, &block).unwrap();
        block[16..20].copy_from_slice(&checksum.to_be_bytes());
        assert_eq!(jbd2_commit_checksum(seed, &block), Some(checksum));
        block[100] ^= 1;
        assert_ne!(jbd2_commit_checksum(seed, &block), Some(checksum));
    }
}


/// JBD2 v2/v3 superblock checksum: CRC32C(~0, the complete 1024-byte
/// journal superblock with s_checksum at 0xfc..0x100 treated as zero).
/// The checksum is stored in big-endian form without a final complement.
pub fn jbd2_superblock_checksum(block: &[u8]) -> Option<u32> {
    if block.len() != 1024 { return None; }
    let mut crc = crc32c_update(!0, &block[..252]);
    crc = crc32c_update(crc, &[0; 4]);
    Some(crc32c_update(crc, &block[256..]))
}

//! Read-side ext4 HTree (dx) directory index traversal.
//!
//! The caller supplies a Linux-compatible major hash and the per-inode
//! checksum seed. This module validates every index block before consuming
//! its entries and returns a logical directory leaf block. Filename hashing
//! and leaf-name comparison remain the VFS caller's responsibility.
use crate::checksum::crc32c_update;
use crate::Error;

const DX_ROOT_INFO_OFFSET: usize = 24;
const DX_ROOT_ENTRIES_OFFSET: usize = 32;
const DX_NODE_ENTRIES_OFFSET: usize = 8;
const DX_ENTRY_SIZE: usize = 8;
const DX_TAIL_SIZE: usize = 8;
const DX_BLOCK_MASK: u32 = 0x0fff_ffff;
const MAX_DX_INDIRECT_LEVELS: u8 = 1;

#[inline]
fn le16(b: &[u8], o: usize) -> Result<u16, Error> {
    let s = b.get(o..o.checked_add(2).ok_or(Error::OutOfRange)?).ok_or(Error::Truncated)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
#[inline]
fn le32(b: &[u8], o: usize) -> Result<u32, Error> {
    let s = b.get(o..o.checked_add(4).ok_or(Error::OutOfRange)?).ok_or(Error::Truncated)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Supplies directory-file logical blocks. Implementations should map each
/// block through the directory inode's checked extent/legacy mapping path.
pub trait DirectoryBlockReader {
    fn read_directory_block(&mut self, logical_block: u32, out: &mut [u8]) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HtreeLookup {
    /// Logical directory-file block containing the selected leaf.
    pub leaf_block: u32,
    /// Hash version stored in dx_root_info. The caller must use a matching
    /// Linux ext4 hash implementation when computing the query hash.
    pub hash_version: u8,
    /// A following index entry may point to another leaf with the same
    /// major hash. The caller must continue in tree order and compare names
    /// in each candidate leaf before reporting a miss.
    pub possible_hash_collision: bool,
}

/// Verify a dx_root/dx_node checksum exactly as ext4_dx_csum() does:
/// checksum the countlimit header and *used* entries, then dt_reserved and
/// a zero checksum word. The tail is located after limit entries, not after
/// count entries.
pub fn validate_dx_checksum(
    block: &[u8],
    count_offset: usize,
    limit: usize,
    count: usize,
    inode_checksum_seed: u32,
) -> Result<(), Error> {
    if block.len() < DX_TAIL_SIZE || count == 0 || count > limit {
        return Err(Error::InvalidDirectoryEntry);
    }
    let entries_bytes = count.checked_mul(DX_ENTRY_SIZE).ok_or(Error::OutOfRange)?;
    let used_end = count_offset.checked_add(entries_bytes).ok_or(Error::OutOfRange)?;
    let tail_offset = count_offset
        .checked_add(limit.checked_mul(DX_ENTRY_SIZE).ok_or(Error::OutOfRange)?)
        .ok_or(Error::OutOfRange)?;
    let tail_end = tail_offset.checked_add(DX_TAIL_SIZE).ok_or(Error::OutOfRange)?;
    if used_end > tail_offset || tail_end != block.len() {
        return Err(Error::InvalidDirectoryEntry);
    }
    if le32(block, tail_offset)? != 0 {
        return Err(Error::InvalidDirectoryEntry);
    }
    let stored = le32(block, tail_offset + 4)?;
    let mut crc = crc32c_update(inode_checksum_seed, &block[..used_end]);
    crc = crc32c_update(crc, &block[tail_offset..tail_offset + 4]);
    crc = crc32c_update(crc, &[0; 4]);
    if crc == stored { Ok(()) } else { Err(Error::InvalidChecksum) }
}

#[derive(Clone, Copy)]
struct DxMap {
    count: usize,
    limit: usize,
    offset: usize,
}

fn parse_dx_map(
    block: &[u8],
    count_offset: usize,
    metadata_csum: bool,
    inode_checksum_seed: u32,
) -> Result<DxMap, Error> {
    if block.len() < count_offset + 4 {
        return Err(Error::Truncated);
    }
    let limit = le16(block, count_offset)? as usize;
    let count = le16(block, count_offset + 2)? as usize;
    let tail_bytes = if metadata_csum { DX_TAIL_SIZE } else { 0 };
    let available = block.len().checked_sub(count_offset + tail_bytes).ok_or(Error::InvalidDirectoryEntry)?;
    let expected_limit = available / DX_ENTRY_SIZE;
    if limit == 0 || limit != expected_limit || count == 0 || count > limit {
        return Err(Error::InvalidDirectoryEntry);
    }
    if metadata_csum {
        validate_dx_checksum(block, count_offset, limit, count, inode_checksum_seed)?;
    }
    let entries_end = count_offset.checked_add(count.checked_mul(DX_ENTRY_SIZE).ok_or(Error::OutOfRange)?)
        .ok_or(Error::OutOfRange)?;
    if entries_end > block.len() - tail_bytes {
        return Err(Error::InvalidDirectoryEntry);
    }
    let map = DxMap { count, limit, offset: count_offset };
    let mut previous_hash = 0u32;
    for index in 0..count {
        let (hash, child) = dx_entry(block, map, index)?;
        if child == 0 || (index != 0 && hash < previous_hash) {
            return Err(Error::InvalidDirectoryEntry);
        }
        previous_hash = hash;
    }
    Ok(map)
}

fn dx_entry(block: &[u8], map: DxMap, index: usize) -> Result<(u32, u32), Error> {
    if index >= map.count { return Err(Error::OutOfRange); }
    let offset = map.offset.checked_add(index.checked_mul(DX_ENTRY_SIZE).ok_or(Error::OutOfRange)?)
        .ok_or(Error::OutOfRange)?;
    // In ext4's packed representation the first entry's hash is implicit zero;
    // its first four bytes are the countlimit pair and its block is at +4.
    if index == 0 {
        Ok((0, le32(block, offset + 4)? & DX_BLOCK_MASK))
    } else {
        Ok((le32(block, offset)?, le32(block, offset + 4)? & DX_BLOCK_MASK))
    }
}

/// Select the rightmost index entry whose separator hash is <= query_hash.
/// This is the same upper-bound search used by Linux dx_probe().
fn has_hash_collision(block: &[u8], map: DxMap, selected: usize, query_hash: u32) -> Result<bool, Error> {
    if selected + 1 >= map.count { return Ok(false); }
    let (next_hash, _) = dx_entry(block, map, selected + 1)?;
    Ok((next_hash & !1) == query_hash)
}

fn select_dx_entry(block: &[u8], map: DxMap, query_hash: u32) -> Result<(usize, u32), Error> {
    let mut low = 0usize;
    let mut high = map.count;
    while low < high {
        let mid = low + (high - low) / 2;
        let (hash, _) = dx_entry(block, map, mid)?;
        if hash <= query_hash { low = mid + 1; } else { high = mid; }
    }
    let selected = low.saturating_sub(1);
    let (_, child) = dx_entry(block, map, selected)?;
    if child == 0 { return Err(Error::InvalidDirectoryEntry); }
    Ok((selected, child))
}

fn ext4_rec_len(raw: u16, block_size: usize) -> usize {
    if block_size >= 65_536 {
        if raw == 0 || raw == u16::MAX { block_size }
        else { ((raw as usize) & 0xfffc) | (((raw as usize) & 3) << 16) }
    } else {
        raw as usize
    }
}

fn validate_dx_root(block: &[u8]) -> Result<(u8, u8), Error> {
    if block.len() < DX_ROOT_ENTRIES_OFFSET + 8 {
        return Err(Error::Truncated);
    }
    // The root overlays the conventional "." and ".." entries.
    if le32(block, 0)? == 0
        || le16(block, 4)? != 12
        || block[6] != 1
        || block[8] != b'.'
        || le32(block, 12)? == 0
        || ext4_rec_len(le16(block, 16)?, block.len()) != block.len() - 12
        || block[18] != 2
        || &block[20..22] != b".."
    {
        return Err(Error::InvalidDirectoryEntry);
    }
    if le32(block, DX_ROOT_INFO_OFFSET)? != 0 || block[29] != 8 || block[31] != 0 {
        return Err(Error::InvalidDirectoryEntry);
    }
    let hash_version = block[28];
    // 0..=5 are legacy/half-MD4/TEA signed and unsigned variants;
    // 6 is SIPHASH. Hash calculation is intentionally outside this module.
    if hash_version > 6 { return Err(Error::UnsupportedFormat); }
    let levels = block[30];
    if levels > MAX_DX_INDIRECT_LEVELS { return Err(Error::UnsupportedFormat); }
    Ok((hash_version, levels))
}

/// Validate an indexed directory root and each traversed dx_node, then
/// return the logical leaf block selected by a precomputed major hash.
///
/// `root_block` is logical directory block zero. `scratch` must be exactly
/// one filesystem block. When metadata_csum is enabled, `inode_checksum_seed`
/// must be the inode-specific seed (filesystem seed + inode number + generation).
///
/// The returned collision flag is conservative: it is set when the next
/// separator in the final index block has the same major hash after masking
/// its continuation bit. A VFS lookup must scan that adjacent candidate too.
pub fn lookup_htree_leaf<R: DirectoryBlockReader>(
    reader: &mut R,
    root_block: &[u8],
    directory_blocks: u32,
    query_hash: u32,
    metadata_csum: bool,
    inode_checksum_seed: u32,
    scratch: &mut [u8],
) -> Result<HtreeLookup, Error> {
    if root_block.len() < 1024 || scratch.len() != root_block.len() || directory_blocks < 2 {
        return Err(Error::InvalidGeometry);
    }
    if query_hash & 1 != 0 { return Err(Error::InvalidGeometry); }
    let (hash_version, levels) = validate_dx_root(root_block)?;
    let root_map = parse_dx_map(root_block, DX_ROOT_ENTRIES_OFFSET, metadata_csum, inode_checksum_seed)?;
    let (mut selected, mut logical) = select_dx_entry(root_block, root_map, query_hash)?;
    if logical >= directory_blocks { return Err(Error::OutOfRange); }
    let mut collision = has_hash_collision(root_block, root_map, selected, query_hash)?;
    let mut visited = [0u32; 2];
    let mut visited_count = 0usize;

    for _ in 0..levels {
        if logical == 0 || visited[..visited_count].contains(&logical) {
            return Err(Error::InvalidDirectoryEntry);
        }
        visited[visited_count] = logical;
        visited_count += 1;
        reader.read_directory_block(logical, scratch)?;
        // An interior node begins with a fake dirent spanning the whole block.
        if le32(scratch, 0)? != 0
            || ext4_rec_len(le16(scratch, 4)?, scratch.len()) != scratch.len()
        {
            return Err(Error::InvalidDirectoryEntry);
        }
        let node_map = parse_dx_map(scratch, DX_NODE_ENTRIES_OFFSET, metadata_csum, inode_checksum_seed)?;
        (selected, logical) = select_dx_entry(scratch, node_map, query_hash)?;
        if logical >= directory_blocks { return Err(Error::OutOfRange); }
        collision = has_hash_collision(scratch, node_map, selected, query_hash)?;
    }
    Ok(HtreeLookup { leaf_block: logical, hash_version, possible_hash_collision: collision })
}

/// Resolve the index and read the selected leaf into `scratch`, verifying
/// its ordinary directory-block checksum before returning. This is the
/// preferred entry point for metadata_csum indexed-directory lookup; the
/// caller can then run the ordinary dirent name comparison over scratch.
pub fn lookup_htree_leaf_checked<R: DirectoryBlockReader>(
    reader: &mut R,
    root_block: &[u8],
    directory_blocks: u32,
    query_hash: u32,
    metadata_csum: bool,
    inode_checksum_seed: u32,
    scratch: &mut [u8],
) -> Result<HtreeLookup, Error> {
    let lookup = lookup_htree_leaf(
        reader, root_block, directory_blocks, query_hash,
        metadata_csum, inode_checksum_seed, scratch,
    )?;
    reader.read_directory_block(lookup.leaf_block, scratch)?;
    validate_directory_leaf_checksum(scratch, inode_checksum_seed, metadata_csum)?;
    Ok(lookup)
}

/// Verify a conventional ext4 directory leaf's metadata checksum. The
/// 12-byte dirent tail must occupy the final 12 bytes and its checksum covers
/// all preceding bytes, matching ext4_dirblock_csum_verify().
pub fn validate_directory_leaf_checksum(
    block: &[u8],
    inode_checksum_seed: u32,
    metadata_csum: bool,
) -> Result<(), Error> {
    if !metadata_csum { return Ok(()); }
    if block.len() < 12 { return Err(Error::Truncated); }
    let tail = block.len() - 12;
    if le32(block, tail)? != 0
        || le16(block, tail + 4)? as usize != 12
        || block[tail + 6] != 0
        || block[tail + 7] != 0xde
    {
        return Err(Error::InvalidDirectoryEntry);
    }
    let stored = le32(block, tail + 8)?;
    let calculated = crc32c_update(inode_checksum_seed, &block[..tail]);
    if stored == calculated { Ok(()) } else { Err(Error::InvalidChecksum) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checksum::ext4_inode_csum_seed;

    struct Fixture { blocks: [[u8; 1024]; 4] }
    impl DirectoryBlockReader for Fixture {
        fn read_directory_block(&mut self, block: u32, out: &mut [u8]) -> Result<(), Error> {
            let src = self.blocks.get(block as usize).ok_or(Error::OutOfRange)?;
            if out.len() != src.len() { return Err(Error::InvalidGeometry); }
            out.copy_from_slice(src);
            Ok(())
        }
    }

    fn set_dx_checksum(block: &mut [u8], offset: usize, limit: usize, count: usize, seed: u32) {
        let end = offset + count * 8;
        let tail = offset + limit * 8;
        let mut crc = crc32c_update(seed, &block[..end]);
        crc = crc32c_update(crc, &block[tail..tail + 4]);
        crc = crc32c_update(crc, &[0; 4]);
        block[tail + 4..tail + 8].copy_from_slice(&crc.to_le_bytes());
    }

    fn make_root(block: &mut [u8; 1024], seed: u32, metadata_csum: bool) {
        block[0..4].copy_from_slice(&2u32.to_le_bytes());
        block[4..6].copy_from_slice(&12u16.to_le_bytes());
        block[6] = 1; block[8] = b'.';
        block[12..16].copy_from_slice(&2u32.to_le_bytes());
        block[16..18].copy_from_slice(&1012u16.to_le_bytes());
        block[18] = 2; block[20..22].copy_from_slice(b"..");
        block[24..28].copy_from_slice(&0u32.to_le_bytes());
        block[28] = 1; block[29] = 8; block[30] = 0; block[31] = 0;
        let limit = if metadata_csum { (1024 - 32 - 8) / 8 } else { (1024 - 32) / 8 };
        block[32..34].copy_from_slice(&(limit as u16).to_le_bytes());
        block[34..36].copy_from_slice(&2u16.to_le_bytes());
        block[36..40].copy_from_slice(&2u32.to_le_bytes());
        block[40..44].copy_from_slice(&0x4000u32.to_le_bytes());
        block[44..48].copy_from_slice(&3u32.to_le_bytes());
        if metadata_csum { set_dx_checksum(block, 32, limit, 2, seed); }
    }

    #[test]
    fn indexed_root_selects_hash_range_and_checks_checksum() {
        let seed = ext4_inode_csum_seed(0x1234_5678, 2, 7);
        let mut root = [0u8; 1024];
        make_root(&mut root, seed, true);
        let mut fixture = Fixture { blocks: [[0; 1024]; 4] };
        let mut scratch = [0u8; 1024];
        let found = lookup_htree_leaf(&mut fixture, &root, 4, 0x5000, true, seed, &mut scratch).unwrap();
        assert_eq!(found.leaf_block, 3);
        assert_eq!(found.hash_version, 1);
        assert!(!found.possible_hash_collision);
        assert_eq!(lookup_htree_leaf(&mut fixture, &root, 4, 0x1000, true, seed, &mut scratch).unwrap().leaf_block, 2);
        let leaf = &mut fixture.blocks[3];
        leaf[1016..1018].copy_from_slice(&12u16.to_le_bytes());
        leaf[1019] = 0xde;
        let leaf_crc = crc32c_update(seed, &leaf[..1012]);
        leaf[1020..1024].copy_from_slice(&leaf_crc.to_le_bytes());
        assert_eq!(lookup_htree_leaf_checked(&mut fixture, &root, 4, 0x5000, true, seed, &mut scratch).unwrap().leaf_block, 3);
        fixture.blocks[3][100] ^= 1;
        assert_eq!(lookup_htree_leaf_checked(&mut fixture, &root, 4, 0x5000, true, seed, &mut scratch), Err(Error::InvalidChecksum));
        root[40] ^= 1;
        assert_eq!(lookup_htree_leaf(&mut fixture, &root, 4, 0x5000, true, seed, &mut scratch), Err(Error::InvalidChecksum));
    }

    #[test]
    fn indexed_root_traverses_checked_interior_node() {
        let seed = ext4_inode_csum_seed(0x8765_4321, 11, 3);
        let mut root = [0u8; 1024];
        make_root(&mut root, seed, true);
        root[30] = 1;
        root[44..48].copy_from_slice(&1u32.to_le_bytes());
        set_dx_checksum(&mut root, 32, (1024 - 40) / 8, 2, seed);
        let mut fixture = Fixture { blocks: [[0; 1024]; 4] };
        let node = &mut fixture.blocks[1];
        node[0..4].copy_from_slice(&0u32.to_le_bytes());
        node[4..6].copy_from_slice(&1024u16.to_le_bytes());
        let limit = (1024 - 8 - 8) / 8;
        node[8..10].copy_from_slice(&(limit as u16).to_le_bytes());
        node[10..12].copy_from_slice(&2u16.to_le_bytes());
        node[12..16].copy_from_slice(&2u32.to_le_bytes());
        node[16..20].copy_from_slice(&0x4000u32.to_le_bytes());
        node[20..24].copy_from_slice(&3u32.to_le_bytes());
        set_dx_checksum(node, 8, limit, 2, seed);
        let mut scratch = [0u8; 1024];
        let found = lookup_htree_leaf(&mut fixture, &root, 4, 0x5000, true, seed, &mut scratch).unwrap();
        assert_eq!(found.leaf_block, 3);
        fixture.blocks[1][20] ^= 1;
        assert_eq!(lookup_htree_leaf(&mut fixture, &root, 4, 0x5000, true, seed, &mut scratch), Err(Error::InvalidChecksum));
    }

    #[test]
    fn rejects_bad_counts_depth_cycles_and_leaf_checksums() {
        let seed = 0x1234;
        let mut root = [0u8; 1024];
        make_root(&mut root, seed, false);
        root[34..36].copy_from_slice(&0u16.to_le_bytes());
        let mut fixture = Fixture { blocks: [[0; 1024]; 4] };
        let mut scratch = [0u8; 1024];
        assert_eq!(lookup_htree_leaf(&mut fixture, &root, 4, 0x5000, false, seed, &mut scratch), Err(Error::InvalidDirectoryEntry));
        root[34..36].copy_from_slice(&2u16.to_le_bytes());
        root[30] = 2;
        assert_eq!(lookup_htree_leaf(&mut fixture, &root, 4, 0x5000, false, seed, &mut scratch), Err(Error::UnsupportedFormat));

        let mut leaf = [0u8; 1024];
        let tail = 1012;
        leaf[tail + 4..tail + 6].copy_from_slice(&12u16.to_le_bytes());
        leaf[tail + 7] = 0xde;
        let crc = crc32c_update(seed, &leaf[..tail]);
        leaf[tail + 8..tail + 12].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(validate_directory_leaf_checksum(&leaf, seed, true), Ok(()));
        leaf[100] ^= 1;
        assert_eq!(validate_directory_leaf_checksum(&leaf, seed, true), Err(Error::InvalidChecksum));
    }
}

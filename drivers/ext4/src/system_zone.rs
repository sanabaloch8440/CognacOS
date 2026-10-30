//! Bounded ext4 system-zone construction and data-block validation.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRange { pub start: u64, pub end: u64 }

#[derive(Clone, Copy)]
pub struct SystemZoneMap<'a> { ranges: &'a [BlockRange] }
impl<'a> SystemZoneMap<'a> {
    pub fn new(ranges: &'a [BlockRange]) -> Self { Self { ranges } }
    pub fn ranges(&self) -> &'a [BlockRange] { self.ranges }
    pub fn is_system_block(&self, block: u64) -> bool { self.ranges.iter().any(|r| r.start <= block && block < r.end) }
    pub fn validate_data_range(&self, start: u64, count: u64, blocks_count: u64) -> Result<(), Error> {
        if count == 0 { return Err(Error::OutOfRange); }
        let end = start.checked_add(count).ok_or(Error::OutOfRange)?;
        if end > blocks_count { return Err(Error::OutOfRange); }
        if self.ranges.iter().any(|r| start < r.end && r.start < end) { return Err(Error::InvalidExtentTree); }
        Ok(())
    }
}

fn push(ranges: &mut [BlockRange], used: &mut usize, start: u64, count: u64, sb: &Superblock) -> Result<(), Error> {
    if count == 0 { return Ok(()); }
    let end = start.checked_add(count).ok_or(Error::OutOfRange)?;
    if end > sb.blocks_count { return Err(Error::InvalidGeometry); }
    if *used == ranges.len() { return Err(Error::TransactionTooLarge); }
    ranges[*used] = BlockRange { start, end }; *used += 1; Ok(())
}
fn sparse_super(sb: &Superblock) -> bool { sb.feature_compat & 1 != 0 }
fn sparse_super2(sb: &Superblock) -> bool { sb.feature_compat & 0x0200 != 0 }
fn power_of(mut n: u32, base: u32) -> bool {
    if n == 0 { return false; }
    while n % base == 0 { n /= base; }
    n == 1
}
fn group_has_super(sb: &Superblock, raw: &[u8], group: u32) -> Result<bool, Error> {
    if group == 0 { return Ok(true); }
    if sparse_super2(sb) { return Ok(group == u32at(raw, 588)? || group == u32at(raw, 592)?); }
    if sparse_super(sb) { return Ok(group == 1 || power_of(group, 3) || power_of(group, 5) || power_of(group, 7)); }
    Ok(true)
}

/// Build the system-zone map from the primary descriptor table. Caller storage
/// must hold up to six ranges per group plus block zero. META_BG is rejected
/// by the superblock parser; scratch must be exactly one filesystem block.
pub fn build_system_zones<D: ReadOnlyBlockDevice>(
    dev: &mut D, sb: &Superblock, raw_superblock: &[u8],
    scratch: &mut [u8], ranges: &mut [BlockRange],
) -> Result<usize, Error> {
    if raw_superblock.len() < SUPERBLOCK_BYTES || scratch.len() != sb.block_size as usize { return Err(Error::InvalidGeometry); }
    validate_superblock_checksum(raw_superblock)?;
    if sb.feature_incompat & INCOMPAT_META_BG != 0 { return Err(Error::UnsupportedIncompatFeature); }
    let groups = sb.group_count()?;
    let dpb = sb.block_size as u64 / sb.descriptor_size as u64;
    if dpb == 0 { return Err(Error::InvalidGeometry); }
    let gdt_bytes = (groups as u64).checked_mul(sb.descriptor_size as u64).ok_or(Error::OutOfRange)?;
    let gdt_blocks = gdt_bytes.checked_add(sb.block_size as u64 - 1).ok_or(Error::OutOfRange)? / sb.block_size as u64;
    let reserved_gdt = u16at(raw_superblock, 206)? as u64;
    let itable_bytes = (sb.inodes_per_group as u64).checked_mul(sb.inode_size as u64).ok_or(Error::OutOfRange)?;
    let itable_blocks = itable_bytes.checked_add(sb.block_size as u64 - 1).ok_or(Error::OutOfRange)? / sb.block_size as u64;
    let mut used = 0usize;
    push(ranges, &mut used, 0, 1, sb)?;
    for group in 0..groups {
        let group_start = (sb.first_data_block as u64).checked_add((group as u64).checked_mul(sb.blocks_per_group as u64).ok_or(Error::OutOfRange)?).ok_or(Error::OutOfRange)?;
        let group_end = core::cmp::min(group_start.checked_add(sb.blocks_per_group as u64).ok_or(Error::OutOfRange)?, sb.blocks_count);
        if group_start >= group_end { return Err(Error::InvalidGeometry); }
        if group_has_super(sb, raw_superblock, group)? {
            push(ranges, &mut used, group_start, 1, sb)?;
            let gdt_start = group_start.checked_add(1).ok_or(Error::OutOfRange)?;
            push(ranges, &mut used, gdt_start, gdt_blocks, sb)?;
            if !sparse_super2(sb) { push(ranges, &mut used, gdt_start.checked_add(gdt_blocks).ok_or(Error::OutOfRange)?, reserved_gdt, sb)?; }
        }
        let desc_block = (sb.first_data_block as u64).checked_add(1).and_then(|v| v.checked_add(group as u64 / dpb)).ok_or(Error::OutOfRange)?;
        read_block(dev, sb, desc_block, scratch)?;
        let slot = (group as u64 % dpb) as usize;
        let off = slot.checked_mul(sb.descriptor_size as usize).ok_or(Error::OutOfRange)?;
        let desc = scratch.get(off..off + sb.descriptor_size as usize).ok_or(Error::Truncated)?;
        if sb.feature_ro_compat & 0x0400 != 0 { validate_group_descriptor_checksum(raw_superblock, group, desc)?; }
        let gd = GroupDescriptor::parse(desc, sb)?;
        let inode_base = (group as u64).checked_mul(sb.inodes_per_group as u64).ok_or(Error::OutOfRange)?;
        let remaining_inodes = (sb.inodes_count as u64).checked_sub(inode_base).ok_or(Error::InvalidGeometry)?;
        let group_inodes = core::cmp::min(sb.inodes_per_group as u64, remaining_inodes);
        let group_itable_bytes = group_inodes.checked_mul(sb.inode_size as u64).ok_or(Error::OutOfRange)?;
        let actual_itable = group_itable_bytes.checked_add(sb.block_size as u64 - 1).ok_or(Error::OutOfRange)? / sb.block_size as u64;
        if actual_itable > itable_blocks { return Err(Error::InvalidGeometry); }
        push(ranges, &mut used, gd.block_bitmap, 1, sb)?;
        push(ranges, &mut used, gd.inode_bitmap, 1, sb)?;
        push(ranges, &mut used, gd.inode_table, actual_itable, sb)?;
        if gd.block_bitmap == gd.inode_bitmap
            || (gd.inode_table <= gd.block_bitmap && gd.block_bitmap < gd.inode_table.saturating_add(actual_itable))
            || (gd.inode_table <= gd.inode_bitmap && gd.inode_bitmap < gd.inode_table.saturating_add(actual_itable)) {
            return Err(Error::InvalidGeometry);
        }
    }
    Ok(used)
}

pub fn validate_data_extent(map: &SystemZoneMap<'_>, sb: &Superblock, start: u64, count: u64) -> Result<(), Error> {
    map.validate_data_range(start, count, sb.blocks_count)
}

pub fn map_extent_block_checked_zones<D: ReadOnlyBlockDevice>(
    dev: &mut D, sb: &Superblock, raw_superblock: &[u8], inode_number: u32,
    generation: u32, root: &[u8], logical: u32, scratch: &mut [u8],
    zones: &SystemZoneMap<'_>,
) -> Result<Option<u64>, Error> {
    if root.len() < 60 || scratch.len() != sb.block_size as usize { return Err(Error::UnsupportedFormat); }
    validate_superblock_checksum(raw_superblock)?;
    let mut is_root = true;
    let mut expected_depth = None;
    for _ in 0..=MAX_EXTENT_DEPTH {
        let node = if is_root { &root[..60] } else { &scratch[..] };
        let (depth, step) = inspect_extent_node(node, logical, sb, expected_depth)?;
        match step {
            ExtentStep::Found(block) => { validate_data_extent(zones, sb, block, 1)?; return Ok(Some(block)); }
            ExtentStep::ZeroOrHole => return Ok(None),
            ExtentStep::Child(child) => {
                if depth == 0 { return Err(Error::InvalidExtentTree); }
                validate_data_extent(zones, sb, child, 1)?;
                read_block(dev, sb, child, scratch)?;
                if sb.feature_ro_compat & 0x0400 != 0 {
                    validate_extent_block_checksum(raw_superblock, inode_number, generation, scratch)?;
                }
                expected_depth = Some(depth - 1);
                is_root = false;
            }
        }
    }
    Err(Error::InvalidExtentTree)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_validation_rejects_metadata_overlap_and_overflow() {
        let ranges = [BlockRange { start: 10, end: 20 }, BlockRange { start: 40, end: 48 }];
        let map = SystemZoneMap::new(&ranges);
        assert_eq!(map.validate_data_range(20, 4, 100), Ok(()));
        assert_eq!(map.validate_data_range(18, 4, 100), Err(Error::InvalidExtentTree));
        assert_eq!(map.validate_data_range(47, 1, 100), Err(Error::InvalidExtentTree));
        assert_eq!(map.validate_data_range(u64::MAX, 2, u64::MAX), Err(Error::OutOfRange));
        assert_eq!(map.validate_data_range(99, 2, 100), Err(Error::OutOfRange));
    }
    #[test]
    fn sparse_super_group_selection() {
        let mut raw = [0u8; SUPERBLOCK_BYTES];
        raw[92..96].copy_from_slice(&1u32.to_le_bytes());
        let sb = Superblock { inodes_count: 1, blocks_count: 100000, first_data_block: 0, block_size: 4096,
            blocks_per_group: 8192, inodes_per_group: 1024, inode_size: 256, descriptor_size: 32,
            revision: 1, feature_compat: 1, feature_incompat: 0, feature_ro_compat: 0, read_only: true };
        assert_eq!(group_has_super(&sb, &raw, 0), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 1), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 3), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 5), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 7), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 2), Ok(false));
        assert_eq!(group_has_super(&sb, &raw, 9), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 11), Ok(false));
    }
    #[test]
    fn sparse_super2_backup_groups() {
        let mut raw = [0u8; SUPERBLOCK_BYTES];
        raw[588..592].copy_from_slice(&5u32.to_le_bytes());
        raw[592..596].copy_from_slice(&9u32.to_le_bytes());
        let sb = Superblock { inodes_count: 1, blocks_count: 100000, first_data_block: 0, block_size: 4096,
            blocks_per_group: 8192, inodes_per_group: 1024, inode_size: 256, descriptor_size: 32,
            revision: 1, feature_compat: 0x0200, feature_incompat: 0, feature_ro_compat: 0, read_only: true };
        assert_eq!(group_has_super(&sb, &raw, 0), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 5), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 9), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 3), Ok(false));
    }
}

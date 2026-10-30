//! Bounded ext4 system-zone construction and data-block validation.
use super::*;

const MAX_RESERVED_JOURNAL_BLOCKS: u64 = 1_048_576;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRange { pub start: u64, pub end: u64 }
impl BlockRange {
    pub fn overlaps(&self, start: u64, end: u64) -> bool {
        start < self.end && self.start < end
    }
}

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
fn sparse_super(sb: &Superblock) -> bool { sb.feature_ro_compat & 1 != 0 }
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

/// Read-only ext4 mount that owns a validated system-zone view. The caller
/// supplies bounded range storage and scratch buffers so the no_std driver
/// does not allocate during mount. This wrapper deliberately exposes no
/// writable mount or metadata mutation operations.
pub struct CheckedExtFilesystem<'a, D: ReadOnlyBlockDevice> {
    filesystem: ExtFilesystem<D>,
    raw_superblock: [u8; SUPERBLOCK_BYTES],
    ranges: &'a mut [BlockRange],
    zone_count: usize,
}

impl<'a, D: ReadOnlyBlockDevice> CheckedExtFilesystem<'a, D> {
    pub fn mount(
        mut device: D,
        ranges: &'a mut [BlockRange],
        inode_buffer: &mut [u8],
        scratch_a: &mut [u8],
        scratch_b: &mut [u8],
    ) -> Result<Self, Error> {
        let mut raw_superblock = [0u8; SUPERBLOCK_BYTES];
        let end = SUPERBLOCK_DISK_OFFSET.checked_add(SUPERBLOCK_BYTES as u64).ok_or(Error::OutOfRange)?;
        if device.capacity_bytes() < end { return Err(Error::OutOfRange); }
        device.read_at(SUPERBLOCK_DISK_OFFSET, &mut raw_superblock).map_err(|_| Error::Io)?;
        let sb = Superblock::parse(&raw_superblock)?;
        validate_superblock_checksum(&raw_superblock)?;
        if inode_buffer.len() != sb.inode_size as usize
            || scratch_a.len() != sb.block_size as usize
            || scratch_b.len() != sb.block_size as usize {
            return Err(Error::InvalidGeometry);
        }
        let mut filesystem = ExtFilesystem::mount_read_only(device)?;
        let zone_count = {
            let ExtFilesystem { device, superblock, .. } = &mut filesystem;
            let count = build_system_zones(device, superblock, &raw_superblock, scratch_a, ranges)?;
            let mut used = count;
            protect_reserved_journal_inode_zones(
                device, superblock, &raw_superblock, ranges, &mut used,
                inode_buffer, scratch_a, scratch_b,
            )?;
            // Never expose metadata from a filesystem whose internal journal
            // still requires replay. Full recovery/transaction coordination is
            // a separate milestone; until then, dirty or unsupported journals
            // are rejected rather than silently mounted with stale metadata.
            if superblock.feature_compat & 0x0004 != 0 {
                let journal = jbd2::discover_internal_journal(&raw_superblock)
                    .map_err(|_| Error::UnsupportedFormat)?;
                let zones = SystemZoneMap::new(&ranges[..used]);
                let journal_block = if superblock.has_extents() {
                    map_extent_block_checked_zones_for_inode(
                        device, superblock, &raw_superblock, journal.inode_number,
                        u32at(inode_buffer, 100)?, &inode_buffer[40..100],
                        0, scratch_a, &zones, Some(journal.inode_number),
                    )?
                } else {
                    map_legacy_block(device, superblock, &inode_buffer[40..100],
                        0, scratch_a, scratch_b)?
                }.ok_or(Error::InvalidExtentTree)?;
                read_block(device, superblock, journal_block, scratch_b)?;
                let journal_sb = jbd2::JournalSuperblock::parse(scratch_b)
                    .map_err(|_| Error::UnsupportedFormat)?;
                jbd2::validate_journal_uuid(&raw_superblock, scratch_b)
                    .map_err(|_| Error::UnsupportedFormat)?;
                if journal_sb.needs_recovery() { return Err(Error::UnsupportedFormat); }
            }
            used
        };
        Ok(Self { filesystem, raw_superblock, ranges, zone_count })
    }

    pub fn superblock(&self) -> &Superblock { &self.filesystem.superblock }
    pub fn system_zones(&self) -> SystemZoneMap<'_> {
        SystemZoneMap::new(&self.ranges[..self.zone_count])
    }
    pub fn map_extent_block(
        &mut self, inode_number: u32, generation: u32, root: &[u8],
        logical: u32, scratch: &mut [u8],
    ) -> Result<Option<u64>, Error> {
        let zones = SystemZoneMap::new(&self.ranges[..self.zone_count]);
        let journal_ino = if u32at(&self.raw_superblock, 92)? & 0x0004 != 0 {
            Some(u32at(&self.raw_superblock, 224)?)
        } else { None };
        map_extent_block_checked_zones_for_inode(
            &mut self.filesystem.device, &self.filesystem.superblock,
            &self.raw_superblock, inode_number, generation, root, logical,
            scratch, &zones, journal_ino,
        )
    }
    pub fn into_device(self) -> D { self.filesystem.device }
}

/// Checked mount for a clean journal. Because this convenience method has no
/// metadata-ownership manifest, it refuses dirty-journal replay. Call
/// `recover_internal_journal_with_policy` with a validated ownership map first,
/// then retry this mount. All buffers remain caller-owned.
impl<'a, D: BlockDevice> CheckedExtFilesystem<'a, D> {
    pub fn mount_with_recovery(
        mut device: D,
        ranges: &'a mut [BlockRange],
        inode_buffer: &mut [u8],
        scratch_a: &mut [u8],
        scratch_b: &mut [u8],
        journal_map: &mut [u64],
        journal_superblock: &mut [u8],
        replay_scratch: &mut [u8],
        replay_payload: &mut [u8],
    ) -> Result<Self, Error> {
        let mut raw_superblock = [0u8; SUPERBLOCK_BYTES];
        let end = SUPERBLOCK_DISK_OFFSET.checked_add(SUPERBLOCK_BYTES as u64)
            .ok_or(Error::OutOfRange)?;
        if device.capacity_bytes() < end { return Err(Error::OutOfRange); }
        device.read_at(SUPERBLOCK_DISK_OFFSET, &mut raw_superblock)
            .map_err(|_| Error::Io)?;
        let sb = Superblock::parse(&raw_superblock)?;
        validate_superblock_checksum(&raw_superblock)?;
        if inode_buffer.len() != sb.inode_size as usize
            || scratch_a.len() != sb.block_size as usize
            || scratch_b.len() != sb.block_size as usize
            || journal_superblock.len() != sb.block_size as usize
            || replay_scratch.len() != sb.block_size as usize
            || replay_payload.len() != sb.block_size as usize {
            return Err(Error::InvalidGeometry);
        }
        if u32at(&raw_superblock, 92)? & 0x0004 != 0 {
            let used = build_system_zones(
                &mut device, &sb, &raw_superblock, scratch_a, ranges,
            )?;
            let mut used = used;
            protect_reserved_journal_inode_zones(
                &mut device, &sb, &raw_superblock, ranges, &mut used,
                inode_buffer, scratch_a, scratch_b,
            )?;
            let zones = SystemZoneMap::new(&ranges[..used]);
            recover_internal_journal(
                &mut device, &sb, &raw_superblock, &zones, inode_buffer,
                scratch_a, scratch_b, journal_map, journal_superblock,
                replay_scratch, replay_payload,
            )?;
        }
        // mount() rebuilds and retains the validated zone map, and verifies
        // that recovery left the journal clean before returning the handle.
        Self::mount(device, ranges, inode_buffer, scratch_a, scratch_b)
    }
 
    /// Recover a dirty internal journal using a mount-derived, conservative
    /// metadata ownership manifest. The caller provides bounded storage for
    /// exact ownership entries and a trusted list of directory inode numbers.
    /// Primary superblock/GDT, group bitmaps, inode tables, allocated supplied
    /// directory data blocks, and their traversed extent nodes are authorized
    /// only after the corresponding checksum/geometry/ownership checks. Xattr
    /// blocks and undiscovered directories remain unauthorized.
    pub fn mount_with_recovery_and_ownership(
        mut device: D,
        ranges: &'a mut [BlockRange],
        ownership_entries: &'a mut [transaction::MetadataOwnership],
        directory_inodes: &[u32],
        inode_buffer: &mut [u8],
        scratch_a: &mut [u8],
        scratch_b: &mut [u8],
        journal_map: &mut [u64],
        journal_superblock: &mut [u8],
        replay_scratch: &mut [u8],
        replay_payload: &mut [u8],
    ) -> Result<Self, Error> {
        let mut raw_superblock = [0u8; SUPERBLOCK_BYTES];
        let end = SUPERBLOCK_DISK_OFFSET.checked_add(SUPERBLOCK_BYTES as u64)
            .ok_or(Error::OutOfRange)?;
        if device.capacity_bytes() < end { return Err(Error::OutOfRange); }
        device.read_at(SUPERBLOCK_DISK_OFFSET, &mut raw_superblock)
            .map_err(|_| Error::Io)?;
        let sb = Superblock::parse(&raw_superblock)?;
        validate_superblock_checksum(&raw_superblock)?;
        if inode_buffer.len() != sb.inode_size as usize
            || scratch_a.len() != sb.block_size as usize
            || scratch_b.len() != sb.block_size as usize
            || journal_superblock.len() != sb.block_size as usize
            || replay_scratch.len() != sb.block_size as usize
            || replay_payload.len() != sb.block_size as usize {
            return Err(Error::InvalidGeometry);
        }
        if u32at(&raw_superblock, 92)? & 0x0004 != 0 {
            let mut used = build_system_zones(
                &mut device, &sb, &raw_superblock, scratch_a, ranges,
            )?;
            protect_reserved_journal_inode_zones(
                &mut device, &sb, &raw_superblock, ranges, &mut used,
                inode_buffer, scratch_a, scratch_b,
            )?;
            let zones = SystemZoneMap::new(&ranges[..used]);
            let mut owner_count = build_system_metadata_ownership(
                &mut device, &sb, &raw_superblock, &zones, scratch_a,
                ownership_entries,
            )?;
            add_directory_metadata_ownership(
                &mut device, &sb, &raw_superblock, &zones, directory_inodes,
                inode_buffer, scratch_a, scratch_b, ownership_entries, &mut owner_count,
            )?;
            let mut ownership = transaction::MetadataOwnershipMap::new(
                &ownership_entries[..owner_count],
            )?;
            recover_internal_journal_with_policy(
                &mut device, &sb, &raw_superblock, &zones, inode_buffer,
                scratch_a, scratch_b, journal_map, journal_superblock,
                replay_scratch, replay_payload, &mut ownership,
            )?;
        }
        Self::mount(device, ranges, inode_buffer, scratch_a, scratch_b)
    }
}

/// Fail-closed convenience recovery entry point. It verifies an already-clean
/// internal journal, but refuses to replay dirty transactions because this API
/// has no exact metadata-ownership manifest. Use
/// `recover_internal_journal_with_policy` with a validated ownership map to replay.
pub fn recover_internal_journal<D: BlockDevice>(
    dev: &mut D, sb: &Superblock, raw_superblock: &[u8], zones: &SystemZoneMap<'_>,
    inode_buffer: &mut [u8], map_scratch: &mut [u8], legacy_scratch: &mut [u8],
    journal_map: &mut [u64], journal_superblock: &mut [u8],
    replay_scratch: &mut [u8], replay_payload: &mut [u8],
) -> Result<jbd2::ReplayReport, Error> {
    // This convenience entry point has no metadata-ownership manifest.
    // It may confirm an already-clean journal, but it must never replay a
    // committed transaction using filesystem-bounds-only authorization.
    struct RejectUnclassifiedTargets;
    impl jbd2::ReplayTargetPolicy for RejectUnclassifiedTargets {
        fn validate_replay_target(&mut self, _target: u64) -> Result<(), jbd2::Error> {
            Err(jbd2::Error::InvalidTag)
        }
    }
    recover_internal_journal_with_policy(
        dev, sb, raw_superblock, zones, inode_buffer, map_scratch, legacy_scratch,
        journal_map, journal_superblock, replay_scratch, replay_payload,
        &mut RejectUnclassifiedTargets,
    )
}

/// Recovery entry point with an exact metadata-owner allowlist. The caller's
/// policy is evaluated for every committed, non-revoked home-block target;
/// journal self-overwrite is rejected independently before the callback.
pub fn recover_internal_journal_with_policy<D: BlockDevice, P: jbd2::ReplayTargetPolicy>(
    dev: &mut D, sb: &Superblock, raw_superblock: &[u8], zones: &SystemZoneMap<'_>,
    inode_buffer: &mut [u8], map_scratch: &mut [u8], legacy_scratch: &mut [u8],
    journal_map: &mut [u64], journal_superblock: &mut [u8],
    replay_scratch: &mut [u8], replay_payload: &mut [u8], policy: &mut P,
) -> Result<jbd2::ReplayReport, Error> {
    let info = jbd2::discover_internal_journal(raw_superblock)
        .map_err(|_| Error::UnsupportedFormat)?;
    if inode_buffer.len() != sb.inode_size as usize
        || map_scratch.len() != sb.block_size as usize
        || legacy_scratch.len() != sb.block_size as usize
        || journal_superblock.len() != sb.block_size as usize
        || replay_scratch.len() != sb.block_size as usize
        || replay_payload.len() != sb.block_size as usize {
        return Err(Error::InvalidGeometry);
    }
    read_inode(dev, sb, raw_superblock, info.inode_number, inode_buffer, map_scratch)?;
    let inode = Inode::parse(inode_buffer, sb.inode_size, sb.revision, sb.feature_ro_compat)?;
    if !inode.is_regular_file || inode.size < sb.block_size as u64 {
        return Err(Error::InvalidGeometry);
    }
    let journal_blocks = inode.size.checked_add(sb.block_size as u64 - 1)
        .ok_or(Error::OutOfRange)? / sb.block_size as u64;
    if journal_blocks > journal_map.len() as u64 || journal_blocks > MAX_RESERVED_JOURNAL_BLOCKS {
        return Err(Error::TransactionTooLarge);
    }
    let generation = u32at(inode_buffer, 100)?;
    for logical in 0..journal_blocks as u32 {
        let physical = if sb.has_extents() {
            map_extent_block_checked_zones_for_inode(
                dev, sb, raw_superblock, info.inode_number, generation,
                &inode_buffer[40..100], logical, map_scratch, zones,
                Some(info.inode_number),
            )?
        } else {
            map_legacy_block(dev, sb, &inode_buffer[40..100], logical as u64,
                map_scratch, legacy_scratch)?
        }.ok_or(Error::InvalidExtentTree)?;
        if physical >= sb.blocks_count { return Err(Error::OutOfRange); }
        journal_map[logical as usize] = physical;
    }
    read_block(dev, sb, journal_map[0], journal_superblock)?;
    jbd2::validate_journal_uuid(raw_superblock, journal_superblock)
        .map_err(|_| Error::UnsupportedFormat)?;
    let parsed = jbd2::JournalSuperblock::parse(journal_superblock)
        .map_err(|_| Error::UnsupportedFormat)?;
    if parsed.block_size != sb.block_size || parsed.max_len as u64 > journal_blocks {
        return Err(Error::UnsupportedFormat);
    }
    if !parsed.needs_recovery() { return Ok(jbd2::ReplayReport::default()); }

    struct Adapter<'a, D: BlockDevice> { device: &'a mut D, sb: Superblock }
    impl<D: BlockDevice> jbd2::ReplayDevice for Adapter<'_, D> {
        fn filesystem_blocks(&self) -> u64 { self.sb.blocks_count }
        fn read_block(&mut self, block: u64, out: &mut [u8]) -> Result<(), ()> {
            super::read_block(self.device, &self.sb, block, out).map_err(|_| ())
        }
        fn write_block(&mut self, block: u64, data: &[u8]) -> Result<(), ()> {
            if block >= self.sb.blocks_count || data.len() != self.sb.block_size as usize { return Err(()); }
            let offset = block.checked_mul(self.sb.block_size as u64).ok_or(())?;
            let end = offset.checked_add(data.len() as u64).ok_or(())?;
            let capacity = self.device.capacity_bytes();
            if end > capacity { return Err(()); }
            self.device.write_at(offset, data).map_err(|_| ())
        }
        fn flush(&mut self) -> Result<(), ()> { self.device.flush().map_err(|_| ()) }
    }
    struct ReplayTargets<'a, P> { journal_blocks: &'a [u64], policy: &'a mut P }
    impl<P: jbd2::ReplayTargetPolicy> jbd2::ReplayTargetPolicy for ReplayTargets<'_, P> {
        fn validate_replay_target(&mut self, target: u64) -> Result<(), jbd2::Error> {
            if self.journal_blocks.contains(&target) { return Err(jbd2::Error::InvalidTag); }
            self.policy.validate_replay_target(target)
        }
    }
    let mut adapter = Adapter { device: dev, sb: *sb };
    let mut map = |logical: u32| journal_map.get(logical as usize).copied().ok_or(());
    let mut targets = ReplayTargets { journal_blocks: &journal_map[..journal_blocks as usize], policy };
    jbd2::recover_with_policy(
        &mut adapter, &mut map, journal_superblock, replay_scratch, replay_payload, &mut targets,
    ).map_err(|_| Error::UnsupportedFormat)

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
            // For block sizes above 1 KiB, the primary superblock lives at
            // byte offset 1024 inside block zero, already reserved above.
            if group_start != 0 { push(ranges, &mut used, group_start, 1, sb)?; }
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
    // ext4 metadata objects must not alias one another. This catches corrupt
    // descriptors that point a bitmap/table into a backup superblock, GDT, or
    // another group's metadata before the map can authorize data writes.
    ranges[..used].sort_unstable_by_key(|r| r.start);
    for i in 1..used {
        if ranges[i - 1].end > ranges[i].start {
            return Err(Error::InvalidGeometry);
        }
    }
    Ok(used)
}

/// Build a conservative replay-authorization manifest for metadata whose
/// ownership is proven directly by the validated superblock and primary group
/// descriptor table. The caller supplies fixed-capacity storage; no allocation
/// is performed. Backup superblocks/GDTs and inode-owned directory/extent/xattr
/// blocks are intentionally omitted, so replay to those targets remains denied
/// until a higher-level object scanner validates their exact ownership.
pub fn build_system_metadata_ownership<D: ReadOnlyBlockDevice>(
    dev: &mut D,
    sb: &Superblock,
    raw_superblock: &[u8],
    zones: &SystemZoneMap<'_>,
    scratch: &mut [u8],
    entries: &mut [transaction::MetadataOwnership],
) -> Result<usize, Error> {
    use transaction::{MetadataObjectKind as Kind, MetadataOwnership};
    if raw_superblock.len() < SUPERBLOCK_BYTES || scratch.len() != sb.block_size as usize {
        return Err(Error::InvalidGeometry);
    }
    validate_superblock_checksum(raw_superblock)?;
    if sb.feature_incompat & INCOMPAT_META_BG != 0 { return Err(Error::UnsupportedIncompatFeature); }
    let groups = sb.group_count()?;
    let dpb = sb.block_size as u64 / sb.descriptor_size as u64;
    if dpb == 0 { return Err(Error::InvalidGeometry); }
    let gdt_bytes = (groups as u64).checked_mul(sb.descriptor_size as u64).ok_or(Error::OutOfRange)?;
    let gdt_blocks = gdt_bytes.checked_add(sb.block_size as u64 - 1).ok_or(Error::OutOfRange)? / sb.block_size as u64;
    let superblock_block = if sb.block_size == 1024 { 1 } else { 0 };
    let mut used = 0usize;

    fn add(
        entries: &mut [transaction::MetadataOwnership],
        used: &mut usize,
        block: u64,
        kind: transaction::MetadataObjectKind,
        sb: &Superblock,
        zones: &SystemZoneMap<'_>,
    ) -> Result<(), Error> {
        if block >= sb.blocks_count || !zones.is_system_block(block) { return Err(Error::InvalidGeometry); }
        if *used == entries.len() { return Err(Error::TransactionTooLarge); }
        if entries[..*used].iter().any(|entry| entry.block == block) { return Err(Error::InvalidGeometry); }
        entries[*used] = transaction::MetadataOwnership {
            block, owner_inode: 0, generation: 0, kind,
        };
        *used += 1;
        Ok(())
    }

    add(entries, &mut used, superblock_block, Kind::Superblock, sb, zones)?;
    let gdt_start = (sb.first_data_block as u64).checked_add(1).ok_or(Error::OutOfRange)?;
    for offset in 0..gdt_blocks {
        add(entries, &mut used, gdt_start.checked_add(offset).ok_or(Error::OutOfRange)?,
            Kind::GroupDescriptor, sb, zones)?;
    }

    let itable_bytes = (sb.inodes_per_group as u64).checked_mul(sb.inode_size as u64).ok_or(Error::OutOfRange)?;
    let itable_blocks = itable_bytes.checked_add(sb.block_size as u64 - 1).ok_or(Error::OutOfRange)? / sb.block_size as u64;
    for group in 0..groups {
        let descriptor_block = gdt_start.checked_add(group as u64 / dpb).ok_or(Error::OutOfRange)?;
        read_block(dev, sb, descriptor_block, scratch)?;
        let slot = (group as u64 % dpb) as usize;
        let offset = slot.checked_mul(sb.descriptor_size as usize).ok_or(Error::OutOfRange)?;
        let desc = scratch.get(offset..offset + sb.descriptor_size as usize).ok_or(Error::Truncated)?;
        if sb.feature_ro_compat & 0x0400 != 0 {
            validate_group_descriptor_checksum(raw_superblock, group, desc)?;
        }
        let gd = GroupDescriptor::parse(desc, sb)?;
        let inode_base = (group as u64).checked_mul(sb.inodes_per_group as u64).ok_or(Error::OutOfRange)?;
        let remaining = (sb.inodes_count as u64).checked_sub(inode_base).ok_or(Error::InvalidGeometry)?;
        let actual_inodes = core::cmp::min(sb.inodes_per_group as u64, remaining);
        let table_bytes = actual_inodes.checked_mul(sb.inode_size as u64).ok_or(Error::OutOfRange)?;
        let actual_table_blocks = table_bytes.checked_add(sb.block_size as u64 - 1).ok_or(Error::OutOfRange)? / sb.block_size as u64;
        if actual_table_blocks > itable_blocks { return Err(Error::InvalidGeometry); }

        add(entries, &mut used, gd.block_bitmap, Kind::BlockBitmap, sb, zones)?;
        add(entries, &mut used, gd.inode_bitmap, Kind::InodeBitmap, sb, zones)?;
        for offset in 0..actual_table_blocks {
            add(entries, &mut used, gd.inode_table.checked_add(offset).ok_or(Error::OutOfRange)?,
                Kind::InodeTable, sb, zones)?;
        }
    }
    Ok(used)
}

/// Prove that an inode is allocated by consulting its group's validated
/// inode bitmap. Free or uninitialized inode groups are never accepted as
/// owners of replayable directory metadata.
fn validate_inode_allocated<D: ReadOnlyBlockDevice>(
    dev: &mut D, sb: &Superblock, raw_superblock: &[u8],
    inode_number: u32, scratch: &mut [u8],
) -> Result<(), Error> {
    if scratch.len() != sb.block_size as usize { return Err(Error::InvalidGeometry); }
    let (group, _) = sb.inode_location(inode_number)?;
    let descriptors_per_block = sb.block_size / sb.descriptor_size as u32;
    if descriptors_per_block == 0 { return Err(Error::InvalidGeometry); }
    let descriptor_block = (sb.first_data_block as u64).checked_add(1)
        .and_then(|v| v.checked_add((group / descriptors_per_block) as u64))
        .ok_or(Error::OutOfRange)?;
    read_block(dev, sb, descriptor_block, scratch)?;
    let index = (group % descriptors_per_block) as usize;
    let start = index.checked_mul(sb.descriptor_size as usize).ok_or(Error::OutOfRange)?;
    let end = start.checked_add(sb.descriptor_size as usize).ok_or(Error::OutOfRange)?;
    if sb.descriptor_size as usize > 64 { return Err(Error::UnsupportedFormat); }
    let source = scratch.get(start..end).ok_or(Error::Truncated)?;
    let mut descriptor_copy = [0u8; 64];
    descriptor_copy[..source.len()].copy_from_slice(source);
    let descriptor = &descriptor_copy[..source.len()];
    if sb.feature_ro_compat & RO_COMPAT_METADATA_CSUM != 0 {
        validate_group_descriptor_checksum(raw_superblock, group, descriptor)?;
    }
    // EXT4_BG_INODE_UNINIT: no inode in this group may be trusted until its
    // bitmap has been initialized by the filesystem.
    if u16at(descriptor, 18)? & 0x0001 != 0 { return Err(Error::InvalidGeometry); }
    let gd = GroupDescriptor::parse(descriptor, sb)?;
    read_block(dev, sb, gd.inode_bitmap, scratch)?;
    if sb.feature_ro_compat & RO_COMPAT_METADATA_CSUM != 0 {
        if sb.inodes_per_group & 7 != 0 { return Err(Error::InvalidGeometry); }
        let bitmap_bytes = (sb.inodes_per_group / 8) as usize;
        let bitmap = scratch.get(..bitmap_bytes).ok_or(Error::Truncated)?;
        validate_inode_bitmap_checksum(raw_superblock, descriptor, bitmap)?;
    }
    let bit = (inode_number - 1) % sb.inodes_per_group;
    let byte = (bit / 8) as usize;
    let mask = 1u8 << (bit & 7);
    if scratch.get(byte).copied().ok_or(Error::Truncated)? & mask == 0 {
        return Err(Error::InvalidGeometry);
    }
    Ok(())
}

/// Extend a system-metadata ownership manifest with directory data blocks.
/// Each inode is first proven allocated by its checksummed group inode bitmap,
/// then read from its validated inode table entry. Extent mappings are checked
/// against the retained system-zone map and directory block checksums are
/// verified before a block is authorized for journal replay.
/// Caller supplies one inode buffer and two block-sized scratch buffers.
pub fn add_directory_metadata_ownership<D: ReadOnlyBlockDevice>(
    dev: &mut D, sb: &Superblock, raw_superblock: &[u8],
    zones: &SystemZoneMap<'_>, directory_inodes: &[u32],
    inode_buffer: &mut [u8], scratch_a: &mut [u8], scratch_b: &mut [u8],
    entries: &mut [transaction::MetadataOwnership], used: &mut usize,
) -> Result<(), Error> {
    use transaction::{MetadataObjectKind, MetadataOwnership};
    if raw_superblock.len() < SUPERBLOCK_BYTES || inode_buffer.len() != sb.inode_size as usize
        || scratch_a.len() != sb.block_size as usize || scratch_b.len() != sb.block_size as usize
        || *used > entries.len() { return Err(Error::InvalidGeometry); }
    validate_superblock_checksum(raw_superblock)?;
    let journal_inode = if u32at(raw_superblock, 92)? & 0x0004 != 0 {
        Some(u32at(raw_superblock, 224)?)
    } else { None };
    for &ino in directory_inodes {
        if ino == 0 || Some(ino) == journal_inode { return Err(Error::InvalidGeometry); }
        validate_inode_allocated(dev, sb, raw_superblock, ino, scratch_a)?;
        read_inode(dev, sb, raw_superblock, ino, inode_buffer, scratch_a)?;
        let inode = Inode::parse(inode_buffer, sb.inode_size, sb.revision, sb.feature_ro_compat)?;
        if !inode.is_directory { return Err(Error::InvalidGeometry); }
        let generation = u32at(inode_buffer, 100)?;
        // EXTENTS_FL is an inode flag; the superblock feature only says the
        // format is available, not that this particular inode uses it.
        let inode_uses_extents = inode.flags & 0x0008_0000 != 0;
        let blocks = inode.size.checked_add(sb.block_size as u64 - 1)
            .ok_or(Error::OutOfRange)? / sb.block_size as u64;
        if blocks > u32::MAX as u64 { return Err(Error::OutOfRange); }

        for logical in 0..blocks as u32 {
            let physical = if inode_uses_extents {
                // Walk the complete lookup path for each populated logical
                // directory block. Recording every external node on these
                // paths makes all nodes that contribute to directory data
                // mapping eligible for narrowly scoped JBD2 replay.
                let mut node_is_root = true;
                let mut expected_depth = None;
                let mut mapped = None;
                let mut finished = false;
                for _ in 0..=MAX_EXTENT_DEPTH {
                    let node = if node_is_root { &inode_buffer[40..100] } else { &scratch_a[..] };
                    let (depth, step) = inspect_extent_node(node, logical, sb, expected_depth)?;
                    match step {
                        ExtentStep::Found(block) => { mapped = Some(block); finished = true; break; }
                        ExtentStep::ZeroOrHole => { finished = true; break; }
                        ExtentStep::Child(child) => {
                            if depth == 0 { return Err(Error::InvalidExtentTree); }
                            zones.validate_data_range(child, 1, sb.blocks_count)?;
                            if entries[..*used].iter().any(|entry| entry.block == child
                                && (entry.owner_inode != ino || entry.generation != generation
                                    || entry.kind != MetadataObjectKind::ExtentNode)) {
                                return Err(Error::InvalidGeometry);
                            }
                            if !entries[..*used].iter().any(|entry| entry.block == child) {
                                if *used == entries.len() { return Err(Error::TransactionTooLarge); }
                                entries[*used] = MetadataOwnership {
                                    block: child, owner_inode: ino, generation,
                                    kind: MetadataObjectKind::ExtentNode,
                                };
                                *used += 1;
                            }
                            read_block(dev, sb, child, scratch_a)?;
                            if sb.feature_ro_compat & RO_COMPAT_METADATA_CSUM != 0 {
                                validate_extent_block_checksum(raw_superblock, ino, generation, scratch_a)?;
                            }
                            expected_depth = Some(depth - 1);
                            node_is_root = false;
                        }
                    }
                }
                if !finished { return Err(Error::InvalidExtentTree); }
                mapped
            } else {
                let block = map_legacy_block(dev, sb, &inode_buffer[40..100],
                    logical as u64, scratch_a, scratch_b)?;
                if let Some(block) = block { zones.validate_data_range(block, 1, sb.blocks_count)?; }
                block
            }.ok_or(Error::InvalidExtentTree)?;

            read_block(dev, sb, physical, scratch_b)?;
            if sb.feature_ro_compat & RO_COMPAT_METADATA_CSUM != 0 {
                validate_directory_block_checksum(raw_superblock, ino, generation, scratch_b)?;
            }
            if entries[..*used].iter().any(|entry| entry.block == physical) {
                return Err(Error::InvalidGeometry);
            }
            if *used == entries.len() { return Err(Error::TransactionTooLarge); }
            entries[*used] = MetadataOwnership {
                block: physical, owner_inode: ino, generation, kind: MetadataObjectKind::Directory,
            };
            *used += 1;
        }
    }
    Ok(())
}

/// Discover every allocated directory inode by scanning each initialized inode
/// bitmap. Descriptor and bitmap checksums are verified before bits are trusted;
/// every candidate inode is then read through the inode-table path (including
/// inode checksum validation) and classified by mode. The output is caller-owned
/// and bounded. This discovers allocated directories, not namespace reachability:
/// callers must still validate root/. /.. parent relationships before treating
/// the result as a complete live namespace.
pub fn discover_allocated_directories<D: ReadOnlyBlockDevice>(
    dev: &mut D,
    sb: &Superblock,
    raw_superblock: &[u8],
    zones: &SystemZoneMap<'_>,
    directory_inodes: &mut [u32],
    inode_buffer: &mut [u8],
    bitmap_scratch: &mut [u8],
    inode_scratch: &mut [u8],
) -> Result<usize, Error> {
    if raw_superblock.len() < SUPERBLOCK_BYTES
        || directory_inodes.is_empty()
        || inode_buffer.len() != sb.inode_size as usize
        || bitmap_scratch.len() != sb.block_size as usize
        || inode_scratch.len() != sb.block_size as usize {
        return Err(Error::InvalidGeometry);
    }
    validate_superblock_checksum(raw_superblock)?;
    let groups = sb.group_count()?;
    let dpb = sb.block_size / sb.descriptor_size as u32;
    if dpb == 0 || sb.descriptor_size as usize > 64
        || sb.inodes_per_group & 7 != 0 {
        return Err(Error::UnsupportedFormat);
    }
    let gdt_start = (sb.first_data_block as u64).checked_add(1).ok_or(Error::OutOfRange)?;
    let journal_inode = if sb.feature_compat & 0x0004 != 0 {
        Some(u32at(raw_superblock, 224)?)
    } else { None };
    let bitmap_bytes = (sb.inodes_per_group / 8) as usize;
    let mut found = 0usize;
    for group in 0..groups {
        let descriptor_block = gdt_start.checked_add((group / dpb) as u64)
            .ok_or(Error::OutOfRange)?;
        read_block(dev, sb, descriptor_block, bitmap_scratch)?;
        let slot = (group % dpb) as usize;
        let offset = slot.checked_mul(sb.descriptor_size as usize).ok_or(Error::OutOfRange)?;
        let end = offset.checked_add(sb.descriptor_size as usize).ok_or(Error::OutOfRange)?;
        let descriptor_slice = bitmap_scratch.get(offset..end).ok_or(Error::Truncated)?;
        let mut descriptor_copy = [0u8; 64];
        descriptor_copy[..descriptor_slice.len()].copy_from_slice(descriptor_slice);
        let descriptor = &descriptor_copy[..descriptor_slice.len()];
        if sb.feature_ro_compat & RO_COMPAT_METADATA_CSUM != 0 {
            validate_group_descriptor_checksum(raw_superblock, group, descriptor)?;
        }
        if u16at(descriptor, 18)? & 0x0001 != 0 {
            return Err(Error::UnsupportedFormat);
        }
        let gd = GroupDescriptor::parse(descriptor, sb)?;
        if !zones.is_system_block(gd.inode_bitmap) || !zones.is_system_block(gd.inode_table) {
            return Err(Error::InvalidGeometry);
        }
        read_block(dev, sb, gd.inode_bitmap, bitmap_scratch)?;
        let bitmap = bitmap_scratch.get(..bitmap_bytes).ok_or(Error::Truncated)?;
        if sb.feature_ro_compat & RO_COMPAT_METADATA_CSUM != 0 {
            validate_inode_bitmap_checksum(raw_superblock, descriptor, bitmap)?;
        }
        let base = (group as u64).checked_mul(sb.inodes_per_group as u64)
            .ok_or(Error::OutOfRange)?;
        let remaining = (sb.inodes_count as u64).checked_sub(base).ok_or(Error::InvalidGeometry)?;
        let in_group = core::cmp::min(sb.inodes_per_group as u64, remaining) as u32;
        for bit in 0..in_group {
            if bitmap_scratch[(bit / 8) as usize] & (1u8 << (bit & 7)) == 0 {
                continue;
            }
            let ino64 = base.checked_add(bit as u64).and_then(|v| v.checked_add(1))
                .ok_or(Error::OutOfRange)?;
            let ino = u32::try_from(ino64).map_err(|_| Error::OutOfRange)?;
            if Some(ino) == journal_inode { continue; }
            read_inode(dev, sb, raw_superblock, ino, inode_buffer, inode_scratch)?;
            let inode = Inode::parse(inode_buffer, sb.inode_size, sb.revision, sb.feature_ro_compat)?;
            if !inode.is_directory { continue; }
            if found == directory_inodes.len() { return Err(Error::TransactionTooLarge); }
            directory_inodes[found] = ino;
            found += 1;
        }
    }
    Ok(found)
}

/// Add the ext4 internal journal inode's data blocks to the protected system
/// zones, matching Linux ext4's reserved-inode protection. The caller supplies
/// an inode-sized buffer and two block-sized scratch buffers. External journal
/// devices remain unsupported.
pub fn protect_reserved_journal_inode_zones<D: ReadOnlyBlockDevice>(
    dev: &mut D,
    sb: &Superblock,
    raw_superblock: &[u8],
    ranges: &mut [BlockRange],
    used: &mut usize,
    inode_buffer: &mut [u8],
    scratch_a: &mut [u8],
    scratch_b: &mut [u8],
) -> Result<(), Error> {
    if raw_superblock.len() < SUPERBLOCK_BYTES
        || inode_buffer.len() != sb.inode_size as usize
        || scratch_a.len() != sb.block_size as usize
        || scratch_b.len() != sb.block_size as usize
        || *used > ranges.len() {
        return Err(Error::InvalidGeometry);
    }
    if u32at(raw_superblock, 92)? & 0x0004 == 0 { return Ok(()); }
    let journal_ino = u32at(raw_superblock, 224)?;
    if journal_ino == 0 { return Err(Error::InvalidGeometry); }
    read_inode(dev, sb, raw_superblock, journal_ino, inode_buffer, scratch_a)?;
    let inode = Inode::parse(inode_buffer, sb.inode_size, sb.revision, sb.feature_ro_compat)?;
    if !inode.is_regular_file || inode.size == 0 { return Err(Error::InvalidGeometry); }
    let blocks = inode.size.checked_add(sb.block_size as u64 - 1).ok_or(Error::OutOfRange)? / sb.block_size as u64;
    if blocks > u32::MAX as u64 || blocks > MAX_RESERVED_JOURNAL_BLOCKS { return Err(Error::OutOfRange); }
    let mut pending_start = 0u64;
    let mut pending_count = 0u64;
    for logical in 0..blocks as u32 {
        let physical = if sb.has_extents() {
            let zones = SystemZoneMap::new(&ranges[..*used]);
            map_extent_block_checked_zones_for_inode(
                dev, sb, raw_superblock, journal_ino, u32at(inode_buffer, 100)?,
                &inode_buffer[40..100], logical, scratch_a, &zones, Some(journal_ino),
            )?
        } else {
            map_legacy_block(dev, sb, &inode_buffer[40..100], logical as u64, scratch_a, scratch_b)?
        }.ok_or(Error::InvalidExtentTree)?;
        if pending_count != 0 && pending_start.checked_add(pending_count) == Some(physical) {
            pending_count = pending_count.checked_add(1).ok_or(Error::OutOfRange)?;
        } else {
            if pending_count != 0 {
                let zones = SystemZoneMap::new(&ranges[..*used]);
                zones.validate_data_range(pending_start, pending_count, sb.blocks_count)?;
                push(ranges, used, pending_start, pending_count, sb)?;
            }
            pending_start = physical;
            pending_count = 1;
        }
    }
    if pending_count != 0 {
        let zones = SystemZoneMap::new(&ranges[..*used]);
        zones.validate_data_range(pending_start, pending_count, sb.blocks_count)?;
        push(ranges, used, pending_start, pending_count, sb)?;
    }
    ranges[..*used].sort_unstable_by_key(|r| r.start);
    for i in 1..*used {
        if ranges[i - 1].end > ranges[i].start { return Err(Error::InvalidGeometry); }
    }
    Ok(())
}

pub fn validate_data_extent(map: &SystemZoneMap<'_>, sb: &Superblock, start: u64, count: u64) -> Result<(), Error> {
    map.validate_data_range(start, count, sb.blocks_count)
}

pub fn map_extent_block_checked_zones<D: ReadOnlyBlockDevice>(
    dev: &mut D, sb: &Superblock, raw_superblock: &[u8], inode_number: u32,
    generation: u32, root: &[u8], logical: u32, scratch: &mut [u8],
    zones: &SystemZoneMap<'_>,
) -> Result<Option<u64>, Error> {
    map_extent_block_checked_zones_for_inode(dev, sb, raw_superblock, inode_number,
        generation, root, logical, scratch, zones, None)
}

/// Variant matching Linux ext4's reserved-inode exception: the journal inode
/// may reference its own protected journal blocks, while every other inode is
/// rejected if it maps into those ranges.
pub fn map_extent_block_checked_zones_for_inode<D: ReadOnlyBlockDevice>(
    dev: &mut D, sb: &Superblock, raw_superblock: &[u8], inode_number: u32,
    generation: u32, root: &[u8], logical: u32, scratch: &mut [u8],
    zones: &SystemZoneMap<'_>, reserved_inode: Option<u32>,
) -> Result<Option<u64>, Error> {
    if root.len() < 60 || scratch.len() != sb.block_size as usize { return Err(Error::UnsupportedFormat); }
    validate_superblock_checksum(raw_superblock)?;
    let bypass_reserved_zone = reserved_inode == Some(inode_number);
    let mut is_root = true;
    let mut expected_depth = None;
    for _ in 0..=MAX_EXTENT_DEPTH {
        let node = if is_root { &root[..60] } else { &scratch[..] };
        let (depth, step) = inspect_extent_node(node, logical, sb, expected_depth)?;
        match step {
            ExtentStep::Found(block) => {
                if !bypass_reserved_zone { validate_data_extent(zones, sb, block, 1)?; }
                return Ok(Some(block));
            }
            ExtentStep::ZeroOrHole => return Ok(None),
            ExtentStep::Child(child) => {
                if depth == 0 { return Err(Error::InvalidExtentTree); }
                if !bypass_reserved_zone { validate_data_extent(zones, sb, child, 1)?; }
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
    fn sparse_super_backup_groups_use_the_ro_compat_feature_bit() {
        let sb = Superblock {
            inodes_count: 4096, blocks_count: 100_000, first_data_block: 0,
            block_size: 4096, blocks_per_group: 8192, inodes_per_group: 2048,
            inode_size: 256, descriptor_size: 64, revision: 1,
            feature_compat: 0, feature_incompat: 0, feature_ro_compat: 1,
            read_only: false,
        };
        let raw = [0u8; SUPERBLOCK_BYTES];
        assert_eq!(group_has_super(&sb, &raw, 0), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 1), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 2), Ok(false));
        assert_eq!(group_has_super(&sb, &raw, 3), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 4), Ok(false));
        assert_eq!(group_has_super(&sb, &raw, 5), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 7), Ok(true));
        assert_eq!(group_has_super(&sb, &raw, 8), Ok(false));
    }

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
    fn builds_primary_and_backup_system_zones_for_4k_filesystem() {
        struct Dev { table: [u8; 4096] }
        impl ReadOnlyBlockDevice for Dev {
            type DeviceError = ();
            fn capacity_bytes(&self) -> u64 { 16384 * 4096 }
            fn read_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), Self::DeviceError> {
                if offset != 4096 || out.len() != 4096 { return Err(()); }
                out.copy_from_slice(&self.table);
                Ok(())
            }
        }
        let mut dev = Dev { table: [0; 4096] };
        for (i, base) in [(0usize, 10u32), (1usize, 8200u32)] {
            let o = i * 32;
            dev.table[o..o+4].copy_from_slice(&base.to_le_bytes());
            dev.table[o+4..o+8].copy_from_slice(&(base+1).to_le_bytes());
            dev.table[o+8..o+12].copy_from_slice(&(base+2).to_le_bytes());
        }
        let sb = Superblock {
            inodes_count: 256, blocks_count: 16384, first_data_block: 0, block_size: 4096,
            blocks_per_group: 8192, inodes_per_group: 128, inode_size: 256,
            descriptor_size: 32, revision: 1, feature_compat: 0,
            feature_incompat: 0, feature_ro_compat: 0, read_only: true,
        };
        let raw = [0u8; SUPERBLOCK_BYTES];
        let mut scratch = [0u8; 4096];
        let mut ranges = [BlockRange { start: 0, end: 0 }; 13];
        let used = build_system_zones(&mut dev, &sb, &raw, &mut scratch, &mut ranges).unwrap();
        let map = SystemZoneMap::new(&ranges[..used]);
        assert!(map.is_system_block(0));
        assert!(map.is_system_block(1));
        assert!(map.is_system_block(8192));
        assert!(map.is_system_block(8193));
        assert!(map.is_system_block(10));
        assert!(map.is_system_block(8202));
        assert_eq!(map.validate_data_range(30, 1, sb.blocks_count), Ok(()));
        assert_eq!(map.validate_data_range(12, 1, sb.blocks_count), Err(Error::InvalidExtentTree));
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

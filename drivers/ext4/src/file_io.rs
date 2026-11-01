//! Root-directory file I/O for the ext4 filesystem driver.
//!
//! These routines operate on the filesystem's bounded block-device interface.
//! They deliberately do not know about ATA, WDM, or CognacOS device objects.

use crate::{
    read_block, read_inode, map_extent_block_checked, read_superblock,
    validate_superblock_checksum, validate_mount_state,
    BlockDevice, Error, ExtFilesystem, Inode, ReadOnlyBlockDevice, Superblock,
    WriteError, SUPERBLOCK_BYTES, SUPERBLOCK_DISK_OFFSET,
};

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    let value = bytes.get(offset..offset.checked_add(2).ok_or(Error::OutOfRange)?)
        .ok_or(Error::Truncated)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let value = bytes.get(offset..offset.checked_add(4).ok_or(Error::OutOfRange)?)
        .ok_or(Error::Truncated)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}
fn root_name(path: &[u8]) -> Result<&[u8], Error> {
    if path.len() < 2 || path[0] != b'\\' || path[1..].contains(&b'\\') {
        return Err(Error::UnsupportedFormat);
    }
    let name = &path[1..];
    if name.is_empty() || name.contains(&b'/') || name.contains(&0) {
        return Err(Error::OutOfRange);
    }
    Ok(name)
}

/// Read and retain the exact validated superblock bytes required for checksum-aware inode/extent reads.
pub fn read_superblock_with_raw<D: ReadOnlyBlockDevice>(
    device: &mut D,
) -> Result<(Superblock, [u8; SUPERBLOCK_BYTES]), Error> {
    let end = SUPERBLOCK_DISK_OFFSET.checked_add(SUPERBLOCK_BYTES as u64)
        .ok_or(Error::OutOfRange)?;
    if device.capacity_bytes() < end { return Err(Error::OutOfRange); }
    let mut raw = [0u8; SUPERBLOCK_BYTES];
    device.read_at(SUPERBLOCK_DISK_OFFSET, &mut raw).map_err(|_| Error::Io)?;
    let superblock = Superblock::parse(&raw)?;
    validate_superblock_checksum(&raw)?;
    validate_mount_state(&raw)?;
    let fs_end = superblock.blocks_count.checked_mul(superblock.block_size as u64)
        .ok_or(Error::OutOfRange)?;
    if fs_end > device.capacity_bytes() { return Err(Error::OutOfRange); }
    Ok((superblock, raw))
}

fn lookup_root_file_inode<D: ReadOnlyBlockDevice>(
    device: &mut D,
    superblock: &Superblock,
    raw_superblock: &[u8; SUPERBLOCK_BYTES],
    name: &[u8],
) -> Result<(u32, Inode, [u8; 256]), Error> {
    if name.is_empty() || name.contains(&b'/') || !superblock.has_extents()
        || superblock.inode_size > 256 {
        return Err(Error::UnsupportedFormat);
    }
    let block_size = superblock.block_size as usize;
    if block_size > 4096 { return Err(Error::UnsupportedFormat); }
    let inode_size = superblock.inode_size as usize;
    let mut scratch = [0u8; 4096];
    let mut root_raw = [0u8; 256];
    read_inode(
        device, superblock, raw_superblock, 2,
        &mut root_raw[..inode_size], &mut scratch[..block_size],
    )?;
    let root = Inode::parse(
        &root_raw[..inode_size], superblock.inode_size,
        superblock.revision, superblock.feature_ro_compat,
    )?;
    if !root.is_directory || root.size > (superblock.block_size as u64) * 64 {
        return Err(Error::UnsupportedFormat);
    }
    let root_generation = u32_at(&root_raw, 100)?;
    let mut found = None;
    for logical in 0..root.size.div_ceil(superblock.block_size as u64) {
        let physical = map_extent_block_checked(
            device, superblock, raw_superblock, 2, root_generation,
            &root_raw[40..100], logical as u32, &mut scratch[..block_size],
        )?;
        if let Some(block) = physical {
            read_block(device, superblock, block, &mut scratch[..block_size])?;
        } else {
            scratch[..block_size].fill(0);
        }
        let mut offset = 0usize;
        while offset < block_size {
            if block_size - offset < 8 { return Err(Error::InvalidDirectoryEntry); }
            let inode_number = u32_at(&scratch, offset)?;
            let record_length = u16_at(&scratch, offset + 4)? as usize;
            let name_length = scratch[offset + 6] as usize;
            if record_length < 8 || record_length & 3 != 0
                || record_length > block_size - offset
                || name_length > record_length - 8
                || inode_number > superblock.inodes_count {
                return Err(Error::InvalidDirectoryEntry);
            }
            if inode_number != 0 && name_length == name.len()
                && &scratch[offset + 8..offset + 8 + name_length] == name {
                found = Some(inode_number);
                break;
            }
            offset += record_length;
        }
        if found.is_some() { break; }
    }
    let inode_number = found.ok_or(Error::OutOfRange)?;
    let mut inode_raw = [0u8; 256];
    read_inode(
        device, superblock, raw_superblock, inode_number,
        &mut inode_raw[..inode_size], &mut scratch[..block_size],
    )?;
    let inode = Inode::parse(
        &inode_raw[..inode_size], superblock.inode_size,
        superblock.revision, superblock.feature_ro_compat,
    )?;
    if !inode.is_regular_file { return Err(Error::UnsupportedFormat); }
    Ok((inode_number, inode, inode_raw))
}

/// Return the size of a root-directory regular file using checked inode and extent traversal.
pub fn root_file_size<D: ReadOnlyBlockDevice>(device: &mut D, path: &[u8]) -> Result<u64, Error> {
    let name = root_name(path)?;
    let (superblock, raw_superblock) = read_superblock_with_raw(device)?;
    let (_, inode, _) = lookup_root_file_inode(device, &superblock, &raw_superblock, name)?;
    Ok(inode.size)
}

/// Read a root-directory file through its extents. Holes are returned as zero-filled bytes.
pub fn read_root_file_range<D: ReadOnlyBlockDevice>(
    device: &mut D, path: &[u8], offset: u64, output: &mut [u8],
) -> Result<(u64, usize), Error> {
    let name = root_name(path)?;
    let (superblock, raw_superblock) = read_superblock_with_raw(device)?;
    let (inode_number, inode, inode_raw) =
        lookup_root_file_inode(device, &superblock, &raw_superblock, name)?;
    if offset >= inode.size || output.is_empty() { return Ok((inode.size, 0)); }
    let block_size = superblock.block_size as usize;
    let generation = u32_at(&inode_raw, 100)?;
    let count = core::cmp::min(output.len() as u64, inode.size - offset) as usize;
    let mut scratch = [0u8; 4096];
    let mut done = 0usize;
    while done < count {
        let absolute = offset.checked_add(done as u64).ok_or(Error::OutOfRange)?;
        let logical = absolute / superblock.block_size as u64;
        if logical > u32::MAX as u64 { return Err(Error::OutOfRange); }
        let within = (absolute % superblock.block_size as u64) as usize;
        let amount = core::cmp::min(block_size - within, count - done);
        let physical = map_extent_block_checked(
            device, &superblock, &raw_superblock, inode_number, generation,
            &inode_raw[40..100], logical as u32, &mut scratch[..block_size],
        )?;
        if let Some(block) = physical {
            read_block(device, &superblock, block, &mut scratch[..block_size])?;
            output[done..done + amount].copy_from_slice(&scratch[within..within + amount]);
        } else {
            output[done..done + amount].fill(0);
        }
        done += amount;
    }
    Ok((inode.size, count))
}

/// Write existing extents only. The filesystem facade rejects journaled/read-only volumes and
/// the write path never allocates blocks or changes inode size/metadata.
pub fn write_root_file_range<D: BlockDevice>(
    device: D, path: &[u8], offset: u64, data: &[u8],
) -> Result<usize, Error> {
    let name = root_name(path)?;
    let mut device = device;
    let (superblock, raw_superblock) = read_superblock_with_raw(&mut device)?;
    let (inode_number, _inode, inode_raw) =
        lookup_root_file_inode(&mut device, &superblock, &raw_superblock, name)?;
    let mut filesystem = ExtFilesystem::mount_writable(device)?;
    let block_size = superblock.block_size as usize;
    if block_size > 4096 { return Err(Error::UnsupportedFormat); }
    let mut scratch = [0u8; 4096];
    filesystem.write_file_data(
        inode_number, &inode_raw[..superblock.inode_size as usize],
        offset, data, &mut scratch[..block_size],
    ).map_err(|error| match error {
        WriteError::Filesystem(error) => error,
        WriteError::ReadOnly => Error::UnsupportedFormat,
        WriteError::OutOfRange => Error::OutOfRange,
        WriteError::Io => Error::Io,
        WriteError::PartialBlock => Error::InvalidGeometry,
    })
}

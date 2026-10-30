//! Mount-owned, bounded JBD2 transaction coordinator.
//!
//! Transactions stage complete block images in caller-owned arenas. Exclusive
//! mutable borrowing serializes writers; target policy validates metadata
//! ownership and the retained system-zone map protects ordered data/revokes.
//! This layer deliberately does not enable writable mounts.

use super::{BlockDevice, Error, Superblock};
use super::jbd2::{self, JournalBlockMap, ReplayDevice};
use super::system_zone::SystemZoneMap;

pub const MAX_STAGED_BLOCKS: usize = jbd2::MAX_TRANSACTION_TAGS;

/// Block-device adapter that bounds every journal/home-block I/O to the
/// mounted filesystem geometry and the underlying partition capacity.
pub struct FilesystemBlockAdapter<'a, D: BlockDevice> {
    device: &'a mut D,
    filesystem: Superblock,
}

impl<'a, D: BlockDevice> FilesystemBlockAdapter<'a, D> {
    pub fn new(device: &'a mut D, filesystem: Superblock) -> Result<Self, Error> {
        let end = filesystem.blocks_count.checked_mul(filesystem.block_size as u64)
            .ok_or(Error::OutOfRange)?;
        if end > device.capacity_bytes() { return Err(Error::OutOfRange); }
        Ok(Self { device, filesystem })
    }
}

impl<D: BlockDevice> ReplayDevice for FilesystemBlockAdapter<'_, D> {
    fn filesystem_blocks(&self) -> u64 { self.filesystem.blocks_count }
    fn read_block(&mut self, block: u64, out: &mut [u8]) -> Result<(), ()> {
        if block >= self.filesystem.blocks_count || out.len() != self.filesystem.block_size as usize {
            return Err(());
        }
        let offset = block.checked_mul(self.filesystem.block_size as u64).ok_or(())?;
        self.device.read_at(offset, out).map_err(|_| ())
    }
    fn write_block(&mut self, block: u64, data: &[u8]) -> Result<(), ()> {
        if block >= self.filesystem.blocks_count || data.len() != self.filesystem.block_size as usize {
            return Err(());
        }
        let offset = block.checked_mul(self.filesystem.block_size as u64).ok_or(())?;
        let end = offset.checked_add(data.len() as u64).ok_or(())?;
        if end > self.device.capacity_bytes() { return Err(()); }
        self.device.write_at(offset, data).map_err(|_| ())
    }
    fn flush(&mut self) -> Result<(), ()> { self.device.flush().map_err(|_| ()) }
}



pub trait MetadataTargetPolicy {
    fn validate_metadata_target(&mut self, block: u64) -> Result<(), Error>;
}

pub struct TransactionCoordinator<'a, D, M, P>
where D: ReplayDevice, M: JournalBlockMap, P: MetadataTargetPolicy {
    device: &'a mut D,
    map: &'a mut M,
    journal_superblock: &'a mut [u8],
    filesystem: Superblock,
    zones: SystemZoneMap<'a>,
    policy: &'a mut P,
}

impl<'a, D, M, P> TransactionCoordinator<'a, D, M, P>
where D: ReplayDevice, M: JournalBlockMap, P: MetadataTargetPolicy {
    pub fn new(
        device: &'a mut D, map: &'a mut M, journal_superblock: &'a mut [u8],
        filesystem: Superblock, zones: SystemZoneMap<'a>, policy: &'a mut P,
    ) -> Result<Self, Error> {
        let journal = jbd2::JournalSuperblock::parse(journal_superblock)
            .map_err(|_| Error::UnsupportedFormat)?;
        if journal.block_size != filesystem.block_size
            || journal.start != 0
            || journal_superblock.len() != filesystem.block_size as usize
            || journal.max_len as u64 > device.filesystem_blocks() {
            return Err(Error::UnsupportedFormat);
        }
        Ok(Self { device, map, journal_superblock, filesystem, zones, policy })
    }

    pub fn begin<'b>(
        &'b mut self,
        metadata_targets: &'b mut [u64], metadata_storage: &'b mut [u8],
        ordered_targets: &'b mut [u64], ordered_storage: &'b mut [u8],
        revoked_targets: &'b mut [u64], descriptor_scratch: &'b mut [u8],
    ) -> Result<Transaction<'b, 'a, D, M, P>, Error> {
        let bs = self.filesystem.block_size as usize;
        if metadata_targets.len() > MAX_STAGED_BLOCKS
            || metadata_storage.len() != metadata_targets.len().checked_mul(bs).ok_or(Error::OutOfRange)?
            || ordered_targets.len() > MAX_STAGED_BLOCKS
            || ordered_storage.len() != ordered_targets.len().checked_mul(bs).ok_or(Error::OutOfRange)?
            || revoked_targets.len() > jbd2::MAX_TRANSACTION_REVOKES
            || descriptor_scratch.len() != bs {
            return Err(Error::TransactionTooLarge);
        }
        Ok(Transaction { coordinator: self, metadata_targets, metadata_storage,
            metadata_count: 0, ordered_targets, ordered_storage, ordered_count: 0,
            revoked_targets, revoke_count: 0, descriptor_scratch })
    }
}

pub struct Transaction<'b, 'a, D, M, P>
where D: ReplayDevice, M: JournalBlockMap, P: MetadataTargetPolicy {
    coordinator: &'b mut TransactionCoordinator<'a, D, M, P>,
    metadata_targets: &'b mut [u64],
    metadata_storage: &'b mut [u8],
    metadata_count: usize,
    ordered_targets: &'b mut [u64],
    ordered_storage: &'b mut [u8],
    ordered_count: usize,
    revoked_targets: &'b mut [u64],
    revoke_count: usize,
    descriptor_scratch: &'b mut [u8],
}

impl<D, M, P> Transaction<'_, '_, D, M, P>
where D: ReplayDevice, M: JournalBlockMap, P: MetadataTargetPolicy {
    fn block_size(&self) -> usize { self.coordinator.filesystem.block_size as usize }

    fn unused(&self, block: u64) -> bool {
        !self.metadata_targets[..self.metadata_count].contains(&block)
            && !self.ordered_targets[..self.ordered_count].contains(&block)
            && !self.revoked_targets[..self.revoke_count].contains(&block)
    }

    pub fn stage_metadata(&mut self, block: u64, image: &[u8]) -> Result<(), Error> {
        let bs = self.block_size();
        if image.len() != bs || block >= self.coordinator.filesystem.blocks_count
            || self.metadata_count == self.metadata_targets.len() || !self.unused(block) {
            return Err(Error::OutOfRange);
        }
        self.coordinator.policy.validate_metadata_target(block)?;
        let start = self.metadata_count.checked_mul(bs).ok_or(Error::OutOfRange)?;
        self.metadata_storage[start..start + bs].copy_from_slice(image);
        self.metadata_targets[self.metadata_count] = block;
        self.metadata_count += 1;
        Ok(())
    }

    pub fn stage_ordered_data(&mut self, block: u64, image: &[u8]) -> Result<(), Error> {
        let bs = self.block_size();
        if image.len() != bs || block >= self.coordinator.filesystem.blocks_count
            || self.ordered_count == self.ordered_targets.len() || !self.unused(block) {
            return Err(Error::OutOfRange);
        }
        self.coordinator.zones.validate_data_range(block, 1, self.coordinator.filesystem.blocks_count)?;
        let start = self.ordered_count.checked_mul(bs).ok_or(Error::OutOfRange)?;
        self.ordered_storage[start..start + bs].copy_from_slice(image);
        self.ordered_targets[self.ordered_count] = block;
        self.ordered_count += 1;
        Ok(())
    }

    pub fn stage_revoke(&mut self, block: u64) -> Result<(), Error> {
        if block >= self.coordinator.filesystem.blocks_count
            || self.revoke_count == self.revoked_targets.len() || !self.unused(block) {
            return Err(Error::OutOfRange);
        }
        self.coordinator.zones.validate_data_range(block, 1, self.coordinator.filesystem.blocks_count)?;
        self.revoked_targets[self.revoke_count] = block;
        self.revoke_count += 1;
        Ok(())
    }

    /// Ordered data is flushed before the journal commit. The journal commit
    /// is flushed before home metadata; home metadata is flushed before the
    /// clean journal superblock is published.
    pub fn commit(self) -> Result<u32, Error> {
        if self.metadata_count == 0 { return Err(Error::InvalidGeometry); }
        let bs = self.block_size();
        let mut metadata_refs: [&[u8]; MAX_STAGED_BLOCKS] = [&[]; MAX_STAGED_BLOCKS];
        let mut ordered_refs: [&[u8]; MAX_STAGED_BLOCKS] = [&[]; MAX_STAGED_BLOCKS];
        for i in 0..self.metadata_count {
            let start = i.checked_mul(bs).ok_or(Error::OutOfRange)?;
            metadata_refs[i] = &self.metadata_storage[start..start + bs];
        }
        for i in 0..self.ordered_count {
            let start = i.checked_mul(bs).ok_or(Error::OutOfRange)?;
            ordered_refs[i] = &self.ordered_storage[start..start + bs];
        }
        jbd2::commit_ordered_with_revokes(
            self.coordinator.device, self.coordinator.map,
            self.coordinator.journal_superblock,
            &self.ordered_targets[..self.ordered_count], &ordered_refs[..self.ordered_count],
            &self.metadata_targets[..self.metadata_count], &metadata_refs[..self.metadata_count],
            &self.revoked_targets[..self.revoke_count], self.descriptor_scratch,
        ).map_err(|_| Error::Io)
    }
}

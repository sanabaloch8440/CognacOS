# ext4 compatibility and safety audit

**Audit date:** 2026-10-30  
**Linux reference:** torvalds/linux master at d89e1d186ec9271bf78e62771779e9fc03bf0fc3  
**CognacOS scope:** drivers/ext4/src/lib.rs, ops.rs, and jbd2.rs

This is a source-level compatibility and safety audit, not a claim that CognacOS is a drop-in implementation of Linux ext4. Linux ext4 is a large filesystem implementation with extensive mount-time feature negotiation, system-zone validation, journaling, recovery, concurrency, and crash testing.

## Executive result

- **Read-only:** CognacOS can read a deliberately limited subset of ext2/ext3/ext4 layouts. It rejects a number of common ext4 feature combinations, including indexed directories, so it will not mount many normal Linux ext4 volumes.
- **Writable:** Writable mounting is now fail-closed. The previous facade could write through an extent mapping without validating that the target physical blocks were outside all ext4 system-zone metadata. It also lacked a mount-owned transaction coordinator. That was not a safe general-purpose writable mount.
- **Recovery:** The bounded JBD2 replay implementation had a correctness defect: descriptor tags and revokes from a transaction were accumulated before its commit record was seen. An incomplete final transaction could therefore leak uncommitted metadata into the replay pass. This has been fixed, with regression tests.
- **Compatibility:** Complete Linux ext4 compatibility is not achieved. Unsupported features are rejected rather than guessed at or silently interpreted.

## System-zone milestone (2026-10-30)

CognacOS now has a caller-bounded system-zone builder in `system_zone.rs`. It derives backup-superblock placement from SPARSE_SUPER/SPARSE_SUPER2, computes primary and backup GDT spans, accounts for reserved GDT blocks, reads and checksum-validates each primary group descriptor, and records block bitmap, inode bitmap, and inode-table ranges. The checked extent traversal variant validates both external extent-tree child blocks and final mapped data blocks against this map. It fails closed when caller-provided range storage is exhausted or metadata geometry/checksums are invalid.

This is not yet mount-integrated: the mount facade does not own/persist the zone map, journal inode extents are not added as a separate reserved set, and allocation helpers do not yet consume the map. Writable mounting therefore remains disabled.

## Changes made during this audit

1. JBD2 replay now snapshots the tag/revoke array positions at the start of each transaction. If the transaction is malformed or has no matching commit block, both positions are rolled back before replay proceeds. This ensures the replay pass sees only records from committed transactions.
2. JBD2 superblock parsing now rejects the legacy checksum-v1 COMPAT feature. CognacOS does not implement its transaction-wide checksum verification; silently accepting this feature would weaken journal integrity validation.
3. JBD2 descriptor tags now reject reserved flag bits rather than interpreting unknown tag semantics.
4. Writable mounting now returns an unsupported-format error after validating the superblock and device bounds. This is intentional until extent system-zone validation, journal recovery, metadata writeback, and transaction coordination are integrated.

## Feature-by-feature comparison

| Area | CognacOS | Linux master | Compatibility consequence |
|---|---|---|---|
| Superblock | Validates magic, basic geometry, selected feature bits and CRC32C metadata checksum | Full feature negotiation, mount options, recovery state, checksums, block-size and feature interactions | Unsupported feature combinations are refused |
| Block geometry | 1 KiB through 64 KiB parsing; checked volume bounds | Block size must be compatible with kernel page size and feature constraints | 64 KiB directory record encoding and large-block constraints are not implemented in CognacOS |
| Group descriptors | 32/64-byte descriptors, 64-bit metadata pointers and metadata checksum | Also supports meta_bg, descriptor flags, uninitialized groups, flex_bg/system-zone validation | meta_bg and other unsupported layouts are rejected; metadata placement validation is incomplete |
| Inodes | Basic mode/size/flags, inode checksums, limited extra fields | Full inode flags, extra-isize semantics, huge-file accounting, xattrs, project IDs, timestamps, orphan state, encryption/verity/casefold interactions | Many valid inodes/features cannot be fully interpreted |
| Extents | Inline extent root lookup, external extent lookup with metadata checksum validation, inline insert/coalesce/truncate | Full B-tree validation, split/grow/merge, unwritten conversion, delayed allocation, fiemap, bigalloc, extent status cache and transaction integration | External-tree mutation and many extent semantics are missing |
| Legacy block maps | Direct/single/double/triple indirect lookup | Mature mapping, validation, allocation and journaling | Basic lookup exists; complete legacy mutation does not |
| Linear directories | Record validation, lookup, insertion, removal and checksum tail | Full namei operations, block growth, directory indexing and locking | Linear operations are partial |
| Indexed directories | Rejected when DIR_INDEX is present | HTree lookup/insert/split, dx checksum validation and depth handling | Many standard Linux ext4 directories are refused |
| Allocation | Bitmap bit selection, counters, checksums, simple inode/block allocate/free primitives | Metadata-aware multiblock allocator, reservations, preallocation, cluster/bigalloc, flex_bg, lazy init and system-zone protection | Not a safe or equivalent allocator; primitives are not mounted write operations |
| JBD2 | Legacy descriptor tags, 32/64-bit targets, revoke records, sequence checks and bounded commit/replay | Multi-pass scan/revoke/replay, checksum v1/v2/v3, async commit, fast commits, larger logs, robust checkpointing and transaction lifecycle | Only a narrow legacy journal subset; unsupported checksum/async/fast-commit formats are refused |
| Orphans | No mount-time orphan cleanup integration | Orphan list and orphan-file recovery | Crash-interrupted unlink/truncate lifecycle is incomplete |
| External journal | Rejected | Supported with device/UUID binding | Not supported |
| Data modes | One ordered-data commit primitive | journal, ordered and writeback modes with barriers and transaction ordering | No complete mode implementation or mount integration |

## Important read-compatibility gaps

### Common ext4 feature flags

The current parser deliberately supports only a small set of RO-compatible flags and rejects DIR_INDEX. Linux ext4 commonly encounters additional flags such as HUGE_FILE, DIR_NLINK, EXTRA_ISIZE, GDT_CSUM, BIGALLOC, quotas, project quotas, verity, and orphan-file state. Some are read-only-compatible in Linux, but CognacOS must implement their semantics before accepting them.

The current incompatible-feature policy also rejects layouts/features such as META_BG, encryption, inline data, casefold, large-directory extensions, and EA-inode support. This is safer than mounting a filesystem whose metadata is being misinterpreted.

### Directory formats

CognacOS handles linear directory blocks and the metadata checksum tail, but does not implement Linux HTree directories or dx root/node checksum semantics. A volume with the DIR_INDEX compatible feature is rejected. It must not be parsed as an ordinary linear directory.

### Extent and inode validation

CognacOS has checked external extent-node checksum helpers, but every read path must use the checked inode and extent APIs. The unchecked extent mapper intentionally refuses metadata_csum volumes. Extent physical-range checks currently establish filesystem bounds, not the complete Linux ext4 system-zone exclusion rules.

## Corruption-risk analysis

### Risks prevented by current policy

- Journaled writable mounts are not enabled.
- metadata_csum writable mounts are not enabled.
- Unknown incompatible features are rejected.
- JBD2 checksum-v2/v3, async-commit, fast-commit and now checksum-v1 journals are rejected.
- Uncommitted JBD2 transaction tags/revokes are discarded.
- Writable mounting is disabled until extent targets can be proven not to overlap superblocks, descriptor blocks, bitmaps, inode tables, journal blocks, or other system-zone metadata.

### Remaining risks / limits

- Read-only code must consistently use checksum-validated APIs; callers that bypass read_inode or map_extent_block_checked can bypass metadata integrity checks.
- The filesystem parser is not a full fsck. It does not repair corrupted metadata.
- The low-level mutation helpers operate on caller-owned buffers and do not provide atomicity by themselves.
- The JBD2 replay engine is bounded and does not yet implement all Linux transaction formats or all journal checksums. It must not be used on journals with unsupported features.
- There is no mount-level orphan cleanup, full journal recovery pipeline, or power-failure test suite covering every write boundary.

## Required milestones before writable ext4

1. [x] Implement bounded Linux-style system-zone construction for supported non-META_BG layouts; validate checked extent-tree child nodes and mapped data blocks against superblock, backup GDT, reserved GDT, bitmap, and inode-table zones. Mount integration and journal-block accounting remain open.
2. Implement full JBD2 checksum handling and the Linux scan/revoke/replay rules for the supported journal feature set; retain fail-closed rejection for formats not yet implemented.
3. Bind journal inode discovery, extent mapping, recovery, journal-superblock updates and recovery completion into mount.
4. Add a mount-owned transaction coordinator that captures every changed block, enforces ordered-data semantics, serializes writers, and guarantees rollback/recovery after interruption.
5. Integrate allocation, inode initialization, parent directory updates, timestamps, link counts, orphan state, extent-tree split/merge, and truncation into one transaction.
6. Implement HTree directories and checksum handling, then add support for common RO-compatible features one by one.
7. Test against Linux-generated images from multiple e2fsprogs versions, feature matrices, 1/2/4 KiB blocks, 32/64-bit descriptors, sparse/flex groups, and crash injection at every journal write/flush boundary. Verify images with Linux e2fsck -fn and cross-read/write them with Linux.

## Linux reference files

- [Linux ext4 superblock and mount implementation](https://github.com/torvalds/linux/blob/d89e1d186ec9271bf78e62771779e9fc03bf0fc3/fs/ext4/super.c)
- [Linux ext4 inode and block mapping](https://github.com/torvalds/linux/blob/d89e1d186ec9271bf78e62771779e9fc03bf0fc3/fs/ext4/inode.c)
- [Linux ext4 directory and HTree implementation](https://github.com/torvalds/linux/blob/d89e1d186ec9271bf78e62771779e9fc03bf0fc3/fs/ext4/namei.c)
- [Linux ext4 extent-tree implementation](https://github.com/torvalds/linux/blob/d89e1d186ec9271bf78e62771779e9fc03bf0fc3/fs/ext4/extents.c)
- [Linux ext4 block allocation](https://github.com/torvalds/linux/blob/d89e1d186ec9271bf78e62771779e9fc03bf0fc3/fs/ext4/balloc.c)
- [Linux JBD2 recovery implementation](https://github.com/torvalds/linux/blob/d89e1d186ec9271bf78e62771779e9fc03bf0fc3/fs/jbd2/recovery.c)
- [Linux ext4 on-disk feature definitions](https://www.kernel.org/doc/html/latest/filesystems/ext4/super.html)
- [Linux JBD2 journal format](https://www.kernel.org/doc/html/latest/filesystems/ext4/journal.html)

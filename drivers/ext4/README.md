# CognacOS ext4 driver: mutation and recovery status

This crate now contains bounded ext4 metadata mutation primitives, a bounded
JBD2 implementation, and a read-only mount facade that retains a validated
system-zone map. The primitives operate on caller-owned metadata buffers;
the filesystem mount facade does not yet expose a general-purpose writable
filesystem API.

## Implemented primitives

- Internal journal discovery from the ext4 superblock, including journal inode
  number and UUID binding. External journal devices are rejected.
- JBD2 v2 superblock parsing, legacy descriptor tags, optional 64-bit block
  numbers, revoke records, commit sequence validation, committed transaction
  replay, incomplete-tail handling, and recovery-start cleanup.
- Mount-owned JBD2 transactions stage bounded metadata/data images in caller-owned arenas, serialize writers through exclusive borrowing, validate metadata targets through a mount-supplied ownership policy, and reject system-zone data/revoke targets. A partition-bounded adapter checks every block I/O.
- Ordered-data metadata commits: ordered data is written and flushed first;
  metadata is logged; a commit record is flushed; home metadata is checkpointed;
  and only then is the journal marked clean.
- JBD2 revoke-record writing for block reclamation. Callers must serialize
  transactions and include all freed/reused blocks in the revoke list.
- Bitmap allocation/freeing with group and superblock counter updates.
- Inode initialization, directory block initialization, directory insertion,
  lookup and unlink, regular-file link-count updates, and inode checksum updates.
- Inline extent insertion/coalescing and truncation, with returned physical
  ranges for the caller to free in the same transaction.
- Checksum-validated superblock, group descriptor, inode, bitmap, directory,
  and external extent-tree read/mutation paths.
- Caller-bounded Linux-style system-zone construction for primary/backup
  superblocks, backup descriptor tables, reserved GDT blocks, block/inode
  bitmaps, and inode tables. Checked extent traversal rejects both child
  extent-tree nodes and final data blocks that overlap those zones. A separate
  reserved-inode pass protects internal journal inode extents and applies the
  Linux exception that permits the journal inode itself to access its ranges.
  The checked read-only mount validates the journal UUID/features and refuses
  dirty journals. A separate bounded `recover_internal_journal` path now maps
  the internal journal inode and runs committed-transaction replay through the
  writable block-device contract before the caller retries the read-only mount.

## Deliberate safety limits

The implementation rejects JBD2 checksum-v2/v3, async-commit and fast-commit
journals. The replay/commit path is bounded to 256 metadata tags and 256 revoke
records per replay/transaction. A transaction must fit one descriptor block.
The caller must provide correct journal-inode extent mapping and serialize all
writers. External extent-tree split/merge, indexed htree directories, external
journals, and journal checksum formats are not supported by these routines.

The new CheckedExtFilesystem mount facade builds and retains the system-zone map and invokes the reserved-journal-inode pass using caller-owned range and scratch buffers. The transaction coordinator is constructed by the mount manager with a MetadataTargetPolicy that must validate exact metadata ownership (superblock/GDT/bitmap/inode-table/directory/inode blocks) and must exclude the journal's own physical blocks from metadata targets. Transactions are serialized by exclusive borrowing and do not allocate memory. `recover_internal_journal` is a separate pre-mount recovery operation: it requires a writable partition-bounded BlockDevice, a complete caller-provided journal logical-to-physical map, and block-sized replay buffers. After successful recovery, mount again through CheckedExtFilesystem. The legacy ExtFilesystem facade remains available for compatibility but does not provide this checked mapping surface. The current ExtFilesystem mount facade rejects **all writable mounts**. This
is a deliberate safety interlock: the current data-write path does not yet
validate extent targets against the complete ext4 system-zone map, and the
journal/recovery and metadata mutation primitives are not integrated through
one mount-owned transaction coordinator. Enabling writable mounts before those
pieces are complete could overwrite filesystem metadata or leave inconsistent
metadata after a crash.

Read-only support is also a strict feature subset. In particular, indexed
HTree directories and several common Linux ext4 feature flags are not yet
implemented. See [COMPATIBILITY_AUDIT.md](COMPATIBILITY_AUDIT.md) for the
feature-by-feature comparison against Linux master, the corruption-risk review,
and the milestones required before writable mounts can be enabled.

## Validation

The GitHub Actions workflow runs the JBD2 parser/replay/commit tests and the
ext4 allocation, directory, inode, extent and checksum mutation tests as part
of the driver test matrix.
\n\n## Transaction coordinator boundary\n\nThe coordinator is a bounded commit primitive, not yet the writable VFS mount path. It permanently poisons its in-memory mount instance after any commit I/O error, preventing further transactions until recovery and remount. It requires the caller to provide validated journal-inode mapping, the retained system-zone map, a metadata-target policy tied to validated ext4 metadata ownership, and preallocated arenas. It currently supports the explicitly implemented legacy JBD2 format only. It does not implement transaction rollback in RAM after an I/O error; after the recovery-start pointer is durably published, any error must leave the mount offline and force journal recovery before reuse. It also does not yet provide mount-manager automatic recovery, orphan cleanup, atomic multi-inode operation builders, or Linux checksum-v1/v2/v3, async-commit, or fast-commit support. Writable mounting remains disabled until these integration and crash-injection requirements pass.\n
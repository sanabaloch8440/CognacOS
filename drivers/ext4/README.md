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

## JBD2 checksum-v2/v3 replay integration

The recovery scanner now recognizes the checksum-v2 16-bit tag layout and
checksum-v3 32-bit tag layout, validates the journal UUID on tags that carry
one, checks descriptor and revoke block tail CRC32C values, validates each
journaled data block before accepting its tag, and validates commit-block
checksum type/size and CRC32C before accepting a transaction. The v3 tag's
high-block field is parsed in its on-disk position. The ordered-commit writer emits checksum-v1 transaction CRC32 and checksum-v2/v3 tag, metadata-block, commit, and superblock CRC32C values. It also escapes payload blocks whose first word equals JBD2 magic.

Checksum-v1 transaction replay is now supported using Linux-compatible CRC32_BE over each descriptor block and its data blocks. Checksum-v1 ordered commits now emit the transaction CRC32_BE; checksum-v2/v3 journal writing remains unsupported.
The implementation must still be exercised against Linux-generated journal
images and broader malformed-input/crash-recovery tests before enabling
writable ext4 mounts. Any checksum failure aborts replay before home-block
writes are issued for that transaction.

## JBD2 checksum implementation status

The crate now contains allocation-free IEEE CRC-32 (reflected and big-endian) and CRC32C primitives,
streamable raw-state updates, ext4 inode checksum seed derivation, and JBD2
checksum-v1 transaction plus checksum-v2/v3 helper routines for UUID-derived seeds, tag data, descriptor/
revoke tails, and commit blocks. Unit tests cover standard CRC vectors,
sequence/payload binding, and corruption detection.

These are checksum engines and verification helpers, not yet end-to-end
journal validation: the replay scanner does not yet parse the v2/v3 tag
layouts or invoke these verifiers for every descriptor, revoke, tag payload,
and commit. Checksum-v1's transaction-wide checksum stream is also not
implemented. Consequently the journal superblock parser continues to reject
all checksum-v1/v2/v3 journals. This fail-closed behavior is intentional;
adding primitives must not be mistaken for enabling checksum-protected
replay or writable mounts.

## Deliberate safety limits

The implementation rejects async-commit and fast-commit journals. The replay/commit path is bounded to 256 metadata tags and 256 revoke
records per replay/transaction. A transaction must fit one descriptor block.
The caller must provide correct journal-inode extent mapping and serialize all
writers. External extent-tree split/merge, complete HTree mutation orchestration, external
journals, and full writable-mount integration are not supported by these routines.

The new CheckedExtFilesystem mount facade builds and retains the system-zone map and invokes the reserved-journal-inode pass using caller-owned range and scratch buffers. The transaction coordinator is constructed by the mount manager with a MetadataTargetPolicy that must validate exact metadata ownership (superblock/GDT/bitmap/inode-table/directory/inode blocks) and must exclude the journal's own physical blocks from metadata targets. Transactions are serialized by exclusive borrowing and do not allocate memory. `recover_internal_journal` is a separate pre-mount recovery operation: it requires a writable partition-bounded BlockDevice, a complete caller-provided journal logical-to-physical map, and block-sized replay buffers. After successful recovery, mount again through CheckedExtFilesystem. The legacy ExtFilesystem facade remains available for compatibility but does not provide this checked mapping surface. The current ExtFilesystem mount facade rejects **all writable mounts**. This
is a deliberate safety interlock: the current data-write path does not yet
validate extent targets against the complete ext4 system-zone map, and the
journal/recovery and metadata mutation primitives are not integrated through
one mount-owned transaction coordinator. Enabling writable mounts before those
pieces are complete could overwrite filesystem metadata or leave inconsistent
metadata after a crash.

Read-only support is also a strict feature subset. DIR_INDEX is accepted and has Linux-compatible legacy, half-MD4, TEA and SipHash name hashing; checksum-validated root/node/leaf reads; collision continuation across sibling index nodes; direct filename comparison; and indexed leaf enumeration including `.` and `..`. The mutation module provides leaf splitting, dx-entry insertion, interior-node splitting and depth-zero root growth as caller-owned buffer primitives. These are not yet wired into a mount-owned, crash-safe VFS/JBD2 operation, so writable mounts remain disabled. See [COMPATIBILITY_AUDIT.md](COMPATIBILITY_AUDIT.md) for exact limits.

## Validation

The GitHub Actions workflow runs the JBD2 parser/replay/commit tests and the
ext4 allocation, directory, inode, extent and checksum mutation tests as part
of the driver test matrix.


## Transaction coordinator boundary

The coordinator is a bounded commit primitive, not yet the writable VFS mount path. It permanently poisons its in-memory mount instance after any commit I/O error, preventing further transactions until recovery and remount. It requires the caller to provide validated journal-inode mapping, the retained system-zone map, a metadata-target policy tied to validated ext4 metadata ownership, and preallocated arenas. It currently supports legacy JBD2 and checksum-v1 replay formats; checksum-enabled writing and v2/v3 writing remain unsupported. It does not implement transaction rollback in RAM after an I/O error; after the recovery-start pointer is durably published, any error must leave the mount offline and force journal recovery before reuse. It also does not yet provide mount-manager automatic recovery, orphan cleanup, atomic multi-inode operation builders, or checksum-enabled journal writing, async-commit, or fast-commit support. Writable mounting remains disabled until these integration and crash-injection requirements pass.


### JBD2 recovery target isolation

The ext4 recovery path now rejects committed journal tags that point back into the internal journal inode's physical extents. This check runs before each home-block write and is covered by a regression test. It prevents journal self-overwrite but is not a substitute for per-object metadata ownership checks or JBD2 checksums. Writable mounting remains disabled.


`CheckedExtFilesystem::mount_with_recovery` is the checked entry point for filesystems with an internal journal: it constructs the system-zone map, protects journal extents, performs bounded replay with journal self-overwrite protection, then mounts only after the journal is clean. This is still a restricted JBD2 implementation: checksum-v1/v2/v3 replay and bounded ordered writing are implemented; async-commit and fast-commit formats fail closed. Writable mounts remain disabled pending crash-injection and Linux interoperability validation.


Mounted mutation code should use `ops::allocate_block_checked` and `ops::free_block_checked`: allocation skips bitmap-free blocks that overlap the retained system-zone map, while freeing rejects protected metadata and journal extents. The original unchecked primitives remain lower-level helpers and are not safe substitutes for the checked APIs.


## Atomic operation and orphan recovery interfaces

The mount-owned transaction layer now exports a typed OperationBuilder for Create, Unlink, Resize, Allocate and Free. Each operation requires a manifest of exact metadata object classes before commit; Free also requires a revoke record. The builder uses MetadataOwnershipMap, an exact per-block allowlist containing owner inode, generation and metadata class. Policy-validated journal recovery can use the same map and separately excludes the journal's own extents.

The orphan module validates the legacy orphan chain with bounded caller storage and submits all inode repairs plus the cleared superblock head through one transaction callback. The callback must implement the correct linked-inode truncate and zero-link inode reclamation behavior; the module does not guess at extent/bitmap changes.

These are foundations for wiring the actual VFS mutation paths, not proof that all operations are already connected. JBD2 checksum-v1/v2/v3 replay is supported with transaction, superblock, metadata-block, tag-data, UUID, and commit validation. Mount-time orphan recovery is not yet connected to a concrete transaction adapter. Writable mounts remain disabled.


## JBD2 checksum validation

The bounded recovery parser validates the JBD2 v2/v3 journal superblock before trusting its geometry or recovery pointer: checksum type must be CRC32C and the complete 1024-byte superblock CRC32C is checked with the checksum field zeroed. Descriptor/revoke tails, per-tag data checksums, UUIDs, and commit checksums are also validated before replay. Async commit and fast commits remain unsupported and fail closed. Bounded checksum-v1/v2/v3 ordered writing is implemented; writable ext4 mounts remain disabled.


## JBD2 checksum-v1 replay (2026-10-30)

Checksum-v1 journals are no longer rejected. The replay scanner computes Linux's big-endian CRC-32 raw state from `~0`, covering each complete descriptor block followed by every associated journal data block in transaction order, and compares it with commit `h_chksum[0]` when the commit header identifies CRC32/size 4. Deleted tags still consume and checksum their following data block, matching Linux's tag-stream accounting. Corrupt descriptor or payload bytes fail the transaction with `InvalidChecksum` before replay writes. The ordered commit writer emits checksum-v1 transaction CRC and checksum-v2/v3 descriptor/revoke/tag/commit/superblock checksums. Async commit and fast commit remain unsupported.


### JBD2 checksum-v1 commit writer

The bounded ordered-commit path now supports checksum-v1 journals. It calculates CRC32_BE over the complete descriptor block and associated data blocks in the same order as Linux JBD2, writes type `JBD2_CRC32_CHKSUM` (1), size 4, and the raw checksum into the commit header, and still excludes revoke blocks from that checksum stream. The existing durability order is retained: ordered data flush, recovery-start publication, descriptor/data/revoke records, commit flush, home checkpoint flush, then clean-superblock publication. This is a bounded commit primitive, not authorization to enable writable mounts; crash-injection and Linux interoperability coverage are still required.


## JBD2 checksum-v2/v3 ordered writer and payload escaping

The ordered writer now emits the Linux on-disk v2 and v3 tag layouts, calculates the per-data-block tag checksum over the journal-stored payload, writes descriptor and revoke tail CRC32C values, writes commit CRC32C, and refreshes the journal-superblock CRC32C whenever `s_start` or `s_sequence` changes. It handles the v3 tag's fixed 16-byte base layout and the v2 tag's optional 64-bit high block field. Payloads beginning with the JBD2 magic word are zero-escaped, marked with TAG_ESCAPE, checksummed in escaped form, and restored during replay. End-to-end tests write v2/v3 journals, validate descriptor/revoke/commit/tag checksums, and recover the generated log. The superblock checksum is computed over the fixed 1024-byte `journal_superblock_t`, even when the journal block size is larger. This remains a bounded primitive; writable ext4 mounting stays disabled until every VFS mutation path uses the transaction coordinator and crash-injection/interoperability testing passes.

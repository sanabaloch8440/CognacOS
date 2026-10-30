# ext4 compatibility and safety audit

**Audit date:** 2026-10-30  
**Linux reference:** torvalds/linux master at d89e1d186ec9271bf78e62771779e9fc03bf0fc3  
**CognacOS scope:** drivers/ext4/src/lib.rs, ops.rs, htree.rs, hash.rs, htree_mut.rs, and jbd2.rs

This is a source-level compatibility and safety audit, not a claim that CognacOS is a drop-in implementation of Linux ext4. Linux ext4 is a large filesystem implementation with extensive mount-time feature negotiation, system-zone validation, journaling, recovery, concurrency, and crash testing.

## Executive result

- **Read-only:** CognacOS can read a deliberately limited subset of ext2/ext3/ext4 layouts. DIR_INDEX is accepted for read-only parsing. Linux-compatible legacy/half-MD4/TEA signed/unsigned selection from superblock s_flags, SipHash, checksum-validated dx traversal, collision continuation across sibling nodes, exact prepared-name comparison and indexed leaf enumeration are implemented. SipHash requires an explicit fscrypt key. Buffer-level leaf/index split and root-growth primitives exist but are not wired into a crash-safe VFS transaction.
- **Writable:** Writable mounting is now fail-closed. The previous facade could write through an extent mapping without validating that the target physical blocks were outside all ext4 system-zone metadata. It also lacked a mount-owned transaction coordinator. That was not a safe general-purpose writable mount.
- **Recovery:** The bounded JBD2 replay implementation had a correctness defect: descriptor tags and revokes from a transaction were accumulated before its commit record was seen. An incomplete final transaction could therefore leak uncommitted metadata into the replay pass. This has been fixed, with regression tests.
- **Compatibility:** Complete Linux ext4 compatibility is not achieved. Unsupported features are rejected rather than guessed at or silently interpreted.

## System-zone milestone (2026-10-30)

CognacOS now has a caller-bounded system-zone builder in `system_zone.rs`. It derives backup-superblock placement from SPARSE_SUPER/SPARSE_SUPER2, computes primary and backup GDT spans, accounts for reserved GDT blocks, reads and checksum-validates each primary group descriptor, and records block bitmap, inode bitmap, and inode-table ranges. The checked extent traversal variant validates both external extent-tree child blocks and final mapped data blocks against this map. A reserved-inode pass also maps the internal journal inode, records its physical blocks as protected ranges, and provides the Linux-style exception for that inode's own accesses. It fails closed when caller-provided range storage is exhausted or metadata geometry/checksums are invalid.

The CheckedExtFilesystem read-only mount facade owns the caller-backed zone range set for the mount lifetime, builds it during mount, adds internal journal inode extents with the reserved-inode exception, validates the journal UUID and supported feature set, and refuses mounts whose journal start pointer indicates pending recovery. The new `recover_internal_journal` pre-mount operation maps the internal journal inode into a bounded caller-provided logical-to-physical table and connects JBD2's committed-transaction replay to a partition-bounded BlockDevice adapter. It flushes replayed home blocks and the updated journal superblock before returning; callers must then retry CheckedExtFilesystem::mount. The older ExtFilesystem facade remains for compatibility; allocation helpers and metadata mutation still do not consume the map. Writable mounting therefore remains disabled.

## Changes made during this audit

1. JBD2 replay now snapshots the tag/revoke array positions at the start of each transaction. If the transaction is malformed or has no matching commit block, both positions are rolled back before replay proceeds. This ensures the replay pass sees only records from committed transactions.
2. JBD2 superblock parsing now rejects the legacy checksum-v1 COMPAT feature. CognacOS does not implement its transaction-wide checksum verification; silently accepting this feature would weaken journal integrity validation.
3. JBD2 descriptor tags now reject reserved flag bits rather than interpreting unknown tag semantics.
4. Writable mounting now returns an unsupported-format error after validating the superblock and device bounds. This is intentional until extent system-zone validation, journal recovery, metadata writeback, and transaction coordination are integrated.

## JBD2 checksum-v2/v3 recovery (2026-10-30)

The replay path now selects checksum-v2/v3 from journal superblock feature
bits and validates descriptor/revoke metadata tails, UUID-bearing tags, every
non-deleted tag's data checksum, and the commit block checksum type, size, and
CRC32C before accepting the transaction. Both v2 and v3 tag layouts have
dedicated end-to-end replay tests, including data/tag corruption. Checksum
metadata CRC calculation includes the full block with the tail field treated
as zero. The commit writer explicitly refuses checksum-enabled journals so
it cannot emit malformed v2/v3 transactions.

**Remaining gaps:** checksum-v1 transaction-wide CRC32 replay is now implemented;
superblock checksum validation and checksum-enabled journal writing are not
implemented; Linux-generated image interoperability and crash-injection
coverage are still required. Do not enable writable ext4 mounts based on this
recovery milestone alone.

## JBD2 checksum-engine milestone (2026-10-30)

Added standalone CRC-32/IEEE and CRC32C/Castagnoli raw-state and finalized
implementations, ext4 inode checksum-seed derivation, and JBD2 checksum-v2/v3
helpers for UUID-derived seeds, tag payload checksums, descriptor/revoke
metadata tails, and commit-block checksums. Regression tests include the
standard `123456789` vectors and deliberate data/metadata corruption.

**This does not enable checksum journals.** The current replay parser still
rejects checksum-v1/v2/v3 because the scanner has not yet been wired to decode
the different on-disk tag layouts and verify every record before accepting a
transaction. In particular, no checksum-protected descriptor, revoke, commit,
or tag is currently replayed. The feature comparison below remains a statement
about end-to-end replay, not availability of standalone checksum helpers.

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
| Indexed directories | Linux legacy/half-MD4/TEA/SipHash hashes; dx root/node and leaf checksums; collision traversal; exact name comparison; indexed enumeration | Linux readdir cookie encoding, fscrypt/casefold preparation, locking, recursive split orchestration and transaction integration | Read lookup/enumeration and low-level split/grow primitives exist; writable HTree integration remains disabled |
| Allocation | Bitmap bit selection, counters, checksums, simple inode/block allocate/free primitives | Metadata-aware multiblock allocator, reservations, preallocation, cluster/bigalloc, flex_bg, lazy init and system-zone protection | Not a safe or equivalent allocator; primitives are not mounted write operations |
| JBD2 | Legacy descriptor tags, 32/64-bit targets, revoke records, sequence checks and bounded commit/replay | Multi-pass scan/revoke/replay, checksum v1/v2/v3, async commit, fast commits, larger logs, robust checkpointing and transaction lifecycle | Only a narrow legacy journal subset; unsupported checksum/async/fast-commit formats are refused |
| Orphans | Bounded legacy s_last_orphan/i_dtime chain validation and transaction callback; no concrete cleanup adapter | Legacy orphan list plus newer orphan-file recovery, linked-inode truncation and zero-link deletion | Legacy chain planning exists, but mount integration, real truncate/reclaim operations, and orphan-file format/checksum support are missing |
| External journal | Rejected | Supported with device/UUID binding | Not supported |
| Data modes | One ordered-data commit primitive | journal, ordered and writeback modes with barriers and transaction ordering | No complete mode implementation or mount integration |

## Important read-compatibility gaps

### Common ext4 feature flags

The current parser deliberately supports only a small set of RO-compatible flags. DIR_INDEX is a compatible feature and is now accepted because Linux-style HTree hashing, candidate selection, collision traversal and name comparison are implemented. Linux ext4 commonly encounters additional flags such as HUGE_FILE, DIR_NLINK, EXTRA_ISIZE, GDT_CSUM, BIGALLOC, quotas, project quotas, verity, and orphan-file state. Some are read-only-compatible in Linux, but CognacOS must implement their semantics before accepting them.

The current incompatible-feature policy also rejects layouts/features such as META_BG, encryption, inline data, casefold, large-directory extensions, and EA-inode support. This is safer than mounting a filesystem whose metadata is being misinterpreted.

### Directory formats

CognacOS now parses the ext4 dx_root overlay and bounded dx_node levels, validates root/node dx_tail checksums using the inode checksum seed, validates count/limit geometry, and uses Linux's rightmost-separator-not-greater-than-hash selection rule. It accepts the DIR_INDEX compatible feature and must route indexed directories through this path rather than treating block zero as a linear leaf. `hash.rs` implements all six legacy/half-MD4/TEA variants and SipHash-2-4; SipHash requires a supplied fscrypt key. `ops::lookup_indexed_directory_entry` is the VFS-facing lookup entry point; it derives hash seed, signedness and inode checksum seed from the superblock. It calls `lookup_htree_name`, which compares the prepared disk name and follows same-major continuation entries across sibling index nodes. `iterate_htree_directory` validates leaves and enumerates dot entries plus leaf entries with major/minor hashes; the VFS must sort hashes to reproduce Linux readdir ordering. `htree_mut.rs` provides leaf split, dx insertion, interior-node split and depth-zero root growth. These operate on caller-owned buffers and require one JBD2 transaction, allocation, inode updates and recursive parent splitting before they can be enabled. Large-directory depth extensions remain rejected.

### Extent and inode validation

CognacOS has checked external extent-node checksum helpers, but every read path must use the checked inode and extent APIs. The unchecked extent mapper intentionally refuses metadata_csum volumes. Extent physical-range checks currently establish filesystem bounds, not the complete Linux ext4 system-zone exclusion rules.

## Corruption-risk analysis

### Risks prevented by current policy

- Journaled writable mounts are not enabled.
- metadata_csum writable mounts are not enabled.
- Unknown incompatible features are rejected.
- JBD2 checksum-v2/v3 replay is checksum-validated; checksum-v1, async-commit, and fast-commit journals are rejected.
- Uncommitted JBD2 transaction tags/revokes are discarded.
- Writable mounting is disabled until extent targets can be proven not to overlap superblocks, descriptor blocks, bitmaps, inode tables, journal blocks, or other system-zone metadata.

### Remaining risks / limits

- Read-only code must consistently use checksum-validated APIs; callers that bypass read_inode or map_extent_block_checked can bypass metadata integrity checks.
- The filesystem parser is not a full fsck. It does not repair corrupted metadata.
- The low-level mutation helpers operate on caller-owned buffers and do not provide atomicity by themselves.
- The JBD2 replay engine is bounded and does not yet implement all Linux transaction formats or all journal checksums. It must not be used on journals with unsupported features.
- There is no mount-level orphan cleanup, full journal recovery pipeline, or power-failure test suite covering every write boundary.

## Required milestones before writable ext4

1. [x] Implement bounded Linux-style system-zone construction for supported non-META_BG layouts; retain it in CheckedExtFilesystem, protect internal journal inode extents, and validate checked extent-tree child nodes and mapped data blocks. The new checked block allocator skips free bitmap bits that overlap the retained map, and the checked free path rejects system-zone blocks. Existing unchecked allocation/free primitives remain for low-level tests and must not be used by mounted mutation paths.
2. [~] The committed-transaction scan/revoke/replay path now supports an ext4-specific replay target policy and rejects writes into the journal's own extents. Still required: Linux-compatible revoke/checksum corner cases and exact metadata ownership validation; unsupported formats remain fail-closed.
3. [~] Internal journal discovery, extent mapping, committed replay, journal-self-overwrite protection, journal-superblock update, and flush are connected to `CheckedExtFilesystem::mount_with_recovery`. Crash-injection validation is still required before treating recovery as production-ready.
4. [~] Add a bounded mount-owned transaction coordinator with caller-backed staging, metadata-target policy, system-zone checks for ordered data/revokes, exclusive writer serialization, and partition-bounded I/O. Remaining: wire every filesystem operation through it, make mount-manager recovery automatic, add orphan handling, and prove failure/restart behavior with crash injection.
5. Integrate allocation, inode initialization, parent directory updates, timestamps, link counts, orphan state, extent-tree split/merge, and truncation into one transaction.
6. [~] HTree hashes, collision-aware lookup, leaf enumeration and buffer-level split/grow primitives are implemented; next wire recursive index splits, VFS cookie ordering, allocation and directory mutations into the mount-owned JBD2 coordinator.
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


## JBD2 replay-target isolation milestone (2026-10-30)

JBD2 replay now exposes a target-policy hook that is invoked for each committed, non-revoked home-block write. The internal-journal recovery path supplies a policy that rejects any target whose physical block is part of the journal inode's own mapped extent set. This prevents a corrupted descriptor from directing replay into the log that is still being scanned. A regression test proves the target is rejected before the home block is modified. The generic JBD2 API retains a filesystem-bounds-only policy for non-ext4 users; ext4 recovery always uses the stricter journal-isolation policy.

This does not yet establish that every replay target is a valid ext4 metadata object. JBD2 checksum-v1/v2/v3 replay and bounded ordered writing are now implemented; async-commit and fast-commit formats remain rejected. Replay target validation against exact inode/bitmap/directory metadata ownership remains required before writable mounting.

## Transaction coordinator milestone (2026-10-30)

Implemented `drivers/ext4/src/transaction.rs` and exported its API. The coordinator is mount-owned by construction (it borrows the device, journal map, journal superblock, zone map, and metadata policy for its lifetime); exclusive mutable borrowing serializes transactions. Metadata/data images are copied into caller-owned fixed-capacity arenas. Duplicate and cross-class target reuse is rejected. Ordered data and revoke targets must pass the retained system-zone map. Metadata targets require a policy callback that validates the exact metadata class and must reject the journal's own physical blocks. `FilesystemBlockAdapter` enforces block-size, filesystem-range, partition-capacity, and flush contracts.

This milestone does **not** enable writable mounting. The commit engine supports bounded legacy and checksum-v1/v2/v3 ordered transactions, and any commit I/O error poisons the in-memory coordinator so it refuses further transactions until recovery and remount. Remaining work includes wiring all inode/extent/directory/bitmap/orphan operations into one transaction builder, robust abort/rundown state, and crash injection at every write/flush boundary. No claim of Linux-equivalent crash safety is made until those tests pass.


## Atomic mutation, orphan recovery and metadata ownership milestone (2026-10-30)

This milestone adds the following bounded primitives:

- `MetadataOwnershipMap` is an exact per-physical-block allowlist carrying owner inode, generation and metadata object kind. Duplicate block ownership is rejected. It implements both transaction metadata-target validation and JBD2 replay-target validation.
- `recover_internal_journal_with_policy` allows the mount layer to supply that exact ownership policy. The recovery wrapper independently rejects any target in the internal journal inode's physical extents before consulting the caller policy.
- `OperationBuilder` introduces typed Create, Unlink, Resize, Allocate and Free transaction manifests. It requires the appropriate directory/inode/bitmap/group-descriptor/superblock/extent metadata classes before committing. Free operations additionally require at least one revoke record. Metadata images must match the exact class in the ownership map.
- `orphan::recover_orphan_chain` validates the legacy s_last_orphan / i_dtime chain with caller-provided cycle-detection storage, stages every orphan and the cleared superblock head through one transaction callback, refreshes the superblock checksum when metadata_csum is enabled, and commits once. The transaction adapter must implement the actual linked-inode truncate versus zero-link inode reclaim semantics.

Important boundaries: these APIs are integration surfaces, not a completed VFS operation implementation. No in-tree adapter currently translates all existing create/unlink/resize/allocate/free primitives into complete operation manifests, and no default mount-time orphan transaction adapter has been wired. The caller must build the exact metadata ownership allowlist from validated filesystem objects before using policy-validated recovery. JBD2 checksum-v1/v2/v3 replay and bounded ordered writing are implemented with transaction/superblock, descriptor/revoke, tag-data, UUID, and commit validation. Writable mounts remain disabled.

The next validation stage must include fault injection after every ordered-data write, journal-start publication, descriptor/data/revoke write, commit write/flush, home checkpoint and clean-superblock publication. The orphan transaction tests must cover linked truncate, zero-link reclaim, cyclic/corrupt lists, inode-generation reuse and failure/replay. Cross-validation must use Linux-generated images and e2fsck.


### Linux orphan-format compatibility note

Linux master supports both the legacy singly linked orphan list and the newer orphan-file feature. The new CognacOS orphan module currently handles only the legacy s_last_orphan/i_dtime chain. The ext4 orphan-file inode, per-block magic/checksum validation, ORPHAN_PRESENT mount-state handling and orphan-file cleanup are not implemented; those feature flags must continue to be rejected. Linux's orphan cleanup also distinguishes linked inodes (truncate to i_size) from zero-link inodes (delete/reclaim), which the CognacOS transaction callback contract requires but does not itself implement.


## JBD2 superblock checksum validation (2026-10-30)

JBD2 checksum-v2/v3 superblock parsing now verifies the checksum type is CRC32C and validates the CRC32C over the complete 1024-byte journal superblock with the checksum field at offset 0xfc zeroed. A checksum mismatch returns `InvalidChecksum` before recovery can trust the superblock's ring geometry, sequence, UUID, or start pointer. Regression coverage builds a valid v3 superblock, confirms parsing, then corrupts a protected byte and confirms rejection. Async commit and fast commit remain unsupported. Writable mounts remain disabled.


## JBD2 checksum-v1 transaction replay (2026-10-30)

Checksum-v1 is now accepted by the journal superblock feature decoder and verified end-to-end during replay. The implementation follows Linux `fs/jbd2/recovery.c`: it initializes the transaction CRC state to all ones, folds in each complete descriptor block and every following data block in log order using non-reflected CRC-32/IEEE (`crc32_be`), then checks the commit header's CRC32 type, size 4, and checksum value. Revoke blocks and the commit block itself are not included. Deleted tags still consume their data block and include it in the CRC stream. New tests cover a valid transaction, descriptor corruption, payload corruption, and deleted-tag stream alignment. This is intentionally fail-closed on a bad/missing checksum rather than accepting an unverified transaction. Linux also has compatibility handling for an all-zero checksum header on some older transactions; CognacOS currently requires the checksum header and value to be present when the v1 feature is enabled, which is stricter and may reject those legacy journals.

**Remaining JBD2 gaps:** async commit and fast commit; Linux-style multi-pass scan/revoke/replay edge cases and commit-time stale-log discrimination; full external journal binding; broader Linux-generated image and power-loss tests. Writable ext4 mounts remain disabled.


## JBD2 checksum-v1 commit writer (2026-10-30)

The bounded ordered-commit writer now accepts checksum-v1 journals and emits the Linux-compatible transaction checksum: CRC32_BE raw state initialized to `~0`, updated with the complete descriptor block and each associated data block, then stored in commit `h_chksum[0]` with checksum type 1 and size 4. Revoke blocks are excluded, matching Linux `calc_chksums()` in `fs/jbd2/recovery.c`. A writer regression test recomputes the checksum from the emitted journal blocks and compares the commit header. Checksum-v2/v3 ordered writing is now implemented, including tag layouts, per-tag data checksums, descriptor/revoke tails, commit checksums, superblock checksum refresh, and escaped payloads. This does not enable writable ext4 mounts; operation-to-transaction integration and power-loss fault injection are still missing.


## JBD2 checksum-v2/v3 ordered commit writer (2026-10-30)

The bounded ordered writer now emits Linux-format checksum-v2/v3 journal records: v2 16-bit tags with optional high block numbers; v3 fixed 16-byte tags; UUID elision after the first tag; per-tag CRC32C over transaction sequence plus the stored (escaped when necessary) data block; descriptor and revoke block tail CRC32C; commit CRC32C; and refreshed journal-superblock CRC32C after both recovery-start publication and clean-sequence publication. The fixed-size 1024-byte `journal_superblock_t` checksum is now calculated correctly even when the journal's logical block size is 4 KiB or larger. `TAG_ESCAPE` is emitted when a payload begins with JBD2 magic; the on-disk block is zero-escaped and replay restores the magic word. Regression tests exercise v2 and v3 ordered writes through recovery, including checksummed revoke records and escaped payloads.

**Safety boundary:** this does not enable writable ext4 mounts. The writer is still a bounded one-descriptor transaction primitive, does not implement async commit or fast commit, and has not yet passed crash injection at every durability boundary or Linux/e2fsprogs image interoperability testing. Exact metadata ownership and all VFS operation paths still need to be wired to the mount-owned coordinator.


## HTree indexed-directory lookup milestone (2026-10-30)

The ext4 superblock parser now accepts the compatible DIR_INDEX feature. A
new `htree.rs` module validates the `dx_root` dot/dotdot overlay and root
info, enforces count/limit bounds, verifies metadata_csum dx_tail checksums
using the inode-specific checksum seed, traverses the supported single
interior-node level, and selects the rightmost hash separator not greater than
the supplied query hash. It also exposes conventional directory-leaf checksum
validation. Tests cover root selection, interior-node traversal, corrupted
root/node checksums, invalid counts, unsupported depth and leaf checksum
corruption.

This is a candidate-leaf lookup primitive, not complete filename lookup:
the VFS caller must compute a hash matching the root's hash_version, compare
names in the returned leaf, and continue into adjacent collision leaves when
required. SIPHASH requires the directory's supported hash-in-dirent semantics;
hashing is not implemented here. HTree insertion/splitting and writable
mounts remain unsupported.


## HTree hashing, collision lookup, readdir and split primitives (2026-10-30)

Linux `fs/ext4/hash.c` algorithms are implemented for legacy, half-MD4 and
TEA signed/unsigned variants plus SipHash-2-4. CI checks the Linux KUnit
reference vectors (including seeded and non-ASCII inputs) and standard
SipHash reference vectors. SipHash requires the per-directory fscrypt key;
CognacOS never substitutes the superblock hash seed for that secret.

The HTree reader retains its index path, follows same-major continuation
leaves across sibling nodes, validates each index and leaf checksum, and
compares exact prepared disk names. Indexed enumeration includes dot entries
and returns major/minor hashes; the VFS must sort them to provide Linux-style
readdir cookies/order.

The mutation layer provides compact leaf splitting, dx separator insertion,
interior-node splitting and depth-zero root growth. These are caller-owned
buffer primitives, not an enabled writable path. The caller must allocate
blocks, update the directory inode and counters, route the new entry into the
correct split leaf, recursively split full parents, and stage all changes in
one JBD2 transaction. Large-directory depth extensions and writable HTree
mount integration remain unsupported.



The HTree layer also exposes `iterate_htree_directory_unique`, which validates the root and each traversed index node, checks dx/leaf checksums, tracks every visited leaf in caller-owned bounded storage, and rejects duplicate pointers even when the duplicated leaf contains no live entries. Capacity exhaustion fails closed. Regression tests cover duplicate empty-leaf references and undersized tracking storage. This is a validated enumeration primitive; the namespace graph validator is still not yet wired to this iterator, so indexed directories remain rejected by the recovery namespace-discovery mount path.

## Mount-state validation update (2026-10-30)

The common mount-state gate now rejects `EXT4_ERROR_FS`, `EXT4_ORPHAN_FS`, and nonzero `s_last_orphan` before the filesystem is exposed. This closes a gap in which a clean journal pointer could otherwise be mistaken for proof that all filesystem-level recovery work was complete. Journal replay and orphan cleanup are separate responsibilities; CognacOS still rejects orphaned volumes until the actual truncate/reclaim adapter is wired through a metadata transaction. A dirty-but-error-free journaled volume is not rejected solely because `EXT4_VALID_FS` is clear: it must still pass the explicit journal recovery path. This is a mount-safety improvement, not a writable-mount milestone.


## Recovery authorization gate (2026-10-30)

The no-policy `recover_internal_journal` convenience entry point no longer authorizes home-block replay using filesystem bounds alone. It can verify an already-clean journal, but dirty journals require `recover_internal_journal_with_policy` and a validated exact metadata-ownership allowlist. The no-policy `mount_with_recovery` wrapper consequently rejects dirty journals; callers must perform policy-authorized recovery and then retry the checked mount. This deliberately closes the unsafe default-policy path rather than weakening target validation.


## JBD2 transaction preflight hardening

The JBD2 ordered-commit writer now validates the complete home-target set before
performing I/O. Metadata targets, ordered-data targets, and revoke targets must
be unique and mutually disjoint. It also preflights the physical journal
superblock and every log slot needed by the transaction, rejecting out-of-range
mappings, journal/home-block aliases, journal-superblock aliases, and duplicate
physical mappings within the transaction. These checks happen before ordered
data or journal writes, so malformed target lists and extent maps fail without
partially modifying the device.

This is transaction-integrity hardening, not a claim of full Linux JBD2
compatibility. Async commit, fast commits, external journals, and a persistent
multi-transaction checkpointing daemon remain outside the supported feature
set. The mount layer must still provide exact metadata ownership policy and
keep the filesystem offline after uncertain I/O failures.


## Failure-atomic metadata operation staging

The mutation layer now provides caller-buffered staging wrappers for regular-file
create, unlink, and inline-extent resize. Each wrapper computes changes against
separate inode/directory/freed-range buffers and publishes them to the caller's
live buffers only after all operation-level validation and checksum updates
succeed. This prevents a failed second half of a multi-object operation from
leaving the caller's in-memory inode or directory image partially modified.

These wrappers do not themselves commit storage. Callers must stage every
resulting metadata image, bitmap, group descriptor, superblock and any ordered
data/revoke records in one mount-owned JBD2 transaction before reporting the
operation durable. Last-link unlink still requires extent and inode reclamation
in that same transaction; external extent-tree mutation remains unsupported.


## Conservative mount-derived metadata replay ownership

Added `build_system_metadata_ownership`, a bounded caller-buffered builder that re-reads the primary group descriptor table, validates descriptor checksums when enabled, validates descriptor geometry against the retained system-zone map, and records exact ownership classes for the primary superblock, primary GDT blocks, per-group block/inode bitmaps, and inode-table blocks. The resulting `MetadataOwnershipMap` can be passed to `recover_internal_journal_with_policy`; duplicate ownership, out-of-range blocks, non-system-zone targets, and insufficient caller capacity fail closed.

This is intentionally a conservative first-stage manifest. Backup superblocks/GDTs and inode-owned directory, extent-tree, xattr, and other metadata blocks are not authorized by this builder. A dirty journal transaction targeting any omitted block is rejected until a separate validated inode/object scanner adds an exact owner record. This does not enable writable mounting or claim complete Linux ext4 replay-target coverage.


## Mount-owned conservative journal recovery entry point

Added `CheckedExtFilesystem::mount_with_recovery_and_ownership`. It constructs the system-zone map, protects the internal journal inode extents, builds the validated primary-system-metadata ownership manifest, wraps it in the duplicate-rejecting `MetadataOwnershipMap`, and supplies that policy to JBD2 replay before returning a checked mount. The ownership-entry arena is caller-provided and bounded. The older `mount_with_recovery` remains fail-closed for dirty journals because it has no ownership arena.

Replay remains intentionally limited to primary superblock/GDT, per-group bitmaps, and inode-table blocks. Transactions that update directories, extent nodes, xattrs, or backup metadata are rejected until validated inode/object ownership is added. This entry point is not a general-purpose writable mount and does not imply full ext4/JBD2 compatibility.


## Validated directory-block replay ownership

Added `add_directory_metadata_ownership` and integrated it into `mount_with_recovery_and_ownership`. The mount caller supplies the directory inode numbers it has discovered (for example, from a validated namespace scan). Each inode is fetched through the checked inode-table path, its inode checksum is validated when metadata_csum is enabled, and it must parse as a directory. Every logical directory block is mapped through checked extent/legacy mapping with system-zone exclusion; metadata_csum directory tails are verified before the physical block is recorded as `Directory` ownership with its inode number and generation. Duplicate physical ownership and insufficient arena capacity fail closed.

The mount recovery entry point now takes a `directory_inodes: &[u32]` argument. This is an explicit coverage boundary: only the supplied, validated directory inode set gains replay authorization. Directory blocks not in that set remain denied. External extent-tree nodes are checksum-validated while traversed but are not yet registered as replay targets; backup metadata and xattrs also remain unsupported. Callers must not supply inode numbers from untrusted directory entries without first validating their namespace and inode allocation state.


## Directory extent-tree replay ownership milestone (2026-10-30)

The mount-derived directory ownership pass now records external extent-tree
nodes on every logical-block lookup path used to map each caller-supplied
directory inode. It validates child-node placement against system zones,
checks each node checksum using the owning inode number and generation when
metadata_csum is active, verifies the expected depth through the existing
extent-node parser, and adds an exact `ExtentNode` ownership record before
directory data blocks are admitted. Repeated nodes are accepted only for the
same inode/generation and kind; cross-owner aliases and collisions with
directory data fail closed.

Coverage is deliberately bounded to paths reached by populated directory
logical blocks. This is not a full extent-tree scrubber, does not authorize
unreachable nodes, and does not discover the complete directory namespace.
The caller-provided directory inode list remains a trust boundary. External
extent-tree mutation, xattrs, backup metadata ownership, full namespace
discovery, and Linux image/crash testing remain outstanding.


## Allocated-inode ownership validation (2026-10-30)

The mount-owned directory ownership pass no longer treats a readable inode
record as sufficient proof of ownership. Before reading a supplied directory
inode, it validates the primary group descriptor checksum, rejects
`EXT4_BG_INODE_UNINIT`, reads the owning group's inode bitmap, validates the
inode bitmap checksum over the defined `inodes_per_group / 8` bytes when
metadata_csum is active, and verifies the requested inode allocation bit.
This prevents free inode-table contents or uninitialized inode groups from
being used to authorize directory data and extent-node replay targets.

This does not establish namespace reachability or discover all allocated
directories. The caller-supplied inode list remains a trust boundary; complete
inode-table scanning, directory graph validation, xattr ownership, backup
metadata ownership, and Linux-generated recovery testing remain outstanding.


## Automatic allocated-directory discovery (2026-10-30)

Added `discover_allocated_directories` and `CheckedExtFilesystem::mount_with_recovery_and_discovery`. The scanner iterates all inode groups, validates the group descriptor and inode-bitmap checksums when metadata_csum is enabled, handles `EXT4_BG_INODE_UNINIT` by skipping a group only when its checksummed free-inode count proves that every inode in that group is free, checks every relevant inode-table block and the inode bitmap against the retained system-zone map, and tests each valid allocation bit. Each allocated inode is loaded through the standard inode-table path, which validates its inode checksum, then classified by file mode. Directory inode numbers are emitted into caller-owned fixed-capacity storage; the internal journal inode is excluded.

The new mount entry point performs this scan before constructing the replay ownership map, then admits only the discovered directories' checksum-validated data blocks and traversed extent nodes. Capacity exhaustion and malformed/uninitialized groups fail closed. This removes the requirement for callers to manually enumerate directory inode numbers for the recovery path.

**Important limit:** inode-bitmap discovery proves allocation and inode type, not namespace reachability. It may include disconnected or orphaned-but-allocated directories; it does not yet prove root inode 2, `.`/`..` consistency, parent-child relationships, uniqueness, or absence of namespace cycles. The existing mount-state gate continues to reject orphan-recovery states. Xattr blocks, backup metadata, and a full namespace graph validator remain outstanding. This milestone does not enable writable mounts or establish full Linux ext4 replay compatibility.


## Directory namespace graph validation (2026-10-30)

Added `validate_directory_namespace` and integrated it into `mount_with_recovery_and_namespace_discovery`. After inode-bitmap discovery, the validator checks unique directory inode numbers, derives a unique parent for every discovered directory from directory entries, requires root inode 2, verifies all parent chains terminate at root within a bounded number of steps (cycle detection), and validates exactly one self-referencing `.` plus one `..` entry matching the derived parent. Directory checksums are validated before entries are trusted; file-type-less filesystems resolve target inode modes through the checked inode-table and allocation-bitmap paths. Caller-owned parent storage is bounded and capacity checked.

**HTree integration:** the namespace graph validator now routes `EXT4_INDEX_FL` directories through the checksum-validated unique HTree iterator. The directory inode's checked extent/legacy mapper backs root, interior-node and leaf reads; dx checksums and leaf checksums are verified; collision continuations and sibling index traversal are included; root `.`/`..` entries are visited; duplicate leaf pointers (including empty leaves) and tracking-buffer exhaustion fail closed. Unknown dirent types are reconciled with the allocated-directory scan. The namespace-discovery mount entry point now requires caller-owned HTree root and traversal buffers plus a visited-leaf array.

**Remaining HTree limits:** maximum supported indirect depth is still one interior level, and SipHash directories requiring an unavailable fscrypt key fail closed. Directory graph validation is not an fsck-equivalent consistency checker and does not validate every non-directory inode reference or all ext4 namespace features. Writable mounts remain disabled. Backup metadata and xattr replay ownership, orphan-file support, and Linux/e2fsprogs crash-interoperability testing remain outstanding.

# CognacOS ext4 compatibility audit against Linux master

**Reference snapshot:** Linux kernel `torvalds/linux` `master`, reviewed 2026-10-30  
**Scope:** source-level comparison of `drivers/ext4` with the Linux ext4 driver and its format documentation.  
**Status:** engineering audit, not a conformance certification. The Linux branch is moving; rerun this audit against a pinned commit before publishing a release claim.

## Executive assessment

CognacOS currently provides a bounded, allocation-free, `no_std` ext-family parser with a read-only checked-mount facade, selected extents/HTree/checksum/JBD2 primitives, metadata mutation building blocks, and conservative replay ownership. It is **not yet a general-purpose Linux-ext4-compatible driver**. The supported superblock feature mask is intentionally small, the checked mount rejects dirty journals, there is no complete VFS-facing file/directory implementation, and writable mounting is not enabled.

A numerical claim such as “99% compatible” is not currently measurable. Compatibility must be measured against a defined matrix of Linux-master feature combinations, operations, filesystem geometries, crash states, and corruption cases. A feature being parsed or having a helper function is not equivalent to Linux behavioural compatibility.

## What was implemented in the latest batch

- Added `discover_and_reconcile_external_xattr_owners`, a bounded scan of allocated inodes.
- The scan validates inode/group metadata, inode checksums (when enabled), xattr block allocation, system-zone exclusion, xattr structure/checksum, and global on-disk reference counts.
- Added `CheckedExtFilesystem::discover_external_xattr_owners` as the checked-mount entry point.
- The scan includes the internal journal inode: it must not be omitted from a filesystem-wide xattr reference inventory.
- Inventory capacity exhaustion and malformed/unverifiable metadata fail closed.

**Important:** this inventory is not yet integrated into journal replay's metadata ownership manifest. Xattr blocks remain denied replay targets. Large EA inodes/value-inode references are not supported. This batch does not enable writable mounts.

## Follow-up hardening batch (2026-10-30)

The subsequent implementation batch made the following bounded changes:

- Removed duplicate superblock feature/revision checks and added per-group bitmap-capacity bounds for `s_blocks_per_group` and `s_inodes_per_group`.
- Tightened JBD2 superblock geometry: journal block sizes are capped at 64 KiB, and a nonzero recovery start must lie at or beyond `s_first` while remaining below `s_maxlen`.
- Made async-commit and fast-commit rejection explicit in the JBD2 incompat-feature gate.
- Extended the metadata ownership map to represent multiple distinct inode owners for one external xattr block. Duplicate owner records, cross-kind physical aliases, and ownerless Xattr records are rejected. `owner_count` exposes the number of recorded owners.

The ownership-map change is **not** replay authorization by itself. The complete checked-mount xattr inventory and its on-disk reference-count reconciliation must be supplied before any shared xattr block is admitted to a replay manifest. This batch deliberately leaves replay and general writable mounting disabled.

Regression tests were added for shared-xattr owner records and JBD2 ring geometry. They have been committed but have **not** been executed in a local Cargo build or verified by a completed CI run in this work session. See [FEATURE_SUPPORT_MATRIX.md](FEATURE_SUPPORT_MATRIX.md) for the current explicit feature policy.


## Large compatibility batch (2026-10-30)

This follow-up batch closes several concrete read-path gaps:

- Enabled Linux `LARGE_DIR` read-only-compatible feature negotiation, requiring the `DIR_INDEX` compatible feature.
- Extended HTree traversal to two interior dx-node levels (root + two interior nodes + leaf), with superblock-aware adapters rejecting depth two unless `LARGE_DIR` is present.
- Added regression fixtures for two-level HTree path traversal and malformed child pointers.
- Added a zone-enforcing ext2/ext3 legacy mapper. It checks each indirect metadata block before reading it and checks the final data block before returning it. The internal journal inode must continue to use its dedicated mapping path because its blocks are deliberately protected as a journal zone.
- Corrected 64 KiB directory-entry record-length decoding in both the base directory validator and mutation-path validator.
- Added regression tests for full-block 64 KiB dirents, legacy direct/indirect system-zone rejection, and the LARGE_DIR/DIR_INDEX feature prerequisite.

These changes improve read compatibility and validation, but do not implement extent-tree mutation lifecycle, full orphan-file support, inline data, EA inodes, or the cryptographic feature families. They do not enable a general writable mount.

The new tests are committed but have not been executed in a Cargo build or CI in this session. The support matrix records the updated scope.


## Feature and subsystem comparison

| Area | CognacOS current state | Linux master expectation | Remaining work |
|---|---|---|---|
| Superblock / feature negotiation | Parses core geometry, 64-bit block count, descriptor size and a narrow feature mask. | Linux negotiates compat, read-only-compat and incompat feature sets, with mount-option and configuration-dependent behaviour. | Complete feature tables, revision/geometry validation, backup-superblock policy, unknown-bit policy, mount-option semantics and exhaustive feature-combination tests. |
| Block groups / descriptors | Basic descriptors, 64-bit descriptor fields, bitmap checksums and flex_bg-aware metadata placement are partially supported. | Includes meta_bg, flex_bg, lazy initialization, online resize and related descriptor layouts. | Implement meta_bg; fully validate descriptor size/layout, reserved GDT, backup copies, uninitialized groups, counters, bitmap tail bits, online-resize layouts and corruption handling. |
| Allocation geometry | Block-based allocation checks. | Linux supports bigalloc clusters as the allocation unit. | Implement cluster-aware bitmap indexing, extent accounting, checksums, free-space counters and system-zone logic, or explicitly reject bigalloc. |
| Extents | Checked extent mapping and mutation primitives exist. | Full extent-tree lifecycle: split/merge, depth changes, unwritten extents, delayed allocation, preallocation, fiemap, truncate/punch/zero-range and error recovery. | Full read/write extent engine, all edge cases, large files, extent status cache semantics, unwritten conversion, bigalloc interaction, and exhaustive malformed-tree tests. |
| Legacy block mapping | Legacy mapping is used in selected paths. | Direct/indirect/double/triple-indirect support remains relevant for ext2/ext3 and old ext4 inodes. | Full mapping, allocation/freeing, truncation and boundary tests for all pointer levels. |
| Directories | Basic dirent validation, namespace discovery, HTree lookup and split primitives. | Linear and indexed directory operations, checksum tails, dx hash variants, depth growth, split/collision handling, casefold and encryption interactions. | Complete create/remove/rename/readdir/lookup, HTree root/node split and conversion, 3-level large_dir, exact hash compatibility, dirent checksum and concurrency semantics. |
| Inodes | Core inode fields and checksums; selected mutation helpers. | Full inode lifecycle and semantics: extra_isize, timestamps, huge_file, flags, project IDs, quota, generation, orphan handling, inline data, EA inode and verity/encryption state. | Full inode read/write and flag validation, 64-bit i_blocks/huge_file, extra_isize, nanosecond timestamps, inode version, project/quota fields, special files and feature-specific inode layouts. |
| Extended attributes | External xattr structure/checksum/allocation/owner reference validation now exists. | In-inode and external xattrs, sharing/refcounts, EA-inode large values, ACL/security/trusted/user namespaces and inline-data's system.data xattr. | In-inode xattr parser and mutation, namespace policy, EA-inode/value-block support, ACL integration, xattr create/replace/remove, deduplication and journal ownership. |
| Inline data | Not implemented as a complete inode data path. | Inline file, directory and symlink data stored in inode/xattr area; conversion to extents on growth. | Implement inline-data detection, data extraction, directory handling, checksum/offset checks, conversion and transactionally correct mutation. |
| Checksums | CRC32C helpers and selected superblock/GDT/bitmap/inode/extent/directory/xattr validation. | Checksums cover all relevant metadata structures and depend on feature flags, UUID/CSUM_SEED, inode generation, block number and structure-specific rules. | Audit every Linux checksum formula/coverage/zeroing rule; add golden vectors from mke2fs/debugfs/e2fsck and corruption tests for every metadata type. |
| JBD2 | Bounded journal parsing/replay and ordered commit primitives; checksum v1/v2/v3 paths; async commit and fast commit rejected. | Full JBD2 recovery and transaction semantics, revoke handling, 64-bit tags, checksum variants, async commit, fast commits, journal features and data modes. | Fast-commit replay, async-commit semantics, full journal feature negotiation, external journals, tag variants, transaction wraparound, revoke ordering, recovery cleanup and power-failure testing. |
| Orphan recovery | Legacy orphan-chain planner exists. | Linux supports legacy orphan chain plus orphan file, with full truncate/delete/reclaim and journal semantics. | Orphan-file parsing/recovery, checksum and bitmap validation, atomic truncate/free/revoke, recovery interruption tests. |
| System zones / replay ownership | Conservative system-zone map and exact ownership manifest for selected metadata and discovered directories. | Linux validates block references and journals all metadata classes affected by operations, including backup metadata and evolving inode-owned objects. | Include every replayable object only after validated discovery; backup super/GDT, extent nodes, xattrs, inline/EA metadata, orphan file, quota and resize metadata. Add ownership collision and revoke tests. |
| Encryption | Unsupported. | fscrypt policy, filename/data encryption and key/nonce semantics when configured. | fscrypt integration or reject encrypted feature/inode flags with precise unsupported status. |
| Casefold | Unsupported. | Unicode normalization/casefold lookup, hash semantics, strict-mode behaviour. | Unicode versioned normalization, casefolded HTree hashing, invalid UTF-8 fallback/strict mode, mixed-directory tests. |
| Verity | Unsupported. | fs-verity Merkle tree, descriptors, measurement and read verification when configured. | Implement fs-verity integration or reject verity inodes safely. |
| Quotas / project IDs | Not a complete implementation. | User/group/project quota accounting and project inheritance. | Quota inode formats, transaction updates, project IDs and enforcement. |
| Resize / MMP / DAX / atomic writes | Not implemented as Linux feature-complete paths. | Linux supports applicable online resize, multiple mount protection, DAX and newer atomic-write capabilities depending on configuration/hardware. | Implement each supported capability or explicitly reject feature flags; do not silently treat them as ordinary filesystems. |
| VFS and concurrency | no_std core and bounded primitives; not a Linux VFS implementation. | Linux provides complete inode/dentry/address-space/file operations, locking, page cache, writeback, fsync, iomap/direct I/O and error handling. | CognacOS VFS integration: open/read/write/seek/readdir/stat/link/unlink/rename/mkdir/rmdir/symlink, permissions, locking, cache coherency, writeback, fsync and crash durability. |
| Performance / scalability | Bounded caller-provided buffers and linear/bounded scans. | Caches, multiblock allocator, delayed allocation, parallelism, scalable counters, readahead and large filesystem handling. | Caches and indexes, allocator, concurrency/SMP safety, memory pressure policy, large-volume benchmarks and adversarial complexity bounds. |

## Linux master source map used for comparison

Linux ext4 is not a single parser. Its build is composed of separate subsystems including allocation/bitmaps, block validity, directories, journaling, extents, indirect blocks, inline data, inode handling, multiblock allocation, MMP, resize, symlinks, xattrs, fast commits and orphan handling. Optional ACL, verity and encryption code is configuration-dependent. See the Linux ext4 [Makefile](https://github.com/torvalds/linux/blob/master/fs/ext4/Makefile).

Key comparison references:

- [Linux ext4 admin guide](https://github.com/torvalds/linux/blob/master/Documentation/admin-guide/ext4.rst)
- [Linux ext4 on-disk superblock and feature flags](https://github.com/torvalds/linux/blob/master/Documentation/filesystems/ext4/super.rst)
- [Linux ext4 data structures and algorithms](https://www.kernel.org/doc/html/latest/filesystems/ext4/)
- [Linux ext4 superblock/mount implementation](https://github.com/torvalds/linux/blob/master/fs/ext4/super.c)
- [Linux ext4 core structures and feature flags](https://github.com/torvalds/linux/blob/master/fs/ext4/ext4.h)
- [Linux inline-data implementation](https://github.com/torvalds/linux/blob/master/fs/ext4/inline.c)
- [Linux orphan implementation](https://github.com/torvalds/linux/blob/master/fs/ext4/orphan.c)

## Priority plan to approach high compatibility

### P0 — Establish a trustworthy baseline
1. Pin the exact Linux master commit used for every audit.
2. Build a feature negotiation table and tests for every compat/ro_compat/incompat bit.
3. Establish reproducible images using current e2fsprogs: ext2, ext3, ext4; 1K/2K/4K/64K blocks where valid; 128/256/512-byte inodes; 32/64-bit descriptors; flex_bg; metadata_csum; sparse_super/sparse_super2.
4. Differentially compare CognacOS and Linux mount/read/stat/readdir/checksum results.
5. Add malformed-image and fault-injection suites; require fail-closed behaviour.

### P1 — Complete read-only filesystem behaviour
1. Complete inode and legacy/extent mapping semantics.
2. Complete linear and indexed directory lookup/readdir, including large_dir and all hash variants supported by the selected feature mask.
3. Implement inline data and full xattr semantics, including EA inode support or explicit feature rejection.
4. Complete special inode handling, orphan file, quotas and metadata validation.
5. Test clean and unclean filesystems against Linux/e2fsck, including corrupted metadata.

### P2 — Make journal recovery Linux-compatible
1. Complete JBD2 feature negotiation, fast commits and async commit or reject them precisely.
2. Complete orphan chain/file recovery and all metadata ownership discovery.
3. Include every metadata class in replay policy only after ownership and allocation are proven.
4. Test crash points at every write/flush boundary and compare replay results with Linux.
5. Keep read-only mount available when an unknown read-only-compatible feature permits it; reject unknown incompatible features.

### P3 — Enable safe writable operations
1. Implement complete transaction coverage for all mutators, allocation/freeing, revokes and checksums.
2. Implement writeback, fsync, ordered/writeback/journal data modes and error propagation.
3. Prove concurrency, locking, cache coherency and SMP safety.
4. Run sustained Linux-generated workload, crash/power-loss, fuzz and e2fsck validation.

## Proposed acceptance criteria for a “99% compatible” claim

Do not calculate compatibility by counting functions or source lines. Define a weighted test matrix first:

- **Feature coverage:** every Linux-master feature bit is either implemented and tested, or explicitly rejected according to Linux-compatible mount semantics.
- **Read-only behaviour:** all supported filesystem geometries and inode/directory/xattr variants pass differential tests against Linux.
- **Mutation/recovery:** each enabled operation survives injected crashes and produces an image accepted by Linux and e2fsck without repair.
- **Corruption handling:** fuzzed malformed images never cause out-of-bounds access, unsafe replay, silent metadata corruption or unbounded work.
- **Interoperability:** Linux-created images are readable; CognacOS-created/modified images are mountable and semantically correct on Linux.
- **Performance and scale:** agreed workload and volume-size targets are met without unbounded scans or memory use.
- **Unsupported features:** every unsupported incompatible feature is rejected; unknown read-only-compatible features are handled according to Linux semantics.

A 99% claim should mean at least 99% of a published, weighted, reproducible Linux-master test matrix passes, with no failures in data integrity, crash consistency, security boundaries or corruption safety. Until that matrix exists and is green, the accurate description is **“experimental ext-family driver with a limited, fail-closed ext4 feature subset.”**

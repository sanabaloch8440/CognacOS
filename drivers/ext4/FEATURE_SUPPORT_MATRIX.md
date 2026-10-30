# CognacOS ext-family feature support matrix

This file is the driver's mount-policy contract, not a claim of full Linux ext4
compatibility. A feature is supported only when its on-disk semantics are
validated by the implementation that consumes it. Merely accepting a feature
bit does not make all operations on that feature safe.

## Superblock negotiation

| Feature class | Current policy | Notes |
|---|---|---|
| Compatible features | Unknown compatible bits are tolerated | This follows ext4's rule that unknown compatible features do not change on-disk interpretation for an old reader. Individual features still need operation-specific checks. |
| Read-only-compatible features | `0x0403` plus `LARGE_DIR` (`0x4000`) is accepted | sparse_super, large_file, metadata_csum, and large_dir. LARGE_DIR requires DIR_INDEX. Other RO-compatible bits, including bigalloc, inline_data, encrypt, casefold, verity and project quota, are rejected. |
| Incompatible features | Only the explicit `KNOWN_INCOMPAT` mask is accepted | filetype, extents, flex_bg, 64bit and csum_seed. meta_bg and all other unknown incompat bits are rejected. |
| Revision | Revision values above 1 are rejected | Dynamic revision layout is the supported baseline. |
| Block size | 1 KiB through 64 KiB, powers of two | The parser rejects larger log sizes. |

## Implemented subsets and hard gaps

| Area | Current implementation | Not yet compatible |
|---|---|---|
| ext2/ext3 legacy mapping | Direct, single-, double- and triple-indirect read mapping; checked mapping rejects system-zone indirect blocks and data blocks | Full indirect-tree mutation lifecycle, allocation/free, truncate and boundary interoperability matrix |
| extents | Checked lookup and selected mutation helpers | Full split/merge/root-depth lifecycle, unwritten conversion, truncate/punch/zero-range, delayed allocation and extent-status semantics |
| flex_bg | Selected descriptor/bitmap placement validation | Complete Linux layout edge cases, resize and every flex-group geometry |
| bigalloc | Rejected at superblock feature negotiation | Cluster-based allocation, accounting, checksums and extent semantics |
| meta_bg | Explicitly rejected | Meta block-group descriptor placement and backup metadata |
| metadata checksums | Selected superblock, group descriptor, bitmap, inode, extent, directory, xattr and JBD2 checks; 64 KiB directory record lengths are decoded | Exhaustive structure-by-structure formula/coverage audit and Linux-generated golden vectors |
| HTree | Read traversal and bounded split/index mutation primitives | Complete recursive index growth, three-level large_dir, full directory operation/VFS semantics and all hash/collision edge cases |
| external xattrs | Structure, checksum, allocation and full owner-inventory reconciliation | EA inodes/value blocks, namespace policy, full mutation and replay ownership integration |
| inline data | Not supported as a complete data path | Inline file/directory/symlink read/write and conversion |
| EA inodes | Rejected with unsupported RO-compatible feature | Large external xattr values and EA inode lifecycle |
| orphan handling | Legacy orphan-chain planning/recovery primitives | Orphan-file format and atomic recovery across every interruption point |
| JBD2 fast commit | Rejected | Fast-commit replay and fallback semantics |
| JBD2 async commit | Rejected | Async commit ordering and recovery semantics |
| fscrypt | Rejected | Policy, key handling, encrypted names/data and directory hash integration |
| fs-verity | Rejected | Merkle tree, descriptors and read verification |
| casefold | Rejected | Unicode normalization, casefold lookup and hash compatibility |
| quotas/project quotas | Incomplete/rejected where feature bits require unsupported semantics | Quota inode formats, accounting, enforcement and project inheritance |
| online resize/MMP | Not implemented as complete operational paths | Resize metadata transitions, backup updates and multi-mount protection |

## Safety rules

1. Unknown incompatible features must fail mount.
2. Unknown read-only-compatible features must not be silently treated as
   supported; this driver currently rejects all bits outside its explicit mask.
3. Unsupported JBD2 async/fast-commit features must fail journal parsing before
   replay starts.
4. External xattr blocks may have multiple inode owners. The ownership map can
   retain multiple Xattr owner records for one physical block, but replay must
   not be enabled until the caller supplies a complete scanner-verified owner
   inventory and reconciles its count with on-disk `h_refcount`.
5. No feature listed as partial or unsupported may be used to justify a
   general-purpose writable mount or a 99% compatibility claim.

## Validation still required

For each accepted feature combination, generate images with current e2fsprogs,
mount/read them in Linux and CognacOS, and compare inode metadata, block
mapping, directory lookup/readdir, xattrs and checksums. For every mutation,
inject failures at each write/flush boundary and require Linux and e2fsck to
accept the recovered image without repair. Keep unsupported feature tests in
the suite to prove deterministic fail-closed behaviour.

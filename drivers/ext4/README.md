# CognacOS ext4 driver: mutation and recovery status

This crate now contains bounded ext4 metadata mutation primitives and a bounded
JBD2 implementation. The primitives operate on caller-owned metadata buffers;
the filesystem mount facade does not yet expose a general-purpose writable
filesystem API.

## Implemented primitives

- Internal journal discovery from the ext4 superblock, including journal inode
  number and UUID binding. External journal devices are rejected.
- JBD2 v2 superblock parsing, legacy descriptor tags, optional 64-bit block
  numbers, revoke records, commit sequence validation, committed transaction
  replay, incomplete-tail handling, and recovery-start cleanup.
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

## Deliberate safety limits

The implementation rejects JBD2 checksum-v2/v3, async-commit and fast-commit
journals. The replay/commit path is bounded to 256 metadata tags and 256 revoke
records per replay/transaction. A transaction must fit one descriptor block.
The caller must provide correct journal-inode extent mapping and serialize all
writers. External extent-tree split/merge, indexed htree directories, external
journals, and journal checksum formats are not supported by these routines.

The current ExtFilesystem mount facade still rejects writable mounts that
require journal recovery or metadata_csum mutation. That gate remains in place
until the journal inode mapping, recovery, metadata read/write paths, and all
allocation/directory/file operations are wired together through one mount-level
transaction coordinator and validated against real ext4 images and crash
injection. These lower-level APIs are not a claim of complete Linux ext4 or
Windows filesystem-driver compatibility.

## Validation

The GitHub Actions workflow runs the JBD2 parser/replay/commit tests and the
ext4 allocation, directory, inode, extent and checksum mutation tests as part
of the driver test matrix.

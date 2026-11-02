# Cache Manager Milestone 3 — implementation contract

Target: Windows 11 x64 / NT 10.0.26100 Cache Manager semantics.

This document is the acceptance contract for the Cache Manager work. It is
intentionally not a claim that the current implementation is complete.

## Current audited gaps

The current `ntoskrnl/src/cc/mod.rs` implementation is a clean-page read cache,
not a full Cache Manager:

- Cache maps are held in a global 128-entry array.
- Each stream is limited to eight cache pages.
- Cache identity is the FILE_OBJECT address; there is no stream lifetime or
  section-object-pointer identity/rundown model.
- `CcCopyWrite` always falls back.
- `CcFlushCache` reports success without performing filesystem writeback.
- There is no dirty-page state, dirty-range accounting, modified-page list,
  lazy-writer queue, or writeback error propagation.
- Read-ahead granularity is recorded but no independent read-ahead work is
  scheduled.
- Size changes discard/truncate clean pages only; purge is not implemented.
- The current spin lock protects metadata while copy operations touch cached
  pages, which is not an acceptable long-term concurrency/rundown model.

## Required design

### 1. Stream identity and lifetime

Use a referenced stream/cache identity owned by the FILE_OBJECT and its
SECTION_OBJECT_POINTERS, not a raw pointer as a reusable identity. Cache-map
creation, file-object close, uninitialization, section teardown, and purge must
use explicit rundown/reference accounting. Reopen of the same underlying
stream must not inherit stale cache pages from a recycled FILE_OBJECT.

### 2. Dynamic metadata

Remove fixed global cache-map and per-map page arrays. Allocate cache-map,
stream, page, dirty-range, and work-item metadata from the Executive pool with
stable ownership and tag checking. Allocation failure must leave the previous
map intact or fail the operation without leaking pages. No allocation or
filesystem callback may run while holding a cache metadata spin lock.

### 3. Page and dirty accounting

Each cached page needs a state machine (absent, clean, dirty, writeback,
invalidated/error), valid byte range, stream offset, reference/pin count,
and dirty generation. Dirty accounting must be updated atomically with data
modification and reconciled only after successful writeback. Concurrent writes
during writeback must remain dirty for the next pass.

### 4. Filesystem I/O contract

Connect cache population and writeback to a filesystem stream provider through
the I/O manager. The provider must support bounded offset reads/writes and
return exact transferred lengths and NTSTATUS. Do not call ext4 directly from
generic Cache Manager code. Do not recurse through cached I/O when servicing
paging I/O; paging writes must use the resident non-recursive paging provider.

### 5. Export semantics

- `CcCopyRead`: satisfy only fully resident, valid ranges; otherwise use the
  documented fallback path without partially modifying the caller buffer.
- `CcCopyWrite`: validate EOF/VDL and access semantics, update cached data,
  mark pages/ranges dirty, and preserve partial-page validity.
- `CcFlushCache`: flush the requested byte range, report the actual status and
  bytes, and retain dirty state on failure.
- Lazy writer: acquire/release filesystem callbacks outside internal locks,
  batch dirty pages, perform writeback, and retry bounded transient failures.
- Read-ahead: issue bounded, aligned, stream-scoped asynchronous reads and
  discard stale completions after stream generation changes.
- Purge/truncation: block new references, flush or explicitly fail on dirty
  data, invalidate the requested range, and reclaim pages only after rundown.

### 6. Mm and filesystem integration

Cache pages must have explicit PFN/page ownership and pin accounting. Unmap,
purge, truncation, and file close must not free pages still referenced by MDLs,
mapped views, or in-flight I/O. Filesystem size updates must publish EOF/VDL
changes before stale cached bytes can be observed.

## Acceptance gates

1. Dynamic metadata stress: more than 128 simultaneous streams and more than
   eight resident pages per stream; allocation failure and teardown tests.
2. Stream identity: same stream through multiple FILE_OBJECTs, independent
   streams on one file object, close/reopen reuse, and concurrent uninitialize.
3. Dirty accounting: overlapping writes, cross-page writes, write during
   writeback, failed/short writes, and dirty-page count reconciliation.
4. Copy operations: cache hit, miss, EOF, VDL boundary, zero length, overflow,
   and partial-page validity.
5. Flush/lazy writer: successful, failed, short, concurrent, and retry cases.
6. Read-ahead: sequential detection, bounds, cancellation/stale completion,
   and no recursive cached I/O.
7. Purge/truncate: clean and dirty ranges, mapped/pinned pages, shrink/grow,
   and concurrent reads/writes.
8. CI: host unit tests, kernel build, ABI/export validation, and QEMU smoke
   test. A green compile alone does not close this milestone.

## Explicit non-goals for this milestone

Do not claim complete Windows Cache Manager parity. External cache providers,
logged/transactional cache, BCB/MDL pin APIs, mapped-file coherency corner cases,
and all private Windows 11 exports remain separate compatibility work unless
the acceptance tests and ABI evidence are added here.

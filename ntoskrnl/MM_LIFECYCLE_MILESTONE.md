# Memory Manager lifecycle completion

This work is based on `main` at merge commit `b5d9a758abcf3ac7891806bb1452fa994ae60319`.

## Non-negotiable safety gates

- Pageout and working-set trimming that writes pages to backing storage remain disabled until a resident, non-recursive paging provider is registered and its I/O path is proven not to allocate pageable memory, fault, or call the filesystem/cache manager.
- A physical page cannot return to an allocator while any mapping, MDL pin, I/O reference, section prototype, or other owner still holds a reference.
- Failed multi-step operations roll back acquired references and mappings in reverse order.
- Process-owned VM metadata is released only after address-space teardown has removed mappings and rundown has excluded concurrent faults/MDL operations.
- File-backed sections must not be reported as demand-paged until file offset/length validation, prototype-PTE state transitions, page-in I/O, and control-area lifetime are implemented.

## Current implementation status (2026-11-02)

- [x] Added `ntoskrnl/src/mm/pfn.rs`: checked fixed-capacity PFN ownership records with separate allocation-owner, mapping, and pin references; overflow/underflow, duplicate registration, and premature owner release are rejected. Unit tests cover mapping/pin lifetime and invalid transitions.
- [x] Registered the PFN module in the MM subsystem.
- [ ] Wire PFN registration into every allocator entry point, including single-page, contiguous, node/range, high-memory, and rollback/deferred-free paths.
- [ ] Wire mapping references into all PTE install/replace/unmap and address-space teardown paths.
- [ ] Replace or unify the current MDL locked-PFN registry with PFN pin references and make pin/unpin transactional.
- [ ] Move demand-area and working-set metadata into dynamically allocated process/address-space-owned structures.
- [ ] Implement and register a real storage-stack paging provider.
- [ ] Implement file-backed control areas, prototype PTEs, and file page-in.
- [ ] Run host tests, kernel build, and QEMU acceptance tests.

The PFN database is currently a foundational component, not yet an end-to-end enforcement mechanism. In particular, do not claim the PFN requirement complete until every allocation/free/mapping/MDL path above is wired and validated.

## Implementation sequence

### 1. PFN database and ownership accounting

Introduce one PFN record per allocatable physical page, initialized transactionally from the final firmware memory map. Track page state, allocation owner/class, reference count, mapping count, pin count, and lifecycle flags. Define checked transitions for allocation, map/unmap, MDL lock/unlock, section/prototype ownership, and final free. Reject underflow, double-free, freeing pinned/mapped pages, and invalid PFNs. Audit every allocator and page-table path, including rollback and address-space destruction, so references are acquired before publishing mappings and released only after invalidation/shootdown.

### 2. Address-space-owned metadata

Replace the bounded global working-set and demand-area registries with dynamically allocated metadata attached to the process/address-space object. Use a bounded global bootstrap registry only for pre-process initialization and compatibility bookkeeping. Add explicit initialize, reference/rundown, detach, and destroy operations. Fault handling must hold a lifetime reference while looking up or mutating a demand area.

### 3. Resident storage paging provider

Connect the paging provider to a storage device through the I/O manager and storage/class/port stack. The provider must use resident request structures, resident locked buffers, non-recursive block I/O, bounded queueing, and synchronous completion semantics suitable for page faults. It must be explicitly registered only after device initialization and self-tests. Until then, pageout, modified-page writeback, and trim-to-pagefile remain unavailable; page-in from an existing slot must fail closed if the provider goes offline.

### 4. File-backed control areas and prototype PTEs

Add reference-counted control areas/segments for file objects, validated file ranges, and prototype PTE state transitions (prototype, transition/resident, file-backed-not-resident, and error). File faults must acquire control-area/file references, issue paging reads through the safe paging-I/O path, install mappings transactionally, and unwind on short reads or I/O failure. Preserve the current bounded eager section implementation as an explicitly separate supported mode until this path passes acceptance tests. COW remains limited to in-memory prototypes until file-backed dirty/writeback semantics exist.

## Required validation

- PFN accounting: allocate/map/unmap/free; shared mappings; MDL pin/unpin; COW replacement; section view lifetime; failed mapping rollback; double-free and refcount-underflow rejection.
- Address spaces: independent working sets and demand-area trees; concurrent fault versus teardown; process exit with mapped sections and pinned MDLs.
- Paging provider: no-provider fail-closed behavior; alignment and range checks; non-recursive callback contract; read/write errors; device removal/rundown; no IDE-specific assumptions.
- File sections: page-in at first access; shared prototype mapping; short read and device error rollback; file truncation/range rejection; COW isolation for in-memory prototypes.
- QEMU: demand-zero, COW isolation, multi-process working-set isolation, address-space teardown, and storage-backed page-in. Pageout tests are gated until the provider is operational.

## Completion rule

Do not mark this milestone complete or enable pageout until the implementation is integrated across all allocation/mapping/MDL/free paths, the host test suite and kernel build pass, and QEMU acceptance tests demonstrate the required lifetime and fault behavior. No diagnostic-print or telemetry-only changes count as implementation.

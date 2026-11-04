# Executive pool runtime milestone — AML storage prerequisite

**Status (2026-11-04):** the kernel has a real page-backed executive pool path and exports the Windows x64 pool entry points used by native drivers. ACPI now has a versioned, callback-based AML pool contract and its method/PRT evaluation arenas use that contract. This is the first integration slice; it does **not** remove the AML value, Package-element, or payload ceilings.

## Existing kernel path

- `ntoskrnl/src/ex/pool.rs` implements `ExAllocatePoolWithTag`, `ExAllocatePool2`, `ExAllocatePool3`, `ExFreePool`, `ExFreePoolWithTag`, `ExFreePool2`, and `ExQueryPoolBlockSize` adapters.
- `ntoskrnl/src/ntoskrnl.rs` publishes the supported functions through the kernel export resolver. ACPI's native driver imports the legacy tagged allocate/free pair.
- `ntoskrnl/src/rtl_runtime/pool.rs` is the small host-test backend. On the kernel target it dispatches to `pool_page.rs`.
- `ntoskrnl/src/rtl_runtime/pool_page.rs` obtains backing pages from kernel VM, uses power-of-two slabs for small blocks, and page extents for larger blocks. It validates exact allocation-base pointers and tags before release.

## Current kernel allocator constraints

These are CognacOS implementation budgets, not Windows pool limits:

- 4,096 live allocation records.
- 1,024 slab descriptors.
- 65,536 pages (256 MiB) maximum for one large allocation.
- A separate preallocated no-wait reserve for constrained nonblocking allocations.
- The host-only test allocator has a 256 KiB static arena; it is not the kernel-target backing store.

The target allocator still has static metadata tables, does not implement pool quotas/session pool/special-pool policy, and does not provide a general-purpose Rust global allocator. Paged pool supports demand-zero fault-in, but eviction/pagefile backing remains a separate memory/storage milestone. Allocation failure must remain a normal, checked outcome.

## ACPI integration in this milestone

- Added `drivers/acpi/src/aml_storage.rs`, a size/version-checked callback contract with a stable AML pool tag, zeroed allocation, and matching release.
- The ACPI driver binds the contract to the kernel's `ExAllocatePoolWithTag` / `ExFreePoolWithTag` services. The callbacks remain resident for the driver image lifetime; AML unload/rundown must drain before driver unload.
- Method-execution and _PRT-evaluation arenas now use this contract and nonpaged kernel pool rather than open-coding pool imports and manually zeroing the arena.

## Remaining work before removing AML ceilings

1. Replace `AmlObjectArena`'s inline 768-value array with allocator-owned capacity and checked growth.
2. Replace inline 128-element Package storage with owned variable-length element arrays.
3. Replace inline 256-byte String/Buffer payloads with owned byte blocks and migrate conversions, fields, references, and return values.
4. Add per-table and per-execution allocation budgets, overflow-safe accounting, rollback, and allocation-failure tests.
5. Tie all AML allocations to table/driver rundown so unload cannot free storage still reachable by a method or reference.
6. Revisit kernel pool metadata growth and fragmentation/pressure behavior; do not silently treat the current descriptor counts as unbounded.

## Acceptance gates

- ACPI library tests and native WDM ACPI driver type-check.
- Kernel dispatcher/type-check and full UEFI/QEMU workflow.
- Large AML package/buffer/string fixtures, allocation failure at each growth point, nested rollback, and table unload/reload tests.
- No claim of production readiness until both the kernel pool lifetime/pressure behavior and ACPI ownership migration pass those gates.

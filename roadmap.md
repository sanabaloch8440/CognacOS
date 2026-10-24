# RdotOS Roadmap

This is the canonical progress tracker. Checkboxes describe code and validation in the repository; they do not imply production readiness or Windows driver compatibility. Detailed per-routine status belongs in the [Ke comparison](WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md) and [Se comparison](WINDOWS11_RDOTOS_NTOSKRNL_SE_COMPARISON.md).

**Target:** a maintainable Rust-first x86-64 OS with NT-inspired interfaces and a long-term path toward Windows x64 driver compatibility.

## Current validation

- Latest recorded workflow for the user-VM-copy/mapping changes: [run #1809](https://github.com/umarfarooq7869/RdotOS/actions/runs/40129424049) — passed on commit `afdc0329f2b1718d3caccb8c2f565eeb40594aa0`.
- This verifies the workflow's host tests, kernel/PE32+ validation, UEFI packaging, QEMU boot, and configured SMP checks for that revision; it does not validate later commits automatically.
- Keep CI status current by checking the latest run on `main` before marking a milestone validated.

## Milestone 1 — Boot and platform foundations

- [x] UEFI loader and native AMD64 PE32+ kernel handoff.
- [x] Kernel GDT/IDT, page-table activation, diagnostics, physical-page allocator, and high-half mappings.
- [x] ACPI table parsing, AP startup, per-CPU GDT/TSS and IST stacks, directed IPI acknowledgements.
- [x] Build, PE validation, OVMF/QEMU boot, and multi-vCPU workflow coverage.
- [ ] Validate on physical hardware and broaden malformed-image/firmware testing.
- [ ] Complete secure-boot signature verification and relocatable-image support.

## Milestone 2 — Memory isolation and fault containment (in progress)

- [x] Validate user-copy ranges against the permitted user virtual-address interval.
- [x] Preflight all pages before multi-page user copies; hold address-space lifetime protection across validation and transfer.
- [x] Make user-page mapping roll back newly allocated intermediate page tables on supported failure paths.
- [x] Run host regression tests for range boundaries, zero-length pointers, and integer overflow.
- [ ] Add integration tests for cross-page failures, read-only destinations, rollback, and map/unmap races.
- [ ] Deliver user-mode exceptions through a defined trap-frame path and terminate only the faulting process.
- [ ] Integrate timer-driven preemption and prove a non-yielding thread cannot monopolize a CPU.
- [ ] Add user demand-zero/page-in handling, per-process VM areas, and working-set accounting.
- [ ] Add exception-safe probe/copy recovery and test concurrent address-space teardown.
- [ ] Harden I/O completion, cancellation, driver unload, and device/IRP rundown with race and fault-injection tests.
- [ ] Pass current QEMU multi-vCPU and stress/fault-injection validation before closing the milestone.

## Milestone 3 — Scheduler, waits, APCs, timers, and DPCs

- [x] Cooperative thread dispatch, scheduler wake IPIs, dispatcher object foundations, and deadline-based timer infrastructure.
- [x] Cross-processor Ke rendezvous and DPC Synchronize/Done barriers have passed a previous 2/4/8-vCPU workflow.
- [x] Consolidate classic KTIMER and KTIMER2 into one timer metadata table and expiry scan; add generation-based stale-expiry checks.
- [ ] Complete timer/DPC cancellation and in-flight callback rundown semantics under stress.
- [ ] Integrate timer interrupt preemption, rescheduling, per-CPU run queues, and load balancing.
- [ ] Complete user APC delivery/SEH interaction, threaded DPC execution at PASSIVE_LEVEL, and broader wait-all/queue/mutant semantics.
- [ ] Add recovery diagnostics for an online but unresponsive processor.

## Milestone 4 — Object Manager, security, and process identity

- [x] Bounded typed objects, generation-tagged handles, directory/symbolic-link foundations, generic access mapping, and post-rundown finalizers.
- [x] Initial security descriptor validation, DACL/privilege checks, token metadata hooks, and bounded audit-event recording.
- [ ] Replace global handle slots with scheduler-bound per-process handle tables and automatic process/thread identity binding.
- [ ] Implement complete token creation/duplication, primary-token and impersonation lifecycle, privilege state, and reference-safe teardown.
- [ ] Add Ob callbacks, SACL/audit policy, handle inheritance, complete namespace parsing, and broader object-type semantics.
- [ ] Validate exact supported public ABI layouts and behavior against WDK-built tests; do not emulate undocumented private structures by guesswork.

## Milestone 5 — I/O manager, PnP, power, DMA, and storage

- [x] Initial x64 WDM layouts, device creation/deletion, IRP allocation/dispatch/completion, cancel routine foundations, and device rundown.
- [x] Initial PnP/power state transitions, bounded power-IRP handling, and S4 image codec/recovery primitives.
- [x] Separate ACPI, PCI, HAL-contract, processor, and storage protocol foundations; native Rust ext-family parser.
- [ ] Complete cancellation-vs-completion, remove-lock, unload, device-stack, and file-object lifetime semantics.
- [ ] Implement PCI enumeration/resource assignment, real AHCI/NVMe hardware paths, and partition/VFS integration.
- [ ] Register and validate a real VT-d/AMD IOMMU backend; keep DMA unavailable when safe translation is absent.
- [ ] Complete device-tree/power ordering, wake handling, and end-to-end S4 persistence/resume.
- [ ] Implement crash-safe ext filesystem metadata/journal operations before enabling general writable mounts.

## Milestone 6 — Registry, NLS, executable loading, and Windows user mode

- [x] Bounded native `regf`/hbin parsing, common subkey indexes, seven hive identities, empty hive-image creation, SYSTEM service lookup, and raw BCD/NLS access.
- [x] Bounded PE32+ AMD64 loading with relocations/imports, TLS callbacks, API-set aliases, and initial unwind metadata registration.
- [ ] Implement dirty-hive log replay, registry mutation/writeback, notifications/transactions/security, and full query/enumeration classes.
- [ ] Parse NLS tables and implement code-page/locale services; discover files on separate Windows/NTFS partitions.
- [ ] Complete Windows loader, user-mode DLL/syscall compatibility, unwind/SEH, and process lifecycle integration.

## Milestone 7 — Windows 11 x64 routine compatibility

- [x] Maintain a pinned Windows 11 x64 reference manifest and compare defined implementations/resolver paths against it.
- [x] Keep internal RdotOS helpers out of the Windows export table; export only defined routines present in the pinned manifest.
- [x] Maintain routine-level Ke and Se comparison documents with scoped evidence and explicit limitations.
- [ ] Add automated ABI/signature/layout and export-table checks for every implemented subsystem.
- [ ] Expand WDK conformance tests and run representative Windows x64 drivers only after dependencies and safety prerequisites are met.
- [ ] Avoid a single headline score unless its denominator, behavior rubric, and tested revision are published alongside it.

## Tracking rules

- One canonical checkbox per deliverable; do not repeat historical milestone logs in multiple documents.
- Mark `[x]` only when the implementation is present and a relevant test or validation path exists.
- Record workflow links and commit IDs in the **Current validation** section, not scattered throughout historical notes.
- Keep dated implementation history in Git commits; keep this file focused on present state and next acceptance criteria.
- Update the README only for stable high-level facts; detailed status belongs here and in the subsystem comparisons.

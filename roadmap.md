# CognacOS Roadmap

## Kernel source organization

- [x] Move address-space and user virtual-memory implementation into `ntoskrnl/mm/address_space.rs`; keep `vm.rs` as a compatibility re-export while call sites migrate.
- [x] Move x86-64 descriptor setup, interrupt entry, exception dispatch and context-switch primitives into `ntoskrnl/ke/arch.rs`; remove the root `arch.rs` module.
- [x] Merge the former `ntoskrnl/ki` clock/interrupt dispatch runtime into `ntoskrnl/ke/runtime.rs` and remove the separate Ki module.
- [x] Remove root `vm.rs` and `vm_range.rs`; route address-space and user-range consumers through `ntoskrnl/mm/address_space`.

This is the canonical progress tracker. Checkboxes describe code and validation in the repository; they do not imply production readiness or Windows driver compatibility. Detailed per-routine status belongs in the [Ke comparison](WINDOWS11_COGNACOS_NTOSKRNL_KE_COMPARISON.md) and [Se comparison](WINDOWS11_COGNACOS_NTOSKRNL_SE_COMPARISON.md).

**Target:** a maintainable Rust-first x86-64 OS with NT-inspired interfaces and a long-term path toward Windows x64 driver compatibility.

## Current validation

- Last recorded full passing kernel/UEFI/QEMU/SMP run: [run 40159978518](https://github.com/umarfarooq7869/CognacOS/actions/runs/40159978518), commit `ed2198d2a5dbaf5820cc863297396098abcfaa58`.
- That run passed the QEMU boot smoke test and 2/4/8-vCPU SMP startup, interrupt, scheduler, and TLB-shootdown matrix. It does not validate the fault-containment and I/O-rundown items below unless the relevant tests were included.
- Later changes must be checked against the latest [Actions runs](https://github.com/umarfarooq7869/CognacOS/actions) before any milestone is marked validated. TPM-present measurement and firmware-enrolled Secure Boot remain separate validation tasks.
- Native NTDLL freestanding PE32+ build, export checks, and no-import check passed: [run 40172132060](https://github.com/umarfarooq7869/CognacOS/actions/runs/40172132060), commit `d0cea1d7ae8d84c3bca784ecbd122bb8963cfc9d`.
- QEMU boot smoke passed, including the native NT dispatcher self-test for five arguments, invalid output buffers, unsupported IDs, and 17-argument stack decoding: [run 40172070835](https://github.com/umarfarooq7869/CognacOS/actions/runs/40172070835), commit `f2fcf55bab1b49dac75bb0f1df323a39d4e97abb`. The 2/4/8-vCPU SMP stress matrix was still running at the last check, so the full workflow is not yet recorded as passing.

## Milestone 1 — Boot and platform foundations

- [x] UEFI loader and native AMD64 PE32+ kernel handoff.
- [x] Kernel GDT/IDT, page-table activation, diagnostics, physical-page allocator, and high-half mappings.
- [x] ACPI table parsing, AP startup, per-CPU GDT/TSS and IST stacks, directed IPI acknowledgements.
- [x] Build, PE validation, OVMF/QEMU boot, and multi-vCPU workflow coverage.
- [ ] Validate on physical hardware and broaden malformed-image/firmware testing.
- [x] Enforce SHA-256 and Ed25519 signature verification of the kernel PE32+ image before loading; reject missing or invalid signatures.
- [x] Detect optional UEFI Secure Boot/SetupMode state and pass the verified-kernel digest plus security flags through BootInfo v8.
- [x] Add optional TPM 2.0 TCG2 PCR 4 measurement of the verified kernel; TPM absence or measurement failure does not block boot.
- [x] Add CI signature generation, valid-signature verification, and tampered-image rejection; allow optional UEFI loader signing when a firmware-trusted certificate is configured.
- [ ] Configure and protect stable release signing keys; validate Secure Boot with an enrolled certificate and TPM 2.0 on QEMU/physical firmware.
- [ ] Implement Windows Authenticode/CI policy semantics and signed driver/module enforcement; current Ed25519 kernel signature is CognacOS-specific.
- [ ] Add relocatable-image support.

## Milestone 2 — Memory isolation and fault containment (in progress)

- [x] Validate user-copy ranges against the permitted user virtual-address interval.
- [x] Preflight all pages before multi-page user copies; hold address-space lifetime protection across validation and transfer.
- [x] Make user-page mapping roll back newly allocated intermediate page tables on supported failure paths.
- [x] Run host regression tests for range boundaries, zero-length pointers, and integer overflow.
- [ ] Add integration tests for cross-page failures, read-only destinations, mapping rollback, and map/unmap races.
- [x] Implement bounded vector/RIP trap-frame fixup records for recoverable supervisor probes, including exact CR2/error matching for page faults, explicit recovery RIP and saved-RAX result, reader-pinned unregister, and a boot self-test requiring an invalid probe to return `STATUS_ACCESS_VIOLATION`.
- [x] Route CPL=3 exceptions using the saved CS and a defined trap-frame layout; try user page-fault resolution first, then terminate only the owning process with an exception-specific exit status. Supervisor faults continue through kernel VM/fixup handling or the fatal kernel path.
- [ ] Add a passing current CI/QEMU run for the new probe regression test and adversarial CPL3 fault-isolation tests; the boot self-test is implemented but not yet confirmed by a new workflow result.
- [ ] Stress concurrent process/address-space teardown against faults, user copies, pin/unpin, and unmap; verify reference/rundown ownership and absence of use-after-free.
- [x] Integrate timer-driven preemption and verify a non-yielding kernel thread is preempted for a runnable peer in QEMU.
- [x] Add per-address-space demand-zero region metadata and resident working-set accounting APIs.
- [x] Route CPL=3 vector-14 faults to the current process's address-space demand-zero resolver; supervisor faults continue through kernel VM handling.
- [ ] Add user-process eviction/pagefile backing (currently only the kernel pageable pool has pagefile eviction/page-in).
- [ ] Add demand-zero resolver integration tests and safe concurrent unmap/fault stress.
- [x] Add non-dereferencing user-range probes and page-by-page permission validation; copy helpers preflight full ranges before transfer.
- [ ] Implement a bounded working-set trim path: select reclaimable resident pages, prevent concurrent pin/lock/remap, invalidate translations on every affected CPU, wait for TLB-shootdown acknowledgements, then reclaim physical pages only after invalidation completes.
- [ ] Test working-set trimming under multi-vCPU load, including stale-TLB access, concurrent pin/unpin, unmap, address-space destruction, allocation pressure, and rollback on incomplete shootdown.
- [ ] Harden I/O completion, cancellation, driver unload, and device/IRP rundown with race and fault-injection tests: exactly-once completion, cancel/completion interleavings, outstanding-IRP drain, remove/unload barriers, and callbacks racing teardown. Initial implementation now claims IRP completion once, tracks bounded per-IRP device references across asynchronous dispatch, prevents IRP storage from being freed during completion/cancel callbacks, and defers device deletion/unload while outstanding IRPs remain. A boot-time WDM regression checks a pending IRP holds unload and repeated completion invokes its callback once; do not mark complete until current QEMU SMP validation and kernel-level callback-vs-teardown fault injection pass.
- [x] Add deterministic host-side concurrency-model tests for exactly-once completion claims, cancel/completion races, outstanding-IRP drain, unload admission barriers, callback teardown, and injected callback faults; CI runs 512 cancel/completion/unload interleavings plus drain and completion-contention stress.
- [ ] Run the kernel WDM rundown regression under QEMU SMP/fault injection and add kernel-level callback races before claiming rundown semantics complete.
- [ ] Prove per-process kernel/user page-table isolation, syscall/trap transitions, and protection against cross-process mappings with adversarial tests. A boot-time test now creates two independent roots, maps the same user VA to distinct physical frames, checks process-local contents and rejects user access to supervisor mappings; the existing ring-3 fixture exercises IRETQ entry, syscall dispatch, and process exit. Keep this open until the latest QEMU boot and 2/4/8-vCPU runs pass and adversarial CPL3 access-fault/process-survival tests are added.
- [ ] Pass current QEMU multi-vCPU and stress/fault-injection validation before closing the milestone.

## Milestone 3 — Scheduler, waits, APCs, timers, and DPCs

- [x] Cooperative thread dispatch, scheduler wake IPIs, dispatcher object foundations, and deadline-based timer infrastructure.
- [x] Cross-processor Ke rendezvous and DPC Synchronize/Done barriers have passed a previous 2/4/8-vCPU workflow.
- [x] Consolidate classic KTIMER and KTIMER2 into one timer metadata table and expiry scan; add generation-based stale-expiry checks.
- [x] Exercise 64 queued-DPC cancellation/reinsertion cycles, in-flight callback reinitialization protection, timer cancellation, and Timer2 callback-rundown behavior in the boot self-test.
- [x] Integrate local timer interrupts, rescheduling, per-CPU ready queues, affinity-aware stealing, and delayed ready-queue publication until the outgoing RSP is saved.
- [x] Run threaded DPC callbacks only at PASSIVE_LEVEL using per-CPU workers; track queued and in-flight callbacks for flush/rundown.
- [x] Track APCs while delivering, avoid rerunning the kernel routine when a user-return frame must be retried, and reclaim completed APC records.
- [x] Add wait-any queue-entry transfer, mutant abandoned-status propagation, atomic immediate wait-all mutant acquisition, and blocking wait-all mutant reservations keyed to the waiting thread.
- [ ] Implement full Windows user APC dispatcher ABI (including safe shadow space/return trampoline) and user-mode exception/SEH resume/unwind delivery; unresolved CPL3 exceptions currently terminate only the owning process.
- [ ] Add wait-all queue-entry reservation (queue wait-any is supported) and broader WDK race/conformance stress tests.
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
- [ ] Complete cancellation-vs-completion, remove-lock, unload, device-stack, and file-object lifetime semantics; validate exactly-once completion, outstanding-IRP drain, callback rundown, and device deletion under injected races.
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
- [x] Connect native NTDLL wrappers to NTOSKRNL through CognacOS DPL3 interrupt vector `0x82`; dispatch `NtClose`, `NtYieldExecution`, and basic process/thread/system information classes to real kernel subsystems using internal service IDs `0x100`–`0x104`.
- [x] Add a boot/QEMU dispatcher self-test for five-argument process queries, invalid user output buffers, unsupported service IDs, and the bounded 17-argument stack-copy path.
- [ ] Run and inspect the latest QEMU workflow on the committed revision; test the loaded native NTDLL PE end-to-end from CPL3, and stress concurrent configuration and user-buffer mapping races. The current boot test calls the same dispatcher/copy helpers directly and does not yet prove the NTDLL DLL itself was loaded and invoked.

- [x] Implement and resolve native NTOSKRNL RTL memory exports present in the pinned Windows 11 manifest: copy/move/fill/zero, prefix and ULONG comparisons, CRC32, zero-memory testing, and unsigned multiply-high. Correct the `RtlCompareMemoryUlong` return width to Windows `ULONG` and CRC length parameters to unsigned `ULONG`. Internal helpers for ULONG-pattern fill and volatile memory operations remain callable only by kernel code until their export status is confirmed in the pinned manifest. Regression tests are now part of CI; a new CI run is still required before treating them as passing.

- [x] Maintain a pinned Windows 11 x64 reference manifest and compare defined implementations/resolver paths against it.
- [x] Keep internal CognacOS helpers out of the Windows export table; export only defined routines present in the pinned manifest.
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

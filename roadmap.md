# RdotOS Development Roadmap

This roadmap tracks code and validation currently present in the repository, not just intended architecture. A feature is marked complete only when implementation and a meaningful test or validation path exist. Completed foundations may still be experimental; they do not imply production readiness or Windows driver compatibility.

**Project target:** a Rust-first x86-64 operating system with NT-inspired kernel interfaces and a long-term path toward hosting Windows x64 drivers.

## Status at a glance

| Subsystem | Status | Main gaps |
| --- | --- | --- |
| UEFI boot and kernel handoff | PE32+ native kernel handoff; CI/QEMU validated | Signature-based verified boot, relocatable images, broader malformed-image tests |
| Memory management/protection | 4 KiB descriptor-driven bootstrap mappings, RAM direct-map, MDL validation/pinning/lifecycle helpers, pageable kernel-pool page-in, non-contiguous kernel and tracked active-process user MDL aliases, registered process-root cross-process copy | Demand-zero/pagefile-backed user faults, Windows-style exception recovery, scheduler-integrated current-process semantics, complete object/handle manager, stress and WDK conformance |
| PE32+ loader and ring-3 test path | Foundation implemented; boot-tested | Full NT loader, unwind/exception handling, Windows user-mode DLLs |
| ACPI and HAL | Foundations implemented; boot-tested | Complete interrupt routing and timer support |
| SMP/AP startup | Per-CPU GDT/TSS and dedicated NMI/double-fault/machine-check IST stacks; cooperative thread dispatch and cross-CPU wake IPIs; Ke/Ki-owned AP startup and stress sequencing | 2/4/8-vCPU CI validation pending; timer preemption, per-CPU run queues/load balancing, MDL teardown races, and real-hardware validation remain open |
| Scheduler and threads | Policy/test foundations | Production context switching, preemption, run queues |
| NT executive/syscall compatibility | 111 Ke export names documented and mapped in comparison scope; per-thread APC/alert state, timer/DPC lifecycle, queued spin locks, mutant/mutex ownership and KQUEUE implemented | Full kernel/user APC delivery, systemwide DPC drain, complete Windows opaque object layouts, remaining device queues/APC APIs, exact wait-all reservation/priority boosts, timer2 and other unresolved Ke exports |
| I/O manager | IRP lifecycle foundations | DPC/completion queues, timeouts, hardware I/O |
| DMA and IOMMU | Partial; hardware gaps remain | Real mappings, Intel VT-d initialization, AMD IOMMU |
| ext filesystem | Conservative read-only parser | Image tests, complete metadata validation, registration |
| VBS/HVCI, storage, graphics, power | Not implemented or early planning | Hardware drivers and platform support |

**Latest previously verified build:** GitHub Actions run [40083235192](https://github.com/umarfarooq7869/RdotOS/actions/runs/40083235192) passed for commit `925e6f06`. Since then, the code has added private per-CPU GDT/TSS descriptors, dedicated IST stacks for NMI/double fault/machine check, scheduler-backed cooperative thread dispatch, cross-CPU wake IPIs, and an executive-owned processor startup/self-test path. The expanded 2/4/8-vCPU matrix now asserts TSS/IST, directed-IPI, lock-contention, thread/IRP/object teardown, and VM/TLB tests; these newer changes remain unverified until the resulting Actions run passes. No QEMU result proves real-hardware reliability, timer-driven preemption, Windows SEH parity, full NT process semantics, or general Windows-driver compatibility.

### Ke / ntoskrnl compatibility pass (2026-10-24)

- [x] Keep per-thread critical/guarded-region nesting and alert state in the thread record; alertable blocked waits can be made runnable.
- [x] Implement legacy notification/synchronization timers, relative/absolute deadline conversion, periodic timer handling, and timer-driven DPC insertion.
- [x] Implement bounded DPC initialization, duplicate-safe insertion/removal, target-CPU and importance metadata, execution-state query, and per-CPU dispatch/flush at logical DISPATCH_LEVEL.
- [x] Implement FIFO ticket-lock foundations for in-stack and global queued-spin-lock exports, with saved IRQL in the handle.
- [x] Implement mutant/mutex ownership, recursion, abandoned-owner cleanup, and waiter handoff; add guarded-mutex wrappers.
- [x] Implement public-layout KQUEUE/ExecutiveQueue headers, intrusive entry lists, head/tail insertion, blocking removal, rundown, and waiter handoff.
- [x] Correct KeSetTimer and KeSetTimerEx LARGE_INTEGER x64 by-value ABI and argument order.
- [x] Strengthen wait validation: support absolute/relative timer deadlines and reject wait-all for mutant/queue objects until ownership/entry reservation can be atomic.
- [x] Expand WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md to all 111 mapped Ke export names and refresh the remaining unresolved-export inventory.
- [ ] Complete actual kernel/user APC object delivery, rundown, and delivery points.
- [ ] Add systemwide DPC dispatch/drain guarantees, timer cancellation-versus-callback rundown, and broader periodic-timer race tests.
- [ ] Complete wait-all reservation for mutants/queues, alert-versus-signal ordering, priority boosts, and timeout/cancellation race tests.
- [ ] Validate queued-spin-lock handle layout and lock-number mapping against Windows WDK ABI; implement true cross-processor queue semantics and stress tests.
- [ ] Run the freestanding build, host tests, OVMF/QEMU smoke test, and 2/4/8-vCPU SMP matrix after this pass. A passing build does not establish general Windows driver compatibility.

### Status legend

- [x] Implemented with a meaningful validation path
- [ ] Not implemented, incomplete, or awaiting required validation
- **Experimental** means code exists but is not yet reliable production support

## 1. Boot chain and kernel foundation

- [x] Rust UEFI application and separate freestanding no_std Rust kernel.
- [x] Convert the fixed-base, linked ELF64 x86-64 kernel to a PE32+ AMD64 native-subsystem image; validate PE headers, fixed image base, section RVAs, and no-import/no-relocation policy in CI.
- [x] Strict bootloader PE32+ section validation and physical backing allocation; verify the build-pinned SHA-256 digest before loading.
- [x] Enter the fixed high-half kernel through a low identity-mapped CR3 transition trampoline; validate the active CR3 and critical live bootstrap dependencies.
- [x] Versioned boot information, UEFI boot-services exit, and kernel handoff.
- [x] Build-time kernel SHA-256 pin; reject missing or mismatched digests.
- [x] Kernel-owned GDT/IDT and exception diagnostics for CPU vectors 0–31.
- [x] COM1 diagnostics and OVMF/QEMU boot validation in CI.
- [ ] Dedicated IST stacks for double-fault and NMI handling.
- [ ] Expand converter and PE-loader unit/fuzz tests for malformed inputs, overlapping sections, arithmetic overflow, unsupported directories, and permission policy.
- [x] Validate high-half PE section backing, executable entry point, and section layout; enforce 4 KiB section permissions in the kernel.
- [ ] Add more negative tests for malformed PE images and section permission combinations.
- [ ] Add framebuffer handoff and document the boot-information ABI.
- [ ] Add asymmetric signature verification, signed trust policy, key enrollment/update, and revocation.
- [ ] Add measured-boot event logging, TPM 2.0 measurements, and reproducible signed releases.

**Security boundary:** the kernel digest pin is tamper detection only when the bootloader is trusted. It is not a signature scheme or a complete verified-boot chain.

## 2. Physical and virtual memory

- [x] Initialize a physical-page allocator from UEFI conventional-memory descriptors.
- [x] Validate boot information and activate a kernel-owned CR3/page-table root.
- [x] Apply 4 KiB kernel-section permissions, CR0.WP, and NXE when supported.
- [x] Leave a kernel stack guard page unmapped.
- [x] Add private user address spaces, supervisor-only shared kernel mappings, checked user-copy helpers, and W^X validation in implemented paths.
- [x] Add kernel virtual mapping/reclamation and pagefile-backed eviction/page-in test paths.
- [x] Add NT memory-manager physical translation, address-validity, and checked-copy self-tests.
- [x] Add Windows x64 ABI wiring and typed signature records for MmCreateMdl, MmProbeAndLockPages, MmUnlockPages, MmMapLockedPagesSpecifyCache, MmUnmapLockedPages, and MmCopyVirtualMemory.
- [x] Validate MDL header/PFN-array bounds before access; preflight source mappings; maintain reference-counted physical-page pins and prevent allocator reuse while pinned.
- [x] Keep MDL PFN writes behind mapping preflight; validate page counts/capacity before walking PFN arrays; reject invalid operation modes and fail closed on unsupported mapping cases.
- [x] Build MDLs for resident nonpaged pool buffers and support direct-map aliases for MDLs whose PFNs form a physically contiguous RAM run.
- [x] Implement `MmSizeOfMdl`/`MmCreateMdl` sizing and creation paths with checked lengths and MDL header validation; track `MmCreateMdl` pool-owned headers separately from caller-owned MDLs.
- [x] Implement MmAdvanceMdl with range/PFN-array bounds checks and balanced pin-reference release for pages removed from a locked MDL.
- [x] Wire `MmCopyVirtualMemory` with the Windows x64 argument layout; validate ranges, hold registered process references and the address-space lifetime lock, translate both roots page-by-page, access physical RAM through validated direct-map pointers, and return partial-copy status when a page is unavailable.
- [x] Register stable kernel-resident EPROCESS-compatible objects with process IDs, address-space roots, state, and reference counts. `PsLookupProcessByProcessId` returns a referenced registered object; process object reference/dereference entry points maintain its lifetime. `MmCopyVirtualMemory` validates the supplied objects, holds process references, translates source and destination in their respective roots, and copies only while the shared address-space lifetime lock excludes map/protect/destroy operations. This internal structure is not the private binary layout of a Windows build; integration with a complete Windows process/object manager and concurrency stress testing remain outstanding.
- [x] Add bounded `MmFreeMdl`, `MmPrepareMdlForReuse`, and `MmBuildPartialMdl` entry points. Pool freeing is restricted to headers allocated by `MmCreateMdl`; partial MDLs validate source/target ranges and acquire independent pin references for locked source pages. Full WDK edge-case parity and stress validation remain pending.
- [x] Add arbitrary scatter/gather kernel MDL mappings: `MmMapLockedPagesSpecifyCache` maps the MDL PFN array into a contiguous supervisor VA extent without taking ownership of backing frames; `MmUnmapLockedPages` tears down only the matching external mapping and reclaims empty page tables. The current implementation accepts cached mappings only.
- [x] Fault in tracked pageable kernel-pool buffers during `MmProbeAndLockPages` through the existing page-fault/page-in handler before PFN capture; MDL ranges are checked for overflow, canonicality, and user/kernel-boundary crossing.
- [x] Add active-process user-mode MDL aliases in a reserved VA arena; track the MDL/process/root tuple, validate teardown in the same active address space, and mark user PTEs as external so address-space destruction never frees MDL-owned frames. Cached mappings only; mapping requires a registered process root to be active and resident locked PFNs. The shared IRQ-safe MDL lifecycle lock serializes probe/map/unmap/unlock bookkeeping; multi-vCPU/process-exit race testing is still pending.
- [ ] Add per-process VMA metadata and demand-zero/pagefile-backed user page-in. Absent user pages currently fail closed rather than being mapped speculatively.
- [ ] Add exception-safe `MmProbeAndLockPages` semantics (NT exception/status contract, guarded user access, and recovery from access violations); the current void ABI entry point validates resident mappings and fails closed but does not yet provide Windows SEH-style exception delivery.
- [ ] Complete Windows MDL lifecycle semantics across all allocation-source combinations and validate races/edge cases against WDK consumers, including process exit during mapping, repeated unmap/unlock, malformed PFN arrays, and mapping-table exhaustion.
- [x] Build bootstrap identity mappings from the final UEFI memory-map descriptors, leaving uncovered physical holes not-present and explicitly mapping APIC MMIO windows.
- [x] Build the bootstrap identity map with 4 KiB PTEs for every 2 MiB window and populate only pages covered by final UEFI memory descriptors; holes remain not-present, with exact 4 KiB APIC MMIO mappings added explicitly. Per-region read-only/NX protection remains a later kernel phase.
- [x] Add a demand-mapped direct-map window for UEFI-described RAM above 4 GiB; reject reserved/MMIO addresses and validate the path in the 5 GiB QEMU boot test.
- [x] Add host-side allocator-map tests and transactional descriptor parsing with firmware descriptor stride and conventional-memory filtering.
- [ ] Expand tests for allocator exhaustion, region-capacity edges, overlap/duplicate descriptors, high-memory allocation/free, and SMP concurrency.
- [ ] Add deliberate write-to-read-only and execute-from-NX fault tests.
- [x] Enter the high-half kernel and validate active CR3, kernel-section virtual-to-physical backing, current stack translation, low boot dependencies, and an unmapped virtual stack guard.
- [ ] Extend validation to every live pointer/descriptor and all future bootstrap allocations as the boot-info ABI evolves.
- [x] Keep SMAP disabled until guarded user-access paths are fully audited; SMEP and supported NX/W^X protections are configured independently.
- [ ] Enable SMAP only after all guarded user-access paths consistently use the required STAC/CLAC protocol.
- [ ] Complete pageable memory semantics, working-set policy, and robust page-fault recovery.

## 3. ACPI, architecture, HAL, and SMP

- [x] Validate ACPI RSDP and RSDT/XSDT bounds and checksums.
- [x] Parse MADT xAPIC/x2APIC processor entries and I/O APIC GSI information.
- [x] Pass firmware ACPI information through versioned boot information.
- [x] Provide xAPIC access, EOI, masked I/O APIC routing primitives, bounded ICR waits, and fixed/INIT/SIPI helpers.
- [x] Register the bootstrap processor in the HAL CPU-local registry.
- [x] Add per-CPU records, APIC-ID lookup, lifecycle states, GS-based access, typed port I/O, MMIO range checks, and IRQ-save spin locks.
- [x] Add a low-memory AP startup trampoline, temporary page-table setup, 16-page AP stacks, bounded INIT/SIPI coordination, APIC-ID verification, and online acknowledgement.
- [x] Have every AP load the kernel GDT/IDT locally, initialize its local APIC, install GS-based CPU-local state, and publish interrupt readiness before interrupt-driven IPIs are enabled.
- [x] Stress the per-CPU IDT/LAPIC route with 64 directed IPIs per online AP; missing acknowledgements fail the boot self-test.
- [ ] Validate the 2/4/8-vCPU QEMU matrix in CI and on real hardware; code and assertions are committed, but the new run is not yet confirmed.
- [x] Allocate a private per-CPU GDT/TSS descriptor and dedicated IST stacks for double fault (IST1), NMI (IST2), and machine check (IST3); install the TSS with LTR and verify descriptor/gate wiring. The TSS RSP0 follows the active cooperative kernel-thread stack.
- [ ] Add guard pages and fault-injection tests for each IST stack.
- [ ] Implement interrupt vectors, complete IRQ routing, MSI/MSI-X, and interrupt remapping.
- [ ] Calibrate and enable a reliable APIC timer and monotonic clock.
- [x] Add repeatable multi-vCPU QEMU boots for 2, 4, and 8 vCPUs, asserting per-CPU TSS/IST setup, 64 directed IPIs per online AP, scheduler wakeups, contended IRQ-save locking, thread/IRP/object lifecycle and teardown, and the VM/TLB self-test. CI results are pending.
- [ ] Add concurrent user-MDL map/unmap, process-exit, and MDL unlock-ordering stress. Mapping/probe/unlock entry points share an IRQ-safe MDL lifecycle lock, but these MDL teardown races are not yet exercised.
- [ ] Keep external interrupts disabled until interrupt entry, routing, and per-CPU state are proven safe.

Discovery and startup primitives do not by themselves establish a production-ready SMP kernel.

## 4. Scheduler, threads, and dispatcher

- [x] Connect the kernel-thread runtime to scheduler task-state/affinity metadata for create, select, yield, block, wake, resume, and exit transitions.
- [x] Add cross-CPU HAL IPI notifications when runnable work is created or woken; AP idle loops dispatch eligible threads after wakeup.
- [x] Pin each cooperative saved context to one allowed online CPU until migration-safe context publication is implemented; round-robin initial placement avoids stale-RSP migration races.
- [x] Maintain each CPU's TSS RSP0 as cooperative kernel threads switch stacks; verify context switching, lock contention, and thread/IRP/object lifecycle and teardown in the SMP stress test.
- [x] Add kernel-thread block/wake/resume paths and dispatcher self-tests for FIFO, cancellation, wait-any/wait-all, and timeout cases.
- [ ] Add per-CPU run queues, priority-aware preemption, affinity migration, load balancing, and timer-driven preemption.
- [ ] Implement timer-driven preemption and race-safe timeout-versus-signal completion.
- [ ] Implement alertable waits and NT-compatible single-object/multiple-object wait semantics.
- [x] Add a bounded eight-thread lock-contention, cross-CPU wakeup, and exit/teardown stress test; the QEMU CI assertions are pending.
- [ ] Add multi-threaded cancellation, timeout-versus-signal, and user-MDL/process-exit teardown race tests.

## 5. NT executive and Windows compatibility

- [x] Move processor startup ownership into `ntoskrnl/ke`: `KeInitializeExecutive`-style threading initialization, `KiInitializeProcessor` checks, `KiStartAllProcessors` MADT/AP startup and interrupt stress, and `KiInitializeKernel` post-startup validation. `ntoskrnl/ex` no longer owns kernel/processor initialization.
- [x] Wire additional Ke processor queries (`KeGetCurrentProcessorNumber`, `KeQueryActiveProcessors`, `KeQueryTimeIncrement`, `KeShouldYieldProcessor`) to live HAL/timer/scheduler state; unimplemented `Ki*` internals remain internal routines, not falsely advertised as Windows exports.
- [x] Add typed kernel handles, basic named-object lookup, access-mask checks, and object-manager self-tests.
- [x] Add initial device/IRP dispatch interfaces and bounded request handling.
- [x] Add initial process address spaces, checked user-copy, ring-3 entry/exit, and a small RdotOS-native syscall path.
- [x] Add a bounded PE32+ AMD64 loader with relocations, normal/delay imports, registered exports, API-set aliases, TLS setup/callbacks, and basic runtime-function registration.
- [x] Create minimal PEB/TEB structures, GS-based TEB access, TLS blocks, and loader lists for the current test path.
- [ ] Implement the Windows NT x64 syscall numbers, service table, calling semantics, and validation.
- [ ] Integrate process identity with scheduler thread switches and current-thread/current-process NT APIs; MDL user mappings currently require a registered process root to be active. Root lookup can promote a registered Created process to Running, but this is not a substitute for scheduler-driven process identity.
- [ ] Implement complete NT-compatible process/thread objects, generic object-type dispatch, per-process handle tables, namespaces, security descriptors, events, semaphores, and executive services.
- [ ] Complete loader namespaces, forwarder chains, API-set schema parsing, DllMain failure/detach handling, and concurrent lifecycle operations.
- [ ] Implement full AMD64 UNWIND_INFO interpretation, language-specific handlers, vectored exception dispatch, and recovery.
- [ ] Provide actual Windows user-mode API implementations; export fixtures are not ntdll/kernel32/advapi32 compatibility.
- [ ] Implement registry hives, ALPC/LPC, and additional executive interfaces.
- [ ] Test documented Windows x64 ABI behavior and real driver expectations.

**Compatibility rule:** export names or NT-style structures alone do not make a Windows driver loadable. ABI, object lifetime, IRQL, synchronization, memory, I/O, and device semantics must also match.

## 6. HAL exports and runtime library

- [x] Maintain a named/ordinal HAL export inventory and resolution checks.
- [x] Implement a subset of functional HAL services and fail closed when prerequisites are absent.
- [ ] Audit every export's exact Windows x64 ABI, arguments, return behavior, IRQL constraints, and ownership rules.
- [ ] Replace placeholders only when meaningful implementation and tests are available.
- [ ] Add ABI-focused export-resolution and calling-convention tests.
- [ ] Expand RTL/NT runtime functions in tested batches: memory, strings, lists, bitmaps, and synchronization.
- [ ] Count a routine as implemented only after behavior, edge cases, ABI, and tests are verified.

## 7. I/O manager, storage, DMA, and IOMMU

- [x] Add bounded IRP allocation, dispatch, completion, query, and release primitives.
- [x] Track in-flight IRPs, restrict cancellation to safe pre-dispatch state, and support explicit asynchronous completion.
- [x] Add IRQ-safe locks for request state, PCI configuration helpers, and basic bus-data operations.
- [x] Discover Intel VT-d/DMAR information from firmware tables.
- [ ] Add completion queues/DPCs, request timeouts, safe in-flight cancellation, and end-to-end asynchronous I/O.
- [ ] Implement DMA buffer pinning, scatter/gather lists, real map-register resources, cache coherency, and device-visible mappings.
- [ ] Initialize Intel VT-d hardware, domains, translation, and fault handling.
- [ ] Implement AMD IOMMU discovery and hardware initialization.
- [ ] Ensure DMA fails safely when required translation is unavailable.
- [ ] Implement PCIe enumeration, AHCI/NVMe drivers, storage queues, reset/recovery, and block-device interfaces.
- [ ] Add end-to-end I/O tests on emulated and supported physical hardware.
- [ ] Add compressed-asset streaming only after storage and memory lifetime guarantees are in place.

**Known limitations:** DMA channel scheduling and ownership handling have foundations, but map-register tokens are bookkeeping, not real hardware mappings. Intel VT-d discovery is not full initialization. AMD IOMMU is not implemented. Unsupported extended DMA operations must remain unavailable rather than pretending to work.

## 8. ext2/ext3/ext4 filesystem

- [x] Pin upstream Ext3Fsd sources as a separate reference submodule.
- [x] Add a no_std Rust parser for superblock geometry, inode metadata, legacy group descriptors, and bounded directory records.
- [x] Add checked filesystem/device bounds and legacy direct/indirect block mapping.
- [x] Add bounded ext4 extent-tree mapping and a CRC32C/superblock-checksum foundation.
- [x] Reject unsupported features rather than interpret metadata without required validation.
- [ ] Add host-side property/fuzz tests and real ext2/ext3/ext4 disk-image tests.
- [ ] Verify all consumed group-descriptor, inode, extent, directory, bitmap, and journal checksums.
- [ ] Add safe directory traversal, inode-table reads, and indexed-directory support.
- [ ] Implement a read-only backend against RdotOS block-device/filesystem interfaces and register it after image tests pass.
- [ ] Defer journal replay and write support until read-only behavior and metadata validation are robust.
- [ ] Review licensing before redistributing any Rust translation derived from upstream code.

The parser is read-only and not a production filesystem driver. The original Ext3Fsd Windows driver cannot be loaded by RdotOS.

## 9. Isolation and security

- [x] Enforce kernel write protection and NX where supported.
- [x] Add W^X checks in implemented mapping paths, a kernel stack guard page, private user address spaces, and checked user-copy primitives.
- [ ] Add per-process handle tables, reference counting, namespace security, capabilities, and access-control policy.
- [ ] Add stack canaries, compiler hardening, kernel ASLR, and read-only-after-init data.
- [ ] Add indirect-call control-flow validation and dynamic-code restrictions.
- [ ] Add systematic syscall validation, parser fuzzing, adversarial boundary tests, and resource-leak tests.
- [ ] Implement a secure-kernel/hypervisor boundary and code-integrity policy before claiming VBS/HVCI support.
- [ ] Detect virtualization, SLAT/EPT, IOMMU, and firmware prerequisites before enabling virtualization-based protection.
- [ ] Define measured boot and document the threat model.

## 10. Driver isolation, graphics, and power

- [x] Add an initial device/IRP dispatch contract and synthetic device test.
- [ ] Define stable user-mode driver IPC and production device objects.
- [ ] Implement driver host processes, timeouts, crash detection, and restart policy.
- [ ] Add permission-checked DMA through an initialized IOMMU.
- [ ] Add GOP framebuffer handoff and a display/device abstraction.
- [ ] Implement selected graphics/GPU drivers, safe VRAM mappings, and reset/recovery.
- [ ] Add ACPI power transitions, device runtime power management, wake sources, and resume recovery.
- [ ] Evaluate connected/Modern Standby-like behavior only on validated platforms.

## 11. Testing and release engineering

- [x] Build the UEFI application and freestanding kernel in GitHub Actions.
- [x] Validate the linked ELF source and generated PE32+ native kernel; package the PE32+ image in a GPT/FAT EFI System Partition.
- [x] Run host-side ext parser tests.
- [x] Boot with OVMF/QEMU (5 GiB RAM, four vCPUs) and assert serial markers for PE32+ handoff, high-half/page-table validation, direct-map/memory-manager checks, and selected subsystem self-tests.
- [ ] Add focused tests for allocator, scheduler, dispatcher, HAL exports, DMA, and parser boundaries.
- [x] Add OVMF/QEMU SMP boot and per-CPU directed-IPI stress matrix for 2, 4, and 8 vCPUs; the first CI result is pending.
- [ ] Add broader timeout/cancellation and concurrent kernel-object/MDL teardown stress tests.
- [ ] Fuzz ELF, PE, ACPI, and filesystem parsers.
- [ ] Test Secure Boot on/off after signed boot support exists.
- [ ] Document tested hardware, threat model, recovery/debug procedure, and known limitations.
- [ ] Establish tagged releases and publish test artifacts with build provenance.

## Recommended implementation order

1. Keep CI green and expand PE32+ converter/loader tests, especially malformed headers, section overlap, and permission edge cases.
2. Finish fine-grained bootstrap mapping boundaries and validate all live boot dependencies and direct-map permissions.
3. Validate the new SMP matrix, add IST guard pages/fault injection and real-hardware checks, then implement timer-driven preemption, per-CPU run queues/load balancing, and concurrent user-MDL/process-exit teardown tests.
4. Implement real DMA mappings, IOMMU initialization, PCI enumeration, and one end-to-end storage driver.
5. Expand Windows compatibility through small, tested batches of ABI-correct RTL/NT/HAL routines and executive semantics.
6. Add signed boot, stronger isolation, power management, graphics, and release-grade hardware validation.

Update these checkboxes alongside code changes, and only mark work complete after its relevant tests pass.


## Windows 11 Ke DDI comparison pass (2026-10-24)

- [x] Add 20 Ke routine implementations covering IRQL, interrupt/system time queries, processor-group and NUMA queries, current-thread identity, APC-disabled queries, and event pulse.
- [x] Add name-based ntoskrnl resolver paths for the 20 routines and include resolver self-tests.
- [x] Add [WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md](WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md), documenting public WDK signatures, return values, call paths, limitations, sources, and a reproducible compatibility calculation.
- [x] Link the comparison document from README.
- [x] Verify freestanding Rust kernel build, PE32+ conversion, UEFI bootloader build, and image validation (workflow #903; these steps passed).
- [x] Verify the OVMF/QEMU boot smoke test (workflow #903 passed the smoke-test step).
- [ ] Verify the 2/4/8-vCPU SMP boot and interrupt stress matrix (workflow #903 was still running this step at last check).
- [ ] Close documented semantic gaps: boot-epoch interrupt time, precise wall-clock interpolation, per-thread APC nesting, atomic event pulse semantics, stable NT-compatible thread-object abstraction, and multi-group/multi-node NUMA discovery.

**Current scoped score:** 20/20 (100%) source-level ABI shapes and name resolver paths; 9/20 (45%) behavior matches within the currently advertised single-group/single-node topology; combined project metric 72.5%. This is a progress estimate only and is not a statement of overall Windows 11 compatibility.

# RdotOS Development Roadmap

This roadmap tracks code and validation currently present in the repository, not just intended architecture. A feature is marked complete only when implementation and a meaningful test or validation path exist. Completed foundations may still be experimental; they do not imply production readiness or Windows driver compatibility.

**Project target:** a Rust-first x86-64 operating system with NT-inspired kernel interfaces and a long-term path toward hosting Windows x64 drivers.

## Status at a glance

| Subsystem | Status | Main gaps |
| --- | --- | --- |
| UEFI boot and kernel handoff | PE32+ native kernel handoff; CI/QEMU validated | Signature-based verified boot, relocatable images, broader malformed-image tests |
| Memory management/protection | 4 KiB descriptor-driven bootstrap mappings, RAM direct-map, MDL range validation/pinning, pageable kernel-pool page-in, non-contiguous kernel MDL mappings, registered process-root cross-process copy | User-mode MDL mappings/pageable user faults, complete Windows process/object manager, broader stress/WDK conformance |
| PE32+ loader and ring-3 test path | Foundation implemented; boot-tested | Full NT loader, unwind/exception handling, Windows user-mode DLLs |
| ACPI and HAL | Foundations implemented; boot-tested | Complete interrupt routing and timer support |
| SMP/AP startup | Experimental | Reliable multi-vCPU boot, per-CPU interrupt setup, stress tests |
| Scheduler and threads | Policy/test foundations | Production context switching, preemption, run queues |
| NT executive/syscall compatibility | Partial foundations | NT syscall ABI and complete executive semantics |
| I/O manager | IRP lifecycle foundations | DPC/completion queues, timeouts, hardware I/O |
| DMA and IOMMU | Partial; hardware gaps remain | Real mappings, Intel VT-d initialization, AMD IOMMU |
| ext filesystem | Conservative read-only parser | Image tests, complete metadata validation, registration |
| VBS/HVCI, storage, graphics, power | Not implemented or early planning | Hardware drivers and platform support |

**Latest checked verification before this documentation update:** GitHub Actions run [40079214772](https://github.com/umarfarooq7869/RdotOS/actions/runs/40079214772) succeeded for commit `f4506942` (MDL advancement lifecycle documentation). The workflow passed the repository's configured build and OVMF/QEMU validation; see the run for exact jobs and steps. It passed host allocator and ext-parser tests, kernel and UEFI-loader builds, ELF-to-PE32+ conversion and header checks, EFI image packaging, and the OVMF/QEMU smoke test with serial assertions. Documentation changes trigger another workflow run; check the badge/actions page for its result. CI covers only its listed paths and does not establish support for all hardware or Windows drivers.

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
- [x] Add active-process user-mode MDL aliases in a reserved VA arena; track the MDL/process/root tuple, validate teardown in the same active address space, and mark user PTEs as external so address-space destruction never frees MDL-owned frames. Cached mappings only; mapping requires a registered running process and resident locked PFNs.
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
- [x] Add an experimental low-memory AP startup trampoline, temporary page-table setup, AP stack preparation, and bounded acknowledgement logic.
- [ ] Validate AP startup on supported QEMU configurations and real hardware.
- [ ] Initialize per-AP GDT/TSS/IDT and CPU-local state before marking APs online.
- [ ] Implement interrupt vectors, complete IRQ routing, MSI/MSI-X, and interrupt remapping.
- [ ] Calibrate and enable a reliable APIC timer and monotonic clock.
- [ ] Add multi-vCPU race/stress tests for locks, interrupts, startup, TLB invalidation, concurrent user-MDL map/unmap, process exit, and MDL unlock ordering.
- [ ] Keep external interrupts disabled until interrupt entry, routing, and per-CPU state are proven safe.

Discovery and startup primitives do not by themselves establish a production-ready SMP kernel.

## 4. Scheduler, threads, and dispatcher

- [x] Add scheduler metadata for task states, priorities, affinity masks, selection policy, and cooperative-yield metadata.
- [x] Add kernel-thread block/wake/resume test paths and dispatcher self-tests for FIFO, cancellation, wait-any/wait-all, and timeout cases.
- [ ] Establish a complete thread control block, saved register context, initial stack frame, and tested x86-64 context-switch routine.
- [ ] Connect blocking waits to scheduler queues so blocked threads are switched out and woken threads can run.
- [ ] Add per-CPU run queues, priority handling, affinity migration, load balancing, and scheduler locking.
- [ ] Implement timer-driven preemption and race-safe timeout-versus-signal completion.
- [ ] Implement alertable waits and NT-compatible single-object/multiple-object wait semantics.
- [ ] Add multi-threaded stress tests for cancellation, wakeup, and timeout races.

## 5. NT executive and Windows compatibility

- [x] Add typed kernel handles, basic named-object lookup, access-mask checks, and object-manager self-tests.
- [x] Add initial device/IRP dispatch interfaces and bounded request handling.
- [x] Add initial process address spaces, checked user-copy, ring-3 entry/exit, and a small RdotOS-native syscall path.
- [x] Add a bounded PE32+ AMD64 loader with relocations, normal/delay imports, registered exports, API-set aliases, TLS setup/callbacks, and basic runtime-function registration.
- [x] Create minimal PEB/TEB structures, GS-based TEB access, TLS blocks, and loader lists for the current test path.
- [ ] Implement the Windows NT x64 syscall numbers, service table, calling semantics, and validation.
- [ ] Integrate process identity with scheduler thread switches and current-thread/current-process NT APIs; MDL user mappings currently require a registered running process root to be active.
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
- [ ] Add repeatable multi-vCPU, timeout, cancellation, and concurrency stress tests.
- [ ] Fuzz ELF, PE, ACPI, and filesystem parsers.
- [ ] Test Secure Boot on/off after signed boot support exists.
- [ ] Document tested hardware, threat model, recovery/debug procedure, and known limitations.
- [ ] Establish tagged releases and publish test artifacts with build provenance.

## Recommended implementation order

1. Keep CI green and expand PE32+ converter/loader tests, especially malformed headers, section overlap, and permission edge cases.
2. Finish fine-grained bootstrap mapping boundaries and validate all live boot dependencies and direct-map permissions.
3. Complete real context switching, scheduler queue integration, timer interrupts, and multi-vCPU stress tests.
4. Implement real DMA mappings, IOMMU initialization, PCI enumeration, and one end-to-end storage driver.
5. Expand Windows compatibility through small, tested batches of ABI-correct RTL/NT/HAL routines and executive semantics.
6. Add signed boot, stronger isolation, power management, graphics, and release-grade hardware validation.

Update these checkboxes alongside code changes, and only mark work complete after its relevant tests pass.

# RdotOS Development Roadmap

This roadmap tracks code and validation currently present in the repository, not just intended architecture. A feature is marked complete only when implementation and a meaningful test or validation path exist. Completed foundations may still be experimental; they do not imply production readiness or Windows driver compatibility.

**Project target:** a Rust-first x86-64 operating system with NT-inspired kernel interfaces and a long-term path toward hosting Windows x64 drivers.

## Fault-isolation hardening milestone — in progress

The current pass hardens user-buffer transfers and records the remaining isolation work explicitly. It is **not** a claim that Windows-grade process or driver fault containment is complete.

- [x] Validate user-copy ranges against the user address interval before walking page tables.
- [x] Preflight every page in a multi-page copy before transferring bytes, so an unmapped or read-only later page does not cause a partial transfer. The address-space lifetime lock is held across preflight and copy to prevent concurrent unmap/destruction from invalidating checked translations.
- [ ] Add host-side tests for cross-page copy failure, overflow, read-only destination pages, and mapping teardown races.
- [ ] Deliver user-mode exceptions through a defined trap-frame/exception path and terminate only the faulting process; current unhandled CPU exceptions still take the fatal kernel path.
- [ ] Integrate timer-driven preemption into context switching and prove that a non-yielding thread cannot monopolize the CPU.
- [ ] Complete demand-zero/page-in handling for user mappings and process working-set accounting.
- [ ] Harden driver-facing I/O completion, cancellation, and rundown with injected failure/race tests. Kernel-mode driver faults still require stronger containment and can compromise the system.
- [ ] Run QEMU multi-vCPU and stress/fault-injection suites before marking this milestone complete.


## Status at a glance

| Subsystem | Status | Main gaps |
| --- | --- | --- |
| UEFI boot and kernel handoff | PE32+ native kernel handoff; CI/QEMU validated | Signature-based verified boot, relocatable images, broader malformed-image tests |
| Memory management/protection | 4 KiB descriptor-driven bootstrap mappings, RAM direct-map, MDL validation/pinning/lifecycle helpers, pageable kernel-pool page-in, non-contiguous kernel and tracked active-process user MDL aliases, registered process-root cross-process copy | Demand-zero/pagefile-backed user faults, Windows-style exception recovery, scheduler-integrated current-process semantics, complete object/handle manager, stress and WDK conformance |
| PE32+ loader and ring-3 test path | Foundation implemented; boot-tested | Full NT loader, unwind/exception handling, Windows user-mode DLLs |
| ACPI / HAL / bus drivers | Separate ACPI, PCI, HAL-contract, and processor crates; CI image build configured for `Hal.dll`, `acpi.sys`, `pci.sys`, `processor.sys`; fixed-base kernel-VM allocation, section W^X protection, and restricted native PE32+ `DriverEntry` loader | Boot-time image discovery/loading, dependency binding, relocations/imports/unload, real HAL entry initialization, ACPI PnP/power, PCI enumeration/resources, processor power policy, CI validation pending |
| SMP/AP startup | Per-CPU GDT/TSS and dedicated NMI/double-fault/machine-check IST stacks; cooperative thread dispatch and cross-CPU wake IPIs; Ke/Ki-owned AP startup and stress sequencing | 2/4/8-vCPU CI validation pending; timer preemption, per-CPU run queues/load balancing, MDL teardown races, and real-hardware validation remain open |
| Scheduler and threads | Policy/test foundations | Production context switching, preemption, run queues |
| NT executive/syscall compatibility | Ke/Ki runtime foundations; APC safe points/rundown; processor callback table; referenced CR3 attach/detach; scheduler-backed priority bands; serialized event/semaphore/timer wait-all reservations; cross-CPU DPC barriers; bounded Object Manager with typed objects and stable type pointers, object create/insert/delete, directory and symbolic-link objects, root-relative names and symbolic-link-prefix resolution, generation-tagged owner-scoped handles and duplication with generic access remapping, process SID-list DACL evaluation, self-relative security descriptor query/set, bounded audit-event ring, and delete finalizers after reference/handle rundown. Io device objects use post-rundown finalizers; process reaping waits for Ob references/handles; scheduler-backed current process/thread queries and an explicit thread-to-process binding hook exist; Io/Ps/Cm producer hooks are present and registry key create/open paths return Object Manager handles. Host-side Object Manager lifecycle/security tests passed in CI run [40124032821](https://github.com/umarfarooq7869/RdotOS/actions/runs/40124032821) | Still not a complete Windows Object Manager: handles remain a bounded global slot array rather than per-process tables; thread/process identity binding is not automatic at every scheduler switch/create/exit; SID lists are not a complete Se token/privilege/impersonation model; no Windows Ob callback registration/invocation or SACL/audit-policy enforcement; handle inheritance and complete directory parse/security semantics remain absent; exact OBJECT_TYPE_INITIALIZER/OBJECT_HEADER ABI and full Ob/Zw export/argument/IRQL behavior are incomplete; CM key/value access, disposition and namespace semantics are partial; cross-subsystem delete/rundown coverage is not exhaustive; Ke/DPC/APC race stress and WDK conformance remain open |
| Registry / Configuration Manager | Volatile `Nt/Zw` APIs; native `regf`/`hbin` read-only parser, base-block/HBIN/cell validation, common subkey indexes, seven hive identities, empty native hive creator, SYSTEM service lookup, raw BCD element access | Dirty-hive log replay, persistent mutation/writeback, complete NT handles/access checks, full query/enumeration classes, callbacks/notifications, transactions, security, WDK tests |
| I/O manager / device lifetime | Runtime under `ntoskrnl/io/runtime.rs`; initial x64 WDM layouts, device create/delete, IRP dispatch/completion, reference/remove-lock rundown, serialized device/system power transitions, asynchronous `PoRequestPowerIrp`, cancellable wait/wake IRPs, bounded idle-detection registration and busy accounting, basic success-committed PnP lifecycle gates, and Object Manager registration/finalizer integration for device-object storage | Remaining: complete WDM/Ob ABI and all Io object types/file-object lifetime integration; complete device-tree/system S-state ordering; scheduler-backed idle worker; PnP bus enumeration, relations/resources/capabilities and query/remove ordering across stacks; device wake capability checks; cancellation/rundown race stress; registered S4 backend and pre-kernel resume loader; WDK conformance and hardware tests |
| NLS / globalization | Native-name catalogue and byte-preserving reader/copy API for `c_037.nls`, `c_437.nls`, `c_1252.nls`, `locale.nls`, `sortkey.nls`, `sorttbls.nls`, `ctype.nls`, `unicode.nls`; optional boot-asset staging | Parse NLS internal tables, code-page conversion, globalization APIs, filesystem writeback, locate files on a separate Windows/NTFS partition |
| DMA and IOMMU | Per-device adapter objects and registered backend contract; no physical-address fallback in transfer/SG paths; per-domain logical mappings; zeroed common buffers with distinct CPU and device addresses; bounded map-register capacity reserved per live transfer/SG segment and deferred release while in use; backend sync before unmap; serialized/retry-safe unmap state; IRP-bound mapping rundown hooks for cancel/completion/free; adapter release blocked while resources remain; fail-closed without a backend | Implement/register real Intel VT-d/AMD IOMMU page tables and IOTLB invalidation; verify direction/cache-coherency and full device address-mask handling; DMA fault handling and bounce buffers; race and hardware/WDK validation |
| ext filesystem | Separate `drivers/ext4` Rust crate; bounded read-only mount and guarded writes to existing extents; checked primary MBR/GPT partition discovery; CRC-validated GPT header/entry array; partition-scoped read/write adapter that rejects out-of-range I/O | NT I/O manager device/IRP dispatch integration; discover disks from real storage drivers; extended/logical MBR partitions; VFS/file objects; allocation and metadata updates; journal/checksum handling; crash consistency and image/fuzz tests |
| Storage / disk stack | New `rdotos-storage` core with AHCI port/register and lifecycle primitives, NVMe queue/doorbell and command layouts, checked geometry/transfer limits, and SCSI READ/WRITE(10/16) disk-class CDB translation; CI now tests the core and builds PE32+ `storahci.sys`, `stornvme.sys`, and `disk.sys` images | The three `DriverEntry` routines are image/loader smoke scaffolding only; PCI PnP resource discovery, DMA/IOMMU mappings, real AHCI command-list/FIS execution, NVMe admin/I/O queue programming and interrupt completions, disk PDO/FDO and IRP/SRB integration, partition-manager/VFS attachment, power/reset/rundown, and hardware/QEMU block-device tests are not implemented yet |

**Latest previously verified build:** GitHub Actions run [40083235192](https://github.com/umarfarooq7869/RdotOS/actions/runs/40083235192) passed for commit `925e6f06`. Since then, the code has added private per-CPU GDT/TSS descriptors, dedicated IST stacks for NMI/double fault/machine check, scheduler-backed cooperative thread dispatch, cross-CPU wake IPIs, and an executive-owned processor startup/self-test path. The expanded 2/4/8-vCPU matrix now asserts TSS/IST, directed-IPI, lock-contention, thread/IRP/object teardown, and VM/TLB tests; subsequent Ke/Ki affinity, Timer2, and clock-path changes are pending CI verification. No QEMU result proves real-hardware reliability, timer-driven preemption, Windows SEH parity, full NT process semantics, or general Windows-driver compatibility.

### DMA, device lifetime, and power-management groundwork (2026-10-24)

- [x] Correct `IoGetDmaAdapter` parsing to read `DEVICE_DESCRIPTION.Version` as a 32-bit field and validate bus-master/reserved fields instead of interpreting the first four bytes as a fictitious Version/Size pair.
- [x] Return version-specific DMA operation table prefixes for `DEVICE_DESCRIPTION_VERSION`, `VERSION1`, and `VERSION2`, and the full table for `VERSION3`; preserve the documented adapter ABI version value.
- [x] Require DMA MDLs to be locked or sourced from nonpaged pool before mapping their physical backing into a device IOMMU domain.
- [x] Remove physical-address fallback from WDM MapTransfer and Get/BuildScatterGatherList. Each per-device adapter is returned only when a platform IOMMU backend is registered; absent hardware translation, adapter acquisition and DMA mapping fail closed.
- [x] Add device-object reference tracking, remove-lock rundown, deferred device deletion, and serialized D-state transition bookkeeping in the I/O manager; move the `PoSetPowerState` / `PoStartNextPowerIrp` ABI implementation and the Po export catalogue into `ntoskrnl/po`.
- [x] Route `IRP_MJ_POWER` set/query requests through driver major-function dispatch; serialize device/system power transitions, commit state only after success, and retain device references across pending IRPs.
- [x] Add bounded `PoRequestPowerIrp` allocation/dispatch/callback handling and basic PnP start/query-stop/cancel/stop/remove/surprise-remove lifecycle sequencing with success-only commits.
- [x] Add cancellable `IRP_MN_WAIT_WAKE` through `PoRequestPowerIrp`, bounded per-device idle-detection registration, busy-period accounting, AC/battery-style timeout selection, and a passive-level idle poll entry point; idle polling is not yet wired to a scheduler worker.
- [ ] Complete device-tree/system S-state ordering and scheduler-backed power workers; full PnP bus enumeration, relations/resources/capabilities, wake capability validation, and race-safe cancellation across arbitrary filter stacks remain open.
- [x] Add an experimental S4 image engine: bounded physical-memory region enumeration, CPU-state capture hook, per-block RLE/raw encoding, per-region CRC-32, image/table/header validation, dual-slot generation selection, and commit-header-last persistence so an interrupted write preserves the previous valid slot. Added platform registration, device-quiesce/resume hooks, and a validate-before-restore path with codec/torn-header tests.
- [ ] Finish actual end-to-end S4: connect the backend to real persistent storage and all-device power-IRP rundown; implement a pre-kernel resume loader that restores physical memory without clobbering its executing code/stack, restores architecture-specific CPU state, and validates cold-boot/power-loss recovery on QEMU and hardware. Current hooks deliberately fail closed without a registered backend; this is not yet a Windows 11-compatible hibernation image format.
- [ ] Implement/register a real VT-d or AMD IOMMU backend that programs page tables and invalidates IOTLBs, then test actual device DMA, DMA faults, address masks, cancellation-versus-completion races, and storage-driver integration. Current callbacks define the contract but no in-tree hardware backend is registered, so DMA intentionally remains unavailable at runtime until one is installed.
- [ ] Complete Windows-compatible Object Manager semantics before any third-party driver compatibility claim: scheduler-bound per-process handle tables, full token/privilege/impersonation and security-descriptor semantics, callback/audit policy, namespace parsing and exact Ob/Zw ABI behavior, plus unified Io/Ps/Cm reference/delete rundown. Host tests and QEMU smoke are necessary but not sufficient; require WDK-built driver tests and Ke/DPC/APC race stress.

### Storage stack foundation (2026-10-24)

- [x] Add an independent `rdotos-storage` `no_std` core with checked disk geometry, overflow-safe transfer bounds, a common block-request representation, AHCI register offsets/device classification and port lifecycle state machine, NVMe submission/completion entry layouts and bounded queue/doorbell helpers, and SCSI READ/WRITE(10/16) CDB parsing.
- [x] Add native PE32+ driver-image targets named `storahci.sys`, `stornvme.sys`, and `disk.sys`; build these and run host-side protocol/class validation tests in CI.
- [ ] Replace the current minimal `DriverEntry` scaffolds with actual NT driver-object dispatch, PnP/resource acquisition, DMA-safe hardware queues and interrupts, and storage IRP/SRB completion paths.
- [ ] Integrate enumerated disk devices with partition discovery and the ext4 block-device adapter; validate reads/writes against a QEMU AHCI and NVMe device, including failure and timeout paths.

These names and image formats do **not** mean the implementations are Windows-compatible replacements yet. Microsoft documents `stornvme.sys` as a system-supplied NVMe miniport and `disk.sys` as the disk class driver within the class/port/miniport storage stack; RdotOS must implement its own compatible interfaces and hardware paths. See [Microsoft's storage driver design guide](https://learn.microsoft.com/en-us/windows-hardware/drivers/storage/) and [StorNVMe feature overview](https://learn.microsoft.com/en-us/windows-hardware/drivers/storage/nvme-features-supported-by-stornvme).

### Independent platform/driver crates (2026-10-24)

- [x] Extract the bounded ACPI parser into the `rdotos-acpi` crate and consume it as a kernel dependency.
- [x] Extract checked PCI ECAM/resource-window primitives into the `rdotos-pci` crate and consume it as a kernel dependency.
- [x] Define a versioned C-ABI HAL service table in `rdotos-hal-contract` without ACPI, PCI, or ntoskrnl dependencies.
- [x] Add a generic processor-class driver crate that validates the HAL contract and reports capabilities without vendor-specific MSR writes.
- [x] Add CI host-test steps for all four independent crates.
- [ ] Produce separate PE32+ HAL/ACPI/PCI/processor driver images with a defined entry/unload contract and loader; connect the versioned service table to the running HAL implementation.
- [ ] Move ACPI runtime discovery/power and PCI bus enumeration/resource allocation into their driver packages; add PnP, interrupt-resource, power, and hot-plug integration tests.

**Status:** source/build boundaries have begun; these crates are not yet separate loadable binaries. No Windows `.sys` compatibility is claimed.


### Native driver images and initial registry runtime (2026-10-24)

- [x] Add freestanding AMD64 `DriverEntry` entry-point binaries for the HAL, ACPI, PCI, and processor package targets.
- [x] Extend the ELF-to-PE32+ converter to emit DLL-characteristic native images at per-driver preferred bases; workflow validates AMD64, PE32+, native subsystem, entry RVA, and the currently required no-import/no-relocation profile.
- [x] Add reserved fixed-base image slots to the kernel VM arena, exact-address page allocation, and per-page permission changes with SMP TLB synchronization.
- [x] Add a restricted `ntoskrnl::driver_loader` routine that validates PE headers/sections, rejects W+X sections and unsupported imports/relocations, allocates at the preferred base, copies sections, applies W^X protections, and invokes `DriverEntry`.
- [x] Implement an initial bounded volatile registry core and resolve `Nt/ZwCreateKey`, `OpenKey`, `SetValueKey`, `QueryValueKey` (partial-information class), `DeleteValueKey`, `DeleteKey`, and `FlushKey` by exact symbol name.
- [ ] Connect boot/driver services to allocate and protect driver image mappings, load artifacts from a trusted filesystem/boot manifest, bind HAL/ntoskrnl imports, support relocation and unload/rundown, and exercise real driver initialization.
- [ ] Replace the current minimal success-returning driver entry points with actual HAL initialization and ACPI/PCI/processor runtime behavior; the current images are scaffolds, not Windows-compatible drivers.
- [ ] Extend registry semantics with persistent hive storage, canonical full paths, access/handle checks, enumeration and full information classes, callbacks/notifications, transactions, security, and WDK tests.

**Validation status:** workflow build and artifact generation have been configured but not yet observed passing for this revision. A PE image build is not evidence of a working runtime load path or Windows 11 driver compatibility.

### Storage and filesystem integration foundation (2026-10-24)

- [x] Add bounded primary MBR partition parsing with signature, empty-entry, extended-partition, arithmetic, and device-capacity checks.
- [x] Add primary GPT parsing with header CRC, entry-array CRC, usable-LBA validation, bounded standard-entry geometry, and disk-capacity checks.
- [x] Add a partition-scoped block-device adapter for reads/writes/flush. All offsets are translated with checked arithmetic and rejected when they cross the selected partition boundary.
- [x] Add host tests for MBR parsing, GPT checksum rejection, partition offset translation, and read/write boundary rejection.
- [ ] Wire a real storage driver through NT IRP dispatch into this block-device trait; current parser/adapter operates on a supplied block device and does not discover hardware by itself.
- [ ] Add GPT backup-header recovery, extended/logical MBR chains, disk hotplug/removal rundown, and fuzzed malformed-table coverage.
- [ ] Mount ext4 from the discovered partition through the kernel VFS/file-object layer and validate end-to-end read paths.

**Status:** partition isolation is implemented as a reusable crate-level foundation. It is not yet a physical storage stack and does not imply that ext4 can currently mount an arbitrary attached disk.

### Native registry hive and NLS ingestion (2026-10-24)

- [x] Add a no-conversion native Windows `regf` parser with checksum/sequence validation, HBIN/cell bounds, NK/VK traversal, and `li`/`lf`/`lh`/`ri` subkey indexes.
- [x] Add a native empty-hive image creator and catalogue for `SYSTEM`, `SOFTWARE`, `SECURITY`, `SAM`, `DEFAULT`, `COMPONENTS`, and `BCD00000000`.
- [x] Read driver service configuration from `Select\\Current` and the active `ControlSetNNN\\Services` key; add raw BCD element access for loader component policy.
- [x] Add opaque, byte-preserving access/copy support for the eight requested NLS file names.
- [x] Extend boot-info v7 to stage optional hive/NLS files from the boot volume, validate their bounds/content, and expose zero-copy kernel accessors.
- [ ] Implement hive log replay for dirty hives, registry mutation/writeback and complete native hive allocation/index updates; current hive creation only creates an empty root.
- [ ] Implement NTFS and disk/partition discovery so RdotOS can locate files on a separate Windows 11 installation rather than only the UEFI boot volume.
- [ ] Parse and consume NLS internals; current NLS support preserves raw bytes and does not yet provide Windows code-page, locale, or sorting semantics.
- [x] Implement PE AMD64 DIR64 relocations, import descriptor/thunk binding, dynamic kernel-VM image allocation, and pluggable dependency symbol resolution.
- [ ] Connect registry-selected service loading to a real filesystem resolver and boot policy; supply loaded-module dependency binding for WDF, Storport, class drivers and other imports, plus unload/rundown and actual HAL initialization.
- [ ] Validate on genuine clean Windows 11 hives and NLS assets, including large SOFTWARE/COMPONENTS hives and BCD; dirty hives require log replay before safe use.

**Validation status:** parser/NLS host-test steps are configured in CI, but the new workflow revision has not yet been observed passing. Optional assets must be accessible on the bootloader's current volume; this is not yet arbitrary Windows-partition access.

### HAL / ntoskrnl dependency boundary (2026-10-24)

- [x] Replace HAL's direct calls into `ntoskrnl::ke::runtime` with a registered processor-lifecycle notifier contract.
- [x] Move APIC implementation from the kernel root into `kernel/src/hal/apic.rs` as a private HAL child module; remove root-level `mod apic` and route audited external callers through HAL.
- [x] Keep CPU topology transitions owned by HAL; Ke registers the observer and receives add/remove-complete notifications using stable codes.
- [x] Make observer registration idempotent for the same function and reject conflicting replacement.
- [x] Route audited Ke/Ki/Io runtime clock and interrupt-priority operations through HAL service functions: monotonic counter, frequency, relative timeout deadline, local clock-event active state, cross-core clock status, and logical IRQL-to-platform priority. Direct `crate::apic` references were removed from the audited NT runtime files.
- [x] Route boot-time APIC initialization, local clock-event startup, interrupt entry/EOI, external interrupt dispatch, and clock/priority queries through HAL service functions.
- [x] Add HAL service wrappers for directed fixed-vector IPIs, INIT/SIPI startup, processor-local APIC initialization, clock synchronization, and I/O-APIC route configuration; migrate SMP startup and IPI delivery away from direct APIC module access.
- [x] Remove ACPI MADT types and discovery iteration from the HAL API; boot/platform code translates validated ACPI records into generic HAL CPU discovery calls. Remove the unused ACPI-dependent ISA route helper from the APIC implementation. APIC is private to HAL; generic MMIO/port-I/O primitives remain available through HAL.
- [ ] Add host tests for notifier registration, duplicate registration, conflicting observer rejection, and lifecycle notification ordering; run the full freestanding build and OVMF/QEMU SMP matrix.
- [x] Add a CI source guard that rejects direct HAL/APIC dependencies on ntoskrnl and ACPI types, plus direct APIC-module imports outside HAL.
- [ ] Define and validate the public HAL-facing contract independently of Windows private HAL binary layouts. Similar naming is not proof of Windows 11 ABI compatibility.

**Boundary status:** HAL/APIC has no direct `ntoskrnl` or ACPI type dependency. HAL exposes clock-event and processor-lifecycle callback contracts, owns the private APIC module, and provides service wrappers for AP startup, directed IPI, timer/priority, and I/O-APIC routing. Boot/platform code interprets ACPI MADT data and feeds generic CPU records to HAL. This is a Windows-NT-style *architectural dependency boundary* for the current x86-64 implementation, not Windows 11 private HAL ABI/binary compatibility. The new source guard is committed but remains unvalidated by a completed CI run; callback pointer representation/lifetime, a genuinely platform-neutral backend, interrupt-object/route lifecycle, and HAL contract stress tests remain open.

### Ke APC / process / scheduling follow-up (2026-10-24)

- [x] Add kernel APC queue metadata, PASSIVE_LEVEL safe-point delivery, and queued APC rundown callbacks during thread exit.
- [x] Add user APC delivery at the RdotOS ring-3 syscall IRET return path after an alertable wait; the normal routine receives RCX/RDX/R8 arguments and the interrupted RIP is used as its return address.
- [x] Add processor-change callback registration/deregistration, opaque handles, add-existing replay, in-flight accounting, and HAL online/offline notification hooks.
- [x] Add nested process attach/detach with referenced process roots, CR3 switching, LIFO restoration, and a teardown guard that refuses to destroy referenced address spaces
- [x] Synchronize Ke priority metadata with scheduler priority bands; dispatcher wake boosts now affect runnable-task ordering.
- [x] Serialize timer-triggered wait-all checks with dispatcher-object state; reuse the locked object-consumption primitive for supported event/semaphore/timer objects and reject duplicate objects.
- [x] Add x64 layout assertions for PROCESSOR_NUMBER, DISPATCHER_HEADER, semaphore/group affinity, processor-change context, and device queues.
- [ ] Run current host tests, freestanding build, PE32+ validation, UEFI/QEMU smoke, and 2/4/8-vCPU SMP matrix; the implementation commits are not considered validated until this run passes.
- [ ] Add dedicated tests for user APC frame injection/rundown, process attach nesting and destroy races, processor callback lifecycle under concurrent unregister, wait-all timeout/signal races, and priority boost/decay behavior.
- [ ] Complete exception-safe attach/detach: thread-exit rundown is implemented, but SEH/unwind-driven detach, current-process/thread ownership integration, nested-attach race tests, and address-space teardown stress remain open.
- [ ] Do not claim full Windows opaque object compatibility: private KAPC/KAPC_STATE/KTHREAD/EPROCESS bytes are not reproduced; validate public WDK size/layout expectations for each supported kit.

### Ke / ntoskrnl compatibility pass (2026-10-24)

- [x] Keep per-thread critical/guarded-region nesting and alert state in the thread record; alertable blocked waits can be made runnable.
- [x] Implement legacy notification/synchronization timers, relative/absolute deadline conversion, periodic timer handling, and timer-driven DPC insertion.
- [x] Implement bounded DPC initialization, duplicate-safe insertion/removal, target-CPU and importance metadata, execution-state query, and per-CPU dispatch/flush at logical DISPATCH_LEVEL.
- [x] Implement FIFO ticket-lock foundations for in-stack and global queued-spin-lock exports, with saved IRQL in the handle.
- [x] Implement mutant/mutex ownership, recursion, abandoned-owner cleanup, and waiter handoff; add guarded-mutex wrappers.
- [x] Implement public-layout KQUEUE/ExecutiveQueue headers, intrusive entry lists, head/tail insertion, blocking removal, rundown, and waiter handoff.
- [x] Correct KeSetTimer and KeSetTimerEx LARGE_INTEGER x64 by-value ABI and argument order.
- [x] Strengthen wait validation: support absolute/relative timer deadlines and reject wait-all for mutant/queue objects until ownership/entry reservation can be atomic.
- [x] Expand WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md to 169 tracked Ke export names and refresh the unresolved-export inventory.
- [ ] Validate and complete APC delivery semantics. Current code has side-table APC metadata, kernel APC safe-point delivery and thread-exit rundown; user APCs are delivered by rewriting the RdotOS ring-3 syscall IRET frame only after an alertable wait. Add user-stack/trap-frame tests, SEH/exception unwind integration, callback suppression/requeue cases, APC disable/migration tests and Windows WDK conformance. No WDK test suite has been run in the current environment.
- [x] Validate cross-CPU generic-DPC rendezvous and Synchronize/Done barriers in the 2/4/8-vCPU QEMU CI matrix; latest run #1162 reports the barrier test passed on every online CPU.
- [x] Remove the bounded-round early return from `KeFlushQueuedDpcs`; it now continues rendezvous/drain rounds until queued and in-flight DPC work and remote dispatch activity are clear. DPC side metadata now tracks per-object callback execution and refuses reinitialization while that object is queued or running. This is a synchronous completion implementation, not a CI-verified Windows-equivalent guarantee yet; an online CPU that stops participating can stall the flush rather than silently return early.
- [x] Add timer-generation tracking for legacy timers and Timer2; invalidate stale periodic expiry snapshots on cancellation/rearm, serialize timer DPC enqueue/removal with timer state, and remove a recursive dispatcher-object lock acquisition in timer expiry processing.
- [ ] Stress timer cancellation-versus-expiry, periodic timer rearm, DPC reinitialization/removal during callback, processor-offline races, and flush-versus-concurrent-insert. Add targeted WDK-derived conformance cases before marking rundown behavior complete.
- [ ] Complete exact wait-all and scheduler behavior. Event/semaphore/timer reservations are serialized under the dispatcher-object lock and duplicate objects are rejected; mutant ownership/abandonment and queue-entry reservations, alert-versus-signal ordering, timeout/cancellation races, priority decay and Windows quantum policy remain open.
- [ ] Validate queued-spin-lock handle layout and lock-number mapping against Windows WDK ABI; implement true cross-processor queue semantics and stress tests.
- [x] Latest Ke-runtime workflow passed host-side tests, freestanding build, PE32+ validation, UEFI loader build, image packaging, OVMF/QEMU smoke, 2/4/8-vCPU SMP stress, and artifact upload: [run #997](https://github.com/umarfarooq7869/RdotOS/actions/runs/40092050955).
- [ ] Continue expanding DDI conformance tests; a passing build and SMP run do not establish general Windows driver compatibility.

### Ke/Ki extension pass (2026-10-24)

- [x] Add 22 name-resolved single-group affinity bitmap routines: initialize, processor/group add/remove, boolean operations, count/find/test, equality/subset/empty, and atomic set/clear.
- [x] Add KeGetXSaveFeatureFlags using CPUID XSAVE/OSXSAVE checks and XGETBV; return zero when the feature is not enabled.
- [x] Add KeDynamicPartitioningSupported fixed-topology result.
- [x] Add distinct KeInitializeTimer2, KeSetTimer2, and KeCancelTimer2 side-table operations with APIC-driven one-shot/periodic callbacks; timer2 is not aliased to legacy KTIMER.
- [x] Add an internal ntoskrnl/ki runtime layer and route APIC clock interrupts through Ki before completing dispatcher timeouts.
- [x] Expand the Ke comparison to 169 tracked names and recalculate the project-defined scoped score to 65.1% (100% declaration/resolver coverage, 51/169 strict behavior matches).
- [ ] Verify the latest affinity/Timer2/Ki changes with the freestanding build, PE32+ conversion, OVMF/QEMU smoke, and 2/4/8-vCPU stress matrix. A score update is not a substitute for CI or Windows-driver tests.
- [ ] Validate processor-change callbacks and process attach/detach. Registration/deregistration, in-flight callback accounting, HAL lifecycle hooks, referenced CR3 switching and LIFO restoration are implemented, but full hot-remove orchestration, exception-safe detach, current-process/thread integration and SMP/process-exit stress remain open. NMI callback lifecycle, logical-processor relationship buffers and XSAVE save/restore ownership are also unresolved.

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
- [x] Extend the bounded Object Manager with typed objects and stable type pointers, object creation/insertion/deletion, directories, symbolic-link objects and prefix traversal, generation-tagged owner-scoped handles, duplication with generic-access remapping, SID-list DACL checks, self-relative security descriptor query/set, a bounded audit-event ring, and finalizers invoked after reference/handle rundown. Io device objects register finalizers so storage is not reclaimed while Ob handles/references remain; process reaping waits for Ob rundown; registry key create/open uses Object Manager handles and defers key-slot reuse until rundown. `PsGetCurrentProcess[Id]` and `PsGetCurrentThread[Id]` resolve to scheduler-backed implementations, but threads still require explicit process-ID binding. Host tests cover type/object lifecycle, owner isolation, symbolic-link traversal, finalizer ordering, access/duplicate-handle mapping, and concurrent reference/handle rundown; this test step passed in CI run [40124032821](https://github.com/umarfarooq7869/RdotOS/actions/runs/40124032821).
- [x] Add bounded cross-subsystem Object Manager integration: Io device objects are registered with the manager and defer storage reclamation until Ob reference/handle rundown; Ps process reaping checks Ob rundown before reusing process storage; Cm registry key create/open paths use Object Manager handles and key-slot reuse is deferred until rundown; scheduler-backed current process/thread query exports and an explicit thread process-ID binding hook feed handle ownership. These are integration points, not yet complete lifetime coverage across all object types or automatic process identity at every scheduler transition.
- [ ] Add race-focused Ke/DPC/APC stress coverage: global DPC flush versus concurrent insertion/rundown, timer cancel/rearm versus expiry, kernel/user APC delivery versus thread exit, and process-exit versus queued APC/DPC cleanup on 2/4/8-vCPU boots.
- [ ] Add a Windows WDK conformance job and signed test-driver matrix for object type/object creation, handle duplicate/close/access-denied cases, directory-relative opens, symbolic-link queries, security descriptor changes, Io device delete with outstanding handles, Ps process exit, and Cm key deletion. Linux host tests and QEMU smoke are not substitutes for WDK execution or Windows driver-load validation.
- [x] Add initial device/IRP dispatch interfaces and bounded request handling.
- [x] Add initial process address spaces, checked user-copy, ring-3 entry/exit, and a small RdotOS-native syscall path.
- [x] Add a bounded PE32+ AMD64 loader with relocations, normal/delay imports, registered exports, API-set aliases, TLS setup/callbacks, and basic runtime-function registration.
- [x] Create minimal PEB/TEB structures, GS-based TEB access, TLS blocks, and loader lists for the current test path.
- [ ] Implement the Windows NT x64 syscall numbers, service table, calling semantics, and validation.
- [ ] Make thread-to-process identity automatic and authoritative on process/thread creation, context switch, attach/detach, and exit. `PsGetCurrentProcess[Id]`/`PsGetCurrentThread[Id]` now resolve to scheduler-backed routines, but the binding hook is still explicit; MDL user mappings also require the correct registered process root to be active.
- [ ] Complete NT-compatible process/thread objects and integrate them with the Object Manager, including automatic process binding, truly per-process handle tables (not a global owner-tagged slot array), full Se token/privilege/impersonation and security-descriptor semantics, events/semaphores, and executive services.
- [ ] Complete loader namespaces, forwarder chains, API-set schema parsing, DllMain failure/detach handling, and concurrent lifecycle operations.
- [ ] Implement full AMD64 UNWIND_INFO interpretation, language-specific handlers, vectored exception dispatch, and recovery.
- [ ] Provide actual Windows user-mode API implementations; export fixtures are not ntdll/kernel32/advapi32 compatibility.
- [ ] Implement registry hives, ALPC/LPC, and additional executive interfaces.
- [ ] Test documented Windows x64 ABI behavior and real driver expectations.

**Compatibility rule:** the implemented Object Manager is a bounded foundation, not “fully Windows compatible.” The host-side lifecycle/security test step passed in CI run [40124032821](https://github.com/umarfarooq7869/RdotOS/actions/runs/40124032821). Release blockers remain: per-process handle tables and automatic current-process identity on scheduler transitions; full Se tokens, privileges and impersonation; Ob callback registration/invocation and SACL-driven audit policy; inheritance and complete namespace parsing; exact object/type/header ABI and full Ob/Zw signatures, access modes, IRQL and error behavior; complete Io/Ps/Cm lifetime/security integration; broader Ke/DPC/APC race stress; and Windows WDK-built driver conformance/load tests. Passing host-side Rust tests or QEMU smoke does not establish Windows driver compatibility.

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
- [x] Add optional monotonic deadlines to IRPs, expire queued requests before dispatch, and complete late in-flight requests as timed out. In-flight hardware cancellation remains driver-specific.
- [x] Add bounded FIFO I/O completion ports with completion packet posting/dequeue and a completion-to-port helper. The dequeue API is nonblocking and still needs integration with dispatcher wait objects.
- [x] Move the I/O manager runtime under `kernel/src/ntoskrnl/io/runtime.rs` while preserving the existing `crate::io` call sites through an explicit module path.
- [x] Resolve `IoIsWdmVersionAvailable` with a conservative WDM 1.0 capability gate and `IoGetConfigurationInformation` with the public seven-counter structure, plus paired cancel-spin-lock exports that raise/restore IRQL; live device enumeration and most NT I/O ABI exports remain unfinished.
- [x] Add 16-bit port I/O and typed volatile MMIO accessors with ordering fences for MMIO writes.
- [x] Track in-flight IRPs, restrict cancellation to safe pre-dispatch state, and support explicit asynchronous completion.
- [x] Add IRQ-safe locks for request state, PCI configuration helpers, and basic bus-data operations.
- [x] Discover Intel VT-d/DMAR information from firmware tables.
- [ ] Integrate completion-port waits with dispatcher objects and DPC-driven completion; add safe driver-specific in-flight cancellation and end-to-end asynchronous I/O.
- [x] Build per-device IOMMU logical mappings for locked/nonpaged MDL transfers and scatter/gather fragments; keep physical addresses out of device-visible SG elements and remove identity/physical fallback.
- [x] Allocate zeroed common buffers from contiguous backing pages, return a CPU-accessible direct-map pointer separately from the device-visible logical address, and unmap the logical aperture before releasing backing pages.
- [x] Reserve bounded map-register capacity against each active transfer/SG mapping; reject exhausted budgets and defer freeing a token while references remain in flight. Return capacity only after backend synchronization and successful unmap.
- [x] Serialize mapping teardown and preserve a mapping record when backend sync/unmap fails, so cleanup can be retried rather than silently dropping ownership. SG teardown tracks per-element progress and retains ownership if any mapping cannot be released.
- [x] Add explicit IRP-binding/release hooks and invoke mapping rundown from cancellation, completion, and final IRP free paths. Driver cancel routines must stop device DMA before returning; callers must bind mappings to their IRP for IRP-scoped rundown.
- [x] Prevent adapter release while that device still owns active mappings, common buffers, or map-register allocations; fail closed when no registered IOMMU backend is available.
- [ ] Add focused automated tests for IOMMU backend failure/retry, SG partial teardown, mapping-vs-cancel/completion races, and common-buffer lifetime; existing map-register tests cover deferred release while mappings remain active.
- [ ] Initialize Intel VT-d hardware, domains, translation, and fault handling.
- [ ] Implement AMD IOMMU discovery and hardware initialization.
- [ ] Ensure DMA fails safely when required translation is unavailable.
- [ ] Implement PCIe enumeration, AHCI/NVMe drivers, storage queues, reset/recovery, and block-device interfaces.
- [ ] Add end-to-end I/O tests on emulated and supported physical hardware, including timeout-vs-completion races, completion-port saturation/FIFO, and device MMIO/port-I/O access.
- [ ] Add compressed-asset streaming only after storage and memory lifetime guarantees are in place.

**Known limitations:** the IOMMU callback contract and software-side mapping/accounting lifecycle are implemented, but no in-tree backend currently programs Intel VT-d or AMD IOMMU hardware. Adapter acquisition therefore fails closed unless a real backend registers; the mapping paths are not yet hardware-validated. Map-register tokens enforce bounded software capacity but are not hardware map-register descriptors. Direction-specific synchronization semantics, full device address-mask handling, non-coherent cache maintenance, DMA fault handling, AMD IOMMU support, and storage-driver integration remain incomplete. IRP rundown only covers mappings explicitly bound to an IRP, and driver cancellation must quiesce the device before releasing mappings.

## 8. ext2/ext3/ext4 filesystem

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

The ext-family parser now lives in a standalone source/build crate and has a constrained write path for already allocated initialized data extents. This is not yet a complete writable filesystem: allocation, metadata updates, journal handling, crash consistency, and VFS/block-device registration remain outstanding. No third-party Windows filesystem driver sources are included in the repository.

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


## Windows 11 Ke executive implementation pass (2026-10-24)

- [x] Move critical/guarded-region nesting into each schedulable thread record; retain a bounded bootstrap-only counter before a thread exists.
- [x] Track per-thread alert state, cancel dispatcher wait registrations on alert, wake alertable blocked threads, and return STATUS_ALERTED from alertable waits/delays.
- [x] Add legacy notification/synchronization timer lifecycle, relative/absolute deadline conversion, periodic timers, cancellation, signal/waiter completion, and timer DPC queueing.
- [x] Add bounded DPC registry and queue, duplicate-insertion detection, removal, callback execution, target-processor selection, importance metadata, threaded-DPC registration metadata, and queue draining.
- [x] Add ticket-ordered in-stack queued spin locks, DPC-level variants, queue-number locks, and SYNCH_LEVEL acquisition variants.
- [x] Add mutant/mutex owner tracking, recursion, release/waiter handoff, abandoned-owner cleanup, and guarded-mutex wrappers.
- [x] Add executive KQUEUE initialization, FIFO/head insertion, dequeue, direct handoff to a waiting thread, and rundown.
- [x] Strengthen single/multiple-object waits with timer dispatcher types, absolute timeout conversion, alert cancellation, and fail-closed rejection of unsupported multi-wait queue/mutant cases.
- [x] Add focused Ke tests for APC nesting, processor-number ABI, DPC duplicate queueing/callback execution, timer expiration, mutant initialization, and executive queue ordering/rundown.
- [x] Expand [WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md](WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md) to 129 scoped Ke names and public contract / stub inventory.

### Remaining correctness and validation gaps

- [ ] Run the latest workflow to completion, including OVMF/QEMU smoke and the 2/4/8-vCPU stress matrix; fix any compiler/test failures before calling this pass validated.
- [ ] Implement actual kernel/user APC objects, delivery, rundown, and alert-vs-APC status distinctions.
- [ ] Run threaded DPC callbacks on PASSIVE_LEVEL worker threads; current threaded-DPC metadata still dispatches at DISPATCH_LEVEL.
- [x] Validate cross-processor `KeGenericCallDpc` and Synchronize/Done completion barriers across the 2/4/8-vCPU QEMU matrix (workflow run #1162).
- [x] Remove the bounded-round early return from `KeFlushQueuedDpcs`; it now repeats cross-processor rendezvous and checks queued, in-flight, and remote-dispatcher state before returning. The flush can still wait indefinitely if a processor remains marked online but stops participating.
- [ ] Add safe recovery/diagnostics for an online-but-unresponsive CPU, and stress target-CPU delivery plus timer/DPC rundown races.
- [ ] Integrate timer deadlines with per-CPU clock events, preemption, suspend/resume, and race-tested timeout/signal completion.
- [ ] Complete KQUEUE concurrency limits, wait-list scheduling, rundown lifecycle, and Windows-compatible queue result semantics.
- [ ] Validate mutant/guarded-mutex recursion, abandoned-state return codes, and all release-vs-timeout races against WDK behavior.
- [ ] Replace fixed-capacity timer/DPC/mutant/queue side tables with lifetime-safe executive objects or dynamic bounded allocation.
- [ ] Add concurrent tests for wait-any cancellation, wait-all reservation, queue handoff, DPC cancellation/flush, and timer cancel-vs-expire races.

**Scope note:** The comparison now lists 132 Ke export names. Its current project-defined score is 53.9% (100% source-level declaration coverage and 7.8% strict behavior matches in the currently advertised topology); this is a scoped progress metric, not a Microsoft metric or a Windows driver-compatibility claim.


## Additional Ke export implementation batch (2026-10-24)

- [x] Implement KeQueryUnbiasedInterruptTimePrecise with QPC sample output; suspend/resume bias remains unmodelled.
- [x] Implement KeGetClockTimerResolution and KeGetNextClockTickDuration using the configured 1-ms quantum and active timer deadline table.
- [x] Implement KeFlushIoBuffers with x86-64 ordering fences for the current coherent-DMA target; non-coherent cache maintenance remains unsupported.
- [x] Implement KDEVICE_QUEUE initialization, FIFO insertion, keyed insertion, busy-state transitions, and removal.
- [x] Add per-thread KeSetPriorityThread, KeQueryPriorityThread, and KeSetBasePriorityThread metadata with Windows priority-range validation; scheduler priority ordering/boosts remain incomplete.
- [x] Implement terminal KeBugCheck/KeBugCheckEx paths that record code/parameters and halt with interrupts disabled; crash dump/diagnostic reporting remains incomplete.
- [x] Wire these 13 names through ntoskrnl import resolution and update the Ke comparison to 135 callable names.
- [x] Verify freestanding Rust build, PE32+ conversion, UEFI image validation, and OVMF/QEMU boot smoke (workflow #995 passed these steps).
- [ ] Finish the 2/4/8-vCPU SMP stress matrix (workflow #995 was still running this step at last check).
- [ ] Implement remaining Ke interrupt-spin-lock, timer2, APC, cross-CPU generic DPC/IPI, NMI callback, processor-change callback, interrupt synchronization, logical-topology, affinity, process-attach, and extended-state APIs.

- [x] Add conservative per-interrupt spin-lock tokens and KeSynchronizeExecution callback serialization; hardware ISR synchronization remains limited until the HAL owns interrupt-object SynchronizeIrql and ISR locks.
- [x] Wire KeAcquireInterruptSpinLock, KeReleaseInterruptSpinLock, and KeSynchronizeExecution into the resolver and comparison.
- [ ] Add hardware interrupt-object locking and verify synchronization callbacks against a real ISR path.

- [x] Add group-0 KeSetAffinityThread, KeSetSystemGroupAffinityThread, and KeRevertToUserGroupAffinityThread with scheduler-backed masks and bounded restore state; safe live-context migration and multi-group affinity remain unsupported.
- [ ] Verify the affinity and interrupt synchronization changes through the latest complete QEMU/SMP workflow.


## Ke cross-processor rendezvous implementation (2026-10-24)

- [x] Implement `KeIpiGenericCall` with one broadcast callback invocation per online processor and synchronous completion on the caller.
- [x] Implement `KeGenericCallDpc` with per-CPU callback execution at logical DISPATCH_LEVEL.
- [x] Implement `KeSignalCallDpcSynchronize` and `KeSignalCallDpcDone` using shared synchronization and completion barriers.
- [x] Route a dedicated Ke broadcast IPI vector through the active rendezvous descriptor and execute callbacks on the target CPU before local-APIC EOI.
- [x] Add import-resolver mappings and self-test assertions for all four exports.
- [x] Run a boot-time Ke rendezvous self-test that checks exactly-once callback counts and DPC Synchronize/Done barrier returns on the online CPU set.
- [x] Update `WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md` with public signatures, call paths, limitations, and the recalculated project score.
- [x] Validate broadcast callback counts and DPC Synchronize/Done barriers in QEMU with 2, 4, and 8 vCPUs (workflow #1162).
- [ ] Add timeout/failure recovery for an online-but-unresponsive processor and validate under concurrent HAL IPI stress.
- [ ] Add bounded timeout/failure recovery for a processor that goes offline or fails to service the broadcast IPI.
- [ ] Validate that HAL IPI dispatch does not starve reschedule/TLB work under simultaneous SMP stress.

The new routines are marked partial until the multi-vCPU workflow passes. The current comparison reports 169 tracked names, 100% source-level declaration/resolver coverage, 51/169 strict scoped behavior matches (30.2%), and a project-defined combined score of 65.1%; this is not a Windows driver compatibility guarantee.


## Ke runtime maintainability refactor (2026-10-24)

- [x] Split the monolithic Ke runtime source into functional source files under `kernel/src/ntoskrnl/ke/runtime/`.
- [x] Keep `runtime.rs` as the existing resolver-facing facade and preserve routine signatures and shared state during the source-only refactor.
- [x] Group processor/IRQL, dispatcher objects, time, waits/APC, interrupt/scheduler, device queue, executive queue, spin locks, mutexes, affinity, and timer/DPC/rendezvous code separately.
- [x] Validate the source-fragment refactor with freestanding build, UEFI/QEMU smoke, and the 2/4/8-vCPU SMP workflow (#1162); the later standalone timer-test and cancellation changes still require a fresh workflow run.
- [ ] In a later cleanup, turn the included source fragments into independently scoped Rust modules after defining explicit shared-state interfaces.


### Timer/DPC expiry-generation regression coverage (2026-10-24)

- [x] Centralize the timer expiry snapshot validity predicate: an expiry may proceed only when both object identity and nonzero generation match the live record.
- [x] Use the same predicate for legacy timer DPC enqueue validation and Timer2 callback dispatch validation.
- [x] Add focused unit tests for valid snapshots, object reuse, cancellation/rearm generation changes, and uninitialized generations.
- [x] Extract the pure predicate and tests to `kernel/src/ntoskrnl/ke/timer_generation.rs`, include it in the Ke runtime, and add a dedicated host-test step to `.github/workflows/build-uefi.yml`.
- [x] Invalidate stale legacy-timer and Timer2 expiry snapshots on every cancellation call, including cancellation after a one-shot has already transitioned inactive; centralize nonzero generation advancement and test wraparound.
- [x] Consolidate classic KTIMER and NT-style KTIMER2 into one timer metadata table and one APIC-driven expiry scan; remove the parallel `TIMERS2` table and `timer2_tick` loop.
- [x] Keep delivery semantics in the shared expiry loop: classic timers signal dispatcher waits and optionally queue a KDPC; KTIMER2 records invoke their registered callback at logical DISPATCH_LEVEL.
- [x] Remove a legacy timer's queued associated KDPC on cancel/rearm even after a one-shot has become inactive; executing-callback rundown remains open.
- [x] Add a per-record Timer2 callback rundown claim under the unified timer lock; refuse reinitialization while a callback is running and allow cancellation to invalidate not-yet-claimed expiry snapshots.
- [ ] Define and test the exact cancellation-versus-in-flight callback contract; cancellation currently does not wait for a callback already claimed for execution.
- [ ] Implement KTIMER2 tolerable-delay coalescing; the tolerance is currently retained but not applied.
- [x] Inspect workflow #1162 logs: the preceding revision passed host allocator/filesystem tests, freestanding build, PE32+ conversion/validation, UEFI packaging, OVMF/QEMU smoke, and 2/4/8-vCPU SMP stress; Ke broadcast rendezvous and Synchronize/Done barriers passed on every online CPU.
- [ ] Wait for the workflow triggered by this commit to validate the new standalone Ke timer-generation tests and current source revision.
- [ ] Continue with integration stress for concurrent cancel/rearm versus expiry, DPC removal/reinitialization during callback, and flush-versus-insert. Predicate tests alone do not prove those interleavings race-free.


### Ext filesystem and NT I/O integration (2026-10-24)

- [x] Remove the third-party filesystem-driver submodule declaration, gitlink, and source-inventory document; keep the native Rust ext-family parser in `drivers/ext4`.
- [x] Establish x64 WDM object prefixes and layout tests for `DRIVER_OBJECT`, `DEVICE_OBJECT`, `IRP`, `IO_STACK_LOCATION`, `UNICODE_STRING`, and `IO_STATUS_BLOCK`.
- [x] Implement bounded `IoCreateDevice`/`IoDeleteDevice`, named device registration/lookup, attachment/detachment, attached/base/lower-device queries, and stack-size propagation.
- [x] Implement IRP allocation/free/initialization/reuse, stack-location helpers, dispatch, completion-routine unwinding, and propagation of final status/information to a caller `IO_STATUS_BLOCK`.
- [x] Add cancel-routine registration and one-shot cancel callback claiming under the cancel-spin-lock protocol.
- [x] Add MDL allocation/freeing using the memory manager's MDL lifecycle; asynchronous READ/WRITE request construction can pin pages for direct I/O and rejects buffered I/O until a real system-buffer pool exists.
- [x] Add bounded filesystem registration/unregistration and counted UTF-16 symbolic-link create/delete primitives.
- [ ] Harden cancellation-versus-completion races, IRP ownership, device remove rundown, stack completion edge cases, and driver unload with dedicated concurrent tests; these routines are not yet safe for arbitrary WDK binaries.
- [ ] Implement `FILE_OBJECT` lifecycle and `IoGetDeviceObjectPointer`, VPBs, mount-volume arbitration, and filesystem-control dispatch.
- [ ] Add real block-device/partition driver contracts and route ext4 read/write/flush requests through IRPs with alignment validation, correct buffer ownership, completion status propagation, and device-removal rundown.
- [ ] Integrate ext4 into a VFS with mount/file objects, pathname lookup, inode/directory operations, safe allocation and metadata mutation, journal recovery/commit, and power-loss tests before enabling general writable mounts.

**Safety status:** the WDM implementation is an initial compatibility base, not a production NT I/O manager. The ext4 crate is not yet a production-ready mounted filesystem and must not be mounted read-write on valuable data. Its current write path is limited to existing allocated extents and is not transactional; it does not provide full inode/metadata updates or rollback after partial I/O failure.


## Windows 11 Se security reference monitor pass (2026-10-25)

- [x] Move Se runtime implementation source under `kernel/src/ntoskrnl/se/`, retaining `se.rs` as the export catalogue/module facade so existing export enumeration remains stable.
- [x] Implement bounded `SeValidSecurityDescriptor` validation for self-relative descriptors: revision/control checks, SID and ACL structural validation, ACE bounds, offset bounds, and overlapping-component rejection.
- [x] Resolve `SeValidSecurityDescriptor` by name to its implementation instead of allowing the export catalogue alone to imply support.
- [x] Add focused unit tests for minimal self-relative descriptors and malformed owner/SID offsets.
- [ ] Run the current CI workflow and ensure the security runtime tests are included in the actual host-test path.
- [ ] Implement ABI-verified subject-context capture/release and token query/privilege operations after RdotOS has real process/thread token ownership and privilege state.
- [ ] Integrate Se access checks with Ob's DACL evaluator, generic mapping, privilege evaluation, process identity, impersonation, and audit/SACL policy; do not claim Windows parity from descriptor validation alone.
- [ ] Add malformed descriptor fuzz/property tests and fault-safe user-buffer capture before resolving routines that dereference caller-provided descriptors.

**Scope limitation:** the current validator assumes the supplied pointer references a readable 4-KiB window because the NT ABI does not provide an input length. It checks descriptor-relative offsets within that bounded window, but it cannot safely probe arbitrary user memory. It must not be treated as a substitute for `SeCaptureSecurityDescriptor` or full access-check semantics.


### Additional Se bounded validation primitives (2026-10-25)

- [x] Add length-aware RdotOS extension helpers for SID validation/equality and ACL structural validation.
- [x] Add explicit-length self-relative descriptor control/component queries with bounds checks and output-pointer validation.
- [x] Add pure privilege-bitmap and allow/deny mask predicates, plus focused tests for truncation and malformed ranges.
- [ ] Run the current workflow and verify the new Se host tests compile and execute.
- [x] Add a bounded `SeAccessCheck` export and exact-name resolver path for self-relative descriptors, generic mapping, owner `READ_CONTROL`/`WRITE_DAC`, and basic allow/deny ACE evaluation against explicitly registered token SIDs and Everyone.
- [x] Add `SePrivilegeCheck` with exact-name resolution, bounded `PRIVILEGE_SET` parsing, all-necessary/any-necessary semantics, and registered-token privilege bitmap evaluation.
- [x] Add an internal copied-SID token identity registry with validation, unregister/rundown hook, and a focused access-granted unit test.
- [ ] Complete automatic SID/privilege registration in a real Ps token allocator and creation/duplication pipeline; current process primary-token assignment, thread impersonation assignment, and token metadata duplication hooks are wired, but no full Windows TOKEN allocator exists.
- [x] Add restricted-token second DACL evaluation, bounded privilege-assisted grants, supported SACL audit ACE evaluation, and an internal copied-SID/AppContainer capability registry.
- [x] Add subject-context capture/release and serialized lock/unlock exports, token reference accounting, and primary/impersonation token rundown.
- [x] Add page-table-validated user-memory copy primitives and bounded self-relative security descriptor capture into kernel storage.
- [ ] Complete conditional callback ACE expression evaluation, GUID-qualified object-type-list semantics, full privilege-set output allocation, comprehensive audit policy, and Windows AppContainer package SID restrictions.

- [x] Extend bounded inspection with SID authority fields, ACL metadata/ACE-type counting, basic SID-in-ACL search, descriptor component presence, and access/privilege mask utilities.


**SeAccessCheck status (2026-10-25):** a real, name-resolved Windows-named entry point now exists, with a deliberately bounded basic-ACE evaluator and an internal token-SID registration hook. It is not full Security Reference Monitor compatibility: token lifecycle is not wired automatically, the routine requires a caller-locked subject context, descriptor access assumes a readable 4-KiB window, unsupported ACE families fail closed, and WDK conformance has not been established.


## Security Reference Monitor integration pass (2026-10-25)

- [x] Correct SeAccessCheck ABI to use a BOOLEAN pointer for SubjectContextLocked and signed-byte KPROCESSOR_MODE arguments; add compile-time function-pointer ABI assertions for implemented Se/Ps security exports.
- [x] Bind referenced primary-token ownership to EPROCESS records and impersonation-token ownership to thread records. Add PsAssignImpersonationToken and PsReferencePrimaryToken/PsDereferencePrimaryToken resolution; release references on thread exit and process reaping.
- [x] Add SeCaptureSubjectContext/SeReleaseSubjectContext, global serialized SeLockSubjectContext/SeUnlockSubjectContext, and internal token-security metadata duplication for newly allocated token objects.
- [x] Add restricted-token double evaluation, unqualified object ACE support, conditionless callback ACE support, fail-closed handling for GUID-qualified/conditional ACEs, privilege-assisted ACCESS_SYSTEM_SECURITY and WRITE_OWNER, and bounded SACL audit events.
- [x] Add user page-table copy helpers and an internal security-descriptor capture routine that validates mapped user pages and copies only referenced descriptor components.
- [x] Add host-side tests for the Se runtime, including restricted-token denial and SACL audit event emission.
- [x] Generate the kernel PE .edata table from defined function symbols filtered by the pinned Windows 11 manifest; add a CI validator that rejects names outside the manifest and wrong ordinals.
- [ ] Verify the current end-to-end CI run passes, including host security tests, PE export validation, and QEMU/SMP boot tests.
- [ ] Finish the real Ps token object allocator and wire token creation, duplication, primary-token installation, and teardown automatically; the current hooks are groundwork rather than full token-object parity.

**Compatibility boundary:** the PE export table only includes implemented, defined function symbols that exist in the pinned Windows 11 manifest. Unsupported Windows exports remain absent from the PE export table; internal RdotOS helpers are not exported. The current implementation is not yet full Windows 11 SRM parity.

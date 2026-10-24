# RdotOS

**RdotOS is an experimental, Rust-first x86-64 operating system built around a UEFI boot path and NT-inspired kernel interfaces.** Its long-term goal is to provide a native platform capable of hosting Windows-style x64 drivers. That compatibility is a work in progress; RdotOS is not currently a Windows 11 replacement and does not claim general Windows driver or WDK binary compatibility.

[![Build and QEMU boot test](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml/badge.svg)](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml)

- **Language:** Rust (no_std kernel and Rust UEFI loader)
- **Architecture:** x86-64
- **Boot mode:** UEFI
- **License:** GPL-2.0-or-later — see [LICENSE](LICENSE)
- **Project status:** Experimental; implementation and hardware support are incomplete

See the [implementation roadmap](roadmap.md) for feature status, known gaps, and planned work.

## Project goals

- Build a maintainable operating-system kernel in Rust.
- Establish explicit boundaries between architecture code, the HAL, memory management, the NT-inspired executive, and device I/O.
- Develop memory isolation, executable-image loading, scheduling, and driver-facing interfaces incrementally.
- Validate changes through host-side tests and automated OVMF/QEMU boot tests.
- Pursue Windows x64 driver compatibility through real ABI and semantic implementations, not export-name lists or placeholder functions.

## Current implementation

The repository contains working foundations in several areas. “Implemented” here means code and tests exist; it does not imply production readiness or complete Windows compatibility.

| Area | Current state |
| --- | --- |
| UEFI boot | Rust EFI application verifies a build-pinned SHA-256 digest for the generated PE32+ AMD64 native kernel, validates its fixed-base section layout, maps its sections at the high-half image base, exits boot services, and transfers control through versioned boot information. The kernel is currently linked as ELF64 for Rust/toolchain convenience, then converted and validated as PE32+ for the bootloader handoff. |
| Kernel bootstrap | Freestanding Rust kernel, kernel-owned GDT/IDT, exception diagnostics, COM1 logging, CR3 activation, and physical-page allocation. |
| Memory protection | High-half PE32+ kernel section mappings with checked virtual-to-physical backing, 4 KiB kernel-section permissions, CR0.WP, NX where supported, kernel stack guard page, isolated user address spaces, checked user-copy helpers, a demand-mapped RAM direct-map window extending above 4 GiB, and W^X checks in implemented mapping paths. |
| NT memory manager | Partial Windows x64 ABI implementations for MDL lifecycle operations, canonical/user-kernel boundary validation, and pinned-page accounting. `MmProbeAndLockPages` can fault in tracked pageable kernel-pool pages through the kernel page-fault/page-in path. `MmMapLockedPagesSpecifyCache` can map arbitrary non-contiguous PFNs into a contiguous kernel VA range or a tracked user alias in the active registered process; user aliases must be unmapped in that same address space before unlock, and backing frames remain MDL-owned. Cached mappings only. Missing pageable user pages still fail closed: per-process VMA/page-in and exception-safe probe recovery are not implemented. `MmCopyVirtualMemory` translates the registered source and target process roots under process references and the shared address-space lifetime lock, and accesses high physical RAM through validated direct-map pointers. These EPROCESS-compatible objects are RdotOS internal structures, not private binary layouts for a specific Windows build. User-mode MDL mappings, pageable user-address faulting, full WDK edge-case parity, and complete Windows process/object manager semantics remain incomplete. |
| ACPI and HAL | RSDP/RSDT/XSDT validation, MADT parsing, CPU-local records, APIC-ID registry, port-I/O primitives, MMIO bounds checks, IRQ-save locks, and a one-way processor-lifecycle observer contract so HAL no longer calls ntoskrnl by module path. |
| SMP | AP startup uses a temporary low-memory page-table root, bounded INIT/SIPI coordination, APIC-ID validation, per-CPU GDT/TSS, dedicated NMI/double-fault/machine-check IST stacks, local APIC/GS state, and 64 directed IPI acknowledgements per AP. Cooperative kernel-thread dispatch is connected to scheduler task state, per-CPU TSS RSP0, and cross-CPU wake IPIs; `ntoskrnl/ke/runtime.rs` now owns Ke/Ki startup: `KeInitializeExecutive`-style scheduler setup, `KiInitializeProcessor` validation, `KiStartAllProcessors` ACPI/AP startup, and `KiInitializeKernel` post-startup validation. `Ex` remains the executive resource/object layer rather than the processor-startup owner. | Still experimental: 2/4/8-vCPU CI results are pending; timer preemption, per-CPU run queues/load balancing, IST guard pages, MDL teardown races, and real-hardware validation remain incomplete. |
| Executable loading | Bounded PE32+ AMD64 loading, relocations, normal/delay imports, registered exports, API-set aliases, TLS setup/callbacks, and basic unwind metadata registration. |
| User mode | Ring-3 transition and a small RdotOS-native syscall/process lifecycle test path. This is not the Windows NT syscall ABI. |
| Kernel services | Initial scheduler policy, typed handles/access checks, dispatcher primitives, deadline-aware IRPs, bounded I/O completion ports, typed port/MMIO accessors, and callable `IoIsWdmVersionAvailable` / `IoGetConfigurationInformation` exports. |
| DMA and IOMMU | DMA request/channel foundations and Intel VT-d ACPI discovery exist. Real DMA mapping, full VT-d initialization, and AMD IOMMU support are incomplete. |
| ext filesystem | A conservative read-only ext2/ext3/ext4 metadata parser with bounds checks and selected block mapping. It rejects unsupported features and is not a production filesystem driver. |

## HAL / ntoskrnl architectural boundary

The dependency direction is explicit for processor lifecycle and clock interrupts: HAL owns CPU topology state and publishes add/remove completion and clock events through narrow registered callback contracts. Ke/Ki register their observers; the HAL/APIC implementation no longer calls `ntoskrnl` or imports ACPI data types. APIC implementation code now lives at `kernel/src/hal/apic.rs` as a private child module of HAL; the root `mod apic` was removed. Boot-time controller setup, interrupt entry handling, clock/timeout queries, timer state, cross-core clock status, and logical interrupt priority are routed through HAL service functions. The notification contract uses stable codes (add complete = 1, remove complete = 2), and registration is idempotent for the same observer while rejecting replacement.

This is an architectural step, **not yet a complete Windows 11-equivalent HAL split**. The kernel remains a single Rust crate, and the HAL still uses validated ACPI structures during topology discovery. Directed INIT/SIPI/fixed-vector IPI and I/O-APIC route operations now have HAL-owned service wrappers, and ACPI MADT iteration is performed by boot/platform code rather than HAL. The implementation is still an in-crate, x86-64-specific HAL rather than a fully platform-neutral subsystem; callback lifetime/ABI, route/interrupt-object integration, behavioral contract tests, and a fresh full build plus QEMU SMP workflow remain required. CI now has a source-level guard that rejects direct HAL-to-ntoskrnl/ACPI dependencies and direct imports of the private APIC module; that guard has not yet been observed passing in a completed workflow. The HAL owns the APIC module boundary, but this alone does not establish Windows 11 private HAL ABI compatibility.

The intended dependency direction is:

`ntoskrnl (Ke/Ki/Io/Ex/Mm)` → `HAL contract` → `x86-64 platform implementation (APIC, ACPI, MMIO, port I/O)`

HAL must not depend on NT executive internals. Windows HAL export names and matching Rust interfaces alone do not establish binary compatibility with Windows 11's private HAL ABI.

## NT executive / Ke compatibility status

The Ke comparison tracks **169 Ke export names** for the Windows 11 x64 target. The current implementation includes kernel APC safe-point delivery and rundown, user APC injection at the ring-3 syscall return path after an alertable wait, processor-change callback registration/deregistration, CR3-based nested process attach/detach with process references, scheduler-backed priority bands and dispatcher wake boosts, and serialized wait-all reservations for supported event/semaphore/timer objects. Public x64 layout assertions cover dispatcher prefixes, semaphore/group-affinity structures, processor notifications and device queues. This is still partial compatibility: user APC delivery depends on the RdotOS syscall/alertable-wait path; process attach still lacks exception-safe thread-exit cleanup, SEH/unwind integration and full Windows thread/process semantics; processor hot-remove orchestration is absent; mutant/queue wait-all reservation and Windows priority decay/quantum policy remain open; and private Windows KAPC/KAPC_STATE/KTHREAD/EPROCESS byte layouts are not reproduced. See the [Windows 11 vs RdotOS Ke comparison](WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md) for signatures, resolver paths, progress calculations, and remaining gaps.

## Build and test

The CI workflow builds the freestanding Rust kernel, converts its linked ELF64 image to a fixed-base PE32+ AMD64 native image, validates PE headers/sections and the pinned digest, builds the Rust UEFI loader, and boots it under OVMF/QEMU. The main smoke test uses 5 GiB RAM and four virtual CPUs. An additional 2/4/8-vCPU SMP matrix asserts expected AP counts, BSP TSS/IST wiring, per-AP directed IPI acknowledgements, cross-CPU scheduler wakeups, contended IRQ-save locks, thread/IRP/object lifecycle and teardown, and VM/TLB self-tests. Host-side tests exercise the physical allocator map parser and ext filesystem parser. The expanded matrix passed in [workflow run #960](https://github.com/umarfarooq7869/RdotOS/actions/runs/40091609754). The newer Ke executive changes passed host tests, freestanding build, PE32+ validation, UEFI build, image packaging, OVMF/QEMU smoke, and the 2/4/8-vCPU SMP matrix in [run #997](https://github.com/umarfarooq7869/RdotOS/actions/runs/40092050955). The latest completed workflow log inspected for this update is [run #1162](https://github.com/umarfarooq7869/RdotOS/actions/runs/40097691737) (commit `177e9a3`, before the timer-generation regression-test merge). It completed successfully: host allocator/filesystem tests, freestanding build, PE32+ validation, UEFI build, OVMF/QEMU smoke, and the 2/4/8-vCPU matrix passed; its serial logs report Ke broadcast rendezvous and DPC Synchronize/Done barriers passing on each online CPU, including the 8-vCPU boot. The timer-generation regression tests were previously present only inside the Ke runtime and were not directly executed by a dedicated CI step. They are now extracted into a standalone pure Rust unit-test file and wired into the workflow so the predicate tests actually run on the host. The new workflow triggered by these changes must complete before this revision is considered validated. The synchronous `KeFlushQueuedDpcs` drains until no work remains and tracks per-object DPC execution so queued/running DPC objects cannot be reinitialized; a failed CPU that remains marked online can still stall flush. Legacy KTIMER and NT-style KTIMER2 now share one 128-slot timer table, one generation/lifecycle model, and one APIC-driven deadline scan; the separate TIMERS2 table and timer2_tick path have been removed. The unified expiry loop dispatches classic timer signal/wait/DPC behavior or the KTIMER2 callback based on the record type, with Timer2 callbacks marked at logical DISPATCH_LEVEL. Cancellation and reinitialization advance generations to invalidate stale snapshots. Legacy timer rearm/cancel also attempts to remove its associated queued DPC even if the one-shot already transitioned inactive; a DPC already executing still requires separate rundown synchronization. The Timer2 tolerable-delay value is retained as metadata but is not yet used for coalescing. Timer2 callback dispatch now claims a per-record rundown flag under the timer lock, and reinitialization is refused while a callback is in flight; cancellation invalidates future snapshots but does not wait for an already-claimed callback to finish. Timer-generation predicates and wraparound behavior have dedicated host tests wired into CI; cancellation/rearm and flush-vs-insert integration stress remains required.

For the canonical build and boot-test result, see [GitHub Actions](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml). A successful CI run verifies only the paths covered by that run; it is not evidence of support for all hardware or Windows drivers.

### Development prerequisites

The CI workflow is the authoritative build recipe. Development generally requires a recent Rust toolchain with the x86_64-unknown-uefi and x86_64-unknown-none targets, the repository's linker script, and (for the runtime smoke test) OVMF and QEMU.

## Recent milestone: PE32+ kernel handoff

- The bootloader now consumes `EFI/BOOT/rdotos-kernel.exe`, a fixed-base AMD64 PE32+ image using the native subsystem. The UEFI bootloader itself remains a normal EFI application.
- The build converts the Rust-linked ELF64 kernel into PE32+ and checks the DOS/PE signatures, AMD64 machine type, PE32+ optional-header magic, fixed image base, native subsystem, section RVAs, and absence of imports/base relocations.
- The bootloader validates the supported static PE layout, allocates physical backing for each section, checks the build-pinned SHA-256 digest, builds the kernel mappings, and enters the high-half kernel through a low identity-mapped CR3 transition trampoline.
- The kernel checks the active CR3, inspects required bootstrap mappings, verifies high-half section backing and stack translation, and checks the virtual stack guard before applying section protections.
- The memory manager has a demand-mapped direct-map window for UEFI-described RAM above 4 GiB; physical-to-virtual conversion rejects addresses outside the eligible RAM ranges.

This is a project-specific, fixed-base PE32+ kernel format—not a general Windows kernel loader. The image deliberately has no imports or base relocations, and it is not yet loaded by the Windows boot manager. Windows x64 driver compatibility remains a long-term goal, not an achieved compatibility claim.

## Recent milestone: tracked user-mode MDL mappings

- `MmMapLockedPagesSpecifyCache` now has a user-mode mapping path for a registered process whose address-space root is active. It reserves a range in a dedicated user VA arena and maps the MDL's locked PFNs without taking ownership of those physical frames.
- Mapping records bind the MDL, process object, address-space root, base VA, and page count. Unmapping validates the same process/root and PFN sequence before removing aliases. External user PTEs are distinguished so address-space teardown does not free the MDL-owned frames.
- Probe/map/unmap/unlock entry points share an IRQ-safe lifecycle lock; MDL free checks avoid releasing an owned MDL while it remains locked or mapped.
- CI run [40083235192](https://github.com/umarfarooq7869/RdotOS/actions/runs/40083235192) passed after the MDL teardown comparison fix.

This milestone is **not** full Windows MM/process compatibility. User mappings require an active registered process and resident locked PFNs. Demand-zero/pagefile-backed user page-in, Windows-style exception delivery/recovery, scheduler-driven current-process/thread semantics, complete object/handle lifetime behavior, and race/WDK conformance tests remain open.

## Security and compatibility notes

- The embedded PE32+ kernel SHA-256 digest detects a changed kernel only when the bootloader is trusted. It is **not** public-key signature verification or a complete verified-boot chain.
- Bootstrap identity mappings use 2 MiB leaves for UEFI-described ranges; descriptor edges are not yet mapped with fully granular 4 KiB permissions, and this is not a complete per-region isolation policy.
- SMAP remains disabled until all guarded user-memory access paths are audited and consistently use STAC/CLAC.
- The EFI application is not necessarily signed for Secure Boot enforcement.
- SMP and executive startup are experimental. Ke timer/DPC, queued-lock, mutex/mutant, KQUEUE, APC and processor-callback implementations use bounded side tables and remain partial. Cross-processor `KeGenericCallDpc` plus Synchronize/Done barriers are now exercised successfully on every online CPU in the 2/4/8-vCPU QEMU SMP matrix (workflow run #1162). `KeFlushQueuedDpcs` uses that rendezvous to drain local queues in repeated rounds and tracks in-flight callbacks, but this is not yet a strict Windows-equivalent global flush guarantee: it has a bounded fail-safe that may return with work pending, declines synchronous flushing above APC_LEVEL, and still needs timer-cancel/rundown race and offline-target tests. The I/O manager runtime now lives under `kernel/src/ntoskrnl/io/runtime.rs` (the top-level `crate::io` name is retained as a source-compatibility facade), alongside the Windows export catalogue. `IoIsWdmVersionAvailable` advertises only the implemented WDM 1.0 baseline, `IoGetConfigurationInformation` returns the public seven-counter structure, and `IoAcquireCancelSpinLock`/`IoReleaseCancelSpinLock` use a shared lock with IRQL raise/restore; counters remain zero until device enumeration is connected. The manager has bounded FIFO completion ports, optional monotonic IRP deadlines, and timeout expiry before dispatch/late completion; callers still need dispatcher-integrated waits and driver-specific cancellation for in-flight I/O. Other I/O exports remain unresolved until ABI-correct semantics are implemented. HAL adds 16-bit port I/O plus ordered volatile MMIO accessors, but mapping/cache policy and actual storage/device drivers remain platform responsibilities. Kernel APCs run at eligible safe points; user APCs are injected only at the RdotOS ring-3 syscall return after an alertable wait. Processor callbacks have registration/deregistration and HAL lifecycle hooks, while complete hot-remove orchestration remains absent. Process attach switches CR3 with referenced roots and LIFO restoration, but exception-safe cleanup and full Windows process/thread integration remain incomplete. The `ntoskrnl/ke` module now owns Ki/Ke initialization and processor startup (`ke_initialize_executive`, `ki_initialize_processor`, `ki_start_all_processors`, and `ki_initialize_kernel`); each CPU gets a private GDT/TSS and dedicated IST stacks, and cooperative kernel-thread dispatch updates TSS RSP0 and uses HAL IPIs to wake eligible CPUs. Until migration-safe context publication and per-CPU run queues exist, each saved thread context is pinned to one allowed online CPU. Timer-driven preemption, per-CPU run queues/load balancing, IST guard pages/fault injection, concurrent user-MDL/process-exit teardown tests, and real-hardware SMP validation remain incomplete. Full process/thread semantics, hardware storage drivers, graphics, and power management also remain incomplete.
- Windows-style names and export metadata do not by themselves provide Windows ABI or semantic compatibility. The current syscall ABI is RdotOS-native.
- The MDL exports use explicit Windows x64 calling-convention signatures and remain a bounded subset. `MmProbeAndLockPages` can fault in tracked pageable kernel-pool pages and pins resident user pages only when their mappings are valid; it does not yet provide demand-zero/pagefile-backed user page-in or Windows SEH-style exception delivery for invalid buffers. `MmMapLockedPagesSpecifyCache` supports non-contiguous PFNs in a kernel VA extent and a tracked user VA alias in the active registered process; user aliases must be unmapped from the same process/address space before unlock, and cached mappings are the only supported cache policy. Probe/map/unmap/unlock bookkeeping is serialized by an IRQ-safe lifecycle lock, but teardown races and WDK edge cases still need stress validation. The internal EPROCESS-compatible structure is not binary-compatible with a private Windows build layout. Cross-process copying is supported only for RdotOS-registered process objects and is serialized against address-space mutation/destruction.
- DMA/IOMMU code must fail safely when a required hardware translation or mapping feature is unavailable; bookkeeping tokens are not equivalent to hardware-backed mappings.
- The ext parser is read-only. It does not replay journals or write filesystem metadata.

## Repository layout

| Path | Purpose |
| --- | --- |
| bootloader/ | Rust UEFI application |
| kernel/ | Freestanding Rust kernel, architecture code, HAL, memory manager, executive and I/O foundations |
| third_party/Ext3Fsd | Pinned upstream filesystem-driver source reference for a planned Rust port |
| .github/workflows/ | Automated build and OVMF/QEMU validation |
| [roadmap.md](roadmap.md) | Implemented features, incomplete work, and next milestones |

## Third-party filesystem reference

The third_party/Ext3Fsd submodule is retained as a separate upstream reference for a possible staged Rust port. The original Windows driver is **not loadable by RdotOS**. See [third_party/README.md](third_party/README.md) and the upstream [Ext3Fsd repository](https://github.com/matt-wu/Ext3Fsd) for project and licensing context.

## Contributing

Contributions should be small enough to review, document their assumptions, include meaningful tests where practical, and avoid claiming compatibility beyond what has been validated. For kernel-facing changes, include relevant build or QEMU results and keep failure paths bounded and safe.

## Windows 11 Ke compatibility tracking

The current Ke inventory is maintained in [WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md](WINDOWS11_RDOTOS_NTOSKRNL_KE_COMPARISON.md), which tracks 169 Ke export names, public WDK contracts, return values, resolver paths, unresolved exports, and the scoring method. The comparison's current project-defined combined score is 65.1% (100% declaration/resolver coverage plus 51/169 scoped strict behavior matches); it is not a Microsoft score or proof of Windows driver compatibility. APC delivery, processor callbacks, process attach, priority bands and wait-all reservations have new implementation foundations but remain partial and need current CI/QEMU, SMP and driver-facing validation. Private Windows object layouts are not inferred from opaque names; only the public layouts currently asserted by the code are claimed.

## Ke runtime source organization

Ke implementations are organized under `kernel/src/ntoskrnl/ke/runtime/` by responsibility: processor/IRQL and spin primitives, dispatcher objects, time, thread waits/APC state, interrupt/scheduler helpers, device queues, executive queues, queued spin locks, mutexes, affinity, and timer/DPC/rendezvous. `runtime.rs` is the compatibility facade included by the import resolver. These files are currently included into the same Rust module scope to preserve shared private state and avoid changing ABI or behavior during this mechanical refactor; follow-up work can extract true Rust submodules after shared interfaces are made explicit.

## License

RdotOS is licensed under **GNU General Public License v2.0 or (at your option) any later version** (SPDX: GPL-2.0-or-later). See [LICENSE](LICENSE). The AP startup trampoline and temporary-page-table approach in kernel/src/smp.rs are adapted from [RustOS-Dev/RustOS](https://github.com/RustOS-Dev/RustOS), which is also GPL-2.0-or-later; the source-file notice identifies the adaptation. Third-party components retain their applicable licenses and notices.

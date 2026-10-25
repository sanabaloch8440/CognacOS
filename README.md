<p align="center">
  <img src="assets/cognacos-logo.svg" alt="CognacOS" width="420">
</p>

# CognacOS

**CognacOS is an experimental Rust-first x86-64 operating system with a UEFI boot path and NT-inspired kernel interfaces.** Its long-term goal is Windows x64 driver compatibility; that goal is not yet achieved. CognacOS is not a Windows 11 replacement and does not claim general WDK or third-party driver compatibility.

[![Build and QEMU boot test](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml/badge.svg)](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml)

- **Language:** Rust (`no_std` kernel and UEFI loader)
- **Architecture:** x86-64
- **Boot:** UEFI; native AMD64 kernel image packaged and validated as PE32+
- **License:** GPL-2.0-or-later

## Project documentation

- **[CognacOS documentation website](https://umarfarooq7869.github.io/CognacOS/)** — project overview, roadmap, and compatibility index.
- [Development roadmap](docs/roadmap.md) — current status, open milestones, and next priorities.
- [Windows 11 x64 function-level compatibility comparison](WINDOWS11_COGNACOS_NTOSKRNL_KE_COMPARISON.md) — Ke routine inventory, implementation scope, and scoring method.
- [Security Reference Monitor comparison](WINDOWS11_COGNACOS_NTOSKRNL_SE_COMPARISON.md) — Se coverage and known semantic gaps.
- [Boot security guide](docs/boot-security.md) — kernel signature verification, optional UEFI Secure Boot, TPM 2.0 measurement, and release-key setup.

## Native virtual-memory syscall status

The Rust NTDLL exports for `NtAllocateVirtualMemory`, `NtFreeVirtualMemory`, `NtProtectVirtualMemory`, `NtQueryVirtualMemory`, `NtReadVirtualMemory`, and `NtWriteVirtualMemory` now call the kernel through CognacOS's private native-service interrupt gate (`0x83`), which dispatches the matching `ntoskrnl/mm/native_vm.rs` handlers. `NtQueryVirtualMemory` uses the Windows x64 48-byte `MEMORY_BASIC_INFORMATION` layout and a `SIZE_T` return-length output. `NtReadVirtualMemoryEx` delegates to the read path only for zero flags. `NtAllocateVirtualMemoryEx`, `NtFlushVirtualMemory`, `NtLockVirtualMemory`, and `NtUnlockVirtualMemory` remain explicitly unsupported because extended allocation parameters, file-backed flush, and locked-page accounting are not implemented. The kernel path currently supports only the current-process pseudo-handle, reserve+commit allocation, and whole tracked-region decommit/release/protect; partial-region splitting, full Windows protection modifiers, cross-process access checks, and complete transactional rollback remain open. This is an incremental CognacOS-native bridge, not complete Windows 11 VM compatibility.

## Current foundations

| Area | Implemented foundation | Important gaps |
| --- | --- | --- |
| Boot and architecture | Rust UEFI loader, SHA-256 plus Ed25519 kernel verification, security-state handoff, GDT/IDT, CR3 activation | Stable release-key management, Secure Boot enrolled-key testing, broader hardware validation |
| Memory management | Kernel mappings and permissions, bounded vector/RIP trap-frame fixups with explicit RAX recovery results, invalid-probe regression self-test, CPL3 exception routing to process-scoped termination, checked user copies, demand-zero region metadata, resident working-set accounting, MDL/pinning foundations, adversarial same-VA cross-process mapping test in boot path | Passing current QEMU isolation validation, concurrent teardown/fault stress, user-mode SEH resume/unwind, working-set trimming, pagefile-backed user paging, SMP-safe TLB invalidation plus page reclamation |
| SMP and scheduling | Per-CPU ready queues, affinity-aware stealing, saved-context publication, timer preemption, wake IPIs, threaded DPC workers | Full Windows quantum/priority decay and scheduler semantics |
| NT executive | Ke/Ki runtime, timer/DPC rundown checks, PASSIVE-level threaded DPCs, APC lifecycle, queue/mutant wait foundations | Full user-mode SEH/APC dispatcher, blocking wait-all for every object type, WDK conformance |
| I/O and drivers | Initial WDM object/IRP layouts, completion claim state, bounded per-IRP device rundown references, unload deferral for outstanding IRPs, cancel-routine pinning, boot-time exactly-once completion/unload regression, deterministic host race/fault-injection model | Passing current QEMU SMP run; kernel-level cancel/completion callbacks racing teardown under injected faults; complete WDK semantics, PnP and storage integration |
| Registry and NLS | Bounded native hive parser, empty hive image creator, service lookup, raw NLS staging | Log replay, persistent registry writes, NLS interpretation, NTFS discovery |
| Native NTDLL loader | Typed PEB module lookup/enumeration and module information query; mapped-PE resource lookup/access; DLL-notification cookie registration; bounded DLL search paths; delay-import thunk resolution against loaded dependencies; AMD64 DIR64 relocation blocks; native `NtOpenFile`/`NtReadFile`; kernel-side bounded VFS-to-PE load/unload transaction API in `kernel/src/pe_loader.rs` | Full disk VFS, native syscall bridge with exclusive active-address-space ownership, recursive dependency loading for absent modules, real DLL initialization/notification delivery, final NTDLL unload integration, remaining loader export ABI work and current CI/QEMU validation |
| Platform and storage | Separate ACPI/PCI/HAL-contract/processor crates; storage protocol foundations; native Rust ext-family driver | Hardware drivers, DMA/IOMMU backend, full VFS and writable filesystem |
| Boot security | Fail-closed kernel signature check; optional UEFI Secure Boot state detection and TPM 2.0 TCG2 PCR 4 measurement | Secure Boot remains optional; firmware-signed loader/release keys and TPM hardware testing needed; custom kernel signature is not Authenticode |
| NT security | Bounded descriptor validation, basic access/privilege evaluation, token metadata hooks, audit-event groundwork | Full token lifecycle, impersonation, audit policy, callback ACEs, WDK parity |

Boot integrity is fail-closed: the loader rejects a kernel unless both its pinned SHA-256 digest and Ed25519 signature verify. UEFI Secure Boot remains optional and requires a firmware-trusted bootloader signing certificate; TPM 2.0 measurement is best-effort and absence of a TPM never blocks boot. The current successful QEMU matrix validates the no-TPM path; TPM-present runtime measurement still needs a dedicated passing test.

The module graph now rejects dependency edges that would create a cycle using a bounded, allocation-free DFS over the fixed module table. Import binding also resolves against the actual selected loaded-module identity after API-set host normalization, avoiding accidental cross-module symbol matches. Final-reference unload is rejected while a module is in its `Initializing` state. These are foundational safeguards; they do not yet implement recursive VFS dependency discovery, forwarded-export loading, or user-mode detach rundown.

The kernel PE loader now also exposes a checked DLL initialization handoff: `prepare_module_initialization` validates the mapped AMD64 PE headers through the process address space, transitions the module from Mapped to Initializing, and returns its entry point for a user-mode `DLL_PROCESS_ATTACH` callout; `complete_module_initialization` commits success or removes the image/export registration and releases dependency references on failure. The kernel never calls user DLL entry points directly. This is a lifecycle primitive, not a completed NTDLL `LdrLoadDll` bridge: recursive loading of absent imports, native section syscalls, loader-lock integration, and actual user-mode callout scheduling remain open.

The native loader work now includes a read-only kernel file-object provider for explicitly staged boot assets, native `NtOpenFile`/`NtReadFile` dispatch for that provider, typed NTDLL queries over the current PEB loader list (module lookup by name/mapping/address, full-name descriptors and bounded enumeration), conservative reference/pin bookkeeping, and AMD64 `DIR64` relocation-block processing with regression tests. Recent NTDLL loader source is now divided into focused Rust submodules under `ntdll/src/windows11_exports/loader/` for search paths, module operations, resources, notifications/queries, and delay loading; `loader.rs` is the shared ABI/state façade. The implementation adds bounded PE resource directory/data lookup, resource access range checks, process module-information snapshots, DLL-notification registration cookie validation, bounded default/user search-path construction with a release helper, delay-import thunk binding against already-loaded dependencies, and `LdrQueryOptionalDelayLoadedAPI` probing without triggering a recursive load. Relocation/resource fixtures and optional-query input validation cover malformed inputs. Notification callbacks are registered but not yet delivered by a real load/unload transaction; PE resource helpers assume the supplied module points to a valid mapped image because this crate does not yet have a universal fault-safe pointer-probe bridge. The kernel now has `kernel/src/pe_loader.rs`, connecting registered VFS reads to the existing PE32+ mapper and per-process module registry, with bounded caller-provided scratch storage, duplicate-load reference accounting, rollback after map/registry failure, and dependency rundown during unload. This is a kernel-callable prerequisite, not yet exposed through NTDLL native syscalls because exclusive active-address-space ownership has not been wired into the dispatcher. These are integration scaffolds, not a general disk VFS or complete DLL loader: only registered immutable assets are visible, the file read path is bounded, and new PE images cannot yet be mapped from NTDLL because native section/map/unmap and process module-lifetime bridges are still incomplete. `LdrLoadDll` returns an existing module when found and otherwise fails closed instead of reporting a nonexistent load. `LdrUnloadDll` does not unmap an image without kernel-backed rundown. The current revision must pass CI before these changes are considered validated.

Fault containment has bounded supervisor exception fixups with an explicit recovery PC/RAX result and a boot self-test that probes an invalid address; unresolved CPL3 exceptions are routed by saved CS/trap frame and terminate only the owning process. The boot path now also checks two address spaces mapping the same user VA to distinct frames and verifies kernel mappings remain supervisor-only, while the user-mode fixture exercises IRETQ entry, syscall dispatch, and process exit. WDM now claims completion once, pins device objects while IRPs remain outstanding, and defers unload until rundown drains; a boot regression and deterministic host cancellation/completion/unload fault-injection matrix were added. These checks are not considered validated until the current CI/QEMU SMP workflow passes. Concurrent address-space teardown, user-mode SEH resume/unwind, SMP-safe working-set trimming/reclamation, and broader kernel-level callback race testing remain open. These are implementation foundations, not production-readiness claims. Details and acceptance criteria live in the roadmap. The CI workflow tests host-side components, builds the freestanding kernel and UEFI loader, validates the PE32+ image, and runs OVMF/QEMU boot and SMP checks. A passing emulator run does not prove real-hardware reliability or Windows driver compatibility.

## Repository layout

- `kernel/src/ntoskrnl/ke/` — processor startup, scheduling, traps and context switching
- `kernel/src/ntoskrnl/mm/address_space.rs` — process address spaces and checked user-memory access
- `kernel/src/ntoskrnl/io/file_services.rs` — initial read-only provider for explicitly staged boot files
- `kernel/src/pe.rs`, `kernel/src/pe_modules.rs` — bounded PE32+ mapping and module-lifetime scaffolding
- `ntdll/src/windows11_exports/` — Rust NTDLL exports, including typed loaded-module lookup APIs
- `kernel/src/ntoskrnl/` and `kernel/src/hal/` — NT-inspired executive and HAL implementations
- `drivers/ext4/` — native Rust ext-family filesystem crate; general VFS mounting remains incomplete
- `.github/workflows/` — build, host-test, and boot validation workflows
- `docs/` — public project documentation source

## Development principles

1. Match Windows 11 x64 names and documented ABI only where evidence supports it; never treat an export name as proof of implementation.
2. Keep unsupported or unsafe paths fail-closed rather than claiming compatibility.
3. Mark a feature complete only when implementation and meaningful validation exist.
4. Keep progress tracking in the roadmap and detailed routine inventories in the subsystem comparison documents, not duplicated in this README.

## License

CognacOS is licensed under **GNU GPL v2.0 or (at your option) any later version**. See [LICENSE](LICENSE). Third-party components retain their applicable licenses and notices.


**PE loader progress:** `kernel/src/pe_loader.rs` now provides a concrete VFS transaction backend for the bounded engine in `kernel/src/pe_dependencies/transaction.rs`. It resolves staged System32 DLL paths, maps dependencies before parents, publishes generation-tagged module edges, and rolls back newly mapped images in reverse order. The new `load_dll_transaction_from_vfs` API requires caller-owned per-depth buffers and the owning process's address-space mutation/rundown contract. The older `load_dll_from_vfs` path and NTDLL native syscall bridge are not switched over yet; the VFS remains limited to explicitly registered immutable files, and this integration is not validated until CI/QEMU and synthetic dependency-graph tests pass.


**NTDLL memory exports:** The core `Nt/Zw*VirtualMemory` calls use CognacOS's private `int 0x83` native-service gate and remain current-process-only. `MEMORY_BASIC_INFORMATION` has the 48-byte x64 layout. RTL memory zones/lookaside allocators, in-memory streams, and a new section-backed stream path are implemented with bounded kernel section objects. `RtlCm*MemIoResource`/`RtlIo*MemIoResource` now encode/decode the documented partial-resource descriptor fields. Secure-memory cache callbacks can be registered and are notified after successful VM free/protect operations. Token membership uses the effective registered thread/process token; non-null token handles are rejected until the object manager can safely resolve user handles. Kernel persistent-memory tokens require UEFI `EFI_MEMORY_NV` backing and use cache-line flush plus `SFENCE` when requested; this path fails closed on ordinary RAM and is not hardware-validated in QEMU. Remaining limits include file-backed sections/flush, section view mapping into user address spaces, extended allocation parameters, nonresident-page lock-in, stream growth/revert/region locking, and a complete Windows token-handle/access-check ABI. These are partial CognacOS facilities, not full Windows 11 compatibility.

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

## Native ETW/session status

CognacOS now has a bounded kernel-owned ETW provider table and event ring behind private native-service calls. Privileged sessions are owner-scoped, named sessions use bounded kernel-owned UTF-16 names, and a provider can have independent level/keyword filters in multiple sessions. Event records carry a session mask, so each controller reads only events admitted by its session filters; unprivileged reads remain restricted to the caller's own providers. EtwEventWriteTransfer carries activity and related-activity IDs through the kernel event record. NTDLL callbacks run in user mode after it observes changed kernel state, never from the kernel. The internal named-session bridge is not yet the full Windows StartTraceW/ControlTraceW/EnableTraceEx2 API surface.

RtlExitUserProcess enters the kernel SYS_EXIT path after LdrShutdownProcess. The loader validates PE image bounds, derives a bounded detach order from resolved import-address dependencies, and serializes concurrent shutdown calls while allowing same-thread reentrancy. Windows-compatible SEH recovery from arbitrary DllMain faults, full loader-lock semantics, and comprehensive x64/QEMU validation remain open. See the ETW section of the roadmap for details.

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


## Native CSR / ALPC status

CognacOS now routes a supported ALPC subset through private native-service IDs on its vector-0x83 syscall gate: `NtAlpcCreatePort`, `NtAlpcConnectPort`/`NtAlpcConnectPortEx` (restricted argument subset), `NtAlpcAcceptConnectPort`, `NtAlpcSendWaitReceivePort`, and `NtAlpcDisconnectPort`. Port, endpoint, and ALPC-section handles use typed Object Manager objects, granted-access checks, and a default DACL. The default DACL currently grants the ALPC access bits to Everyone; caller-provided security descriptors are rejected rather than silently applied.

`NtCreateSection` now creates unnamed committed Section objects with typed Object Manager handles; `NtAlpcCreatePortSection` can import one and retains an independent MM reference. `NtAlpcCreateSectionView` / `NtAlpcDeleteSectionView` use the section manager's real map/unmap path. The MM section primitive can now map a backing section into a target address space; ALPC view creation permits peer-process mapping when the caller has a valid handle for the same port section. The ALPC core and advanced-resource metadata have CI regression coverage.

This is **not full Windows 11 ALPC compatibility**. CognacOS uses private service IDs, not Windows build-specific syscall ordinals. Connect/accept/send/receive currently use bounded nonblocking polling; message attributes and timeouts are not wired into the transport. `NtAlpcImpersonateClientOfPort` now resolves peer identity from the kernel connection object, duplicates the peer process's registered primary-token security identity into an impersonation token, and assigns it to the current KTHREAD; absent/unregistered tokens fail closed. ALPC resources are also cleaned from the process-exit path. Named/file-backed Section creation, full Windows Section access-mask semantics, and handle duplication/transfer to peer processes remain incomplete. Connection-time QoS capture, full Windows TOKEN/privilege semantics, container impersonation, resource reserves, query/set information, and the Windows shared-memory completion-list ABI also remain incomplete. The `AlpcGetHeaderSize`, `AlpcGetMessageAttribute`, and `AlpcInitializeMessageAttribute` helpers now have the x64 `ULONG` ABI; completion-list helper exports still lack a working shared-memory list implementation.

See [the CSR server design and remaining gaps](csrss/README.md) and [the CSRSRV export inventory](csrss/exports/csrsrv-x64.def). The export inventory is based on a public Windows 10 x64 baseline and must be compared with the exact target Windows 11 `csrsrv.dll` before claiming export-table parity.

## Recent native NTDLL and NT executive work

The Rust NTDLL now has an incremental x64 printf-family bridge for the Windows-inventory-listed `_vscprintf`, `_vscwprintf`, `sprintf`, `sprintf_s`, `swprintf`, and `swprintf_s` entry points. Its assembly shims forward Windows x64 variadic arguments to Rust formatting helpers. Wide-format strings preserve non-ASCII UTF-16 literal text through UTF-8 parsing and UTF-16 output conversion; malformed UTF-16 is rejected. Conversion coverage and CRT semantics remain partial, and this bridge is not yet runtime-validated.

Recent NT executive work also adds process-init diagnostics and implementation foundations across native VM services, ETW sessions, ALPC/section objects, PE dependency transactions, and fault containment. These are bounded CognacOS-native paths using private service IDs where applicable, not Windows syscall ordinals or proof of full Windows 11 compatibility.

**Validation caveat:** the latest GitHub Actions run for the UTF-16 formatting change failed during the QEMU boot smoke test. Do not treat these changes or the current kernel boot as validated until a subsequent end-to-end workflow passes. See the [roadmap](roadmap.md) for current gaps and acceptance tests.

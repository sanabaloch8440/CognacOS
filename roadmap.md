# CognacOS — actionable roadmap

**Goal:** a maintainable Rust-first x86-64 OS with NT-inspired interfaces and a long-term path toward Windows x64 driver compatibility. Windows 11 or general third-party driver compatibility is not yet achieved.

This is the canonical task list. Completed work is summarized by area; detailed routine inventories belong in the [Ke comparison](WINDOWS11_COGNACOS_NTOSKRNL_KE_COMPARISON.md) and [Se comparison](WINDOWS11_COGNACOS_NTOSKRNL_SE_COMPARISON.md). Do not duplicate historical implementation notes here.

## Execution order

1. **Green the current revision.** Reproduce the latest workflow failure, fix its root cause, and pass Rust builds, NTDLL PE/export checks, QEMU boot smoke, and 2/4/8-vCPU SMP tests. Do not treat older green runs as validation of newer code.
2. **Memory safety and isolation.** Prove independent address spaces and supervisor-only mappings; stress faults, checked copies, pin/unpin, unmap, and teardown concurrently; implement safe user working-set trimming and pagefile-backed user paging.
3. **I/O lifetime safety.** Validate exactly-once completion, cancellation races, callbacks during teardown, outstanding-IRP drain, and unload/remove barriers with kernel-level fault injection under QEMU SMP.
4. **Native user-mode loader.** Connect NTDLL to the existing bounded VFS-to-PE transaction using process-owned address-space/section services; validate recursive dependency order, forwarded exports, rollback, attach/detach, and final unload.
5. **Process and executive semantics.** Complete per-process handles, token lifecycle, user APC/SEH delivery, wait-all reservations, and scheduler-backed blocking waits.
6. **Storage and Windows ABI coverage.** Add safe PCI/DMA/IOMMU and disk/VFS integration, crash-safe writable filesystem support, remaining registry/security semantics, then automate ABI and WDK-style conformance tests.

## Current foundations (brief)

- **Boot/platform:** UEFI loader, PE32+ kernel handoff, SHA-256/Ed25519 kernel verification, ACPI/AP startup, GDT/IDT/TSS/IST, and CI/QEMU validation paths.
- **Memory:** address-space and checked-copy primitives, demand-zero region metadata, resident accounting, MDL/pinning foundations, bounded supervisor fault fixups, and process-scoped CPL3 termination.
- **Scheduling/processor state:** per-CPU queues, affinity stealing, timer preemption, DPC/APC foundations, and legacy x87/SSE XSAVE/XRSTOR paths.
- **NT executive/NTDLL:** shared no_std ABI crate; selected native VM, registry, ETW, ALPC/section, process, and loader paths use CognacOS-private service IDs.
- **Object/I/O/security:** initial typed Object Manager handles, access checks, WDM IRP completion claims and device rundown, bounded token/security metadata, and audit groundwork.
- **Registry/loader/storage:** read-only REGF mounting plus a bounded writable-hive backend and recovery primitives; bounded VFS-to-PE transaction backend; native Rust ext-family filesystem foundations.
- **Important limitation:** code presence and host tests do not prove QEMU/SMP correctness or Windows ABI/driver compatibility.

## P0 — CI and runtime validation

- [x] Investigated [run 40816579264](https://github.com/umarfarooq7869/CognacOS/actions/runs/40816579264): compilation and host-side checks passed; QEMU stopped in the NT MM self-test because NUMA page allocation treated transient allocator-lock contention as allocation failure. Changed the blocking node-aware allocator to acquire the allocator lock normally; the nonblocking API remains fail-closed until its complete allocation path is nonblocking.
- [ ] Pass the complete current-revision build, NTDLL PE/export/no-import checks, QEMU boot smoke, and 2/4/8-vCPU SMP matrix.
- [ ] Ensure every new boot regression is asserted in workflow logs and fails the job when its expected result is absent.
- [ ] USB host stack: `drivers/usbxhci` now contains controller-specific capability/TRB/ring, DMA-resource, startup/stop, and event-drain primitives; hub descriptor validation and port/enumeration policy have been split into the new `drivers/usbhub3` library. The hub library checks USB 2 hub (descriptor type 0x29) and SuperSpeed hub (0x2a) descriptors, device/configuration descriptor bounds, tracks per-port lifecycle and increments a generation on removal/failure so late completions can be rejected. CognacOS targets `usbhub3.sys` for the xHCI stack and does not duplicate the legacy USBPORT `usbhub.sys` driver. CI now checks/tests the hub policy library and builds a separate `usbhub3.sys` PE32+ image. The hub DriverEntry requires a versioned kernel-published extension with port status, reset, control-transfer, child-PDO and removal callbacks and fails closed if these are absent. Neither driver is end-to-end ready: the kernel still needs live PCI resource arbitration, device-scoped IOMMU mappings with invalidation/teardown, interrupt assignment and ISR/DPC synchronization, the UCX-equivalent request-routing contract, completion-driven port reset/slot/address/configure, EP0 control transfers, endpoint rings, child-PDO PnP, HID report delivery, and QEMU proof of keyboard input and power/removal safety. See [USB driver stack design](docs/usb-driver-stack.md). No Windows 11 x64 hardware compatibility is claimed.
- [ ] Publish the passing run/commit in this section only after all required jobs finish successfully.

## P1 — Memory safety and process isolation

- [ ] Add adversarial CPL3 tests for cross-process same-VA mappings, supervisor-page access, protection faults, process survival, and syscall/trap transitions.
- [ ] Stress concurrent address-space teardown against page faults, user copies, pin/unpin, and unmap; prove reference/rundown safety and no use-after-free.
- [ ] Test cross-page copies, read-only destinations, partial-copy failures, map/unmap races, invalid pointers, and allocation rollback.
- [ ] Implement user-page eviction/pagefile-backed page-in and demand-zero fault tests.
- [ ] Implement working-set trimming with pin/lock exclusion, cross-CPU TLB shootdown acknowledgements, and reclamation only after invalidation; stress under allocation pressure and SMP.
- [ ] Extend native VM decommit/protect to partial ranges with correct region splitting and transactional rollback; test all supported information classes and reject unsupported cross-process handles.

## P1 — I/O manager and device rundown

- [ ] Run kernel-level cancellation/completion/unload fault injection under QEMU SMP: exactly-once completion, cancel-vs-complete interleavings, callback-vs-teardown, outstanding-IRP drain, remove locks, and device deletion.
- [ ] Complete file/device object lifetime and documented IRP completion/cancellation semantics before expanding driver coverage.
- [ ] Add PCI enumeration/resource assignment and a real AHCI/NVMe path only after DMA safety is available.
- [x] Harden DMA/IOMMU failure ownership: copy the one-shot backend callback table into kernel-owned immutable storage, require the backend before allocating common-buffer pages, and attempt rollback when a successful map returns malformed outputs.
- [x] Harden IOMMU backend registration and DMA ownership: immutable kernel-owned callback-table copy, backend-before-allocation checks, malformed-map rollback, and fail-closed DMA behavior when no complete translation backend exists.
- [ ] Implement hardware VT-d and AMD-IOMMU translation, page-table/domain programming, IOTLB invalidation, cache-coherency synchronization, and device-removal teardown. ACPI Intel DMAR discovery/parser exists, but parsing firmware tables is not hardware DMA remapping.
- [x] Add bounded attached-device-stack capability checks for D-state transitions and wait/wake; copy the S4 platform callback descriptor into kernel-owned immutable storage.
- [ ] Complete parent-bus/USB-hub wake propagation, real hardware D-state sequencing, and production S4 storage/memory/CPU resume integration. The hibernation image codec is present, but no platform backend is registered by the boot path, so real S4 suspend/resume remains unavailable.

## P1 — NTDLL and native PE loader

- [ ] Wire the VFS-backed dependency transaction into the native NTDLL syscall path with exclusive, process-owned address-space mutation/rundown.
- [ ] Connect section creation, map/unmap, module registry, PEB loader-list updates, and NtClose lifetime semantics.
- [ ] Add synthetic multi-DLL graph tests: dependency-before-parent mapping, duplicate references, missing file/import, cycle detection, forwarded exports/ordinals, map/link failure rollback, and initializer failure.
- [ ] Queue DLL attach/detach callouts on a process thread; prevent unload until callbacks and dependent references have drained.
- [ ] Complete NtOpenFile/NtReadFile documented flags, asynchronous I/O, event/APC completion, cancellation, IO_STATUS_BLOCK ordering, buffer validation, and general file handles.
- [ ] Replace the immutable boot-asset provider with a mounted disk-backed VFS including directory traversal, normalized paths, sharing/access checks, synchronized file positions, and file-object lifetime.
- [ ] Add end-to-end tests proving the actual NTDLL PE is loaded and invoked from CPL3, not merely that shared dispatcher helpers work.

## P1 — Object Manager, security, and user-mode execution

- [ ] Replace global handle slots with process-bound handle tables and automatic process/thread identity binding; test access checks and rundown during process exit.
- [ ] Complete token creation/duplication, primary and impersonation token lifecycle, privilege state, and reference-safe teardown.
- [ ] Add object callbacks, handle inheritance, namespace parsing, SACL/audit policy, and documented object-type semantics.
- [ ] Implement Windows-compatible user APC dispatch and SEH/unwind resume; unresolved CPL3 faults currently terminate the owning process.
- [ ] Implement wait-all queue-entry reservation and scheduler-backed blocking waits; add WDK-style race/conformance tests.
- [ ] Associate every user thread with a scheduler ThreadRecord; enable AVX/YMM only after all thread, trap, context-copy, and restore paths support the expanded XSTATE area.

## P2 — Registry, NLS, and writable filesystem

- [ ] Add legacy REGF DIRT replay, correct LOG1/LOG2 selection/sequence policy, and validate against Windows-generated hive fixtures.
- [ ] Add REGF-backed enumeration, remaining query classes, change notifications, transaction semantics, complete KEY_* access masks, DACL inheritance, and token-aware policy.
- [ ] Test access denial, relative opens, invalid user buffers, mutation rollback, and persistent hive reopen/recovery; do not call a mount persistent without a passing durability/recovery test.
- [ ] Implement NLS table parsing, code-page/locale services, and Windows/NTFS partition discovery.
- [ ] Implement crash-safe ext filesystem metadata/journal operations before enabling general writable mounts.

## P2 — ALPC, CSR, and session services

- [ ] Add named/file-backed Section objects, full Section access masks, and normal handle duplication/transfer to peer processes.
- [ ] Wire ALPC message attributes, connection-time QoS, impersonation/revert policy, and scheduler-backed blocking waits.
- [ ] Implement ALPC resource reserves and the Windows shared-memory completion-list ABI, worker registration, atomics, and rundown.
- [ ] Build native PE32+ csrss.exe/csrsrv.dll, implement server-DLL initialization, and add an asynchronous process/session-manager launch path.
- [ ] Verify CSRSRV exports against an exact pinned Windows 11 x64 binary; the current inventory is not yet confirmed as an exact Windows 11 match.

## P2 — Keyboard input drivers

- [x] Add independent no_std driver cores for kbdclass.sys, kbdhid.sys, hidclass.sys, and i8042prt.sys; add bounded host tests and native PE32+ build validation.
- [ ] Replace minimal DriverEntry stubs with real device-object/PnP registration and IRP dispatch; connect keyboard class reads to the OS input consumer.
- [ ] Implement HID transport/minidriver integration for USB and Bluetooth, HIDClass request forwarding, report descriptor/report parsing, device removal, and cancellation/rundown.
- [ ] Wire PS/2 controller access through HAL-owned port-I/O/interrupt services; add IRQ/DPC packet delivery, controller initialization/reset, ACK/RESEND and timeout tests under QEMU.
- [ ] Add end-to-end QEMU tests that inject PS/2 input and verify delivered key make/break packets; add HID virtual-device tests and test keyboard disconnect/unload races.
- [ ] Add service/INF-style device matching and registry-based loading before claiming Windows Plug and Play compatibility.
- [ ] Keep vendor gaming keyboard filters and proprietary Razer/Logitech/Corsair features out of the baseline stack until the standard HID stack is functional; compliant HID keyboards should work without vendor-specific drivers.

See [keyboard driver stack design](docs/keyboard-driver-stack.md) for the current boundary and known limitations.

## P2 — ETW and miscellaneous native exports

- [ ] Test ETW multi-session fan-out, independent filters, duplicate names, stop/rundown races, ring overrun, invalid buffers, activity IDs, and process-exit concurrency.
- [ ] Expose the Windows-compatible StartTraceW/ControlTraceW controller surface and complete EnableTraceEx2/QueryAllTracesW behavior.
- [ ] Add user-mode SEH containment around DLL detach callbacks.
- [ ] Replace remaining placeholder exports only with verified Windows 11 x64 signatures and implemented prerequisites.
- [ ] Add host tests for ETW provider lifecycle, ring wrap/drop, invalid descriptor arrays, and exact payload copying.

## P2 — ABI, boot security, and compatibility evidence

- [ ] Automate signature, structure-layout, calling-convention, import, and export-table checks against a pinned Windows 11 x64 reference manifest for every implemented subsystem.
- [ ] Add targeted tests for the x64 variadic printf bridge: Unicode BMP and supplementary characters, surrogate pairs, malformed UTF-16, width/precision, narrow/wide strings, and secure-buffer boundaries; verify final PE exports.
- [ ] Add WDK-built ABI/conformance tests and test representative Windows x64 drivers only after dependency and safety prerequisites pass.
- [ ] Configure protected release signing keys; test enrolled-key Secure Boot and TPM 2.0 measurement on firmware. CognacOS Ed25519 kernel signing is not Authenticode or Windows driver-signing policy.
- [ ] Add relocatable-image support and broaden malformed-image/firmware tests on physical hardware.
- [ ] Enable AVX/YMM, AVX-512, AMX, supervisor XSS, or PKRU only after their complete architectural state and security paths are implemented and tested.

## Acceptance rules

- [ ] Do not mark a milestone validated until the relevant tests pass on the current commit; older successful runs are historical evidence only.
- [ ] Keep one canonical task per deliverable; move completed work to Git history and keep this roadmap focused on open acceptance criteria.
- [ ] Never equate export presence, compilation, or an emulator boot with full Windows 11 semantics or third-party driver compatibility.


## Memory-manager compatibility status (2026-10-29)

The latest observed QEMU run (`40815716643`) fails the aggregate NT memory-manager translation/copy self-test before the runtime driver-load stage. A follow-up commit adds per-subtest serial diagnostics; the next run must identify and fix the failing component before this milestone can be marked green.

Recent work extends the MDL/physical allocator path with:
- Constrained physical page selection honoring inclusive low/high address bounds and the SkipBytes boundary.
- A contiguous-run path for MM_ALLOCATE_REQUIRE_CONTIGUOUS_CHUNKS, plus a best-effort contiguous attempt for MM_ALLOCATE_PREFER_CONTIGUOUS.
- MM_ALLOCATE_NO_WAIT now has reserve-backed nonpaged MDL metadata (small and large MDLs), try-lock constrained/contiguous physical allocation, try-lock NUMA allocation, and try-lock MDL registration/rollback. It never waits for pool, physical allocator, pinned-page registry, or MDL registry locks. This remains incremental compatibility: no-wait physical allocation is restricted to already-mapped low RAM, finite reserves/lock contention can return NULL, a bounded lock-free deferred-free queue now recovers rollback PFNs when allocator/registry locks are contended; queue-capacity exhaustion and QEMU/SMP stress coverage remain open.
- MmProtectMdlSystemAddress supports PAGE_NOACCESS, read-only, read/write, execute, execute/read, and explicit execute/read/write protections. Non-present MDL PTEs retain their PFN so protections can be restored and mappings safely torn down.
- Runtime coverage exercises the protection transitions and the MDL mapping/free lifecycle.

Still intentionally incomplete:
- SRAT memory affinity now feeds node-owned physical ranges; processor-local APIC/x2APIC affinity records back KeGetCurrentNodeNumber; and the generic MDL allocator routes MM_ALLOCATE_FROM_LOCAL_NODE_ONLY through the current node. A review caught and fixed the SRAT type-2 x2APIC proximity-domain offset and added a regression test. The API still uses a conservative node-0 fallback when firmware omits a CPU affinity; validate on multi-node firmware and add scheduler-maintained ideal-node thread policy before claiming full NUMA semantics.
- Non-cached and write-combined mappings remain unsupported until PAT/MTRR-aware cache-type policy is tracked consistently across every alias (identity map, direct map, and MDL system mapping). Applying cache bits to only one alias would be unsafe.
- MM_ALLOCATE_NO_WAIT has a reserve-backed try-lock path for generic and NUMA MDL requests. It returns promptly when reserve capacity or locks are unavailable and currently restricts no-wait physical frames to already-mapped low RAM; the deferred-free queue is bounded (capacity-exhaustion behavior needs stress testing), and QEMU/SMP validation remains required before claiming full Windows 11 parity.
- Physical page and contiguous-run allocation now validate high-memory direct-map aliases before removing PFNs from free lists or mutating extents, avoiding page leaks on failed mapping validation.
- This is incremental NT ABI compatibility, not a claim of complete Windows 11 memory-manager parity.

- xHCI command-ring submission now publishes command TRBs with producer-cycle handling, writes the Link TRB with Toggle Cycle on wrap, and rings Doorbell 0 only after a release/full ordering fence. Host tests cover submission, link rollover, and invalid command/ring rejection. Command completion correlation, slot/address/configure sequencing, endpoint transfer rings, USB transfers, and HID delivery remain outstanding.

- Added fixed-capacity xHCI Command Completion Event correlation keyed by command TRB DMA address, plus validated Enable Slot, Address Device, and Configure Endpoint command builders. Host tests cover completion correlation, unknown/non-completion events, and slot/input-context validation. Controller-owned event-ring draining, input-context population, command queue synchronization/timeouts, port reset, endpoint transfer rings, and HID delivery remain outstanding.


## Device power prerequisites (2026-10-29)

- [x] Add a conservative per-device D-state capability registry: default D0/D3, opt-in D1/D2, wake disabled unless explicitly registered.
- [x] Validate power query/set requests and reject unregistered wait/wake requests.
- [ ] Wire capabilities to PnP reporting and remove registry entries on device rundown.
- [ ] Implement USB hub/port power sequencing, xHCI save/restore, parent-bus wake propagation, PS/2 power/IRQ restore, and QEMU suspend/resume tests.


## Keyboard and USB power lifecycle (2026-10-29)

- [x] Register conservative D0/D3 power capabilities on WDM device creation and clean them on successful remove/surprise-remove and final deletion.
- [x] Add xHCI controller D0-exit/D0-entry primitives that retain DMA/slot contexts, snapshot/restore registers, and gate interrupts around halt/resume.
- [x] Add per-port U3 suspend and resume signalling primitives with bounded polling.
- [x] Add bounded PS/2 initialization and IRQ-synchronized keyboard suspend/resume lifecycle.
- [x] Add CI tests for simulated keyboard input across PS/2 transitions and xHCI port power-state primitives.
- [ ] Wire these primitives into live PnP/USB parent wait/wake policy, real interrupt/DPC delivery, HID transfer completion, and QEMU keyboard input before/after actual device power transitions.


- [x] Add ordered USB wait/wake orchestration with capability checks and reverse-order rollback/cancellation.
- [ ] Connect the wait/wake orchestration callbacks to real hub/device control transfers and the kernel PnP parent relation graph; current tests exercise the backend contract with a fake controller/hub/device.

# CognacOS ACPI Driver — 100-Task Production Readiness Plan

**Repository:** `sanabaloch8440/CognacOS`  
**Branch:** `main`  
**Status (2026-11-04):** tasks 1–28, 32, and 67 have implementation evidence recorded below; task 29 remains partial and unchecked; all other unchecked tasks remain open. ACPI firmware interface v4 now carries a full 255-hop PCI bridge path; dynamic pool-backed `_PRT` storage remains open. This is a readiness work log, not a production-readiness certification.

Treat each numbered item as a separately reviewable task with tests. Table parsing, AML execution, Windows-compatible PnP behavior, and hardware power management are distinct capabilities. Avoid diagnostic-print spam; prefer regression tests, structured error context, and CI artifacts.

## Firmware tables and boot validation

1. [x] Validate RSDP v1/v2 revision, lengths, checksums, and extended checksum before dereferencing.
2. [x] Validate XSDT/RSDT entry alignment, physical ranges, entry counts, and arithmetic overflow.
3. [x] Map firmware tables through a validated HAL mapping contract; never cast arbitrary physical addresses into slices.
4. [x] Validate every SDT length against mapped extents before checksum traversal.
5. [x] Define deterministic policy for duplicate/conflicting singleton tables and table instances.
6. [x] Validate FADT revision/length before optional fields and prefer valid extended GAS fields.
7. [x] Resolve DSDT through validated FADT pointers with legacy/extended fallback and mapping checks.
8. [x] Document and test SSDT enumeration and the HAL contract for contiguous table instances.
9. [x] Add table-specific validation for FACS, HPET, MCFG, MADT, SRAT, SLIT, DMAR, GTDT, PPTT, and BGRT.
10. [x] Track table provenance, physical address, revision, checksum, and mapping lifetime.

## AML parser and namespace

11. [x] Test PkgLength decoding against ACPI encoding and boundary/overflow vectors.
12. [x] Complete NameString grammar and validate root, parent, DualName, MultiName, and NullName contexts.
13. [x] Handle declaration ordering, forward references, External declarations, and cross-table duplicates.
14. [x] Implement exact AML namespace search rules for rooted, parent-prefixed, and unprefixed paths.
15. [x] Add table owner IDs so unloading/reloading removes only objects introduced by that table.
16. [x] Support reopening Scope objects without treating valid scope reuse as a duplicate.
17. [x] Parse Processor, PowerResource, ThermalZone, Device, Mutex, Event, Alias, and DataRegion fields completely.
18. [x] Complete Field, IndexField, BankField, ConnectField, AccessField, and ExtendedAccessField parsing.
19. [x] Use checked bit-offset arithmetic and validate field lengths and access widths.
20. [x] Make namespace capacity configurable or return explicit capacity diagnostics.
21. [x] Implement integer constants and AML revision-dependent integer width semantics.
22. [x] Validate Buffer, Package, and VarPackage payloads and nested package boundaries.
23. [x] Reject unterminated strings and malformed encodings without scanning past table bounds.
24. [x] Report unsupported opcode with table ID, term-boundary byte offset, opcode, and enclosing namespace path; operand failures are attributed to their owning term.
25. [x] Create an independently authored ACPI-specification-derived AML parser conformance corpus with documented provenance.

## AML execution and runtime objects

26. [x] Implement exact AML operand/target typing and conversion rules. Verified by AML runtime, method-executor, parser, and table-lifecycle regression suites; see `AML_OPERAND_TARGET_CONVERSIONS.md`.
27. [x] Complete arithmetic, logical, bitwise, conversion, and target-store opcode semantics.
28. [x] Implement If/Else/While/Break/Continue/Return with package-boundary-safe control flow.
29. [ ] Correct method argument/local lifetime, recursion, and serialization-flag behavior.

Task 29 follow-up (2026-11-04): the ACPI driver now supplies a concrete `AmlMethodSynchronizer` backed by the kernel's `KeInitializeMutex`, `KeWaitForSingleObject`, and `KeReleaseMutex` path. The AML executor validates monotonic SyncLevel across nested serialized and non-serialized calls and releases implicit mutexes on success and execution errors. Commit `878319c64d40228ec261690dfd4a5f1228dbef60` adds a kernel dispatcher self-test for the exact KMUTEX path used by ACPI: recursive ownership/signal-state transitions and a real second-thread blocking wait with ownership transfer. This does not check task 29 yet: the self-test is invoked once by the kernel lock-stress worker (dispatcher_extended_self_test) and must pass the boot smoke path; table-unload/reload and abandoned-owner behavior still need end-to-end verification. On commit d1c276b757811c8fb4651748fd9e7e344d2346c9, ACPI library tests and native ACPI WDM driver type-check both passed; kernel dispatcher type-check then exposed a Rust parsing error in drivers/pci/src/host_bridge.rs (cast followed by <), corrected in fd680ed1176e75d39de50f6bb4bbd54f12d58024. The next ACPI workflow run must confirm the complete kernel dispatcher stage.

30. [x] Implement namespace mutation rules for Store, Alias, Create*Field, and dynamic objects.

Task 30 implementation slice (2026-11-04): added `aml_mutation.rs` and
`AML_DYNAMIC_NAMESPACE_MUTATION.md`. The bounded service currently provides
transactional dynamic Name insertion/binding, canonical-target Alias insertion,
validated BufferField descriptor creation for the Create*Field family, and
Store/CopyObject forwarding to the typed runtime. The crate exports the module.
Task 30 remains unchecked: the method executor does not yet own this service or
dispatch Create*Field opcodes; dynamic namespace synchronization and table
unload integration remain open. No runtime or QEMU pass is claimed.
31. [ ] Implement Reference, DerefOf, RefOf, Index, CondRefOf, and reference lifetime semantics.
32. [x] Evaluate Package/VarPackage elements including dynamic counts.
33. [ ] Implement mutex acquire/release timeout and synchronization-level rules.
34. [ ] Implement Event signal/wait semantics and safe kernel wakeup integration.
35. [ ] Implement FACS-backed AML Global Lock acquisition/release.
36. [ ] Implement guarded OperationRegion handlers for SystemMemory, SystemIO, PCI_Config, EC, SMBus, CMOS, and supported serial buses.
37. [ ] Implement Field width/alignment, LockRule, UpdateRule, bank selection, and access attributes.
38. [ ] Implement BufferField/CreateField with bounds-checked bit-accurate behavior.
39. [ ] Implement Notify queue overflow policy and deferred callbacks outside interpreter locks.
40. [ ] Define and security-review a documented Windows 11 x64 _OSI compatibility profile.
41. [ ] Enforce instruction budgets, deadlines, recursion limits, and object-size limits.
42. [ ] Guarantee lock release and temporary-object cleanup on every error path.
43. [ ] Implement module-level AML execution and table dependency ordering.
44. [ ] Validate supported method results against independently authored expected-value vectors derived from the ACPI specification and documented Windows behavior; record unsupported cases separately.
45. [ ] Document every unsupported opcode and distinguish unsupported from malformed AML.

## Device enumeration and PnP

46. [ ] Enumerate devices across all loaded tables using one stable global namespace.
47. [ ] Evaluate _HID, _CID, _UID, _STA, and _ADR with typed results and conversion rules.
48. [ ] Apply ACPI-defined default behavior when _STA is absent and preserve each status bit meaning.
49. [ ] Generate hardware IDs, compatible IDs, instance IDs, and descriptions in Windows-compatible ordering.
50. [ ] Implement child PDO lifetime, reference counting, removal, surprise removal, and relation ownership.
51. [ ] Return independently allocated DEVICE_RELATIONS and reference each returned PDO.
52. [ ] Implement PnP START/STOP/REMOVE transitions with rollback and idempotency.
53. [ ] Honor IRP completion ownership and pending-status rules in the I/O manager contract.
54. [ ] Implement QUERY_CAPABILITIES with truthful power/removal/address/UI-number fields.
55. [ ] Audit QUERY_ID allocation, terminators, ownership, pool tags, and failure cleanup.
56. [ ] Integrate with generic PnP devnodes without duplicate registrations or stale pointers.
57. [ ] Implement _DEP dependency ordering and defer start until dependencies are ready.
58. [ ] Reevaluate _STA and notify PnP after relevant Notify events.
59. [ ] Expose namespace paths for diagnostics and driver matching.
60. [ ] Test large namespaces, duplicate IDs, disabled devices, and malformed _STA values.

## Resources, PCI, and bus integration

61. [ ] Parse fixed/variable I/O, memory, bus-number, IRQ, DMA, and address-space descriptors in _CRS.
62. [ ] Implement _PRS alternatives with priorities, ranges, alignment, length, and descriptor constraints.
63. [ ] Verify exact ABI layout, alignment, and overflow handling for CM_RESOURCE_LIST and IO_RESOURCE_REQUIREMENTS_LIST.
64. [ ] Implement _SRS only after choosing a valid alternative and confirming safe device state.
65. [ ] Re-read _CRS after _SRS and verify the programmed assignment.
66. [ ] Connect host-bridge _CRS apertures to hierarchical PCI resource allocation.
67. [x] Implement _PRT routing including link-device resolution, source indices, direct GSI routes, dynamic method-valued tables, and validated downstream-bridge swizzling.
68. [ ] Implement link-device _PRS/_CRS/_SRS arbitration and interrupt programming.
69. [ ] Validate MCFG segments/bus ranges and reject overlapping or out-of-range ECAM windows.
70. [ ] Implement bridge-window allocation, rebalance, rollback, and preservation of firmware assignments.
71. [ ] Publish PDO raw and translated resources with correct ownership and translation.
72. [ ] Detect resource conflicts across ACPI, PCI, legacy ISA, and platform reservations.
73. [ ] Test IRQ polarity/trigger conversion for MADT and _PRS encodings.
74. [ ] Implement device-specific _DMA/_CCA policy where required by architecture.
75. [ ] Fail closed on unsupported descriptors while retaining actionable structured error context.

## Power, thermal, sleep, and events

76. [ ] Implement FADT hardware-reduced and legacy power-register paths.
77. [ ] Parse and validate _S0 through _S5 sleep packages and sleep-type values.
78. [ ] Implement safe S-state sequencing with CPU, interrupt, device, and timer coordination.
79. [ ] Implement _PS0/_PS1/_PS2/_PS3 sequencing and rollback after partial failure.
80. [ ] Implement _PR0/_PR1/_PR2/_PR3 dependency graphs and reference counting.
81. [ ] Implement _PSC/_PPC/_PSR and power-resource status handling.
82. [ ] Discover GPE blocks and implement enable/disable/status-clear/dispatch lifecycle.
83. [ ] Implement _Lxx/_Exx dispatch with storm protection and deferred execution.
84. [ ] Implement EC query/events and serialized transactions with timeout recovery.
85. [ ] Implement thermal-zone _TMP/_CRT/_HOT/_PSV/_ACx/_ALx/_TZD/_SCP policy.
86. [ ] Implement fan/cooling relationships, trip-point changes, and thermal notifications.
87. [ ] Implement _PRW/_PSW wake-device handling and validate wake sources.
88. [ ] Integrate RTC/timers with sleep transitions and event timeouts.
89. [ ] Reevaluate power/thermal state on Notify without executing AML at interrupt level.
90. [ ] Test suspend/resume and power transitions under QEMU and documented hardware.

## CPU, interrupt, NUMA, and architecture

91. [ ] Validate MADT LAPIC/x2APIC, IOAPIC, interrupt overrides, NMI, and online-capable records.
92. [ ] Reconcile ACPI processor objects with MADT CPU IDs and kernel processor topology.
93. [ ] Parse SRAT affinity records safely and reconcile them with the physical memory manager.
94. [ ] Validate SLIT dimensions/distances and define deterministic fallback for missing locality data.
95. [ ] Implement NUMA-aware locality hints without assuming every CPU has a namespace object.
96. [ ] Validate DMAR structures and separate parsing from actual remapping activation.
97. [ ] Route GSI requests through the kernel-selected interrupt subsystem.
98. [ ] Coordinate CPU/memory hotplug notifications with kernel lifecycle operations.
99. [ ] Test x86-64 alignment, packed structures, unaligned reads, and pointer-width assumptions.
100. [ ] Define a versioned HAL contract for table mapping, registers, interrupts, timers, and global lock.

## Security, reliability, performance, and CI


## Acceptance gates

- [ ] Clean formatting, lint, and build for the actual target/toolchain.
- [ ] Unit and malformed-input tests pass for every parser/interpreter/resource path changed.
- [ ] QEMU boot and device-enumeration tests pass with ACPI fixtures.
- [ ] Failure injection proves initialization/PnP rollback do not leak resources or leave stale pointers.
- [ ] Supported behavior is compared against independently authored ACPI-specification-derived vectors; unsupported behavior fails closed.
- [ ] Suspend/resume, IRQ routing, EC/thermal, NUMA, and DMA-remapping claims are limited to tested features.
- [ ] Production readiness is withheld until critical-path gates have evidence.


## Firmware discovery batch 1 evidence

Tasks 1–10 were checked off after the ACPI test workflow compiled the complete library test suite and passed the focused firmware-discovery regression suites on commit `3abb1b47ea29eb451f5995509b94d93acbb8852d` (workflow run `41571153077`). The test workflow compiles all library tests, then runs RSDP, root-table, DSDT, SSDT/provenance, FADT GAS, FACS/HPET/BGRT/GTDT/PPTT, ACPI 5/6, and NUMA table regression suites. The broader pre-existing AML/PnP/resource tests are compiled but not all executed by this focused workflow; their existing runtime failures are outside this batch and are not claimed fixed.


## AML parser and namespace batch 2 evidence

Tasks 11–20 were implemented and verified in the focused ACPI CI workflow on commit `815d8b604e33d87ae26c4e248c830eeb24866aa7` (run `41573605837`). The workflow compiled the complete ACPI library test target and passed the focused firmware-discovery, AML parser/namespace, and AML table-lifecycle suites.

Highlights:
- PkgLength decoding now validates the caller's input limit and exercises one- through four-byte encodings and truncation/overflow boundaries.
- NameString parsing and namespace lookup cover rooted, parent-prefixed, DualName, MultiName, NullName, and ancestor-search behavior.
- External declarations are checked for compatibility, promoted by later definitions, rolled back on failed loads, and restored when the defining table unloads.
- Namespace table ownership uses 32-bit owner IDs and is integrated into the table manager's reverse-order unload path.
- Scope reopening is supported without duplicating valid namespace containers.
- Device, Processor, PowerResource, ThermalZone, Mutex, Event, Alias, and DataRegion declarations are parsed; Processor and PowerResource fixed fields are retained.
- Field, IndexField, BankField, ConnectField, AccessField, and ExtendedAccessField operands/metadata are retained and validated.
- Field offset/length conversions are checked; zero-width ReservedField handling and explicit namespace-capacity diagnostics have regression coverage.

The UEFI/QEMU build workflow remains failing separately and is not represented as passing by this ACPI library-test result.


## AML diagnostic context verification

Task 24 was verified on commit `96520689707d3bf20c2405fa79b4597861c01582` by ACPI Rust unit-test workflow run `41577363837`. The job compiled all ACPI library tests and ran the firmware/AML suites; its log explicitly reports both `structured_diagnostic_identifies_nested_term_and_scope_parent` and `structured_diagnostic_tracks_nested_scope_path` passing. Diagnostic offsets are term-boundary offsets, not guaranteed exact inner operand cursors.


## AML conformance corpus verification

Task 25 was verified on commit `d9f460432dfecc134cb9a6dcafbd6fa8a9202540` by ACPI Rust unit-test workflow run `41577872796`. The workflow compiled all ACPI library tests and passed the firmware discovery and AML parser suites; the job log explicitly reports `aml::tests::specification_derived_conformance_corpus ... ok`. The corpus is independently authored from public ACPI grammar references and documents its provenance in `drivers/acpi/AML_CONFORMANCE_CORPUS.md`; it is not ACPICA/iASL-derived.


## AML opcode semantics task 27 evidence

Task 27 was completed and verified on PR branch commit `b663d80eed8aa5a6d23a0e8a9587fedb463a4fa2` by ACPI workflow run [`41587243631`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41587243631). The workflow compiled all ACPI library tests and passed the focused firmware suites, including **45 AML method-executor tests**, **37 AML runtime tests**, **42 AML parser tests**, and **11 AML table-lifecycle tests**.

The independently authored executor implements the arithmetic, logical, bitwise, explicit data-conversion, BCD-conversion, and target-store semantics listed in `AML_OPCODE_SEMANTICS.md`. The task is checked only for these defined opcode families; it does not imply full AML interpreter or ACPI driver production readiness. The separate UEFI/QEMU workflow remains a distinct acceptance gate.


## AML control-flow task 28 evidence

Task 28 was completed on PR branch code commit `444ac7506d752a5647610f868469fe9ead7d092a` and verified by ACPI workflow [`41587859995`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41587859995). The workflow passed all configured suites, including 49 AML method-executor tests, 37 AML runtime tests, 42 AML parser tests, and 11 AML table-lifecycle tests. Documentation-only follow-up commit `8e4077dfab7a9b2f505bde7d6bcfade4fd37cc00` also passed [workflow `41587872679`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41587872679).

Control-flow execution is bounded by the enclosing Reader's package end. If and Else are handled as sibling packages, While re-evaluates its predicate each iteration, Break/Continue are consumed by the nearest While, and Return propagates to the method boundary. Regression tests cover malformed If/Else/While package lengths and standalone Else rejection. This milestone does not implement method serialization or synchronization semantics (task 29).


## AML method-frame task 29 progress (not yet complete)

The executor creates independent Local0–Local7 and Arg0–Arg6 frames, validates argument extents and arena handles, enforces the 16-level recursion limit, and checks the AML SerializeFlag/SyncLevel contract. The WDM ACPI driver now has a concrete `AmlMethodSynchronizer` backed by the Windows x64 `KeInitializeMutex`, `KeWaitForSingleObject`, and `KeReleaseMutex` exports. Each namespace method index maps to a stable 24-byte dispatcher header; lazy initialization is serialized, kernel mutant ownership provides recursive same-thread acquisition, and wait failures fail closed. Direct-return String/Buffer/Package fast paths are also guarded, rather than bypassing serialization.

The kernel mutant registry was expanded to cover the ACPI namespace capacity (2,048 entries), and an unregistered mutant now returns `STATUS_INSUFFICIENT_RESOURCES` instead of entering an unresolvable wait. Thread rundown processes mutants incrementally to avoid a 16 KiB kernel-stack temporary. The focused workflow now type-checks both the native ACPI driver for `x86_64-unknown-none` and the kernel dispatcher implementation, while retaining the host AML regression suites. Driver type-check and AML suites passed on [workflow `41604126182`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41604126182).

The integration now passes CI type-checks for both the native ACPI driver and kernel dispatcher, plus all configured AML/firmware regression suites, in [workflow `41604648484`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41604648484). The later [workflow `41604796392`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41604796392) also passed on the documentation head.

Kernel mutant registration is lazy: `KeInitializeMutex` initializes the caller-owned dispatcher header without permanently registering its address. The first wait creates the kernel ownership record; the final release retires that record when no single-object waiter or wait-all group still needs it. Wait-all registers idle mutants consistently with single-object waits. Active ownership and queued waits retain the record until handoff/release. The x64 `KMUTANT.Abandoned` flag byte at offset 48 now preserves abandoned state after an idle registry record is retired; a later single-object or wait-all acquisition reconstructs the record from that ABI field and clears it when ownership transfers. Reinitialization at a reused virtual address clears stale abandoned metadata. This avoids retaining idle or abandoned driver-image addresses without inventing a non-Windows cleanup export.

ACPI PnP callbacks now acquire an active-operation guard, and `DriverUnload` closes admission and drains active callbacks before the image can be reclaimed. The firmware interface now supplies reference/dereference callbacks; held interface references prevent unload, and table-query callbacks participate in operation rundown. Each AML method mutex now reserves the full 56-byte x64 `KMUTEX/KMUTANT` object rather than only the 24-byte `DISPATCHER_HEADER`, with compile-time assertions for its storage offset and extent. The mutant-specific dispatcher header bytes now follow the Windows 10/11 x64 layout (`MutantSize` at byte 1, `DpcActive` at byte 2, reserved byte 3), rather than reusing the event-header size-byte position. The public WDK layout is documented in [Microsoft's KMUTANT reference](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-kmutant).

A host-thread concurrency regression now runs six callers through the same serialized AML method and verifies that at most one enters the protected execution region at a time. The test passed, along with all AML suites, in [workflow `41606355832`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41606355832). The full native driver/kernel type-checks and regression suites, including the 56-byte KMUTEX storage assertions, passed in [workflow `41606544690`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41606544690). The follow-up [workflow `41606827668`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41606827668) passed after adding the ABI-visible abandoned flag handling and wait-all reconstruction. The Windows 11 mutant-header byte-layout correction then passed the same native-driver/kernel type-check and full regression suite in [workflow `41607039037`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41607039037). Kernel mutex ownership now also enters/leaves the per-thread critical region on each recursive acquire/release, and the `KMUTANT.OwnerThread` pointer is maintained through immediate acquisition, wait-any/wait-all acquisition, release handoff, and abandoned-owner handoff. The type-check/regression workflow for this APC/owner-pointer batch is running on the current head; full per-thread mutant-list integration and real QEMU scheduler contention tests remain open.

Task 29 remains unchecked: host-thread testing validates the interpreter's synchronization contract, but real kernel scheduler contention, owner handoff, abandoned-owner rundown, and unload races still need QEMU/kernel execution tests. The UEFI/QEMU workflow remains manual-only, so no runtime-kernel validation is claimed. The follow-up [workflow `41604796392`](https://github.com/sanabaloch8440/CognacOS/actions/runs/41604796392) also completed successfully on the documentation head.

Kernel mutant registration is now lazy: `KeInitializeMutex` initializes the caller-owned dispatcher header without permanently registering its address. The first wait creates the kernel ownership record; the final release retires that record when no waiter is queued. Wait-all registers idle mutants consistently with single-object waits. Active ownership and queued waits retain the record until handoff/release, while abandoned ownership remains represented for the next waiter. This removes the normal idle-driver-unload stale-address path without adding a non-Windows cleanup export. ACPI PnP callbacks now acquire an active-operation guard, and `DriverUnload` closes admission and drains active callbacks before the image can be reclaimed. The firmware interface now supplies reference/dereference callbacks; held interface references also prevent unload, and table-query callbacks participate in operation rundown. These lifecycle changes are awaiting target type-check and regression CI; they do not yet prove unload safety under real concurrent scheduler execution or hostile unload races. Task 29 remains unchecked until concurrent multi-thread execution, ownership transfer, abandoned-owner rundown, and recursive nested-method behavior are exercised against the real kernel scheduler. AML mutex acquisition currently uses an indefinite non-alertable wait; timeout semantics remain a separate follow-up (task 33). The UEFI/QEMU workflow remains manual-only during this ACPI hardening phase, so no boot-time runtime claim is made yet.

## Dynamic AML packages and hierarchical PCI interrupt routing (2026-11-04)

Tasks 32 and 67 were advanced on main through commit c78e40661fb9bdbfb4c19f993598cff9abc96ea5.

### Task 32 — dynamic Package/VarPackage evaluation

- AML method execution evaluates PackageOp and VarPackageOp, including runtime TermArg element counts and nested package values.
- Namespace NameString references can resolve Device objects for dynamic link-device references in method-returned _PRT packages.
- Caller-owned object arenas and pool-backed execution paths avoid constructing the large AML arena on the kernel stack.
- AML integer width follows the DefinitionBlock revision.
- Regression coverage includes nested dynamic packages, runtime package counts, invalid return types, and 128-element packages.

### Task 67 — dynamic _PRT and bridge hierarchy

- Name-valued and method-valued _PRT use the same route-table evaluator.
- Link-device _PRS/_CRS resolution and direct GSI entries are validated; the route path does not use PCI config-space InterruptLine as a fallback.
- PCI waits for complete bridge inventory, constructs a root-to-endpoint bridge ancestry, and passes it through the versioned ACPI firmware interface.
- ACPI validates the bridge chain, resolves bridge namespace nodes under the correct parent, searches downstream _PRT tables and swizzles INTx pins while walking toward the root.
- Missing, ambiguous, malformed, or incomplete topology fails closed.
- An AML execution guard protects namespace/table lifetime during the route query.

### Current limits — compatibility analysis, not Windows limits

The current implementation has these explicit resource/ABI ceilings:

| Limit | Current value | Interpretation |
|---|---:|---|
| _PRT entries per package | 128 | Internal fixed-capacity AML/route representation; not an ACPI or documented Windows maximum |
| Bridge hops in the route request | 8 | Version-3 request ABI array capacity; not a PCI or Windows topology maximum |
| Collected bridge records per segment | 256 | Consistent with the 256 bus-number values available in an 8-bit PCI bus-number space, provided inventory rejects duplicate/overlapping or malformed bridge topology |
| AML object arena | 768 values | Internal bounded execution budget, not a firmware format limit |

The ACPI _PRT definition specifies a Package containing a variable-length list of route packages; it does not define a 128-entry maximum. ACPI also requires the _PRT function-number field to be 0xFFFF (all functions). See [ACPI 6.4 _PRT](https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/06_Device_Configuration/Device_Configuration.html) and [ACPI 6.6 device configuration](https://uefi.org/specs/ACPI/6.6/06_Device_Configuration.html).

The UEFI PI description defines a PCI segment as containing up to 256 bus numbers. This supports a 256-record bus-topology ceiling when each accepted bridge consumes a unique secondary bus number; it does not justify silently truncating a valid inventory. Inventory overflow, duplicate secondary-bus ownership, loops, and inconsistent primary/secondary/subordinate ranges must remain hard errors. See [UEFI PI PCI segment definition](https://uefi.org/specs/PI/1.9/V5_Introduction.html).

Microsoft's public ACPI documentation describes Windows' use of ACPI tables but publishes no _PRT entry-count limit or maximum PCI bridge depth. Microsoft's ACPI bug-check documentation explicitly treats missing link nodes, missing _PRT mappings, and malformed function-specific entries as routing failures; it does not establish the present fixed caps as Windows requirements. See [Microsoft ACPI system description tables](https://learn.microsoft.com/en-us/windows-hardware/drivers/bringup/acpi-system-description-tables) and [ACPI_BIOS_ERROR routing diagnostics](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xa5--acpi-bios-error).

**Production direction:** do not describe 128 entries or eight hops as Windows limits. Remove the eight-hop ceiling through a new, separately versioned firmware-interface request (new GUID/version) with a bridge path sized for the architectural bus-number space (up to 255 hops), preserving existing v1/v2/v3 layouts. The current v3 ABI must not be enlarged in place. Ultimately replace the 128-entry _PRT ceiling with checked, pool-backed capacity derived from the validated AML package/table size and an explicit driver memory budget; until then, oversized packages must continue to fail closed. Keep the 256-record segment ceiling only with validation that bridge inventory is one-to-one with allocated secondary bus numbers and never silently truncates. These are engineering choices; no Microsoft-published numeric maxima were found.

### Verification

- ACPI library tests, native ACPI driver type-check, kernel dispatcher type-check, and firmware discovery regression suites passed in [ACPI workflow 41793056109](https://github.com/sanabaloch8440/CognacOS/actions/runs/41793056109) for commit c78e406.
- PCI PnP/shared-ABI/driver checks passed in [PCI workflow 41792199284](https://github.com/sanabaloch8440/CognacOS/actions/runs/41792199284).
- UEFI/QEMU workflow 41793056113 was still in progress at the time of the implementation report; no QEMU boot success is claimed here.
- These checks establish configured build/type-check/regression coverage only. They do not prove all firmware topologies, deep bridge chains, or Windows 11 hardware interoperability.

### Remaining follow-up tasks

1. Add ACPI firmware interface v4 with a new GUID and a 255-hop bridge-path request; retain v1/v2/v3 ABI layouts and callbacks unchanged.
2. Widen PCI requester-path discovery and ACPI traversal buffers to the same bound, with checked pool allocation or bounded local storage appropriate to IRQL/stack constraints.
3. Replace fixed 128-entry AML package/object arrays with pool-backed, validated dynamic storage governed by a documented per-method and per-table memory budget.
4. Add adversarial tests for 255-hop chains, maximum valid bus-number allocation, duplicate secondary buses, cycles, overlapping subordinate ranges, oversized AML packages, allocation failure, and rollback.
5. Keep task 67 checked for the implemented routing semantics, but do not claim unrestricted-depth or unrestricted-size routing until these follow-ups and their CI/QEMU tests pass.
## ACPI firmware interface v4 and cross-OS PCI INTx routing review (2026-11-04)

Implementation commits after the prior checklist update:

- `ed1f7fdf88915f3c0bc70eb0d38b22dcd624c229` — add version-4 ACPI firmware interface ABI and 255-entry bridge path request.
- `6f9997d34f7a92ecb5ff7c8676e4ce5ef9449d1d` — widen PCI requester-path representation to 255 bridges plus endpoint.
- `de4d28f2bc0a01b4a997d8a45b2e71c495fdb147` — allow up to 256 bus nodes in hierarchical `_PRT` traversal.
- `be3732ef7723ce0dfebb2e5812c892f4c9230d59` — publish ACPI interface v4 and preserve v1/v2/v3 query callbacks/layouts.
- `91456dbaead88af25f1939220874b2f48a3b1782` — consume and validate v4 from PCI.
- `9f038648c1c2c122ce6ac16bf9f436aabcd2ea45` — add maximum-depth PCI requester-path regression test.
- `c38a11929dde84933ff0464dbff90ff3720bc32a` — allocate the expanded route request and bridge inventory from nonpaged pool instead of placing multi-kilobyte buffers on the kernel stack.
- `eac1b30648442ee78e13bbbfef1f970d522d249e` — avoid copying the 2 KiB v4 request into another stack local in ACPI.
- `41dc287b9773f000ae9bd606e50132e04811929a` — assert v4 ABI size and GUID isolation.

### Interface and topology behavior

- ACPI firmware interface v4 uses a new GUID and a new version; v1/v2/v3 structures and callbacks remain unchanged.
- The v4 request carries up to 255 validated bridge entries, which is the maximum number of bridge transitions possible in an 8-bit bus-number space when the root bus is included.
- PCI requester path storage has 256 entries (255 bridges plus the endpoint) and uses a 16-bit path count.
- ACPI traverses up to 256 namespace bus nodes; the bridge request and segment topology are still validated and ambiguous or incomplete paths fail closed.
- PCI route request and bridge inventory scratch buffers are allocated from nonpaged pool. The ACPI callback borrows the request rather than copying its full 2 KiB payload to the kernel stack.
- The older interface callbacks remain available for compatibility; the PCI driver now queries v4 for its full-depth route operation.

### InterruptLine fallback decision — deliberately not added

Cross-platform source review does not support a generic PCI endpoint InterruptLine fallback as a Windows 11 compatibility requirement:

- Windows documents ACPI `_PRT` routing failures, including a missing link node and a missing route mapping, as ACPI BIOS routing failures. Its public documentation does not specify falling back to the PCI Interrupt Line register. See [Microsoft ACPI_BIOS_ERROR](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xa5--acpi-bios-error) and [Acpi.sys](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/acpi-driver).
- Linux resolves PCI INTx using the `_PRT` associated with the parent bus and walks upstream bridges applying standard swizzling. Its `acpi_isa_register_gsi()` use of `dev->irq` is an ISA/EISA-specific fallback constrained to IRQs 1–15, not a generic PCI endpoint route fallback. See [Linux ACPI PCI IRQ routing](https://github.com/torvalds/linux/blob/master/drivers/acpi/pci_irq.c) and [Linux PCI swizzling](https://github.com/torvalds/linux/blob/master/drivers/pci/irq.c).
- FreeBSD's documented routing architecture uses ACPI `_PRT`, PCI link devices, PCI-PCI swizzling, and legacy `$PIR`/MP-table mechanisms where applicable. Its historical ACPI link-driver behavior may inspect endpoint InterruptLine values to infer a link's firmware-selected IRQ when a link `_CRS` is absent; this is link-resource recovery, not a general endpoint route fallback. See [FreeBSD PCI Interrupt Routing](https://people.freebsd.org/~jhb/papers/bsdcan/2007/article/node5.html) and [FreeBSD routing architecture](https://people.freebsd.org/~jhb/papers/bsdcan/2007/article/node11.html).

Therefore the PCI driver's generic InterruptLine fallback remains disabled. The register is an 8-bit legacy BIOS-assigned interrupt number, not a portable GSI or a trustworthy APIC route. Using it as one could program the wrong I/O-APIC input, especially where GSI bases are nonzero or the link is reprogrammed. If a future platform-compatibility mode is added, it must be a narrowly scoped, validated legacy-PIC or link-device recovery path with explicit mode detection, IRQ range validation, and no override of a valid `_PRT` route.

### Remaining work after v4

- The `_PRT` decoder and `PrtTable` still have a 128-entry fixed-capacity representation; this is an implementation limit, not an ACPI, Windows, Linux, or FreeBSD limit. The required next change is a pool-backed variable-capacity route table whose capacity is checked against the enclosing AML package/table size and a documented memory budget.
- The AML object arena still has fixed `MAX_AML_VALUES` and `MAX_PACKAGE_ELEMENTS` capacities. Increasing constants alone is not considered completion; the arena and nested package element storage need explicit pool-backed ownership and rundown.
- Add ACPI-side 255-hop namespace-path tests, end-to-end v4 interface tests, allocation-failure tests, and QEMU validation. A PCI unit test now covers the maximum-depth requester path, but that alone does not prove the complete firmware route path.
- No Windows 11, Linux, or FreeBSD hardware interoperability claim is made from source comparison alone.
### Additional topology hardening (2026-11-04)

- `db9910d4159ac80a1240d9de42f6cfa329d7d60a` and `257cef07f8f93e5345aef47e160daa6d1983163a`: validate bridge inventory before requester-path traversal and reject duplicate secondary-bus ownership.
- `199e2d7d3b85dca7a4b5f46f3af2e2424b365ca1`: ACPI validates nested subordinate-bus windows while resolving the firmware namespace path and rejects an old v3 request whose bridge count exceeds its eight-entry payload.
- `2ac8799dcc58f9e8f85a655ed44ac8941695480f`: add a duplicate secondary-bus ownership regression test.
- `65b2403db9424b38a95b00eb815aeec389a0ae6f`: add a route-walker regression test covering 256 bus nodes and 255 bridge crossings.
- `375abafe38d11e787186197dca47d4b15d25df6c`: assert the v4 callback offset as well as the interface/request sizes.

These are source-level changes and added tests. They are not marked CI-passed until the repository's relevant GitHub Actions jobs execute successfully. The dynamic pool-backed `_PRT` representation remains outstanding as described above.

Task 30 completion record (2026-11-04, PR #51):
- Integrated CreateBitField, CreateByteField, CreateWordField, CreateDWordField,
  CreateQWordField, and extended CreateField opcode execution into the AML
  method evaluator.
- Dynamic BufferField names resolve through a bounded namespace overlay with
  checked NameString parent resolution, duplicate rejection, buffer extent
  validation, and stable non-recycled reference IDs.
- Store/CopyObject targets resolve to the correct BufferField reference; Local
  and Arg buffers can be promoted to stable arena-backed objects so field
  writes preserve object identity.
- The table manager owns the dynamic field namespace. The quiesced
  `unload_with_runtime` path invalidates table-owned Name bindings and retires
  dynamic fields before returning; runtime execution uses the paired
  `runtime_namespace` accessor and `execute_method_in_dynamic_context`.
- Focused executor, bounds, unload-ID, and binding-invalidation tests pass in
  ACPI CI run 41811235576. The UEFI/QEMU workflow is being checked separately.

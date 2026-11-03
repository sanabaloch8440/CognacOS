# CognacOS ACPI Driver — 100-Task Production Readiness Plan

**Repository:** `sanabaloch8440/CognacOS`  
**Branch:** `acpi-production-readiness-audit`  
**Status:** tasks 1–25 implemented and covered by focused CI regression suites; remaining tasks are not implied complete.

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

26. [ ] Implement exact AML operand/target typing and conversion rules.
27. [ ] Complete arithmetic, logical, bitwise, conversion, and target-store opcode semantics.
28. [ ] Implement If/Else/While/Break/Continue/Return with package-boundary-safe control flow.
29. [ ] Correct method argument/local lifetime, recursion, and serialization-flag behavior.
30. [ ] Implement namespace mutation rules for Store, Alias, Create*Field, and dynamic objects.
31. [ ] Implement Reference, DerefOf, RefOf, Index, CondRefOf, and reference lifetime semantics.
32. [ ] Evaluate Package/VarPackage elements including dynamic counts.
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
44. [ ] Differential-test supported method results against ACPICA AcpiExec.
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
67. [ ] Implement _PRT routing including link-device resolution, source indices, and direct GSI routes.
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
- [ ] Supported behavior is compared against ACPICA/iASL fixtures; unsupported behavior fails closed.
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

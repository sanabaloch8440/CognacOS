# AML reference semantics — task 31 progress

This document records the first integrated reference-semantics implementation slice. It is not a claim of complete AML conformance.

## Implemented

- `RefOf(LocalX/ArgX)` promotes the selected frame slot into a bounded arena cell and returns an `ArenaValue` reference.
- Frame-owned cells remain valid while nested methods execute, then are invalidated on both successful return and ordinary execution errors.
- `DerefOf(LocalX/ArgX)` reads the raw slot value so a stored reference is not prematurely dereferenced by ordinary TermArg evaluation.
- `Index(Package, index, target)` and `Index(Buffer, index, target)` use zero-based indexing, return a typed PackageElement/BufferByte reference, and publish it to the target.
- `CondRefOf` recognizes existing Name and Device namespace objects and dynamic BufferFields; missing namespace paths return Integer zero without publishing a target reference.
- Runtime reference validation, dereference, and stores reject arena cells retired at method-frame teardown.
- Parser-side method-term validation recognizes RefOf and CondRefOf.

## Safety and bounds

The executor owns at most 128 temporary reference roots per invocation. It does not recycle retired arena IDs; stale references fail closed. AML execution remains outside interrupt/DPC context and continues to use caller-owned bounded arenas.

## Remaining task-31 work

- Add owner/generation identity to namespace-node references so DefinitionBlock unload/reload cannot make a stale reference resolve to a different compacted namespace node.
- Complete reference handling for all SuperName forms, including OperationRegion Fields and method/other namespace objects.
- Add dedicated Buffer Index, nested/cross-method aliasing, table-unload/reload, and failure-injection regressions.
- Run the focused ACPI executor/parser/runtime suites, native WDM ACPI driver type-check, kernel dispatcher type-check, and QEMU boot gates.

Task 31 remains unchecked in `ACPI_PRODUCTION_READINESS_100_TASKS.md` until those remaining behaviors and acceptance gates are verified. No overall ACPI production-readiness claim is made.

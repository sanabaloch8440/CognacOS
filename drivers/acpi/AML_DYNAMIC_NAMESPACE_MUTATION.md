# AML dynamic namespace mutation (task 30)

This document records the first implementation slice for task 30 of
`ACPI_PRODUCTION_READINESS_100_TASKS.md`.

## Implemented boundary

`aml_mutation.rs` introduces a fixed-capacity runtime mutation service that
shares the existing `AmlNamespace`, `AmlBindings`, and `AmlObjectArena`
contracts:

- dynamic Name creation validates the initial value and publishes the binding
  only after the namespace node has been inserted;
- dynamic Alias creation resolves and stores a canonical target, preventing
  alias-chain cycles from being introduced by a new alias;
- CreateBitField/CreateByteField/CreateWordField/CreateDWordField/
  CreateQWordField/CreateField share one BufferField creation path, validate
  bit extents with checked arithmetic, and publish a bounded side-table
  descriptor only after validation;
- Store and CopyObject against a named object reuse the runtime's established
  target-type conversion and ownership semantics;
- namespace capacity, dynamic field capacity, and the u16 runtime reference
  encoding are enforced explicitly.

The dynamic BufferField descriptor is deliberately separate from the existing
OperationRegion Field metadata: its backing object is an AML Buffer value ID,
not a namespace OperationRegion node.

## Completed task-30 integration

Task 30 is now implemented end-to-end within the supported interpreter surface:

- `Machine` owns the shared dynamic namespace during execution and dispatches
  all six Create*Field opcode forms.
- `execute_method_in_dynamic_context` retains dynamic field objects across
  method calls. The legacy convenience context fails closed if a method
  publishes dynamic fields rather than returning dangling references.
- `AmlTableManager` owns the dynamic namespace, exposes it alongside the static
  namespace, and retires owner-tagged fields under its unload quiescence gate.
- `AmlBindings::invalidate_table` and `unload_with_runtime` invalidate cached
  Name values before their owning table's nodes are removed.
- Local/Arg buffer sources are promoted to stable arena objects; namespace
  Name sources retain their existing arena identity.

Namespace and dynamic-state access requires an exclusive runtime borrow; AML
execution remains outside interrupt/DPC context. The task's focused ACPI test
suite passed at the recorded CI head. Full AML language conformance and overall
ACPI production readiness remain separate checklist work.

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

## Remaining task-30 integration

This is an implementation foundation, **not completion of task 30**. The AML
method executor still needs to own and pass an `AmlDynamicNamespace` instance,
decode and execute the Create*Field opcodes, and route dynamic namespace
operations through this service. The parser's static Alias declaration and
runtime Store path also need a unified table-owner/unload policy before the
task can be checked off. Dynamic namespace mutation must remain serialized
with namespace readers and must not run at interrupt level.

No claim is made here for complete AML interpreter conformance, successful
UEFI/QEMU boot, or ACPI production readiness.

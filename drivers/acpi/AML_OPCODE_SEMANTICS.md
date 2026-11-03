# AML Opcode Semantics — Task 27 Progress

This document tracks the independently authored AML executor work for task 27 of
`ACPI_PRODUCTION_READINESS_100_TASKS.md`. It is an implementation-status note,
not a declaration that the AML interpreter is complete.

## Normative references and provenance

Behavior is derived from the public ACPI Specification 6.6, especially the AML
term grammar in Section 19.2 and the ASL operator reference in Section 19.6.
Relevant conversion operators are Sections 19.6.138–19.6.143.

- https://uefi.org/sites/default/files/resources/ACPI_Spec_6.6.pdf

All CognacOS implementation code and regression fixtures are independently
authored. No ACPICA implementation code or ACPICA test fixture is included.

## Implemented and regression-covered in this batch

### Integer arithmetic and bitwise operators

- Add, Subtract, Multiply, Divide, and Mod.
- Increment and Decrement.
- ShiftLeft and ShiftRight, including counts at or above the AML Integer width.
- And, Nand, Or, Nor, Xor, and Not.
- FindSetLeftBit and FindSetRightBit using AML one-based bit positions.
- Integer-width normalization for 32-bit and 64-bit DefinitionBlocks.
- Arithmetic result targets use the existing target's implicit result conversion.

### Logical operators

- LAnd, LOr, LNot.
- LEqual, LGreater, and LLess.
- Logical operators produce AML Integer Boolean results and do not have result
  targets.

### Explicit data conversion operators

- ToBuffer: Integer uses the DefinitionBlock width and little-endian byte order;
  Buffer is copied; String bytes include the terminating NUL, except that an
  empty String produces a zero-length Buffer.
- ToDecimalString: Integer is rendered in decimal; String is copied; Buffer
  bytes are rendered as comma-separated decimal byte values.
- ToHexString: Integer is rendered in uppercase hexadecimal; String is copied;
  Buffer bytes are rendered as comma-separated two-digit hexadecimal values.
- ToInteger: uses the explicit decimal-or-`0x`-prefixed String rules and
  little-endian Buffer conversion with the DefinitionBlock Integer width.
- ToString: accepts Buffer input, copies up to the requested length, and stops
  at the first NUL byte.

Explicit conversion targets bypass ordinary implicit Store result conversion:
the converted result's type is stored directly. This is distinct from
arithmetic operator targets, which use implicit conversion to an existing
named target's type.

### Target stores

- Arithmetic result targets route through the common Store path.
- Explicit conversion targets use direct type-preserving replacement.
- Existing Store and CopyObject target behavior from task 26 remains covered
  separately by the task-26 conversion matrix.

## Current limitations

Task 27 remains in progress. This batch does not claim complete support for
every AML data operator, compound assignment form, reference/field target,
or hardware-backed FieldUnit operation. Unsupported terms continue to fail
closed. Package result ownership, OperationRegion handlers, and broader
namespace mutation remain separate tasks.

## Verification

The focused ACPI workflow executes the AML executor suite in addition to the
runtime, parser, table-lifecycle, and firmware regression suites. Task 27 will
remain unchecked in the readiness checklist until its complete acceptance
matrix is implemented and passes CI.

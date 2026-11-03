# AML parser conformance corpus

SPDX-License-Identifier: MIT OR Apache-2.0

## Provenance

The byte streams in `drivers/acpi/src/aml.rs` test module
`specification_derived_conformance_corpus` are independently authored
first-party fixtures based on the public ACPI specification's AML grammar.
They are not copied from ACPICA, iASL, another AML implementation, or an
upstream test suite. No external source files or third-party dependencies
were added for this corpus.

This provenance is intentional: `LICENSE-POLICY.md` requires the CognacOS
ACPI/AML implementation and its tests to be independently authored and
prohibits ACPICA integration, copied ACPICA source, and close code-level
derivatives. Any future externally generated fixture must receive a separate
provenance and license review before inclusion.

## Current coverage

The table-driven corpus currently covers:

- Empty DefinitionBlock input.
- Name declaration with the Ones integer constant.
- Device declaration containing an integer-valued `_STA`.
- OperationRegion and Field declarations.
- Rejection of reserved PkgLength encoding bits.
- Rejection of an unterminated String object at the package boundary.
- Distinct rejection of an unsupported term opcode.
- Namespace rollback after every failing fixture.

The corpus exercises the structural parser and namespace loader only. It does
not claim AML interpreter conformance, method evaluation, hardware operation,
or Windows compatibility. Additional fixtures should be added alongside each
newly supported grammar production and each parser-boundary fix.

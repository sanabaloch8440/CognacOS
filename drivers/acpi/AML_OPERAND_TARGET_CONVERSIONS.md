# AML Operand, Target, and Conversion Rules

This document records the independently implemented runtime rules for CognacOS AML task 26. It is a behavior matrix, not a claim that every AML opcode or hardware FieldUnit backend is implemented.

## Normative reference and provenance

Behavior is derived from the public **ACPI Specification 6.6**, ASL Reference sections 19.3.5.4–19.3.5.8, especially Tables 19.6–19.10:

- https://uefi.org/sites/default/files/resources/ACPI_Spec_6.6.pdf

All CognacOS code and regression fixtures are independently authored. No ACPICA source, test fixture, or code-level derivative is included.

## Implicit source conversion

When an operator requires an Integer operand:

| Source object | Result |
|---|---|
| Integer | Integer value truncated to the DefinitionBlock Integer width (32 or 64 bits) |
| String | Hexadecimal digits, without a `0x` prefix; parsing stops at the first non-hexadecimal character or at the Integer width |
| Buffer | First 4 or 8 bytes, little-endian; excess high bytes are discarded; zero-length Buffer is an error |
| Package / Uninitialized / ObjectReference | Not implicitly converted to Integer |

This is intentionally different from explicit `ToInteger`: explicit conversion accepts decimal strings by default and hexadecimal strings with a `0x` prefix, and rejects values outside the selected Integer width.

## Implicit result conversion to named objects

A `Store` or operator result targeting an existing named data object is converted to the target's existing type:

| Existing target | Accepted source types and result |
|---|---|
| Integer | Integer unchanged; String parsed as implicit hexadecimal; Buffer read little-endian |
| String | Integer becomes exactly 8 or 16 hexadecimal ASCII digits; Buffer becomes two hexadecimal digits per byte separated by spaces; String is copied |
| Buffer | Integer is serialized little-endian to the existing Buffer extent; String bytes plus its terminator are copied and truncated/zero-filled to the existing extent; Buffer is copied and truncated/zero-extended to the existing extent |
| Package | Only Package is accepted; source contents are deep-copied into the existing Package while preserving the target Package identity |
| Other named object types | Rejected by this runtime conversion layer |

An existing zero-length Buffer remains zero-length. A conversion helper creating a new Buffer with no existing target uses the source-derived extent.

A Name with no bound/evaluated initializer establishes its type on first assignment. `CopyObject` bypasses implicit conversion and replaces both the named object's value and type with an owned copy of the source object.

## Field and reference targets

- BufferFields no wider than the AML Integer width are read/written as Integer values.
- Wider BufferFields are read/written as Buffer values, with little-endian bit numbering.
- Field writes preserve bits outside the field extent.
- `Store` applies the corresponding source conversion. `CopyObject` accepts only Integer or Buffer for field targets and performs no implicit conversion.
- PackageElement stores replace the element with an owned copy; mutable source objects are not aliased.
- ObjectReference runtime values can refer to namespace Names, PackageElements, BufferBytes, and BufferFields.

## LocalX and ArgX targets

- LocalX stores copy the complete source object without implicit result conversion. A LocalX containing an ObjectReference is overwritten rather than dereferenced.
- ArgX stores copy without implicit result conversion. If ArgX contains an ObjectReference, reads and writes automatically dereference it to the referenced target.
- The method-frame and method-executor paths implement the above distinction for supported reference targets.

## CopyObject

`CopyObject` does not perform implicit source or result conversion. Named targets receive a copy of the source object and its type. Field targets accept only Integer or Buffer. LocalX and ArgX use their target-specific copy/reference behavior.

## Verified implementation boundaries

The current runtime and executor tests cover Integer/String/Buffer/Package values, 32/64-bit Integer width, named-object Store and CopyObject, BufferField bit extents, Package deep-copy/identity, LocalX/ArgX state, supported ObjectReference targets, and typed Integer/String/Buffer method returns.

The following are intentionally not claimed as complete by task 26:

- `RefOf(LocalX)` and `RefOf(ArgX)` frame-slot reference creation.
- OperationRegion-backed FieldUnit hardware access, serialization, access widths, bank/index field dispatch, or region handlers.
- Package method returns whose element handles require an owning result-arena API.
- Full conversion-opcode dispatch (task 27) and the full AML operator set.

These are separate implementation boundaries and must not be inferred from the tested conversion layer.

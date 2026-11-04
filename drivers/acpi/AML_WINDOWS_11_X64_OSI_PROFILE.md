# Windows 11 x64 ACPI `_OSI` Compatibility Profile

**Status:** reviewed implementation profile; firmware interoperability remains subject to runtime validation.

## Contract

The predefined `_OSI` method accepts one String and returns an AML Integer. The Windows-compatible result convention is exactly `0x00000000` for unsupported and `0xFFFFFFFF` for supported. The implementation uses byte-for-byte, case-sensitive matching and performs no allocation, namespace mutation, hardware access, or firmware-controlled pointer dereference while comparing the argument.

## Advertised Windows version interfaces

The profile advertises the Windows version strings documented by Microsoft for Windows 2000 through Windows 11 22H2:

- Windows 2000
- Windows 2001, Windows 2001 SP1, Windows 2001.1, Windows 2001 SP2, Windows 2001.1 SP1
- Windows 2006, Windows 2006 SP1, Windows 2006.1
- Windows 2009, Windows 2012, Windows 2013
- Windows 2015, Windows 2016, Windows 2017, Windows 2017.2
- Windows 2018, Windows 2018.2, Windows 2019, Windows 2020
- Windows 2021 (Windows 11)
- Windows 2022 (Windows 11, version 22H2)

A newer profile advertises the documented earlier Windows version levels as well as its own level. This is an interface compatibility declaration, not a claim that CognacOS implements every unrelated ACPI feature.

## Deliberate exclusions

- Unknown version strings return zero.
- Non-Windows vendor strings return zero.
- ACPI feature-group strings are not advertised merely because they are defined by the ACPI specification. Windows documents its `_OSI` support as Windows-version identification; optional feature groups require separate capability implementation and must not be inferred from a Windows version match.
- No string prefix matching, case folding, substring matching, or numeric-version guessing is performed.
- No result is inferred from the host CPU, firmware vendor, DMI identity, or a user-mode setting.

## Platform overrides and security boundary

Platform quirks are immutable, kernel-owned boot policy. The optional policy pointer is accepted only through the versioned/size-checked installation contract and must remain valid for the kernel lifetime. A quirk can alter only its exact named interface; it cannot cause arbitrary AML strings to be accepted. Policy installation is a privileged kernel initialization operation, not an AML-callable operation.

The interpreter must not execute AML while holding the policy publication lock (the current policy is read through acquire/release atomic publication). Policy storage must never be freed or mutated after publication. A future mutable policy requires a separate rundown/reference protocol and is outside this profile.

## Regression requirements

The focused tests in `aml_osi.rs` cover Windows 11 and 22H2, older Windows levels, unknown versions, non-Windows strings, unsupported feature groups, and exact per-level platform suppression. Changes to this list require updating the test matrix and this document together.

## References

- Microsoft, [How to identify the Windows version in ACPI by using _OSI](https://learn.microsoft.com/en-us/windows-hardware/drivers/acpi/winacpi-osi) (Windows 11 / Windows 11 22H2 version strings and Windows return values).
- ACPI Specification 6.4, [Predefined Objects: _OSI](https://uefi.org/sites/default/files/resources/ACPI_Spec_6_4_Jan22.pdf) (feature-group semantics and integer Ones/Zero return convention).

## Scope of the review

This review covers the advertised string set, return values, matching behavior, override authority, and policy lifetime. It does not certify all AML execution paths, prove QEMU firmware compatibility, or claim that the entire ACPI driver is production-ready.

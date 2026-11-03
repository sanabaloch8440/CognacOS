# Windows 11 x64 ACPI _OSI compatibility profile

CognacOS advertises the documented Windows ACPI operating-system interface levels
through the native AML interpreter's predefined `_OSI` implementation. This is
an ACPI firmware compatibility contract; it is not a Windows kernel-version
macro, SMBIOS identity, CPUID identity, or an architecture-reporting mechanism.
There is no separate Microsoft `Windows 11 x64` _OSI string. The x64 qualifier
describes this kernel's target architecture.

## Advertised Windows levels

| _OSI argument | Meaning |
| --- | --- |
| `Windows 2021` | Windows 11 |
| `Windows 2022` | Windows 11, version 22H2 |
| Earlier documented Windows strings | Supported as backward-compatible interface levels |

The profile returns the ACPI-defined true value `0xFFFF_FFFF` for the listed
Windows levels and zero for unknown OS-version strings. A newer Windows release
reports support for earlier Windows interface levels, so the profile retains
the documented legacy strings rather than advertising only one version.

Optional ACPI feature-group strings are not enabled by this profile. They must
be enabled individually only when the corresponding feature is actually
implemented and validated; returning true for an unimplemented feature could
cause firmware to select an unsafe hardware path.

## Execution

`_OSI` is handled as a predefined interpreter interface before AML namespace
lookup. Firmware therefore does not need to declare an AML method named
`_OSI`. Both ordinary TermArg evaluation and integer predicate evaluation
use the same policy. AML string arguments are parsed within the enclosing
package bounds and capped by the runtime's fixed string capacity.

## Reference

Microsoft Learn, [How to Identify the Windows Version in ACPI by Using _OSI](https://learn.microsoft.com/en-us/windows-hardware/drivers/acpi/winacpi-osi).

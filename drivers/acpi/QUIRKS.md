# CognacOS ACPI Platform Quirks

## Boot and ownership

The UEFI loader locates SMBIOS only through the UEFI configuration-table GUIDs
(`SMBIOS3_GUID` first, then `SMBIOS_GUID`). It passes the physical entry-point
address in BootInfo v12. SMBIOS is optional: a missing, malformed, oversized,
or unmapped SMBIOS table disables platform quirks without preventing boot.

The kernel validates the entry-point extent and the complete bounded structure
table against the active identity mapping before constructing slices. It parses
SMBIOS Type 0, 1, and 2 into a fixed-size `PlatformIdentity`, matches the
reviewed registry, and publishes an immutable `QuirkPolicy`.

ACPI.sys obtains the policy through the version-2 `AcpiSciHalServices` table.
The returned pointer is read-only, kernel-owned, and valid for the kernel
lifetime. ACPI.sys must validate the table version, policy version, size, and
alignment before using it. The AML interpreter never performs SMBIOS discovery.

## Extension points

- `aml_osi::platform_osi`: applies selective OSI advertisement/suppression.
- `acpi_device::evaluate_device_identity_with_policy`: applies exact _ADR
  remaps after normal firmware identity evaluation.
- `quirks::apply_table_patches`: applies expected-byte patches to a private
  writable SDT copy and recomputes the checksum.
- `quirks::apply_policy_to_fadt`: applies only explicitly permitted FADT
  override bits.
- `QuirkPolicy::override_prt_gsi`: resolves exact segment/bus/device/function/
  pin route overrides for the PCI routing adapter.

Table patching must happen before AML namespace installation. A patch must
include the table signature, offset, expected bytes, and replacement bytes.
The SDT header (signature, length, OEM identity, and checksum byte) is never a
patch target. Firmware-owned memory is never modified.

## Safety rules

1. Prefer exact DMI matching. Require multiple identity fields for machine-
   specific quirks; use a wildcard namespace scope only when exact DMI matching
   and exact source _ADR values make the match unambiguous.
2. Do not change FADT hardware-reduced or low-power S0 capability flags as a
   workaround. The current quirk mask permits only the PM timer 32-bit flag and
   documented boot-architecture bits.
3. Never infer a PCI GSI, polarity, or trigger mode. A route override must be
   explicit and must still pass the platform interrupt/resource validator.
4. Conflicting policy entries fail closed. A malformed SMBIOS table or failed
   expected-byte patch disables that override; it does not silently mutate
   firmware data.
5. Do not add OEM-specific branches to the AML parser or interpreter.

## Clean-room and licensing

The ACPI/SMBIOS specifications and vendor documentation define externally
observable formats and behavior. Linux and ACPICA may be consulted to identify
a hardware defect and understand the required outcome. CognacOS implementation
code must be independently authored; do not copy GPL-only Linux implementation
code into the MIT OR Apache-2.0 components. If licensed third-party code is ever
integrated, preserve its complete notices and comply with its exact license
terms. Record the source of each hardware observation and test both matching
and nonmatching DMI fixtures.

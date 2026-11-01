# SYSTEM hive and control-set selection

CognacOS treats the Windows-format SYSTEM hive as the authoritative source for
installed kernel-driver service configuration. The Configuration Manager
parses registry data; it does not itself enumerate hardware or decide which
device function driver should bind.

## Control-set selection

The active boot configuration is selected by the DWORD values under
`SYSTEM\\Select`:

- `Current`: the control set selected for the current boot.
- `Default`: the preferred control set for a subsequent normal boot.
- `LastKnownGood`: the recovery control set.
- `Failed`: zero means no failed control set is recorded; a nonzero value
  identifies a control set recorded as failed.

CognacOS resolves `Current` to the corresponding three-digit key name
(`ControlSet001` through `ControlSet999`). It validates the DWORD type,
range, and existence of every nonzero referenced control-set key before
accepting the selection. Missing optional `Default`, `LastKnownGood`, or
`Failed` values are tolerated for minimal/older test hives; `Current` is
required.

Driver service configuration is then read from:

```text
SYSTEM\\ControlSetNNN\\Services\\<ServiceName>
```

Load-order metadata is read from:

```text
SYSTEM\\ControlSetNNN\\Control\\ServiceGroupOrder\\List
SYSTEM\\ControlSetNNN\\Control\\GroupOrderList\\<GroupName>
```

The active set is always `Select\\Current`; CognacOS must not silently
substitute `Default` when `Current` is absent or malformed.

## Driver service values

The service configuration parser validates `Start`, `Type`,
`ErrorControl`, `ImagePath`, `Group`, `Tag`, `DependOnService`, and
`DependOnGroup`. It preserves the distinction between boot-start,
system-start, automatic-start, demand-start, and disabled services. The
boot/system driver scheduler consumes the registry-derived group order,
tag order, and dependency metadata.

For PnP device drivers, demand-start is expected when the driver is not needed
to reach the boot volume. Boot-start is reserved for drivers required during
the loader/boot-device phase. System-start is for drivers initialized during
OS initialization, including applicable non-PnP detection cases.

## Current implementation boundary

Implemented in this milestone:

- Shared parsing and validation of `Select\\Current`, `Default`,
  `LastKnownGood`, and `Failed`.
- Validation that nonzero selectors reference existing `ControlSetNNN` keys.
- One active-control-set resolver used by both base service queries and
  extended driver metadata queries.
- Unit coverage for three-digit control-set formatting and selector bounds.

Not yet implemented by this milestone:

- Boot-loader policy for choosing `Current` from `Default` or recovery state.
- A transactional LastKnownGood retry after a critical boot-driver failure.
- A real registry symbolic-link object for the public
  `CurrentControlSet` namespace.
- Full SYSTEM-hive security descriptors, transaction logs, and complete
  Windows registry semantics.

A parsed `LastKnownGood` value is metadata only; it does not mean fallback or
rollback is operational. Do not mark recovery support complete until the
loader, hive persistence, and boot retry path are implemented and tested.

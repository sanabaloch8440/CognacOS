# NT I/O manager ownership

`wdm.rs` is the canonical device-object and IRP implementation for the NT I/O
manager. The former standalone `runtime.rs` device/IRP registries have been
retired so new code does not create a second I/O ownership model.

The migration is intentionally staged. Callers that still need the legacy
high-level request API must be moved onto WDM device objects, IRPs, stack
locations, dispatch routines, and completion before that API is reintroduced
as a thin adapter. Do not recreate independent device or IRP tables here.

The boot-file service remains a separate compatibility layer for now. Its
FILE_OBJECT lifecycle and IRP-backed create/read/cleanup/close paths are
follow-up WDM integration work; this change does not claim those hooks exist.


## Kernel resource arbiter

`resource_arbiter.rs` is the kernel-owned fixed-capacity arbitration core.
It accepts complete alternatives from a bus/PnP layer, validates address
ranges and alignment, finds non-overlapping placements, and commits all
reservations for one device atomically. A failed alternative does not mutate
the reservation database. Reservations are released by devnode owner during
failed start and removal.

The current PnP adapter exposes `assign_pnp_resource_alternative` for the
kernel's indexed devnode table. That table is still PCI-oriented; the ACPI PDO
relation database is not mapped to those devnodes. Consequently ACPI's
published `IO_RESOURCE_REQUIREMENTS_LIST` is not consumed by this selection
arbiter through a complete WDM start-device path.

There is also a distinct WDM start-IRP resource-claim table in
`wdm/pnp.rs`. It validates and tentatively reserves the translated
`CM_RESOURCE_LIST` supplied with START_DEVICE, then commits or rolls back
claims at terminal IRP completion. It does **not** choose a configuration from
`IO_RESOURCE_REQUIREMENTS_LIST`. These two stores are not yet unified; do not
describe them as one end-to-end resource arbiter or claim that ACPI
`_PRS`-to-`_SRS` assignment works.

The selection arbiter is deliberately not a firmware interface: it never
evaluates AML, writes PCI configuration space, programs interrupt controllers,
or invokes ACPI `_SRS`. Bus drivers remain responsible for translating the
selected assignment into their bus-specific firmware/hardware programming.

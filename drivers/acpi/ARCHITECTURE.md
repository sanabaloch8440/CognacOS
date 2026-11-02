# ACPI driver architecture and ownership boundaries

This document is the design guardrail for the native ACPI bus driver. It records
which layer owns each operation so ACPI work does not silently migrate into
the HAL, PCI driver, or kernel executive.

## Independent implementation and licensing

The ACPI/AML implementation in CognacOS is independently authored. ACPICA is
not a dependency or implementation source: do not integrate its code, copy its
source, or create a close code-level derivative. Use the ACPI specifications
and documented Windows driver contracts as behavioral requirements, while
maintaining an independently designed implementation and test suite. Any
external code proposed for this subsystem requires a provenance and license
review before it is accepted. See the repository [license policy](../../LICENSE-POLICY.md).

## Reference architecture

Microsoft Windows driver documentation is the primary behavioral reference:

- https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/acpi-driver
- https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/device-stacks-for-an-acpi-device
- https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/irp-mn-query-resource-requirements
- https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/irp-mn-query-resources
- https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/mapping-bus-relative-addresses-to-virtual-addresses

ReactOS is an implementation reference, not a substitute for the WDK contract:

- https://github.com/reactos/reactos/tree/master/drivers/bus/acpi
- https://github.com/reactos/reactos/blob/master/drivers/bus/acpi/main.c
- https://github.com/reactos/reactos/blob/master/drivers/bus/acpi/buspdo.c
- https://doxygen.reactos.org/df/d5f/drivers_2bus_2acpi_2pnp_8c_source.html

## Ownership boundaries

| Layer | Owns | Must not absorb |
|---|---|---|
| ACPI bus driver | Firmware-table interpretation through the firmware-table interface; AML namespace and method evaluation; ACPI device identities; _CRS, _PRS, _SRS, _PRT, _PRW, _PSx, GPE/Notify policy; ACPI PDO/filter behavior | PCI configuration-space enumeration, BAR probing/programming, global interrupt-controller programming, kernel device-object lifetime, global resource conflict arbitration |
| Kernel PnP/I/O manager | DEVICE_OBJECT/IRP lifetime, relation publication and PDO references, driver binding, start/stop/remove sequencing, device-stack dispatch, pool ownership and interface lifetime | AML parsing, firmware namespace policy, PCI BAR discovery |
| Resource arbiter | Global resource conflict detection, assignment, translated-resource validation, reservation commit/rollback and release | Firmware AML interpretation or device-specific hardware programming |
| PCI bus driver | PCI segment/bus/function discovery, configuration space, BAR sizing, bridge windows/topology, PCI PDOs, PCI resource requirements and BAR programming | AML interpreter, ACPI namespace ownership, GPE handling, generic ACPI power policy |
| HAL | Processor/platform abstraction, interrupt-controller primitives, GSI/vector translation, low-level MMIO/port access, firmware-table access and architecture-specific services | AML method execution, ACPI child enumeration, device resource policy, PCI PnP lifecycle |
| Power manager | System sleep/wake policy and global device power sequencing | Firmware namespace parsing or direct ownership of ACPI methods |
| Device function drivers | Device-specific hardware operation using translated resources and interfaces | Direct AML interpretation or assumptions about the presence of an ACPI filter |

### Device-stack rule

Windows distinguishes ACPI namespace devices enumerated as ACPI PDOs from
devices already enumerated by another bus. For a namespace-described
system-board device, ACPI can act as a bus filter above the existing bus PDO;
for a device not already represented by another bus, ACPI creates the PDO.
ACPI is transparent to function drivers. Do not create a duplicate PDO for a
PCI function that the PCI bus driver owns.

The HAL provides the platform entry point and low-level mechanisms. It does
not become the AML interpreter or the ACPI bus enumerator.

### PnP resource contract

The bus driver answers IRP_MN_QUERY_RESOURCE_REQUIREMENTS for child devices
that need resources. The bus driver answers IRP_MN_QUERY_RESOURCES for boot
configuration resources. These are different requests:

- _PRS describes possible resource configurations and feeds requirements
  generation and arbitration.
- _CRS describes the current resource configuration and must be represented
  as the device's boot/current resources where applicable.
- _SRS is invoked by the ACPI driver only when the OS has selected a
  configuration that firmware must program.
- The resource arbiter owns conflict resolution and the kernel PnP manager
  owns start sequencing. ACPI must not independently reserve global ranges.
- The PCI driver owns PCI BAR sizing/programming and bridge-window allocation.
  ACPI host-bridge _CRS apertures are input to PCI/resource-window discovery,
  not permission for ACPI to probe or program PCI BARs.

The kernel must preserve the WDM ownership contract for returned resource
buffers and device relations. Returned PDOs must remain valid through PnP
publication; the PnP relation database owns references after publication.

### Interrupt routing contract

ACPI evaluates _PRT and resolves link-device methods/resources (for example
_PRS / _CRS / _SRS) to determine the firmware-described route. The HAL owns
the actual GSI-to-vector/controller operation and interrupt-controller
programming. ACPI must not write IOAPIC registers itself. PCI bus policy
consumes the resolved route through kernel/HAL interrupt interfaces.

### Power and hotplug contract

ACPI evaluates device-specific power and wake methods, GPE methods and
Notify events. The power manager owns global system/device power policy and
the kernel PnP manager owns device transition and removal sequencing. ACPI
requests those transitions through kernel interfaces; it must not maintain a
competing authoritative PDO lifecycle. A PDO cannot be destroyed until PnP
removal and outstanding references/rundown have completed.

## Current CognacOS integration snapshot

This is an implementation status record, not a compatibility claim.

| Area | Present in source | End-to-end integration status |
|---|---|---|
| Firmware tables / MADT / MCFG / DMAR | HAL table query and parsers | Partial; table consumers and lifecycle need broader validation |
| DSDT/SSDT namespace | AML table manager and namespace loading | Connected during ACPI DriverEntry |
| AML execution | Bounded scalar method executor with arithmetic, logical, divide, increment/decrement, and control-flow support; typed data-object decoder; gated immutable Name(Buffer) extraction; typed direct Return(Buffer(...)) method results with bounded BufferSize TermArg evaluation and caller-owned storage | General typed expression evaluation, method-to-method calls, general NameString/reference evaluation in BufferSize, reference-valued method results, synchronization opcodes in the evaluator, and complete operation-region semantics remain incomplete |
| Device identity | _HID, _CID, _UID, _STA, _ADR evaluator | Used to create initial ACPI PDO inventory; unsupported methods can suppress records |
| ACPI PDO/PnP | Bus device, PDO creation, BusRelations response and kernel relation reference ownership; PDO QueryId and conservative QueryCapabilities; PDO QueryResources evaluates immutable static child _CRS Name(Buffer) objects and serializes supported fixed raw resources; QueryResourceRequirements evaluates immutable static child _PRS Name(Buffer) objects and returns multiple IO_RESOURCE_LIST alternatives, including common resources combined with dependent-function groups, address ranges and per-resource IRQ/DMA choices | Partial; general computed method-returned buffers (beyond direct Return(Buffer(...)) with the supported bounded BufferSize TermArg subset), dependent-function alternatives beyond the bounded four-configuration limit, translated interrupt vectors, _SRS assignment, ContainerID, device text, bus information, TargetDeviceRelation and complete start/stop/remove behavior remain incomplete |
| _CRS / _PRS / _SRS | Bounded resource-template parser; PDO QueryResources and QueryResourceRequirements consume immutable child Name(Buffer) objects and return WDM CM/IO resource lists; kernel fixed-capacity transactional arbiter core and indexed PnP-devnode adapter are present | Partial; the kernel now has a bounded WDM requirements decoder/selector that expands IO_RESOURCE_ALTERNATIVE choice groups, honors IO_RESOURCE_PREFERRED ordering, and supports shared level-sensitive line IRQ claims; edge/MSI/policy-bearing shared interrupts and oversized Cartesian expansions fail closed; typed method-returned buffers, assignment-to-_SRS translation and _SRS execution remain outstanding |
| _PRT | Typed route parser and link-device resolver interface | No complete AML-to-link-device-to-HAL route path |
| Power / wake | State and _PRW data primitives | No complete kernel power-manager dispatch path |
| GPE / Notify / eject | Queue and lifecycle primitives | Not connected end-to-end to GPE dispatch, PnP invalidation and removal rundown |
| ACPI 5.x / 6.x | Several revision-aware table and descriptor parsers | Do not infer full specification or Windows 11 compatibility from parser coverage |

## Integration rules for future changes

1. Before adding a kernel or HAL API, prove that the operation belongs to that
   layer and that an existing Windows-compatible contract cannot be reused.
2. ACPI may request a kernel operation; it must not reach into private kernel
   state or duplicate the kernel's lifecycle/resource database.
3. PCI configuration and BAR operations stay in the PCI bus driver. ACPI
   contributes host-bridge firmware windows and interrupt-routing information.
4. Interrupt-controller register access stays in HAL. ACPI supplies firmware
   topology, polarity, trigger mode and route policy through typed interfaces.
5. A parser unit test proves only parsing. A module test proves only the
   module. A feature is integrated only after its real IRP/interface path,
   lifetime rules, rollback and QEMU/CI behavior are exercised.
6. Do not claim Windows 11 x64 compatibility or ACPI 6.x completion until the
   corresponding end-to-end contracts and tests pass.

## ACPI PDO PnP milestone update

The PDO dispatch now handles the first device-identification requests directly
on the child PDO:

- BusQueryDeviceID returns a single REG_SZ using the ACPI enumerator prefix.
- BusQueryHardwareIDs returns a REG_MULTI_SZ ordered with the most-specific
  ACPI hardware ID first, followed by its wildcard form and compatible IDs.
- BusQueryCompatibleIDs returns the ACPI-prefixed compatible-ID list.
- BusQueryInstanceID returns the firmware _UID when available, otherwise a
  stable namespace-node-derived parent-bus-scoped identifier.
- All returned strings are allocated from the kernel paged pool with the
  PnP-owned lifetime expected by the WDM contract.
- Returned IDs are rejected when they contain characters forbidden by the
  Windows QueryId contract.
- QueryCapabilities advertises only conservative D0/D3 defaults; it does not
  claim unsupported wake, D1/D2 or globally unique instance-ID behavior.

This is not resource-start integration. The AML table manager now exposes a gated, table-identity-validated read path
for immutable Name(Buffer) objects such as static _CRS/_PRS templates. It checks
package bounds, BufferSize, initializer length and caller capacity, and zero-fills
the declared buffer tail. Control methods returning buffers are still unsupported.
The PDO now implements _QueryResources_ for immutable Name(Buffer) _CRS
objects whose resource descriptors map to fixed raw bus resources. It rejects
unassigned address ranges and unsupported AML/resource values. IRQ values are
published as raw GSI bus-relative values; HAL translation and arbiter ownership
remain outside this driver. QueryResourceRequirements is also implemented for
immutable Name(Buffer) _PRS objects, including bounded dependent-function
alternatives. Child IRP_MN_START_DEVICE now fails closed with
STATUS_NOT_SUPPORTED: the driver must not publish a child as started until the
selected configuration is translated, programmed through _SRS, and verified
against _CRS. Typed AML method-returned buffers and the complete start/stop/
remove lifecycle remain incomplete. Do not enable hardware drivers that require
resources based on this milestone alone.


The ACPI PDO creation path now registers each child with the kernel's generic
PnP devnode table through `IoPnPRegisterBusDevice`, using the AML namespace-node
identity as its stable enumerator-scoped key. The resulting devnode index is
retained in the ACPI PDO extension for later resource-assignment handoff. The
WDM BusRelations database and the generic devnode table each retain their own
PDO reference. This establishes
the identity/lifetime bridge only: it does not yet select `_PRS` resources,
translate assignments into raw/translated `CM_RESOURCE_LIST` objects, invoke
`_SRS`, or authorize child START_DEVICE success.

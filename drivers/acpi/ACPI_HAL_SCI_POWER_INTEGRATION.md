# ACPI HAL, SCI, and Platform Power Integration

## Purpose

This work closes the gap between the ACPI driver's existing bounded parsers/state machines and live QEMU platform operation. It is an integration milestone, not a claim of ACPI v6 or Windows 11 driver compatibility.

## Existing code boundaries

- `drivers/acpi/src/acpi_events.rs` defines `GasIo`, fixed/hardware-reduced register selection, sleep-register plans, GPE register blocks, and deferred event state.
- `drivers/acpi/src/acpi_power.rs` defines device power policy and an initial `_PRW` representation.
- `drivers/acpi/src/aml_region.rs` dispatches OperationRegion accesses through installed handlers but deliberately has no hardware backend.
- `drivers/acpi/src/bin/acpi.rs` is the WDM adapter and owns ACPI bus/PDO integration.
- `FadtInfo` retains the parsed FADT fields. Parsing is not evidence that a register has been mapped or programmed.

## Required implementation slices

### 1. HAL GAS backend

Add a HAL-owned, no_std GAS service. It must:
- validate GAS address-space ID, access size, bit width/offset, alignment, and checked address extent;
- implement SystemMemory through the kernel physical-memory mapping API and SystemIO through the architecture port-I/O API;
- reject unsupported spaces (including FunctionalFixedHardware without an explicit platform implementation);
- serialize read/modify/write sequences where the register semantics require it;
- never turn an AML-controlled physical address into an unchecked pointer;
- expose a stable kernel/HAL ABI consumed by the ACPI driver.

Tests: pure validation vectors for all access sizes and bitfields; fake mapping/port backends; QEMU reads of the FADT-described PM/GPE registers.

### 2. SCI registration and interrupt-safe GPE path

The ACPI driver must request the SCI GSI through the kernel interrupt manager using MADT overrides and the platform interrupt controller. Registration must retain trigger/polarity and teardown ownership. The ISR may only snapshot/acknowledge status and enqueue bounded event records. AML methods, EC transactions, PnP notifications, and power IRPs must never run at interrupt level.

Tests: SCI GSI/override translation, registration rollback, masked/unmasked lifecycle, simultaneous GPE status, edge/level acknowledgement, storm throttling, and queue overflow behavior.

### 3. Kernel deferred dispatch

Use the kernel's actual DPC/work-item API (not a driver-local polling loop). The interrupt callback queues a DPC; the DPC drains hardware event snapshots and schedules passive-level work for AML evaluation and IRP completion. Queue ownership, cancellation, unload rundown, and duplicate scheduling must be explicit.

Tests: ISR-to-DPC-to-worker ordering, no AML at DIRQL, bounded queue saturation, cancellation/rundown during unload, and exactly-once completion.

### 4. System sleep and resume

Keep system sleep queries rejected until the complete kernel power coordinator exists. Once connected:
- validate _S0–_S5 packages and FADT model;
- query/sequence device power IRPs and dependencies;
- quiesce storage, DMA, CPUs, timers, and interrupts in documented order;
- save/restore platform state and arm wake sources;
- execute legacy PM1 or hardware-reduced sleep control through HAL GAS;
- verify wake status, restore interrupt/GPE state, and resume devices in reverse dependency order.

Never advertise an S-state solely because firmware supplies a sleep type. A failed preparation must unwind before touching the sleep-enable bit.

### 5. Wake and WAIT_WAKE

Decode the complete _PRW package, including GPE device/reference, GPE number, deepest sleep state, and all power-resource references. Acquire/release those resources transactionally. Arm the selected GPE and platform interrupt before completing WAIT_WAKE as pending; complete it exactly once on signal, cancellation, device removal, or teardown. _PSW fallback must be applied only under the documented method-selection policy.

Tests: malformed package rejection, resource rollback, wake-source arm/disarm, cancellation races, pending IRP completion, and sleep-state eligibility.

### 6. Embedded controller

Implement EC OpRegion through an EC protocol backend with serialized command/data transactions, IBF/OBF polling deadlines, timeout recovery, query command handling, and SCI deferral. _Qxx methods execute only at passive level. No direct port access from AML interpreter code.

Tests: fake EC protocol state machine, timeout/recovery, query queue overflow, concurrent field access, and QEMU EC-backed fixture where available.

### 7. Thermal manager and cooling interface

Discover thermal zones and cooling devices from the AML namespace; evaluate trip points and relationships; publish the Windows-compatible thermal interface only after its ABI is pinned to the repository's Windows 11 x64 manifest. Route callbacks through passive-level work, coordinate policy updates with power IRPs, and handle trip-point changes/Notify. Unsupported active cooling must not be advertised as operational.

Tests: trip conversion/bounds, _ALx/_TZD relationships, callback rundown, thermal notification coalescing, and fake cooling-device transitions.

## Acceptance gates

- Build the actual UEFI/kernel/ACPI driver targets with the repository-pinned toolchain.
- Unit tests for GAS, GPE, SCI, deferred dispatch, _PRW, EC, thermal, and power rollback.
- QEMU OVMF boot proving FADT/GAS discovery, SCI registration, GPE delivery, and safe AML deferral.
- System sleep, WAIT_WAKE, EC, and cooling are marked supported only after end-to-end tests exist.
- No diagnostic-print spam; use existing structured test/log facilities.
- Keep the implementation independently authored and MIT OR Apache-2.0 compatible; do not copy ACPICA implementation code.

## Status

### Implemented on this branch

- Added `Hal/acpi_gas.rs` and registered it as a HAL module.
- SystemMemory GAS access uses the kernel uncached I/O-space mapper; SystemIO uses x86 port I/O.
- GAS descriptors are checked for address-space support, access-size/width consistency, bitfield bounds, natural alignment, page crossing, and I/O-port extent. Field writes preserve unrelated bits under a serialized read/modify/write operation.
- Follow-up hardening uses checked arithmetic for both read and write port extents.

### Still not integrated

- The ACPI driver does not yet consume a driver-visible GAS service table. The GAS routines are HAL-internal Rust functions, not an ACPI driver ABI.
- SCI GSI resolution/registration is not connected to the ACPI driver. The existing generic device interrupt/DPC service table is a foundation, but it does not itself resolve an ACPI SCI or publish the GAS callbacks.
- No end-to-end ISR → DPC → passive worker → AML event path has been demonstrated.
- Sleep, WAIT_WAKE, EC OpRegion, and thermal cooling remain unsupported as live hardware features. Keep their power/wake requests fail-closed.

### Validation snapshot

- Fixed all seven regressions identified in the earlier UEFI workflow: corrected `_SRS` test AML package lengths and argument counts; corrected QWord descriptor encoding/fixture lengths; modeled GPE status W1C behavior in the test backend; corrected the Windows CM interrupt flag expectation; and corrected IndexField Preserve-rule read accounting.
- ACPI unit-test workflow for commit `594e9d95a951485d6337c1680e6fb8e3c1291954`: passed. The separate UEFI/kernel workflow for that commit is still running; it has not yet reached the QEMU boot test.
- Earlier UEFI run `41724422941` reached the ACPI suite and exposed three remaining fixture/expectation issues after the first batch; these were fixed in `d19ff7a` and `594e9d9`. Do not use the earlier `8e2f7df` run as validation of the final test fixes.

This remains an integration workstream, not a claim of ACPI v6 or Windows 11 driver compatibility. The HAL GAS module is still not consumed by the ACPI driver, and the driver currently has no live `GasIo` implementation, SCI ISR registration, or ISR-to-DPC/work-item dispatch. The next architectural change is a versioned ACPI HAL interface with a real WDM `IRP_MN_QUERY_INTERFACE` discovery endpoint; it must be backed by the kernel's actual HAL device/service lifecycle rather than a fabricated Windows export. After discovery is working, wire GAS, SCI route ownership, deferred AML execution, and unload rundown.

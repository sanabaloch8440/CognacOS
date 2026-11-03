# ACPI driver developer guide

This document describes the intent, ownership boundaries, and completion contracts of
`drivers/acpi`. It is a native Rust ACPI bus driver, not a general-purpose firmware
emulator. Parsers are bounded and fail closed: unsupported AML opcodes or resource
forms must not be guessed, silently accepted, or converted into unsafe hardware access.

## Source map

| Source | Responsibility | Important contract / next implementation area |
|---|---|---|
| `src/lib.rs` | ACPI SDT/RSDP, FADT, MADT, MCFG, SRAT, SLIT, DMAR/RMRR parsing and PCI-root `_CRS` apertures | Validate firmware lengths/checksums/ranges before publishing; connect topology and root apertures to kernel-owned consumers. |
| `src/aml.rs` | AML definition-block parser and namespace construction | Parsing retains validated method-body ranges; it must not execute methods or perform I/O. Extend opcode/object coverage without weakening package bounds. |
| `src/aml_objects.rs` | Bounded typed AML object decoding into caller-owned storage | Preserve object type/length and reject unsupported package/reference forms. |
| `src/aml_runtime.rs` | Namespace references, Name bindings, dereference/store and conversions | References must remain scoped to a live namespace/table generation. |
| `src/aml_exec.rs` | Bounded scalar AML method execution | This is not yet a complete AML interpreter. Add typed operands, references, method serialization and complete control-method semantics incrementally. |
| `src/aml_buffer.rs` | Static Name(Buffer) and direct Return(Buffer) extraction | General computed BufferSize and method-returned object evaluation still require integration with the typed evaluator. |
| `src/aml_region.rs` | OperationRegion decoding and address-space handler dispatch | Never dereference firmware addresses directly. Install handlers through the platform/kernel contract and enforce region bounds. |
| `src/aml_field.rs` | Field access, AML Mutex and Event primitives | Finish access widths/attributes, BankField/IndexField, synchronization integration and blocking wait semantics with kernel scheduling. |
| `src/aml_tables.rs` | DSDT/SSDT validation, namespace install/unload, execution rundown | New calls must be gated against quiesce; unload only after all users have exited and namespace rollback is valid. |
| `src/acpi_device.rs` | `_HID`, `_CID`, `_UID`, `_ADR`, `_STA` identity evaluation | Identity evaluation is separate from resource assignment and PDO start. |
| `src/acpi_resources.rs` | `_CRS` and `_PRS` resource-template decoding | Parsing describes resources; it does not reserve, translate, or program them. |
| `src/acpi_wdm_requirements.rs` | Serialize resource alternatives into WDM `IO_RESOURCE_REQUIREMENTS_LIST` | Kernel PnP/resource arbiter chooses a configuration. |
| `src/acpi_wdm_resources.rs` | Serialize assigned resources into WDM `CM_RESOURCE_LIST` | Publish only resources actually assigned and translated by kernel PnP. |
| `src/acpi_prt.rs` | Validate evaluated `_PRT` entries and resolve link-device sources | Complete AML-to-link-device-to-HAL routing is still an integration milestone. |
| `src/acpi_power.rs` | `_PRS`, `_SRS`, `_PRW`, and device D-state policy primitives | Connect to the kernel power manager; implement complete system sleep/wake sequencing. |
| `src/acpi_hotplug.rs` | Notify event queue, eject, removal and wake lifecycle primitives | Connect event delivery to GPE dispatch, PnP invalidation, cancellation and PDO rundown. |
| `src/bin/acpi.rs` | Native ACPI bus FDO/PDO and WDM dispatch integration | Adapter only. Kernel owns devnodes/resources; PCI owns config/BARs; HAL owns interrupt hardware. |

## Ownership rules

1. **ACPI interprets firmware.** It validates tables, evaluates AML and reports device
   identity, resource requirements, routing and power capabilities.
2. **Kernel PnP owns device lifetime and resource arbitration.** ACPI must not assign
   arbitrary BARs, publish guessed translated resources, or bypass start/remove IRPs.
3. **PCI owns PCI configuration and BAR programming.** ACPI may describe root-bridge
   windows and interrupt routing but must use the PCI service contract.
4. **HAL owns interrupt-controller operations.** ACPI must not program IOAPIC/LAPIC
   registers; it requests GSI/vector services through HAL.
5. **Firmware inputs are untrusted.** Check every length, arithmetic operation, index,
   count, namespace handle and generation. Unsupported descriptors fail closed.
6. **Lifetime is part of correctness.** Table unload, PDO removal, method execution,
   wake resources and asynchronous Notify work require rundown/reference protection.

## Completion checklist

- [ ] Full AML typed expression and reference evaluator, including method-to-method calls
      with per-invocation argument/local frames, serialization and recursion limits.
- [ ] Complete OperationRegion/Field semantics: all access widths and attributes,
      BankField, IndexField, connection objects, and kernel-backed synchronization.
- [ ] Dynamic `_CRS`/`_PRS` evaluation, dependent alternatives, arbitration, `_SRS`
      programming, post-program `_CRS` verification, rollback and translated publication.
- [ ] Complete ACPI PDO start/query/stop/remove lifecycle with reference rundown and
      correct raw/translated `CM_RESOURCE_LIST` ownership.
- [ ] End-to-end `_PRT` and link-device routing through PCI and HAL contracts.
- [ ] GPE register/event dispatch, `_Lxx`/`_Exx`, Notify work queues, cancellation and
      PnP bus-relations invalidation.
- [ ] Device/system power integration: `_PSC`, `_PSx`, `_PRx`, `_PRW`, wake arming,
      sleep entry, wake vector, resume and failure rollback.
- [ ] ACPI 5.x/6.x table/opcode/descriptor coverage validated against specification
      fixtures and real firmware, including revision-specific optional fields.
- [ ] QEMU and hardware validation across multiple firmware implementations, SMP,
      PCI bridges, interrupt modes, sleep/wake and hotplug.

## Commenting and test conventions

Each Rust source module starts with module-level `//! ` documentation describing its
purpose and ownership boundary. Public APIs should have `///` documentation that
states what is validated, what side effects occur, and which subsystem owns the next
step. Keep comments focused on invariants and non-obvious ordering; do not restate
obvious syntax. Every correctness fix should add a regression test for malformed
input, rollback, stale generations, or ordering as applicable.

A parser unit test proves parsing only. A serializer test proves ABI encoding only.
A module test proves module behavior only. Mark a feature integrated only when the
real IRP/interface path, object lifetime, failure rollback and QEMU/CI behavior are
also exercised. Do not infer Windows 11 compatibility or full ACPI 6.x support from
table revision parsing alone.

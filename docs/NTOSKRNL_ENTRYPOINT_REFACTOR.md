# NTOSKRNL entry-point and driver-boundary refactor

## Scope of this pass

This pass separates the monolithic Rust entrypoint into explicit boot, executive-test, and I/O-driver orchestration modules. It preserves behavior while establishing the seams required for the subsequent Windows-style driver lifecycle conversion. No driver behavior is deleted solely because it is CognacOS-specific; removals require a replacement path and passing regression tests.

## Resulting ownership

| Path | Responsibility |
|---|---|
| `ntoskrnl/src/main.rs` | Crate composition, the ABI-visible `_start` trampoline, serial/panic terminal path |
| `ntoskrnl/src/boot/handoff.rs` | UEFI handoff structure, linker section symbols, validated boot asset views and registry/NLS mounting |
| `ntoskrnl/src/boot/initialize.rs` | NT executive initialization sequence and boot-time subsystem sequencing |
| `ntoskrnl/src/boot/platform_devices.rs` | Temporary firmware-to-boot-driver coordination for ACPI, PCI, VT-d and USB; legacy keyboard bring-up is still here and is scheduled for migration |
| `ntoskrnl/src/io/driver_self_test.rs` | WDM/DriverEntry/IRP and Beep driver QEMU smoke fixture; not part of normal driver implementation |
| `ntoskrnl/src/ke/self_test.rs` | Scheduler/thread/dispatcher boot smoke fixtures |

## Windows architecture constraints applied

1. Executive services remain in NTOSKRNL: object, memory, process/thread, I/O, configuration and security managers. HAL and device drivers are separate architectural components.
2. Drivers own device-specific state and callbacks. Their DriverEntry initializes driver-wide state; AddDevice and PnP start handling initialize individual devices.
3. PnP drivers receive resource assignments through IRP_MN_START_DEVICE. The PnP manager, not a function driver, arbitrates hardware resources.
4. Interrupt handling is split: a short ISR acknowledges/records device state and defers remaining work to a DPC.
5. Boot-start drivers are loaded by the OS loader before kernel entry. PnP then configures the boot device hierarchy; remaining PnP devices are configured afterward.

## Remaining migration work (intentionally not papered over)

- Replace the kernel-owned PS/2 polling/translation path in `boot/platform_devices.rs` with the existing `i8042prt` -> `kbdclass` driver stack. The PE driver sources already contain their own ISR/DPC and class-service paths; the bootstrap handoff/resource path must be wired before deleting the duplicate path.
- Remove kernel-embedded, hard-coded ACPI/PCI/USB driver image selection in favor of the registry-backed boot-driver list and loader-provided images. Keep only the minimal firmware/loader handoff needed before the PnP manager is operational.
- Move device resource acquisition, interrupt registration, DMA setup and start/stop/power transitions behind the appropriate bus/function driver PnP lifecycle.
- Remove QEMU-only audio and boot-smoke behavior from production initialization after the CI test harness invokes the same driver contracts independently.
- Audit unused imports, dead entrypoints, compatibility shims and duplicated state after each migration; delete only when repository-wide references and tests establish that the path is obsolete.

## Microsoft references

- [Kernel-mode architecture and executive components](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/)
- [Executive support library](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/windows-kernel-mode-executive-support-library)
- [WDM driver structure and device stacks](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/writing-wdm-drivers)
- [DriverEntry responsibilities](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/writing-a-driverentry-routine)
- [Device objects and stacks](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/device-objects-and-device-stacks)
- [Boot-start driver load ordering](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/specifying-driver-load-order)
- [PnP device start and assigned resources](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/irp-mn-start-device)
- [ISR/DPC separation](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/introduction-to-dpc-objects)

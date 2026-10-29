# Keyboard input stack

CognacOS now has initial, separately buildable Rust driver components for the Windows-style keyboard stack:

- **kbdclass.sys** — bounded, fixed-capacity keyboard packet queue, connect/disconnect state, exclusive open semantics, and KEYBOARD_INPUT_DATA-shaped packets.
- **hidclass.sys** — bounded HID short-item report-descriptor parser and top-level keyboard collection discovery.
- **kbdhid.sys** — boot-keyboard report differencing, common HID usage to scan-code mapping, modifier make/break tracking, and HID rollover handling.
- **i8042prt.sys** — PS/2 port protocol helpers with bounded controller polling, ACK/RESEND handling, and scan-code Set 1 decoding. HAL now exposes privileged x86 port-I/O and volatile MMIO primitives behind unsafe APIs.
- **usbxhci.sys** — xHCI core with checked capabilities, Link-TRB/cycle handling, USB descriptor parsing, enumeration-state validation, bounded controller startup, a HAL-backed platform adapter for the versioned DMA/interrupt service table, allocation/ERST initialization with rollback, live DMA event-ring draining with ERDP updates, bounded command-ring submission with Link-TRB cycle rollover and Doorbell 0 notification, and safe stop-before-free rundown. CI builds a separate native PE32+ image and runs host-side unit tests.

## Architecture

For a HID keyboard, the intended path is:

USB/Bluetooth transport -> HIDClass -> KbdHid -> KbdClass -> input consumer

For a legacy PS/2 keyboard, the intended path is:

i8042 controller -> i8042prt -> KbdClass -> input consumer

This mirrors the broad Windows class/port/client layering. Windows' kbdclass.sys provides the common keyboard class service; key-layout translation is normally performed by the input subsystem/user-mode layout machinery, not by the class driver itself. Windows' kbdhid.sys is a HID client mapper and relies on HIDClass and a transport/minidriver beneath it.

## Validation

GitHub Actions host-tests queue bounds and exclusive ownership, HID descriptor parsing, boot-report transitions/rollover, PS/2 scan-code decoding and bounded polling, PCI ECAM/BAR decoding, HAL device-service contracts, and xHCI capability/TRB/ring/ERST/controller-start/descriptor/enumeration primitives. It builds kbdclass, hidclass, kbdhid, i8042prt, and usbxhci as separate AMD64 PE32+ images with the repository's current driver-image pipeline.

## Explicit limitations

These are driver-core foundations, **not yet end-to-end working hardware drivers**. The current DriverEntry routines are minimal loader stubs. The xHCI core now contains a generic runtime orchestration path that can allocate four DMA regions through an injected platform adapter, populate the ERST, register an interrupt before controller startup, consume live event-ring entries, and stop the controller before freeing DMA memory. A concrete adapter now translates the HAL DeviceServiceTable into the xHCI platform interface, validates MMIO bounds/alignment, requests contiguous coherent DMA with explicit flags, and connects interrupt callbacks. This is integration glue, not a finished hardware backend: the kernel still does not populate the table with real DMA/IOMMU and interrupt implementations, and DriverEntry does not invoke this runtime path. Real PCI scanning/BAR mapping, IOMMU-backed DMA, an ISR/DPC implementation, command/doorbell and port-reset/slot/address/configure commands, control and interrupt transfers, HID report delivery, PnP/IRP registration, PS/2 IRQ/DPC packet delivery, keyboard input syscall/console delivery, and registry-based service/INF installation remain unwired. HID USB/Bluetooth transport, actual HIDClass IRP/IOCTL forwarding, PnP/device-object registration, interrupt/DPC integration, safe PS/2 interrupt registration and scan delivery, keyboard input syscall/console delivery, and registry-based service/INF installation remain unwired. The kernel HAL now also provides bounded/alignment-checked 32-bit MMIO accessors and explicit acquire/release/sequence-cst device-ordering barriers, and defines a separately versioned extended service contract for explicit uncached/write-combined device mappings and interrupt synchronization. This follows the Windows driver model's separation of device-resource mapping, DMA adapter operations, and ISR synchronization; it is not a claim of binary-compatible Windows HAL exports. The kernel still needs concrete mapping and synchronization callbacks. PE builds and host tests do not establish keyboard operation in QEMU or on physical hardware. Do not load these drivers against hardware until the missing runtime plumbing is implemented and tested.


The command-ring API submits bounded command TRBs and rings Doorbell 0 after publishing the TRB with release ordering. It reports submission only; command completion events must still be consumed and correlated by the controller owner. It does not yet implement slot enable/address-device/configure-endpoint sequencing, USB control/interrupt transfers, or HID report delivery.

The next controller-core milestone adds a fixed-capacity command completion tracker keyed by the command TRB DMA address, with explicit errors for unknown or malformed completion events. It also adds validated builders for Enable Slot, Address Device, and Configure Endpoint command TRBs. These are primitives only: callers must track commands before processing interrupts, validate completion codes, allocate/populate input contexts, and perform the required USB enumeration state transitions.


## Power-management readiness (audit, 2026-10-29)

Microsoft's WDM model requires PnP capability reporting, power-IRP dispatch/forwarding, D-state transitions, and wait/wake propagation through the device stack. CognacOS has power-IRP lifecycle tracking and an S4 codec, but USB hub/port policy, a real xHCI PnP/IRP DriverEntry path, and PS/2 IRQ/DPC delivery remain unwired. It therefore cannot yet guarantee end-to-end power/resume/wake behavior for USB 3.x devices or USB/PS/2 keyboards.

A conservative capability registry defaults unregistered devices to D0/D3 and rejects wait/wake unless the device explicitly registers verified support. D1/D2 and wake are opt-in; drivers must still implement hardware changes in IRP_MJ_POWER. xHCI command submission now reserves completion tracking before ringing the doorbell, and live event routing correlates command completions while surfacing transfer and port-status events. These are prerequisites, not end-to-end hardware support.


## Device power transition implementation (2026-10-29)

- WDM device creation now registers conservative D0/D3 capabilities; successful PnP remove/surprise-remove and final object deletion clear registry entries.
- The xHCI core provides per-port U3 suspend/U0 resume primitives and controller D0-exit/D0-entry routines that mask interrupts, halt without freeing DMA or resetting slots, snapshot controller registers, and restore them before re-enabling interrupts. Callers must quiesce transfers before controller halt and propagate wait/wake through the USB parent stack.
- The PS/2 core provides bounded 8042 self-test/configuration/port-test startup and a lifecycle owner that masks and synchronizes IRQ delivery before suspend, disables scanning, and reinitializes before restoring IRQ delivery on resume.
- CI unit tests cover simulated scan-code delivery before/after PS/2 power transitions and xHCI port state changes. These are deterministic driver tests, **not** proof of live USB HID, PS/2 IRQ/DPC delivery, or QEMU keyboard input across real power transitions.

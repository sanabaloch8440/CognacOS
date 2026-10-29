# Keyboard input stack

CognacOS now has initial, separately buildable Rust driver components for the Windows-style keyboard stack:

- **kbdclass.sys** — bounded, fixed-capacity keyboard packet queue, connect/disconnect state, exclusive open semantics, and KEYBOARD_INPUT_DATA-shaped packets.
- **hidclass.sys** — bounded HID short-item report-descriptor parser and top-level keyboard collection discovery.
- **kbdhid.sys** — boot-keyboard report differencing, common HID usage to scan-code mapping, modifier make/break tracking, and HID rollover handling.
- **i8042prt.sys** — PS/2 port protocol helpers with bounded controller polling, ACK/RESEND handling, and scan-code Set 1 decoding. HAL now exposes privileged x86 port-I/O and volatile MMIO primitives behind unsafe APIs.
- **usbxhci.sys** — xHCI core with checked capabilities, Link-TRB/cycle handling, USB descriptor parsing, enumeration-state validation, bounded controller startup, a HAL-backed platform adapter for the versioned DMA/interrupt service table, allocation/ERST initialization with rollback, live DMA event-ring draining with ERDP updates, and safe stop-before-free rundown. CI builds a separate native PE32+ image and runs host-side unit tests.

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

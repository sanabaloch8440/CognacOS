# Keyboard input stack

CognacOS now has initial, separately buildable Rust driver components for the Windows-style keyboard stack:

- **kbdclass.sys** — bounded, fixed-capacity keyboard packet queue, connect/disconnect state, exclusive open semantics, and KEYBOARD_INPUT_DATA-shaped packets.
- **hidclass.sys** — bounded HID short-item report-descriptor parser and top-level keyboard collection discovery.
- **kbdhid.sys** — boot-keyboard report differencing, common HID usage to scan-code mapping, modifier make/break tracking, and HID rollover handling.
- **i8042prt.sys** — PS/2 port protocol helpers with bounded controller polling, ACK/RESEND handling, and scan-code set 1 decoding.

## Architecture

For a HID keyboard, the intended path is:

USB/Bluetooth transport -> HIDClass -> KbdHid -> KbdClass -> input consumer

For a legacy PS/2 keyboard, the intended path is:

i8042 controller -> i8042prt -> KbdClass -> input consumer

This mirrors the broad Windows class/port/client layering. Windows' kbdclass.sys provides the common keyboard class service; key-layout translation is normally performed by the input subsystem/user-mode layout machinery, not by the class driver itself. Windows' kbdhid.sys is a HID client mapper and relies on HIDClass and a transport/minidriver beneath it.

## Validation

GitHub Actions now host-tests queue bounds and exclusive ownership, HID descriptor parsing, boot-report transitions/rollover, PS/2 scan-code decoding and bounded polling. It also builds the four components as native AMD64 PE32+ images with the repository's current driver-image pipeline.

## Explicit limitations

These are driver-core foundations, **not yet end-to-end working hardware drivers**. The current DriverEntry routines are minimal loader stubs. The HID USB/Bluetooth transport, actual HIDClass IRP/IOCTL forwarding, PnP/device-object registration, interrupt/DPC integration, real HAL-backed x86 port I/O, keyboard input syscall/console delivery, and registry-based service/INF installation are not yet wired together. The PE build and host tests do not establish that a keyboard works in QEMU or on physical hardware. Do not load these drivers against hardware until the missing runtime plumbing is implemented and tested.

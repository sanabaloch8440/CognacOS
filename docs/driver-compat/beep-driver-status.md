# CognacOS Beep driver implementation status

## Implemented core

`drivers/beep` contains a `no_std` Rust implementation of the Windows x64 Beep protocol and PC-speaker programming core:

- `IOCTL_BEEP_SET` is `0x00010000` (FILE_DEVICE_BEEP, function 0, METHOD_BUFFERED, FILE_ANY_ACCESS).
- `BEEP_SET_PARAMETERS` is represented as two little-endian 32-bit ULONG values: frequency and duration in milliseconds; its size is asserted to be 8 bytes.
- Input length, IOCTL value, frequency (37–32767 Hz), and duration (0–10000 ms) are validated before use.
- PIT channel 2 is programmed in mode 3 using the 1,193,182 Hz PIT input clock; the PC-speaker gate/data bits on port 0x61 are enabled and cleared by the backend.
- Port access is abstracted behind `SpeakerIo`, so tests can verify the exact hardware sequence without executing privileged I/O instructions.

## Deliberate non-claims

This is **not yet a loadable Windows 11-compatible `beep.sys`**. The protocol and hardware core are implemented, but the WDM integration is not complete. In particular, the current CognacOS kernel/driver contract does not yet provide the complete combination needed by this driver:

1. A real `IoCreateDevice` / `IoCreateSymbolicLink` device-object and `\\Device\\Beep` / `\\DosDevices\\Beep` namespace lifecycle.
2. A complete WDM dispatch path for IRP_MJ_CREATE, IRP_MJ_CLOSE, and IRP_MJ_DEVICE_CONTROL, including METHOD_BUFFERED SystemBuffer capture and IO_STATUS_BLOCK completion.
3. A kernel timer/DPC service with cancel-safe rundown so a duration expiry stops the speaker without busy-waiting or blocking a dispatch thread.
4. A documented, versioned HAL port-I/O service available to a loaded driver, including privilege/ownership policy for ports 0x42, 0x43, and 0x61.
5. QEMU guest tests that open `\\.\\Beep`, issue `IOCTL_BEEP_SET`, observe PIT/speaker state, verify automatic duration stop, and unload cleanly.

The driver image must not return success or be packaged as a working `.sys` until these dependencies are connected. The previous imported 24-symbol Windows `beep.sys` compatibility audit remains a separate target; its import set is not interchangeable with this smaller in-tree implementation.

## CI

The build workflow checks and runs host-side tests for the protocol parser, ABI size, IOCTL value, invalid parameter rejection, PIT divisor calculation, and speaker port sequence. These tests do not execute privileged hardware I/O and do not constitute guest driver validation.

# CognacOS Beep driver implementation status

## Implemented core

`drivers/beep` contains a `no_std` Rust implementation of the Windows x64 Beep protocol and PC-speaker programming core:

- `IOCTL_BEEP_SET` is `0x00010000` (FILE_DEVICE_BEEP, function 0, METHOD_BUFFERED, FILE_ANY_ACCESS).
- `BEEP_SET_PARAMETERS` is represented as two little-endian 32-bit ULONG values: frequency and duration in milliseconds; its size is asserted to be 8 bytes.
- Input length, IOCTL value, frequency (37–32767 Hz), and duration (0–10000 ms) are validated before use.
- PIT channel 2 is programmed in mode 3 using the 1,193,182 Hz PIT input clock; the PC-speaker gate/data bits on port 0x61 are enabled and cleared by the backend.
- Port access is abstracted behind `SpeakerIo`, so tests can verify the exact hardware sequence without executing privileged I/O instructions.

## Deliberate non-claims

This is **not yet a loadable Windows 11-compatible `beep.sys`**. The protocol and hardware core are implemented, but the beep driver's WDM integration is not complete. The kernel already has device-object, symbolic-link, IRP buffering/completion, timer/DPC, and versioned HAL port-I/O primitives; the missing work is connecting these services in a real beep `DriverEntry` and dispatch implementation. In particular, the current CognacOS kernel/driver contract does not yet provide the complete combination needed by this driver:

1. The beep driver still needs to call the kernel's existing `IoCreateDevice` / `IoCreateSymbolicLink` APIs and implement its `\Device\Beep` / `\DosDevices\Beep` namespace lifecycle.
2. The beep driver still needs native `IRP_MJ_CREATE`, `IRP_MJ_CLOSE`, and `IRP_MJ_DEVICE_CONTROL` dispatch routines. The kernel already implements buffered SystemBuffer capture and IRP completion; the driver must use them correctly.
3. The kernel already exports timer/DPC entry points (`KeInitializeTimer`, `KeSetTimer`, `KeCancelTimer`, `KeInitializeDpc`, `KeInsertQueueDpc`, and `KeRemoveQueueDpc`). The beep driver needs a timer/DPC adapter with cancellation and unload rundown. `SpeakerDeadline` now supplies a tested generation-ticket primitive so stale callbacks can be rejected; it is not itself a scheduled kernel timer.
4. The HAL already publishes versioned port-byte read/write services in its driver service table. The driver still needs a supported way to acquire that table and enforce exclusive ownership/serialization for ports 0x42, 0x43, and 0x61.
5. QEMU guest tests that open `\\.\\Beep`, issue `IOCTL_BEEP_SET`, observe PIT/speaker state, verify automatic duration stop, and unload cleanly.

The driver image must not return success or be packaged as a working `.sys` until these dependencies are connected. The previous imported 24-symbol Windows `beep.sys` compatibility audit remains a separate target; its import set is not interchangeable with this smaller in-tree implementation.

## CI

The build workflow checks and runs host-side tests for the protocol parser, ABI size, IOCTL value, invalid parameter rejection, PIT divisor calculation, and speaker port sequence. These tests do not execute privileged hardware I/O and do not constitute guest driver validation.

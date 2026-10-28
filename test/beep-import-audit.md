# Beep.sys WDM runtime test: imported-routine audit

Target ABI: Windows 11 x64 / NT 10.0.26100 family. This is a source audit of the current CognacOS implementation, not a claim that the imported routines are fully Windows-compatible.

## Binary disassembly findings (current `test/beep.sys`)

The CI PE inspection and x64 disassembly identify this image as AMD64 PE32+ with native subsystem, entry RVA `0x6010`, image size `0xA000`, and no COFF symbol table (`SymbolCount: 0`). Its only import descriptor is `ntoskrnl.exe`; the imported names include WDM I/O, queue, synchronization, IRQL, and pageable-image routines. **It does not import `DbgPrint`, `DbgPrintEx`, or another explicit debug-print routine.** Therefore we cannot expect driver-originated debug strings from this binary. That is a property of this image, not proof that the kernel cannot debug it.

The disassembly does contain ordinary x64 driver code and indirect calls through the IAT, so the kernel can still observe the driver at its entry point and at WDM dispatch boundaries. Because the binary has no symbols, source-level function names/locals are unavailable; diagnostics should identify loaded image plus RVA, callback address, IRP major/minor code, and returned NTSTATUS.

## Classification

| Imported routine / group | Current source evidence | Classification / caveat |
|---|---|---|
| `IoBuildDeviceIoControlRequest` | `kernel/src/ntoskrnl/io/wdm.rs` | Implemented for supported transfer methods; synchronous/thread/IRQL preconditions apply. The builder can return NULL, including when no current scheduler thread is registered. |
| `IoCallDriver` / completion | `io_call_driver`, `io_complete_request` in `wdm.rs` | Implemented WDM path with stack/rundown/completion checks; not full Windows I/O-manager coverage. A builder-created IRP without an event requires a completion routine. |
| `IoCsqInitialize[Ex]`, insert/remove APIs | `io_csq_* ` in `wdm.rs` | Concrete bounded cancel-safe queue implementation, including cancel-spin-lock and driver callbacks. Concurrency and ABI parity still require dedicated tests; do not infer full CSQ compatibility from export resolution. |
| `IoStartPacket` / `IoStartNextPacket` | `wdm.rs` start-I/O queue | Implemented with bounded per-device queueing and optional keyed ordering. Queue exhaustion / missing queue metadata can cause a request to be dropped; stress tests should cover this. |
| `IoGetRequestorSessionId` | `wdm.rs` requestor-session query | Implemented only for tracked IRPs with a nonzero requestor PID resolvable to a process session. Returns `STATUS_NOT_FOUND` for untracked/kernel-owned requests; it does not silently synthesize session 0. |
| `MmLockPagableDataSection` / `MmUnlockPagableImageSection` | `kernel/src/ntoskrnl/mm/runtime.rs` | Present as runtime functions, but pageable-image semantics depend on CognacOS image/page management and are not equivalent to proving Windows loader/page-fault parity. Audit call paths and return values before declaring these fully compatible. |
| Export resolver | `kernel/src/ntoskrnl/mod.rs` and module export tables | An import can resolve to a concrete function without that function implementing every Windows contract. Exports explicitly catalogued as unsupported must remain distinguished from callable implementations. |

## Runtime test changes

The beep test now logs create status, file-object pointer and FsContext; immediately before IOCTL dispatch it logs the IOCTL, input/output lengths, system-buffer pointer, file-object pointer, FsContext, and whether it uses `IoCallDriver`. The IOCTL is built with `IoBuildDeviceIoControlRequest` and dispatched through the WDM path. The builder's NULL result is reported explicitly rather than hidden behind a fake successful dispatch.

The separate PC-speaker diagnostic is still a kernel PIT/port-I/O tone. It is not evidence that Beep.sys generated sound. Only a successful Beep.sys IOCTL plus a driver-originated hardware operation can establish that; QEMU audio capture alone does not attribute the sound source.

## Known limits / next checks

- The CREATE smoke request still calls the registered CREATE dispatch directly; opening the device by name through Object Manager and I/O manager remains a separate integration task.
- Verify the builder's synchronous completion, IOSB publication and IRP lifetime on QEMU. Do not free the builder-owned IRP manually after completion.
- If the builder returns NULL, first check current-thread registration and IRQL, then allocation/device state. Do not rewrite that failure to `STATUS_SUCCESS`.
- Treat `STATUS_NOT_IMPLEMENTED`, `STATUS_PROCEDURE_NOT_FOUND`, `STATUS_NOT_FOUND`, `STATUS_INVALID_PARAMETER`, and `STATUS_INVALID_DEVICE_REQUEST` as distinct diagnostics; they represent different failure classes.

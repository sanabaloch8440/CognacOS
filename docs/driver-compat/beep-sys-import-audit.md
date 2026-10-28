# Windows 11 `beep.sys` Import Compatibility Audit

This audit is based on the uploaded x64 native PE32+ driver `beep.sys` (40,960 bytes). Its PE import directory contains one module, `ntoskrnl.exe`, and 24 named imports. This file is a compatibility worklist, not a claim that the driver is already loadable or that all listed routines are fully implemented.

## Binary facts

- Machine: AMD64 (`0x8664`)
- Optional-header magic: PE32+ (`0x020b`)
- Subsystem: Native (`1`)
- Import module: `ntoskrnl.exe`
- Import count: 24
- The image must still pass the loader's DLL-characteristic, relocation, section-protection, entry-point, and runtime ABI checks.

## Complete import list

### Device objects and I/O queues

| Import | Compatibility contract |
|---|---|
| `IoCreateDevice` | Create a correctly laid-out device object and optional extension |
| `IoDeleteDevice` | Remove the device safely after outstanding activity drains |
| `IoStartPacket` | Queue an IRP and invoke the driver's StartIo routine according to device-queue state |
| `IoStartNextPacket` | Dequeue/start the next IRP, respecting cancellation and queue ordering |
| `IoSetStartIoAttributes` | Persist device StartIo queue policy (including key ordering and boost behavior) |
| `KeRemoveDeviceQueue` | Remove the next entry and update queue busy/hint state |
| `KeRemoveEntryDeviceQueue` | Remove only an entry actually linked to the supplied queue |
| `IoCsqInitialize` | Validate and retain cancel-safe queue callbacks |
| `IoCsqInsertIrp` | Insert IRPs with cancel-lock/callback ordering |
| `IoCsqRemoveNextIrp` | Select and remove the next IRP through the driver's peek callback |
| `IoAcquireCancelSpinLock` | Acquire the global cancel lock and return the previous IRQL |
| `IoReleaseCancelSpinLock` | Release the cancel lock and restore the caller's IRQL |
| `IofCompleteRequest` | Run completion stack processing and release IRP/device rundown correctly |

### IRQL, synchronization and dispatcher events

| Import | Compatibility contract |
|---|---|
| `KfRaiseIrql` | Raise to the requested IRQL and return the prior IRQL |
| `KeLowerIrql` | Restore IRQL to a valid lower level |
| `KeInitializeSpinLock` | Initialize the x64 spin-lock representation |
| `KeAcquireSpinLockRaiseToDpc` | Save prior IRQL, raise to DISPATCH_LEVEL, acquire lock |
| `KeReleaseSpinLock` | Release lock and restore the supplied prior IRQL |
| `KeInitializeEvent` | Initialize a notification or synchronization event |
| `ExAcquireFastMutex` | Acquire FAST_MUTEX with APC-disable and ownership semantics |
| `ExReleaseFastMutex` | Release FAST_MUTEX and restore the caller's APC state |

### Memory, strings and requestor identity

| Import | Compatibility contract |
|---|---|
| `MmLockPagableDataSection` | Pin the relevant pageable data section and return a matching lock handle |
| `MmUnlockPagableImageSection` | Release the matching pageable-section lock exactly once |
| `RtlInitUnicodeString` | Initialize a counted UTF-16 string without copying its buffer |
| `IoGetRequestorSessionId` | Derive the requestor's session from the IRP's owning process; do not fabricate a session for unknown requestors |

## Source-audit findings

The existing kernel already contains implementations or resolver candidates for several imports: device creation/deletion, cancel-safe IRP queue operations, IRP completion, cancel-spin-lock operations, spin locks, IRQL lowering, events, device-queue removal, and Unicode-string initialization.

The following names did not appear in the current named-export resolver dispatch during the initial audit and require implementation or a verified ABI-correct alias:

- `IoStartPacket`
- `IoStartNextPacket`
- `IoSetStartIoAttributes`
- `MmLockPagableDataSection`
- `MmUnlockPagableImageSection`
- `ExAcquireFastMutex`
- `ExReleaseFastMutex`
- `KfRaiseIrql`
- `IoGetRequestorSessionId`

An export-catalogue entry is not proof of implementation. An import must only resolve when the target has the exact x64 calling convention, parameter/return types, and behavior. Unsupported imports must remain unresolved rather than be routed to a generic stub.

## Implementation and acceptance order

1. **ABI gate:** add compile-time layout checks for the `DEVICE_OBJECT`, `DRIVER_OBJECT`, `KDEVICE_QUEUE`, `KDEVICE_QUEUE_ENTRY`, `FAST_MUTEX`, and IRP structures used by these calls.
2. **IRQL and synchronization:** implement/verify `KfRaiseIrql` and FAST_MUTEX ownership, recursion/invalid-release behavior, APC disable accounting, and contention/wakeup behavior.
3. **StartIo queue:** implement the StartIo queue APIs using the actual WDM device queue embedded in `DEVICE_OBJECT`; invoke `DriverObject.DriverStartIo` with the correct IRP and preserve cancel/completion ordering.
4. **Pageable-section lifetime:** track section lock handles/refcounts and ensure the image cannot be paged out while locked. Do not return fake handles.
5. **Requestor session:** connect IRP requestor identity to the process/session model; return the documented failure when requestor identity is unavailable rather than silently claiming session 0.
6. **Import-resolution regression:** add a test that checks all 24 imports against the callable resolver and verifies each resolved address points to the intended implementation.
7. **Guest validation:** load this exact driver through CognacOS's PE loader in QEMU, capture DriverEntry status and device creation, then exercise dispatch and unload paths. A successful PE parse or import bind alone is not a successful driver test.

## Definition of done

- All 24 imports resolve to ABI-correct implementations.
- Driver image mapping, relocation, import binding, and section protections succeed.
- `DriverEntry` returns success and the driver creates its expected device object.
- IRP dispatch/cancel/completion and unload are exercised in the guest without hangs, invalid IRQL transitions, use-after-free, or unresolved imports.
- The CI log records the exact driver hash and each test stage; host-only PE inspection is not treated as guest validation.

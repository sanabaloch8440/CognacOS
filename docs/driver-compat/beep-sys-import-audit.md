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

The initial resolver gaps below have been reduced by subsequent commits. The StartIo entries are now registered to callable routines; pageable-section locking and requestor-session lookup remain outstanding. FAST_MUTEX and IRQL implementations exist but still require full runtime/guest validation:

- `MmLockPagableDataSection`
- `MmUnlockPagableImageSection`
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


## StartIo implementation update (2026-10-28)

The three StartIo imports are now registered to the WDM implementation in `kernel/src/ntoskrnl/io/wdm.rs`:

- `IoStartPacket` tracks the active IRP and queues subsequent packets; keyed mode inserts by ascending sort key.
- `IoStartNextPacket` promotes the next queued IRP and invokes `DriverStartIo` outside the queue lock.
- `IoSetStartIoAttributes` records keyed-device and boosted-I/O policy for the device.

The queue table is currently bounded (64 queued IRPs per device, with the existing 32-device limit). This first pass does not yet provide complete Windows cancel-lock ordering, canceled-queued-IRP draining, or I/O priority boosting; those must be added before claiming full StartIo compatibility. The CI build and guest driver test are still required.


## DEVICE_OBJECT / KDEVICE_QUEUE ABI correction (2026-10-28)

The x64 public WDM layout now embeds a typed `KDeviceQueue` at `DEVICE_OBJECT+0x98`, rather than placing it at `+0xA0` behind a non-WDK `AlignmentRequirement` field. The embedded queue is asserted to be `0x28` bytes; `KDeviceQueueEntry` is asserted to be `0x18` bytes. The downstream `Dpc`, active-thread, security-descriptor, device-lock, and extension offsets were shifted accordingly, and the structure retains the SDK-sized `0x150` total size. These compile-time layout assertions must pass before StartIo queue behavior can be considered ABI-valid; queue semantics and guest runtime validation remain separate requirements.


## Remaining import implementation pass (2026-10-28)

The remaining three names are now registered to callable routines:

- `MmLockPagableDataSection` / `MmUnlockPagableImageSection`: paired calls use stable opaque handles and reference counts. **Important limitation:** driver-image paging/page-out is not implemented, and the loader does not yet retain PE section-range metadata for these calls. The current 64-KiB address bucket is temporary bookkeeping, not exact Windows image-section identity or a real page-residency pin. This is not sufficient evidence for general third-party driver compatibility.
- `IoGetRequestorSessionId`: uses the I/O manager's tracked IRP owner PID and registered process session metadata. Unknown/untracked IRPs fail rather than receiving a fabricated session ID. CognacOS currently models registered processes in session 0; multiple Windows sessions are not implemented.

The resolver now binds all three imports to their respective implementations. Required next steps are replacing the pageable-section bucket with loader-owned image-section records and actual paging pins, recording requestor identity for IRPs entering from every I/O path, then adding focused ABI tests and QEMU driver tests. All 24 import names resolving is still not equivalent to `beep.sys` successfully loading or behaving correctly in the guest.

# WDM I/O manager source layout

`../wdm.rs` is the module entry point. The implementation is divided into functionality-oriented source fragments in this directory and included in the same Rust module namespace. This preserves existing private ABI types, locks, and helper visibility while making each subsystem independently navigable.

| File | Responsibility |
|---|---|
| `core.rs` | ABI types, constants, stable demand-grown nonpaged device registry, and IRP tracking primitives |
| `device_lifecycle.rs` | Device references, remove rundown, per-device power capabilities, and PnP state transitions |
| `pnp.rs` | Resource arbitration, device relations, bus enumeration and PnP manager |
| `device_objects.rs` | Per-device StartIo queues, nonpaged device creation/deletion, and complete driver-device unload draining |
| `irp_management.rs` | IRP allocation, dispatch, IOCTL builders and completion |
| `device_stack.rs` | Device attachment, stack-location helpers and IRP reuse |
| `filesystem.rs` | Filesystem registration, VPB mount/verify/dismount |
| `namespace.rs` | Object-manager symbolic-link namespace |
| `cancel.rs` | Cancel-safe queues, cancel-spin-lock synchronization, and IRP cancellation |
| `mdl.rs` | MDL allocation and release |
| `self_test.rs` | Runtime WDM rundown self-test |
| `tests.rs` | WDM ABI and behavior unit tests |
| `requestor_session.rs` | Requestor session ID query export |

These are `include!` fragments, not independent Rust modules. Moving to true submodules would require a separate visibility/API refactor and is deliberately not mixed into this source-organization change.

Device records keep the first 32 slots embedded for early boot, then allocate one page-backed nonpaged record at a time at PASSIVE_LEVEL. Overflow records are linked without moving existing DEVICE_OBJECT addresses, participate in the same reference/remove-lock/IRP/power/Ob rundown checks, and are reclaimed only after final deletion. StartIo queue state and power-capability metadata live in the device record, so neither imposes a separate 32/64-device ceiling. Driver unload walks the complete intrusive DRIVER_OBJECT device chain instead of truncating at bootstrap capacity. The driver-facing `IoSetCancelRoutine` follows the WDK caller-held cancel-spin-lock contract; kernel-owned paths use a synchronized helper, and cancel callbacks release the lock using `Irp->CancelIrql`.

The file-service layer is not modified here. Its migration to FILE_OBJECT and IRP-based Windows-style create/read/cleanup/close flows is the next step.

## Filesystem boundary audit

The WDM fragment `filesystem.rs` is I/O-manager infrastructure, not a filesystem implementation. It owns the generic registered-filesystem device list, registration-change notifications, referenced snapshots, mount/verify/dismount IRP coordination, VPB lifetime/commit rules, and dispatch to registered filesystem drivers. It must not parse FAT/exFAT/NTFS metadata, own filesystem-specific block adapters, or implement file/directory semantics.

Filesystem-specific code belongs under `drivers/filesystem/*` (currently `drivers/fat32`). The FAT32 driver owns its `BlockDevice` adapter and filesystem engine calls; it reaches storage through the generic `io/block_io.rs` IRP helper. The I/O manager helper constructs and submits generic IRPs only and has no dependency on `cognacos_fat32`.

DMA ownership is similarly split: `io/dma.rs` retains Windows DMA_ADAPTER / DMA_OPERATIONS ABI, adapter lifetime, MDL/SG bookkeeping, and generic IOMMU mapping contracts; Intel VT-d register programming and platform-specific translation machinery live in `Hal/vtd.rs`. The IOMMU module re-exports the HAL backend for compatibility with existing internal call sites.

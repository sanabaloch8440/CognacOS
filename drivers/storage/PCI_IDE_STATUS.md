# PCI IDE / ATA Port QEMU Readiness

**Assessment date:** 2026-11-03  
**Branch:** `storage/pio-first`  
**Pull request:** [#32](https://github.com/sanabaloch8440/CognacOS/pull/32)

## Current implementation

The controller-to-port PIO request path is now connected using a CognacOS-owned, versioned miniport interface delivered through an internal device-control request. This is Windows-style WDM layering, not a claim of binary compatibility with Microsoft's private ATA miniport ABI.

Implemented:
- `ataport.sys` attaches an FDO to IDE controller PDOs, forwards PnP requests, tracks start/stop/remove state, and owns the buffered READ/WRITE/FLUSH dispatch path.
- `ataport.sys` validates request sizes, sector alignment, transfer bounds, operation payloads and kernel system buffers before calling a registered miniport.
- The miniport registration/unregistration IOCTLs are implemented; registration is rejected before the port FDO has successfully started.
- Active request rundown prevents miniport teardown while a synchronous PIO request is executing.
- `pciide.sys` consumes PnP-translated resources, probes IDENTIFY, retains channel/device state and registers its PIO submit callback with the lower `ataport.sys` FDO through the `pciidex.sys` forwarding layer.
- Controller PIO commands are serialized. Read/write supports bounded multi-sector LBA28 transfers (up to 256 sectors); FLUSH is routed to the ATA channel.
- The port/miniport interface uses a CognacOS-owned contract. DMA and asynchronous queueing are intentionally not advertised in version 1.
- The UEFI/QEMU workflow now attaches a separate 8 MiB primary-slave IDE test disk. On discovery, `pciide.sys` snapshots two sectors, writes a deterministic pattern through the ataport request path, flushes, reads and verifies the data, then restores the original sectors.

## Validation status

<badge color="warning">QEMU validation pending</badge>

Latest UEFI build/boot workflow: [run #41627107426](https://github.com/sanabaloch8440/CognacOS/actions/runs/41627107426).

The QEMU test is designed to fail the IDE controller start path if the read/write/flush/restore round-trip fails. It also emits one success line for the workflow assertion. Do not mark the test as passed until the run completes and the serial log contains:

`pciide: ATA PIO read/write/flush round-trip passed`

## Remaining work after the controller PIO test

1. Create disk child PDOs and publish them through BusRelations.
2. Add disk geometry/capacity and stable device identity reporting.
3. Connect disk class/partition manager and FAT32 file I/O to these devices; the current QEMU test is controller/port/miniport PIO validation, not a filesystem mount/read/write test.
4. Add PASSIVE_LEVEL work-item scheduling and explicit cancellation/timeout handling before expanding beyond synchronous PIO.
5. Add robust per-controller miniport enumeration and resource-claim/mapping integration.
6. Keep DMA disabled until HAL DMA mapping, IOMMU, interrupt and teardown paths are validated.

A successful PE32+ build proves image compilation only. A successful QEMU PIO round-trip proves the tested controller/port/miniport path only; it does not establish full Windows 11 storage-stack or filesystem compatibility.

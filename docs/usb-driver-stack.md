# CognacOS USB driver stack

## Intended Windows 11 x64 boundaries

Microsoft's USB 3.x model separates three layers: the xHCI host-controller driver (usbxhci.sys), the USB host-controller extension (UCX), and the SuperSpeed hub driver (usbhub3.sys). The hub layer owns hub/port state machines, enumeration policy and child-device creation; xHCI owns controller registers, DMA rings, command/transfer scheduling and hardware events. HIDClass/KbdHid own HID reports and keyboard semantics.

CognacOS follows that separation. The selected implementation is `usbhub3.sys`, with support for both USB 2 hub descriptors (type `0x29`) and SuperSpeed hub descriptors (type `0x2a`) when the controller is xHCI. This does not mean the SuperSpeed hub protocol itself carries low/full/high-speed devices: USB 3 hubs may expose separate USB 2 and SuperSpeed hub functions. The legacy `usbhub.sys` belongs to the older USBPORT/EHCI/OHCI/UHCI stack; CognacOS is targeting the xHCI stack, so we are not creating a duplicate legacy hub driver.

| Component | Owns | Current status |
|---|---|---|
| drivers/usbxhci | xHCI capabilities, TRBs, command/event rings, DMA resource lifecycle, controller startup/stop, event dispatch primitives | Adds a bounded transfer engine: per-ring-slot ownership, Link TRB publication, doorbell ordering, transfer-event correlation and DPC completion. Live DriverEntry/URB queue wiring and QEMU proof remain incomplete |
| drivers/usbhub3 | USB2/SuperSpeed hub descriptor validation, device/configuration descriptor validation, hub port state and generation tracking, enumeration policy | Adds passive-level reset/descriptor enumeration coordination and child-PDO create/remove callbacks with stale-generation and reference-rundown checks. Kernel PDO callbacks are still an adapter contract, not yet bound to the live PnP manager |
| drivers/ucx01020 | UCX 1.2.0-compatible controller/root-hub handles, endpoint state, bounded asynchronous request tracking, completion correlation, cancellation and generation checks | Adds WDM major-function routing, PnP device-node rundown, URB-header validation, D0 power gating, root-hub class requests/change bitmaps and bulk-stream object lifetimes. These contracts still need binding to real kernel DEVICE_OBJECT/IRP, DMA and DPC services |
| drivers/hidclass, drivers/kbdhid, drivers/kbdclass | HID collection/report processing and keyboard input delivery | Separate foundations exist; USB endpoint/transfer completion is not yet connected end to end |

## Safety and completion criteria

- Device-visible addresses must come from the device-scoped DMA/IOMMU service; no physical-address fallback is allowed.
- DMA spans must be aligned and within the device address limit; translation invalidation and teardown must complete before pages can be reused.
- Interrupt resources must be assigned by PnP/HAL, then route to an ISR and deferred event-ring processing. A host unit test does not prove this path.
- The hub state machine increments a port generation when removal/failure begins so late completions can be rejected. The kernel must still enforce rundown, cancel pending URBs and wait for hardware/DPC quiescence before freeing resources.
- Enumeration acceptance requires live port-change events, port reset, the initial eight-byte device descriptor at address zero, Enable Slot/Address Device with the discovered EP0 packet size, the full device descriptor, two-stage configuration descriptor retrieval, endpoint/configuration programming, and child PDO publication.
- Keyboard acceptance requires a real HID interrupt-IN transfer, transfer-event completion, report delivery through HIDClass/KbdHid/KbdClass and an OS-visible key event.
- Power/removal acceptance requires QEMU tests for D0 exit/entry, disconnect during an outstanding transfer, controller reset, IOMMU unmap/invalidation, interrupt synchronization and no stale DMA/interrupt access. Endpoint abort uses endpoint-scoped quiescence; controller reset/removal uses a separate controller-wide quiescence proof.

## Current assessment

**usbhub3.sys is not ready for real devices.** Its library tests USB2/SuperSpeed hub descriptor bounds, standard device/configuration descriptor bounds and hub-port lifecycle policy. The native image is a guarded DriverEntry contract, not yet a functional hub driver. The kernel does not yet provide the complete PnP, control-transfer, child-PDO and remove-rundown contract required to make DriverEntry operational. The driver deliberately rejects a missing/incomplete extension instead of silently claiming successful hub startup.

Likewise, **usbxhci.sys is not yet end-to-end ready**. It contains controller primitives and a live-driver adapter, but the required kernel DMA/interrupt/PnP services and full completion-driven enumeration/keyboard path have not yet been validated together under QEMU.

## Microsoft references

- [USB host-side drivers in Windows](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/usb-3-0-driver-stack-architecture)
- [Architecture of USB Host Controller Extension (UCX)](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/get-started-with-host-controller-driver-development)
- [USB Device Class Drivers Included in Windows](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/supported-usb-classes)

## UCX implementation boundary

The new `drivers/ucx01020` crate implements the first native UCX-equivalent core: controller and root-hub handles, endpoint objects, asynchronous request records, exactly-once completion correlation, endpoint abort/rundown gating, and stale-generation rejection. It intentionally does not claim to be a Microsoft-compatible WDF/UCX binary. Microsoft's public host-controller documentation identifies the inbox extension as `Ucx01000.sys`; the `ucx01020` name here is CognacOS's requested 1.2.0 interface-version target, not a claim about Microsoft's shipped filename or private implementation.

The remaining integration work is substantial: instantiate real kernel DEVICE_OBJECTs and PnP device nodes; route the WDM dispatch table through the I/O manager; validate complete function-specific URB layouts and transfer buffers; publish UCX controller/root-hub/device/endpoint objects from PnP callbacks; connect the xHCI TransferEngine to the live DriverEntry runtime and ISR/DPC event drain; connect root-hub notifications to pending interrupt URBs; and bind hub PDO callbacks to the kernel PnP manager. The current additions are testable no_std subsystem contracts, not a claim that real USB hardware enumeration or a loadable Windows-compatible driver is complete.

Public architecture references:
- https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/developing-windows-drivers-for-usb-host-controllers
- https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/ucx-objects-and-handles-used-by-host-controller-driver
- https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/configuring-usb-endpoints-in-a-host-controller-driver

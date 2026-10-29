# Keyboard input stack

The intended Windows-like order is **hardware/transport → HIDClass → KbdHid → KbdClass → Win32 input dispatch/layout translation**. `win32kbase.sys`/`win32k.sys` are part of the Windows GUI/input subsystem, not a direct hardware-driver stage. The US layout is conventionally named `kbdus.dll`; the US-International layout is a separate layout. CognacOS currently implements a tested portable US-layout translation core under `drivers/kbdus`, but has not yet implemented the Windows `KbdLayerDescriptor`/KBDTABLES binary contract or wired layout events into a working Win32 input queue.


CognacOS now has initial, separately buildable Rust driver components for the Windows-style keyboard stack:

- **kbdclass.sys** — bounded, fixed-capacity keyboard packet queue, connect/disconnect state, exclusive open semantics, and KEYBOARD_INPUT_DATA-shaped packets.
- **hidclass.sys** — bounded HID short-item report-descriptor parser and top-level keyboard collection discovery.
- **kbdhid.sys** — boot-keyboard report differencing, common HID usage to scan-code mapping, modifier make/break tracking, and HID rollover handling.
- **kbdus layout core** — tested Set-1 scan-code to virtual-key mapping, US punctuation/shifted digits, Shift/Caps Lock casing, extended navigation keys, and key-up suppression of text; not yet a loadable Windows-compatible `kbdus.dll`.
- **i8042prt.sys** — PS/2 port protocol helpers with bounded controller polling, ACK/RESEND handling, and scan-code Set 1 decoding. HAL now exposes privileged x86 port-I/O and volatile MMIO primitives behind unsafe APIs.
- **usbxhci.sys** — xHCI core with checked capabilities, Link-TRB/cycle handling, USB descriptor parsing, enumeration-state validation, bounded controller startup, a HAL-backed platform adapter for the versioned DMA/interrupt service table, allocation/ERST initialization with rollback, live DMA event-ring draining with ERDP updates, bounded command-ring submission with Link-TRB cycle rollover and Doorbell 0 notification, and safe stop-before-free rundown. CI builds a separate native PE32+ image and runs host-side unit tests.

## Architecture

For a HID keyboard, the intended path is:

USB/Bluetooth transport -> HIDClass -> KbdHid -> KbdClass -> input consumer

For a legacy PS/2 keyboard, the intended path is:

i8042 controller -> i8042prt -> KbdClass -> input consumer

This mirrors the broad Windows class/port/client layering. Windows' kbdclass.sys provides the common keyboard class service; key-layout translation is normally performed by the input subsystem/user-mode layout machinery, not by the class driver itself. Windows' kbdhid.sys is a HID client mapper and relies on HIDClass and a transport/minidriver beneath it.

## Validation

GitHub Actions host-tests queue bounds and exclusive ownership, HID descriptor parsing, boot-report transitions/rollover, PS/2 scan-code decoding and bounded polling, PCI ECAM/BAR decoding, HAL device-service contracts, and xHCI capability/TRB/ring/ERST/controller-start/descriptor/enumeration primitives. It builds kbdclass, hidclass, kbdhid, i8042prt, and usbxhci as separate AMD64 PE32+ images with the repository's current driver-image pipeline.

## Device-power and S4 lifetime hardening milestone

The power manager now checks D-state capability across the bounded attached-device stack before accepting device power queries/transitions. Wait/wake requests similarly require wake capability at every attached layer; cyclic or overlong attachment chains fail closed. This is capability validation and ordering protection, not a substitute for each bus/port/driver's hardware D0/D3 sequencing or propagation through a separate parent-bus relationship. A regression test covers multi-layer support, a layer that does not support wake, and a cyclic stack.

The S4 image engine's platform callback descriptor is now copied into immutable kernel-owned storage, preventing use-after-return if the registration caller supplied a temporary descriptor. Platform context, storage, and callbacks must still remain valid for the kernel lifetime. The hibernation codec supports bounded checksummed images and commit-last metadata, but the boot path does not register a production storage/memory/CPU restore backend; real S4 persistence/resume is therefore still unavailable and fails closed.

## DMA lifetime hardening milestone

The kernel IOMMU registry now copies the registered callback table into immutable kernel-owned storage instead of retaining a pointer to the caller's table. The backend context itself must still remain valid for the kernel lifetime, and registration remains one-shot. Common-buffer allocation now checks for a complete IOMMU backend before taking contiguous physical pages, avoiding a page leak when DMA translation is unavailable. A successful map that returns malformed logical-address/cookie values attempts rollback before failing closed.

This hardens failure paths; it does **not** add a hardware VT-d/AMD-IOMMU backend, enable DMA for xHCI, or prove IOTLB invalidation/cache coherency on real hardware. The device-service table remains intentionally without DMA and interrupt callbacks until those backends and teardown synchronization are wired.

## Explicit limitations

These are driver-core foundations, **not yet end-to-end working hardware drivers**. The current DriverEntry routines are minimal loader stubs. The xHCI core now contains a generic runtime orchestration path that can allocate four DMA regions through an injected platform adapter, populate the ERST, register an interrupt before controller startup, consume live event-ring entries, and stop the controller before freeing DMA memory. A concrete adapter now translates the HAL DeviceServiceTable into the xHCI platform interface, validates MMIO bounds/alignment, requests contiguous coherent DMA with explicit flags, and connects interrupt callbacks. This is integration glue, not a finished hardware backend: the kernel still does not populate the table with real DMA/IOMMU and interrupt implementations, and DriverEntry does not invoke this runtime path. Real PCI scanning/BAR mapping, IOMMU-backed DMA, an ISR/DPC implementation, command/doorbell and port-reset/slot/address/configure commands, control and interrupt transfers, HID report delivery, PnP/IRP registration, PS/2 IRQ/DPC packet delivery, keyboard input syscall/console delivery, and registry-based service/INF installation remain unwired. HID USB/Bluetooth transport, actual HIDClass IRP/IOCTL forwarding, PnP/device-object registration, interrupt/DPC integration, safe PS/2 interrupt registration and scan delivery, keyboard input syscall/console delivery, and registry-based service/INF installation remain unwired. The kernel HAL now also provides bounded/alignment-checked 32-bit MMIO accessors and explicit acquire/release/sequence-cst device-ordering barriers, and defines a separately versioned extended service contract for explicit uncached/write-combined device mappings and interrupt synchronization. This follows the Windows driver model's separation of device-resource mapping, DMA adapter operations, and ISR synchronization; it is not a claim of binary-compatible Windows HAL exports. The kernel still needs concrete mapping and synchronization callbacks. PE builds and host tests do not establish keyboard operation in QEMU or on physical hardware. Do not load these drivers against hardware until the missing runtime plumbing is implemented and tested.


The command-ring API submits bounded command TRBs and rings Doorbell 0 after publishing the TRB with release ordering. It reports submission only; command completion events must still be consumed and correlated by the controller owner. It does not yet implement slot enable/address-device/configure-endpoint sequencing, USB control/interrupt transfers, or HID report delivery.

The next controller-core milestone adds a fixed-capacity command completion tracker keyed by the command TRB DMA address, with explicit errors for unknown or malformed completion events. It also adds validated builders for Enable Slot, Address Device, and Configure Endpoint command TRBs. These are primitives only: callers must track commands before processing interrupts, validate completion codes, allocate/populate input contexts, and perform the required USB enumeration state transitions.


## Power-management readiness (audit, 2026-10-29)

Microsoft's WDM model requires PnP capability reporting, power-IRP dispatch/forwarding, D-state transitions, and wait/wake propagation through the device stack. CognacOS has power-IRP lifecycle tracking and an S4 codec, but USB hub/port policy, a real xHCI PnP/IRP DriverEntry path, and PS/2 IRQ/DPC delivery remain unwired. It therefore cannot yet guarantee end-to-end power/resume/wake behavior for USB 3.x devices or USB/PS/2 keyboards.

A conservative capability registry defaults unregistered devices to D0/D3 and rejects wait/wake unless the device explicitly registers verified support. D1/D2 and wake are opt-in; drivers must still implement hardware changes in IRP_MJ_POWER. xHCI command submission now reserves completion tracking before ringing the doorbell, and live event routing correlates command completions while surfacing transfer and port-status events. These are prerequisites, not end-to-end hardware support.


## Device power transition implementation (2026-10-29)

- WDM device creation now registers conservative D0/D3 capabilities; successful PnP remove/surprise-remove and final object deletion clear registry entries.
- The xHCI core provides per-port U3 suspend/U0 resume primitives and controller D0-exit/D0-entry routines that mask interrupts, halt without freeing DMA or resetting slots, snapshot controller registers, and restore them before re-enabling interrupts. Callers must quiesce transfers before controller halt and propagate wait/wake through the USB parent stack.
- The PS/2 core provides bounded 8042 self-test/configuration/port-test startup and a lifecycle owner that masks and synchronizes IRQ delivery before suspend, disables scanning, and reinitializes before restoring IRQ delivery on resume.
- CI unit tests cover simulated scan-code delivery before/after PS/2 power transitions and xHCI port state changes. These are deterministic driver tests, **not** proof of live USB HID, PS/2 IRQ/DPC delivery, or QEMU keyboard input across real power transitions.


The xHCI core also includes a wait/wake orchestration contract that requires device, hub, controller, and connected-port wake capability before arming. It enables wake in controller-to-device order and rolls back in reverse order on failure; cancellation attempts all teardown steps even if one layer fails. The backend callbacks are contracts only until real USB control-transfer and hub/host-controller wake handlers are wired.


## Windows 11 x64 alignment and implementation boundary

This design follows Microsoft's documented **driver-role and device-stack contracts**, not a claim that CognacOS can load Microsoft's proprietary driver binaries. The current target is an NT-style WDM contract with distinct kernel executive, HAL services, and separately built drivers:

- **Kernel / NT executive:** owns PnP device objects, IRP dispatch/completion and cancellation, resource arbitration, interrupt/DPC scheduling, DMA/IOMMU policy, and driver lifetime/rundown.
- **HAL:** owns architecture-specific port/MMIO access, interrupt routing/synchronization, and the platform DMA backend contract. Driver crates must not reach into kernel-private modules or implement a second HAL.
- **Drivers:** PCI, xHCI, HIDClass, KbdHid, KbdClass, and i8042prt remain separate no_std driver components. The xHCI core consumes explicit platform services rather than bypassing HAL.

Microsoft's Windows driver documentation describes DriverEntry, AddDevice, dispatch, unload, ISR/DPC and DMA routines as distinct driver responsibilities; PnP IRPs must be propagated correctly through the stack. The documented USB host stack also separates the host-controller driver from the USB host-controller extension and hub/device stacks. References: [standard driver routines](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/introduction-to-standard-driver-routines), [passing PnP IRPs down the stack](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/passing-pnp-irps-down-the-device-stack), [USB host-controller driver architecture](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/developing-windows-drivers-for-usb-host-controllers), and [wait/wake propagation through device stacks](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/understanding-the-path-of-wait-wake-irps-through-a-device-tree).

The latest implementation adds two **tested building blocks**: read-only ECAM discovery of xHCI PCI functions (including multifunction traversal and assigned 32/64-bit BAR decoding), and bounded xHCI interrupt-transfer TRB construction/completion tracking keyed by TRB DMA address. These additions deliberately do not enable PCI bus mastering, map BARs, allocate DMA, or register interrupts on their own.

Still not implemented end-to-end: firmware ACPI MCFG-to-ECAM mapping in the live PCI driver path; kernel resource arbitration and PCI enable/disable; a real IOMMU-backed HAL DMA implementation; kernel-provided ISR/DPC registration; xHCI DriverEntry/PnP/IRP dispatch; port reset, slot/address/configure sequencing driven by actual completions; control transfers for descriptors and SET_PROTOCOL/SET_CONFIGURATION; endpoint context/ring setup and doorbell scheduling; transfer-event delivery into HIDClass/KbdHid/KbdClass; PS/2 IRQ/DPC packet delivery; and a QEMU test asserting real keyboard packets reach the input consumer. Until those are wired and QEMU-tested, this remains **controller/driver-core work, not Windows 11-compatible hardware support**.


## Kernel-to-HAL service publication milestone

The kernel now publishes a versioned `HalServiceTable` for driver adapters, backed by existing privileged port I/O, already-mapped MMIO access, TSC/monotonic counter query, and validated fixed-vector IPI services. IRQL mutation is intentionally left unavailable because the executive must own per-thread IRQL state and restoration. The kernel also publishes a versioned `DeviceServiceTable`, but DMA and interrupt/DPC entries remain `None`; xHCI must reject it rather than fabricate device addresses or run completion callbacks at the wrong IRQL. This is the first concrete kernel-to-HAL publication boundary, not yet a working DMA/interrupt backend or a live xHCI startup path. Next: wire memory-manager-owned contiguous/pinned DMA with IOMMU isolation, then kernel interrupt registration tied to IDT/APIC routing and a real DPC queue with rundown before enabling xHCI.

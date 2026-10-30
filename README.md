<p align="center">
  <img src="assets/cognacos-logo.svg" alt="CognacOS" width="360">
</p>

# CognacOS

**CognacOS is an experimental Rust-first x86-64 operating system with a UEFI boot path and NT-inspired kernel interfaces.** Its long-term goal is Windows x64 driver compatibility; that goal is not yet achieved. It is not a Windows 11 replacement and does not claim general WDK or third-party driver compatibility.

[![Build and QEMU boot test](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml/badge.svg)](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml)

- **Language:** Rust, with a no_std kernel and UEFI loader
- **Architecture:** x86-64 / AMD64
- **Boot image:** UEFI loader and PE32+ kernel
- **License:** GPL-2.0-or-later

## Documentation

- [Current roadmap](roadmap.md) — remaining work and acceptance criteria
- [Documentation website](https://umarfarooq7869.github.io/CognacOS/)
- [Windows 11 x64 Ke compatibility comparison](WINDOWS11_COGNACOS_NTOSKRNL_KE_COMPARISON.md)
- [Windows 11 x64 Se compatibility comparison](WINDOWS11_COGNACOS_NTOSKRNL_SE_COMPARISON.md)
- [Boot security guide](docs/boot-security.md)
- [Keyboard driver stack](docs/keyboard-driver-stack.md) — kbdclass, kbdhid, HIDClass, and i8042prt
- [USB driver stack](docs/usb-driver-stack.md) — Windows-style xHCI, UCX-equivalent, hub, and HID boundaries

## Implemented foundations

- UEFI boot and native AMD64 PE32+ kernel image validation; fail-closed SHA-256 and Ed25519 kernel signature verification.
- Rust ntoskrnl/HAL structure, x86-64 interrupt and processor setup, SMP startup, scheduler foundations, and kernel diagnostics.
- Process address spaces, checked user-memory copies, bounded fault fixups, process-scoped CPL3 fault termination, and foundational TLB synchronization.
- Initial NT-style Object Manager, security, WDM/IRP rundown, registry/REGF, ext-family filesystem, and PE32+ loader components.
- A shared no_std NT ABI crate (shared/nt-abi) and Rust NTDLL native-service paths for selected VM, ETW, ALPC/section, registry, and process operations.
- GitHub Actions build, ABI/image checks, host-side tests, and OVMF/QEMU boot/SMP validation paths.

These are implementation foundations, not declarations of full Windows 11 compatibility. A feature is validated only when its relevant tests pass on the current revision.

## Highest-priority remaining work

1. **Restore green CI:** diagnose and fix the current QEMU boot-smoke failure, then pass the full build, NTDLL export/ABI checks, QEMU boot, and SMP matrix on the latest commit.
2. **Prove memory isolation:** pass adversarial CPL3 and same-VA cross-process tests; stress address-space teardown against faults, copies, pinning, and unmapping; implement user working-set trimming and pagefile-backed paging.
3. **Finish safe I/O rundown:** exercise cancel/completion/unload races in-kernel under QEMU SMP and fault injection before expanding storage support.
4. **Connect the native loader:** wire the bounded VFS-to-PE dependency transaction to NTDLL through safe process-address-space and section/map/unmap services; test dependency ordering, rollback, initialization, and unload.
5. **Complete process/object semantics:** move to process-owned handle tables and finish token lifecycle, user APC/SEH delivery, and scheduler-backed waits.
6. **Advance storage and registry:** integrate disk-backed VFS/partitions and PCI/AHCI/NVMe with a safe DMA/IOMMU backend; add crash-safe writable ext filesystem operations and remaining REGF log/security semantics.
7. **Validate Windows ABI claims:** automate signature/layout/export checks, add targeted WDK-style tests, and test representative drivers only when their dependencies and safety prerequisites exist.

## Repository map

- kernel/src/ntoskrnl/ — NT executive, memory manager, I/O manager, process and object services
- kernel/src/hal/ — hardware abstraction layer
- shared/nt-abi/ — shared NT-style Rust ABI definitions
- ntdll/src/ — Rust NTDLL exports and native runtime
- dlls/msvcrt/ — Rust PE32+ C runtime DLL with 1,330 pinned Windows 11 x64 exports (stub bodies)
- drivers/ext4/ — native Rust ext-family filesystem driver
- drivers/kbdclass/, drivers/hidclass/, drivers/kbdhid/, drivers/i8042prt/ — keyboard stack cores
- drivers/usbxhci/ — xHCI startup/event-ring core with a HAL-backed DMA/interrupt adapter (kernel DMA/IOMMU/interrupt callbacks remain incomplete)
- .github/workflows/ — build, host-test, and emulator validation
- docs/ — public documentation

## Development rules

- Use documented Windows 11 x64 signatures and layouts only when backed by a pinned reference; CognacOS-private service IDs are not Windows syscall ordinals.
- Keep unsupported paths explicit and fail closed.
- Mark work complete only when implementation and relevant validation exist.
- Keep detailed checklists in the roadmap and routine inventories in subsystem comparison documents.

## License

GNU GPL v2.0 or (at your option) any later version. See [LICENSE](LICENSE); third-party components retain their own notices and licenses.

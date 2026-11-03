<p align="center">
  <img src="assets/cognacos-logo.svg" alt="CognacOS" width="360">
</p>

# CognacOS

**CognacOS is an experimental Rust-first x86-64 operating system with a UEFI boot path and NT-inspired kernel interfaces.** Its long-term goal is Windows x64 driver compatibility; that goal is not yet achieved. It is not a Windows 11 replacement and does not claim general WDK or third-party driver compatibility.

[![Build and QEMU boot test](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml/badge.svg)](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml)

- **Language:** Rust, with a no_std kernel and UEFI loader
- **Architecture:** x86-64 / AMD64
- **Boot image:** UEFI loader and PE32+ kernel
- **License:** MIT OR Apache-2.0

## Documentation



## Implemented foundations

- UEFI boot and native AMD64 PE32+ kernel image validation; fail-closed SHA-256 and Ed25519 kernel signature verification.
- Rust ntoskrnl/HAL structure, x86-64 interrupt and processor setup, SMP startup, scheduler foundations, and kernel diagnostics.
- Process address spaces, checked user-memory copies, bounded fault fixups, process-scoped CPL3 fault termination, and foundational TLB synchronization.
- Initial NT-style Object Manager, security, WDM/IRP rundown, registry/REGF, ext-family filesystem, and PE32+ loader components.
- A shared no_std NT ABI crate (shared/nt-abi) and Rust NTDLL native-service paths for selected VM, ETW, ALPC/section, registry, and process operations.
- GitHub Actions build, ABI/image checks, host-side tests, and OVMF/QEMU boot/SMP validation paths.

## Repository map

- ntoskrnl/src/kernel/ — NT executive, memory manager, I/O manager, process and object services
- Hal/ — hardware abstraction layer
- shared/nt-abi/ — shared NT-style Rust ABI definitions
- ntdll/src/ — Rust NTDLL exports and native runtime
- dlls/msvcrt/ — Rust PE32+ C runtime DLL with 1,330 pinned Windows 11 x64 exports, including working x64 memory/string primitives and explicit remaining stubs
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

Dual-licensed under either the [MIT License](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), at your option. See [LICENSE](LICENSE) and [LICENSE-POLICY.md](LICENSE-POLICY.md). Third-party components retain their own licenses and notices.

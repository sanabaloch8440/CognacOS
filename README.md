# RdotOS

**RdotOS is an experimental, Rust-first x86-64 operating system built around a UEFI boot path and NT-inspired kernel interfaces.** Its long-term goal is to provide a native platform capable of hosting Windows-style x64 drivers. That compatibility is a work in progress; RdotOS is not currently a Windows 11 replacement and does not claim general Windows driver or WDK binary compatibility.

[![Build and QEMU boot test](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml/badge.svg)](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml)

- **Language:** Rust (no_std kernel and Rust UEFI loader)
- **Architecture:** x86-64
- **Boot mode:** UEFI
- **License:** GPL-2.0-or-later — see [LICENSE](LICENSE)
- **Project status:** Experimental; implementation and hardware support are incomplete

See the [implementation roadmap](roadmap.md) for feature status, known gaps, and planned work.

## Project goals

- Build a maintainable operating-system kernel in Rust.
- Establish explicit boundaries between architecture code, the HAL, memory management, the NT-inspired executive, and device I/O.
- Develop memory isolation, executable-image loading, scheduling, and driver-facing interfaces incrementally.
- Validate changes through host-side tests and automated OVMF/QEMU boot tests.
- Pursue Windows x64 driver compatibility through real ABI and semantic implementations, not export-name lists or placeholder functions.

## Current implementation

The repository contains working foundations in several areas. “Implemented” here means code and tests exist; it does not imply production readiness or complete Windows compatibility.

| Area | Current state |
| --- | --- |
| UEFI boot | Rust EFI application loads a bounded ELF64 x86-64 kernel image, checks a build-pinned SHA-256 digest, exits boot services, and transfers control through versioned boot information. |
| Kernel bootstrap | Freestanding Rust kernel, kernel-owned GDT/IDT, exception diagnostics, COM1 logging, CR3 activation, and physical-page allocation. |
| Memory protection | 4 KiB kernel-section permissions, CR0.WP, NX where supported, stack guard page, isolated user address spaces, checked user-copy helpers, and W^X checks in implemented mapping paths. |
| ACPI and HAL | RSDP/RSDT/XSDT validation, MADT parsing, CPU-local records, APIC-ID registry, port-I/O primitives, MMIO bounds checks, and IRQ-save locks. |
| SMP | AP startup trampoline and bounded startup coordination exist; multi-CPU startup and scheduling remain experimental and require broader validation. |
| Executable loading | Bounded PE32+ AMD64 loading, relocations, normal/delay imports, registered exports, API-set aliases, TLS setup/callbacks, and basic unwind metadata registration. |
| User mode | Ring-3 transition and a small RdotOS-native syscall/process lifecycle test path. This is not the Windows NT syscall ABI. |
| Kernel services | Initial scheduler policy, typed handles/access checks, dispatcher primitives, and device/IRP request lifecycle foundations. |
| DMA and IOMMU | DMA request/channel foundations and Intel VT-d ACPI discovery exist. Real DMA mapping, full VT-d initialization, and AMD IOMMU support are incomplete. |
| ext filesystem | A conservative read-only ext2/ext3/ext4 metadata parser with bounds checks and selected block mapping. It rejects unsupported features and is not a production filesystem driver. |

## Build and test

The CI workflow builds the Rust UEFI application and freestanding kernel, validates the kernel ELF, packages a GPT/FAT EFI image, and boots it under OVMF/QEMU. Serial-log assertions check kernel entry and selected subsystem self-tests. Host-side tests also exercise the ext filesystem parser.

For the canonical build and boot-test result, see [GitHub Actions](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml). A successful CI run verifies only the paths covered by that run; it is not evidence of support for all hardware or Windows drivers.

### Development prerequisites

The CI workflow is the authoritative build recipe. Development generally requires a recent Rust toolchain with the x86_64-unknown-uefi and x86_64-unknown-none targets, the repository's linker script, and (for the runtime smoke test) OVMF and QEMU.

## Security and compatibility notes

- The embedded kernel SHA-256 digest detects a changed kernel only when the bootloader is trusted. It is **not** public-key signature verification or a complete verified-boot chain.
- The EFI application is not necessarily signed for Secure Boot enforcement.
- AP startup, interrupt routing, timer-driven preemption, full process/thread semantics, hardware storage drivers, graphics, and power management remain incomplete.
- Windows-style names and export metadata do not by themselves provide Windows ABI or semantic compatibility. The current syscall ABI is RdotOS-native.
- DMA/IOMMU code must fail safely when a required hardware translation or mapping feature is unavailable; bookkeeping tokens are not equivalent to hardware-backed mappings.
- The ext parser is read-only. It does not replay journals or write filesystem metadata.

## Repository layout

| Path | Purpose |
| --- | --- |
| bootloader/ | Rust UEFI application |
| kernel/ | Freestanding Rust kernel, architecture code, HAL, memory manager, executive and I/O foundations |
| third_party/Ext3Fsd | Pinned upstream filesystem-driver source reference for a planned Rust port |
| .github/workflows/ | Automated build and OVMF/QEMU validation |
| [roadmap.md](roadmap.md) | Implemented features, incomplete work, and next milestones |

## Third-party filesystem reference

The third_party/Ext3Fsd submodule is retained as a separate upstream reference for a possible staged Rust port. The original Windows driver is **not loadable by RdotOS**. See [third_party/README.md](third_party/README.md) and the upstream [Ext3Fsd repository](https://github.com/matt-wu/Ext3Fsd) for project and licensing context.

## Contributing

Contributions should be small enough to review, document their assumptions, include meaningful tests where practical, and avoid claiming compatibility beyond what has been validated. For kernel-facing changes, include relevant build or QEMU results and keep failure paths bounded and safe.

## License

RdotOS is licensed under **GNU General Public License v2.0 or (at your option) any later version** (SPDX: GPL-2.0-or-later). See [LICENSE](LICENSE). The AP startup trampoline and temporary-page-table approach in kernel/src/smp.rs are adapted from [RustOS-Dev/RustOS](https://github.com/RustOS-Dev/RustOS), which is also GPL-2.0-or-later; the source-file notice identifies the adaptation. Third-party components retain their applicable licenses and notices.

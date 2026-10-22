# RdotOS

RdotOS is an experimental, x86-64-only operating-system project exploring a modern UEFI-first architecture, hardware-backed security, isolation, and low-latency I/O.

## Status

The repository currently contains an initial UEFI application/bootstrap, not a production-ready general-purpose kernel. The bootstrap checks the firmware Secure Boot state and reports it; this is **not** yet a complete verified-boot chain. See [roadmap.md](roadmap.md) for implemented work and remaining milestones.

## Build

The GitHub Actions workflow builds an x86-64 EFI application using Clang/LLD and packages it at `EFI/BOOT/BOOTX64.EFI` in a FAT disk image. Download the workflow artifact, write/mount the image for a VM, or place the EFI executable at the removable-media fallback path on a FAT-formatted EFI System Partition.

For a VM, use x86-64 UEFI firmware. Secure Boot behavior depends on whether the EFI binary is signed by a key trusted by that firmware. The initial build is unsigned and may be rejected when Secure Boot is enabled.

## Design principles

- Native x86-64; no 32-bit compatibility subsystem is planned.
- Explicit trust boundaries: firmware, boot chain, kernel, isolated services, and drivers.
- Security claims are only marked complete after enforcement and negative tests exist.
- Hardware-specific scheduling, NVMe/GPU direct I/O, Modern Standby, and virtualization require staged platform support; they are not implied by the initial bootstrap.

## Licensing

RdotOS is dual-licensed under **Apache-2.0 OR MIT**, following the common Rust ecosystem licensing convention. You may choose either license. See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).

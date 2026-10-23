# RdotOS

RdotOS is an experimental, x86-64-only operating-system project exploring a modern UEFI-first architecture, hardware-backed security, isolation, and low-latency I/O.

## Implementation language and status

The bootloader and kernel are written in **Rust**. C is not used for the bootloader or kernel. The Rust UEFI application now reads a bounded kernel ELF file from the EFI system partition, validates core ELF64 x86-64 executable headers and PT_LOAD bounds, loads segments at their identity-mapped physical addresses, exits UEFI boot services, and jumps to the kernel with a versioned boot-information structure. The freestanding Rust kernel validates that structure and reports the handoff and memory-map count over COM1. The bootstrap now includes CPU mitigation setup, 4 KiB page-permission hardening for kernel sections, an unmapped kernel-stack guard page, exception-specific diagnostics, a build-time SHA-256 pin for the kernel ELF, and initial isolated user address spaces with checked copy helpers. It remains an early experimental OS, not production-hardened.

The kernel now also contains bounded ACPI MADT parsing, xAPIC/I/O APIC register primitives, a priority/affinity scheduler policy core, typed handles with access checks, and an initial device/IRP dispatch layer. These are foundations only: AP startup, interrupt/timer activation, actual context switching, full NT executive semantics, hardware storage drivers, filesystems, and graphics drivers are not yet implemented. Interrupts remain disabled until routing and per-CPU entry paths are complete.

See [roadmap.md](roadmap.md) for implemented work and remaining milestones.

## Build

The GitHub Actions workflow installs the Rust targets x86_64-unknown-uefi and x86_64-unknown-none, builds both crates, links the kernel with `kernel/linker.ld`, checks the ELF headers/program headers, and packages BOOTX64.EFI plus `rdotos-kernel.elf` into a FAT disk image. OVMF/QEMU runtime smoke tests assert the page-hardening and user-copy checks. The kernel hash is pinned into the bootloader at build time; this detects post-build tampering only if the bootloader itself is trusted. Asymmetric signature verification, firmware Secure Boot signing/enrollment, full ELF-segment permission validation, ring-3 entry, and complete process lifecycle remain outstanding.

For a VM, use x86-64 UEFI firmware. The EFI application is currently unsigned and may be rejected by firmware with Secure Boot enforcement enabled. The current kernel digest pin is not a substitute for public-key signature verification. A real verified-boot chain requires a trusted/signed bootloader and a protected public-key policy; firmware Secure Boot state alone is not enough.

## Design principles

- Native x86-64; no 32-bit compatibility subsystem is planned.
- Rust for both bootloader and kernel; freestanding kernel code uses no_std.
- Explicit trust boundaries: firmware, boot chain, kernel, isolated services, and drivers.
- Security claims are only marked complete after enforcement and negative tests exist.
- Hardware-specific scheduling, NVMe/GPU direct I/O, Modern Standby, and virtualization require staged platform support; they are not implied by the bootstrap.

## Licensing

RdotOS is dual-licensed under **Apache-2.0 OR MIT**, following the common Rust ecosystem licensing convention. You may choose either license. See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).

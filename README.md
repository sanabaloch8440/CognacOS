# RdotOS

RdotOS is an experimental, x86-64-only operating-system project exploring a modern UEFI-first architecture, hardware-backed security, isolation, and low-latency I/O.

## Implementation language and status

The bootloader and kernel are written in **Rust**. C is not used for the bootloader or kernel. The Rust UEFI application currently initializes the UEFI console and reports development status. A separate freestanding Rust kernel builds as an ELF image and includes an early COM1 serial diagnostic entry point. **The bootloader does not yet load or jump to the kernel**, so this is not yet a fully booting OS.

See [roadmap.md](roadmap.md) for implemented work and remaining milestones.

## Build

The GitHub Actions workflow installs the Rust targets x86_64-unknown-uefi and x86_64-unknown-none, builds the UEFI bootloader and freestanding kernel, and packages BOOTX64.EFI plus the kernel ELF into a FAT disk image. The kernel is included as a file for the upcoming verified-loading and handoff work; it is not yet executed by the bootloader.

For a VM, use x86-64 UEFI firmware. The EFI application is currently unsigned and may be rejected by firmware with Secure Boot enforcement enabled. Reading or displaying firmware state is not equivalent to enforcing a verified boot policy.

## Design principles

- Native x86-64; no 32-bit compatibility subsystem is planned.
- Rust for both bootloader and kernel; freestanding kernel code uses no_std.
- Explicit trust boundaries: firmware, boot chain, kernel, isolated services, and drivers.
- Security claims are only marked complete after enforcement and negative tests exist.
- Hardware-specific scheduling, NVMe/GPU direct I/O, Modern Standby, and virtualization require staged platform support; they are not implied by the bootstrap.

## Licensing

RdotOS is dual-licensed under **Apache-2.0 OR MIT**, following the common Rust ecosystem licensing convention. You may choose either license. See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).

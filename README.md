# RdotOS

RdotOS is an experimental, x86-64-only operating-system project exploring a modern UEFI-first architecture, hardware-backed security, isolation, and low-latency I/O.

## Implementation language and status

The bootloader and kernel are written in **Rust**. C is not used for the bootloader or kernel. The Rust UEFI application now reads a bounded kernel ELF file from the EFI system partition, validates core ELF64 x86-64 executable headers and PT_LOAD bounds, loads segments at their identity-mapped physical addresses, exits UEFI boot services, and jumps to the kernel with a versioned boot-information structure. The freestanding Rust kernel validates that structure and reports the handoff and memory-map count over COM1. This is an early bootstrap, not yet a production-hardened or fully boot-tested OS.

See [roadmap.md](roadmap.md) for implemented work and remaining milestones.

## Build

The GitHub Actions workflow installs the Rust targets x86_64-unknown-uefi and x86_64-unknown-none, builds both crates, links the kernel with `kernel/linker.ld`, checks the ELF headers/program headers, and packages BOOTX64.EFI plus `rdotos-kernel.elf` into a FAT disk image. OVMF/QEMU runtime smoke tests, signature enforcement, and hardened ELF segment-permission enforcement remain outstanding.

For a VM, use x86-64 UEFI firmware. The EFI application is currently unsigned and may be rejected by firmware with Secure Boot enforcement enabled. Reading or displaying firmware state is not equivalent to enforcing a verified boot policy.

## Design principles

- Native x86-64; no 32-bit compatibility subsystem is planned.
- Rust for both bootloader and kernel; freestanding kernel code uses no_std.
- Explicit trust boundaries: firmware, boot chain, kernel, isolated services, and drivers.
- Security claims are only marked complete after enforcement and negative tests exist.
- Hardware-specific scheduling, NVMe/GPU direct I/O, Modern Standby, and virtualization require staged platform support; they are not implied by the bootstrap.

## Licensing

RdotOS is dual-licensed under **Apache-2.0 OR MIT**, following the common Rust ecosystem licensing convention. You may choose either license. See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).

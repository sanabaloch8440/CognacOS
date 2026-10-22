# RdotOS implementation roadmap

Status reflects code currently present in the repository, not intended architecture. A checkbox is marked complete only when implementation and meaningful tests exist.

## Rust-first foundation

- [x] Add Apache-2.0 license text.
- [x] Add MIT license text and document dual licensing as Apache-2.0 OR MIT.
- [x] Replace the C UEFI bootstrap with a Rust UEFI application in bootloader/.
- [x] Add a separate freestanding no_std Rust kernel crate in kernel/.
- [x] Add a kernel linker layout, COM1 diagnostic routine, and panic halt handler.
- [x] Add CI steps to build both Rust targets and package the EFI app plus kernel ELF.
- [ ] Compile and validate artifacts in CI; resolve any toolchain/API issues exposed by the first run.
- [ ] Boot and smoke-test the EFI application in OVMF/QEMU in CI.
- [ ] Load the kernel ELF from the EFI filesystem and validate its format and load ranges.
- [ ] Define a versioned boot-information structure (memory map, framebuffer, ACPI RSDP, firmware tables).
- [ ] Exit boot services safely, preserve the final memory map, and transfer control to the kernel entry.
- [ ] Add exception/interrupt entry, GDT/IDT, APIC/timer, serial logging, and fault diagnostics.

## UEFI and verified boot

- [ ] Read/report firmware Secure Boot state for diagnostics (informational only).
- [ ] Verify signatures and hashes for every next-stage component against an explicit trust policy.
- [ ] Reject unsigned, revoked, malformed, or tampered boot components; add negative tests.
- [ ] Add key enrollment/update and revocation policy.
- [ ] Establish measured-boot event log and TPM 2.0 measurements.
- [ ] Define signed release artifacts and reproducible build provenance.

## Virtualization-based security / HVCI

- [ ] Detect CPU virtualization, SLAT/EPT, IOMMU, and firmware prerequisites.
- [ ] Implement a minimal hypervisor or a clearly specified supported hypervisor interface.
- [ ] Do not treat Windows Hypervisor Platform (WHVP) as a kernel facility: WHVP is a Windows host API and is not available as a native hypervisor inside an independent OS.
- [ ] Create a secure-kernel boundary with isolated page tables and strict memory permissions.
- [ ] Enforce code integrity for kernel and driver pages, including W^X and executable-page transition checks.
- [ ] Add isolation tests and attack-oriented negative tests; credential protection remains future work.

## x86-64 kernel and memory

- [ ] Establish long-mode kernel entry, GDT/IDT, exception handlers, APIC, and timer.
- [ ] Implement physical page allocator, virtual memory, per-process address spaces, and demand paging.
- [ ] Support large physical memory subject to architecture/platform limits; validate memory-map handling.
- [x] Keep 32-bit execution and WOW64-like compatibility out of scope.

## Scheduling and heterogeneous CPUs

- [ ] Implement preemptive scheduler, per-CPU run queues, affinity, priorities, and load balancing.
- [ ] Discover topology using ACPI/CPUID and identify core types where supported.
- [ ] Add optional Intel Thread Director telemetry integration on supported processors.
- [ ] Add generic ARM big.LITTLE capacity-aware scheduling only if/when an ARM64 port is planned.
- [ ] Benchmark latency, throughput, fairness, and power; gracefully fall back without vendor telemetry.

## Storage, DirectStorage-style I/O, and graphics

- [ ] Implement async I/O requests, completion queues, cancellation, and bounded queues.
- [ ] Add PCIe, NVMe, DMA, IOMMU, and storage-driver support.
- [ ] Add safe compressed-asset streaming and decompression pipelines.
- [ ] Add GPU drivers, VRAM mapping, and GPU-side decompression only where hardware/driver APIs support it.
- [ ] Measure CPU overhead and end-to-end latency against a baseline; do not claim DirectStorage compatibility until implemented.

## Power management

- [ ] Parse ACPI tables and implement platform power-state transitions.
- [ ] Add per-device runtime power management, wake sources, and timer coalescing.
- [ ] Implement sleep/resume paths and recovery from failed device transitions.
- [ ] Connected/Modern Standby-like behavior requires platform firmware, networking, and device support; test on real supported hardware.

## Isolation, containers, and sandboxing

- [ ] Implement process isolation, capabilities, handles, and access-control policy.
- [ ] Add namespaces/resource limits and filesystem/network isolation primitives.
- [ ] Build disposable sandbox environments on top of tested isolation primitives.
- [ ] Add adversarial boundary tests and verify teardown releases all resources.

## Driver isolation / user-mode drivers

- [ ] Define a stable user-mode driver IPC and device-object model.
- [ ] Implement driver host processes, request timeouts, crash detection, and restart.
- [ ] Add permission-checked DMA through IOMMU and safe device reset/recovery.
- [ ] Port eligible device classes to user mode; kernel-mode drivers remain necessary for selected low-level paths.

## Exploit mitigations

- [ ] Add stack canaries and compiler hardening to supported toolchains.
- [ ] Enforce W^X, NX, kernel ASLR, and read-only-after-init memory.
- [ ] Implement kernel control-flow integrity/CFG-like indirect-call validation.
- [ ] Implement ACG-like dynamic-code restrictions for untrusted processes.
- [ ] Add syscall validation, guarded user copies, safe integer/length handling, and fuzz tests.
- [ ] Add mitigation status reporting and regression tests for each enforcement point.

## Release quality

- [ ] Unit tests for allocators, parsers, crypto, scheduler, and I/O queues.
- [ ] QEMU/OVMF boot tests with Secure Boot on and off.
- [ ] Fuzz EFI/ACPI/PE/ELF and storage parsers.
- [ ] Document supported hardware, threat model, recovery path, and known limitations.

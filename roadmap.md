# RdotOS implementation roadmap

Status reflects code currently present in the repository, not intended architecture. A checkbox is marked complete only when implementation and meaningful tests exist.

## Implemented in source, pending build/boot verification

- Rust UEFI bootloader allocates a low-memory PML4, PDPT, and four page directories before ExitBootServices; the bootstrap map identity-maps 0..4 GiB with 2 MiB writable pages.
- Kernel stack, boot-info structure, and a bounded copy of the final UEFI memory map are allocated below 4 GiB so they remain reachable after CR3 changes.
- Kernel validates the PML4 address, loads it into CR3, and then initializes a single-core first-fit physical-page allocator from EfiConventionalMemory descriptors using the firmware-provided descriptor stride.
- These paths are not considered complete until CI compiles them and OVMF/QEMU tests prove the handoff, CR3 switch, allocator behavior, and failure paths.

## Rust-first foundation

- [x] Add Apache-2.0 license text.
- [x] Add MIT license text and document dual licensing as Apache-2.0 OR MIT.
- [x] Replace the C UEFI bootstrap with a Rust UEFI application in bootloader/.
- [x] Add a separate freestanding no_std Rust kernel crate in kernel/.
- [x] Add a kernel linker layout, COM1 diagnostic routine, and panic halt handler.
- [x] Add CI steps to build both Rust targets and package the EFI app plus kernel ELF.
- [x] Compile and validate artifacts in CI; resolve toolchain/API issues exposed by the first runs.
- [x] Boot and smoke-test the complete kernel handoff in OVMF/QEMU in CI (serial markers confirm kernel entry, CR3 activation, and allocator initialization).
- [x] Load the kernel ELF from the EFI filesystem with a bounded read and validate ELF64 little-endian x86-64 ET_EXEC headers and PT_LOAD ranges.
- [x] Define a versioned boot-information structure carrying the final UEFI memory-map pointer, entry count, descriptor size, and version.
- [x] Exit boot services through the uefi-rs retrying helper and transfer control to the kernel entry using the x86-64 System V ABI.
- [x] Link the first-stage kernel at a low identity-mapped address, allocate a dedicated aligned kernel stack, and report the handoff details over COM1.
- [x] Install a kernel-owned flat GDT and an IDT with fatal exception gates for CPU vectors 0-31.
- [x] Keep external interrupts disabled until interrupt-controller and timer setup is implemented.
- [x] Add exception-specific assembly stubs for CPU vectors 0–31, normalize error-code frames, and report vector/error/RIP/CS/RFLAGS plus CR2 for page faults.
- [ ] Add a dedicated IST stack for double-fault/NMI handling before claiming recoverable stack-overflow diagnostics.
- [ ] Add ELF parser unit/fuzz tests and verify segment overlap/permission policy before treating the loader as hardened.
- [ ] Add framebuffer and ACPI RSDP to the boot-information structure.
- [x] Add bounded ACPI RSDP checksum validation and ACPI MADT parsing for xAPIC/x2APIC CPU entries and I/O APIC GSI bases; synthetic MADT parser self-test passes.
- [x] Add guarded xAPIC and I/O APIC register primitives, local EOI, masked GSI routing, and timer-mask support.
- [ ] Wire firmware RSDP discovery into BootInfo, map APIC MMIO as uncacheable, install IRQ vectors, calibrate a timer, and enable interrupts only after routing is validated.
- [ ] Implement APIC/PIC routing, timer interrupts, and interrupt-safe diagnostics.
- [ ] Verify the Rust UEFI bootloader's low-memory bootstrap PML4 and 0..4 GiB identity map under OVMF/QEMU.
- [ ] Add tests for the physical-page allocator's descriptor-stride handling, conventional-memory filtering, alignment, exhaustion, and region-capacity limits.
- [ ] Harden the early physical allocator (currently single-core, first-fit, and capped below 4 GiB) before enabling concurrency.
- [x] Convert the kernel-section and stack-guard 2 MiB regions into 4 KiB PTEs; enforce NX by default and RX/RO/RW-NX section permissions, with CR0.WP active.
- [ ] Replace the remaining general-purpose 2 MiB identity map with complete per-region mappings and add a direct map for RAM above 4 GiB.
- [ ] Add boot-time checks that kernel, stack, boot-info, memory-map copy, and page-table allocations are covered by active mappings.
- [x] Reserve an unmapped page below the bootstrap kernel stack and enforce section-level NX/W^X permissions.
- [ ] Add double-fault IST handling and a controlled high-half mapping.
- [x] Add early CPU feature detection and enable CR0.WP; enable EFER.NXE and SMEP only when advertised by CPUID.
- [ ] Enable SMAP only after guarded user-memory accessors implement the required STAC/CLAC protocol; NXE alone is not page-level NX enforcement.

## Boot validation and memory hardening

- [x] Confirm GitHub Actions Rust build succeeds; resolve compiler/linker errors exposed by CI.
- [x] Confirm OVMF/QEMU reaches the Rust kernel and emits serial markers for CR3 activation and physical allocator initialization.
- [x] Confirm CR0.WP, NXE, strict kernel-section permissions, a stack guard page, and the address-space/user-copy self-test through CI/QEMU serial assertions.
- [ ] Add a reproducible regression test for malformed/truncated ELF images and overlapping PT_LOAD segments.
- [ ] Add deterministic allocator tests for descriptor stride, conventional-memory filtering, page alignment, exhaustion, and region limits.
- [ ] Replace the permissive 2 MiB identity map with 4 KiB mappings and explicit per-segment permissions. Do not enable NX until NX support is checked and all executing code, IDT/GDT, stack, boot-info, and page-table memory remain mapped.
- [ ] Introduce a higher-half mapping only with a trampoline that keeps the current identity mapping until execution, stack, IDT, and all live pointers have safely switched to the higher-half addresses.
- [ ] Test the new mappings under OVMF/QEMU, including deliberate write-to-read-only and execute-from-NX-page fault tests.
- [ ] Preserve the current low identity-map fallback until the stricter mapping path has passed automated boot tests.

## UEFI and verified boot

- [ ] Read/report firmware Secure Boot state for diagnostics (informational only).
- [x] Verify the kernel ELF SHA-256 against a build-time digest embedded in the bootloader; reject missing or mismatched digests. This is tamper detection only when the bootloader itself is trusted.
- [ ] Add asymmetric signature verification (for example Ed25519) against an externally protected public-key trust anchor and verify every next-stage component.
- [x] Reject an unpinned or hash-mismatched kernel ELF before parsing/loading it.
- [ ] Reject unsigned/revoked components using a signed trust policy and add signature/revocation negative tests.
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
- [x] Add independent PML4 address spaces, supervisor-only shared kernel mappings, checked user-copy helpers, and final per-page RX/RO/RW-NX user permissions; reject writable-executable mappings and keep user mappings in private PML4 slots.
- [x] Add ring-3 user code/data descriptors, TSS RSP0, IRETQ entry, DPL3 process-exit gate, and DPL3 syscall gate; QEMU executes a loaded PE entry in ring 3 and returns to the kernel.
- [x] Add a bounded PE32+ AMD64 loader for headers/sections, DIR64 relocations, normal and delay imports, registered DLL exports, API-set contract aliases, and final W^X section permissions; QEMU loads a synthetic DLL and binds/calls its export from a PE entry.
- [x] Add an RdotOS x64 syscall dispatcher (GETPID, WRITE, EXIT, cooperative YIELD), a process-state table (Created/Running/Exited/Reaped), exit validation, and address-space teardown with physical-page reclamation; QEMU executes GETPID, calls an imported DLL export, exits, and reaps the process.
- [x] Initialize a minimal Windows x64 PEB/TEB, TEB GS base, TLS vector, process parameters, and circular PEB loader lists for the executable and preloaded DLLs.
- [x] Parse PE TLS directories, assign per-image TLS indexes, initialize static TLS templates and zero-fill blocks, and execute DLL/executable TLS callbacks in ring 3 before entrypoints.
- [x] Register bounded AMD64 RUNTIME_FUNCTION entries from .pdata and support lookup by image/control PC.
- [x] Resolve PE delay imports and common API-set contract families through exports parsed from mapped user-mode DLL images; invoke registered DLL process-attach entrypoints in ring 3.
- [ ] Implement the Windows NT syscall ABI and service table; the current syscall ABI is RdotOS-native and not binary-compatible with Windows ntdll syscall numbers.
- [ ] Implement a full UNWIND_INFO interpreter, language-specific exception handlers, vectored exception dispatch, and process/thread exception recovery.
- [ ] Add complete process parameters, per-process DLL loader namespaces, forwarder chains, API-set schema parsing, DllMain failure/detach handling, demand paging, and concurrent process/thread lifecycle support.
- [ ] Provide actual user-mode implementations of the Windows API DLLs; imports bind to registered exports, but a full kernel32/ntdll/advapi32 implementation is not claimed.
- [ ] Support large physical memory subject to architecture/platform limits; validate memory-map handling.
- [x] Keep 32-bit execution and WOW64-like compatibility out of scope.

## ACPI, APIC, and SMP

- [x] Parse MADT Local APIC/x2APIC processor entries and I/O APIC GSI bases with table bounds/checksum validation.
- [x] Provide xAPIC MMIO initialization, masked I/O APIC routing, bounded ICR waits, and fixed/INIT/Startup IPI primitives (not yet invoked by a boot-time AP-startup coordinator).
- [x] Discover the ACPI 2.0 RSDP (falling back to ACPI 1.0) through UEFI configuration tables and pass it in boot-info v3.
- [x] Pass boot-info v4 with an optional UEFI-reserved SIPI-vector page below 1 MiB; validate AP startup-target address/alignment/APIC-ID constraints in the kernel and assert the self-test in QEMU CI (page reserved only, not yet executable).
- [x] Add xAPIC INIT deassert and a serialized INIT/deassert/two-SIPI startup coordinator with bounded AP-online timeout; requires a calibrated delay callback and is not invoked until a working trampoline/stack mailbox exists.
- [x] Validate firmware RSDP and RSDT/XSDT checksums/bounds, locate the MADT, and register enabled processors in the HAL CPU registry (discovery only; no AP startup).
- [x] Add a separate x86-64 HAL with fixed per-CPU records, APIC-ID registry, ACPI MADT CPU-registration API, CPU lifecycle states, GS-based CPU-local access, typed port I/O, MMIO range validation, and IRQ-save spin locks. The bootstrap CPU is registered at kernel entry.
- [ ] Bring up application processors using a low-memory SIPI trampoline, per-CPU stacks/GDT/TSS/IDT, calibrated startup timeouts, and APIC startup IPIs.
- [ ] Enable timer interrupts, IRQ routing, MSI/MSI-X, interrupt remapping, and multi-vCPU regression tests.

## NT executive foundations

- [x] Add bounded typed handles, basic named objects, explicit access masks, and device IRP dispatch primitives.
- [ ] Implement NT-compatible object namespace/security reference monitor, process/thread objects, events/semaphores, and reference counting.
- [ ] Implement the Windows NT syscall ABI/service table, ALPC/LPC, registry hives, and executive services.
- [ ] Implement actual context switching, preemption, and process/thread lifecycle beyond the current fixture.

## Scheduling and heterogeneous CPUs

- [x] Add a bounded scheduler metadata core with task states, priority selection, CPU-affinity masks, and cooperative yield metadata; CI smoke-tests policy selection.
- [ ] Implement actual context switching, timer-driven preemption, per-CPU run queues, affinity migration, load balancing, AP startup, and scheduler locking validated on multiple vCPUs.
- [ ] Discover topology using ACPI/CPUID and identify core types where supported.
- [ ] Add optional Intel Thread Director telemetry integration on supported processors.
- [ ] Add generic ARM big.LITTLE capacity-aware scheduling only if/when an ARM64 port is planned.
- [ ] Benchmark latency, throughput, fairness, and power; gracefully fall back without vendor telemetry.

## Storage, DirectStorage-style I/O, and graphics

- [x] Add bounded IRP allocation, dispatch, query, completion, and release primitives.
- [x] Add exclusive in-flight IRP ownership, pre-dispatch cancellation, asynchronous completion, and IRQ-safe SMP locks; QEMU boot tests exercise the request state transitions.
- [ ] Implement completion queues/DPCs, race-safe cancellation after dispatch begins, request timeouts, DMA-safe buffer pinning, and end-to-end asynchronous device I/O.
- [ ] Add PCIe enumeration, NVMe/AHCI, DMA mapping, IOMMU, and storage-driver support.
- [ ] Add safe compressed-asset streaming and decompression pipelines.
- [ ] Add a GOP framebuffer handoff, display/device abstraction, GPU drivers, VRAM mapping, and GPU-side decompression only where hardware/driver APIs support it.
- [ ] Measure CPU overhead and end-to-end latency against a baseline; do not claim DirectStorage compatibility until implemented.

## Power management

- [ ] Parse ACPI tables and implement platform power-state transitions.
- [ ] Add per-device runtime power management, wake sources, and timer coalescing.
- [ ] Implement sleep/resume paths and recovery from failed device transitions.
- [ ] Connected/Modern Standby-like behavior requires platform firmware, networking, and device support; test on real supported hardware.

## Isolation, containers, and sandboxing

- [x] Add a bounded typed kernel handle table, named-object lookup, and per-handle access-mask checks with boot self-tests.
- [ ] Add per-process handle tables, object reference counting, namespace security descriptors, capabilities, and access-control policy.
- [ ] Add namespaces/resource limits and filesystem/network isolation primitives.
- [ ] Build disposable sandbox environments on top of tested isolation primitives.
- [ ] Add adversarial boundary tests and verify teardown releases all resources.

## Driver isolation / user-mode drivers

- [x] Add an initial device/IRP dispatch contract with bounded request buffers, dispatch callbacks, completion status, and a synthetic device self-test.
- [ ] Define stable user-mode driver IPC and a production device-object model.
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


## Third-party filesystem driver port

- [x] Pin upstream Ext3Fsd sources as a separate submodule at `third_party/Ext3Fsd` for reference and staged Rust translation.
- [ ] Audit the upstream GPLv2 notices and determine distribution/licensing treatment for any Rust translation before redistribution.
- [x] Start a safe `no_std` Rust translation with checked ext superblock, geometry, inode-location, group-descriptor, inode-metadata, and directory-entry parsers; add synthetic boot self-tests.
- [ ] Add property/fuzz tests and validate against real ext2/ext3/ext4 disk images, including current Ubuntu-created 64-bit ext4 images.
- [x] Add bounds-checked ext2/ext3 legacy direct, single-, double-, and triple-indirect block mapping with synthetic fixtures; wire it into the parser self-test.
- [x] Add the ext4 CRC32C primitive and superblock metadata-checksum verifier, with host-side corruption tests. This is a foundation only: metadata_csum remains rejected by the mount parser until all consumed metadata types are verified.
- [x] Parse ext4 64-bit block counts and 64-byte group-descriptor block pointers/counts with overflow/range checks; host and boot self-tests cover high block-count parsing.
- [ ] Verify feature-correct group-descriptor, inode, extent, directory, bitmap, and journal checksums before consuming metadata; modern metadata_csum filesystems remain rejected until these checks are complete.
- [ ] Implement a read-only ext filesystem backend against RdotOS block-device and filesystem interfaces, including htree-indexed directories and validated inode-table reads.
- [ ] Add ext2/ext3/ext4 image tests before considering journal replay or write support.
- [ ] Add an NT-style filesystem-driver adapter only after the filesystem backend passes image-based tests. This is a native RdotOS implementation, not a Windows 11 .sys binary; the original Windows kernel driver cannot load directly in RdotOS.

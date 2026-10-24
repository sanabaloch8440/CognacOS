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
| UEFI boot | Rust EFI application verifies a build-pinned SHA-256 digest for the generated PE32+ AMD64 native kernel, validates its fixed-base section layout, maps its sections at the high-half image base, exits boot services, and transfers control through versioned boot information. The kernel is currently linked as ELF64 for Rust/toolchain convenience, then converted and validated as PE32+ for the bootloader handoff. |
| Kernel bootstrap | Freestanding Rust kernel, kernel-owned GDT/IDT, exception diagnostics, COM1 logging, CR3 activation, and physical-page allocation. |
| Memory protection | High-half PE32+ kernel section mappings with checked virtual-to-physical backing, 4 KiB kernel-section permissions, CR0.WP, NX where supported, kernel stack guard page, isolated user address spaces, checked user-copy helpers, a demand-mapped RAM direct-map window extending above 4 GiB, and W^X checks in implemented mapping paths. |
| NT memory manager | Partial Windows x64 ABI implementations for MDL lifecycle operations, canonical/user-kernel boundary validation, and pinned-page accounting. `MmProbeAndLockPages` can fault in tracked pageable kernel-pool pages through the kernel page-fault/page-in path. `MmMapLockedPagesSpecifyCache` can map arbitrary non-contiguous PFNs into a contiguous kernel VA range or a tracked user alias in the active registered process; user aliases must be unmapped in that same address space before unlock, and backing frames remain MDL-owned. Cached mappings only. Missing pageable user pages still fail closed: per-process VMA/page-in and exception-safe probe recovery are not implemented. `MmCopyVirtualMemory` translates the registered source and target process roots under process references and the shared address-space lifetime lock, and accesses high physical RAM through validated direct-map pointers. These EPROCESS-compatible objects are RdotOS internal structures, not private binary layouts for a specific Windows build. User-mode MDL mappings, pageable user-address faulting, full WDK edge-case parity, and complete Windows process/object manager semantics remain incomplete. |
| ACPI and HAL | RSDP/RSDT/XSDT validation, MADT parsing, CPU-local records, APIC-ID registry, port-I/O primitives, MMIO bounds checks, and IRQ-save locks. |
| SMP | AP startup uses a temporary low-memory page-table root, 16-page stacks, bounded INIT/SIPI coordination, APIC-ID validation, AP-local GDT/IDT loads, local-APIC setup, and GS CPU-local state. Each online AP must acknowledge 64 directed IPIs in the boot stress test. | Still experimental; the new 2/4/8-vCPU QEMU matrix must pass in CI. Per-CPU TSS/IST, scheduler integration, broader race testing, and real-hardware validation remain incomplete. |
| Executable loading | Bounded PE32+ AMD64 loading, relocations, normal/delay imports, registered exports, API-set aliases, TLS setup/callbacks, and basic unwind metadata registration. |
| User mode | Ring-3 transition and a small RdotOS-native syscall/process lifecycle test path. This is not the Windows NT syscall ABI. |
| Kernel services | Initial scheduler policy, typed handles/access checks, dispatcher primitives, and device/IRP request lifecycle foundations. |
| DMA and IOMMU | DMA request/channel foundations and Intel VT-d ACPI discovery exist. Real DMA mapping, full VT-d initialization, and AMD IOMMU support are incomplete. |
| ext filesystem | A conservative read-only ext2/ext3/ext4 metadata parser with bounds checks and selected block mapping. It rejects unsupported features and is not a production filesystem driver. |

## Build and test

The CI workflow builds the freestanding Rust kernel, converts its linked ELF64 image to a fixed-base PE32+ AMD64 native image, validates PE headers/sections and the pinned digest, builds the Rust UEFI loader, and boots it under OVMF/QEMU. The main smoke test uses 5 GiB RAM and four virtual CPUs. An additional 2/4/8-vCPU SMP matrix asserts expected AP counts, BSP TSS/IST wiring, per-AP directed IPI acknowledgements, cross-CPU scheduler wakeups, contended IRQ-save locks, thread exit/teardown, and VM/TLB self-tests. Host-side tests exercise the physical allocator map parser and ext filesystem parser. The expanded matrix is committed but its first CI result is not yet confirmed.

For the canonical build and boot-test result, see [GitHub Actions](https://github.com/umarfarooq7869/RdotOS/actions/workflows/build-uefi.yml). A successful CI run verifies only the paths covered by that run; it is not evidence of support for all hardware or Windows drivers.

### Development prerequisites

The CI workflow is the authoritative build recipe. Development generally requires a recent Rust toolchain with the x86_64-unknown-uefi and x86_64-unknown-none targets, the repository's linker script, and (for the runtime smoke test) OVMF and QEMU.

## Recent milestone: PE32+ kernel handoff

- The bootloader now consumes `EFI/BOOT/rdotos-kernel.exe`, a fixed-base AMD64 PE32+ image using the native subsystem. The UEFI bootloader itself remains a normal EFI application.
- The build converts the Rust-linked ELF64 kernel into PE32+ and checks the DOS/PE signatures, AMD64 machine type, PE32+ optional-header magic, fixed image base, native subsystem, section RVAs, and absence of imports/base relocations.
- The bootloader validates the supported static PE layout, allocates physical backing for each section, checks the build-pinned SHA-256 digest, builds the kernel mappings, and enters the high-half kernel through a low identity-mapped CR3 transition trampoline.
- The kernel checks the active CR3, inspects required bootstrap mappings, verifies high-half section backing and stack translation, and checks the virtual stack guard before applying section protections.
- The memory manager has a demand-mapped direct-map window for UEFI-described RAM above 4 GiB; physical-to-virtual conversion rejects addresses outside the eligible RAM ranges.

This is a project-specific, fixed-base PE32+ kernel format—not a general Windows kernel loader. The image deliberately has no imports or base relocations, and it is not yet loaded by the Windows boot manager. Windows x64 driver compatibility remains a long-term goal, not an achieved compatibility claim.

## Recent milestone: tracked user-mode MDL mappings

- `MmMapLockedPagesSpecifyCache` now has a user-mode mapping path for a registered process whose address-space root is active. It reserves a range in a dedicated user VA arena and maps the MDL's locked PFNs without taking ownership of those physical frames.
- Mapping records bind the MDL, process object, address-space root, base VA, and page count. Unmapping validates the same process/root and PFN sequence before removing aliases. External user PTEs are distinguished so address-space teardown does not free the MDL-owned frames.
- Probe/map/unmap/unlock entry points share an IRQ-safe lifecycle lock; MDL free checks avoid releasing an owned MDL while it remains locked or mapped.
- CI run [40083235192](https://github.com/umarfarooq7869/RdotOS/actions/runs/40083235192) passed after the MDL teardown comparison fix.

This milestone is **not** full Windows MM/process compatibility. User mappings require an active registered process and resident locked PFNs. Demand-zero/pagefile-backed user page-in, Windows-style exception delivery/recovery, scheduler-driven current-process/thread semantics, complete object/handle lifetime behavior, and race/WDK conformance tests remain open.

## Security and compatibility notes

- The embedded PE32+ kernel SHA-256 digest detects a changed kernel only when the bootloader is trusted. It is **not** public-key signature verification or a complete verified-boot chain.
- Bootstrap identity mappings use 2 MiB leaves for UEFI-described ranges; descriptor edges are not yet mapped with fully granular 4 KiB permissions, and this is not a complete per-region isolation policy.
- SMAP remains disabled until all guarded user-memory access paths are audited and consistently use STAC/CLAC.
- The EFI application is not necessarily signed for Secure Boot enforcement.
- SMP and executive startup are experimental. The NT executive startup module now sequences AP startup and SMP self-tests; each CPU gets a private GDT/TSS and dedicated IST stacks, and cooperative kernel-thread dispatch updates TSS RSP0 and uses HAL IPIs to wake eligible CPUs. Timer-driven preemption, per-CPU run queues/load balancing, IST guard pages/fault injection, concurrent user-MDL/process-exit teardown tests, and real-hardware SMP validation remain incomplete. Full process/thread semantics, hardware storage drivers, graphics, and power management also remain incomplete.
- Windows-style names and export metadata do not by themselves provide Windows ABI or semantic compatibility. The current syscall ABI is RdotOS-native.
- The MDL exports use explicit Windows x64 calling-convention signatures and remain a bounded subset. `MmProbeAndLockPages` can fault in tracked pageable kernel-pool pages and pins resident user pages only when their mappings are valid; it does not yet provide demand-zero/pagefile-backed user page-in or Windows SEH-style exception delivery for invalid buffers. `MmMapLockedPagesSpecifyCache` supports non-contiguous PFNs in a kernel VA extent and a tracked user VA alias in the active registered process; user aliases must be unmapped from the same process/address space before unlock, and cached mappings are the only supported cache policy. Probe/map/unmap/unlock bookkeeping is serialized by an IRQ-safe lifecycle lock, but teardown races and WDK edge cases still need stress validation. The internal EPROCESS-compatible structure is not binary-compatible with a private Windows build layout. Cross-process copying is supported only for RdotOS-registered process objects and is serialized against address-space mutation/destruction.
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

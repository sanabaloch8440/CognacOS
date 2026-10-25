<p align="center">
  <img src="assets/cognacos-logo.svg" alt="CognacOS" width="420">
</p>

# CognacOS

**CognacOS is an experimental Rust-first x86-64 operating system with a UEFI boot path and NT-inspired kernel interfaces.** Its long-term goal is Windows x64 driver compatibility; that goal is not yet achieved. CognacOS is not a Windows 11 replacement and does not claim general WDK or third-party driver compatibility.

[![Build and QEMU boot test](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml/badge.svg)](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml)

- **Language:** Rust (`no_std` kernel and UEFI loader)
- **Architecture:** x86-64
- **Boot:** UEFI; native AMD64 kernel image packaged and validated as PE32+
- **License:** GPL-2.0-or-later

## Project documentation

- **[CognacOS documentation website](https://umarfarooq7869.github.io/CognacOS/)** — project overview, roadmap, and compatibility index.
- [Development roadmap](docs/roadmap.md) — current status, open milestones, and next priorities.
- [Windows 11 x64 function-level compatibility comparison](WINDOWS11_COGNACOS_NTOSKRNL_KE_COMPARISON.md) — Ke routine inventory, implementation scope, and scoring method.
- [Security Reference Monitor comparison](WINDOWS11_COGNACOS_NTOSKRNL_SE_COMPARISON.md) — Se coverage and known semantic gaps.

## Current foundations

| Area | Implemented foundation | Important gaps |
| --- | --- | --- |
| Boot and architecture | Rust UEFI loader, digest-pinned PE32+ kernel handoff, GDT/IDT, exception diagnostics, CR3 activation | Secure-boot signature chain, broader hardware validation |
| Memory management | Kernel mappings and permissions, user address-space roots, checked user copies, MDL/pinning foundations, RAM direct map above 4 GiB | User demand paging/working sets, exception-safe probes, complete process isolation |
| SMP and scheduling | Per-CPU ready queues, affinity-aware stealing, saved-context publication, timer preemption, wake IPIs, threaded DPC workers | Full Windows quantum/priority decay and scheduler semantics |
| NT executive | Ke/Ki runtime, timer/DPC rundown checks, PASSIVE-level threaded DPCs, APC lifecycle, queue/mutant wait foundations | Full user-mode SEH/APC dispatcher, blocking wait-all for every object type, WDK conformance |
| I/O and drivers | Initial WDM object/IRP layouts, dispatch/completion, device rundown, restricted PE driver loader | General driver compatibility, complete cancellation/rundown, PnP and storage integration |
| Registry and NLS | Bounded native hive parser, empty hive image creator, service lookup, raw NLS staging | Log replay, persistent registry writes, NLS interpretation, NTFS discovery |
| Platform and storage | Separate ACPI/PCI/HAL-contract/processor crates; storage protocol foundations; native Rust ext-family driver | Hardware drivers, DMA/IOMMU backend, full VFS and writable filesystem |
| Security | Bounded descriptor validation, basic access/privilege evaluation, token metadata hooks, audit-event groundwork | Full token lifecycle, impersonation, audit policy, callback ACEs, WDK parity |

These are implementation foundations, not production-readiness claims. Details and acceptance criteria live in the roadmap. The CI workflow tests host-side components, builds the freestanding kernel and UEFI loader, validates the PE32+ image, and runs OVMF/QEMU boot and SMP checks. A passing emulator run does not prove real-hardware reliability or Windows driver compatibility.

## Repository layout

- `kernel/src/arch.rs`, `kernel/src/smp.rs` — architecture and processor startup
- `kernel/src/vm.rs`, `kernel/src/memory.rs` — virtual memory and physical allocation
- `kernel/src/ntoskrnl/` — NT-inspired executive and subsystem implementations
- `kernel/src/hal/` — HAL and APIC implementation
- `drivers/ext4/` — native Rust ext-family filesystem crate
- `.github/workflows/` — build, host-test, and boot validation workflows
- `docs/` — public project documentation source

## Development principles

1. Match Windows 11 x64 names and documented ABI only where evidence supports it; never treat an export name as proof of implementation.
2. Keep unsupported or unsafe paths fail-closed rather than claiming compatibility.
3. Mark a feature complete only when implementation and meaningful validation exist.
4. Keep progress tracking in the roadmap and detailed routine inventories in the subsystem comparison documents, not duplicated in this README.

## License

CognacOS is licensed under **GNU GPL v2.0 or (at your option) any later version**. See [LICENSE](LICENSE). Third-party components retain their applicable licenses and notices.

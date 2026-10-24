---
layout: default
title: Project status
---

# Project status

RdotOS is an experimental, Rust-first x86-64 kernel using a Rust UEFI loader and a native AMD64 PE32+ kernel handoff. Its design separates architecture, HAL, virtual memory, the NT-inspired executive, and driver-facing services.

## Implemented foundations

| Area | Current foundation | Open work |
| --- | --- | --- |
| Boot | Digest-pinned PE32+ kernel handoff, UEFI boot, kernel diagnostics | Secure-boot signature chain, hardware coverage |
| Memory | User address-space roots, checked user copies, MDL/pinning groundwork, direct map above 4 GiB | Demand paging, working sets, safe exception recovery |
| SMP | AP startup, per-CPU GDT/TSS/IST, directed IPI acknowledgements, cooperative dispatch | Timer preemption, run queues, real hardware |
| Executive | Ke/Ki routines, dispatcher and APC/DPC foundations, bounded Object Manager | Full NT semantics and WDK conformance |
| I/O | Initial WDM object/IRP lifecycle, restricted PE driver loader | Full cancellation/rundown, PnP, storage integration |
| Security | Bounded descriptor validation, basic access/privilege checks, token metadata hooks | Complete token lifecycle, impersonation and audit policy |
| Storage | Protocol and filesystem foundations | Real hardware paths, IOMMU, VFS, crash-safe writes |

See the [roadmap](roadmap.md) for acceptance criteria. Detailed Ke and Se status is maintained in the [compatibility section](compatibility/index.md).

## Validation

The main GitHub Actions workflow runs host-side regression tests, builds the freestanding kernel and UEFI loader, validates the PE32+ image, and runs OVMF/QEMU boot tests including SMP configurations. Emulator success is not a substitute for real hardware or WDK driver tests.

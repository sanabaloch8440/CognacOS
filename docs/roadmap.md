---
layout: default
title: Roadmap
---

# CognacOS roadmap

This page is a short index. The root [roadmap.md](https://github.com/umarfarooq7869/CognacOS/blob/main/roadmap.md) is the single canonical checklist and contains the remaining tasks and acceptance criteria.

## Current priorities

1. Restore green current-revision CI: Rust builds, NTDLL PE/export checks, QEMU boot, and 2/4/8-vCPU SMP.
2. Prove memory isolation and safe concurrent address-space teardown; implement user working-set trimming and pagefile-backed paging.
3. Validate I/O cancellation/completion/unload rundown with kernel-level fault injection under QEMU SMP.
4. Connect the native PE dependency transaction to NTDLL and test real CPL3 loading, initialization, rollback, and unload.
5. Complete process-owned handle tables, token lifecycle, user APC/SEH delivery, and scheduler-backed waits.
6. Integrate disk-backed VFS, PCI/storage, safe DMA/IOMMU, crash-safe writable filesystems, and remaining registry/NLS semantics.
7. Automate Windows 11 x64 ABI/layout/export checks and expand conformance testing.

## Implemented foundations

CognacOS has a Rust UEFI/PE32+ boot path with kernel signature verification; NT-inspired kernel/HAL structure; SMP, scheduler, memory/address-space, checked-copy and fault-containment foundations; initial Object Manager, WDM/IRP, registry, ETW, ALPC/section, NTDLL and PE-loader paths; and a native Rust ext-family filesystem crate.

These foundations are not claims of complete Windows 11 or third-party driver compatibility. Each item remains open until its relevant tests pass on the current revision.

[**Open the actionable roadmap on GitHub →**](https://github.com/umarfarooq7869/CognacOS/blob/main/roadmap.md)

[View current GitHub Actions runs](https://github.com/umarfarooq7869/CognacOS/actions)

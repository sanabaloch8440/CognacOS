---
layout: default
title: Roadmap
---

# CognacOS roadmap

The roadmap is maintained in one canonical file in the repository root to avoid two copies drifting apart. Open the complete milestone tracker below.

[**Open the canonical roadmap on GitHub →**](https://github.com/umarfarooq7869/CognacOS/blob/main/roadmap.md)

## Current focus

- **Memory isolation:** cross-page copy failures, rollback, map/unmap races, and exception-safe fault recovery.
- **Scheduling and dispatch:** user APC return/SEH delivery, queue wait-all reservations, and wider race/conformance stress.
- **Object Manager and security:** process-bound handle tables, token lifecycle, impersonation, callbacks, and audit policy.
- **I/O and storage:** cancellation/completion races, device rundown, PCI resource assignment, AHCI/NVMe paths, and crash-safe writable filesystems.
- **Windows x64 ABI:** routine-by-routine evidence, automated ABI/export checks, and WDK conformance testing.

## Validation policy

A milestone is complete only when the implementation exists and a relevant test or validation path supports it. A successful compile, import resolution, or emulator boot alone does not establish full Windows ABI or third-party driver compatibility.

[View current GitHub Actions runs](https://github.com/umarfarooq7869/CognacOS/actions)

---
layout: default
title: Roadmap
---

# CognacOS roadmap

Implementation ownership is being organized under `kernel/src/ntoskrnl/`: address-space and user virtual-memory code lives in `mm/address_space.rs`; root `vm.rs` and `vm_range.rs` have been removed. x86-64 CPU/IDT entry and exception dispatch live in `ke/arch.rs`, and the former `ki` runtime has been merged into `ke/runtime.rs`.

The roadmap is maintained in one canonical file in the repository root to avoid two copies drifting apart. Open the complete milestone tracker below.

[**Open the canonical roadmap on GitHub →**](https://github.com/umarfarooq7869/CognacOS/blob/main/roadmap.md)

## Current focus

- **Boot security:** validate signature tamper rejection in CI; configure stable release keys and exercise enrolled-key Secure Boot plus TPM 2.0 measurement on real firmware.
- **Memory isolation:** validate CPL=3 demand-zero fault routing; implement user working-set eviction/pagefile-backed page-in, recover kernel probes through generalized trap fixups, deliver user access violations without halting the kernel, and prove isolation with multi-process fault/race tests.
- **Scheduling and dispatch:** user APC return/SEH delivery, queue wait-all reservations, and wider race/conformance stress.
- **Object Manager and security:** process-bound handle tables, token lifecycle, impersonation, callbacks, and audit policy.
- **I/O and storage:** cancellation/completion races, device rundown, PCI resource assignment, AHCI/NVMe paths, and crash-safe writable filesystems.
- **Windows x64 ABI:** routine-by-routine evidence, automated ABI/export checks, and WDK conformance testing.

## Validation policy

A milestone is complete only when the implementation exists and a relevant test or validation path supports it. A successful compile, import resolution, or emulator boot alone does not establish full Windows ABI or third-party driver compatibility.

[View current GitHub Actions runs](https://github.com/umarfarooq7869/CognacOS/actions)

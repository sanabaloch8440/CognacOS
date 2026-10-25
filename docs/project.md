---
layout: default
title: Project details
---
# Project details

CognacOS is an experimental Rust-first x86-64 operating system using a Rust UEFI loader and a native AMD64 PE32+ kernel handoff. Its architecture separates platform support, virtual memory, the NT-inspired executive and driver-facing services.

## Goals
- Keep kernel and platform code modular and maintainable.
- Build a reliable UEFI boot and initialization path.
- Implement scoped NT-inspired interfaces with explicit tests and documented limits.
- Measure public ABI declarations separately from behavior-level compatibility.
- Make progress reproducible through CI, QEMU/OVMF and milestone acceptance criteria.

## Implementation foundations and gaps

| Subsystem | Foundations | Major gaps |
| --- | --- | --- |
| Boot | UEFI loader, PE32+ handoff, GDT/IDT, diagnostics | Secure-boot signature chain, broader hardware validation |
| Memory | User address-space roots, checked user copies, MDL/pinning, high-memory direct map | Demand paging, working sets, exception-safe probes, complete isolation |
| SMP/scheduler | AP startup, per-CPU state, timer preemption, per-CPU queues, affinity-aware stealing | Full Windows quantum/priority policy, hardware hot-remove |
| Executive | Ke/Ki runtime, dispatcher, timer/DPC/APC foundations, bounded Object Manager | Full NT semantics, private object ABI, WDK conformance |
| I/O/drivers | Initial WDM object/IRP lifecycle and restricted PE driver loader | Complete cancellation/rundown, PnP, storage integration |
| Security | Descriptor validation, access/privilege checks, token metadata, audit groundwork | Full token lifecycle, impersonation, SACL/audit policy |
| Storage | Protocol and native Rust ext-family filesystem foundations | Hardware paths, DMA/IOMMU, full VFS, writable crash-safe filesystem |

These are implementation foundations, not production-readiness claims. See the [roadmap]({{ '/docs/roadmap.html' | relative_url }}) for the canonical milestone checklist.

## Validation strategy
1. Run host-side unit/regression tests.
2. Build the freestanding kernel and UEFI loader.
3. Validate image format and imports/exports.
4. Boot with OVMF/QEMU, including multi-vCPU scenarios.
5. Track race/stress testing separately from ordinary boot success.
6. Treat physical hardware and Windows WDK conformance as separate acceptance stages.

- [Repository](https://github.com/umarfarooq7869/CognacOS)
- [CI workflow](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml)
- [ABI dashboard]({{ '/docs/compatibility/index.html' | relative_url }})

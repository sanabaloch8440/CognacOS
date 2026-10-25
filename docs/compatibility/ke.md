---
layout: default
title: Ke ABI compatibility
---
# Ke — Windows kernel executive compatibility

**Reference:** Windows 11 x64 build 10.0.26100.8036 (AMD64), as scoped by the canonical comparison.

| Measure | Current comparison |
| --- | ---: |
| Tracked Ke routine names | 169 |
| Source declarations / ABI call paths | 169/169 · 100% |
| Name-based import resolution | 169/169 · 100% |
| Scoped strict behavior matches | 51/169 · 30.2% |
| Partial or not stress-verified | 118/169 · 69.8% |
| Project-defined combined score | 65.1% |

The combined score averages declaration coverage and scoped strict behavior. It is not a Microsoft metric and does not predict the percentage of Windows drivers that will load or run.

## Foundations tracked
- Timer/DPC lifecycle and rundown foundations; per-CPU queues and timer preemption.
- APC delivery paths and dispatcher wait foundations for supported objects.
- Processor callbacks, rendezvous foundations, and selected process attach/detach behavior.

## Known gaps
- Full user APC return trampoline and user-mode exception/SEH delivery.
- Complete queue/mutant wait-all reservations and wider race/conformance stress.
- Full processor hot-remove, unresponsive-CPU recovery, multiple processor groups.
- Exact private KTHREAD/KDPC/object layouts and Windows quantum/boost-decay policy.
- General WDK conformance and physical-hardware validation.

**Canonical routine inventory:** [Windows 11 x64 vs CognacOS Ke comparison](https://github.com/umarfarooq7869/CognacOS/blob/main/WINDOWS11_COGNACOS_NTOSKRNL_KE_COMPARISON.md).

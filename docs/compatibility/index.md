---
layout: default
title: Windows x64 ABI compatibility
---
# Windows x64 ABI compatibility

This dashboard compares CognacOS with public Windows 11 x64 WDK/DDI contracts, using Windows 11 build 10.0.26100.8036 (AMD64) where stated in the canonical comparisons.

## Ke status snapshot

| Metric | Current comparison | Interpretation |
| --- | ---: | --- |
| Tracked Ke routine names | 169 | Current comparison inventory |
| Source declarations / ABI call paths | 169/169 · 100% | Does not prove private layouts or edge-case behavior |
| Name-based import resolution | 169/169 · 100% | A resolver path is not semantic compatibility |
| Scoped strict behavior matches | 51/169 · 30.2% | Matches only within documented CognacOS scope |
| Project-defined combined score | 65.1% | Average of declaration coverage and strict behavior; not a Microsoft metric |

## Component reports
- [Ke — Kernel Executive]({{ '/docs/compatibility/ke.html' | relative_url }}) — routines, ABI declaration coverage, behavior and missing semantics.
- [Se — Security Reference Monitor]({{ '/docs/compatibility/se.html' | relative_url }}) — implemented security entry points, current limitations and remaining work.

## Status definitions
- **Scoped match:** behavior matches the documented contract within the explicitly stated project constraints and has supporting tests.
- **Partial:** an implementation path exists, but important semantics, ABI details, concurrency behavior or validation remain incomplete.
- **Declared/resolved only:** a name or import path exists; correct behavior is not implied.
- **Missing:** required behavior is not adequately implemented.
- **Unverified:** code exists but the required test or latest CI run has not passed.

Private Windows kernel source and internal layouts are proprietary. Consult the canonical [Ke comparison](https://github.com/umarfarooq7869/CognacOS/blob/main/WINDOWS11_COGNACOS_NTOSKRNL_KE_COMPARISON.md) and [Se comparison](https://github.com/umarfarooq7869/CognacOS/blob/main/WINDOWS11_COGNACOS_NTOSKRNL_SE_COMPARISON.md) for per-routine evidence and scoring rules.

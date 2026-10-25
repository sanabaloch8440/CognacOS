---
layout: default
title: CognacOS — Project Overview
---
<style>
.site-title{display:none}.cognac-hero{padding:2rem 1rem 2.5rem;margin-bottom:2rem;border-radius:20px;background:linear-gradient(135deg,#fff6e9,#fff 70%,#f8e8d8);text-align:center}.cognac-hero img{width:min(100%,560px);height:auto}.cognac-hero h1{font-size:clamp(2rem,6vw,3.6rem);letter-spacing:-.05em;color:#20242b}.cognac-hero p{max-width:760px;margin:auto;color:#555;font-size:1.1rem}.cognac-actions,.cognac-grid{display:flex;flex-wrap:wrap;gap:1rem;justify-content:center;margin:1.4rem 0}.cognac-actions a{padding:.7rem 1rem;border-radius:9px;background:#d96d1d;color:#fff;text-decoration:none;font-weight:650}.cognac-card{flex:1 1 210px;max-width:340px;border:1px solid #e6dfd8;border-radius:14px;padding:1rem;background:#fff}.cognac-card h3{margin-top:0}.muted{color:#666;font-size:.93rem}@media(prefers-color-scheme:dark){.cognac-hero{background:#25211d}.cognac-hero h1{color:#f5eee7}.cognac-hero p{color:#ddd}.cognac-card{background:#222;border-color:#444}.muted{color:#bbb}}
</style>
<div class="cognac-hero">
<img src="{{ '/assets/cognacos-logo.svg' | relative_url }}" alt="CognacOS bottle logo">
<h1>Build an open, Rust-first operating system.</h1>
<p>CognacOS is an experimental x86-64 operating system with a UEFI boot path and NT-inspired kernel interfaces. Track engineering progress, validation, and the gaps between CognacOS and public Windows x64 ABI contracts.</p>
<div class="cognac-actions"><a href="{{ '/docs/project/' | relative_url }}">Project details</a><a href="{{ '/docs/roadmap/' | relative_url }}">Roadmap & milestones</a><a href="{{ '/docs/compatibility/' | relative_url }}">Windows ABI status</a></div>
</div>

## Project at a glance
<div class="cognac-grid">
<div class="cognac-card"><h3>Platform</h3><p><strong>x86-64 · UEFI · Rust</strong></p><p class="muted">Freestanding kernel and native AMD64 PE32+ handoff.</p></div>
<div class="cognac-card"><h3>Compatibility target</h3><p><strong>Windows 11 x64</strong></p><p class="muted">Public contracts target build 10.0.26100.8036 where stated. Not a Windows replacement.</p></div>
<div class="cognac-card"><h3>Validation</h3><p><strong>CI + OVMF/QEMU</strong></p><p class="muted">Automated build, image checks, emulator boot and SMP tests. Hardware and WDK conformance are separate.</p></div>
</div>

## Windows kernel ABI dashboard
The canonical Ke comparison tracks **169 routine names**: declaration/call-path coverage **100%**, import-name resolution **100%**, strict scoped behavior matches **30.2%** (51/169), and a project-defined combined score of **65.1%**. These are not Microsoft metrics and do not predict third-party driver compatibility.

<div class="cognac-grid">
<div class="cognac-card"><h3>Ke — Kernel Executive</h3><p>Routine inventory, ABI declarations, resolver coverage, behavior scope and missing semantics.</p><a href="{{ '/docs/compatibility/ke/' | relative_url }}">Read Ke status →</a></div>
<div class="cognac-card"><h3>Se — Security Reference Monitor</h3><p>Implemented security entry points, token metadata, descriptor validation and open security work.</p><a href="{{ '/docs/compatibility/se/' | relative_url }}">Read Se status →</a></div>
<div class="cognac-card"><h3>Roadmap</h3><p>Milestones for memory, scheduling, object management, I/O, security and storage.</p><a href="{{ '/docs/roadmap/' | relative_url }}">Track milestones →</a></div>
</div>

## Development links
- [Source repository](https://github.com/umarfarooq7869/CognacOS)
- [Build and QEMU workflow](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml)
- [Latest workflow runs](https://github.com/umarfarooq7869/CognacOS/actions)
- [Canonical roadmap source](https://github.com/umarfarooq7869/CognacOS/blob/main/roadmap.md)

> **Compatibility disclaimer:** An exported name, matching signature, PE32+ image, or successful emulator boot does not prove Windows ABI conformance. CognacOS does not currently claim general WDK or third-party Windows driver compatibility.

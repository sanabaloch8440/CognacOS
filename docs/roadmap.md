---
layout: default
title: Roadmap
---

# CognacOS roadmap

Implementation ownership is being organized under `kernel/src/ntoskrnl/`: address-space and user virtual-memory code lives in `mm/address_space.rs`; root `vm.rs` and `vm_range.rs` have been removed. x86-64 CPU/IDT entry and exception dispatch live in `ke/arch.rs`, and the former `ki` runtime has been merged into `ke/runtime.rs`.

The roadmap is maintained in one canonical file in the repository root to avoid two copies drifting apart. Open the complete milestone tracker below.

[**Open the canonical roadmap on GitHub →**](https://github.com/umarfarooq7869/CognacOS/blob/main/roadmap.md)

## Current focus

- **Native file and DLL loader:** read-only registered boot-file objects and bounded native open/read dispatch are present; NTDLL has typed PEB loader-list lookup/enumeration. General disk-backed VFS, native section/map/unmap services, dependency-aware image loading, and safe final unload remain incomplete. See the [canonical loader milestones](https://github.com/umarfarooq7869/CognacOS/blob/main/roadmap.md#native-file-and-dll-loader-progress-in-progress).

- **Boot security:** validate signature tamper rejection in CI; configure stable release keys and exercise enrolled-key Secure Boot plus TPM 2.0 measurement on real firmware.
- **Memory isolation and fault containment:** bounded vector/RIP trap-frame fixups now set an explicit recovery RIP and RAX result, with an invalid-probe boot self-test; unresolved CPL3 exceptions use the saved trap frame and terminate only the owning process. A new boot test checks same-VA mappings across two address spaces and supervisor-only kernel mappings; ring-3 IRETQ/syscall/process-exit checks remain in the QEMU path. Current CI/QEMU validation and adversarial CPL3 access-fault tests remain open, along with concurrent teardown stress, user-mode SEH resume/unwind, user-page eviction/pagefile-backed page-in, and SMP-safe working-set trimming/reclamation.
- **Scheduling and dispatch:** user APC return/SEH delivery, queue wait-all reservations, and wider race/conformance stress.
- **Object Manager and security:** process-bound handle tables, token lifecycle, impersonation, callbacks, and audit policy.
- **I/O and storage:** initial WDM completion-claim state and bounded per-IRP device rundown references are implemented; unload defers while an IRP remains outstanding. A boot regression checks pending-IRP drain and exactly-once completion; CI also runs a deterministic host cancellation/completion/unload fault-injection model. Passing current QEMU SMP validation and kernel-level callback-vs-teardown stress remain required before calling rundown complete; PCI resource assignment, AHCI/NVMe paths, and crash-safe writable filesystems also remain open.
- **Windows x64 ABI:** routine-by-routine evidence, automated ABI/export checks, and WDK conformance testing.

The trap-fixup, process-scoped CPL3 termination, same-VA address-space isolation test, and WDM rundown regression are implemented, but the current workflow must pass before those boot checks count as validated. The host race model has passed its CI step; that does not substitute for QEMU SMP execution or kernel-level callback-race fault injection. Concurrent teardown, user-mode SEH resume/unwind, working-set reclamation, and complete I/O rundown remain open acceptance criteria. The canonical tracker records the detailed tests and ordering guarantees in [Milestone 2](../roadmap.md).

## Validation policy

A milestone is complete only when the implementation exists and a relevant test or validation path supports it. A successful compile, import resolution, or emulator boot alone does not establish full Windows ABI or third-party driver compatibility.

[View current GitHub Actions runs](https://github.com/umarfarooq7869/CognacOS/actions)

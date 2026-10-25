---
layout: default
title: Se security ABI compatibility
---
# Se — Security Reference Monitor compatibility

The comparison targets public Windows 11 x64 contracts. An export name or resolver path does not prove equivalent security behavior.

## Implemented foundations
- Bounded structural validation of self-relative security descriptors.
- Registered CognacOS token metadata queries, including selected token/session/authentication fields.
- Initial privilege-check and length-aware SID/ACL/descriptor helper foundations.

These routines apply to explicitly registered CognacOS metadata and supported readable spans; they are not substitutes for Windows token objects or a complete Windows security subsystem.

## Remaining work
- Complete token allocation and SID/privilege installation across process creation and duplication.
- Conditional callback ACEs, GUID-qualified object ACEs, full privilege-set output and restricted-token double evaluation.
- Subject-context capture/reference semantics, impersonation, SACL/audit policy, AppContainer capabilities and silo policy.
- Descriptor inheritance, complete Se export semantics, final-image export validation and WDK conformance.

Focused unit tests cover malformed descriptors and selected token metadata operations, but the canonical comparison states that a new CI run is required before that revision is considered validated.

**Canonical details:** [Windows 11 vs CognacOS Se comparison](https://github.com/umarfarooq7869/CognacOS/blob/main/WINDOWS11_COGNACOS_NTOSKRNL_SE_COMPARISON.md).

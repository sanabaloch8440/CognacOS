# Windows 11 vs RdotOS Se compatibility

Target reference: Windows 11 x64 10.0.26100.8036-AMD64. The Se export list in `kernel/src/ntoskrnl/se/mod.rs` is inventory metadata; an export name or ordinal does not imply implementation.

## Implemented Se exports

| Routine | Implementation | Scope |
| --- | --- | --- |
| `SeValidSecurityDescriptor` | `kernel/src/ntoskrnl/se/runtime.rs` | Bounded structural validation of self-relative descriptors: revision/control, SID and ACL/ACE bounds, offsets and overlap checks. |
| `SeTokenIsAdmin`, `SeTokenIsRestricted`, `SeTokenIsWriteRestricted` | `se/runtime.rs` | Query registered RdotOS token metadata; unknown tokens fail closed. |
| `SeTokenType`, `SeTokenImpersonationLevel` | `se/runtime.rs` | Query registered token kind/impersonation level; unknown or non-impersonation tokens return 0. |
| `SeSinglePrivilegeCheck` | `se/runtime.rs` | Kernel-mode bypass; user-mode requests fail closed until current-thread token/privilege integration exists. |
| `SeQuerySessionIdToken`, `SeQuerySessionIdTokenEx` | `se/runtime.rs` | Query session ID from registered token metadata with pointer and token validation. |
| `SeQueryAuthenticationIdToken` | `se/runtime.rs` | Query authentication LUID stored in registered token metadata. |

The Se export catalogue is located at `kernel/src/ntoskrnl/se/mod.rs`; the old top-level `se.rs` catalogue was removed. Twenty-one additional RdotOS-only security helpers provide token kind/known-state queries, packed flags, privilege bitmap and range checks, privilege counts, session/logon matching, impersonation-level checks, and output-parameter query variants. These internal helpers do not claim Windows export ABI compatibility. The import resolver routes the ten implemented Se export entry points by exact name.

## Remaining work

- Integrate token metadata with real Ps primary/impersonation token objects, references and rundown; this table is not a Windows TOKEN implementation.
- Implement `SePrivilegeCheck` against real `PRIVILEGE_SET` and subject-context layouts.
- Implement the `SeAccessCheck` family and Windows-compatible access-state semantics.
- Subject-context capture/lock/release and safe capture of user-provided descriptors.
- Security descriptor assignment/inheritance and complete owner/group/DACL/SACL semantics.
- Audit policy, privilege auditing, SACL evaluation, callbacks and transactional audit events.
- AppContainer/capability and silo policy, signing-level policy, secure-boot policy integration and WDK conformance.

## Safety and validation notes

The descriptor validator accepts a raw pointer without a length and inspects a maximum 4-KiB window. The caller must guarantee that memory is readable; this is not a user-buffer probe/capture implementation. Token queries recognize only explicitly registered RdotOS metadata and are not compatible with arbitrary Windows token pointers. The Se single-privilege check denies user-mode requests because current-thread token binding is not yet implemented. These changes establish a limited implementation foundation, not complete Windows security semantics.

## Validation status

Focused Rust unit tests cover malformed descriptors, token metadata registration/query/unregistration, session/authentication ID queries, and fail-closed privilege behavior. A new CI run must complete before this revision is considered validated; Windows WDK conformance remains outstanding.

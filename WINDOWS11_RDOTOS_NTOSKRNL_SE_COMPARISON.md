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


## Additional length-aware security primitives (2026-10-25)

The runtime now also includes the following **RdotOS extension helpers**, which are not Windows-export ABI implementations:

- internal Se helper and internal Se helper: validate/compare SID encodings only within explicit caller-supplied readable spans.
- internal Se helper: validates ACL size and ACE boundaries within an explicit span and checks SID bounds for common basic ACE forms.
- internal Se helper and internal Se helper: length-aware self-relative descriptor control/component queries, including component offset and length validation.
- internal Se helper and internal Se helper: small pure predicates for explicit privilege bitmaps and allow/deny masks.

Focused unit tests cover truncated SID/ACL buffers, SID equality, descriptor component bounds, privilege masks, and deny-wins access-mask behavior. These helpers do not yet provide a complete ACE evaluator, generic mapping, restricted-token double evaluation, privilege-set ABI handling, subject-context capture, auditing, or a Windows SeAccessCheck implementation. Caller-supplied pointers must still refer to readable memory for the exact explicit span; this is length-aware validation, not fault-safe user-buffer probing.


The same extension layer now also exposes bounded SID revision/subauthority/identifier-authority queries; ACL metadata and ACE-type counting; basic SID membership search in ACLs; descriptor self-relative/component-presence predicates; and access/privilege mask utilities. Invalid or truncated inputs return conservative failure values. The SID-in-ACL search only understands common basic SID-bearing ACE types and is not an authorization decision.


## Export-table boundary correction

All RdotOS-only SID, ACL, descriptor, token-metadata and mask helpers are ordinary crate-internal Rust functions; they are not `no_mangle` symbols and are not added to the Windows kernel export table. The security runtime retains unmangled Windows-named ABI entry points only for the Se routines that are explicitly implemented and listed by the project's Windows 11 export catalogue. Do not expose private helper routines as kernel exports. The catalogue may contain unsupported Windows exports for tracking purposes; listing a name does not claim the routine is implemented.


## SeAccessCheck implementation pass (2026-10-25)

- Added the Windows-named `SeAccessCheck` entry point and name-based import resolution. The ABI surface uses a subject-context record, generic mapping, granted-access output, access status, and optional privilege-set output.
- Added a bounded internal token SID registry. The registration hook validates and copies up to eight SIDs per token, and access checks only match SIDs registered for the selected primary/impersonation token or the Everyone SID.
- The access evaluator supports self-relative descriptors, absent/null DACL semantics, owner `READ_CONTROL`/`WRITE_DAC`, generic access mapping, and basic `ACCESS_ALLOWED_ACE` / `ACCESS_DENIED_ACE` processing. Unsupported object/callback ACE families, unknown tokens, malformed descriptors, and unlocked subject contexts fail closed.
- Added a focused unit test for a registered token SID and an allow ACE.

**Important integration and compatibility limits:** token SID registration is currently an internal hook and is not yet called by Ps token creation/rundown; callers must register identities and unregister them before token storage is reclaimed. The implementation uses a bounded 4-KiB readable-memory contract because the Windows routine does not receive a descriptor length; this is not fault-safe probing/capture. Privilege-set construction/auditing, restricted-token double evaluation, AppContainer/capability checks, full object/callback ACE semantics, kernel subject-context locking, and WDK conformance remain incomplete. This is a real but intentionally scoped implementation, not full Windows 11 Security Reference Monitor parity.

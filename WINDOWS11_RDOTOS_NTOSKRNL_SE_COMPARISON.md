# Windows 11 vs RdotOS Se compatibility

Target reference: Windows 11 x64 10.0.26100.8036-AMD64. The Se export list in `kernel/src/ntoskrnl/se/mod.rs` is inventory metadata; an export name or ordinal does not imply implementation.

## Implemented Se exports

| Routine | Implementation | Scope |
| --- | --- | --- |
| `SeValidSecurityDescriptor` | `kernel/src/ntoskrnl/se/runtime.rs` | Bounded structural validation of self-relative descriptors: revision/control, SID and ACL/ACE bounds, offsets and overlap checks. |
| `SeTokenIsAdmin`, `SeTokenIsRestricted`, `SeTokenIsWriteRestricted` | `se/runtime.rs` | Query registered RdotOS token metadata; unknown tokens fail closed. |
| `SeTokenType`, `SeTokenImpersonationLevel` | `se/runtime.rs` | Query registered token kind/impersonation level; unknown or non-impersonation tokens return 0. |
| `SeSinglePrivilegeCheck` | `se/runtime.rs` | Kernel-mode bypass; user-mode checks query the referenced effective thread/process token privilege bitmap. |
| `SeQuerySessionIdToken`, `SeQuerySessionIdTokenEx` | `se/runtime.rs` | Query session ID from registered token metadata with pointer and token validation. |
| `SeQueryAuthenticationIdToken` | `se/runtime.rs` | Query authentication LUID stored in registered token metadata. |

The Se export catalogue is located at `kernel/src/ntoskrnl/se/mod.rs`; the old top-level `se.rs` catalogue was removed. Twenty-one additional RdotOS-only security helpers provide token kind/known-state queries, packed flags, privilege bitmap and range checks, privilege counts, session/logon matching, impersonation-level checks, and output-parameter query variants. These internal helpers do not claim Windows export ABI compatibility. The import resolver routes the ten implemented Se export entry points by exact name.

## Remaining work

- Complete the Windows TOKEN object allocator and automatically populate SID/privilege metadata during all real Ps token creation and duplication paths.
- Complete conditional callback ACE expression evaluation and GUID-qualified object ACE evaluation with object-type-list context.
- Implement full privilege-set output allocation and privilege/audit reporting from SeAccessCheck.
- Add per-token subject-context locking and exact Windows capture/reference semantics; the current global lock is a serialized foundation.
- Integrate full SACL policy, audit delivery/policy, AppContainer package SID semantics, capabilities and silo policy.
- Implement descriptor assignment/inheritance and remaining Se exports only when their ABI and behavior can be validated.
- Run WDK conformance tests and verify the final built PE export table in CI against the pinned manifest; the current CI path checks names/ordinals are a subset of Windows 11 and rejects extra names.

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

**Status note:** this paragraph describes the first implementation pass and is superseded by the follow-up section below. The implementation remains partial; see the latest integration limits and validation status.


### SePrivilegeCheck follow-up (2026-10-25)

- Added the Windows-named `SePrivilegeCheck` export and exact-name resolver mapping.
- The implementation evaluates bounded `PRIVILEGE_SET` LUID entries against the selected registered token's 64-bit privilege bitmap, honors `PRIVILEGE_SET_ALL_NECESSARY`, rejects more than 64 entries, and denies unknown tokens.
- Kernel-mode checks retain the documented privilege-check bypass behavior.

**Remaining limitations:** the privilege namespace is currently a 64-bit internal bitmap rather than the full Windows LUID/token privilege representation; token lifecycle and subject-context lock/reference semantics are not integrated with Ps; user-memory fault-safe capture and WDK conformance remain outstanding.


## Security lifecycle, access evaluation, and export validation follow-up (2026-10-25)

- Corrected SeAccessCheck's SubjectContextLocked argument to a BOOLEAN pointer and used signed-byte KPROCESSOR_MODE for access-mode arguments. Added compile-time function-pointer signature checks for the implemented Se/Ps routines.
- Added SeCaptureSubjectContext/SeReleaseSubjectContext, global serialized SeLockSubjectContext/SeUnlockSubjectContext, primary-token references on process objects, impersonation-token references on thread records, PsAssignImpersonationToken, and PsReferencePrimaryToken/PsDereferencePrimaryToken. Process/thread rundown releases held token references. Internal token-duplication metadata propagation copies ordinary, restricting, and capability SID sets plus privilege metadata.
- Added restricted-token second-pass DACL evaluation, a 64-event SACL audit ring for supported audit ACEs, privilege-assisted ACCESS_SYSTEM_SECURITY/WRITE_OWNER grants, capability SID matching, and page-table-validated user-memory copy helpers. The internal descriptor capture helper copies bounded descriptor components into kernel storage.
- Basic ACEs, unqualified object ACEs, and conditionless callback ACEs are handled. GUID-qualified object ACEs and callback ACEs with application-data conditions fail closed because this API lacks object-type-list context and a Windows conditional-ACE expression evaluator.
- Added a host-side test harness for Se runtime validation, access checks, privilege checking, restricted-token evaluation, and SACL audit-event recording.
- Updated PE conversion to build an .edata directory from defined global function symbols whose names and ordinals exist in manifest_26100.json. CI checks every final PE export against the pinned Windows 11 10.0.26100.8036-AMD64 manifest and requires implemented security exports.

**Still incomplete:** the repository does not yet have a full token allocator/Windows TOKEN object model or a complete Ps token creation/duplication pipeline that automatically invokes every internal registration hook. Process primary-token assignment is an internal lifecycle API; callers must use it after token metadata creation. The global subject-context lock is a serialized compatibility foundation, not Windows' exact per-token lock implementation. The bounded user-copy helper is available to capture code, but exported SeAccessCheck consumes a kernel-captured descriptor and does not itself probe arbitrary user pointers. Conditional callback ACE evaluation, GUID-aware object-type access lists, restricted-token edge cases, full privilege-set output allocation, comprehensive SACL policy/audit delivery, AppContainer package-SID semantics, WDK tests, and a successful final CI run remain open. Exported names are constrained to names in the pinned Windows table; unsupported Windows exports remain unimplemented.

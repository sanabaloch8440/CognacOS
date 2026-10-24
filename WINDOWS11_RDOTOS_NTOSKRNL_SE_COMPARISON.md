# Windows 11 vs RdotOS Se compatibility

Target reference: Windows 11 x64 10.0.26100.8036-AMD64. The Se export list in `kernel/src/ntoskrnl/se.rs` is inventory metadata; an export name or ordinal does not imply implementation.

## Implemented

| Routine | Implementation | Scope |
| --- | --- | --- |
| `SeValidSecurityDescriptor` | `kernel/src/ntoskrnl/se/runtime.rs` | Bounded structural validation of self-relative security descriptors: revision and control checks, owner/group SID structure, ACL/ACE bounds, offset checks, and component overlap rejection. |

The routine is explicitly wired in `ntoskrnl::resolve` by name. Unit tests cover a minimal self-relative descriptor and malformed SID/offset cases.

## Not yet implemented

- `SeAccessCheck` family and Windows-compatible access-state semantics.
- Process/thread primary tokens, impersonation tokens, privilege sets, restricted tokens, linked tokens, logon sessions, and token lifetime/rundown.
- Subject-context capture/lock/release and safe capture of user-provided security descriptors.
- Security descriptor assignment/inheritance and complete owner/group/DACL/SACL semantics.
- Audit policy, privilege auditing, SACL evaluation, callbacks, and transactional audit events.
- AppContainer/capability and silo policy, signing-level policy, secure-boot policy integration, and WDK conformance.

## Safety and validation notes

The current validator accepts a raw pointer without a length, matching the export's pointer-only shape, and inspects a maximum 4-KiB window. The caller must already guarantee that this memory is readable; this is not a user-buffer probe/capture implementation. Before exposing further Se routines, RdotOS needs scheduler-bound current-process identity and actual token/privilege state. Existing Object Manager SID-list DACL checks are a separate bounded mechanism and are not full Se integration.

## Validation status

The source includes focused Rust unit tests. They are not considered CI-validated until a workflow run executes them successfully. No general Windows 11 security subsystem or driver compatibility is claimed.

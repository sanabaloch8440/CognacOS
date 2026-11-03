# AML Method Frames and Serialization

This document tracks task 29 of the ACPI production-readiness checklist.
Behavioral references are the public ACPI Specification 6.6 Method definition
(Section 19.6.85) and AML MethodFlags grammar (Section 20.2.5.3). CognacOS
executor code is independently authored; no ACPICA implementation code or
fixtures are copied.

## Method frame lifetime

- Every invocation allocates fresh Local0–Local7 and Arg0–Arg6 slots.
- Locals and unused argument slots begin as Uninitialized, not Integer zero.
- The low three MethodFlags bits define the exact argument count (0–7).
- Nested method invocations create independent frames and share only the
  current top-level object arena, namespace bindings, and method synchronizer.
- The call-depth limit is 16 nested levels. Execution step limits remain
  enforced per invocation.
- Runtime argument values are validated against the active object arena before
  frame creation. The convenience API rejects arena-owned Package arguments;
  namespace-node ObjectReferences remain valid, while stale PackageElement,
  BufferByte, and BufferField handles fail closed.
- Package method returns remain rejected by the convenience API because their
  element handles need an owning result-arena API.

## Serialized methods and SyncLevel

MethodFlags are decoded as specified by ACPI:

- Bits 0–2: argument count.
- Bit 3: SerializeFlag.
- Bits 4–7: SyncLevel (0–15).

For Serialized methods, the executor validates that the method SyncLevel is
not lower than the active caller SyncLevel, calls the host's
`AmlMethodSynchronizer::acquire(method_index, sync_level)`, executes the
method, and calls `release` on every normal/error return path after successful
acquisition. Nested calls inherit the active SyncLevel; returning from a nested
call restores the caller's level.

The synchronizer trait is `Sync` and must be backed by a per-thread host
provider that coordinates callers of each method object, blocks until the
implicit method mutex can be acquired (or returns `ExecutionBusy`), and
supports the reentrancy/ownership behavior required by the OS. The provider
instance must be scoped to the namespace/table whose method indices it keys.

The default convenience API deliberately fails closed with
`SynchronizationUnavailable` for Serialized methods. Callers that have an OS
synchronization provider use `execute_method_typed_with_sync` or
`execute_method_with_sync`.

## Regression coverage

Tests cover:
- Exact argument counts and independent nested Local/Arg frames.
- Uninitialized locals and unused arguments.
- Recursive calls and the call-depth limit, including recursive Serialized
  method acquisition/release.
- Successful Serialized method acquisition and release.
- Release after an AML execution error.
- Busy lock propagation without releasing an unowned mutex.
- Nested SyncLevel increase and rejection of a lower nested SyncLevel.
- Rejection of invalid extents and unowned arena handles in caller arguments.

## Remaining integration boundary

The interpreter API and fail-closed behavior are implemented, but CognacOS
still needs a concrete kernel-backed `AmlMethodSynchronizer` connected to its
thread scheduler/wait primitives before Serialized firmware methods can run
under true concurrent execution. That integration is not claimed complete by
this executor-side batch. Full Mutex/Acquire/Release, Events, and Global Lock
semantics remain separate work.

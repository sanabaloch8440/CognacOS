# Win32 keyboard input delivery: implementation contract

This document defines the end-to-end acceptance contract for Win32 keyboard input. A kernel build, a scan-code trace, or an entry in a kernel event queue is not evidence that a ring-3 application received a Win32 message.

## Required path

`QEMU keyboard device → controller IRQ → HAL interrupt dispatch → keyboard ISR → deferred keyboard processing → input subsystem → target thread's message queue → user-mode GetMessage/NtUserGetMessage → ring-3 test application`

## Implementation boundaries

1. **Hardware and interrupt path**
   - Resolve legacy IRQ 1 through ACPI MADT interrupt-source overrides.
   - Register the keyboard ISR and validate the installed vector before unmasking the route.
   - Drain the controller with a bounded interrupt-safe handler and issue exactly one APIC EOI for each delivered interrupt.
   - Keep allocation, blocking, user-memory access, and message construction out of ISR context. Hand off decoded input through a bounded queue/DPC or equivalent deferred path.
   - Track interrupt count, scan-code count, queue overflow, and deferred-processing count independently.

2. **Input decoding**
   - Handle make/break codes, extended prefixes, modifier state, key-repeat policy, and malformed/incomplete sequences.
   - Preserve virtual-key and scan-code information for key messages.
   - Generate character input using the active keyboard layout and modifier state; do not equate a raw scan code with a Unicode character.
   - Post key-down/up messages and character messages in the documented order, with repeat and system-key flags represented correctly.

3. **Per-thread Win32 message queues**
   - Create the queue with the owning GUI thread and associate it with that thread, not globally with the kernel.
   - Protect queue state against concurrent producers and consumers; define lock ordering and avoid lost wakeups between the empty check and thread blocking.
   - Bound queue capacity and specify overflow behavior. Wake a blocked consumer after publishing an entry.
   - Keep thread teardown and queue destruction safe against an in-flight producer or waiter.

4. **User-mode boundary**
   - Validate the entire user output range for the active process and copy `MSG` fields with the address-space copy helpers; never dereference a user pointer directly in kernel mode.
   - Implement the native `NtUserGetMessage` wait/remove behavior and the user-mode `GetMessage` wrapper with compatible return semantics, including `WM_QUIT`.
   - A blocked read must be interruptible by queue input and thread termination. Invalid output pointers must fail safely without corrupting queue state.
   - Keep kernel-private queue structures out of the user ABI.

## End-to-end QEMU acceptance test

The test must run the built OS under QEMU and inject a key through the emulated keyboard device (not by calling a kernel enqueue helper). Use a unique key sequence, for example pressing and releasing `A`, and capture independently verifiable checkpoints:

1. QEMU device injection was issued successfully.
2. The real registered keyboard IRQ handler incremented its interrupt counter.
3. The deferred input path decoded the make and break events.
4. The target ring-3 process/thread was scheduled and its queue received the corresponding messages.
5. The ring-3 test application called the actual user-mode `GetMessage` path and observed the expected `WM_KEYDOWN`, `WM_CHAR` (where the active layout produces one), and `WM_KEYUP` values, including key code and ordering.
6. The application emitted a machine-readable PASS record containing the test ID and received message values; the harness rejects missing checkpoints, timeouts, duplicate or out-of-order messages, and kernel-only evidence.

The QEMU smoke test must fail if any checkpoint is absent. A successful compilation, boot banner, IRQ counter alone, kernel queue contents, or synthetic direct call to the message queue is explicitly **not** a pass.

## Status

This is an acceptance contract, not a claim that the complete path already exists. The repository currently has ring-3 transition support, checked current-address-space copy helpers, kernel thread primitives, and PS/2 foundations, but these primitives alone do not establish a functioning Win32 GUI message path. Mark the end-to-end milestone complete only after the QEMU application-level assertions above run in CI.

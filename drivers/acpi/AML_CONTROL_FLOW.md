# AML Control-Flow Execution

This document describes CognacOS's bounded AML method control-flow executor.
The implementation is independently authored and uses the public ACPI
Specification 6.6 AML grammar and operator descriptions as behavioral
references; no ACPICA implementation code or test fixture is copied.

## Package boundaries

- Each method executes through a Reader whose end is the exact method-body end.
- Nested If, Else, and While packages are decoded against the enclosing Reader's
  end, not merely against the backing AML byte slice.
- Predicate and body Readers are bounded to the corresponding control-flow
  package. A nested term cannot consume the next sibling term or escape the
  method body.
- Invalid or truncated package lengths fail with a bounded package error.

## Control-flow behavior

- If evaluates its predicate once. Its Else package is a sibling package and
  executes only when the predicate is false.
- While re-evaluates its predicate at the start of the bounded While body on
  every iteration.
- Break and Continue propagate through nested If/Else blocks and are consumed
  by the nearest enclosing While.
- Return propagates through every enclosing If/Else/While and terminates the
  method.
- Else without a preceding If is rejected.
- Loop iteration, nesting, method call depth, and total execution steps are
  bounded to prevent malformed AML from monopolizing the interpreter.

## Regression coverage

The AML executor tests cover both If predicate outcomes, Else execution,
nested loop Break/Continue propagation, Return from a live While body,
Break/Continue outside loops, non-terminating loop bounds, standalone Else
rejection, and malformed If/Else/While packages that exceed their enclosing
method body.

This is an executor milestone, not a claim that all AML control-flow-adjacent
features (serialization, synchronization, events, method reentrancy, or
hardware-backed FieldUnits) are complete.

## Verification

Task 28 passed ACPI workflow [41587859995](https://github.com/sanabaloch8440/CognacOS/actions/runs/41587859995): 49 AML method-executor tests, 37 AML runtime tests, 42 AML parser tests, and 11 AML table-lifecycle tests, plus all configured firmware regression suites. The follow-up documentation commit passed [41587872679](https://github.com/sanabaloch8440/CognacOS/actions/runs/41587872679).

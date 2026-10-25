# Native NT syscall bridge (x86-64)

This directory begins a native Windows execution backend that is separate from
Wine's Unix syscall path. The first primitive is
`CognacNtSyscall4`, a Microsoft x64 ABI thunk for NT system calls with up to
four arguments.

## Critical rules

- **Never hard-code syscall numbers for Windows 11.** Service numbers vary by
  Windows build. A future native NTDLL loader must resolve the relevant stub
  from the exact target build and validate it before dispatch.
- The bridge only handles four arguments. Calls with additional stack
  arguments require a separate, ABI-correct stack-marshalling thunk.
- This does not make Wine's current `ntdll.dll` independent of Wine server or
  Unix code. The existing `Makefile.in` still builds `ntdll.so` and includes
  Unix implementations. Do not disable those sources until native replacements
  exist for every referenced service.
- The native backend must be built and tested as a distinct target; it must
  not alter the existing Wine-on-Unix build.

## ABI

Microsoft x64 entry registers are transformed to the NT syscall convention:
`EAX = service number`, `R10 = argument 1`, `RDX = argument 2`,
`R8 = argument 3`, and `R9 = argument 4`. The `SYSCALL` instruction
transfers control to the operating system kernel. The kernel's syscall ABI,
not the ordinary user-mode calling convention, governs this boundary.

## Required follow-up

1. Add a separately configured native NTDLL target that excludes Unix objects
   and Wine server imports.
2. Resolve native service numbers safely for the running Windows build.
3. Implement and test the complete argument marshalling path, including
   stack-passed arguments and return/status handling.
4. Port every required NTDLL service away from Wine server/POSIX dependencies.
5. Run ABI tests and end-to-end tests on supported Windows 11 builds before
   claiming native compatibility.

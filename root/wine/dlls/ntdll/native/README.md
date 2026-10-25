# Native NTDLL PE32+ bootstrap (x86-64)

A standalone MinGW-w64 target is provided. It produces an AMD64 PE32+ DLL
without compiling Wine's Unix implementation, ntdll.so, or linking to the Wine
server.

Build and check it with:

```sh
make -C root/wine/dlls/ntdll/native CROSS=x86_64-w64-mingw32- check
```

The output is a minimal bootstrap DLL exporting NtClose, NtYieldExecution, and
a syscall-configuration entry point. It is **not yet a drop-in replacement**
for Microsoft's NTDLL: the full export surface, loader initialization, runtime
contracts, unwind support, and extensive NT runtime behavior remain unimplemented.

## Syscall safety

Windows syscall numbers vary by OS build. None are hard-coded. The
CognacNtdllConfigureSyscalls entry point requires nonzero service numbers from
a trusted component that knows the exact running kernel's syscall table.
Until configured, wrappers return STATUS_NOT_IMPLEMENTED. Incorrect numbers
can invoke unrelated kernel services. The four-argument assembly bridge is
used only for these one-argument/zero-argument examples; it is not a general
syscall marshaller.

## Build separation

The existing Wine Makefile.in still builds ntdll.dll and ntdll.so for Wine's
normal environment. This standalone target does not alter that build. It
emits ntdll.dll only as a local build artifact; do not install or use this
incomplete DLL as a Windows system NTDLL.

## Next milestones

1. Implement the complete, versioned syscall ABI and argument marshalling for
   CognacOS, including arguments beyond the first four.
2. Port required Rtl*, Ldr*, heap, exception/unwind, TLS, PEB/TEB, and process
   startup routines without Wine server/POSIX dependencies.
3. Validate the required export table and ordinals against Windows 11 images.
4. Add Windows VM loader tests and ABI tests before calling this a drop-in NTDLL.

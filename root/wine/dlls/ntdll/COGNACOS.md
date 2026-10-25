# CognacOS NTDLL

NTDLL development lives directly in this directory. The module manifest
(`Makefile.in`) no longer lists Wine's Unix companion library, server, syscall
dispatcher, POSIX libraries, or `unix/*.c` sources. The native PE32+ source
set is freestanding and is built with:

```sh
make -C root/wine/dlls/ntdll -f Makefile.cognac clean all check
```

GitHub Actions runs this build when any NTDLL source or the workflow changes
and publishes the resulting `ntdll.dll` artifact.

The dependency removal is a build-graph change, not proof that the whole
Windows 11 runtime is implemented. The native target currently has a small
bootstrap surface (`NtClose`, `NtYieldExecution`, and RTL memory routines).
Syscall IDs must be supplied by a trusted, version-aware CognacOS kernel
integration; they are not hard-coded because service numbers vary across NT
builds. The current assembly bridge supports no more than four arguments.

Remaining work is to port existing loader, process/thread, exception/unwind,
heap, synchronization, file, registry, security, and ALPC functionality to
CognacOS kernel interfaces, then expand and verify the export table and ABI
against Windows 11. Wine-only implementation files remain in the source
checkout for reference, but are no longer part of the native NTDLL build.

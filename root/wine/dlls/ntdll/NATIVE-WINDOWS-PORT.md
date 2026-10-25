# Native Windows backend for NTDLL (porting plan)

## Goal

Allow CognacOS to use the Wine-derived NTDLL implementation in a native Windows process environment, where native NT system services are provided by the running Windows kernel. This is a separate execution mode from Wine-on-Unix; it must not silently fall back to Wine's Unix library or wineserver.

## Current blocker

The existing NTDLL target is explicitly split into a PE DLL and a Unix library. `Makefile.in` declares `UNIXLIB = ntdll.so` and compiles the `unix/*.c` implementation, while `unix/loader.c` starts the Wine loader/server and relies on Unix process APIs. `unix/server.c` implements Wine-specific server IPC. Removing a few calls or changing syscall stubs cannot make this architecture native.

## Required implementation boundaries

1. **Keep the Wine-on-Unix build intact.** Native mode must be opt-in and must not regress the existing `ntdll.dll + ntdll.so + wineserver` configuration.
2. **Use the host NT ABI for kernel services.** Native-mode `Nt*` entry points must issue the documented platform syscall transition or call a verified OS-provided NTDLL entry point where appropriate. Do not hard-code syscall numbers copied from a particular Windows build; syscall IDs are not a stable public ABI.
3. **Remove Wine-only dependencies from native mode.** The native build must not link `ntdll.so`, initialize `__wine_unix_call_dispatcher`, start wineserver, or depend on Wine server request structures and Unix file descriptors.
4. **Preserve Windows ABI details.** Keep exported names/ordinals, calling conventions, structure layouts, NTSTATUS behavior, alignment, and x64 register/stack argument rules compatible with the target Windows version.
5. **Do not recurse into the DLL being implemented.** Importing the host's `ntdll.dll` under the same module name is not a valid generic forwarding strategy. Any host-service binding needs an explicit, verified bootstrap design.
6. **Separate kernel services from user-mode policy.** NTDLL routines that currently implement behavior using wineserver or POSIX facilities need native implementations or an explicitly unsupported status; they must not pretend a Wine server operation succeeded.

## Suggested staged port

- [ ] Inventory PE exports and classify them as pure user-mode helpers, native system-service stubs, Wine extensions, or Unix/server-backed implementations.
- [ ] Add an architecture-specific native syscall-stub generator/metadata source, sourced from a controlled Windows build/SDK test environment rather than guessed service numbers.
- [ ] Implement and test a minimal x64 syscall path for a small set of essential services (for example, close, query basic system information, virtual-memory operations, and delay execution) on a disposable Windows 11 test VM.
- [ ] Add a separate native build configuration that excludes all `unix/*.c` sources and Wine-only exports from the native artifact.
- [ ] Port the PE-side implementations in dependency order, replacing server-backed process/thread, synchronization, virtual-memory, file, registry, security, and loader paths with native NT calls.
- [ ] Add ABI/export checks and runtime tests on Windows 11; compare NTSTATUS, output structures, handle lifetime, cancellation, and error paths against the host implementation.
- [ ] Keep the native backend disabled by default until it passes the test suite.

## Acceptance criteria

A successful native port must build a PE DLL without an ELF/Mach-O NTDLL companion, start without a Wine loader or wineserver, and pass the native Windows 11 test suite for every service marked supported. Merely compiling the PE DLL or resolving exports is not sufficient.

## References

- Microsoft Learn, [Libraries and Headers](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/libraries-and-headers): describes NTDLL user-mode entry points as the interface that converts Nt/Zw calls into system calls trapped to kernel mode.
- Wine's [NTDLL Unix interface](https://github.com/wine-mirror/wine/blob/master/dlls/ntdll/unixlib.h): illustrates the explicit Unix-library boundary used by Wine.

# CognacOS NTDLL

NTDLL development lives directly in this directory. The native build excludes
Wine's Unix companion library, server, syscall dispatcher, POSIX support
libraries, and `unix/*.c` sources. Build the freestanding x64 PE32+ DLL with:

```sh
make -C root/wine/dlls/ntdll -f Makefile.cognac clean all check
```

## Compile-time syscall link helper

`tools/ntdll_link_helper.py` parses an x64 PE32+ `ntdll.dll` without executing
it, extracts canonical `Nt*`/`Zw*` syscall stubs, and emits a header containing
service IDs for that exact Windows build. To generate the header from a known
Windows 11 installation:

```sh
make -C root/wine/dlls/ntdll -f Makefile.cognac syscall-header \
  WINDOWS_NTDLL=/path/to/Windows/System32/ntdll.dll
make -C root/wine/dlls/ntdll -f Makefile.cognac clean all check \
  WINDOWS_SYSCALL_IDS=1
```

The IDs are build-specific. Rebuild the header from the exact target OS image;
never treat them as stable constants or copy them to another Windows build.
This optional mode is for side-by-side ABI experiments, not a supported
replacement for the protected system `ntdll.dll`. Windows expects a much
larger export surface and strict loader/runtime semantics that this bootstrap
does not yet implement.

## CognacOS kernel binding

For CognacOS, leave `WINDOWS_SYSCALL_IDS=0`. The trusted CognacOS loader/kernel
integration must provide the syscall numbers using
`CognacNtdllConfigureSyscalls` before invoking `NtClose` or
`NtYieldExecution`. This keeps OS service numbers out of universal constants.
The native bootstrap currently exports `NtClose`, `NtYieldExecution`, the RTL
memory routines, and `RtlGetVersion` (reports Windows 11 build 26100). The
assembly bridge currently supports four syscall arguments only; it is not yet
suitable for syscalls requiring stack arguments.

GitHub Actions builds and checks the PE32+ artifact and validates the helper's
CLI. This does not claim full Windows 11 compatibility. The remaining work is
to port loader, process/thread, exception/unwind, heap, synchronization, file,
registry, security, and ALPC semantics to CognacOS kernel interfaces, then
expand and verify NTDLL exports and ABI behavior.

## RTL routines shared with the kernel

The supplied Windows x64 NTDLL image is used as an ABI/export reference, not
as code to copy into CognacOS. Its export directory contains 2,429 named
exports; inspection found no literal PE forwarded-export entries. Therefore,
a function that also exists in ntoskrnl should not automatically be treated as
a PE forwarder: user-mode NTDLL and kernel-mode RTL implementations have
different execution contexts and must be bound across the OS boundary only
through an explicit supported ABI.

As an interim implementation, `root/wine/dlls/ntoskrnl.exe/rtl_memory.c`
now provides kernel-side `RtlCopyMemory`, `RtlMoveMemory`,
`RtlFillMemory`, `RtlZeroMemory`, `RtlCompareMemory`,
`RtlFillMemoryUlong`, and `RtlCompareMemoryUlong`. The Wine ntoskrnl
compatibility module builds this file from its Makefile. These implementations
avoid heap/server calls and use bytewise operations, including overlap-safe
move and unaligned buffers.

This is a staging point, not the final native kernel implementation: the
Wine `ntoskrnl.exe` module still has Wine/Win32 runtime dependencies. The
same ABI and tests must be carried into CognacOS's native `ntoskrnl` before
these routines can be considered production kernel exports. NTDLL retains its
own user-mode copies where appropriate; user-mode exports cannot directly
jump into privileged kernel code.

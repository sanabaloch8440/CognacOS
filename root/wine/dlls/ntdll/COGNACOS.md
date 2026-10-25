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
The current assembly bridge supports four register arguments only; it is not
yet suitable for syscalls requiring stack arguments.

GitHub Actions builds and checks the PE32+ artifact and validates the helper's
CLI. This does not claim full Windows 11 compatibility. The remaining work is
to port loader, process/thread, exception/unwind, heap, synchronization, file,
registry, security, and ALPC semantics to CognacOS kernel interfaces, then
expand and verify NTDLL exports and ABI behavior.

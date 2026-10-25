# CognacOS NTDLL

The NTDLL work lives directly in this directory alongside Wine's existing NTDLL
sources. There is no nested second NTDLL source tree.

Build the current native x86-64 PE32+ target with:

```sh
make -C root/wine/dlls/ntdll -f Makefile.cognac check
```

The GitHub Actions workflow builds this target and publishes the resulting
`ntdll.dll` artifact. This target is an incremental bootstrap, not yet a
Windows 11 drop-in replacement. The existing Wine Makefile.in source set still
contains Wine/Unix integration and must be progressively ported to CognacOS;
this target does not yet compile all of that source set.

The syscall IDs are deliberately not hard-coded because NT service numbers vary
by OS build. `CognacNtdllConfigureSyscalls` must be called by a trusted,
version-aware kernel integration before NtClose or NtYieldExecution can work.
The current syscall bridge supports at most four arguments and must not be used
for services with more arguments.

Next work should fold implementations into the existing `rtl.c`, `loader.c`,
`process.c`, and other established source files and progressively replace the
Unix server/syscall layer with CognacOS kernel interfaces.

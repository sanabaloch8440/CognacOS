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

For CognacOS, leave `WINDOWS_SYSCALL_IDS=0`. The native build defaults to CognacOS service IDs `0x100` through `0x104` for Close, YieldExecution, QueryInformationProcess, QueryInformationThread, and QuerySystemInformation. A trusted loader may override these using `CognacNtdllConfigureSyscalls` for the original close/yield pair or `CognacNtdllConfigureSyscallTable` for all five services. `WINDOWS_SYSCALL_IDS=1` remains an ABI experiment only and must not be used against the CognacOS dispatcher.
The native bootstrap also exports a bounded 256 KiB per-process default heap through `RtlGetProcessHeap`, `RtlAllocateHeap`, `RtlFreeHeap`, `RtlReAllocateHeap`, `RtlSizeHeap`, and `RtlValidateHeap`; the loader points `PEB.ProcessHeap` at that image-local heap object. Custom heaps, LFH, heap callbacks/instrumentation, and the complete Windows heap contract remain unsupported. The native bootstrap currently exports `NtClose`, `NtYieldExecution`, `NtQueryInformationProcess`, `NtQueryInformationThread`, `NtQuerySystemInformation`, core RTL
memory routines (`RtlCompareMemory`, `RtlCompareMemoryUlong`, `RtlCopyMemory`,
`RtlFillMemory`, `RtlMoveMemory`, `RtlZeroMemory`), CRC32 helpers,
`RtlIsZeroMemory`, and `RtlGetVersion` (reports Windows 11 build 26100), plus ANSI/Unicode string initialization, comparison, copy, and integer-conversion routines. Case-insensitive string comparison currently folds ASCII only; full Windows Unicode upcase-table semantics remain outstanding. The
x64 bridge now marshals up to 17 arguments: the first four use the Windows syscall register convention and arguments five onward are copied into syscall-visible stack slots. `NtQueryInformationProcess` and `NtQueryInformationThread` exercise five-argument calls; `NtQuerySystemInformation` exercises four. The loader can configure the original two service numbers or provide the five-entry `CognacNtdllConfigureSyscallTable` table. The kernel implements `NtClose` through the native Object Manager close path, `NtYieldExecution` through the scheduler, and limited basic information classes for process, thread, and system queries. Unsupported information classes/handles are rejected. This is an initial supported subset, not full Windows semantics.

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

## Native execution regression gate

The UEFI/QEMU boot workflow now executes the in-tree DLL's focused ring-3
smoke test instead of treating successful mapping/export registration as enough.
The test configures the CognacOS-native service table and exercises the five
implemented NT wrappers through the custom INT 0x82 ABI. The Windows x64
SYSCALL probe remains separate because it uses a different entry convention and
build-specific service numbers. A successful smoke test is still only a narrow
bootstrap milestone: loader initialization, a complete PEB/TEB/LDR runtime,
full exception/unwind semantics, and most of NTDLL's exports remain unfinished.

## Current QEMU test coverage

The native ring-3 smoke resolves exports from the mapped DLL and calls the actual
function pointers. It covers syscall-table configuration; process, thread, and
system information wrappers; yield; ANSI/Unicode initialization and comparison;
Unicode copy; hexadecimal Unicode-string parsing; integer-to-Unicode conversion;
and the NT version/status helpers, plus the CRC32 and core RTL memory routines. The table is deliberately limited to the
services CognacOS implements today. Unsupported NT information classes and
status mappings remain explicit gaps, not successful Windows compatibility.

The expanded version/status smoke passed in the boot-smoke phase of
[Actions run 40183695385](https://github.com/umarfarooq7869/CognacOS/actions/runs/40183695385).
The subsequent run adds explicit CRC32, compare-memory, zero-memory, fill, move,
and copy tests; that expanded test run is still in progress.

## Default process heap scope

The in-tree heap is intentionally a bounded bootstrap implementation backed by
writable NTDLL image data. It supports first-fit allocation, aligned blocks,
splitting/coalescing, free, realloc, size, validation, and zero-memory
allocation. Its exported heap object is installed into the PEB before ring-3
entry. It does not yet implement custom heap creation/destruction, low-fragmentation
heap behavior, debug/instrumentation flags, exception generation, or the full
Windows heap ABI. The heap lifecycle passed in [Actions run 40184178016](https://github.com/umarfarooq7869/CognacOS/actions/runs/40184178016), including allocation, size, realloc preservation, free/double-free rejection, and zeroed reuse.

## PEB access

The native DLL now exports `RtlGetCurrentPeb` using the x64 GS-relative PEB pointer. The process launcher installs the TEB as the user GS base, and the ring-3 smoke checks that `RtlGetCurrentPeb` returns the active PEB and that its `ProcessHeap` field points at the native heap object. The first QEMU result for this accessor is pending.

# Windows 11 x64 NTDLL export comparison

## Pinned reference

- Reference source: [WinExports ntdll.dll data](https://github.com/learn-more/winexports/tree/master/data/exports/nt100_x64_26200/ntdll.dll.json) and [human-readable table](https://winexports.info/dlls/ntdll_dll/).
- Dataset: Windows 11 x64 build family 26200; PE metadata reports file version `10.0.26100.7920` (AMD64).
- The committed inventory contains **2,516 named exports**. Ordinal-only slots are not part of this name list.
- The active native C/assembly export definition is still `ntdll/exports/ntdll-x64.def`; the Rust source export surface is maintained separately until the native PE link step consumes it.

## Rust export wiring

`ntdll/exports/windows-11-x64-exports.txt` is the committed inventory. `ntdll/src/windows11_exports.rs` is now only the module index; placeholder functions are split into focused files under `ntdll/src/windows11_exports/`. The files are grouped by loader, heap, memory, registry, process, exception, time, strings, crypto, path, printf, ALPC, atom, environment, version, debug, resource, bitmap, PEB, syscall, runtime, CRT and miscellaneous responsibilities. Each group can be implemented and tested independently, while already-implemented names in the established NTDLL modules remain excluded from these placeholders.

The crate declares both `rlib` and `cdylib` outputs so symbols can be linked as a shared library for the selected target. This change is source organization only: it does not yet reduce the number of unimplemented exports, and the generic placeholders still need to be replaced subsystem by subsystem. A Windows PE32+ `ntdll.dll` still requires building for a Windows x64 target and integrating the resulting exports into the native PE packaging/link step.

The optional `ntdll/tools/update_windows11_exports.py` script refreshes the committed inventory from the WinExports data source. It is a manual maintenance tool only; ordinary builds do not need network access.

## ABI and compatibility limitations

- These placeholder functions provide **name-level resolution only**. Their generic zero-argument signatures are not ABI-compatible with the real Windows APIs and must be replaced with signature-correct declarations/implementations before calling them.
- This inventory does not fully encode ordinals, ordinal-only entries, forwarded exports, or data exports.
- A committed name list and Rust `#[no_mangle]` symbols do not alone prove that a final PE DLL export directory contains every name. Verify the produced PE with an export-table tool as part of the Windows x64 packaging tests.

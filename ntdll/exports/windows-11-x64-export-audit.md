# Windows 11 x64 NTDLL export comparison

## Pinned reference

- Reference source: [WinExports ntdll.dll data](https://github.com/learn-more/winexports/tree/master/data/exports/nt100_x64_26200/ntdll.dll.json) and [human-readable table](https://winexports.info/dlls/ntdll_dll/).
- Dataset: Windows 11 x64 build family 26200; PE metadata reports file version `10.0.26100.7920` (AMD64).
- The committed inventory contains **2,516 named exports**. Ordinal-only slots are not part of this name list.
- The active native C/assembly export definition is still `ntdll/exports/ntdll-x64.def`; the Rust source export surface is maintained separately until the native PE link step consumes it.

## Rust export wiring

`ntdll/exports/windows-11-x64-exports.txt` is a committed inventory. The corresponding Rust symbols are checked in under `ntdll/src/windows11_exports.rs` and wired through `ntdll/src/lib.rs`. Windows export names already implemented in other Rust modules are not duplicated; the remaining names have `#[no_mangle]` Rust placeholder functions returning `STATUS_NOT_IMPLEMENTED`.

The crate declares both `rlib` and `cdylib` outputs so symbols can be linked as a shared library for the selected target. A Windows PE32+ `ntdll.dll` still requires building for a Windows x64 target and integrating the resulting exports into the native PE packaging/link step.

The optional `ntdll/tools/update_windows11_exports.py` script refreshes the committed inventory from the WinExports data source. It is a manual maintenance tool only; ordinary builds do not need network access.

## ABI and compatibility limitations

- These placeholder functions provide **name-level resolution only**. Their generic zero-argument signatures are not ABI-compatible with the real Windows APIs and must be replaced with signature-correct declarations/implementations before calling them.
- This inventory does not fully encode ordinals, ordinal-only entries, forwarded exports, or data exports.
- A committed name list and Rust `#[no_mangle]` symbols do not alone prove that a final PE DLL export directory contains every name. Verify the produced PE with an export-table tool as part of the Windows x64 packaging tests.

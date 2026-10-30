# Windows DLL export baseline files

The local TSV inventories and matching .def files are self-contained build inputs. Cargo and CI read only repository files; no web request or external export database is needed to build or validate the DLLs.

The kernel32, kernelbase, and shell32 inventories are pinned to Windows 11 x64 AMD64 version 10.0.26100.7920. The msvcrt inventory is pinned independently to the Windows 11 x64 AMD64 msvcrt.dll binary version 7.0.26100.7623 (WinExports dataset key `nt100_x64_26200`). TSV columns are ordinal, export name (empty for ordinal-only exports), and forwarder target.

Run `python3 dlls/tools/verify_windows_exports.py` to check each inventory against its .def and validate the kernel32 forwarder targets. `tools/verify_dll_exports.py` compares built PE export names, ordinals, and forwarder strings against the local manifests. All builds and checks are offline.

These inventories are exact for their pinned binary snapshots, not universal across every Windows 11 servicing build. The Rust msvcrt functions currently return zero; matching exports does not mean C runtime behavior or application compatibility is implemented.

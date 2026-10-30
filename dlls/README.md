# Vendored Windows 11 x64 export baselines

- dlls/kernel32/exports/windows-11-x64-exports.tsv and kernel32-x64.def
- dlls/kernelbase/exports/windows-11-x64-exports.tsv and kernelbase-x64.def
- dlls/shell32/exports/windows-11-x64-exports.tsv and shell32-x64.def

These local files are pinned to Windows 11 x64 version 10.0.26100.7920. They contain ordinals, named and ordinal-only exports, and forwarder strings. Run python3 dlls/tools/verify_windows_exports.py to check the inventories, .def files, and KERNELBASE/NTDLL forwarder targets. No external website is queried by the build.

- dlls/msvcrt/exports/windows-11-x64-exports.tsv and msvcrt-x64.def

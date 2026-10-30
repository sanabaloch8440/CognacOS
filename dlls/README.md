# Vendored Windows 11 x64 export baselines

- dlls/kernel32/exports/windows-11-x64-exports.tsv and kernel32-x64.def
- dlls/kernelbase/exports/windows-11-x64-exports.tsv and kernelbase-x64.def
- dlls/shell32/exports/windows-11-x64-exports.tsv and shell32-x64.def

These local files are pinned to Windows 11 x64 version 10.0.26100.7920. They contain ordinals, named and ordinal-only exports, and forwarder strings. Run python3 dlls/tools/verify_windows_exports.py to check the inventories, .def files, and KERNELBASE/NTDLL forwarder targets. No external website is queried by the build.

- dlls/msvcrt/exports/windows-11-x64-exports.tsv and msvcrt-x64.def

- dlls/syssetup/exports/windows-11-x64-exports.tsv and syssetup-x64.def (11-export starter inventory; source provenance and servicing-build verification caveat are documented in dlls/syssetup/README.md).
- programs/rundll32 builds a separate rundll32.exe host; it is not a DLL and therefore has no DLL export manifest.
\n- `dlls/bcrypt` provides the pinned Windows 11 x64 bcrypt export baseline and an initial SHA-2 hashing / system RNG implementation. See `dlls/bcrypt/EXPORT_BASELINE.md` for the implemented subset and explicit gaps.\n
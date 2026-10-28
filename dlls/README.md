# CognacOS user-mode DLLs

`kernel32/` and `shell32/` are Rust crates that build x64 PE32+ DLLs. Both depend on the shared `cognacos-nt-abi` crate rather than defining duplicate NT scalar and pointer aliases. Their export inventories are checked against the matching .def files at build time.

The generated functions are placeholders; exporting a name does not mean the API is implemented. Build and validate them through the existing `.github/workflows/build-uefi.yml` workflow; no separate DLL-only workflow is needed.

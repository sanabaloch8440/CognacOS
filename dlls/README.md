# CognacOS user-mode DLLs

The `kernel32/` and `shell32/` directories contain early freestanding x64 PE32+ DLL stubs with explicit .def export tables. These are compatibility scaffolding, not complete Windows implementations. See each DLL's README for export scope and stub behavior.

Build locally with `clang` and `lld-link`, or use the `Build x64 DLL stubs` GitHub Actions workflow. Generated DLLs are workflow artifacts and are not committed into source control.

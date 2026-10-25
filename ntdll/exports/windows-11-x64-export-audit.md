# Windows 11 x64 NTDLL export comparison

## Pinned reference

- Reference source: [WinExports ntdll.dll table](https://winexports.info/dlls/ntdll_dll/)
- Reference build: Windows 11 x64 build 26100, reported file version 10.0.26100.7920 (WinExports snapshot generated October 10, 2026).
- Reference named-export count: **2,513**. The source also documents an ordinal-only export slot; named exports and total export-address-table entries are not identical.
- CognacOS's hand-maintained `ntdll/exports/ntdll-x64.def` is the active native PE export definition and is not the full reference list.

## Inventory and stub policy

`ntdll/tools/update_windows11_exports.py` refreshes the named-export inventory from WinExports into `ntdll/exports/windows-11-x64-exports.txt`. Run it when intentionally updating the reference snapshot; `--check` checks whether the generated snapshot matches the current source. The script refuses to overwrite the file if it parses fewer than 2,000 names, to reduce the risk of committing a truncated scrape.

The Rust build script reads the pinned inventory when present and generates generic ABI placeholders for names not already declared in Rust. It also unions names from the in-tree `root/wine/dlls/ntdll/ntdll.spec` as a compatibility superset. Until the pinned inventory file is generated and committed, Windows-only names absent from Wine's spec are not guaranteed to receive Rust placeholders.

## Important limitations

- This is an export-name inventory, **not** a signature/ABI database. A zero-argument placeholder returning `STATUS_NOT_IMPLEMENTED` only provides a name-level scaffold; it is not a safe implementation of the real function's signature or behavior.
- The Rust crate is currently an `rlib`; its symbols are not yet linked as a complete production PE32+ `ntdll.dll`.
- Ordinals, ordinal-only slots, forwarded exports, data exports, and exact signatures must be validated against the pinned Windows PE export directory before claiming binary-level compatibility.
- Replace placeholders with signature-correct implementations and tests as each native service is implemented.

Reference: https://winexports.info/dlls/ntdll_dll/

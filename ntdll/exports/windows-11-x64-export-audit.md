# Windows 11 x64 NTDLL export comparison

Reference checked: WinExports' Windows 11 x64 column for build 26100 (reported file version 10.0.26100.7920, generated October 10, 2026).

- Reference inventory: **2,513 named exports**; the reference also documents an ordinal-only export slot, so named exports and total export-address-table entries are not identical.
- CognacOS's hand-maintained `ntdll/exports/ntdll-x64.def` previously contained **66 named entries**.
- The Rust crate is a collection of partial ports, not a finished PE32+ `ntdll.dll`; it builds as an `rlib`. Its current Rust function inventory is not yet a complete Windows 11 export match.
- Build-time placeholder generation now fills missing names listed by the in-tree `root/wine/dlls/ntdll/ntdll.spec` with generic x64 ABI functions returning `STATUS_NOT_IMPLEMENTED`, excluding names already declared in Rust. This is a compatibility scaffold based on Wine's export inventory, **not a claim that Wine's list is identical to Windows 11 build 26100**. Windows 11-only names, ordinal-only slots, exact ordinals, forwarded exports, data exports, and signatures still require validation against a pinned Windows 11 PE export dump.
- These generic placeholders deliberately do not implement the underlying routines and should not be treated as safe functional substitutes. High-value Native API and RTL functions must be replaced with signature-correct implementations as the kernel services become available.

Reference: https://winexports.info/dlls/ntdll_dll/

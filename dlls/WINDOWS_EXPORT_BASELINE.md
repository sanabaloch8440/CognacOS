# Windows DLL export baseline files

The three local TSV inventories and matching .def files are self-contained build inputs. Cargo and CI read only repository files; no web request or external export database is needed to build or validate the DLLs.

Reference snapshot: Windows 11 x64 AMD64, file version 10.0.26100.7920. TSV columns are ordinal, export name (empty for ordinal-only exports), and forwarder target. Each .def preserves the ordinal and forwarder string from the snapshot. The offline verifier checks each table against its .def and confirms every kernel32 forwarder to KERNELBASE or NTDLL resolves to a name in the corresponding vendored inventory.

This is exact export-table parity for the pinned snapshot, not universal parity across every Windows 11 update. Direct Rust function bodies are still stubs; matching exports and forwarders does not mean Win32 API behavior is implemented.

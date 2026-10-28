# Windows DLL export reference provenance

CognacOS export manifests for kernel32.dll, kernelbase.dll, and shell32.dll are pinned to the WinExports records for Windows 11 x64 binaries version **10.0.26100.7920 (AMD64)** in the dataset folder `nt100_x64_26200`. Upstream source: [learn-more/winexports](https://github.com/learn-more/winexports/tree/master/data/exports/nt100_x64_26200). Each local TSV stores the reference ordinal, export name (empty for ordinal-only exports), and forwarder target (empty for direct exports). The corresponding .def file is checked byte-for-byte against that TSV during Cargo build.

This is a factual export-table reference. It does not copy Windows binaries or implementations. The generated Rust functions remain compatibility placeholders. Windows export tables can change across servicing builds; do not call this a universal list for all Windows 11 releases.

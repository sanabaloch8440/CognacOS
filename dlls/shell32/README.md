# CognacOS shell32.dll — Rust x64 stub crate

Builds a PE32+ DLL from Rust and uses the shared `cognacos-nt-abi` crate at `../../shared/nt-abi`. `build.rs` validates the export definition against the committed starter inventory and generates Rust placeholder exports.

- `src/lib.rs`: shared ABI types and HRESULT placeholder status.
- `build.rs`: inventory validation and Rust stub generation.
- `exports/shell32-x64.def`: explicit x64 export surface.
- `exports/windows-11-x64-exports.txt`: curated starter inventory, not an exhaustive Windows 11 export set.
- `resources/icons/`: Tango Project artwork with attribution and license notices.

Generic placeholders return zero; selected HRESULT-style APIs return `E_NOTIMPL`. Shell namespace, COM interfaces, filesystem operations, known-folder resolution, icon loading, Recycle Bin, and notifications remain unimplemented. The placeholder signatures are intentionally shallow and are not functional Win32 implementations.

```sh
cargo check --manifest-path dlls/shell32/Cargo.toml --target x86_64-unknown-linux-gnu
cargo build --manifest-path dlls/shell32/Cargo.toml --lib --target x86_64-pc-windows-msvc --release
```

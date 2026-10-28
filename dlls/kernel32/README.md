# CognacOS kernel32.dll — Rust x64 stub crate

Builds a PE32+ DLL from Rust and uses the canonical `cognacos-nt-abi` crate at `../../shared/nt-abi`. `build.rs` validates the export definition against the committed starter inventory and generates the placeholder functions.

- `src/lib.rs`: shared ABI types, pseudo-handle/last-error helpers, and generated exports.
- `build.rs`: inventory validation and Rust stub generation.
- `exports/kernel32-x64.def`: explicit x64 export surface.
- `exports/windows-11-x64-exports.txt`: curated starter inventory, not the exhaustive export set for a particular Windows 11 build.

Most placeholders return zero. `GetLastError` returns `ERROR_CALL_NOT_IMPLEMENTED`; `SetLastError` is a no-op. Generic function parameter lists and API semantics are not implemented, so these are import-resolution scaffolding only.

```sh
cargo check --manifest-path dlls/kernel32/Cargo.toml --target x86_64-unknown-linux-gnu
cargo build --manifest-path dlls/kernel32/Cargo.toml --lib --target x86_64-pc-windows-msvc --release
```

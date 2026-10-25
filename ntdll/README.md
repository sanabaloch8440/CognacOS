# CognacOS Rust NTDLL

This directory is the canonical home for the in-progress Rust NTDLL port.

## Compile

```sh
cargo check --manifest-path ntdll/Cargo.toml --all-targets
```

The CI workflow runs this check on pushes and pull requests that touch `ntdll/**`.
The existing freestanding PE32+ bootstrap and QEMU smoke test remain under
`root/wine/dlls/ntdll`; this crate does not yet replace that artifact.

## Status and limits

- Rust source modules were moved from `root/wine/dlls/ntdll/rust/` without
  rewriting their contents.
- The `relay.rs` port is retained but feature-gated because relay porting was
  previously deferred.
- Passing `cargo check` establishes Rust compilation/type-checking only. It
  does not prove that the modules can be linked together without export-symbol
  collisions, that the resulting PE exports match Windows 11, or that NTDLL
  passes QEMU/Windows runtime tests.
- The production integration phase must define a stable module ABI, resolve
  duplicate exported symbols, and link the Rust object code into the native
  PE32+ build before claiming runtime coverage.

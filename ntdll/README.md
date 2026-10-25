# CognacOS Rust NTDLL

This directory is the canonical home for the in-progress Rust NTDLL port.

## Compile

```sh
cargo check --manifest-path ntdll/Cargo.toml --all-targets --target x86_64-unknown-linux-gnu
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
- The active PE export definition is `ntdll/exports/ntdll-x64.def`; it is passed to the native linker by `root/wine/dlls/ntdll/Makefile.cognac`. The table exports named Windows-style APIs currently implemented by the native C/assembly bootstrap. Rust modules are not linked into that PE yet, so their `#[no_mangle]` symbols must not be advertised as DLL exports until Rust objects are linked and ABI-tested.
- Windows NTDLL export ordinals vary by Windows build; this project deliberately avoids assigning guessed ordinals. The export-table checker validates unique names and required core exports.

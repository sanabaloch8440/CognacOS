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

- Native Rust NTDLL modules must contain Windows ABI/runtime code only; the Wine relay-tracing port and Wine-specific `math_error` hook were removed.
- The native NTDLL crate has no Wine server, POSIX host-runtime, or Wine crate dependency. The separate Wine source tree remains under `root/wine` and is not used as an export-list source.
- Passing `cargo check` establishes Rust compilation/type-checking only. It
  does not prove that the modules can be linked together without export-symbol
  collisions, that the resulting PE exports match Windows 11, or that NTDLL
  passes QEMU/Windows runtime tests.
- The active PE export definition is `ntdll/exports/ntdll-x64.def`; it is passed to the native linker by `root/wine/dlls/ntdll/Makefile.cognac`. The pinned Windows 11 x64 inventory is the only source for generated Rust placeholder names; the Wine `.spec` must never widen CognacOS exports. A build-time audit rejects any `#[no_mangle]` Rust symbol absent from that inventory. The native `.def` file owns the public export surface, and source-level `dllexport` annotations are not used to bypass that allowlist. Rust modules are not linked into that PE yet, so their symbols must not be advertised as DLL exports until ABI-tested.
- Windows NTDLL export ordinals vary by Windows build; this project deliberately avoids assigning guessed ordinals. The export-table checker validates unique names and required core exports.

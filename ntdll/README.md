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


## Recent printf / variadic ABI bridge

`src/windows11_exports/printf.rs` contains x64 assembly entry shims and Rust helpers for the inventory-listed `_vscprintf`, `_vscwprintf`, `sprintf`, `sprintf_s`, `swprintf`, and `swprintf_s` exports. The assembly shims home the Windows x64 register arguments and provide a raw argument cursor to Rust; the helpers use the in-tree typed formatter instead of calling back into CRT formatting functions.

Wide-format strings are decoded as UTF-16, converted to UTF-8 for the existing parser, and the result is converted back to UTF-16. This preserves non-ASCII literal text, including valid surrogate pairs. Invalid UTF-16 is rejected. The supported conversion set and semantics remain limited by `src/printf.rs`; this is not full MSVC/UCRT behavior. Secure variants return an error and clear the output where possible, but do not invoke Microsoft's invalid-parameter handler. Positional arguments and additional CRT-specific behavior remain unsupported.

The assembly ABI, final PE export/link resolution, Unicode edge cases, secure-buffer behavior, and runtime calls still require dedicated tests. The latest Actions run built and launched QEMU but failed the boot smoke test, so this change is **not yet end-to-end validated**. See the [canonical roadmap](../roadmap.md) for acceptance criteria.

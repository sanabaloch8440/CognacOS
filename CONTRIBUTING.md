# Contributing to CognacOS

Thanks for helping improve CognacOS. Changes should be reviewable, reproducible,
and compatible with the project's native x86-64 ABI and boot contracts.

## Before opening a pull request

1. Open or reference an issue for non-trivial changes.
2. Keep changes focused; separate refactors from behavior changes.
3. Do not change exported names, ordinals, calling conventions, or PE layouts
   without updating the versioned ABI manifests and compatibility tests.
4. Do not add compiler or linker overrides to individual packages or scripts.
   The supported toolchain is defined in `rust-toolchain.toml`,
   `.cargo/config.toml`, and `scripts/build.sh`.
5. Do not add first-party C/C++ source. New OS and tooling code is Rust.
6. Preserve DWARF/PDB information and the ELF debug path used for GDB source
   stepping. Do not replace useful debug symbols with diagnostic print spam.

## Local validation

Run the same entry point used by contributors and CI:

```sh
./scripts/build.sh setup
./scripts/build.sh fmt-check
./scripts/build.sh lint
./scripts/build.sh check
./scripts/build.sh test
```

For a release kernel image:

```sh
./scripts/build.sh build
```

A successful compile is not proof that a kernel change boots. Changes to the
loader, memory manager, interrupts, drivers, or PE loader must include the
relevant QEMU smoke-test evidence. Keep failure logs and exact workflow links
in the pull request.

## ABI and compatibility changes

- Treat export manifests and `.def` files as reviewed source, not generated
  build artifacts.
- Preserve ordinal-only exports and forwarders.
- Add regression fixtures for malformed PE/ELF input and boundary conditions.
- Avoid silently widening unsupported API behavior; return the documented
  NTSTATUS when an operation is not implemented.
- Keep host-side tests deterministic and safe to run concurrently.

## Commit and review hygiene

Use concise imperative commit subjects. Explain the reason for non-obvious
changes and list validation performed. Do not include generated `target/`,
`build/`, QEMU disk images, or runtime logs in source commits.

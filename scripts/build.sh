#!/usr/bin/env bash
# CognacOS reproducible Rust/LLVM toolchain entry point.
# Rust is the sole project-language compiler. LLVM LLD is the sole linker.
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

die() { printf 'cognacos-build: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "required tool not found: $1"; }

setup_toolchain() {
  need rustc
  need cargo
  need rustup
  need clang
  need ld.lld

  local sysroot lld_bin shim
  sysroot="$(rustc --print sysroot)"
  lld_bin="$sysroot/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld"
  [[ -x "$lld_bin" ]] || die "Rust LLVM LLD binary missing: $lld_bin"

  shim="${CARGO_TARGET_DIR:-$ROOT/target}/cognacos-toolchain/bin"
  mkdir -p "$shim"
  ln -sfn "$lld_bin" "$shim/rust-lld"
  ln -sfn "$lld_bin" "$shim/lld-link"
  ln -sfn "$lld_bin" "$shim/ld.lld"
  export PATH="$shim:$PATH"
  unset CC CXX AR
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=lld"
  export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=lld-link
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=clang
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=--target=x86_64-w64-windows-gnu -C link-arg=-fuse-ld=lld"
  export CARGO_TARGET_X86_64_UNKNOWN_NONE_LINKER=rust-lld
  export CARGO_TARGET_X86_64_UNKNOWN_UEFI_LINKER=rust-lld
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

  # Persist the same linker selection for subsequent GitHub Actions steps.
  if [[ -n "${GITHUB_PATH:-}" ]]; then printf '%s\n' "$shim" >> "$GITHUB_PATH"; fi
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    {
      printf 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang\n'
      printf 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=-C link-arg=-fuse-ld=lld\n'
      printf 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=lld-link\n'
      printf 'CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=clang\n'
      printf 'CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS=-C link-arg=--target=x86_64-w64-windows-gnu -C link-arg=-fuse-ld=lld\n'
      printf 'CARGO_TARGET_X86_64_UNKNOWN_NONE_LINKER=rust-lld\n'
      printf 'CARGO_TARGET_X86_64_UNKNOWN_UEFI_LINKER=rust-lld\n'
      printf 'CARGO_TARGET_DIR=%s\n' "$CARGO_TARGET_DIR"
    } >> "$GITHUB_ENV"
  fi
}

install_targets() {
  rustup target add x86_64-unknown-linux-gnu x86_64-unknown-none x86_64-unknown-uefi x86_64-pc-windows-msvc x86_64-pc-windows-gnu
}

check_toolchain() {
  printf '%s\n' '=== CognacOS compiler/linker policy ==='
  rustc --version --verbose
  cargo --version
  clang --version | head -n 1
  ld.lld --version
  [[ "$(readlink -f "$(command -v rust-lld)")" == "$(readlink -f "$(command -v ld.lld)")" ]] || die "ELF and COFF linkers do not resolve to the same LLD binary"
  [[ "$(readlink -f "$(command -v lld-link)")" == "$(readlink -f "$(command -v rust-lld)")" ]] || die "lld-link does not resolve to the same LLD binary"
  printf 'Rust compiler: rustc (all project Rust sources)\n'
  printf 'Linker: LLVM LLD (ELF, UEFI/COFF and PE/COFF driver modes)\n'
  printf 'C dependency compiler: Clang (third-party C dependencies only)\n'
  printf 'Cargo target directory: %s\n' "$CARGO_TARGET_DIR"
  if find . -type f \( -name '*.c' -o -name '*.cc' -o -name '*.cpp' -o -name '*.cxx' \) -not -path './target/*' -not -path './.git/*' -print -quit | grep -q .; then
    printf '%s\n' 'C/C++ sources exist; they are compiled with Clang, never GCC.'
  fi
  if command -v gcc >/dev/null 2>&1 && [[ "${COGNACOS_ALLOW_GCC:-0}" != 1 ]]; then
    printf '%s\n' 'Note: GCC is installed on this host but is not selected by the CognacOS build.' 
  fi
}

run_checks() {
  cargo run --target x86_64-unknown-linux-gnu --manifest-path tools/rust/Cargo.toml -- validate-ntoskrnl-manifest
  cargo run --target x86_64-unknown-linux-gnu --manifest-path tools/rust/Cargo.toml -- check-ntdll-def
  cargo run --target x86_64-unknown-linux-gnu --manifest-path tools/rust/Cargo.toml -- check-csrsrv-def
  cargo check --manifest-path shared/nt-abi/Cargo.toml --target x86_64-unknown-linux-gnu --all-targets
  cargo check --manifest-path kernel/Cargo.toml --target x86_64-unknown-none
  cargo check --manifest-path bootloader/Cargo.toml --target x86_64-unknown-uefi
  cargo check --manifest-path ntdll/Cargo.toml --target x86_64-unknown-linux-gnu --all-targets
  cargo check --manifest-path drivers/hal/Cargo.toml --target x86_64-unknown-linux-gnu
  cargo check --manifest-path drivers/acpi/Cargo.toml --target x86_64-unknown-linux-gnu
  cargo check --manifest-path drivers/pci/Cargo.toml --target x86_64-unknown-linux-gnu
  cargo check --manifest-path drivers/ext4/Cargo.toml --target x86_64-unknown-linux-gnu
  cargo check --manifest-path drivers/fat32/Cargo.toml --target x86_64-unknown-linux-gnu
  cargo check --manifest-path drivers/storage/Cargo.toml --target x86_64-unknown-linux-gnu
}

format_sources() {
  while IFS= read -r manifest; do
    cargo fmt --manifest-path "$manifest" --all
  done < <(find . -name Cargo.toml -not -path './target/*' -not -path './.git/*' -print | sort)
}

check_format() {
  while IFS= read -r manifest; do
    cargo fmt --manifest-path "$manifest" --all -- --check
  done < <(find . -name Cargo.toml -not -path './target/*' -not -path './.git/*' -print | sort)
}

lint_core() {
  cargo clippy --manifest-path shared/nt-abi/Cargo.toml --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
  cargo clippy --manifest-path tools/rust/Cargo.toml --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
  cargo clippy --manifest-path drivers/acpi/Cargo.toml --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
  cargo clippy --manifest-path drivers/pci/Cargo.toml --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
}

run_tests() {
  cargo test --manifest-path shared/nt-abi/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path dlls/bcrypt/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/acpi/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/pci/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/fat32/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/storage/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/kbdclass/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/kbdus/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/hidclass/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/kbdhid/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/beep/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/i8042prt/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/usbxhci/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path drivers/usbhub3/Cargo.toml --target x86_64-unknown-linux-gnu --lib
  cargo test --manifest-path tools/rust/Cargo.toml --target x86_64-unknown-linux-gnu
}

case "${1:-help}" in
  setup)
    setup_toolchain
    install_targets
    check_toolchain
    ;;
  check)
    setup_toolchain
    run_checks
    ;;
  test)
    setup_toolchain
    run_tests
    ;;
  format)
    setup_toolchain
    format_sources
    ;;
  fmt-check)
    setup_toolchain
    check_format
    ;;
  lint)
    setup_toolchain
    lint_core
    ;;
  all)
    setup_toolchain
    run_checks
    run_tests
    ;;
  toolchain)
    setup_toolchain
    check_toolchain
    ;;
  *)
    cat <<'USAGE'
Usage: scripts/build.sh {setup|toolchain|format|fmt-check|lint|check|test|all}
  setup      Install supported Rust targets and verify the Rust/LLVM toolchain.
  toolchain  Verify compiler/linker selection without building.
  format     Format all Rust packages using the pinned rustfmt.
  fmt-check  Verify formatting without modifying files.
  lint       Run strict Clippy checks on the core libraries and tools.
  check      Run core repository, kernel, UEFI and driver compile checks.
  test       Run host-side ABI, driver and Rust tooling tests.
  all        Run check followed by test.
Set CARGO_TARGET_DIR to override the default target/ directory.
USAGE
    ;;

esac

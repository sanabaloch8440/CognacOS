#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
OUT="$ROOT/build"
mkdir -p "$OUT"
CLANG="${CLANG:-clang}"
LLD_LINK="${LLD_LINK:-lld-link}"
"$CLANG" --target=x86_64-pc-windows-msvc -c -O2 -ffreestanding -fno-builtin -fno-stack-protector -fno-ident "$ROOT/src/stubs.c" -o "$OUT/stubs.obj"
"$LLD_LINK" /dll /noentry /machine:x64 /nodefaultlib /def:"$ROOT/exports/kernel32-x64.def" /out:"$OUT/kernel32.dll" "$OUT/stubs.obj"
printf 'Built %s\n' "$OUT/kernel32.dll"

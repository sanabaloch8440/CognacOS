#!/usr/bin/env python3
"""Generate build/ntdll/ntdll-rust.def from the pinned Windows 11 x64 inventory."""
from pathlib import Path
import re

names = []
for raw in Path("ntdll/exports/windows-11-x64-exports.txt").read_text().splitlines():
    name = raw.strip()
    if not name or name.startswith("#"):
        continue
    if not re.fullmatch(r"[A-Za-z0-9_]+", name):
        raise SystemExit(f"invalid NTDLL export name: {name!r}")
    names.append(name)

if len(names) < 2000 or len(names) != len(set(names)):
    raise SystemExit(
        f"unexpected Windows NTDLL inventory: {len(names)} unique={len(set(names))}"
    )

out = "LIBRARY ntdll\nEXPORTS\n" + "".join(f"    {n}\n" for n in names)
Path("build/ntdll/ntdll-rust.def").write_text(out)
print(f"Generated Rust NTDLL definition with {len(names)} Windows 11 exports")

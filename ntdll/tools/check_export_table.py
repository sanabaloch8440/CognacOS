#!/usr/bin/env python3
"""Validate CognacOS's named x64 NTDLL export contract before Rust compilation."""
from pathlib import Path
import sys

root = Path(__file__).resolve().parents[2]
def_file = root / "ntdll" / "exports" / "ntdll-x64.def"
text = def_file.read_text(encoding="utf-8")
lines = [line.split(";", 1)[0].strip() for line in text.splitlines()]
if "LIBRARY ntdll" not in lines:
    sys.exit("export table must declare LIBRARY ntdll")
if "EXPORTS" not in lines:
    sys.exit("export table is missing EXPORTS")

exports = []
for line in lines:
    if not line or line.upper() in {"LIBRARY NTDLL", "EXPORTS"}:
        continue
    name = line.split()[0]
    if name.startswith("@"):
        continue
    exports.append(name)

duplicates = sorted({name for name in exports if exports.count(name) > 1})
if duplicates:
    sys.exit("duplicate exports: " + ", ".join(duplicates))
required = {
    "NtClose", "NtOpenKey", "NtQueryValueKey", "NtYieldExecution",
    "NtQueryInformationProcess", "NtQueryInformationThread",
    "NtQuerySystemInformation", "RtlGetVersion", "RtlInitUnicodeString",
    "RtlAllocateHeap", "LdrGetProcedureAddress",
}
missing = sorted(required - set(exports))
if missing:
    sys.exit("missing required named exports: " + ", ".join(missing))
print(f"validated {len(exports)} unique named NTDLL exports")

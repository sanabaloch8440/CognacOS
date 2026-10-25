#!/usr/bin/env python3
"""Refresh/check the committed Windows 11 x64 NTDLL export-name inventory.

The build consumes the committed file and never needs network access. This
script is a manual maintenance helper only.

Usage:
  python3 ntdll/tools/update_windows11_exports.py
  python3 ntdll/tools/update_windows11_exports.py --check
"""
import json
from pathlib import Path
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
URL = "https://raw.githubusercontent.com/learn-more/winexports/master/data/exports/nt100_x64_26200/ntdll.dll.json"
OUT = ROOT / "ntdll" / "exports" / "windows-11-x64-exports.txt"
BUILD_MARKER = "nt100_x64_26200"

def render():
    request = urllib.request.Request(URL, headers={"User-Agent": "CognacOS-export-audit/1.0"})
    with urllib.request.urlopen(request, timeout=30) as response:
        data = json.load(response)
    names = {
        entry["name"]
        for entry in data.get("exports", [])
        if isinstance(entry.get("name"), str) and entry["name"]
    }
    if len(names) < 2000:
        raise SystemExit(f"Refusing suspiciously small inventory ({len(names)} names)")
    version = data.get("FileVersion", "unknown")
    result = [
        "# Windows 11 x64 ntdll.dll named exports",
        f"# Reference: {URL}",
        f"# Reference dataset: {BUILD_MARKER}; PE file version {version} (AMD64).",
        f"# Named exports only: {len(names)} entries. Ordinal-only slots are not represented.",
        "# Inventory only; this is not an ABI-signature database.",
        *sorted(names, key=lambda name: (name.casefold(), name)),
        "",
    ]
    return "\n".join(result), len(names)

def main():
    check = "--check" in sys.argv[1:]
    result, count = render()
    if check:
        if not OUT.exists() or OUT.read_text(encoding="utf-8") != result:
            raise SystemExit("Windows 11 export inventory is stale; run update_windows11_exports.py")
        print(f"Windows 11 NTDLL inventory is current ({count} named exports)")
    else:
        OUT.write_text(result, encoding="utf-8")
        print(f"Wrote {count} named exports to {OUT}")

if __name__ == "__main__":
    main()

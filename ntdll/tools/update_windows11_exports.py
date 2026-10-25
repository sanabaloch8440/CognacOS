#!/usr/bin/env python3
"""Refresh the pinned Windows 11 x64 NTDLL export-name inventory.

Source: WinExports' ntdll.dll table. The selected column is Windows 11
24H2 x64 (build 26100); this intentionally stores named exports only.
Ordinals and ordinal-only slots are not guessed or synthesized.

Usage:
  python3 ntdll/tools/update_windows11_exports.py
  python3 ntdll/tools/update_windows11_exports.py --check
"""
from html.parser import HTMLParser
from pathlib import Path
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
URL = "https://winexports.info/dlls/ntdll_dll/"
OUT = ROOT / "ntdll" / "exports" / "windows-11-x64-exports.txt"
BUILD_MARKER = "10.0.26100.7920"

class TableParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.rows = []
        self.row = []
        self.cell = None
        self.in_table = False
        self.table_depth = 0

    def handle_starttag(self, tag, attrs):
        if tag == "table":
            self.table_depth += 1
            self.in_table = True
        elif self.in_table and tag == "tr":
            self.row = []
        elif self.in_table and tag in ("td", "th"):
            self.cell = []

    def handle_data(self, data):
        if self.in_table and self.cell is not None:
            self.cell.append(data)

    def handle_endtag(self, tag):
        if self.in_table and tag in ("td", "th") and self.cell is not None:
            self.row.append(" ".join("".join(self.cell).split()))
            self.cell = None
        elif self.in_table and tag == "tr":
            if self.row:
                self.rows.append(self.row)
        elif tag == "table" and self.table_depth:
            self.table_depth -= 1
            if self.table_depth == 0:
                self.in_table = False

def main():
    check = "--check" in sys.argv[1:]
    request = urllib.request.Request(URL, headers={"User-Agent": "CognacOS-export-audit/1.0"})
    with urllib.request.urlopen(request, timeout=30) as response:
        page = response.read().decode("utf-8", errors="replace")
    parser = TableParser()
    parser.feed(page)

    # WinExports' table has the export name followed by version columns.
    # Select rows with a numeric ordinal in the final (26100 x64) column.
    names = set()
    for row in parser.rows:
        if len(row) < 2:
            continue
        name = row[0].strip()
        if not name or name.lower() in {"export name", "fileversion", "filedescription"}:
            continue
        if not (name[0].isalpha() or name[0] == "_"):
            continue
        # The last column is the newest Windows 11 x64 column on this source.
        if row[-1].strip().isdigit():
            names.add(name)
    if len(names) < 2000:
        sys.exit(f"Refusing to write suspiciously small inventory ({len(names)} names); source layout may have changed.")
    result = (
        "# Windows 11 x64 ntdll.dll named exports\n"
        f"# Reference: {URL}\n"
        f"# Reference build: {BUILD_MARKER} (WinExports snapshot; refresh source may advance)\n"
        f"# Named exports only: {len(names)} entries. Ordinal-only slots are not represented.\n"
        "# This is an inventory, not an ABI-signature database. Do not infer signatures from names.\n"
        + "".join(name + "\n" for name in sorted(names, key=str.casefold))
    )
    if check:
        if not OUT.exists() or OUT.read_text(encoding="utf-8") != result:
            sys.exit("Windows 11 export inventory is stale; run update_windows11_exports.py")
        print(f"Windows 11 NTDLL inventory is current ({len(names)} named exports)")
    else:
        OUT.write_text(result, encoding="utf-8")
        print(f"Wrote {len(names)} named exports to {OUT}")

if __name__ == "__main__":
    main()

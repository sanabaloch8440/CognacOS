#!/usr/bin/env python3
"""Validate the checked-in NTOSKRNL stub coverage manifest."""
import json
from pathlib import Path

p = Path(__file__).resolve().parents[1] / "kernel/src/ntoskrnl/manifest_19041.json"
m = json.loads(p.read_text(encoding="utf-8"))
e = m["exports"]
assert len(e) == m["counts"]["eat_entries"] == 3093
assert sum(x["name"] is not None for x in e) == m["counts"]["named_exports"] == 3070
assert sum(x["name"] is None for x in e) == m["counts"]["ordinal_only_exports"] == 23
assert [x["ordinal"] for x in e] == list(range(1, len(e) + 1))
names = [x["name"] for x in e if x["name"] is not None]
assert len(names) == len(set(names)), "duplicate named exports"
assert all(x["kind"] == "unknown" for x in e), "source has no kind classification"
assert m["source"]["version"].startswith("10.0.19041.")
print(f"validated {len(e)} EAT slots: {len(names)} named, {len(e)-len(names)} ordinal-only")

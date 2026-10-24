#!/usr/bin/env python3
"""Validate that every PE export is an exact name/ordinal from the pinned Windows manifest."""
import argparse
import json
import struct
import sys


def u16(b, o): return struct.unpack_from("<H", b, o)[0]
def u32(b, o): return struct.unpack_from("<I", b, o)[0]


def cstring(b, o):
    end = b.find(b"\\0", o)
    if end < 0: raise ValueError("unterminated PE export string")
    return b[o:end].decode("ascii")


def rva_to_offset(blob, rva, sections, headers_size):
    if rva < headers_size:
        if rva < len(blob): return rva
        raise ValueError("header RVA outside file")
    for va, virtual_size, raw_size, raw_offset in sections:
        if va <= rva < va + max(virtual_size, raw_size):
            delta = rva - va
            if delta >= raw_size or raw_offset + delta >= len(blob):
                raise ValueError(f"RVA 0x{rva:x} has no file backing")
            return raw_offset + delta
    raise ValueError(f"RVA 0x{rva:x} does not map to a PE section")


def parse_exports(blob):
    if blob[:2] != b"MZ": raise ValueError("missing MZ signature")
    pe = u32(blob, 0x3c)
    if blob[pe:pe + 4] != b"PE\\0\\0": raise ValueError("missing PE signature")
    if u16(blob, pe + 4) != 0x8664: raise ValueError("not AMD64")
    if u16(blob, pe + 24) != 0x20b: raise ValueError("not PE32+")
    section_count = u16(blob, pe + 6)
    opt = pe + 24
    opt_size = u16(blob, pe + 20)
    headers_size = u32(blob, opt + 60)
    sections = []
    shoff = opt + opt_size
    for i in range(section_count):
        o = shoff + i * 40
        virtual_size, va, raw_size, raw_off = struct.unpack_from("<IIII", blob, o + 8)
        sections.append((va, virtual_size, raw_size, raw_off))
    export_rva, export_size = struct.unpack_from("<II", blob, opt + 112)
    if export_rva == 0 or export_size < 40:
        return {}
    eo = rva_to_offset(blob, export_rva, sections, headers_size)
    (_chars, _stamp, _major, _minor, _dll_name, base, nfunc, nname,
     eat_rva, names_rva, ords_rva) = struct.unpack_from("<IIHHIIIIIII", blob, eo)
    eat = rva_to_offset(blob, eat_rva, sections, headers_size)
    names = rva_to_offset(blob, names_rva, sections, headers_size)
    ords = rva_to_offset(blob, ords_rva, sections, headers_size)
    result = {}
    for i in range(nname):
        name_rva = u32(blob, names + i * 4)
        name = cstring(blob, rva_to_offset(blob, name_rva, sections, headers_size))
        index = u16(blob, ords + i * 2)
        if index >= nfunc: raise ValueError(f"bad ordinal index for {name}")
        ordinal = base + index
        if u32(blob, eat + index * 4) == 0: raise ValueError(f"{name} points to an empty EAT slot")
        if name in result: raise ValueError(f"duplicate export name: {name}")
        result[name] = ordinal
    return result


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pe")
    ap.add_argument("manifest")
    ap.add_argument("--require", action="append", default=[])
    args = ap.parse_args()
    try:
        with open(args.pe, "rb") as f: pe = f.read()
        with open(args.manifest, "r", encoding="utf-8") as f: manifest = json.load(f)
        expected = {x["name"]: x["ordinal"] for x in manifest["exports"]
                    if x.get("name") is not None}
        actual = parse_exports(pe)
        extras = sorted(set(actual) - set(expected))
        mismatches = sorted((name, ordinal, expected.get(name))
                            for name, ordinal in actual.items()
                            if expected.get(name) != ordinal)
        missing = sorted(set(args.require) - set(actual))
        if extras or mismatches or missing:
            if extras: print("Non-Windows export names:", ", ".join(extras), file=sys.stderr)
            if mismatches: print("Wrong/missing Windows ordinals:", mismatches, file=sys.stderr)
            if missing: print("Required implemented exports missing:", ", ".join(missing), file=sys.stderr)
            return 1
        print(f"Validated {len(actual)} PE exports: all names and ordinals are in {manifest['source']['version']}.")
        return 0
    except (OSError, ValueError, KeyError, IndexError, struct.error, UnicodeDecodeError) as e:
        print(f"PE export validation failed: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

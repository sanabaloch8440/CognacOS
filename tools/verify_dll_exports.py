#!/usr/bin/env python3
"""Verify CognacOS PE exports against the pinned Windows 11 x64 export TSV."""
import argparse
import struct
import sys
from pathlib import Path

def read_manifest(path):
    expected = {}
    for line_no, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 3:
            raise ValueError(f"{path}:{line_no}: expected ordinal<TAB>name<TAB>forwarder")
        ordinal = int(fields[0])
        if ordinal in expected:
            raise ValueError(f"{path}:{line_no}: duplicate ordinal {ordinal}")
        expected[ordinal] = (fields[1] or None, fields[2] or None)
    return expected

def parse_pe_exports(path):
    data = Path(path).read_bytes()
    if data[:2] != b"MZ":
        raise ValueError(f"{path}: missing DOS signature")
    pe = struct.unpack_from("<I", data, 0x3c)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError(f"{path}: missing PE signature")
    machine, sections_count = struct.unpack_from("<HH", data, pe + 4)
    if machine != 0x8664:
        raise ValueError(f"{path}: expected AMD64 image, machine=0x{machine:04x}")
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    opt = pe + 24
    if struct.unpack_from("<H", data, opt)[0] != 0x20b:
        raise ValueError(f"{path}: expected PE32+ optional header")
    export_rva, export_size = struct.unpack_from("<II", data, opt + 112)
    if not export_rva or not export_size:
        raise ValueError(f"{path}: no export directory")
    section_off = opt + optional_size
    sections = []
    for i in range(sections_count):
        off = section_off + i * 40
        name, vsize, va, raw_size, raw_ptr = struct.unpack_from("<8sIIII", data, off)
        sections.append((va, max(vsize, raw_size), raw_size, raw_ptr, name.rstrip(b"\0").decode("ascii", "replace")))
    def rva_offset(rva):
        for va, span, raw_size, raw_ptr, name in sections:
            if va <= rva < va + span:
                delta = rva - va
                if delta >= raw_size:
                    raise ValueError(f"{path}: RVA 0x{rva:x} points to unbacked bytes in {name}")
                return raw_ptr + delta
        size_headers = struct.unpack_from("<I", data, opt + 60)[0]
        if 0 <= rva < size_headers:
            return rva
        raise ValueError(f"{path}: RVA 0x{rva:x} is outside mapped sections")
    def cstring(rva):
        off = rva_offset(rva)
        end = data.find(b"\0", off)
        if end < 0:
            raise ValueError(f"{path}: unterminated export string at RVA 0x{rva:x}")
        return data[off:end].decode("ascii")
    exp_off = rva_offset(export_rva)
    (_, _, _, _, dll_name_rva, ordinal_base, function_count, name_count,
     functions_rva, names_rva, ordinals_rva) = struct.unpack_from("<IIHHIIIIIII", data, exp_off)
    function_off = rva_offset(functions_rva)
    name_ptr_off = rva_offset(names_rva) if name_count else 0
    ord_off = rva_offset(ordinals_rva) if name_count else 0
    names_by_index = {}
    for i in range(name_count):
        name_rva = struct.unpack_from("<I", data, name_ptr_off + i * 4)[0]
        index = struct.unpack_from("<H", data, ord_off + i * 2)[0]
        names_by_index.setdefault(index, []).append(cstring(name_rva))
    actual = {}
    for index in range(function_count):
        function_rva = struct.unpack_from("<I", data, function_off + index * 4)[0]
        if function_rva == 0:
            continue
        ordinal = ordinal_base + index
        names = names_by_index.get(index, [])
        if len(names) > 1:
            raise ValueError(f"{path}: ordinal {ordinal} has multiple names {names}; manifest format needs alias support")
        name = names[0] if names else None
        forwarder = None
        if export_rva <= function_rva < export_rva + export_size:
            forwarder = cstring(function_rva)
        actual[ordinal] = (name, forwarder)
    return actual

def compare(dll, pe_path, manifest_path):
    expected = read_manifest(manifest_path)
    actual = parse_pe_exports(pe_path)
    missing = sorted(set(expected) - set(actual))
    extra = sorted(set(actual) - set(expected))
    mismatched = [(o, expected[o], actual[o]) for o in sorted(set(expected) & set(actual)) if expected[o] != actual[o]]
    if missing or extra or mismatched:
        print(f"FAIL {dll}: reference={len(expected)} PE={len(actual)} missing={len(missing)} extra={len(extra)} mismatched={len(mismatched)}")
        for o in missing[:30]: print(f"  missing ordinal {o}: expected {expected[o]}")
        for o in extra[:30]: print(f"  extra ordinal {o}: actual {actual[o]}")
        for o, e, a in mismatched[:50]: print(f"  ordinal {o}: expected {e!r}, actual {a!r}")
        return False
    forwarders = sum(1 for _, f in actual.values() if f)
    ordinal_only = sum(1 for n, _ in actual.values() if n is None)
    print(f"PASS {dll}: {len(actual)} exports, {forwarders} forwarders, {ordinal_only} ordinal-only exports match names/ordinals/forwarders exactly")
    return True

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--dll", action="append", required=True, metavar="NAME=PE,TSV",
                   help="DLL logical name, built PE path, and manifest path separated by commas")
    args = p.parse_args()
    ok = True
    for spec in args.dll:
        name, pe, manifest = spec.split(",", 2)
        ok = compare(name, pe, manifest) and ok
    return 0 if ok else 1

if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, struct.error) as exc:
        print(f"export verification error: {exc}", file=sys.stderr)
        raise SystemExit(2)

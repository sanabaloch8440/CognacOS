#!/usr/bin/env python3
"""Compile-time PE helper for CognacOS NTDLL syscall ABI selection.

Reads an x86-64 Windows ntdll.dll without loading/executing it. It extracts
export names and recognizes the canonical Windows x64 Nt/Zw syscall stub:
    4c 8b d1 b8 <service-id little-endian> 0f 05 c3
The generated header is a build input, never a runtime dependency.

This is intentionally not a DLL injector/installer and never modifies the
Windows system directory. Syscall IDs are tied to the exact input PE build.
"""
from __future__ import annotations

import argparse
import pathlib
import struct
import sys

class PEError(ValueError):
    pass

def u16(data: bytes, off: int) -> int:
    if off < 0 or off + 2 > len(data):
        raise PEError("truncated PE field")
    return struct.unpack_from("<H", data, off)[0]

def u32(data: bytes, off: int) -> int:
    if off < 0 or off + 4 > len(data):
        raise PEError("truncated PE field")
    return struct.unpack_from("<I", data, off)[0]

def parse_pe(data: bytes):
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise PEError("input is not an MZ/PE image")
    pe = u32(data, 0x3c)
    if data[pe:pe + 4] != b"PE\0\0":
        raise PEError("missing PE signature")
    coff = pe + 4
    sections_count = u16(data, coff + 2)
    optional_size = u16(data, coff + 16)
    opt = coff + 20
    if u16(data, opt) != 0x20b:
        raise PEError("input is not PE32+ (AMD64)")
    if u16(data, coff) != 0x8664:
        raise PEError("input machine is not AMD64")
    image_base = struct.unpack_from("<Q", data, opt + 24)[0]
    directories = opt + 112
    export_rva = u32(data, directories)
    export_size = u32(data, directories + 4)
    section_table = opt + optional_size
    sections = []
    for i in range(sections_count):
        off = section_table + i * 40
        virtual_size = u32(data, off + 8)
        virtual_address = u32(data, off + 12)
        raw_size = u32(data, off + 16)
        raw_pointer = u32(data, off + 20)
        sections.append((virtual_address, max(virtual_size, raw_size), raw_pointer, raw_size))
    def rva_offset(rva: int) -> int:
        for va, span, raw, raw_size in sections:
            if va <= rva < va + span:
                delta = rva - va
                if delta >= raw_size:
                    raise PEError("RVA points to unbacked section data")
                return raw + delta
        if rva < len(data):
            return rva
        raise PEError(f"RVA 0x{rva:x} is outside image sections")
    def cstring(rva: int) -> str:
        off = rva_offset(rva)
        end = data.find(b"\0", off)
        if end < 0:
            raise PEError("unterminated export name")
        return data[off:end].decode("ascii", "strict")
    if not export_rva or not export_size:
        raise PEError("PE image has no export directory")
    exp = rva_offset(export_rva)
    nfunc = u32(data, exp + 20)
    nname = u32(data, exp + 24)
    funcs_rva = u32(data, exp + 28)
    names_rva = u32(data, exp + 32)
    ords_rva = u32(data, exp + 36)
    funcs_off, names_off, ords_off = map(rva_offset, (funcs_rva, names_rva, ords_rva))
    exports = {}
    for i in range(nname):
        name = cstring(u32(data, names_off + i * 4))
        ordinal_index = u16(data, ords_off + i * 2)
        if ordinal_index >= nfunc:
            raise PEError("export ordinal index exceeds function table")
        function_rva = u32(data, funcs_off + ordinal_index * 4)
        # Forwarded exports point back into the export directory.
        if export_rva <= function_rva < export_rva + export_size:
            continue
        exports[name] = rva_offset(function_rva)
    return exports


def extract_syscalls(data: bytes, exports: dict[str, int], required=()):
    """Return (name, macro, service-id) rows for recognized syscall stubs."""
    rows = []
    for name in sorted(exports):
        if not (name.startswith("Nt") or name.startswith("Zw")):
            continue
        off = exports[name]
        # Windows 11 may insert a shared-user-data test between the
        # service ID and syscall. Accept the canonical prefix and require
        # a syscall instruction in the short stub window.
        if data[off:off + 3] != bytes.fromhex("4c 8b d1") or data[off + 3:off + 4] != b"\xb8":
            continue
        stub_window = data[off + 8:off + 32]
        if bytes.fromhex("0f 05") not in stub_window:
            continue
        service = u32(data, off + 4)
        macro = "".join(ch.upper() if ch.isalnum() else "_" for ch in name)
        rows.append((name, macro, service))
    stubbed_names = {name for name, _, _ in rows}
    missing_stubs = [name for name in required if name not in stubbed_names]
    if missing_stubs:
        raise PEError("required exports are not recognized syscall stubs: " + ", ".join(missing_stubs))
    if not rows:
        raise PEError("no canonical x64 Nt/Zw syscall stubs found; refusing to guess")
    return rows

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=pathlib.Path,
                        help="exact x64 Windows 11 ntdll.dll PE image")
    parser.add_argument("--output", required=True, type=pathlib.Path,
                        help="generated C header path")
    parser.add_argument("--require", action="append", default=[],
                        help="required Nt/Zw export; may be repeated")
    args = parser.parse_args()
    try:
        data = args.input.read_bytes()
        exports = parse_pe(data)
        missing = [name for name in args.require if name not in exports]
        if missing:
            raise PEError("missing required exports: " + ", ".join(missing))
        rows = extract_syscalls(data, exports, args.require)
        lines = [
            "/* Generated by tools/ntdll_link_helper.py from the exact input PE.",
            " * Do not edit manually; syscall IDs vary by Windows build.",
            " */",
            "#ifndef COGNAC_NT_SYSCALLS_GENERATED_H",
            "#define COGNAC_NT_SYSCALLS_GENERATED_H",
            f"#define COGNAC_NT_SYSCALL_SOURCE_COUNT {len(rows)}",
        ]
        lines.extend(f"#define COGNAC_SYSCALL_{macro} 0x{service:03x}u" for _, macro, service in rows)
        lines.extend(["#endif", ""])
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text("\n".join(lines), encoding="ascii")
        print(f"generated {args.output}: {len(rows)} syscall IDs from {args.input}")
        print("WARNING: use only with the exact Windows build matching this PE; do not ship these IDs as universal constants.")
        return 0
    except (OSError, PEError, UnicodeError, struct.error) as exc:
        print(f"ntdll-link-helper: error: {exc}", file=sys.stderr)
        return 2

if __name__ == "__main__":
    raise SystemExit(main())

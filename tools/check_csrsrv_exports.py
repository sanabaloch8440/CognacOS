#!/usr/bin/env python3
"""Check CSRSRV export names/ordinals; optional comparison to a real PE image."""
from __future__ import annotations
import argparse
import struct
import sys
from pathlib import Path

DEF = Path(__file__).resolve().parents[1] / "csrss" / "exports" / "csrsrv-x64.def"

def expected_exports(path: Path) -> dict[str, int]:
    result: dict[str, int] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.split(";", 1)[0].strip()
        if not line or line.upper().startswith(("LIBRARY ", "EXPORTS")):
            continue
        fields = line.split()
        name = fields[0]
        ordinal = next((int(x[1:]) for x in fields[1:] if x.startswith("@") and x[1:].isdigit()), None)
        if ordinal is None:
            raise ValueError(f"missing ordinal for {name}")
        if name in result:
            raise ValueError(f"duplicate export {name}")
        result[name] = ordinal
    return result

def u16(b: bytes, o: int) -> int:
    return struct.unpack_from("<H", b, o)[0]
def u32(b: bytes, o: int) -> int:
    return struct.unpack_from("<I", b, o)[0]

def actual_exports(path: Path) -> dict[str, int]:
    b = path.read_bytes()
    if b[:2] != b"MZ":
        raise ValueError("reference is not an MZ/PE image")
    pe = u32(b, 0x3c)
    if b[pe:pe+4] != b"PE\0\0":
        raise ValueError("invalid PE signature")
    coff = pe + 4
    if u16(b, coff) != 0x8664:
        raise ValueError("reference is not AMD64")
    nsec, opt_size = u16(b, coff+2), u16(b, coff+16)
    opt, sectab = pe + 24, pe + 24 + opt_size
    if u16(b, opt) != 0x20b:
        raise ValueError("reference is not PE32+")
    headers_size = u32(b, opt+60)
    def rva(rva: int, size: int = 1) -> int:
        if rva < headers_size and rva + size <= len(b):
            return rva
        for i in range(nsec):
            s = sectab + i*40
            va, rawsize, rawptr = u32(b,s+12), u32(b,s+16), u32(b,s+20)
            vsize = u32(b,s+8)
            span = max(vsize, rawsize)
            if va <= rva and rva + size <= va + span:
                delta = rva-va
                if delta + size > rawsize:
                    break
                off = rawptr+delta
                if off+size <= len(b):
                    return off
                break
        raise ValueError(f"unmapped RVA 0x{rva:x}")
    export_rva, export_size = u32(b,opt+112), u32(b,opt+116)
    if not export_rva or export_size < 40:
        raise ValueError("reference has no export directory")
    e = rva(export_rva,40)
    ordinal_base = u32(b,e+16)
    nfunc, nname = u32(b,e+20), u32(b,e+24)
    funcs, names, ords = u32(b,e+28), u32(b,e+32), u32(b,e+36)
    if nfunc > 200000 or nname > 200000:
        raise ValueError("unreasonable export table sizes")
    fn_off, nm_off, ord_off = rva(funcs,nfunc*4), rva(names,nname*4), rva(ords,nname*2)
    out: dict[str,int] = {}
    for i in range(nname):
        nrva = u32(b,nm_off+i*4)
        at = rva(nrva)
        end = b.find(b"\0",at)
        if end < 0:
            raise ValueError("unterminated export name")
        name = b[at:end].decode("ascii",errors="strict")
        ord_index = u16(b,ord_off+i*2)
        if ord_index >= nfunc:
            raise ValueError("ordinal index outside function table")
        frva = u32(b,fn_off+ord_index*4)
        # Forwarders still count as exports and keep their ordinal.
        if name in out:
            raise ValueError(f"duplicate named export {name}")
        out[name] = ordinal_base + ord_index
    return out

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--reference", type=Path, help="exact Windows 11 x64 csrsrv.dll to compare")
    ap.add_argument("--definition", type=Path, default=DEF)
    args = ap.parse_args()
    try:
        expected = expected_exports(args.definition)
        if not expected:
            raise ValueError("definition contains no exports")
        if args.reference:
            actual = actual_exports(args.reference)
            missing = sorted(set(actual)-set(expected))
            extra = sorted(set(expected)-set(actual))
            ordinal = sorted((n, expected[n], actual[n]) for n in set(expected)&set(actual) if expected[n] != actual[n])
            for n in missing: print(f"missing from CognacOS definition: {n} @{actual[n]}")
            for n in extra: print(f"not in reference image: {n} @{expected[n]}")
            for n, want, got in ordinal: print(f"ordinal mismatch {n}: definition @{want}, reference @{got}")
            if missing or extra or ordinal:
                return 1
            print(f"exact export name/ordinal match: {len(expected)} exports")
        else:
            print(f"validated {len(expected)} unique named CSRSRV exports (inventory-only; pass --reference for Windows-build parity)")
        return 0
    except (OSError, ValueError, struct.error) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

if __name__ == "__main__":
    raise SystemExit(main())

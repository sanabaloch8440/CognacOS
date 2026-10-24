#!/usr/bin/env python3
"""Convert RdotOS's fixed-base x86-64 ELF64 ET_EXEC kernel into PE32+.

This is intentionally a narrow linker-output converter, not a general ELF tool:
it accepts only static ELF64 little-endian x86-64 ET_EXEC with non-overlapping
PT_LOAD segments, a fixed high-half image base, and no dynamic relocations.
Physical load addresses are intentionally not carried over; the PE-aware
bootloader allocates physical backing for each PE section and maps its RVA to
the kernel's fixed high-half virtual address.
"""
import argparse
import struct
import sys

BASE = 0xFFFFFFFF80000000
PAGE = 0x1000
FILE_ALIGN = 0x200
SECT_ALIGN = 0x1000
PT_LOAD = 1
PF_X, PF_W, PF_R = 1, 2, 4


def align(v, a):
    return (v + a - 1) & ~(a - 1)


def u16(data, off):
    return struct.unpack_from("<H", data, off)[0]


def u32(data, off):
    return struct.unpack_from("<I", data, off)[0]


def u64(data, off):
    return struct.unpack_from("<Q", data, off)[0]


def convert(blob):
    if len(blob) < 64 or blob[:4] != b"\x7fELF":
        raise ValueError("input is not ELF")
    if blob[4:7] != b"\x02\x01\x01" or u16(blob, 16) != 2 or u16(blob, 18) != 62:
        raise ValueError("expected little-endian ELF64 ET_EXEC x86-64")
    if u16(blob, 52) != 64 or u16(blob, 54) != 56:
        raise ValueError("unsupported ELF header/program-header size")
    entry = u64(blob, 24)
    phoff = u64(blob, 32)
    phnum = u16(blob, 56)
    if phnum == 0 or phnum > 128 or phoff + phnum * 56 > len(blob):
        raise ValueError("invalid program-header table")

    segments = []
    entry_ok = False
    for i in range(phnum):
        off = phoff + i * 56
        kind, flags = struct.unpack_from("<II", blob, off)
        p_offset, vaddr, _paddr, filesz, memsz, _align = struct.unpack_from("<QQQQQQ", blob, off + 8)
        if kind != PT_LOAD:
            continue
        if not memsz or filesz > memsz or p_offset + filesz > len(blob):
            raise ValueError("invalid PT_LOAD range")
        if vaddr < BASE or vaddr + memsz > 0xFFFFFFFFFFFFFFFF:
            raise ValueError("PT_LOAD outside fixed high-half image")
        if (vaddr - BASE) & (PAGE - 1):
            raise ValueError("PT_LOAD RVA must be page aligned")
        if filesz and (p_offset & (FILE_ALIGN - 1)) != ((vaddr - BASE) & (FILE_ALIGN - 1)):
            # This is not required for PE raw layout, but ELF segment congruence
            # is still checked by the ELF loader contract.
            if (vaddr - p_offset) & (PAGE - 1):
                raise ValueError("invalid ELF PT_LOAD alignment")
        if flags & PF_X and vaddr <= entry < vaddr + memsz:
            entry_ok = True
        for old in segments:
            if vaddr < old["vaddr"] + old["memsz"] and old["vaddr"] < vaddr + memsz:
                raise ValueError("overlapping PT_LOAD virtual ranges")
        segments.append({"vaddr": vaddr, "filesz": filesz, "memsz": memsz,
                         "offset": p_offset, "flags": flags})
    if not segments or not entry_ok:
        raise ValueError("no loadable executable segment contains entry point")
    segments.sort(key=lambda s: s["vaddr"])
    if BASE != min(s["vaddr"] for s in segments) & ~(PAGE - 1):
        raise ValueError("unexpected image base; update PE base deliberately")

    # PE headers use fixed offsets and section raw data is laid out contiguously.
    peoff = 0x80
    optional_size = 240
    headers_size = align(peoff + 4 + 20 + optional_size + 40 * len(segments), FILE_ALIGN)
    raw_cursor = headers_size
    section_headers = []
    for i, s in enumerate(segments):
        s["rva"] = s["vaddr"] - BASE
        s["raw_size"] = align(s["filesz"], FILE_ALIGN) if s["filesz"] else 0
        s["raw_offset"] = raw_cursor if s["raw_size"] else 0
        raw_cursor += s["raw_size"]
        s["characteristics"] = 0x40000000
        if s["flags"] & PF_X:
            s["characteristics"] = 0x60000020
        else:
            if s["flags"] & PF_W:
                s["characteristics"] |= 0x80000000 | 0x40
            else:
                s["characteristics"] |= 0x40000040
        names = [".text", ".rdata", ".data", ".bss"]
        s["name"] = (names[i] if i < len(names) else f".s{i}")[:8].encode().ljust(8, b"\0")
        section_headers.append(struct.pack(
            "<8sIIIIIIHHI", s["name"], s["memsz"], s["rva"], s["raw_size"],
            s["raw_offset"], 0, 0, 0, 0, s["characteristics"]))

    size_image = align(max(s["rva"] + s["memsz"] for s in segments), SECT_ALIGN)
    out = bytearray(raw_cursor)
    out[0:2] = b"MZ"
    struct.pack_into("<I", out, 0x3C, peoff)
    out[peoff:peoff + 4] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", out, peoff + 4, 0x8664, len(segments), 0, 0, 0, optional_size, 0x0022)
    o = peoff + 24
    struct.pack_into("<HBBIIIII", out, o, 0x20B, 0, 0,
                     sum(s["raw_size"] for s in segments if s["flags"] & PF_X),
                     sum(s["raw_size"] for s in segments if not s["flags"] & PF_X),
                     0, entry - BASE, min(s["rva"] for s in segments if s["flags"] & PF_X),
                     0)
    struct.pack_into("<QII", out, o + 24, BASE, SECT_ALIGN, FILE_ALIGN)
    struct.pack_into("<HHHHHH", out, o + 40, 6, 0, 0, 0, 6, 0)
    struct.pack_into("<IIII", out, o + 52, 0, size_image, headers_size, 0)
    struct.pack_into("<HH", out, o + 68, 1, 0x0100)  # IMAGE_SUBSYSTEM_NATIVE, NX_COMPAT
    struct.pack_into("<QQQQ", out, o + 72, 0x100000, 0x1000, 0x100000, 0x1000)
    struct.pack_into("<II", out, o + 104, 0, 16)  # loader flags, data-directory count
    # No imports, relocations, TLS, or exception directory: this is a fixed-base
    # static kernel image, not a Windows user-mode executable.
    shoff = o + optional_size
    for i, sh in enumerate(section_headers):
        out[shoff + i * 40:shoff + (i + 1) * 40] = sh
    for s in segments:
        if s["raw_size"]:
            out[s["raw_offset"]:s["raw_offset"] + s["filesz"]] = blob[s["offset"]:s["offset"] + s["filesz"]]
    return bytes(out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("output")
    args = parser.parse_args()
    try:
        with open(args.input, "rb") as f:
            source = f.read()
        result = convert(source)
        with open(args.output, "wb") as f:
            f.write(result)
        print(f"PE32+ kernel image written: {len(result)} bytes")
    except (OSError, ValueError, struct.error) as e:
        print(f"ELF-to-PE conversion failed: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

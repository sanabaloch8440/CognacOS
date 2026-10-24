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

BASE = 0xFFFFFFFF7FE00000
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



def _u64(data, off):
    return struct.unpack_from("<Q", data, off)[0]


def read_elf_function_exports(blob, manifest_path):
    """Return only defined global function symbols present in the pinned Windows manifest."""
    import json
    with open(manifest_path, "r", encoding="utf-8") as f:
        manifest = json.load(f)
    allowed = {e["name"]: e["ordinal"] for e in manifest.get("exports", [])
               if e.get("name") is not None and isinstance(e.get("ordinal"), int)}
    if len(blob) < 64:
        raise ValueError("truncated ELF symbol table")
    shoff = _u64(blob, 40)
    shentsize, shnum = u16(blob, 58), u16(blob, 60)
    if shentsize < 64 or shnum == 0 or shoff + shentsize * shnum > len(blob):
        raise ValueError("invalid ELF section-header table")
    sections = []
    for i in range(shnum):
        off = shoff + i * shentsize
        sections.append(struct.unpack_from("<IIQQQQIIQQ", blob, off))
    found = {}
    for sec in sections:
        _name, kind, _flags, _addr, off, size, link, _info, _align, entsize = sec
        if kind != 2:  # SHT_SYMTAB (dynamic symbols are not used in this static image)
            continue
        if link >= len(sections) or entsize < 24 or off + size > len(blob):
            raise ValueError("invalid ELF symbol/string table")
        strings_sec = sections[link]
        str_off, str_size = strings_sec[4], strings_sec[5]
        if str_off + str_size > len(blob):
            raise ValueError("invalid ELF symbol string table")
        strings = blob[str_off:str_off + str_size]
        for pos in range(off, off + size, entsize):
            if pos + 24 > len(blob):
                raise ValueError("truncated ELF symbol entry")
            name_off, info, _other, shndx, value, _sym_size = struct.unpack_from("<IBBHQQ", blob, pos)
            if shndx == 0 or (info >> 4) not in (1, 2) or (info & 0x0f) != 2:
                continue
            if name_off >= len(strings):
                continue
            end = strings.find(b"\\0", name_off)
            if end < 0:
                continue
            try:
                name = strings[name_off:end].decode("ascii")
            except UnicodeDecodeError:
                continue
            if name in allowed:
                if name in found and found[name][0] != value:
                    raise ValueError(f"duplicate Windows export symbol {name}")
                found[name] = (value, allowed[name])
    return [(name, ordinal, value) for name, (value, ordinal) in sorted(found.items())]


def build_export_directory(exports, image_base):
    """Build a PE export directory with Windows-manifest ordinals and no non-Windows names."""
    if not exports:
        return b""
    max_ordinal = max(ordinal for _name, ordinal, _value in exports)
    if max_ordinal < 1 or max_ordinal > 65535:
        raise ValueError("invalid export ordinal range")
    names = sorted(exports, key=lambda item: item[0])
    directory_size = 40
    eat_off = directory_size
    names_off = eat_off + max_ordinal * 4
    ords_off = names_off + len(names) * 4
    strings_off = ords_off + len(names) * 2
    strings = bytearray()
    dll_name = b"ntoskrnl.exe\\0"
    dll_name_off = strings_off
    strings.extend(dll_name)
    name_string_offsets = []
    for name, _ordinal, _value in names:
        name_string_offsets.append(strings_off + len(strings) - (strings_off + len(dll_name)))
        strings.extend(name.encode("ascii") + b"\\0")
    data = bytearray(strings_off + len(strings) - strings_off)
    # PE RVA is based on the section RVA; the caller patches this directory using
    # the known .edata RVA after the final section layout has been selected.
    for name, ordinal, value in exports:
        if value < image_base or value - image_base > 0xffffffff:
            raise ValueError(f"export {name} is outside the PE image")
        struct.pack_into("<I", data, eat_off + (ordinal - 1) * 4, value - image_base)
    for i, (name, ordinal, _value) in enumerate(names):
        struct.pack_into("<H", data, ords_off + i * 2, ordinal - 1)
    data[strings_off:strings_off + len(strings)] = strings
    # Relative string offsets are converted to image RVAs by finalize_export_directory.
    return bytes(data), {"eat_off": eat_off, "names_off": names_off, "ords_off": ords_off,
                         "dll_name_off": dll_name_off,
                         "name_string_offsets": name_string_offsets, "names": names}


def finalize_export_directory(data, metadata, section_rva, exports):
    out = bytearray(data)
    eat_off, names_off, ords_off = metadata["eat_off"], metadata["names_off"], metadata["ords_off"]
    # The name pointer table uses RVAs, not offsets into .edata.
    for i, string_off in enumerate(metadata["name_string_offsets"]):
        struct.pack_into("<I", out, names_off + i * 4, section_rva + string_off)
    dll_name_rva = section_rva + metadata["dll_name_off"]
    directory = struct.pack("<IIHHIIIIIIII", 0, 0, 0, 0, dll_name_rva, 1,
                            max(ordinal for _name, ordinal, _value in exports), len(exports),
                            section_rva + eat_off, section_rva + names_off, section_rva + ords_off)
    out[:40] = directory
    return bytes(out)


def convert(blob, image_base=BASE, dll=False, export_manifest=None):
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
        if vaddr < image_base or vaddr + memsz > 0xFFFFFFFFFFFFFFFF:
            raise ValueError("PT_LOAD outside fixed high-half image")
        if (vaddr - image_base) & (PAGE - 1):
            raise ValueError("PT_LOAD RVA must be page aligned")
        if filesz and (p_offset & (FILE_ALIGN - 1)) != ((vaddr - image_base) & (FILE_ALIGN - 1)):
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
    first_rva = min(s["vaddr"] - image_base for s in segments)
    if image_base & 0xFFFF or first_rva < PAGE or first_rva & (PAGE - 1):
        raise ValueError("image base must be 64 KiB aligned with a nonzero section RVA")

    export_data = b""
    export_meta = None
    export_rva = 0
    if export_manifest:
        exports = read_elf_function_exports(blob, export_manifest)
        if exports:
            export_rva = align(max(s["vaddr"] + s["memsz"] for s in segments) - image_base, SECT_ALIGN)
            export_data, export_meta = build_export_directory(exports, image_base)
            export_data = finalize_export_directory(export_data, export_meta, export_rva, exports)
            segments.append({"vaddr": image_base + export_rva, "filesz": len(export_data),
                             "memsz": len(export_data), "offset": 0, "flags": PF_R,
                             "name_override": ".edata", "generated_data": export_data})
            segments.sort(key=lambda s: s["vaddr"])

    # PE headers use fixed offsets and section raw data is laid out contiguously.
    peoff = 0x80
    optional_size = 240
    headers_size = align(peoff + 4 + 20 + optional_size + 40 * len(segments), FILE_ALIGN)
    raw_cursor = headers_size
    section_headers = []
    for i, s in enumerate(segments):
        s["rva"] = s["vaddr"] - image_base
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
    struct.pack_into("<HHIIIHH", out, peoff + 4, 0x8664, len(segments), 0, 0, 0, optional_size, 0x0022 | (0x2000 if dll else 0))
    o = peoff + 24
    struct.pack_into("<HBBIIIII", out, o, 0x20B, 0, 0,
                     sum(s["raw_size"] for s in segments if s["flags"] & PF_X),
                     sum(s["raw_size"] for s in segments if not s["flags"] & PF_X),
                     sum(max(0, s["memsz"] - s["filesz"]) for s in segments),
                     entry - image_base, min(s["rva"] for s in segments if s["flags"] & PF_X))
    struct.pack_into("<QII", out, o + 24, image_base, SECT_ALIGN, FILE_ALIGN)
    struct.pack_into("<HHHHHH", out, o + 40, 6, 0, 0, 0, 6, 0)
    struct.pack_into("<IIII", out, o + 52, 0, size_image, headers_size, 0)
    struct.pack_into("<HH", out, o + 68, 1, 0x0100)  # IMAGE_SUBSYSTEM_NATIVE, NX_COMPAT
    struct.pack_into("<QQQQ", out, o + 72, 0x100000, 0x1000, 0x100000, 0x1000)
    struct.pack_into("<II", out, o + 104, 0, 16)  # loader flags, data-directory count
    if export_data:
        struct.pack_into("<II", out, o + 112, export_rva, len(export_data))
    # No imports, relocations, TLS, or exception directory: this is a fixed-base
    # static kernel image, not a Windows user-mode executable.
    shoff = o + optional_size
    for i, sh in enumerate(section_headers):
        out[shoff + i * 40:shoff + (i + 1) * 40] = sh
    for s in segments:
        if s["raw_size"]:
            payload = s.get("generated_data")
            if payload is None:
                payload = blob[s["offset"]:s["offset"] + s["filesz"]]
            out[s["raw_offset"]:s["raw_offset"] + s["filesz"]] = payload
    return bytes(out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("output")
    parser.add_argument("--image-base", type=lambda v: int(v, 0), default=BASE)
    parser.add_argument("--dll", action="store_true", help="mark the PE image as a native driver DLL")
    parser.add_argument("--export-manifest", help="Windows export manifest; export only defined global function symbols present in this manifest")
    args = parser.parse_args()
    try:
        with open(args.input, "rb") as f:
            source = f.read()
        result = convert(source, image_base=args.image_base, dll=args.dll, export_manifest=args.export_manifest)
        with open(args.output, "wb") as f:
            f.write(result)
        print(f"PE32+ kernel image written: {len(result)} bytes")
    except (OSError, ValueError, struct.error) as e:
        print(f"ELF-to-PE conversion failed: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

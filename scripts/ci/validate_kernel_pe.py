#!/usr/bin/env python3
"""Validate the fixed-base AMD64 PE32+ native kernel images produced by elf-to-pe32plus."""
from pathlib import Path
import struct

IMAGES = (
    Path("build/cognacos-kernel.exe"),
    Path("build/cognacos-kernel-release.exe"),
)


def u16(data, offset):
    return struct.unpack_from("<H", data, offset)[0]


def u32(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def validate(path):
    data = path.read_bytes()
    assert len(data) >= 0x40 and data[:2] == b"MZ", f"{path}: missing DOS header"
    pe = u32(data, 0x3C)
    assert pe + 24 <= len(data) and data[pe:pe + 4] == b"PE\0\0", f"{path}: missing PE signature"

    machine = u16(data, pe + 4)
    section_count = u16(data, pe + 6)
    characteristics = u16(data, pe + 22)
    optional_size = u16(data, pe + 20)
    assert machine == 0x8664, f"{path}: expected AMD64 machine, got 0x{machine:04x}"
    assert characteristics & 0x0002, f"{path}: not an executable image"
    assert not (characteristics & 0x2000), f"{path}: kernel image must not be a DLL"
    assert section_count > 0, f"{path}: no PE sections"

    opt = pe + 24
    assert optional_size >= 240 and opt + optional_size <= len(data), f"{path}: invalid PE32+ optional header"
    assert u16(data, opt) == 0x20B, f"{path}: not PE32+"
    entry_rva = u32(data, opt + 16)
    section_alignment = u32(data, opt + 32)
    file_alignment = u32(data, opt + 36)
    size_image = u32(data, opt + 56)
    size_headers = u32(data, opt + 60)
    subsystem = u16(data, opt + 68)
    directory_count = u32(data, opt + 108)
    assert entry_rva != 0, f"{path}: missing entry point"
    assert subsystem == 1, f"{path}: expected IMAGE_SUBSYSTEM_NATIVE, got {subsystem}"
    assert section_alignment and section_alignment & (section_alignment - 1) == 0, f"{path}: invalid section alignment"
    assert file_alignment and file_alignment & (file_alignment - 1) == 0, f"{path}: invalid file alignment"
    assert file_alignment <= section_alignment, f"{path}: file alignment exceeds section alignment"
    assert size_image > 0 and size_headers <= len(data), f"{path}: invalid image/header size"

    table = opt + optional_size
    assert table + section_count * 40 <= len(data), f"{path}: truncated section table"
    virtual_ranges = []
    entry_is_executable = False
    for index in range(section_count):
        sh = table + index * 40
        name = data[sh:sh + 8].split(b"\0", 1)[0].decode("ascii", "replace")
        virtual_size, virtual_address, raw_size, raw_offset = struct.unpack_from("<IIII", data, sh + 8)
        section_flags = u32(data, sh + 36)
        span = max(virtual_size, raw_size)
        assert span > 0, f"{path}: empty section {name}"
        assert virtual_address + span <= size_image, f"{path}: section {name} exceeds SizeOfImage"
        if raw_size:
            assert raw_offset >= size_headers, f"{path}: section {name} overlaps headers"
            assert raw_offset + raw_size <= len(data), f"{path}: section {name} raw data exceeds file"
        for start, end, previous in virtual_ranges:
            assert virtual_address >= end or start >= virtual_address + span, (
                f"{path}: virtual sections {previous} and {name} overlap"
            )
        virtual_ranges.append((virtual_address, virtual_address + span, name))
        if section_flags & 0x20000000 and virtual_address <= entry_rva < virtual_address + span:
            entry_is_executable = True
    assert entry_is_executable, f"{path}: entry point is not in an executable section"

    # The kernel is linked at its fixed high-half image base. The stage-1 loader
    # does not resolve imports or apply base relocations for the kernel image.
    if directory_count > 1:
        import_rva, import_size = struct.unpack_from("<II", data, opt + 112 + 8)
        assert (import_rva, import_size) == (0, 0), f"{path}: unexpected kernel import directory"
    if directory_count > 5:
        reloc_rva, reloc_size = struct.unpack_from("<II", data, opt + 112 + 5 * 8)
        assert (reloc_rva, reloc_size) == (0, 0), f"{path}: unexpected base relocations"

    print(f"validated fixed-base AMD64 PE32+ native kernel: {path} ({len(data)} bytes, {section_count} sections)")


def main():
    for image in IMAGES:
        validate(image)


if __name__ == "__main__":
    main()

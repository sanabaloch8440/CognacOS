#!/usr/bin/env python3
"""Provision GPT entry 4 as a CognacOS raw paging area in a disk-image file.

This is deliberately opt-in and only edits a regular image file, never a host
block device. Existing GPT partitions are preserved. The new partition uses a
CognacOS-specific type GUID and a COGNACPF v1 header; it is not Windows'
pagefile.sys format.
"""
from __future__ import annotations

import argparse
import math
import os
import struct
import sys
import uuid
import zlib
from pathlib import Path

SECTOR = 512
GPT_HEADER_SIG = b"EFI PART"
PAGEFILE_MARKER = b"COGNACPF"
PAGEFILE_VERSION = 1
PAGEFILE_HEADER_SIZE = 4096
# Private type GUID for CognacOS raw paging partition (stored on disk GUID byte order).
COGNACOS_PAGEFILE_TYPE = uuid.UUID("b7e4c8d1-2a63-4e8a-9f52-434f474e4143")


class ProvisionError(Exception):
    pass


def read_at(f, offset: int, size: int) -> bytes:
    f.seek(offset)
    data = f.read(size)
    if len(data) != size:
        raise ProvisionError(f"short read at byte offset {offset}")
    return data


def write_at(f, offset: int, data: bytes) -> None:
    f.seek(offset)
    if f.write(data) != len(data):
        raise ProvisionError(f"short write at byte offset {offset}")


def parse_header(raw: bytes, file_sectors: int, expected_lba: int) -> dict:
    if len(raw) != SECTOR or raw[:8] != GPT_HEADER_SIG:
        raise ProvisionError("missing GPT header signature")
    revision, header_size, stored_crc = struct.unpack_from("<III", raw, 8)
    if header_size < 92 or header_size > SECTOR:
        raise ProvisionError("invalid GPT header size")
    temp = bytearray(raw[:header_size])
    struct.pack_into("<I", temp, 16, 0)
    if zlib.crc32(temp) & 0xffffffff != stored_crc:
        raise ProvisionError("GPT header CRC mismatch")
    current, backup, first_usable, last_usable = struct.unpack_from("<QQQQ", raw, 24)
    entries_lba = struct.unpack_from("<Q", raw, 72)[0]
    entry_count, entry_size, entries_crc = struct.unpack_from("<III", raw, 80)
    if current != expected_lba or backup >= file_sectors or current >= file_sectors:
        raise ProvisionError("GPT header LBA fields are inconsistent with image size")
    if entry_count < 4 or entry_size < 128 or entry_size % 8:
        raise ProvisionError("unsupported GPT partition-entry geometry")
    array_bytes = entry_count * entry_size
    if array_bytes > 16 * 1024 * 1024:
        raise ProvisionError("partition-entry array exceeds safety limit")
    array = read_at(_ACTIVE_FILE[0], entries_lba * SECTOR, array_bytes)
    if zlib.crc32(array) & 0xffffffff != entries_crc:
        raise ProvisionError("GPT partition-entry array CRC mismatch")
    if first_usable > last_usable or last_usable >= file_sectors:
        raise ProvisionError("invalid GPT usable-LBA range")
    return {
        "raw": bytearray(raw), "revision": revision, "header_size": header_size,
        "current": current, "backup": backup, "first_usable": first_usable,
        "last_usable": last_usable, "entries_lba": entries_lba,
        "entry_count": entry_count, "entry_size": entry_size,
        "array_crc": entries_crc, "array": bytearray(array),
    }


_ACTIVE_FILE: list = []


def recalc_header_crc(header: bytearray, header_size: int) -> bytes:
    struct.pack_into("<I", header, 16, 0)
    crc = zlib.crc32(header[:header_size]) & 0xffffffff
    struct.pack_into("<I", header, 16, crc)
    return bytes(header)


def provision(image: Path, size_mib: int) -> tuple[int, int]:
    if size_mib < 16:
        raise ProvisionError("paging partition must be at least 16 MiB")
    if not image.is_file():
        raise ProvisionError("--image must name an existing regular disk-image file")
    with image.open("r+b") as f:
        _ACTIVE_FILE[:] = [f]
        length = os.fstat(f.fileno()).st_size
        if length % SECTOR or length < 8 * 1024 * 1024:
            raise ProvisionError("image size must be a sector-aligned disk image of at least 8 MiB")
        sectors = length // SECTOR
        primary = parse_header(read_at(f, SECTOR, SECTOR), sectors, 1)
        backup = parse_header(read_at(f, (sectors - 1) * SECTOR, SECTOR), sectors, sectors - 1)
        if primary["backup"] != backup["current"] or backup["backup"] != primary["current"]:
            raise ProvisionError("primary and backup GPT headers do not point to each other")
        if primary["entry_count"] != backup["entry_count"] or primary["entry_size"] != backup["entry_size"]:
            raise ProvisionError("primary and backup GPT partition geometry differs")
        entry_size = primary["entry_size"]
        # GPT entry index 3 is partition number 4; it must be completely unused.
        offset = 3 * entry_size
        if any(primary["array"][offset:offset + entry_size]):
            raise ProvisionError("GPT partition entry 4 is already occupied; refusing to overwrite it")
        bytes_needed = size_mib * 1024 * 1024
        sectors_needed = math.ceil(bytes_needed / SECTOR)
        # Align the new partition start to 1 MiB and leave a 1 MiB safety gap
        # before the last usable LBA boundary.
        start = (primary["last_usable"] - sectors_needed + 1) & ~((1024 * 1024 // SECTOR) - 1)
        end = start + sectors_needed - 1
        if start < primary["first_usable"] or end > primary["last_usable"]:
            raise ProvisionError("not enough unallocated GPT usable space for requested paging area")
        for index in range(primary["entry_count"]):
            pos = index * entry_size
            entry = primary["array"][pos:pos + entry_size]
            if not any(entry):
                continue
            first, last = struct.unpack_from("<QQ", entry, 32)
            if not (end < first or start > last):
                raise ProvisionError(f"requested range overlaps existing GPT partition entry {index + 1}")
        unique = uuid.uuid4()
        name = "CognacOS Paging".encode("utf-16le")
        entry = bytearray(entry_size)
        entry[:16] = COGNACOS_PAGEFILE_TYPE.bytes_le
        entry[16:32] = unique.bytes_le
        struct.pack_into("<QQQ", entry, 32, start, end, 0)
        entry[56:56 + min(len(name), entry_size - 56)] = name[:entry_size - 56]
        primary["array"][offset:offset + entry_size] = entry
        backup["array"][offset:offset + entry_size] = entry
        # Write the backup array/header first, then the primary array/header.
        array_bytes = primary["entry_count"] * entry_size
        write_at(f, backup["entries_lba"] * SECTOR, bytes(backup["array"]))
        write_at(f, primary["entries_lba"] * SECTOR, bytes(primary["array"]))
        array_crc = zlib.crc32(primary["array"]) & 0xffffffff
        struct.pack_into("<I", primary["raw"], 88, array_crc)
        struct.pack_into("<I", backup["raw"], 88, array_crc)
        write_at(f, (sectors - 1) * SECTOR, recalc_header_crc(backup["raw"], backup["header_size"]))
        write_at(f, SECTOR, recalc_header_crc(primary["raw"], primary["header_size"]))
        # Header at partition start; first 4 KiB reserved, data slots follow it.
        page_header = bytearray(PAGEFILE_HEADER_SIZE)
        page_header[:8] = PAGEFILE_MARKER
        struct.pack_into("<II", page_header, 8, PAGEFILE_VERSION, PAGEFILE_HEADER_SIZE)
        write_at(f, start * SECTOR, page_header)
        f.flush()
        os.fsync(f.fileno())
        return start, sectors_needed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, type=Path, help="existing GPT disk-image file to modify")
    parser.add_argument("--size-mib", type=int, default=128, help="paging partition size in MiB (minimum 16)")
    args = parser.parse_args()
    try:
        start, count = provision(args.image, args.size_mib)
    except (OSError, ProvisionError, struct.error) as exc:
        print(f"provision-pagefile: error: {exc}", file=sys.stderr)
        return 2
    print(f"Provisioned CognacOS paging partition: start_lba={start}, sectors={count}, size_mib={args.size_mib}")
    print("Warning: this is a CognacOS raw paging area, not a Windows pagefile.sys volume/file.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

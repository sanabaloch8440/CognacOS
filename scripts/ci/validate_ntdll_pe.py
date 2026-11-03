#!/usr/bin/env python3
"""Validate that the Rust NTDLL output is a well-formed AMD64 PE32+ DLL."""
import struct
import sys

blob = open(sys.argv[1], "rb").read()
assert blob[:2] == b"MZ", "Rust NTDLL lacks DOS header"
pe = struct.unpack_from("<I", blob, 0x3c)[0]
assert blob[pe:pe+4] == b"PE\0\0", "Rust NTDLL lacks PE signature"
assert struct.unpack_from("<H", blob, pe+4)[0] == 0x8664, "Rust NTDLL is not AMD64"
sections = struct.unpack_from("<H", blob, pe+6)[0]
opt_size = struct.unpack_from("<H", blob, pe+20)[0]
characteristics = struct.unpack_from("<H", blob, pe+22)[0]
opt = pe + 24
assert struct.unpack_from("<H", blob, opt)[0] == 0x20b, "Rust NTDLL is not PE32+"
assert characteristics & 0x2000, "Rust NTDLL is not marked as a DLL"
assert opt_size >= 240 and sections > 0, "Rust NTDLL optional header/sections are invalid"
print(f"Rust NTDLL PE32+ validation passed: {sections} sections")

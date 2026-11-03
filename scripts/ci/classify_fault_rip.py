#!/usr/bin/env python3
"""Classify a captured RIP against the NTDLL PE; only resolve if it is in range."""
import re
import struct
import subprocess
from pathlib import Path

gdb = Path("build/gdb-state.txt").read_text(errors="replace")
serial = Path("build/serial.log").read_text(errors="replace")
m = re.search(r"\brip\s+0x([0-9a-fA-F]+)", gdb)
if not m:
    print("GDB did not report RIP; retain gdb-state.txt and qemu-debug.log for manual analysis.")
    raise SystemExit(0)
rip = int(m.group(1), 16)

pe_path = Path("target/x86_64-pc-windows-msvc/release/cognac_ntdll.dll")
if not pe_path.exists():
    print(f"NTDLL PE/PDB input missing: {pe_path}")
    raise SystemExit(0)
blob = pe_path.read_bytes()
if blob[:2] != b"MZ":
    print("NTDLL input is not a PE image.")
    raise SystemExit(0)
pe = struct.unpack_from("<I", blob, 0x3c)[0]
if blob[pe:pe+4] != b"PE\0\0":
    print("NTDLL PE signature is invalid.")
    raise SystemExit(0)
opt = pe + 24
image_base = struct.unpack_from("<Q", blob, opt + 24)[0]
image_size = struct.unpack_from("<I", blob, opt + 56)[0]

candidates = [int(x, 16) for x in re.findall(r"module=0x([0-9a-fA-F]+)", serial)]
candidates.append(image_base)
base = next((b for b in reversed(candidates) if b <= rip < b + image_size), None)
if base is None:
    print(
        f"Captured RIP 0x{rip:x} is outside all known NTDLL image ranges; "
        f"NTDLL preferred base=0x{image_base:x}, image size=0x{image_size:x}."
    )
    print("This is not evidence that execution was inside NTDLL or that the PE loader is broken.")
    print("Treat this RIP as a ntoskrnl/HAL/other-image address and use the ntoskrnl/HAL DWARF and QEMU exception trace above.")
    print("NTDLL source resolution requires a RIP inside its actual loaded image range; do not subtract the NTDLL preferred base from an unrelated address.")
    raise SystemExit(0)

rva = rip - base
print(f"NTDLL source resolution: RIP=0x{rip:x}, loaded_base=0x{base:x}, RVA=0x{rva:x}")
cmd = [
    "llvm-symbolizer", "--obj=" + str(pe_path), "--relative-address",
    "--functions=linkage", "--demangle", hex(rva),
]
result = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
print(result.stdout.rstrip() or "llvm-symbolizer returned no line information; inspect the archived PDB.")
if result.returncode:
    print(f"llvm-symbolizer exit status: {result.returncode}")

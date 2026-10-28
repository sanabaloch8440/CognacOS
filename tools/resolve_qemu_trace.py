#!/usr/bin/env python3
"""Map live GDB RIPs and QEMU x86 exception PCs to source lines."""
import argparse
import re
import struct
import subprocess
from pathlib import Path


def command(args):
    try:
        p = subprocess.run(args, text=True, stdout=subprocess.PIPE,
                           stderr=subprocess.STDOUT, check=False)
        return p.stdout.strip()
    except FileNotFoundError:
        return ""


def pe_info(path):
    try:
        b = path.read_bytes()
        if b[:2] != b"MZ":
            return None
        pe = struct.unpack_from("<I", b, 0x3c)[0]
        if b[pe:pe + 4] != b"PE\0\0":
            return None
        opt = pe + 24
        if struct.unpack_from("<H", b, opt)[0] != 0x20b:
            return None
        return struct.unpack_from("<Q", b, opt + 24)[0], struct.unpack_from("<I", b, opt + 56)[0]
    except (OSError, ValueError, struct.error):
        return None


def resolve_elf(path, pc):
    """Resolve an ELF PC, retaining good inline frames and falling back to GDB
    symbol/disassembly data when hand-written assembly has no DWARF line table."""
    if not path.exists():
        return ""
    out = command(["addr2line", "-e", str(path), "-f", "-C", "-i", hex(pc)])
    if out:
        lines = [line for line in out.splitlines() if line.strip()]
        # addr2line -i can emit a valid outer frame alongside an unknown inline
        # frame. Do not discard the useful source location because of one ??.
        useful = [line for line in lines if "??:0" not in line and "??:?" not in line and line != "??"]
        if useful:
            return "\\n".join(lines)
    # Rust global_asm!/hand-written entry stubs may have no DWARF line table.
    # GDB still knows the nearest symbol and can show the exact machine
    # instruction at the faulting PC, rather than claiming there is no context.
    gdb = command([
        "gdb", "-q", "-batch", "-ex", f"file {path}",
        "-ex", f"info symbol {hex(pc)}",
        "-ex", f"info line *{hex(pc)}",
        "-ex", f"x/8i {hex(pc)}",
    ])
    if not gdb:
        return ""
    if "No symbol matches" in gdb and "No line number information" in gdb:
        return ""
    return "[ELF symbol / assembly fallback; source line may not be emitted for this assembly range]\\n" + gdb


def main():
    ap = argparse.ArgumentParser()
    for name in ("trace", "serial", "kernel", "hal", "ntdll", "gdb"):
        ap.add_argument("--" + name, type=Path, required=True)
    a = ap.parse_args()
    trace = a.trace.read_text(errors="replace") if a.trace.exists() else ""
    serial = a.serial.read_text(errors="replace") if a.serial.exists() else ""
    gdb = a.gdb.read_text(errors="replace") if a.gdb.exists() else ""
    modules = [int(x, 16) for x in re.findall(r"module=0x([0-9a-fA-F]+)", serial)]
    info = pe_info(a.ntdll)
    if info:
        preferred, image_size = info
        modules.append(preferred)
    else:
        image_size = 0

    def resolve_pc(pc):
        location = resolve_elf(a.kernel, pc)
        if location:
            return "[ntoskrnl]\n" + location
        location = resolve_elf(a.hal, pc)
        if location:
            return "[HAL]\n" + location
        if info and image_size:
            base = next((b for b in reversed(modules) if b <= pc < b + image_size), None)
            if base is not None:
                rva = pc - base
                location = command([
                    "llvm-symbolizer", "--obj=" + str(a.ntdll), "--relative-address",
                    "--functions=linkage", "--demangle", hex(rva)
                ])
                if location and "??:0" not in location and "??:?" not in location:
                    return f"[NTDLL] base=0x{base:x} RVA=0x{rva:x}\n{location}"
        return "Source line unavailable; inspect archived debug companions."

    print("=== GDB stopped-CPU RIPs -> source line ===")
    pcs = []
    for match in re.finditer(r"\brip\s+0x([0-9a-fA-F]+)", gdb):
        pc = int(match.group(1), 16)
        if pc not in pcs:
            pcs.append(pc)
    if not pcs:
        print("No RIP register records found in GDB snapshot.")
    for pc in pcs:
        print(f"\nRIP=0x{pc:x}\n{resolve_pc(pc)}")

    # QEMU -d int records vector/error and the corresponding architectural PC.
    pattern = re.compile(
        r"\bv=([0-9a-fA-F]{1,2})\s+e=([0-9a-fA-F]+).*?"
        r"\bIP=([0-9a-fA-F]+):([0-9a-fA-F]+)\s+pc=([0-9a-fA-F]+)"
    )
    events = []
    for match in pattern.finditer(trace):
        vector = int(match.group(1), 16)
        if vector < 32:
            events.append((vector, int(match.group(2), 16), int(match.group(5), 16)))
    print("\n=== QEMU exception PCs -> source lines ===")
    if not events:
        print("No x86 exception vectors 0-31 were recorded; this may be a hang/spin path rather than a CPU exception.")
        return 0

    seen = set()
    latest = []
    for event in reversed(events):
        if event not in seen:
            seen.add(event)
            latest.append(event)
        if len(latest) == 30:
            break
    for vector, error, pc in latest:
        print(f"\nvector=0x{vector:02x} error=0x{error:x} PC=0x{pc:x}")
        print(resolve_pc(pc))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

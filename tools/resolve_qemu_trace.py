#!/usr/bin/env python3
"""Resolve QEMU/GDB PCs against the correct image and retain useful fallbacks."""
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
        return (struct.unpack_from("<Q", b, opt + 24)[0],
                struct.unpack_from("<I", b, opt + 56)[0])
    except (OSError, ValueError, struct.error):
        return None


def elf_exec_ranges(path):
    """Return executable ELF section virtual-address ranges, never guessing from a PC."""
    out = command(["readelf", "-W", "-S", str(path)])
    ranges = []
    # readelf -W -S: [ 1] .text PROGBITS  ffffffff80001000 ... size ... AX ...
    for line in out.splitlines():
        match = re.match(
            r"\s*\[\s*\d+\]\s+(\S+)\s+\S+\s+([0-9a-fA-F]+)\s+"
            r"[0-9a-fA-F]+\s+([0-9a-fA-F]+)\s+\S+\s+([A-Z]*)",
            line)
        if not match:
            continue
        name, address, size, flags = match.groups()
        if "X" in flags and int(size, 16):
            start = int(address, 16)
            ranges.append((start, start + int(size, 16), name))
    return ranges


def in_ranges(pc, ranges):
    return next((name for start, end, name in ranges if start <= pc < end), None)


def useful_source_line(output):
    """True only for actual file:line records, not merely a function name."""
    for line in output.splitlines():
        if re.search(r"(?:^|/)[^:\n]+:\d+(?::\d+)?$", line.strip()):
            if not re.search(r":(?:0|\?)$", line.strip()):
                return True
    return False


def resolve_elf(path, pc, ranges):
    """Resolve only PCs actually inside executable sections of this ELF."""
    section = in_ranges(pc, ranges)
    if not section or not path.exists():
        return ""
    out = command(["addr2line", "-e", str(path), "-f", "-C", "-i", hex(pc)])
    if out and useful_source_line(out):
        # Keep good outer frames while marking unknown inline frames honestly.
        return out.replace("??:0", "[no line table: ?? :0]").replace(
            "??:?", "[no line table: ?? :?]")
    # Assembly may have no DWARF line table. GDB fallback is deliberately
    # gated by the ELF executable-section range to avoid disassembling an
    # unrelated virtual address as if it belonged to this image.
    gdb = command([
        "gdb", "-q", "-batch", "-ex", f"file {path}",
        "-ex", f"info symbol {hex(pc)}",
        "-ex", f"info line *{hex(pc)}",
        "-ex", f"x/8i {hex(pc)}",
    ])
    if not gdb:
        return f"[{path.name}:{section}] no DWARF line; GDB unavailable"
    return (f"[ELF {path.name} section={section}; assembly/symbol fallback]\n"
            f"{gdb}")


def main():
    ap = argparse.ArgumentParser()
    for name in ("trace", "serial", "kernel", "hal", "ntdll", "gdb"):
        ap.add_argument("--" + name, type=Path, required=True)
    a = ap.parse_args()
    trace = a.trace.read_text(errors="replace") if a.trace.exists() else ""
    serial = a.serial.read_text(errors="replace") if a.serial.exists() else ""
    gdb_text = a.gdb.read_text(errors="replace") if a.gdb.exists() else ""

    kernel_ranges = elf_exec_ranges(a.kernel) if a.kernel.exists() else []
    hal_ranges = elf_exec_ranges(a.hal) if a.hal.exists() else []
    # Only runtime module bases reported by the guest are authoritative.
    # A PE preferred image base is not proof that the image was loaded there.
    modules = [int(x, 16) for x in re.findall(
        r"\bmodule=0x([0-9a-fA-F]+)", serial)]
    info = pe_info(a.ntdll)
    image_size = info[1] if info else 0

    def resolve_pc(pc):
        location = resolve_elf(a.kernel, pc, kernel_ranges)
        if location:
            return "[ntoskrnl]\n" + location
        location = resolve_elf(a.hal, pc, hal_ranges)
        if location:
            return "[HAL]\n" + location
        if info and image_size:
            base = next((b for b in reversed(modules)
                         if b <= pc < b + image_size), None)
            if base is not None:
                rva = pc - base
                location = command([
                    "llvm-symbolizer", "--obj=" + str(a.ntdll),
                    "--relative-address", "--functions=linkage",
                    "--demangle", hex(rva)])
                if location and useful_source_line(location):
                    return f"[NTDLL] base=0x{base:x} RVA=0x{rva:x}\n{location}"
                # Keep the module attribution even when CodeView line data is absent.
                return (f"[NTDLL] base=0x{base:x} RVA=0x{rva:x}; "
                        "symbol/source line unavailable")
        return ("[unmatched image] PC is outside the kernel/HAL executable "
                "sections and no loaded NTDLL range matches; inspect the "
                "captured trap frame/module map.")

    print("=== GDB stopped-CPU RIPs -> source line / symbol ===")
    pcs = []
    for match in re.finditer(r"\brip\s+0x([0-9a-fA-F]+)", gdb_text):
        pc = int(match.group(1), 16)
        if pc not in pcs:
            pcs.append(pc)
    if not pcs:
        print("No RIP register records found in GDB snapshot.")
    for pc in pcs:
        print(f"\nRIP=0x{pc:x}\n{resolve_pc(pc)}")

    pattern = re.compile(
        r"\bv=([0-9a-fA-F]{1,2})\s+e=([0-9a-fA-F]+).*?"
        r"\bIP=([0-9a-fA-F]+):([0-9a-fA-F]+)\s+pc=([0-9a-fA-F]+)")
    events = []
    for match in pattern.finditer(trace):
        vector = int(match.group(1), 16)
        if vector < 32:
            events.append((vector, int(match.group(2), 16),
                           int(match.group(5), 16)))
    print("\n=== QEMU exception PCs -> source lines / symbols ===")
    if not events:
        print("No x86 exception vectors 0-31 were recorded; this may be a hang/spin path rather than a CPU exception.")
        return 0
    seen, latest = set(), []
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

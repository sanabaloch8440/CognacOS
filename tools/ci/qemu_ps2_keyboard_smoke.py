#!/usr/bin/env python3
"""Run an isolated PS/2 keyboard smoke test without a QEMU USB keyboard.

QEMU's HMP sendkey is routed to an attached USB keyboard when one is present.
The main xHCI boot verifies that USB keyboard topology; this second boot omits
usb-kbd so injected monitor keys exercise the guest's PS/2 IRQ1/KbdClass path.
"""
from __future__ import annotations

import argparse
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path


def tail(path: Path, lines: int = 80) -> str:
    if not path.exists():
        return "<serial log not created>"
    return "\n".join(path.read_text(errors="replace").splitlines()[-lines:])


def wait_for_serial(path: Path, marker: str, process: subprocess.Popen[bytes], timeout: float) -> str:
    deadline = time.monotonic() + timeout
    last = ""
    while time.monotonic() < deadline:
        if path.exists():
            last = path.read_text(errors="replace")
            if marker in last:
                return last
        if process.poll() is not None:
            raise RuntimeError(f"QEMU exited with status {process.returncode} before marker {marker!r}\n{tail(path)}")
        time.sleep(0.1)
    raise RuntimeError(f"Timed out waiting for serial marker {marker!r}\n{tail(path)}")


def connect_monitor(path: Path, process: subprocess.Popen[bytes], timeout: float = 10.0) -> socket.socket:
    deadline = time.monotonic() + timeout
    monitor = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    monitor.settimeout(1.0)
    while time.monotonic() < deadline:
        try:
            monitor.connect(str(path))
            return monitor
        except (FileNotFoundError, ConnectionRefusedError):
            if process.poll() is not None:
                raise RuntimeError(f"QEMU exited with status {process.returncode} before monitor became available")
            time.sleep(0.1)
    raise RuntimeError(f"QEMU monitor socket did not become available: {path}")


def read_prompt(monitor: socket.socket, timeout: float = 3.0) -> bytes:
    data = bytearray()
    deadline = time.monotonic() + timeout
    while b"(qemu)" not in data and time.monotonic() < deadline:
        try:
            chunk = monitor.recv(65536)
        except socket.timeout:
            continue
        if not chunk:
            break
        data.extend(chunk)
    if b"(qemu)" not in data:
        raise RuntimeError(f"QEMU HMP prompt was not received: {bytes(data)!r}")
    return bytes(data)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    parser.add_argument("--code", required=True, type=Path)
    parser.add_argument("--vars-template", required=True, type=Path)
    parser.add_argument("--disk", required=True, type=Path)
    parser.add_argument("--pagefile", required=True, type=Path)
    parser.add_argument("--serial", required=True, type=Path)
    parser.add_argument("--monitor", required=True, type=Path)
    parser.add_argument("--debug-log", required=True, type=Path)
    parser.add_argument("--memory", default="5G")
    parser.add_argument("--smp", type=int, default=2)
    parser.add_argument("--boot-timeout", type=float, default=60.0)
    parser.add_argument("--input-timeout", type=float, default=10.0)
    args = parser.parse_args()

    for path in (args.code, args.vars_template, args.disk, args.pagefile):
        if not path.is_file():
            raise RuntimeError(f"Required QEMU input is missing: {path}")

    args.serial.parent.mkdir(parents=True, exist_ok=True)
    args.monitor.parent.mkdir(parents=True, exist_ok=True)
    args.debug_log.parent.mkdir(parents=True, exist_ok=True)
    args.serial.write_text("")
    args.debug_log.write_text("")
    args.monitor.unlink(missing_ok=True)

    vars_path = args.vars_template.parent / "OVMF_VARS_ps2.fd"
    shutil.copyfile(args.vars_template, vars_path)

    command = [
        args.qemu,
        "-machine", "q35",
        "-m", args.memory,
        "-cpu", "max",
        "-smp", str(args.smp),
        "-device", "intel-iommu,intremap=off,caching-mode=on,aw-bits=48",
        "-device", "qemu-xhci,id=xhci",
        "-device", "piix3-ide,id=legacyide,bus=pcie.0",
        "-drive", f"if=pflash,format=raw,readonly=on,file={args.code}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"if=none,id=osdisk,format=raw,file={args.disk}",
        "-device", "ide-hd,drive=osdisk,bus=legacyide.0,unit=0",
        "-drive", f"if=none,id=pagefile,format=raw,file={args.pagefile}",
        "-device", "ide-hd,drive=pagefile,bus=legacyide.0,unit=1",
        "-boot", "order=c",
        "-display", "none",
        "-monitor", f"unix:{args.monitor},server=on,wait=off",
        "-serial", f"file:{args.serial}",
        "-d", "int,cpu_reset,guest_errors",
        "-D", str(args.debug_log),
        "-no-reboot",
        "-no-shutdown",
    ]

    process: subprocess.Popen[bytes] | None = None
    monitor: socket.socket | None = None
    try:
        with open(args.debug_log.with_suffix(".launch.log"), "wb") as launch_log:
            process = subprocess.Popen(command, stdout=launch_log, stderr=subprocess.STDOUT)
        wait_for_serial(
            args.serial,
            "PS/2 IRQ: route enabled after controller initialization; bounded ISR active.",
            process,
            args.boot_timeout,
        )
        monitor = connect_monitor(args.monitor, process)
        read_prompt(monitor)
        for key in (b"a", b"shift-a", b"ret"):
            monitor.sendall(b"sendkey " + key + b"\n")
            read_prompt(monitor)
            time.sleep(0.15)
        wait_for_serial(args.serial, "QEMU KEYBOARD INPUT:", process, args.input_timeout)
        print("PS/2 keyboard smoke passed: monitor-injected keys reached IRQ1 and KbdClass.")
        return 0
    finally:
        if monitor is not None:
            try:
                monitor.sendall(b"quit\n")
            except OSError:
                pass
            monitor.close()
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"PS/2 keyboard smoke failed: {exc}", file=sys.stderr)
        raise SystemExit(1)

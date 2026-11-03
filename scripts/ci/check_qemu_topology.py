#!/usr/bin/env python3
"""Connect to the QEMU monitor socket and assert the device tree contains expected devices."""
import socket
import time

path = "build/qemu-monitor.sock"
deadline = time.monotonic() + 10
monitor = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
monitor.settimeout(2)
while True:
    try:
        monitor.connect(path)
        break
    except (FileNotFoundError, ConnectionRefusedError):
        if time.monotonic() >= deadline:
            raise SystemExit("QEMU monitor did not become available")
        time.sleep(0.1)


def read_prompt():
    data = bytearray()
    end = time.monotonic() + 3
    while b"(qemu)" not in data and time.monotonic() < end:
        try:
            chunk = monitor.recv(65536)
        except socket.timeout:
            break
        if not chunk:
            break
        data.extend(chunk)
    return data.decode("utf-8", "replace")


read_prompt()
monitor.sendall(b"info qtree\n")
tree = read_prompt()
monitor.close()

for expected in ("qemu-xhci", "usb-kbd", "piix3-ide", "osdisk"):
    if expected not in tree:
        raise SystemExit(f"QEMU device topology missing {expected}:\n{tree}")
print("QEMU topology verified: xHCI + USB keyboard + PIIX3 primary-channel ATA disks.")
if "intel-iommu" not in tree:
    raise SystemExit("QEMU IOMMU device missing from guest topology")
print("QEMU topology includes Intel VT-d; guest DMAR parsing and translation are checked separately.")

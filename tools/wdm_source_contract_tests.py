#!/usr/bin/env python3
"""Fast regression checks for Windows x64 WDM source contracts used by CI."""
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[1]
wdm = (root / "kernel/src/ntoskrnl/io/wdm.rs").read_text(encoding="utf-8")
exports = (root / "kernel/src/ntoskrnl/mod.rs").read_text(encoding="utf-8")
manifest = (root / "kernel/src/ntoskrnl/manifest_26100.json").read_text(encoding="utf-8")

errors = []
if re.search(r"#\[repr\([^\]]+\)\]\s*const\s", wdm):
    errors.append("repr attribute is incorrectly attached to a const item")
if not re.search(r"#\[repr\(C\)\]\s*struct IrpSlot\b", wdm):
    errors.append("IrpSlot must retain C layout for contiguous IRP/stack storage")

# Rust 2024 rejects implicit references created by indexing/method calls through
# raw IRP pointers. All accesses must go through explicit raw-address operations.
for pattern in (
    r"\(\*\w+\)\.bytes\.as_mut_ptr\(\)",
    r"\(\*\w+\)\.bytes\.as_ptr\(\)",
    r"\(\*\w+\)\.bytes\[[^\]]+\]",
):
    if re.search(pattern, wdm):
        errors.append(f"implicit raw-pointer autoref pattern remains: {pattern}")

for forbidden in ("MAX_WDM_IRPS", "MAX_IOCTL_SYSTEM_BUFFER"):
    if forbidden in wdm:
        errors.append(f"obsolete fixed WDM capacity remains: {forbidden}")

for required in (
    "struct IrpTable{head:*mut IrpSlot,len:usize}",
    "IRP_SLAB_PAGES:usize=256",
    "system_buffer:*mut u8",
    "thread_irps:[ThreadIrpQueue;MAX_SYNC_IRP_THREADS]",
    "sync_queue_attach_locked",
    "pub unsafe extern \"system\" fn io_csq_initialize",
    "pub unsafe extern \"system\" fn io_csq_initialize_ex",
    "pub unsafe extern \"system\" fn io_csq_insert_irp",
    "pub unsafe extern \"system\" fn io_csq_insert_irp_ex",
    "pub unsafe extern \"system\" fn io_csq_remove_irp",
    "pub unsafe extern \"system\" fn io_csq_remove_next_irp",
):
    if required not in wdm:
        errors.append(f"missing WDM implementation contract: {required}")

for symbol in (
    "IoAllocateIrp", "IoFreeIrp", "IoBuildDeviceIoControlRequest",
    "IoCallDriver", "IoCompleteRequest", "IoAcquireCancelSpinLock",
    "IoReleaseCancelSpinLock", "IoCsqInitialize", "IoCsqInitializeEx",
    "IoCsqInsertIrp", "IoCsqInsertIrpEx", "IoCsqRemoveIrp",
    "IoCsqRemoveNextIrp",
):
    if symbol not in exports or f'"name": "{symbol}"' not in manifest:
        errors.append(f"missing kernel export or manifest entry: {symbol}")
    if f'export_name = "{symbol}"' not in wdm:
        errors.append(f"missing PE export alias for {symbol}")

if errors:
    print("WDM source-contract checks FAILED:", file=sys.stderr)
    for error in errors:
        print(f" - {error}", file=sys.stderr)
    raise SystemExit(1)

print("WDM source-contract checks passed.")

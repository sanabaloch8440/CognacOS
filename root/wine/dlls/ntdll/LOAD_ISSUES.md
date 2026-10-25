# NTDLL loading compatibility issue log

This file is the working issue ledger for attempting to map a real AMD64
`ntdll.dll` into a CognacOS process using the native `ntoskrnl` PE loader.
It separates **mapping/export registration** from **executing Windows NTDLL code**.
Those are different milestones: a stock Windows NTDLL uses Windows' privileged
`SYSCALL` entry and Windows build-specific service numbers, while CognacOS's
native NTDLL uses the CognacOS DPL3 interrupt ABI.

## Reference image inspected

The user-provided Windows x64 `ntdll.dll` was inspected locally (the Microsoft
binary is not copied into this repository):

- File size: 2,023,688 bytes.
- AMD64 PE32+, DLL characteristic set; `SizeOfImage = 2,052,096` bytes.
- 10 sections; preferred image base `0x180000000`; entry-point RVA is zero.
- Import directory is empty; TLS directory is empty.
- Export directory has 2,430 function slots and 2,429 named exports; no literal
  forwarded-export RVAs were found.
- Exception directory is 58,776 bytes, i.e. 4,898 x64 runtime-function records.
- Base relocations contain 631 DIR64 entries and 5 ABSOLUTE padding entries.
- Load-config directory is present (280 bytes). CognacOS currently does not
  interpret its CFG/load-config contents; mapping the image is not proof that
  Windows mitigation semantics are implemented.

## Issues and fixes

### NTDLL-LOAD-001 — real `.pdata` exceeds the runtime-function table cap

**Observed:** the Windows image has 4,898 runtime-function records, while the
kernel runtime metadata store originally accepted only 2,048. The loader would
reject it during runtime-function registration, before export registration.

**Fix committed:** increase the bounded table to 8,192 entries in
`kernel/src/pe_runtime.rs`. This accommodates this image while retaining a
hard cap. A separate stress test is still needed to prove exhaustion and
concurrent registration behavior.

### NTDLL-LOAD-002 — export registration cap is too small

**Observed:** the export parser/store originally allowed 512 function slots;
the reference image has 2,430. The loader would reject its export directory.

**Fix committed:** raise the bounded export capacity to 4,096 in
`kernel/src/winabi.rs`. The reference image's longest named export is 53 ASCII
bytes, within the current 64-byte name limit.

### NTDLL-LOAD-003 — native syscall vector conflicts with callback return

**Observed:** native NT syscalls and the user-mode callback return trampoline
both used interrupt vector `0x82`. The later IDT registration could replace the
other handler, so either a native NTDLL syscall or a DLL callback return could
enter the wrong path.

**Fix committed:** vector `0x82` is reserved for native NT syscalls; callback
return now uses vector `0x83`. The legacy process exit path remains on `0x81`.

### NTDLL-LOAD-004 — boot path did not stage NTDLL for the kernel

**Observed:** the UEFI handoff included registry hives and NLS assets but no
NTDLL image, so the kernel could only test synthetic PE fixtures.

**Fix committed:** the bootloader optionally stages
`\\Windows\\System32\\ntdll.dll` or falls back to
`\\EFI\\BOOT\\ntdll.dll`, passes the bounded file descriptor through boot
info version 9, and the kernel validates the descriptor before exposing bytes.
The UEFI workflow builds CognacOS's freestanding native NTDLL, places it at
`EFI/BOOT/ntdll.dll`, and requires QEMU log assertions for the load test.

**Security note:** this optional test asset is not yet independently signed or
hash-pinned. Do not treat this staging mechanism as production code-integrity
enforcement.

### NTDLL-LOAD-005 — every relocated image competed for one fallback base

**Observed:** images whose preferred base is outside CognacOS's user range all
fell back to one fixed VA. Loading a second DLL in the same process could fail
even when ample user VA remained.

**Fix committed:** the PE loader now probes up to 1,024 64-KiB-aligned fallback
candidates, checking every page before selecting one. The preferred Windows
base `0x180000000` is intentionally relocated because it is below CognacOS's
currently supported user range.

### NTDLL-LOAD-006 — native wrappers had no end-to-end ring-3 test

**Observed:** prior tests called the kernel dispatcher directly, bypassing the
actual DLL exports, x64 wrapper ABI, and interrupt transition.

**Fix committed:** QEMU boot now maps the built native NTDLL and registers its
real export table. When the CognacOS-only configuration export is present, a
ring-3 smoke program configures IDs `0x100`–`0x104` and calls
`NtQueryInformationProcess` (five arguments), `NtQueryInformationThread`
(five arguments), `NtQuerySystemInformation`, and `NtYieldExecution`. It
checks NTSTATUS values and key output fields before exiting. A stock Windows
NTDLL without the CognacOS configuration export is mapped and export-checked,
but its service stubs are deliberately **not executed**.

## Remaining issues

- **NTDLL-LOAD-007 — no transactional rollback for a failed PE load.** A malformed
  image can fail after pages or runtime metadata have been allocated/registered.
  Add a load transaction that unmaps pages and removes runtime/export state on
  every error path; this is not yet fixed.
- **NTDLL-LOAD-008 — Windows load-config/CFG is not interpreted.** The reference
  image's load-config directory is ignored. CFG checks, security-cookie setup,
  loader flags, and mitigation metadata must be implemented before executing
  the stock Windows binary.
- **NTDLL-LOAD-009 — stock Windows syscall stubs are incompatible.** The stock
  image's wrappers cannot be called as CognacOS services. Implement a verified
  Windows-NT syscall-stub translation layer or use the CognacOS-native NTDLL;
  never feed Windows syscall numbers directly into the CognacOS dispatcher.
- **NTDLL-LOAD-010 — this is not yet the full Windows loader.** PEB/TEB/LDR,
  TLS index allocation, loader notifications, API-set resolution, unwind/SEH,
  and module dependency order are incomplete. The current QEMU smoke is a narrow
  ABI test, not a Windows application compatibility claim.
- **NTDLL-LOAD-011 — global module/export/runtime registries need concurrency
  and lifetime hardening.** Registration and teardown are not yet transactionally
  synchronized against concurrent lookup or process destruction.

## Reproducing the tests

The regular UEFI workflow builds the freestanding CognacOS NTDLL and stages it
as `EFI/BOOT/ntdll.dll`. The kernel's optional asset lookup also accepts a
stock Windows NTDLL at that path. A stock Windows image can therefore be used
for mapping/export-table testing by replacing the staged file in a local EFI
image; the ring-3 native-wrapper test is expected to skip for that binary.
The Windows binary itself is intentionally excluded from this repository.

### NTDLL-LOAD-012 — native DLL had no relocation directory

**Observed in QEMU:** the first actual boot attempt reached the staged native
DLL and failed with `RelocationsRequired`. Its x64 code was otherwise position
independent, so the linker omitted `.reloc`; the image base is outside the
CognacOS user range and the loader correctly refused to move an image that
advertised no relocation records.

**Fix committed:** `cognac_native.c` now exports an image-anchor function pointer
initialized to `DllMain`. That creates a real DIR64 base relocation, allowing
the loader to relocate the native DLL without weakening the loader's
`RelocationsRequired` rule. The next QEMU run must confirm the relocation
directory is present and that mapping advances beyond this failure.

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

- File size: 2,023,688 bytes; SHA-256: `eee317568e7901913399dd8fc3c804a35a5466f167aed279c652e1797f0a8bf6`.
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
as `EFI/BOOT/ntdll.dll`. To test a stock Windows binary without committing the
Microsoft file, manually dispatch [the UEFI workflow](https://github.com/umarfarooq7869/CognacOS/actions/workflows/build-uefi.yml)
and set the optional `ntdll_url` input to a URL for either a raw AMD64
`ntdll.dll` or a ZIP containing it. The workflow downloads and validates the
PE32+ image, stages it in the EFI volume, and QEMU attempts real kernel mapping
and export registration. It deliberately skips execution when the
CognacOS-specific syscall configuration export is absent. The current CI
push-triggered path leaves `ntdll_url` empty and tests the freestanding
CognacOS NTDLL. The Windows binary itself is intentionally excluded from this
repository.

### NTDLL-LOAD-012 — native DLL had no relocation directory

**Observed in QEMU:** the first actual boot attempt reached the staged native
DLL and failed with `RelocationsRequired`. Its x64 code was otherwise position
independent, so the linker omitted `.reloc`; the image base is outside the
CognacOS user range and the loader correctly refused to move an image that
advertised no relocation records.

**Fix committed and QEMU-verified:** `cognac_native.c` exports an image-anchor
function pointer initialized to `DllMain`, producing a DIR64 relocation. The
native DLL subsequently mapped and registered its exports in QEMU without
weakening the loader's `RelocationsRequired` rule.

### NTDLL-LOAD-013 — smoke caller wrote its fifth argument above the stack

**Observed in QEMU:** NTDLL mapped successfully, its export table registered,
and DllMain returned. The smoke trampoline then faulted writing the fifth
argument at `[RSP+0x20]`: it reserved only the 32-byte shadow space, placing
that slot at the unmapped top of the user stack.

**Fix committed and QEMU-verified:** the test trampoline reserves 0x30 bytes
(32-byte shadow space plus the fifth argument slot), preserving 16-byte
alignment before calls. The native wrapper sequence now passes in QEMU.

### NTDLL-LOAD-014 — smoke accounting ignored native NT syscalls

**Observed in QEMU:** after the fifth-argument stack fix, the smoke process
exited with status zero and the expected `Exited` state, but validation still
reported failure because the shared dispatch counter increased only once for
the legacy process-exit call. Native NT dispatches were not incrementing that
counter, so the validation condition could not see them.

**Fix committed and QEMU-verified:** `cognacos_nt_syscall_dispatch` increments
the shared dispatch counter for each native NT interrupt. The ring-3 smoke
validated table configuration, both five-argument queries, the system query,
yield, and a zero process exit in QEMU run
[40176341592](https://github.com/umarfarooq7869/CognacOS/actions/runs/40176341592).

### NTDLL-LOAD-015 — yield switched from a bootstrap process with no ETHREAD

**Observed in QEMU:** the native NTDLL wrapper sequence and the existing PE
ring-3 fixture both passed, but the boot log stopped after the final loader
smoke. The NTDLL smoke calls `NtYieldExecution` before the bootstrap user-mode
launcher has a scheduler-owned current thread; the kernel unconditionally
called `yield_current`, which can switch the bootstrap stack into unrelated
scheduler state.

**Fix committed and boot-smoke validated:** the native service now invokes
the scheduler only when `current_thread_id()` identifies a scheduler-owned
thread. The bootstrap smoke verifies the wrapper returns success without an
invalid context switch. The full UEFI workflow, including boot smoke and the 2/4/8-vCPU stress matrix,
passed in [Actions run 40176419581](https://github.com/umarfarooq7869/CognacOS/actions/runs/40176419581).

### NTDLL-LOAD-016 — smoke-process cleanup attempted reap before address-space detach

**Observed during lifecycle review:** `cleanup_process` called `process::reap`
before `AddressSpace::destroy`. The process reaper deliberately refuses to
recycle an EPROCESS while its address-space root is nonzero, so the call always
failed and each ring-3 loader smoke leaked a process-table slot.

**Fix committed; boot smoke passed:** cleanup now destroys/detaches the
address space first, then retries process reaping. This applies to the NTDLL
test and the existing PE fixture tests. The full UEFI workflow, including boot smoke and the 2/4/8-vCPU stress matrix,
passed in [Actions run 40176419581](https://github.com/umarfarooq7869/CognacOS/actions/runs/40176419581).

### NTDLL-LOAD-017 — Windows x64 SYSCALL entry and service-number dispatch

**Implemented; QEMU validation pending for this revision.** The x64 kernel now
programs IA32_EFER.SCE, IA32_STAR, IA32_LSTAR and IA32_FMASK on the BSP and each
initialized AP. A per-CPU kernel stack is selected through IA32_KERNEL_GS_BASE
and SWAPGS, because SYSCALL itself does not change RSP. The entry synthesizes
an IRETQ frame, saves all general-purpose registers, and dispatches the Windows
service number separately from CognacOS's private interrupt-0x82 service IDs.
The return path uses IRETQ and restores the user GS base; it does not use SYSRET
on unchecked user state.

The GDT was expanded to include ring-3 code/data descriptors at selectors
0x1b/0x23, moving the TSS descriptor to selector 0x28. This also corrects the
descriptor layout required for the existing IRETQ user-mode transition.

The following service IDs were extracted from the exact inspected reference
image (SHA-256 recorded above): NtClose=0x0f,
NtQueryInformationProcess=0x19, NtQueryInformationThread=0x25,
NtQuerySystemInformation=0x36, and NtYieldExecution=0x46. These IDs are
**specific to that NTDLL build**, not stable Windows ABI constants. Only these
five services are currently mapped to native handlers; all other numbers fail
closed with STATUS_NOT_IMPLEMENTED. Full Windows SSDT coverage, per-build
service-table generation, native handle semantics, and complete NTSTATUS/API
semantics remain outstanding.

The QEMU ring-3 smoke now issues raw SYSCALL instructions for system, process,
and thread queries, including fifth arguments on the Windows user stack. Stock
Windows NTDLL is not executed wholesale: only the focused ABI probe runs, while
its own runtime initialization and unrelated service calls remain unsupported.

### NTDLL-LOAD-018 — native NTDLL ring-3 smoke path was disabled

**Observed during compatibility review:** the boot path recognized the in-tree
CognacOS NTDLL, then unregistered and destroyed it without executing its
exports. The existing `run_native_ntdll_smoke` harness exercises the DLL's
table-configuration export, five-argument process/thread queries, system query,
yield wrapper, and process exit through the CognacOS INT 0x82 ABI. It also
contained a separate raw Windows `SYSCALL` probe after an earlier write call,
mixing two distinct ABIs in one test.

**Fix committed:** the native DLL path now runs its ring-3 smoke harness and
fails the boot test if it cannot run or validate. The stock Windows NTDLL
`SYSCALL` probe remains a separate test path. The native harness no longer
executes the unrelated raw Windows service-number sequence. The UEFI workflow
now requires the explicit log marker
`CognacOS native NTDLL ring-3 syscall wrappers passed.`

**Validation status:** pending the new GitHub Actions/QEMU run. This restores
execution of the native bootstrap subset; it does not imply that the full
Windows NTDLL export surface or complete Windows process runtime is implemented.


### NTDLL-LOAD-019 — smoke cleanup double-destroyed the address space

**Observed in QEMU:** the native NTDLL ring-3 wrapper smoke completed, but the
boot then failed with `native NTDLL cleanup failed`. The user-mode runner's
`cleanup_process` already unregisters the loaded modules, destroys the process
address space, and reaps the process. The boot path attempted to destroy that
same address space a second time.

**Fix committed:** the native and stock-Windows smoke branches now leave
address-space cleanup to `run_loaded_image`; the map-only unsupported-image
branch retains its own cleanup. This was reproduced in
[Actions run 40183393568](https://github.com/umarfarooq7869/CognacOS/actions/runs/40183393568)
and corrected in commit `28f075e`. The next run passed the native ring-3 smoke
and proceeded to the SMP stress matrix.

### NTDLL-LOAD-020 — native RTL string and status helper expansion

**Implemented:** added native exports for ANSI/Unicode string initialization,
comparison, Unicode copying, Unicode-string/integer conversion, NT version
numbers, and NTSTATUS-to-Win32-error conversion. QEMU smoke calls the actual
mapped export addresses and validates outputs. The initial string API test set
passed in [Actions run 40183506729](https://github.com/umarfarooq7869/CognacOS/actions/runs/40183506729).
The version/status smoke passed in the boot-smoke phase of [Actions run 40183695385](https://github.com/umarfarooq7869/CognacOS/actions/runs/40183695385). Additional CRC32 and memory-operation tests are now added and await their workflow result.

**Known limitations:** case-insensitive string comparison folds ASCII only;
the status mapping covers only a small explicit set; these exports are not a
substitute for Wine's full NTDLL runtime or Microsoft's complete export ABI.


### NTDLL-LOAD-021 — default process heap bootstrap

**Implemented:** added a bounded in-image heap to native NTDLL with allocation,
free, reallocation, size, and validation exports. The loader now sets
`PEB.ProcessHeap` to the exported per-process heap object before user entry.
The implementation uses aligned first-fit blocks, splitting/coalescing, and a
lock; `HEAP_ZERO_MEMORY` is supported.

**QEMU-verified:** [Actions run 40184178016](https://github.com/umarfarooq7869/CognacOS/actions/runs/40184178016) passed the native ring-3 smoke and the 2/4/8-vCPU stress matrix, including heap handle identity, allocation/size, realloc data preservation, free/double-free rejection, and zeroed reuse.

**Limitations:** this is a fixed 256 KiB process heap, not the Windows heap
manager. Custom heaps, LFH, heap instrumentation/callbacks, exception flags,
and exact compatibility for all heap flags are not implemented.


### NTDLL-LOAD-022 — GS-based current PEB accessor

**Implemented:** exported `RtlGetCurrentPeb`, reading the Windows x64 TEB/PEB pointer through `GS:[0x60]`. The process launcher already installs the TEB at the user GS base and now exposes the default process heap in `PEB.ProcessHeap`. The ring-3 smoke verifies the returned PEB address and its heap pointer against the mapped DLL export.

**QEMU-verified:** the accessor and PEB.ProcessHeap checks passed in the boot-smoke phase of [Actions run 40184395068](https://github.com/umarfarooq7869/CognacOS/actions/runs/40184395068). This is still a narrow PEB access primitive; complete TEB fields, loader state, TLS, and process startup remain unfinished.


### NTDLL-LOAD-023 — TEB last-error and last-status helpers

**Implemented:** native exports read/write x64 TEB `LastErrorValue` at GS offset
`0x68` and `LastStatusValue` at GS offset `0x1250`. The combined status helper
maps NTSTATUS through the bootstrap status table and updates both TEB fields.

**QEMU-verified:** ring-3 smoke tests passed in [Actions run 40184485240](https://github.com/umarfarooq7869/CognacOS/actions/runs/40184485240), covering direct last-error set/get and `STATUS_INVALID_PARAMETER` conversion to `ERROR_INVALID_PARAMETER`, including preservation of the original NTSTATUS across native INT 0x82 calls.


### NTDLL-LOAD-024 — LdrGetProcedureAddress bootstrap

**Implemented:** a freestanding export resolver validates the loaded PE32+ headers and export-directory bounds, supports exact ANSI name and ordinal lookup, initializes the output pointer to null, and rejects forwarded exports. The QEMU smoke resolves `RtlGetCurrentPeb` through the mapped NTDLL base. Initial validation failed before ring-3 execution because the generated test stub exceeded its 3072-byte code buffer; the buffer was increased to one page (4096 bytes). The expanded 4096-byte stub passed in the boot-smoke phase of [Actions run 40185018220](https://github.com/umarfarooq7869/CognacOS/actions/runs/40185018220), including the `RtlGetCurrentPeb` name lookup.

### NTDLL-LOAD-025 — LdrGetDllHandle bootstrap

**Implemented:** a freestanding lookup traverses the active PEB's load-order list, compares `BaseDllName` values case-insensitively for ASCII, and returns the mapped base for an already-loaded module. It does not load DLLs or resolve search paths.

**QEMU-verified:** [Actions run 40185018220](https://github.com/umarfarooq7869/CognacOS/actions/runs/40185018220) resolved `ntdll.dll` through the PEB load-order list and verified the returned handle equals the actual mapped base. Full `LdrLoadDll`, forwarded exports, loader lock, activation contexts, and dependency graph/rundown remain future work.

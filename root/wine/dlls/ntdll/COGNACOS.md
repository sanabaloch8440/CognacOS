# CognacOS NTDLL

NTDLL development lives directly in this directory. The native build excludes
Wine's Unix companion library, server, syscall dispatcher, POSIX support
libraries, and `unix/*.c` sources. Build the freestanding x64 PE32+ DLL with:

```sh
make -C root/wine/dlls/ntdll -f Makefile.cognac clean all check
```

## Compile-time syscall link helper

`tools/ntdll_link_helper.py` parses an x64 PE32+ `ntdll.dll` without executing
it, extracts canonical `Nt*`/`Zw*` syscall stubs, and emits a header containing
service IDs for that exact Windows build. To generate the header from a known
Windows 11 installation:

```sh
make -C root/wine/dlls/ntdll -f Makefile.cognac syscall-header \
  WINDOWS_NTDLL=/path/to/Windows/System32/ntdll.dll
make -C root/wine/dlls/ntdll -f Makefile.cognac clean all check \
  WINDOWS_SYSCALL_IDS=1
```

The IDs are build-specific. Rebuild the header from the exact target OS image;
never treat them as stable constants or copy them to another Windows build.
This optional mode is for side-by-side ABI experiments, not a supported
replacement for the protected system `ntdll.dll`. Windows expects a much
larger export surface and strict loader/runtime semantics that this bootstrap
does not yet implement.

## CognacOS kernel binding

The five implemented native NT exports use canonical Windows x64 syscall stubs:
`mov r10, rcx; mov eax, service_number; syscall; ret`. At boot, CognacOS
parses the actual PE export directory and extracts service numbers from the
staged NTDLL stubs; it does not depend on a private syscall-table configuration
export. The current default IDs match the inspected Windows 11 reference build:
`NtClose=0x0f`, `NtQueryInformationProcess=0x19`,
`NtQueryInformationThread=0x25`, `NtYieldExecution=0x46`, and
`NtQuerySystemInformation=0x36`. These IDs are not stable across Windows
builds. To target another exact Windows NTDLL, use the compile-time helper above
to generate the header and build with `WINDOWS_SYSCALL_IDS=1`; the kernel
still discovers the resulting values from the exported stubs at boot.

The native wrappers use the Windows x64 syscall argument convention: RCX is
copied to R10, arguments two through four remain in RDX/R8/R9, and later
arguments remain on the caller's stack at the Windows-defined offsets. The
kernel maps the five discovered service numbers to the implemented native
Object Manager, scheduler, and limited process/thread/system information paths.
Unsupported information classes and invalid handles are rejected. This is an
initial supported subset, not full Windows semantics.

The native bootstrap also exports a bounded 256 KiB per-process default heap
through `RtlGetProcessHeap`, `RtlAllocateHeap`, `RtlFreeHeap`,
`RtlReAllocateHeap`, `RtlSizeHeap`, and `RtlValidateHeap`; the loader points
`PEB.ProcessHeap` at that image-local heap object. Custom heaps, LFH, heap
callbacks/instrumentation, and the complete Windows heap contract remain
unsupported. Other implemented exports include core RTL memory/CRC routines,
NT version and status conversion helpers, ANSI/Unicode string operations,
PEB/TEB accessors, and basic loader lookup APIs. Case-insensitive string
comparison currently folds ASCII only; full Windows Unicode upcase-table
semantics remain outstanding.

GitHub Actions builds and checks the PE32+ artifact and validates the helper's
CLI. This does not claim full Windows 11 compatibility. Loader initialization,
dependency loading, forwarded exports, process/thread startup, exception/unwind,
TLS, file, registry, security, and ALPC semantics remain incomplete.

## RTL routines shared with the kernel

The supplied Windows x64 NTDLL image is used as an ABI/export reference, not
as code to copy into CognacOS. Its export directory contains 2,429 named
exports; inspection found no literal PE forwarded-export entries. Therefore,
a function that also exists in ntoskrnl should not automatically be treated as
a PE forwarder: user-mode NTDLL and kernel-mode RTL implementations have
different execution contexts and must be bound across the OS boundary only
through an explicit supported ABI.

As an interim implementation, `root/wine/dlls/ntoskrnl.exe/rtl_memory.c`
now provides kernel-side `RtlCopyMemory`, `RtlMoveMemory`,
`RtlFillMemory`, `RtlZeroMemory`, `RtlCompareMemory`,
`RtlFillMemoryUlong`, and `RtlCompareMemoryUlong`. The Wine ntoskrnl
compatibility module builds this file from its Makefile. These implementations
avoid heap/server calls and use bytewise operations, including overlap-safe
move and unaligned buffers.

This is a staging point, not the final native kernel implementation: the
Wine `ntoskrnl.exe` module still has Wine/Win32 runtime dependencies. The
same ABI and tests must be carried into CognacOS's native `ntoskrnl` before
these routines can be considered production kernel exports. NTDLL retains its
own user-mode copies where appropriate; user-mode exports cannot directly
jump into privileged kernel code.

## Native execution regression gate

The UEFI/QEMU boot workflow executes the in-tree native NTDLL in ring 3. The
five `Nt*` exports are now canonical Windows x64 `SYSCALL` stubs; the kernel
discovers their service numbers from the staged PE export directory and enables
the matching dispatcher before the smoke test. The smoke calls the actual
export function pointers, then exits through the test harness's separate
`INT 0x81` write/exit service. Native NT service calls no longer use the
private `INT 0x82` bridge or a Cognac-specific service-table configuration
export.

This remains a narrow compatibility milestone: only five native NT services
are implemented, and the DLL still lacks `LdrLoadDll`, forwarded-export
resolution, a full loader lock/dependency graph, comprehensive PEB/TEB state,
exception/unwind support, TLS semantics, and most NTDLL exports.

## Current QEMU test coverage

The native ring-3 smoke resolves and calls mapped export pointers. It covers
the five native NT syscall stubs; ANSI/Unicode initialization, comparison, and
copy; integer/string conversion; NT version and status conversion; CRC32 and
core RTL memory routines; the bounded default heap; PEB/TEB last-error fields;
`LdrGetProcedureAddress`; and `LdrGetDllHandle`. Unsupported NT information
classes and status mappings remain explicit gaps, not successful Windows
compatibility.

## Default process heap scope

The in-tree heap is intentionally a bounded bootstrap implementation backed by
writable NTDLL image data. It supports first-fit allocation, aligned blocks,
splitting/coalescing, free, realloc, size, validation, and zero-memory
allocation. Its exported heap object is installed into the PEB before ring-3
entry. It does not yet implement custom heap creation/destruction, low-fragmentation
heap behavior, debug/instrumentation flags, exception generation, or the full
Windows heap ABI. The heap lifecycle passed in [Actions run 40184178016](https://github.com/umarfarooq7869/CognacOS/actions/runs/40184178016), including allocation, size, realloc preservation, free/double-free rejection, and zeroed reuse.

## PEB access

The native DLL now exports `LdrGetProcedureAddress` for names/ordinals in already-mapped PE32+ images and `LdrGetDllHandle` for modules already present in `PEB.Ldr`; forwarded exports and dynamic DLL loading are not implemented. It also exports `RtlGetCurrentPeb` using the x64 GS-relative PEB pointer, plus `RtlGetLastWin32Error`, `RtlSetLastWin32Error`, `RtlGetLastNtStatus`, and `RtlSetLastWin32ErrorAndNtStatusFromNtStatus` using the documented TEB offsets. The process launcher installs the TEB as the user GS base, and the ring-3 smoke checks that `RtlGetCurrentPeb` returns the active PEB and that its `ProcessHeap` field points at the native heap object. The accessor and PEB.ProcessHeap checks passed in the boot-smoke phase of [Actions run 40184395068](https://github.com/umarfarooq7869/CognacOS/actions/runs/40184395068).

The TEB error/status routines are now exercised in ring 3: setting and reading
LastErrorValue, then mapping `STATUS_INVALID_PARAMETER` into LastErrorValue while recording
LastStatusValue. These tests passed in the boot-smoke phase of [Actions run 40184485240](https://github.com/umarfarooq7869/CognacOS/actions/runs/40184485240).

## Loader API subset

`LdrGetProcedureAddress` validates the in-memory PE32+ export directory, resolves exact case-sensitive ANSI names or ordinals, and rejects forwarded exports rather than treating forwarder strings as executable addresses. `LdrGetDllHandle` traverses the PEB load-order list and compares the loaded module's BaseDllName using ASCII case folding. Both are intentionally limited to already-mapped modules; `LdrLoadDll`, dependency resolution, forwarder resolution, activation contexts, and loader-lock/rundown semantics are not yet implemented. Both loader APIs passed in the boot-smoke phase of [Actions run 40185018220](https://github.com/umarfarooq7869/CognacOS/actions/runs/40185018220). The generated test stub now fits the single executable page.

## RTL bitmap Rust port

The Rust source-first module `rust/rtl_bitmap.rs` ports the principal
`rtlbitmap.c` APIs: bitmap initialization and bulk/range updates, range
predicates, set/clear run search and mutation, set/clear counts, most/least
significant-bit scans, forward/backward run lookup, run enumeration, and
longest-run helpers. It uses the Windows x64 `RTL_BITMAP` and
`RTL_BITMAP_RUN` layouts, LSB-first bit numbering, tail-bit masking for
counts, and allocation-free run enumeration. Unit tests cover range updates,
counts, search/mutate, bit scans, forward/backward runs, and non-word-aligned
bitmap sizes.

The Rust file is not yet connected to the freestanding PE32+ build; these
unit tests have not been reported as executed in CI. Invalid pointers cannot
be made generally safe by a raw-pointer ABI, and this bootstrap implementation
is not yet a verified replacement for every Windows RTL bitmap edge case.
The original C file's backward-run functions are stubs; this port implements
those routines rather than copying the stubs.

## Large-integer string conversion Rust port

The x64-active conversion routines from Wine's `large_int.c` now have a
source-first Rust counterpart at `rust/rtl_large_int.rs`:
`RtlLargeIntegerToChar` and `RtlInt64ToUnicodeString`. The port preserves
base 0 as decimal, the supported bases (2, 8, 10, 16), ANSI exact-fit behavior
(no NUL when the digit count exactly equals the supplied length), UTF-16 byte
length reporting, and overflow status behavior. Unit tests cover base
conversion, exact-fit ANSI output, invalid bases, and UTF-16 overflow/termination.

The other arithmetic routines in `large_int.c` are under a `!WIN64` guard
and are not part of this x64 port. As with the other Rust RTL modules, this file
is not yet linked into the freestanding PE32+ NTDLL build, so these tests have
not been claimed as executed by the native NTDLL workflow.

## PEB/TEB access and environment synchronization

The native x64 bootstrap now exposes `RtlGetCurrentTeb` and uses the Windows
GS-relative TEB fields (`GS:0x30` for TEB self, `GS:0x48` for the current
thread ID, and `GS:0x60` for the PEB). PEB.FastPebLock is read at PEB offset
`0x38`; ProcessParameters is at PEB offset `0x20`, and its Environment
pointer is at process-parameters offset `0x80`. The launcher must initialize a
valid Windows-layout TEB/PEB before these APIs can be called.

The first native `RTL_CRITICAL_SECTION` implementation uses the standard
x64 structure layout, atomic acquisition, recursive ownership, and periodic
`NtYieldExecution` while contended. It deliberately does not claim full
Windows behavior: the event-backed waiter/wake path, contention fairness,
critical-section initialization/deletion, and concurrency stress testing remain
open. PEB lock operations are therefore suitable as bootstrap synchronization,
not yet a production-complete Windows critical-section implementation.

The Rust `env.rs` source now uses this PEB accessor and lock for inherited
environment creation, current-environment replacement, and null-environment
query/set/expansion. Rust modules remain source-first and are not yet part of
the freestanding PE build; the native C exports are what the current PE workflow
compiles and checks.


## ALPC message-attribute Rust port

The source-first module `rust/alpc.rs` ports the three attribute helpers from
Wine's `alpc.c`: `AlpcGetHeaderSize`, `AlpcGetMessageAttribute`, and
`AlpcInitializeMessageAttribute`. It models the x64 attribute structures
declared in the vendored `include/winternl.h`, computes storage from the
recognized ALPC attribute flags, preserves the descending-bit attribute
layout, reports required buffer sizes, and initializes the allocated/valid
attribute masks.

Unit tests cover the recognized-flag size calculation, insufficient and
sufficient buffer sizes, attribute pointer ordering, invalid/multiple flags,
and x64 structure sizes. These tests are present in source but have not been
reported as executed. The Rust module remains unlinked from the freestanding
PE32+ build; full ALPC port/communication syscalls and Windows runtime ABI
validation are not part of this change.


## Math helpers Rust port

The source-first module `rust/math.rs` ports the architecture-independent
helpers in Wine's `math.c`: `abs` and `math_error`. `math_error` preserves
Wine's current behavior of returning the supplied result without processing
the error metadata. The C source's `_CIcos`, `_CIlog`, `_CIpow`, `_CIsin`,
`_CIsqrt`, and `_ftol` implementations are i386-only x87 stack helpers and
are not emitted for the x86-64 target.

Tests cover ordinary positive/negative values, the INT_MIN wrapping edge case,
and return-value preservation including NaN and infinity. The tests are added
but not yet confirmed as executed. This module is not linked into the native
PE32+ build.

**ALPC scope note:** `rust/alpc.rs` covers every function present in the
vendored `alpc.c` (the three message-attribute helpers). That does not mean
the complete Windows ALPC subsystem is implemented: port creation/connect,
message send/receive, handle/view transfer, and kernel-side ALPC object and
syscall support remain separate work.


## Miscellaneous NTDLL helpers Rust port

The source-first module `rust/misc.rs` ports the sorting and searching
helpers from Wine's `misc.c`: `qsort_s`, `qsort`, `bsearch_s`,
`bsearch`, and `_lfind`. Sorting uses an in-place heapsort implementation
with bounded stack use and (O(n log n)) worst-case time; search helpers
preserve comparator ordering and return matching element pointers. Argument
validation and checked element-offset arithmetic prevent size overflow from
turning into an invalid calculated offset.

The module also mirrors the current Wine stub return behavior for the SQM and
ETW entry points represented in `misc.c`. Those functions are placeholders,
not a functioning telemetry/event-tracing service. The Wine debug-only
`debugstr_us` helper and C-variadic `EtwTraceMessage` wrapper are not ported:
they depend on Wine's debug runtime and C variadic forwarding respectively.
The Rust module is not linked into the freestanding PE32+ build, and its unit
tests have not yet been confirmed as executed in CI. Callback ABI, invalid
raw-pointer behavior, and Windows runtime compatibility still require native
integration and testing.


## Printf format parser Rust port

The source-first module `rust/printf.rs` ports the format-specifier grammar
used by Wine's `printf.c` / `printf.h`: flags, static and dynamic width,
precision, integer length modifiers, wide-string markers, escaped percent
sequences, and conversion characters. It also provides overflow-safe signed
decimal digit extraction (including `i64::MIN`) and unsigned digit conversion
for bases 2 through 16. Tests cover common flags/modifiers, dynamic fields,
multiple conversions, malformed/positional formats, and integer edge cases.

This is deliberately **not** a complete port of the exported formatting APIs.
C `va_list` argument decoding, string encoding conversions, buffer/truncation
semantics, floating-point conversions, and the `_snprintf`/`_swprintf` ABI
wrappers remain unimplemented in Rust. The module is not linked into the
freestanding PE32+ build, and its tests have not yet been confirmed as executed
in CI.


## Expanded printf formatter Rust port

The source-first module `rust/printf.rs` now goes beyond parsing: it formats
signed and unsigned integers (decimal/octal/hex), pointer values, byte and
UTF-16 strings, characters, floating-point values, escaped percent signs, and
dynamic width/precision from a typed argument list. It applies sign, alternate,
precision, alignment, and padding rules, reports the full required output
length, and safely truncates to a caller-supplied byte capacity. Tests cover
integer edge cases, pointer formatting, dynamic width/precision, floats, and
truncation.

This is a formatter engine, **not yet a complete replacement for Wine's
exported printf API**. Stable Rust cannot transparently forward the Windows C
variadic `va_list` ABI through ordinary Rust functions, so `_vsnprintf`,
`_vsnwprintf`, `_snprintf_s`, `_snwprintf_s`, `sprintf`/ `swprintf` and
related exports still need a target-specific ABI bridge. The current engine
also does not implement the `%n` pointer write, exact Wine ANSI/Unicode
conversion semantics, or UTF-16 output buffers; byte output converts wide
strings lossily. Rust sources remain unlinked from the native PE32+ build, and
the new unit tests have not yet been confirmed by CI.


## Native ANSI/UTF-16 printf ABI bridge

The freestanding PE32+ build now compiles `cognac_printf.c` and exports the
variadic and `va_list` entry points `_vsnprintf`, `_vsnwprintf`,
`_vscprintf`, `_vscwprintf`, `_snprintf`, `_snwprintf`, their
`_s` variants, and the `sprintf`/`swprintf` family. The wrappers use the
target compiler's Windows x64 variadic ABI and forward into one formatter;
the PE check now requires these names in the export table. ANSI output is
byte-oriented, wide output is UTF-16 code-unit-oriented, and cross-width
string conversion uses a deterministic Windows-1252 mapping instead of
lossily converting UTF-16 to UTF-8 bytes.

This is a native ABI bridge and an initial compatibility implementation, not
yet an exact Wine/Windows CRT clone. It currently implements integer/pointer
formatting, basic string/character formatting, `%n`, and fixed-point
`f/F` output. Scientific/general floating formats (`e/E/g/G`) return an
error; locale-sensitive ACP/NLS behavior, surrogate-pair handling, exact
secure-CRT invalid-parameter behavior, and exhaustive width/precision edge
cases remain. The wrappers are now in the native build, unlike the source-first
Rust formatter, but compilation and export checks must pass in GitHub Actions
before this bridge is considered validated.


## Rust port: Wine NTDLL version.c

Added `rust/version.rs` as a source-first Rust port of the deterministic
version-comparison logic from Wine's `version.c`. It defines the x64
`RTL_OSVERSIONINFOEXW` ABI layout, Windows `VER_*` masks and comparison
conditions, comparison-condition merging, build/platform/product-type
comparisons, suite-mask AND/OR behavior, and the version-checking core used by
`RtlVerifyVersionInfo`. It also includes a field-copy helper that validates
the caller's advertised base-structure size before writing the version data.

The module deliberately accepts the authoritative current OS version as input.
It does not hard-code a Windows build or export a misleading `RtlGetVersion`
implementation. The native export wrapper still needs to obtain version data
from CognacOS's actual shared-data/version provider and the Rust module is not
yet linked into the freestanding PE32+ build. Tests cover ordering conditions,
major/minor comparison, suite/product checks, invalid masks, and x64 structure
size; CI has to confirm compilation and execution before the port is considered
validated.


## Rust port status: `version.c` (continued)

The Rust source `rust/version.rs` now also includes source-derived helpers for
`RtlGetNtVersionNumbers`, `RtlGetNtProductType`, and Wine's current
`RtlGetProductInfo` policy, alongside the version-comparison core and `VerSetConditionMask` condition-slot builder. The
version-number helper preserves Wine's documented-in-source high build nibble
(`0xF0000000 | build`) and accepts null output pointers. Version-info copying
handles the base 276-byte `OSVERSIONINFOW` layout and the 284-byte extended
layout separately.

Scope remains deliberately partial: this does not port registry-backed
`version_init`, Wine's configuration/version aliases, or the NT/Win9x registry
parsers. Product SKU selection follows Wine's simplified behavior, not the full
Windows SKU catalogue. The Rust module is not yet wired into the freestanding
PE32+ export bridge, and its unit tests require execution in CI before this
work should be treated as validated.


### Additional `version.c` parser coverage

The Rust module now includes the Wine `Version` configuration alias table
(including the documented aliases such as `win2k`, `nt2000`, `winvista`,
and `win2k8r2`), an NT `CurrentVersion` string parser, and a Win9x
`VersionNumber` parser that preserves the packed major/minor bits in the
build field. These are pure parsing helpers with unit tests; they do not yet
open registry keys or select a version profile at process startup. Registry
access, application-specific `AppDefaults\\app.exe` precedence, version
profile data records, and PEB version-field initialization remain to be wired
to CognacOS's real registry and process initialization services.


### Version profile records and selection precedence

The Rust version port now contains source-derived RTL_OSVERSIONINFOEXW records for each Wine Version alias in version.c, including legacy Win32s and Win9x records, NT workstation/server distinctions, service-pack strings, suite masks, and the source's existing legacy/reserved values. The pure select_version_profile helper models the selection order: a recognized application-specific alias wins, then a recognized global alias, then the caller-supplied registry/OS record. Tests cover representative workstation and server records, UTF-16 service-pack strings, and selection/fallback precedence.

This is still a source-first Rust implementation: it does not query HKCU\\Software\\Wine\\AppDefaults, HKCU\\Software\\Wine, or the NT/Win9x registry itself; it does not yet initialize the process PEB version fields or supply the native RtlGetVersion export. Unknown profile names fall back to the supplied version record rather than inventing a host version. The Rust module is not wired into the freestanding PE32+ NTDLL build, and these new unit tests have not yet been confirmed as executed in CI.


### NT and Win9x registry version payload parsers

Added pure Rust decoders for the value payloads consumed by Wine version.c's NT-style registry reader: complete CurrentMajorVersionNumber/CurrentMinorVersionNumber DWORD pairs, CurrentVersion string fallback, CurrentBuildNumber, UTF-16 CSDVersion, the packed Control\\Windows CSDVersion service-pack value, and ProductOptions ProductType (WinNT, LanmanNT, ServerNT). Added a Win9x VersionNumber/SubVersionNumber decoder and the source's NT-first, Win9x-second fallback selection helper. Unit tests cover type/value fallback, product type mapping, service-pack decoding, UTF-16 termination, malformed values, and packed Win9x build fields.

These helpers consume already-decoded values; they do not call NtOpenKey/NtQueryValueKey, validate registry value types themselves, or initialize PEB fields. The native registry bridge and startup wiring remain outstanding, and CI must execute the Rust tests before this slice is considered validated.


### Registry-backed version initialization and PEB wiring

The kernel process initializer now asks the existing Configuration Manager runtime
for the tuple stored at
`\\Registry\\Machine\\Software\\Microsoft\\Windows NT\\CurrentVersion`
and initializes the x64 PEB's `OSMajorVersion`, `OSMinorVersion`,
`OSBuildNumber`, and `OSPlatformId` fields when that tuple is present.
The registry reader accepts a complete DWORD major/minor pair, falls back to
`CurrentVersion` text when the pair is unavailable, and reads
`CurrentBuildNumber` as a decimal string. The native PE32+ NTDLL's
`RtlGetVersion` and `RtlGetNtVersionNumbers` now read those PEB fields
instead of returning a hard-coded Windows 10/build 26100 tuple. If the PEB has
no initialized version tuple, `RtlGetVersion` returns
`STATUS_NOT_IMPLEMENTED` rather than fabricating a version.

This is an initial live connection to CognacOS's current **volatile** registry
runtime, not complete registry/hive integration. The current runtime stores a
full path as a single root key and has bounded in-memory values; SYSTEM/SOFTWARE
hive loading and the normal native `NtOpenKey`/`NtQueryValueKey` user-mode
syscall path are not yet connected. Unless the matching key and values have
been provisioned in that runtime, the PEB version fields remain zero and the
NTDLL API reports the version as unavailable. The build workflow must verify
the changes; no CI result is implied by this source update.


### Hive-backed Configuration Manager and native registry syscalls

The UEFI bootloader already stages the seven native hive assets (SYSTEM, SOFTWARE,
SECURITY, SAM, DEFAULT, COMPONENTS, and BCD00000000) in the boot-information
structure. Kernel boot initialization now validates each REGF image and mounts it
into the Configuration Manager without converting its bytes. The registry runtime
can resolve absolute paths into the corresponding hive and exposes read-only
hive-backed key/value queries. `query_version_tuple()` now checks the
mounted SOFTWARE hive for `Microsoft\\Windows NT\\CurrentVersion` before
falling back to explicitly provisioned volatile values, so PEB version startup
can consume actual staged registry data.

The native PE32+ NTDLL now exports assembly syscall stubs for `NtOpenKey` and
`NtQueryValueKey`. The kernel extracts their service numbers from the staged
NTDLL PE, dispatches them through the Windows x64 syscall entry, copies
`OBJECT_ATTRIBUTES`, `UNICODE_STRING`, and value buffers through the active
process address-space copy helpers, then calls the Configuration Manager routines.
Relative opens under a hive-backed key and size-only `NtQueryValueKey` probes are
supported. The native query bridge currently stages at most 1024 bytes per value
call; larger values report the required length and need a larger-buffer strategy.

This is an initial **read-only hive-backed registry path**, not a complete Windows
Configuration Manager. The bootloader stages optional hives from the Windows
config paths or `\\EFI\\CognacOS\\Hives\\`; absent files are not synthesized.
Hive log replay/dirty-hive recovery, persistent writes, transactions, complete
registry object namespace semantics, all `KEY_VALUE_INFORMATION_CLASS` values,
and access-mask enforcement still need implementation. Volatile create/set
operations remain separate from hive-backed read-only values. CI status must be
checked on the newest commit before treating the integration as validated.

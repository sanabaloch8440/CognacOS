# NTDLL relay.c Rust port status

## Compatibility decision

Wine's `relay.c` is primarily a Wine diagnostics facility: it installs relay thunks into specially prepared built-in PE DLL exports so calls and returns can be traced, and it provides i386-only snoop hooks. It is not a Windows 11 NTDLL ABI component and is not required to load ordinary Windows PE applications or drivers.

**Do not wire this module into CognacOS's production NTDLL exports, loader startup, or syscall path.** Keep it as optional developer diagnostics. Native Windows compatibility work should prioritize exact NTDLL exports and behavior used by applications, the PE loader, process/thread startup, memory, synchronization, and syscalls.

## Implemented in Rust

- C-layout relay descriptor metadata.
- UTF-16 semicolon-list parsing and module/function include/exclude matching helpers.
- Signature marker decoding and non-dereferencing diagnostic word summaries.
- Bounds-checked lookup for a Wine relay descriptor marker in a mapped image.
- Procedure selection helper and conservative pass-through fallback entry points.
- Unit tests for list parsing, filtering, signature parsing, descriptor bounds, and fallback selection.

## Not yet ported

- Registry-backed `HKCU\\Software\\Wine\\Debug` initialization and heap lifecycle for filter lists.
- Actual per-architecture `relay_call` assembly and register/stack reconstruction for x86, x64, ARM, and ARM64.
- Safe string tracing under Windows structured exception handling.
- Live PE export-table patching, page-protection restoration, and thunk allocation.
- i386 SNOOP entry/return trampolines and return-frame management.

These pieces should only be added if CognacOS explicitly wants Wine-style API-call tracing. They should not be represented as native Windows 11 behavior. The Rust file intentionally does not export unmangled duplicate C symbols, to avoid collisions if Wine's original `relay.c` is still built.

## Validation

The code and tests are committed, but this document does not claim successful Rust compilation or runtime validation. Run the repository's actual Rust build/test target before treating this port as build-integrated.

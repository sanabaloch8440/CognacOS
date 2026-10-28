# CognacOS kernel32.dll — x64 stub stage

Kernel32 provides the Win32 base API namespace. This curated starter set covers common process/thread, memory, file, synchronization, console, environment, locale, and system imports.

## Files

- `exports/kernel32-x64.def`: explicit named PE exports.
- `exports/windows-11-x64-exports.txt`: curated name inventory, not a complete Windows build-specific export table.
- `src/stubs.c`: freestanding placeholder implementations.
- `build.sh`: links a PE32+ DLL using Clang and LLD without the Windows SDK.

## Stub contract

Most placeholders return zero (failure/null); selected HRESULT-style APIs return `E_NOTIMPL`; `GetLastError` returns `ERROR_CALL_NOT_IMPLEMENTED`; `SetLastError` is a no-op. A few pseudo-handle helpers return their documented pseudo-handle values. No parameter validation, object management, filesystem, process, registry, COM, or shell semantics are implemented. These simplified zero-argument ABI placeholders are for export/loader bring-up only and must not be treated as functional Win32 APIs.

## Build

With `clang` and `lld-link` installed, run `./build.sh`. Output: `build/kernel32.dll`. The DLL is not yet integrated into the boot image or loader search path; the CognacOS loader still needs to map it and resolve imports before applications can use it.

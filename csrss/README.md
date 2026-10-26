# CognacOS CSR server core

This directory defines the native CSR (Client/Server Runtime Subsystem) server core for the eventual `csrss.exe` user-mode image. It uses the Windows-style CSR API message envelope, bounded API dispatch, client process/thread bookkeeping, and explicit status returns for unknown API numbers.

## Current implementation boundary

- `CsrServerInitialize`: initializes dispatcher state.
- `CsrServerRegisterApi`: registers a handler under a complete CSR API number.
- `CsrServerDispatchMessage`: validates message sizes and routes only registered API numbers.
- `CsrServerRegisterClient` / `CsrServerUnregisterClient`: bounded client tracking with reference accounting.
- `CsrServerQueryState`: boot/debug status.
- Port name is exposed as a UTF-16 string using the Windows subsystem API-port naming convention.

## ALPC transport

The kernel-internal bounded transport core is in `kernel/src/ntoskrnl/alpc.rs`. It implements named port creation, listen state, queued connection requests, accept/reject, endpoint send/receive, sender identity checks, message-size limits, disconnect and port rundown. Messages are copied into kernel-owned bounded buffers, not retained as user pointers. The queue returns `STATUS_PENDING` when empty or resource-limited; scheduler-blocking waits, user handle/object attributes, ALPC section views, security contexts, and completion lists still need syscall/object-manager integration.

## CSRSRV export contract

`exports/csrsrv-x64.def` records a 32-export x64 inventory from the public Windows 10 x64 10.0.19041 reference. It is **not yet a verified exact Windows 11 export match**, and is not linked into a built `csrsrv.dll`. Public export inventories describe CSRSRV server DLL exports, which are distinct from exports of the `csrss.exe` process image.

Run `python3 tools/check_csrsrv_exports.py --reference /path/to/Windows11/System32/csrsrv.dll` to compare names and ordinals against an exact Windows 11 x64 image. Without that reference binary, CI can only validate the committed inventory's format.

## Still required for a bootable Windows-style subsystem

The kernel currently does not provide a fully wired NT syscall ABI for the ALPC functions, scheduler-blocking listen/receive, the full Windows ALPC attribute/security/section-view semantics, a complete CSRSS server-DLL initialization protocol, or an asynchronous session-manager launch path. The existing `usermode::run_loaded_image` is a synchronous smoke-test runner, not a general process-creation service. Do not treat the current core as a bootable Windows-compatible CSRSS yet.

Windows CSRSS is a user-mode subsystem process, with server functionality supplied by server DLLs and IPC through LPC/ALPC message ports. See [Microsoft's CSRSS debugging overview](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/debugging-csrss), [the CSRSRV export inventory](https://geoffchappell.com/studies/windows/win32/csrsrv/api/index.htm), and [the server-DLL loading architecture](https://geoffchappell.com/studies/windows/win32/csrsrv/api/srvloadr/index.htm).

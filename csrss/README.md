# CognacOS CSR server core

This directory defines the native CSR (Client/Server Runtime Subsystem) server core for the eventual **`csrss.exe`** user-mode image. It uses the Windows-style CSR API message envelope, bounded API dispatch, client process/thread bookkeeping, and explicit status returns for unknown API numbers.

## Current implementation boundary

- `CsrServerInitialize`: initializes dispatcher state.
- `CsrServerRegisterApi`: registers a handler under a complete CSR API number.
- `CsrServerDispatchMessage`: validates message sizes and routes only registered API numbers.
- `CsrServerRegisterClient` / `CsrServerUnregisterClient`: bounded client tracking with reference accounting.
- `CsrServerQueryState`: boot/debug status.
- Port name is exposed as a UTF-16 string using the Windows subsystem API-port naming convention.

## Required before this is a running `csrss.exe`

The kernel currently does not provide working ALPC create/listen/accept/send/receive services, process startup from a mapped user PE image, or a session-manager launch path. Therefore this crate is the real server dispatch/state core, **not yet a bootable server process**. The NTDLL CSR client must continue to fail closed when transport is absent; do not replace transport failure with fake success. Next integration steps are: implement the kernel ALPC port object and message queue, publish the CSR port in the object namespace, launch this image from the session manager, then register CSRSRV/BASESRV and console API tables.

Windows' CSRSS architecture uses a user-mode `csrss.exe` process with server DLLs and LPC/ALPC message dispatch; a matching export name alone is not sufficient for compatibility. See [Microsoft's CSRSS debugging overview](https://github.com/MicrosoftDocs/windows-driver-docs/blob/staging/windows-driver-docs-pr/debugger/debugging-csrss.md) and [CSR API message architecture notes](https://geoffchappell.com/studies/windows/win32/csrsrv/api/apireqst/api_msg.htm).

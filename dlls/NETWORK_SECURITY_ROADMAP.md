# CognacOS networking and security DLL implementation plan

**Target:** Windows 11 x64 user-mode ABI, implemented in Rust.

The shared crate at shared/net-security supplies protocol and parsing building blocks. It does not replace the OS networking provider, Windows security services, or DLL ABI contracts.

## Components and ownership

| Native module | CognacOS owner | Required contract |
|---|---|---|
| ws2_32.dll | Winsock provider | WSA startup/refcount, SOCKET handles, sockaddr layouts, errors, overlapped I/O and extension functions |
| bcrypt.dll | CNG primitive provider | Algorithm providers, hash/key handles, crypto operations, RNG and NTSTATUS behavior |
| ncrypt.dll | CNG key storage | Provider/key handles, persistence, ACLs, isolation and NCrypt status values |
| crypt32.dll | CryptoAPI | Certificate contexts/stores, chain engine, revocation, message and signature APIs |
| secur32.dll | SSPI public facade | Package enumeration, credential/context handles, buffer descriptors and dispatch |
| sspicli.dll | SSPI client transport | LSA communication and package marshalling |
| schannel.dll | Schannel SSP | TLS credentials, handshake tokens, stream sizes, encrypt/decrypt and shutdown |
| winhttp.dll | Service HTTP client | Session/connection/request handles, proxy, TLS policy, async callbacks and errors |
| wininet.dll | Interactive Internet client | URL/connection/request handles, cookies/cache, FTP/HTTP and errors |
| lsass.exe | Local security authority | Authentication packages, policy database, credential validation and privileged RPC |

## Dependency graph

Applications call ws2_32, winhttp, or wininet over the kernel TCP/UDP and I/O completion provider. SSPI calls secur32/sspicli, which communicate with LSA/LSASS. Schannel is an SSPI package and can use rustls/AWS-LC as its protocol/crypto engine where semantics match. bcrypt/ncrypt expose CNG provider and key-storage contracts. crypt32 owns certificate stores and chain policy.

## Compatibility rules

- Export names, ordinals, forwarders, and calling conventions must come from a pinned Windows 11 x64 reference image. Do not reuse kernel32/syssetup export lists.
- Public entry points use exact Windows-width types and structure packing. Rust String, Vec, trait objects, and Rust enums never cross the ABI boundary.
- Preserve SOCKET versus HANDLE, SECURITY_STATUS versus NTSTATUS, DWORD error domains, callback conventions, and ownership/lifetime rules.
- Never return success from an unimplemented export. Return the documented unsupported status or a safe error until behavior exists.
- Keep protocol engines behind provider traits so kernel sockets, certificate stores, TPM, and hardware-backed crypto can replace bootstrap implementations.

## Current status (2026-10-31)

| Area | Status |
|---|---|
| Shared Rust dependency foundation | Initial crate added in this change |
| TLS client configuration | TLS 1.2+ with AWS-LC provider and bootstrap roots |
| DER X.509 parsing helper | Initial strict parser |
| Kernel TCP/UDP stack | Not yet verified as available |
| Windows 11 DLL export baselines | Not yet imported for these modules |
| Listed Windows ABI DLLs | Not yet implemented |
| LSASS / LSA package integration | Not yet implemented |
| End-to-end Windows application networking | Not yet available |

This status is intentionally conservative: a dependency manifest or export stub is not a working Windows API.

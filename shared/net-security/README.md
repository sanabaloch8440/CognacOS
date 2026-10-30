# CognacOS networking and security foundation

This crate centralizes vetted Rust building blocks for future native Windows-compatible networking DLLs. It is not itself a DLL and does not claim Windows ABI compatibility.

| Crate | Intended subsystem |
|---|---|
| socket2 | Socket creation and low-level options |
| tokio | Async runtime and I/O scheduling |
| rustls + aws-lc-rs | TLS 1.2/1.3 and cryptographic provider |
| tokio-rustls | Async TLS streams |
| webpki-roots | Bootstrap public trust anchors |
| x509-parser | DER X.509 parsing |
| hyper | HTTP protocol engine |
| reqwest | High-level HTTP client building block |

## Security boundaries

- No TLS certificate-verification bypass is provided.
- TLS 1.0/1.1, SSL, and DTLS are not enabled by this foundation.
- 0-RTT is disabled until replay handling is designed.
- Mozilla roots do not replace Windows certificate stores.
- NCrypt requires persistent key storage and access control; raw private keys must not be exposed by default.
- SSPI handles, impersonation, and LSASS RPC are separate ABI/service work.
- reqwest and hyper do not replace WinHTTP/WinINet exports or the kernel TCP/IP stack.

## Required sequence

1. Establish the kernel TCP/UDP provider and I/O completion semantics.
2. Implement ws2_32.dll with a pinned Windows 11 x64 export inventory and Winsock error translation.
3. Implement bcrypt.dll and ncrypt.dll over a provider interface.
4. Implement crypt32.dll certificate stores, chain building, revocation, and Authenticode.
5. Implement secur32.dll/sspicli.dll and SSPI package ABI.
6. Implement schannel.dll as an SSPI package backed by rustls where policy semantics match.
7. Implement WinHTTP and WinINet over native sockets and SSPI.
8. Integrate LSASS authentication and policy services.

Each DLL needs a verified Windows 11 x64 export manifest, .def file, ABI tests, and implementation-status report. Export stubs are not functional implementations.

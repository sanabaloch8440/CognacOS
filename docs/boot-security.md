---
layout: default
title: Boot security
---

# Boot security and code integrity

CognacOS now verifies the exact kernel PE32+ file before loading it. TPM 2.0 measured boot and firmware Secure Boot are optional platform features: neither TPM presence nor Secure Boot being enabled is required to boot. A missing or failed kernel signature, however, is a hard boot failure.

## Boot-time policy

1. The UEFI loader reads the kernel image into memory and checks its SHA-256 digest against a build-time trust anchor.
2. The loader verifies an Ed25519 signature over the exact image bytes. Missing keys/signatures or a verification failure stop the boot with `EFI_SECURITY_VIOLATION`.
3. Before `ExitBootServices`, the loader queries the UEFI `SecureBoot` and `SetupMode` variables. Unknown state is reported as unknown, not assumed disabled.
4. If firmware exposes the UEFI TCG2 protocol and reports a TPM present, the loader asks firmware to hash and extend the verified kernel image into PCR 4. TPM absence or measurement failure is logged and does not block boot.
5. A versioned BootInfo security record passes the signature-verification result, Secure Boot state, TPM measurement result, and kernel SHA-256 digest to the kernel. The kernel refuses a handoff that does not attest that signature verification succeeded.

The loader uses the platform's active TCG2 hash banks and event log. It does not initialize a TPM directly, alter PCR-bank configuration, or write firmware variables.

## CI and release signing

The GitHub Actions workflow tests the signature path on every build:

- If the `COGNACOS_SIGNING_PRIVATE_KEY` repository secret contains a PEM-encoded Ed25519 private key, the workflow signs the kernel with it and embeds the matching public key in the loader.
- If that secret is absent, CI generates a temporary Ed25519 key solely to test signing and tamper rejection. Such artifacts do **not** have a stable release identity and must not be treated as production-signed releases.
- To have firmware Secure Boot accept the UEFI loader, configure both `COGNACOS_SECURE_BOOT_CERT` (PEM X.509 certificate) and `COGNACOS_SECURE_BOOT_KEY` (matching PEM private key). The workflow signs and verifies the loader with `sbsign`/ `sbverify`. The certificate must chain to a key enrolled in the target firmware's Secure Boot database.
- If the Secure Boot certificate/key are absent, the workflow emits an unsigned development loader. Firmware with Secure Boot enabled will normally reject that loader; this does not make Secure Boot a prerequisite for development boots with Secure Boot disabled.

Protect signing secrets with restricted access and rotate them through a deliberate trust-key migration. Do not commit private keys, put them in artifacts, or use ephemeral CI keys for release signing.

## Current limits

- The kernel image uses a CognacOS-specific Ed25519 detached-signature trust anchor embedded into the UEFI loader, plus a SHA-256 check. This is **not Windows Authenticode** and is not a substitute for a complete Windows Code Integrity policy.
- Firmware Secure Boot authenticates the UEFI loader only when the loader is signed by a certificate trusted by the machine's firmware. With Secure Boot disabled, replacing both the loader and its embedded trust anchor defeats the boot trust chain.
- TPM support currently measures the kernel image only. It does not yet implement remote attestation, quote generation, PCR policy enforcement, measured-driver loading, or TPM-backed key sealing.
- A successful QEMU build/boot test does not prove behavior across physical firmware, TPM implementations, Secure Boot databases, or revocation lists. Test on enrolled-key firmware and TPM 2.0 hardware before describing a release as hardware-validated.

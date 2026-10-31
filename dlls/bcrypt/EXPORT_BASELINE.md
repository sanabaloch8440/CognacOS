# bcrypt.dll Windows 11 x64 export baseline

Source: [learn-more/winexports](https://github.com/learn-more/winexports/tree/master/data/exports/nt100_x64_26200), binary metadata version **10.0.26100.7623**, AMD64. The inventory records the exact 60 named exports and ordinals from that reference binary. It is pinned to this servicing build and should be refreshed only against a newly inspected reference image.

CognacOS currently implements 9 entry points: BCryptOpenAlgorithmProvider, BCryptCloseAlgorithmProvider, BCryptGetProperty, BCryptCreateHash, BCryptHashData, BCryptFinishHash, BCryptDestroyHash, BCryptGenRandom, and BCryptDeriveKeyPBKDF2. Supported digest providers are SHA-256, SHA-384, and SHA-512. The remaining 51 exports are present in the PE export table but fail closed with STATUS_NOT_IMPLEMENTED; they are not behaviorally compatible yet.

Known limitations in this first batch: AES and other cipher providers, symmetric/asymmetric key lifecycle, signing/verification, multi-hash, algorithm enumeration/configuration, FIPS policy, provider registration, secure key storage, and full Windows CNG object-buffer semantics are not implemented. Handles are process-local heap objects; robust invalid/stale handle validation and cross-process provider semantics remain future work. Do not treat this as a complete bcrypt.dll replacement.

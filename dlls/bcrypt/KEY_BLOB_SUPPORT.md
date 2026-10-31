# bcrypt symmetric key blob support

CognacOS implements the version-1 `BCRYPT_KEY_DATA_BLOB` path for AES symmetric keys.

Supported operations:
- `BCryptDuplicateKey`: duplicates the opaque key handle and copies its key bytes.
- `BCryptExportKey`: exports the 12-byte version-1 key-data header followed by raw key bytes.
- `BCryptImportKey`: imports that format when the destination AES provider has already selected CBC or GCM.
- Key lengths: 128, 192, and 256 bits.
- Import validates the magic, version, declared length, exact input length, key size, and provider type.
- Key bytes are held in the existing zeroizing key-handle type.

Deliberate limitations:
- Only `BCRYPT_KEY_DATA_BLOB` is accepted. Wrapped, opaque, RSA, ECC, and DH blobs are unsupported.
- Import/export wrapping-key handles are rejected.
- Caller-provided key-object storage is not supported; the key is stored in the DLL's opaque handle table.
- This format exports raw key material. Callers must protect the destination buffer and must not use it as a secure backup format.
- This does not make bcrypt.dll generally Windows-compatible; unsupported exports continue to return `STATUS_NOT_IMPLEMENTED`.

Tests cover round-tripping AES-128/192/256 keys, preserving the provider's chaining mode, duplicate handles, and malformed headers/lengths.


## P-256 ECDH public-key blobs

The ECDH provider accepts and exports the Windows `EccPublicBlob` format for
P-256 keys. The blob is validated for the `BCRYPT_ECDH_PUBLIC_P256_MAGIC`
value, 32-byte coordinate size, exact 72-byte length, and a valid uncompressed
SEC1 point. `BCryptSecretAgreement` requires a finalized local private key and
an imported peer public key; the resulting shared secret is held in a
zeroizing CNG secret handle and can be consumed by the existing `TRUNCATE`
raw-secret KDF. P-384/P-521 and private ECC blob import/export are not yet
implemented.

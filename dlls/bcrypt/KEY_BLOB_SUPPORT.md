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

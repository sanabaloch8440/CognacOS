# Windows 11 x64 RTL ABI audit

Target export catalogue: NTOSKRNL x64, Windows 11 build 10.0.26100.8036.

## Calling convention and data layout

All **102 newly registered RTL export entry points** in this pass are Rust `unsafe extern "win64"` functions. On x86-64 this selects the Microsoft Windows x64 ABI: integer/pointer arguments 1–4 are passed in RCX, RDX, R8 and R9; later arguments use the stack; callers reserve the 32-byte shadow space; the stack is 16-byte aligned at call boundaries; scalar/pointer results return in RAX. This matches Microsoft's documented x64 convention: https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention?view=msvc-170.

The new C-facing structures use `#[repr(C)]` for ACL headers, ACE headers, absolute security descriptors, UTF-8 counted strings, bitmap runs, SID-and-attributes records, generic mappings, and version-information records. Fixed-width Windows types are represented with 8/16/32/64-bit Rust integers or pointer-sized `usize` where the Windows prototype uses `SIZE_T`/pointer-sized values.

## Newly registered export names

### ACL and security descriptors
`RtlCreateAcl`, `RtlValidAcl`, `RtlGetAce`, `RtlFirstFreeAce`, `RtlDeleteAce`, `RtlAddAccessAllowedAce`, `RtlAddAccessAllowedAceEx`, `RtlAddAccessDeniedAceEx`, `RtlCreateSecurityDescriptor`, `RtlValidSecurityDescriptor`, `RtlLengthSecurityDescriptor`, `RtlGetControlSecurityDescriptor`, `RtlGetDaclSecurityDescriptor`, `RtlGetSaclSecurityDescriptor`, `RtlGetOwnerSecurityDescriptor`, `RtlGetGroupSecurityDescriptor`, `RtlSetDaclSecurityDescriptor`, `RtlSetSaclSecurityDescriptor`, `RtlSetOwnerSecurityDescriptor`, `RtlSetGroupSecurityDescriptor`, `RtlOwnerAcesPresent`, `RtlGetAcesBufferSize`, `RtlSetControlSecurityDescriptor`, `RtlFindAceByType`, `RtlAddAccessAllowedObjectAce`, `RtlAddAccessDeniedObjectAce`, `RtlAddAuditAccessObjectAce`, `RtlAddAuditAccessAceEx`, `RtlAddMandatoryAce`, `RtlAddProcessTrustLabelAce`, `RtlValidRelativeSecurityDescriptor`, `RtlAbsoluteToSelfRelativeSD`, `RtlAddAce`, `RtlCopySidAndAttributesArray`

### Strings and code-page conversion
`RtlInitStringEx`, `RtlInitUTF8String`, `RtlInitUTF8StringEx`, `RtlUTF8ToUnicodeN`, `RtlUnicodeToUTF8N`, `RtlUTF8StringToUnicodeString`, `RtlUnicodeStringToUTF8String`, `RtlOemToUnicodeN`, `RtlUnicodeToOemN`, `RtlUpcaseUnicodeToOemN`, `RtlUpcaseUnicodeToMultiByteN`, `RtlUpcaseUnicodeStringToOemString`, `RtlUpcaseUnicodeStringToCountedOemString`, `RtlUnicodeStringToCountedOemString`, `RtlOemStringToCountedUnicodeString`, `RtlOemStringToUnicodeString`, `RtlUnicodeStringToOemString`, `RtlUpcaseUnicodeToCustomCPN`, `RtlUnicodeToCustomCPN`, `RtlIpv4StringToAddressA`, `RtlIpv4StringToAddressW`, `RtlIpv4StringToAddressExA`, `RtlIpv4StringToAddressExW`, `RtlIpv4AddressToStringExA`, `RtlIpv4AddressToStringExW`, `RtlEthernetStringToAddressA`, `RtlEthernetStringToAddressW`, `RtlConvertSidToUnicodeString`

### IPv4 and Ethernet
`RtlIpv4StringToAddressA`, `RtlIpv4StringToAddressW`, `RtlIpv4StringToAddressExA`, `RtlIpv4StringToAddressExW`, `RtlIpv4AddressToStringExA`, `RtlIpv4AddressToStringExW`, `RtlEthernetStringToAddressA`, `RtlEthernetStringToAddressW`

### Run-once synchronization
`RtlRunOnceInitialize`, `RtlRunOnceBeginInitialize`, `RtlRunOnceComplete`, `RtlRunOnceExecuteOnce`

### Bitmap operations
`RtlInterlockedSetBitRun`, `RtlInterlockedClearBitRun`, `RtlInterlockedSetBitRunEx`, `RtlInterlockedClearBitRunEx`, `RtlFindClearRuns`

### SID and LUID helpers
`RtlCopyLuidAndAttributesArray`, `RtlConvertSidToUnicodeString`, `RtlEqualWnfChangeStamps`, `RtlCopySidAndAttributesArray`

### Image, version, and compatibility helpers
`RtlMapGenericMask`, `RtlMapSecurityErrorToNtStatus`, `RtlIsApiSetImplemented`, `RtlIsFunctionalityAvailable`, `RtlIsNameLegalDOS8Dot3`, `RtlGetVersion`, `RtlGetDefaultCodePage`, `RtlIsNonEmptyDirectoryReparsePointAllowed`, `RtlIsPartialPlaceholder`, `RtlIsPartialPlaceholderFileHandle`, `RtlImageNtHeader`, `RtlImageNtHeaderEx`, `RtlImageDirectoryEntryToData`, `RtlVerifyVersionInfo`, `RtlIsGenericTableEmpty`, `RtlIsGenericTableEmptyAvl`, `RtlNumberGenericTableElements`, `RtlNumberGenericTableElementsAvl`, `RtlSetPortableOperatingSystem`, `RtlSetConsoleSessionForegroundProcessId`, `RtlGetConsoleSessionForegroundProcessId`, `RtlSetProcessPlaceholderCompatibilityMode`, `RtlQueryProcessPlaceholderCompatibilityMode`, `RtlSetThreadPlaceholderCompatibilityMode`, `RtlQueryThreadPlaceholderCompatibilityMode`, `RtlGetNtSystemRoot`

## ABI corrections and validation

- `RtlRunOnceExecuteOnce` callback return type is a one-byte `LOGICAL/BOOLEAN`; the outer routine returns `NTSTATUS`.
- `RtlNumberOfSetBitsUlongPtr` returns `ULONG` (32-bit), while its input remains pointer-sized.
- `RtlGetAcesBufferSize` now follows the two-parameter `NTSTATUS` contract and returns the total ACE-byte count through `PULONG`.
- `RtlIsPartialPlaceholder` uses its two `ULONG` inputs; `RtlIsPartialPlaceholderFileHandle` and `RtlIsPartialPlaceholderFileInfo` use their documented `NTSTATUS`/output-BOOLEAN signatures.
- `RtlGetVersion` respects the base `OSVERSIONINFOW` size and writes the `OSVERSIONINFOEXW` tail only when the caller supplied the larger structure.
- `RtlFindClearRuns` uses a C-layout pair of 32-bit run fields and does not allocate temporary kernel heap memory.
- Bitmap Ex operations use pointer-sized indexes and counts; ordinary `RTL_BITMAP` range-count operations use 32-bit `ULONG` indexes/counts.
- Resolver entries were added only for names present in the checked NTOSKRNL export catalogue. Existing bitmap implementations were also registered where their exported signatures match.

## Compatibility limits still requiring differential tests

This is a source-level ABI review, **not proof of full Windows behavioral equivalence**. The following gaps are intentionally recorded rather than hidden:

- ANSI/OEM conversion routines currently use a single-byte approximation; they do not yet implement Windows' active code-page tables. Allocation-requesting string conversions return an error because RdotOS does not yet supply the matching RTL pool/string allocator semantics.
- The run-once implementation uses an atomic one-word state but does not yet reproduce Windows' complete asynchronous/waiter behavior under contention.
- `RtlVerifyVersionInfo` compares against the RdotOS-reported Windows 11 compatibility version (10.0, build 26100); it needs tests for every condition-mask combination.
- PE image helpers that lack an input-size parameter necessarily trust that the supplied image base is a readable mapped image. `RtlImageNtHeaderEx` is the bounded alternative.
- The native `RtlSelfRelativeToAbsoluteSD` prototype has no source-buffer length argument; the implementation validates structure fields but cannot independently prove the source allocation bounds.
- Generic-table element counts assume the documented Windows x64 `RTL_GENERIC_TABLE` and `RTL_AVL_TABLE` layouts; differential tests against Windows headers are still required.
- Placeholder capability checks return false because Cloud Files placeholder semantics are not implemented in RdotOS.
- `RtlFindNextForwardRunClearCapped` and `RtlCopyBitMapEx` remain deliberately unmapped until their local prototypes are corrected against the Windows 11 signatures.

The next verification gate is the kernel build and boot smoke test in GitHub Actions. Passing compilation verifies Rust type/layout integration, not the semantic items above.

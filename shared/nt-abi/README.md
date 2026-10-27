# CognacOS NT ABI crate

`cognacos-nt-abi` is the canonical source of truth for ABI-level types and declarations shared by CognacOS components. Rust modules replace textual headers; consumers import the layer they need:

- `windows`: common scalar types and status values (Windows.h-like subset)
- `ntddk`: executive/kernel types and declarations
- `wdm`: device/driver/IRP contracts
- `ntifs`: filesystem-facing subset
- `object`, `io`, `mm`, `security`, `ntstatus`: canonical definitions

## Migration status

The kernel and NTDLL both depend on this crate. NTDLL exposes it as `crate::nt_abi`; the kernel has an `nt_abi` compatibility path for existing code. The shared `NTSTATUS`, scalar aliases (including `UCHAR`), `ANSI_STRING`, `UNICODE_STRING`, `OBJECT_ATTRIBUTES`, `RTL_SPLAY_LINKS`, `LIST_ENTRY`, and `IO_STATUS_BLOCK` are the source of truth for new code and migrated call sites. NTDLL's RTL string implementation and native bootstrap now import shared scalar types and NTSTATUS constants rather than redeclaring them locally; the source-first NTDLL CRT string routines also use shared `SIZE_T` and `ULONG`. NTDLL's RTL handle-table implementation now imports the canonical scalar, status, virtual-memory flag, and page-size definitions too. Kernel RTL's `AnsiString`/`UnicodeString` and NTDLL RTL's `COGNAC_ANSI_STRING`/`COGNAC_UNICODE_STRING` names now alias the shared string layouts instead of defining independent structs. The WDK-style `windows`, `ntddk`, `wdm`, and `ntifs` facades re-export `PUNICODE_STRING` and `POBJECT_ATTRIBUTES` as pointer aliases to those canonical layouts. The active kernel WDM runtime now uses the shared `IO_STATUS_BLOCK` union layout, including its status/pointer overlay and information field, rather than maintaining a second local structure. `NTSTATUS_TYPE` is a compatibility alias of the canonical signed `NTSTATUS`; the NTDLL `rtl` module re-exports the shared `RTL_SPLAY_LINKS`/`PRTL_SPLAY_LINKS` definitions so existing `crate::rtl::*` paths remain valid. NTDLL generic-table routines now also use the shared `LIST_ENTRY` instead of a second local two-pointer structure.

This dependency alone does not mean every local type has been migrated. The kernel's active WDM `DRIVER_OBJECT`, `DEVICE_OBJECT`, and opaque runtime-owned `IRP` layouts still differ from the initial shared scaffolding and remain local until their complete layouts and semantics can be reconciled. Do not replace those types with a shared struct until call sites have been audited and the x64 size, alignment, field offsets, and semantics are proven equivalent. Preserve existing module paths with `pub use` re-exports or thin compatibility adapters while migration is in progress.

## Integration policy

1. Add this crate as a path dependency to every kernel, NTDLL, HAL, and driver crate that consumes NT ABI declarations.
2. Replace duplicate local definitions with imports/re-exports in small, reviewable groups. Prefer shared definitions directly for new code; do not bulk-replace names based on spelling alone.
3. Keep implementation and state private to the owning subsystem. This crate contains ABI declarations, constants, and layouts only.
4. Verify Windows x64 sizes, alignments, offsets, calling conventions, signed status semantics, and target-build differences with compile-time assertions and fixtures before claiming WDK compatibility.
5. Keep compatibility adapters only where required by established call sites; adapters must not introduce a second conflicting wire layout or change exported symbol names.
6. Treat the initial IRP/DEVICE_OBJECT/DRIVER_OBJECT layouts as migration scaffolding, not a complete WDK-accurate layout. Extend only against a pinned Windows SDK/WDK reference and add layout tests.
7. Run the kernel and NTDLL build/test workflows after each migration batch. Never describe a declared function or type as implemented merely because its ABI declaration exists.

The crate is `no_std` and uses fixed-width NT scalar types. Layout tests now check pointer-width-specific sizes, alignments, and field offsets for `ANSI_STRING`, `UNICODE_STRING`, `OBJECT_ATTRIBUTES`, `IO_STATUS_BLOCK`, and the two-pointer LIST_ENTRY and three-pointer RTL splay-link layouts. The driver/IRP structures remain scaffolding and must not yet be treated as third-party WDK binary compatibility.

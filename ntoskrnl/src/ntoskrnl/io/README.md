# NT I/O manager ownership

`wdm.rs` is the canonical device-object and IRP implementation for the NT I/O
manager. The former standalone `runtime.rs` device/IRP registries have been
retired so new code does not create a second I/O ownership model.

The migration is intentionally staged. Callers that still need the legacy
high-level request API must be moved onto WDM device objects, IRPs, stack
locations, dispatch routines, and completion before that API is reintroduced
as a thin adapter. Do not recreate independent device or IRP tables here.

The boot-file service remains a separate compatibility layer for now. Its
FILE_OBJECT lifecycle and IRP-backed create/read/cleanup/close paths are
follow-up WDM integration work; this change does not claim those hooks exist.

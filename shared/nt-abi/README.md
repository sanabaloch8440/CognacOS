# CognacOS NT ABI crate

`cognacos-nt-abi` is the intended single source of truth for ABI-level types and declarations shared by CognacOS components. Rust modules replace textual headers; consumers import the layer they need:

- `windows`: common scalar types and status values (Windows.h-like subset)
- `ntddk`: executive/kernel types and declarations
- `wdm`: device/driver/IRP contracts
- `ntifs`: filesystem-facing subset
- `object`, `io`, `mm`, `security`, `ntstatus`: canonical definitions

## Integration policy

1. Add this crate as a path dependency to the kernel, NTDLL, HAL, and driver crates as each component is migrated.
2. Replace duplicate local definitions with imports/re-exports; do not bulk-replace names until layout and call-site semantics are checked.
3. Keep implementation and state private to the owning subsystem. This crate contains ABI declarations, constants, and layouts only.
4. Verify Windows x64 sizes, offsets, calling conventions, and target-build differences with compile-time assertions and fixtures before claiming WDK compatibility.
5. Treat the initial IRP/DEVICE_OBJECT/DRIVER_OBJECT layouts as migration scaffolding, not a complete WDK-accurate layout. Extend only against a pinned Windows SDK/WDK reference and add layout tests.

The crate is `no_std` and uses fixed-width NT scalar types. It is deliberately not wired into every crate yet: each existing definition must be reconciled before replacing it, to avoid duplicate symbols, ABI drift, or breaking the current kernel/NTDLL build.

# WDM I/O manager source layout

`../wdm.rs` is the module entry point. The implementation is divided into functionality-oriented source fragments in this directory and included in the same Rust module namespace. This preserves existing private ABI types, locks, and helper visibility while making each subsystem independently navigable.

| File | Responsibility |
|---|---|
| `core.rs` | ABI types, constants, global state and IRP tracking primitives |
| `device_lifecycle.rs` | Device references, remove rundown, power and PnP state transitions |
| `pnp.rs` | Resource arbitration, device relations, bus enumeration and PnP manager |
| `device_objects.rs` | Device queues, device creation/deletion and driver unload |
| `irp_management.rs` | IRP allocation, dispatch, IOCTL builders and completion |
| `device_stack.rs` | Device attachment, stack-location helpers and IRP reuse |
| `filesystem.rs` | Filesystem registration, VPB mount/verify/dismount |
| `namespace.rs` | Object-manager symbolic-link namespace |
| `cancel.rs` | Cancel-safe queues and IRP cancellation |
| `mdl.rs` | MDL allocation and release |
| `self_test.rs` | Runtime WDM rundown self-test |
| `tests.rs` | WDM ABI and behavior unit tests |
| `requestor_session.rs` | Requestor session ID query export |

These are `include!` fragments, not independent Rust modules. Moving to true submodules would require a separate visibility/API refactor and is deliberately not mixed into this source-organization change. WDM behavior is otherwise unchanged.

The file-service layer is not modified here. Its migration to FILE_OBJECT and IRP-based Windows-style create/read/cleanup/close flows is the next step.
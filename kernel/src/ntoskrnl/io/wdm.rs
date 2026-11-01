//! Windows x64 WDM I/O manager implementation.
//! Functionality-oriented fragments are included in this parent module so existing
//! private ABI/state helpers retain their visibility and one coherent namespace.

include!("wdm/core.rs");
include!("wdm/device_lifecycle.rs");
include!("wdm/pnp.rs");
include!("wdm/device_objects.rs");
include!("wdm/irp_management.rs");
include!("wdm/device_stack.rs");
include!("wdm/filesystem.rs");
include!("wdm/namespace.rs");
include!("wdm/cancel.rs");
include!("wdm/mdl.rs");
include!("wdm/self_test.rs");
include!("wdm/tests.rs");
include!("wdm/requestor_session.rs");

//! WDM I/O manager module facade.
//!
//! Keep implementation in the functionality-oriented source fragments under
//! `wdm/`. They are included into this module namespace so shared private
//! ABI state and helper visibility remain unchanged.

include!("wdm/core.rs");
include!("wdm/configuration.rs");
include!("wdm/interrupts.rs");
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
include!("wdm/driver_services.rs");
include!("wdm/interrupts.rs");
include!("wdm/io_exports.rs");

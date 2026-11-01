// Host-test module tree that imports the real kernel CM files as Rust modules.
// Using #[path] preserves their module-level docs, inner attributes, and super::
// relationships; include! would textually splice those inner items into this file.
#[path = "../kernel/src/ntoskrnl/cm/hive.rs"]
pub mod hive;
#[path = "../kernel/src/ntoskrnl/cm/driver_config.rs"]
pub mod driver_config;
#[path = "../kernel/src/ntoskrnl/cm/service_config.rs"]
pub mod service_config;

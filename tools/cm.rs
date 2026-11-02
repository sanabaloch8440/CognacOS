// Host-test module tree that imports the real kernel CM files as Rust modules.
// Using #[path] preserves their module-level docs, inner attributes, and super::
// relationships; include! would textually splice those inner items into this file.
#[path = "../ntoskrnl/src/cm/hive.rs"]
pub mod hive;
#[path = "../ntoskrnl/src/cm/driver_config.rs"]
pub mod driver_config;
#[path = "../ntoskrnl/src/cm/service_config.rs"]
pub mod service_config;

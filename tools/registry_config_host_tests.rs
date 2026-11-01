#![allow(dead_code)]

// Compile the actual freestanding Configuration Manager modules in a host test
// crate so their parsing and validation tests run without booting the kernel.
mod cm {
    pub mod hive {
        include!("../kernel/src/ntoskrnl/cm/hive.rs");
    }
    pub mod driver_config {
        include!("../kernel/src/ntoskrnl/cm/driver_config.rs");
    }
    pub mod service_config {
        include!("../kernel/src/ntoskrnl/cm/service_config.rs");
    }
}

#![allow(dead_code)]

// Compile the actual freestanding Configuration Manager modules in a host test
// crate so their parsing and validation tests run without booting the kernel.
mod ntoskrnl {
    pub mod cm {
        #[path = "../kernel/src/ntoskrnl/cm/hive.rs"]
        pub mod hive;
        #[path = "../kernel/src/ntoskrnl/cm/driver_config.rs"]
        pub mod driver_config;
        #[path = "../kernel/src/ntoskrnl/cm/service_config.rs"]
        pub mod service_config;
    }
}

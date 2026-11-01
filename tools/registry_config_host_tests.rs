#![allow(dead_code)]

// Compile the actual freestanding Configuration Manager modules in a host test
// crate so their parsing and validation tests run without booting the kernel.
// Use a path-backed module tree so the kernel files retain their module context.
#[path = "cm.rs"]
mod cm;

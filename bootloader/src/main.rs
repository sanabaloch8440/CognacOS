#![no_main]
#![no_std]

use core::fmt::Write;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    // Initialize the UEFI environment and its console helpers. This is a Rust
    // EFI application; it does not yet transfer control to the kernel.
    if let Err(error) = uefi::helpers::init() {
        return error.status();
    }

    uefi::println!("RdotOS Rust UEFI bootloader");
    uefi::println!("Architecture: native x86-64");
    uefi::println!("Firmware services initialized.");
    uefi::println!("Secure Boot policy enforcement and kernel handoff are pending.");
    let _ = uefi::system::with_stdout(|out| writeln!(out, "Boot chain is under development."));
    Status::SUCCESS
}

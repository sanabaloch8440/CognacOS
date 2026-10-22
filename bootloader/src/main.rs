#![no_main]
#![no_std]

use uefi::prelude::*;

#[entry]
fn main() -> Status {
    // Native Rust EFI application. Verified loading and kernel handoff are pending.
    if let Err(status) = uefi::helpers::init() {
        return status;
    }
    uefi::println!("RdotOS Rust UEFI bootloader");
    uefi::println!("Architecture: native x86-64");
    uefi::println!("UEFI services initialized.");
    uefi::println!("Secure Boot enforcement and kernel handoff are not implemented yet.");
    Status::SUCCESS
}

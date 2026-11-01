/// Windows WDM version gate. RdotOS currently implements the WDM 1.0
/// baseline only; newer requested minor versions are rejected rather than
/// claiming capabilities that are not present.
#[unsafe(no_mangle)]
pub extern "win64" fn io_is_wdm_version_available(major_version: u16, minor_version: u16) -> u8 {
    u8::from(major_version == 0 || (major_version == 1 && minor_version == 0))
}

/// Public NT configuration counters returned by IoGetConfigurationInformation.
/// The device enumerator is not yet wired to these counters, so they remain zero
/// until the relevant bus/device classes are registered.
#[repr(C)]
pub struct ConfigurationInformation {
    pub disk_count: u32,
    pub floppy_count: u32,
    pub cd_rom_count: u32,
    pub tape_count: u32,
    pub scsi_port_count: u32,
    pub serial_count: u32,
    pub parallel_count: u32,
}
static mut CONFIGURATION_INFORMATION: ConfigurationInformation = ConfigurationInformation {
    disk_count: 0, floppy_count: 0, cd_rom_count: 0, tape_count: 0,
    scsi_port_count: 0, serial_count: 0, parallel_count: 0,
};

#[unsafe(no_mangle)]
pub extern "win64" fn io_get_configuration_information() -> *mut ConfigurationInformation {
    core::ptr::addr_of_mut!(CONFIGURATION_INFORMATION)
}


static IO_CANCEL_SPIN_LOCK: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Acquire the global NT I/O cancel spin lock and raise to DISPATCH_LEVEL.
/// The caller must supply a writable KIRQL slot, as required by the WDK ABI.
#[unsafe(export_name = "IoAcquireCancelSpinLock")]
pub extern "win64" fn io_acquire_cancel_spin_lock(old_irql: *mut u8) {
    if old_irql.is_null() {
        return;
    }
    let previous = unsafe { crate::ntoskrnl::ke::runtime::ke_raise_irql_to_dpc_level() };
    while IO_CANCEL_SPIN_LOCK
        .compare_exchange_weak(
            false,
            true,
            core::sync::atomic::Ordering::Acquire,
            core::sync::atomic::Ordering::Relaxed,
        )
        .is_err()
    {
        core::hint::spin_loop();
    }
    unsafe { old_irql.write(previous); }
}

/// Release the global cancel spin lock, then restore the caller's previous IRQL.
#[unsafe(export_name = "IoReleaseCancelSpinLock")]
pub extern "win64" fn io_release_cancel_spin_lock(old_irql: u8) {
    IO_CANCEL_SPIN_LOCK.store(false, core::sync::atomic::Ordering::Release);
    unsafe { crate::ntoskrnl::ke::runtime::ke_lower_irql(old_irql); }
}


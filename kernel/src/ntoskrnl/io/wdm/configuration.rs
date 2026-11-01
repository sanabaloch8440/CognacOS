/// WDM version gate: only the implemented 1.0 baseline is advertised.
#[unsafe(export_name="IoIsWdmVersionAvailable")]
pub extern "win64" fn io_is_wdm_version_available(major_version:u16,minor_version:u16)->u8{
    u8::from(major_version==0||(major_version==1&&minor_version==0))
}

#[repr(C)]
pub struct ConfigurationInformation{
    pub disk_count:u32,pub floppy_count:u32,pub cd_rom_count:u32,pub tape_count:u32,
    pub scsi_port_count:u32,pub serial_count:u32,pub parallel_count:u32,
}
static mut CONFIGURATION_INFORMATION:ConfigurationInformation=ConfigurationInformation{
    disk_count:0,floppy_count:0,cd_rom_count:0,tape_count:0,
    scsi_port_count:0,serial_count:0,parallel_count:0,
};
#[unsafe(export_name="IoGetConfigurationInformation")]
pub extern "win64" fn io_get_configuration_information()->*mut ConfigurationInformation{
    core::ptr::addr_of_mut!(CONFIGURATION_INFORMATION)
}

//! Kernel-side storage-class boundary.
//!
//! Disk transports are owned by loadable port/miniport drivers. This adapter
//! intentionally contains no ATA register access; it becomes available when
//! ataport publishes a partition device through the I/O Manager.
use cognacos_fat32::DiskError;

pub struct IoManagerBlockDevice {
    capacity: u64,
}
impl IoManagerBlockDevice {
    fn unavailable() -> Self { Self { capacity: 0 } }
    pub fn sector_count(&self) -> u64 { self.capacity / 512 }
    pub fn capacity_bytes(&self) -> u64 { self.capacity }
}
pub fn debug_trace_store_read(_offset: u64, _output: &mut [u8]) -> bool { false }
pub fn debug_trace_store_write(_offset: u64, _input: &[u8]) -> bool { false }
impl cognacos_fat32::BlockDevice for IoManagerBlockDevice {
    fn sector_size(&self) -> u32 { 512 }
    fn sector_count(&self) -> u64 { self.capacity / 512 }
    fn read_sectors(&mut self, _lba: u64, _output: &mut [u8]) -> Result<(), DiskError> {
        Err(DiskError::DeviceFailure)
    }
    fn write_sectors(&mut self, _lba: u64, _input: &[u8]) -> Result<(), DiskError> {
        Err(DiskError::DeviceFailure)
    }
    fn flush(&mut self) -> Result<(), DiskError> { Err(DiskError::DeviceFailure) }
}
pub fn fat32_block_device() -> Result<IoManagerBlockDevice, DiskError> {
    let device = IoManagerBlockDevice::unavailable();
    Err(if device.capacity == 0 { DiskError::DeviceFailure } else { DiskError::InvalidGeometry })
}

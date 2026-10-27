//! PCIe Enhanced Configuration Access Mechanism (ECAM) primitives.
//!
//! The caller must map the firmware-described ECAM aperture with uncached,
//! device-memory attributes before calling these routines. This module does not
//! assume a universal ECAM base or silently identity-map arbitrary physical RAM.
#![no_std]

/// Canonical Windows NT ABI declarations shared with ntoskrnl and NTDLL.
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)]
extern crate std;
use core::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PciError { InvalidWindow, InvalidBdf, InvalidOffset, AddressOverflow, UnsupportedRange }

/// Convert PCI discovery/configuration failures to the shared NTSTATUS ABI.
pub const fn error_to_ntstatus(error: PciError) -> nt_abi::ntstatus::NTSTATUS {
    use nt_abi::ntstatus::*;
    match error {
        PciError::UnsupportedRange => STATUS_INVALID_DEVICE_REQUEST,
        PciError::InvalidWindow | PciError::InvalidBdf | PciError::InvalidOffset
        | PciError::AddressOverflow => STATUS_INVALID_PARAMETER,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EcamWindow {
    /// Virtual address corresponding to bus_start/device 0/function 0/offset 0.
    pub virtual_base: u64,
    pub physical_base: u64,
    pub segment: u16,
    pub bus_start: u8,
    pub bus_end: u8,
}

impl EcamWindow {
    pub fn validate(&self) -> Result<(), PciError> {
        if self.virtual_base == 0 || self.physical_base == 0 || self.bus_end < self.bus_start
            || self.virtual_base & 0xfffff != 0 || self.physical_base & 0xfffff != 0 {
            return Err(PciError::InvalidWindow);
        }
        let span = ((self.bus_end as u64 - self.bus_start as u64) + 1)
            .checked_mul(1 << 20).ok_or(PciError::AddressOverflow)?;
        self.virtual_base.checked_add(span).ok_or(PciError::AddressOverflow)?;
        self.physical_base.checked_add(span).ok_or(PciError::AddressOverflow)?;
        Ok(())
    }

    /// Calculate the mapped virtual address for one ECAM register. ECAM uses
    /// 1 MiB per bus, 32 devices, 8 functions and 4096 bytes per function.
    pub fn register_address(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16) -> Result<*mut u8, PciError> {
        self.validate()?;
        if segment != self.segment || bus < self.bus_start || bus > self.bus_end || device >= 32 || function >= 8 {
            return Err(PciError::InvalidBdf);
        }
        if offset >= 4096 { return Err(PciError::InvalidOffset); }
        let bus_offset = (bus as u64 - self.bus_start as u64) << 20;
        let bdf_offset = ((device as u64) << 15) | ((function as u64) << 12) | offset as u64;
        let address = self.virtual_base.checked_add(bus_offset)
            .and_then(|v| v.checked_add(bdf_offset)).ok_or(PciError::AddressOverflow)?;
        Ok(address as *mut u8)
    }

    pub unsafe fn read_u8(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16) -> Result<u8, PciError> {
        let p = self.register_address(segment, bus, device, function, offset)?;
        Ok(unsafe { core::ptr::read_volatile(p) })
    }
    pub unsafe fn read_u16(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16) -> Result<u16, PciError> {
        if offset & 1 != 0 { return Err(PciError::InvalidOffset); }
        let p = self.register_address(segment, bus, device, function, offset)?;
        Ok(unsafe { core::ptr::read_volatile(p.cast::<u16>()) })
    }
    pub unsafe fn read_u32(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16) -> Result<u32, PciError> {
        if offset & 3 != 0 { return Err(PciError::InvalidOffset); }
        let p = self.register_address(segment, bus, device, function, offset)?;
        Ok(unsafe { core::ptr::read_volatile(p.cast::<u32>()) })
    }

    /// Write one naturally aligned dword. Callers must apply PCI register
    /// semantics (W1C status bits, BAR sizing, command bits) before writing.
    pub unsafe fn write_u32(&self, segment: u16, bus: u8, device: u8, function: u8, offset: u16, value: u32) -> Result<(), PciError> {
        if offset & 3 != 0 { return Err(PciError::InvalidOffset); }
        let p = self.register_address(segment, bus, device, function, offset)?;
        unsafe { core::ptr::write_volatile(p.cast::<u32>(), value); }
        core::sync::atomic::fence(Ordering::SeqCst);
        Ok(())
    }
}

/// A checked translation window used by bus/resource code. It only translates
/// addresses inside an explicitly supplied firmware/bridge window; identity
/// translation is never assumed for arbitrary PCI resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressWindow { pub bus_start: u64, pub physical_start: u64, pub length: u64 }
impl AddressWindow {
    pub fn translate(&self, bus_address: u64, length: u64) -> Result<u64, PciError> {
        if length == 0 || self.length == 0 { return Err(PciError::UnsupportedRange); }
        let end = bus_address.checked_add(length).ok_or(PciError::AddressOverflow)?;
        let window_end = self.bus_start.checked_add(self.length).ok_or(PciError::AddressOverflow)?;
        if bus_address < self.bus_start || end > window_end { return Err(PciError::UnsupportedRange); }
        self.physical_start.checked_add(bus_address - self.bus_start).ok_or(PciError::AddressOverflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ecam_bdf_offsets_are_correct() {
        let w = EcamWindow { virtual_base: 0x1000_0000, physical_base: 0x8000_0000, segment: 0, bus_start: 0, bus_end: 255 };
        assert_eq!(w.register_address(0, 0, 0, 0, 0x100).unwrap() as u64, 0x1000_0100);
        assert_eq!(w.register_address(0, 2, 3, 1, 0x20).unwrap() as u64, 0x1021_b020);
        assert!(w.register_address(0, 0, 32, 0, 0).is_err());
        assert!(w.register_address(0, 0, 0, 8, 0).is_err());
        assert!(w.register_address(0, 0, 0, 0, 4096).is_err());
    }
    #[test]
    fn resource_translation_checks_entire_range() {
        let w = AddressWindow { bus_start: 0x1000, physical_start: 0x8000_0000, length: 0x1000 };
        assert_eq!(w.translate(0x1100, 0x100), Ok(0x8000_0100));
        assert!(w.translate(0x1fff, 2).is_err());
        assert!(w.translate(0x0fff, 1).is_err());
    }
}

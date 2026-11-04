//! Validated ACPI _CRS to PCI root-aperture handoff.
//!
//! This is the sole conversion boundary from the versioned ACPI host-bridge
//! resource set to PCI allocator apertures. Every output aperture is clipped
//! against protected physical RAM before it can reach the allocator.
#![no_std]

use crate::e820::{self, E820Range, ClipError};
use crate::resource_manager::Aperture;
use crate::resources::BarKind;
use cognacos_hal_contract::{
    PciHostBridgeResourceSet, PCI_HOST_BRIDGE_RESOURCES_VERSION,
    PCI_HOST_BRIDGE_MAX_WINDOWS, PCI_HOST_BRIDGE_WINDOW_IO,
    PCI_HOST_BRIDGE_WINDOW_MEMORY,
};

pub const MAX_PCI_HOST_APERTURES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostBridgeHandoffError {
    InvalidVersion,
    InvalidIdentity,
    InvalidDescriptor,
    Overflow,
    Capacity,
    ProtectedMemory,
    UnsupportedTranslation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedHostBridgeApertures {
    pub segment: u16,
    pub bus_start: u8,
    pub bus_end: u8,
    pub apertures: [Aperture; MAX_PCI_HOST_APERTURES],
    pub count: usize,
}
impl PreparedHostBridgeApertures {
    pub fn as_slice(&self) -> &[Aperture] { &self.apertures[..self.count] }
    pub const EMPTY: Self = Self {
        segment: 0, bus_start: 0, bus_end: 0,
        apertures: [Aperture { start: 0, length: 0, kind: BarKind::Io, prefetchable: false }; MAX_PCI_HOST_APERTURES], count: 0,
    };
}

/// Convert the ACPI resource set and apply mandatory protected-RAM clipping.
/// No quirk or caller flag can disable the E820/firmware-memory exclusion.
pub fn prepare_host_bridge_apertures(
    resources: &PciHostBridgeResourceSet,
    protected_ram: &[E820Range],
) -> Result<PreparedHostBridgeApertures, HostBridgeHandoffError> {
    if resources.version != PCI_HOST_BRIDGE_RESOURCES_VERSION
        || resources.size as usize < core::mem::size_of::<PciHostBridgeResourceSet>()
        || resources.count as usize > PCI_HOST_BRIDGE_MAX_WINDOWS {
        return Err(HostBridgeHandoffError::InvalidVersion);
    }
    if resources.bus_start > resources.bus_end {
        return Err(HostBridgeHandoffError::InvalidIdentity);
    }

    let mut raw = [Aperture {
        start: 0, length: 0, kind: BarKind::Io, prefetchable: false,
    }; MAX_PCI_HOST_APERTURES];
    let mut raw_count = 0usize;

    for window in resources.windows[..resources.count as usize].iter() {
        if window.reserved != 0 || window.attributes & !0x03 != 0
            || window.length == 0 || window.minimum > window.maximum {
            return Err(HostBridgeHandoffError::InvalidDescriptor);
        }
        let exclusive_end = window.maximum.checked_add(window.length)
            .ok_or(HostBridgeHandoffError::Overflow)?;
        let span = exclusive_end.checked_sub(window.minimum)
            .ok_or(HostBridgeHandoffError::InvalidDescriptor)?;
        if span == 0 || window.minimum.checked_add(span).is_none() {
            return Err(HostBridgeHandoffError::Overflow);
        }
        if window.translation_offset != 0 { return Err(HostBridgeHandoffError::UnsupportedTranslation); }
        if window.alignment != 0 && !window.alignment.checked_add(1).is_some_and(u64::is_power_of_two) {
            return Err(HostBridgeHandoffError::InvalidDescriptor);
        }

        let kind = match window.space {
            PCI_HOST_BRIDGE_WINDOW_IO => BarKind::Io,
            PCI_HOST_BRIDGE_WINDOW_MEMORY => {
                if window.address_width > 2 { return Err(HostBridgeHandoffError::InvalidDescriptor); }
                if window.address_width == 0 && exclusive_end > (1u64 << 16)
                    || window.address_width == 1 && exclusive_end > (1u64 << 32) {
                    return Err(HostBridgeHandoffError::InvalidDescriptor);
                }
                if exclusive_end <= (1u64 << 32) { BarKind::Memory32 } else { BarKind::Memory64 }
            }
            _ => return Err(HostBridgeHandoffError::InvalidDescriptor),
        };

        if kind == BarKind::Io {
            if exclusive_end > 0x1_0000 { return Err(HostBridgeHandoffError::InvalidDescriptor); }
            if raw_count == raw.len() { return Err(HostBridgeHandoffError::Capacity); }
            raw[raw_count] = Aperture {
                start: window.minimum, length: span, kind, prefetchable: false,
            };
            raw_count += 1;
        } else {
            // ACPI cacheability is not PCI prefetchability. A root-host memory
            // aperture is therefore exposed as both forwarding classes; the
            // downstream Type-1 bridge windows still constrain each BAR.
            // The allocator's shared occupied-range checks prevent aliasing.
            for prefetchable in [false, true] {
                if raw_count == raw.len() { return Err(HostBridgeHandoffError::Capacity); }
                raw[raw_count] = Aperture {
                    start: window.minimum, length: span, kind, prefetchable,
                };
                raw_count += 1;
            }
        }
    }

    let mut clipped = [Aperture {
        start: 0, length: 0, kind: BarKind::Io, prefetchable: false,
    }; MAX_PCI_HOST_APERTURES];
    let clipped_count = e820::clip_host_apertures(
        &raw[..raw_count], protected_ram, &mut clipped,
    ).map_err(|error| match error {
        ClipError::InvalidRange | ClipError::Overflow => HostBridgeHandoffError::ProtectedMemory,
        ClipError::Capacity => HostBridgeHandoffError::Capacity,
    })?;

    let mut result = PreparedHostBridgeApertures::EMPTY;
    result.segment = resources.segment;
    result.bus_start = resources.bus_start;
    result.bus_end = resources.bus_end;
    for (index, aperture) in clipped[..clipped_count].iter().copied().enumerate() {
        result.apertures[index] = aperture;
    }
    result.count = clipped_count;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cognacos_hal_contract::{PciHostBridgeResourceSet, PciHostBridgeResourceWindow};

    fn set() -> PciHostBridgeResourceSet {
        let mut set = PciHostBridgeResourceSet::empty(0, 0, 255);
        set.windows[0] = PciHostBridgeResourceWindow {
            space: PCI_HOST_BRIDGE_WINDOW_MEMORY, attributes: 0,
            address_width: 1, reserved: 0,
            translation_offset: 0,
            minimum: 0x8000_0000, maximum: 0x8fff_ffff,
            alignment: 0x0fff, translation_offset: 0, length: 0x1000,
        };
        set.count = 1;
        set
    }

    #[test]
    fn acpi_crs_is_converted_and_ram_is_always_clipped() {
        let ram = [E820Range { start: 0x8800_0000, length: 0x1000_0000 }];
        let prepared = prepare_host_bridge_apertures(&set(), &ram).unwrap();
        assert_eq!(prepared.segment, 0);
        assert_eq!(prepared.bus_start, 0);
        assert_eq!(prepared.count, 2);
        let first = prepared.apertures[0];
        assert_eq!(first.start, 0x8000_0000);
        assert_eq!(first.length, 0x0800_0000);
        assert!(!first.prefetchable);
        assert!(prepared.apertures[1].prefetchable);
    }

    #[test]
    fn rejects_bad_descriptor_without_publishing_any_aperture() {
        let mut invalid = set();
        invalid.windows[0].reserved = 1;
        assert_eq!(
            prepare_host_bridge_apertures(&invalid, &[]),
            Err(HostBridgeHandoffError::InvalidDescriptor),
        );
    }

    #[test]
    fn rejects_overflowing_host_window() {
        let mut invalid = set();
        invalid.windows[0].minimum = u64::MAX - 4;
        invalid.windows[0].maximum = u64::MAX - 4;
        invalid.windows[0].length = 16;
        assert_eq!(
            prepare_host_bridge_apertures(&invalid, &[]),
            Err(HostBridgeHandoffError::Overflow),
        );
    }

    #[test]
    fn nonzero_acpi_translation_is_not_silently_discarded() {
        let mut translated = set();
        translated.windows[0].translation_offset = 0x1000;
        assert_eq!(
            prepare_host_bridge_apertures(&translated, &[]),
            Err(HostBridgeHandoffError::UnsupportedTranslation),
        );
    }

    #[test]
    fn fully_ram_backed_window_is_not_published() {
        let ram = [E820Range { start: 0x8000_0000, length: 0x1000_0000 }];
        let prepared = prepare_host_bridge_apertures(&set(), &ram).unwrap();
        assert_eq!(prepared.count, 0);
    }
}

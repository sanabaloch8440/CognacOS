//! Boot-time readiness graph for the in-kernel platform driver stacks.
//!
//! This is a dependency gate, not a substitute for the drivers themselves.
//! Stages are published only after the corresponding subsystem has completed
//! its real initialization path. The graph is monotonic for one boot session.

use core::sync::atomic::{AtomicU32, Ordering};
use cognacos_storage::Geometry;
#[cfg(test)]
extern crate std;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stage {
    Acpi = 0,
    PciXhciResources = 1,
    VtdRequester = 2,
    XhciInterrupt = 3,
    XhciController = 4,
    Storage = 5,
    ExtFilesystem = 6,
    KeyboardStack = 7,
}

const fn bit(stage: Stage) -> u32 { 1u32 << stage as u8 }
const ACPI: u32 = bit(Stage::Acpi);
const PCI: u32 = bit(Stage::PciXhciResources);
const VTD: u32 = bit(Stage::VtdRequester);
const XHCI_IRQ: u32 = bit(Stage::XhciInterrupt);
const STORAGE: u32 = bit(Stage::Storage);

const fn prerequisites(stage: Stage) -> u32 {
    match stage {
        Stage::Acpi | Stage::Storage | Stage::KeyboardStack => 0,
        Stage::PciXhciResources => ACPI,
        Stage::VtdRequester => ACPI | PCI,
        Stage::XhciInterrupt => ACPI | PCI,
        Stage::XhciController => PCI | VTD | XHCI_IRQ,
        Stage::ExtFilesystem => STORAGE,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishError {
    PrerequisiteMissing { required: u32, observed: u32 },
}

static READY: AtomicU32 = AtomicU32::new(0);

/// Publish a subsystem only after its initialization routine has succeeded.
/// A failed prerequisite check leaves the readiness bitmap unchanged.
pub fn mark_ready(stage: Stage) -> Result<(), PublishError> {
    let required = prerequisites(stage);
    let observed = READY.load(Ordering::Acquire);
    if observed & required != required {
        return Err(PublishError::PrerequisiteMissing { required, observed });
    }
    READY.fetch_or(bit(stage), Ordering::AcqRel);
    Ok(())
}

pub fn is_ready(stage: Stage) -> bool {
    READY.load(Ordering::Acquire) & bit(stage) != 0
}

pub fn validate_storage_geometry(sector_count: u64, logical_sector_size: u32, physical_sector_size: u32) -> bool {
    Geometry { sector_count, logical_sector_size, physical_sector_size }.validate().is_ok()
}

pub fn readiness_snapshot() -> u32 {
    READY.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xhci_cannot_be_published_before_dma_and_interrupt_prerequisites() {
        let required = prerequisites(Stage::XhciController);
        assert_eq!(required, PCI | VTD | XHCI_IRQ);
        assert_eq!(required & ACPI, 0); // ACPI is transitively required by PCI and VT-d.
    }

    #[test]
    fn dependency_graph_encodes_storage_before_filesystem() {
        assert_eq!(prerequisites(Stage::ExtFilesystem), STORAGE);
        assert_eq!(prerequisites(Stage::PciXhciResources), ACPI);
        assert_eq!(prerequisites(Stage::VtdRequester), ACPI | PCI);
    }

    #[test]
    fn storage_contract_rejects_invalid_sector_geometry() {
        assert!(validate_storage_geometry(4096, 512, 4096));
        assert!(!validate_storage_geometry(0, 512, 512));
        assert!(!validate_storage_geometry(4096, 4096, 512));
    }

    #[test]
    fn optional_keyboard_polling_does_not_require_apic_irq_routing() {
        assert_eq!(prerequisites(Stage::KeyboardStack), 0);
    }
}

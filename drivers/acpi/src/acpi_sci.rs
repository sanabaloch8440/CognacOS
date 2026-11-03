//! SCI routing and deferred execution lifecycle.
//!
//! SCI routing is resolved from FADT.SCI_INT plus MADT interrupt-source
//! overrides. The HAL owns vector/IDT and I/O-APIC operations; this module
//! enforces their ordering and keeps AML execution out of ISR/DPC context.
#![no_std]

use crate::{FadtInfo, MadtInfo};

pub const SCI_ABI_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SciError {
    MissingSciInterrupt,
    MissingIoApic,
    InvalidOverride,
    InvalidRoute,
    BackendUnavailable,
    BackendFailure,
    InvalidState,
    QueueFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SciRoute {
    pub source_irq: u8,
    pub gsi: u32,
    pub io_apic_address: u64,
    pub io_apic_gsi_base: u32,
    pub active_low: bool,
    pub level_triggered: bool,
}

/// Resolve the ACPI SCI source. In the absence of an ISA IRQ override, ACPI's
/// SCI electrical default is active-low, level-triggered. A MADT override
/// replaces both the GSI and polarity/trigger flags.
pub fn resolve_sci_route(fadt: &FadtInfo, madt: &MadtInfo) -> Result<SciRoute, SciError> {
    let irq = fadt.sci_interrupt;
    if irq == 0 || irq > u8::MAX as u16 {
        return Err(SciError::MissingSciInterrupt);
    }
    let mut gsi = irq as u32;
    let mut active_low = true;
    let mut level_triggered = true;
    for entry in madt.irq_overrides[..madt.irq_override_count].iter() {
        if entry.bus == 0 && entry.source_irq as u16 == irq {
            gsi = entry.gsi;
            active_low = entry.active_low;
            level_triggered = entry.level_triggered;
            break;
        }
    }
    let io_apic = madt.io_apics[..madt.io_apic_count]
        .iter()
        .filter(|io| io.address != 0 && io.gsi_base <= gsi)
        .max_by_key(|io| io.gsi_base)
        .ok_or(SciError::MissingIoApic)?;
    Ok(SciRoute {
        source_irq: irq as u8,
        gsi,
        io_apic_address: io_apic.address as u64,
        io_apic_gsi_base: io_apic.gsi_base,
        active_low,
        level_triggered,
    })
}

/// Function-pointer callbacks have the same lifetime as the ACPI driver image.
/// The ISR callback may only inspect/acknowledge hardware and capture bounded
/// event records. The DPC callback may only enqueue passive work.
pub type SciIsr = extern "C" fn(context: u64) -> bool;
pub type SciDpc = extern "C" fn(context: u64);
pub type SciPassiveWorker = extern "C" fn(context: u64);

/// Versioned, size-tagged service ABI consumed by ACPI.sys. The HAL must
/// install its IDT gate and callback before the I/O-APIC route is unmasked.
/// configure_masked also binds the route to the vector for later enable/disable.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AcpiSciHalServices {
    pub size: u32,
    pub version: u32,
    pub allocate_vector: Option<unsafe extern "C" fn(vector: *mut u8) -> u32>,
    pub configure_masked: Option<unsafe extern "C" fn(
        vector: u8, io_apic: u64, gsi_base: u32, gsi: u32,
        active_low: bool, level_triggered: bool,
    ) -> u32>,
    pub register: Option<unsafe extern "C" fn(
        vector: u8, context: u64, isr: Option<SciIsr>, dpc: Option<SciDpc>,
    ) -> u32>,
    pub set_enabled: Option<unsafe extern "C" fn(vector: u8, enabled: bool) -> u32>,
    pub unregister: Option<unsafe extern "C" fn(vector: u8, context: u64) -> u32>,
    pub release_vector: Option<unsafe extern "C" fn(vector: u8) -> u32>,
}
impl AcpiSciHalServices {
    pub const fn compatible(&self) -> bool {
        self.size as usize >= core::mem::size_of::<Self>()
            && self.version == SCI_ABI_VERSION
            && self.allocate_vector.is_some()
            && self.configure_masked.is_some()
            && self.register.is_some()
            && self.set_enabled.is_some()
            && self.unregister.is_some()
            && self.release_vector.is_some()
    }
}

/// Lifecycle state makes partial startup and teardown retryable. A route is
/// never enabled until vector allocation, masked route setup, IDT/ISR/DPC
/// registration, and GPE initialization have all succeeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SciState { Stopped, VectorAllocated, RouteConfigured, HandlerRegistered, Enabled, Stopping, Failed }

pub struct SciLifecycle {
    state: SciState,
    vector: u8,
    context: u64,
}
impl SciLifecycle {
    pub const fn new() -> Self {
        Self { state: SciState::Stopped, vector: 0, context: 0 }
    }
    pub fn state(&self) -> SciState { self.state }
    pub fn vector(&self) -> Option<u8> {
        if matches!(self.state, SciState::Stopped | SciState::Failed) { None } else { Some(self.vector) }
    }

    pub unsafe fn start(
        &mut self, services: &AcpiSciHalServices, route: SciRoute, context: u64,
        isr: SciIsr, dpc: SciDpc, initialize_gpe: impl FnOnce() -> bool,
    ) -> Result<u8, SciError> {
        if self.state != SciState::Stopped || context == 0 || !services.compatible() {
            return Err(SciError::InvalidState);
        }
        let allocate = services.allocate_vector.ok_or(SciError::BackendUnavailable)?;
        let configure = services.configure_masked.ok_or(SciError::BackendUnavailable)?;
        let register = services.register.ok_or(SciError::BackendUnavailable)?;
        let enable = services.set_enabled.ok_or(SciError::BackendUnavailable)?;
        let unregister = services.unregister.ok_or(SciError::BackendUnavailable)?;
        let release = services.release_vector.ok_or(SciError::BackendUnavailable)?;
        let mut vector = 0u8;
        if unsafe { allocate(&mut vector) } != 0 { return Err(SciError::BackendFailure); }
        if vector < 0x20 || vector > 0xdf {
            let _ = unsafe { release(vector) };
            return Err(SciError::BackendFailure);
        }
        self.vector = vector;
        self.context = context;
        self.state = SciState::VectorAllocated;

        if unsafe { configure(vector, route.io_apic_address, route.io_apic_gsi_base,
            route.gsi, route.active_low, route.level_triggered) } != 0 {
            let _ = unsafe { release(vector) };
            self.state = SciState::Stopped;
            return Err(SciError::BackendFailure);
        }
        self.state = SciState::RouteConfigured;
        if !initialize_gpe() {
            let _ = unsafe { release(vector) };
            self.state = SciState::Stopped;
            return Err(SciError::QueueFailure);
        }
        if unsafe { register(vector, context, Some(isr), Some(dpc)) } != 0 {
            let _ = unsafe { release(vector) };
            self.state = SciState::Stopped;
            return Err(SciError::BackendFailure);
        }
        self.state = SciState::HandlerRegistered;
        if unsafe { enable(vector, true) } != 0 {
            let _ = unsafe { enable(vector, false) };
            let _ = unsafe { unregister(vector, context) };
            let _ = unsafe { release(vector) };
            self.state = SciState::Stopped;
            return Err(SciError::BackendFailure);
        }
        self.state = SciState::Enabled;
        Ok(vector)
    }

    /// Mask first, then unregister (which must drain HAL ISR/DPC activity),
    /// then release the vector. Caller must drain the passive worker before
    /// freeing context or AML namespace state.
    pub unsafe fn stop(&mut self, services: &AcpiSciHalServices) -> Result<(), SciError> {
        if self.state == SciState::Stopped { return Ok(()); }
        if !services.compatible() { return Err(SciError::BackendUnavailable); }
        let vector = self.vector;
        let context = self.context;
        let handler_was_registered = matches!(self.state, SciState::HandlerRegistered | SciState::Enabled);
        self.state = SciState::Stopping;
        let enable = services.set_enabled.ok_or(SciError::BackendUnavailable)?;
        let unregister = services.unregister.ok_or(SciError::BackendUnavailable)?;
        let release = services.release_vector.ok_or(SciError::BackendUnavailable)?;
        if unsafe { enable(vector, false) } != 0 { return Err(SciError::BackendFailure); }
        if handler_was_registered && unsafe { unregister(vector, context) } != 0 {
            return Err(SciError::BackendFailure);
        }
        if unsafe { release(vector) } != 0 { return Err(SciError::BackendFailure); }
        self.state = SciState::Stopped;
        self.vector = 0;
        self.context = 0;
        Ok(())
    }
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IoApicEntry, IrqOverride};

    #[test]
    fn sci_route_uses_madt_override_and_matching_ioapic() {
        let mut fadt = FadtInfo::default();
        fadt.sci_interrupt = 9;
        let mut madt = MadtInfo::EMPTY;
        madt.io_apics[0] = IoApicEntry { id: 2, address: 0xfec0_0000, gsi_base: 0 };
        madt.io_apics[1] = IoApicEntry { id: 3, address: 0xfec0_1000, gsi_base: 24 };
        madt.io_apic_count = 2;
        madt.irq_overrides[0] = IrqOverride { bus: 0, source_irq: 9, gsi: 28, active_low: true, level_triggered: true };
        madt.irq_override_count = 1;
        let route = resolve_sci_route(&fadt, &madt).unwrap();
        assert_eq!(route.gsi, 28);
        assert_eq!(route.io_apic_address, 0xfec0_1000);
        assert!(route.active_low && route.level_triggered);
    }

    #[test]
    fn sci_route_defaults_to_isa_irq_and_acpi_sci_polarity() {
        let mut fadt = FadtInfo::default();
        fadt.sci_interrupt = 9;
        let mut madt = MadtInfo::EMPTY;
        madt.io_apics[0] = IoApicEntry { id: 0, address: 0xfec0_0000, gsi_base: 0 };
        madt.io_apic_count = 1;
        let route = resolve_sci_route(&fadt, &madt).unwrap();
        assert_eq!(route.gsi, 9);
        assert!(route.active_low);
        assert!(route.level_triggered);
    }

    #[test]
    fn missing_sci_or_ioapic_fails_closed() {
        assert_eq!(resolve_sci_route(&FadtInfo::default(), &MadtInfo::EMPTY), Err(SciError::MissingSciInterrupt));
        let mut fadt = FadtInfo::default(); fadt.sci_interrupt = 9;
        assert_eq!(resolve_sci_route(&fadt, &MadtInfo::EMPTY), Err(SciError::MissingIoApic));
    }
}

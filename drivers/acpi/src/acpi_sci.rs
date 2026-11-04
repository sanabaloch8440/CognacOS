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
    if madt.io_apic_count > madt.io_apics.len()
        || madt.irq_override_count > madt.irq_overrides.len() {
        return Err(SciError::InvalidRoute);
    }
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

/// Callback signatures and the size/version-tagged HAL service table live in
/// the shared HAL contract crate so both sides compile against one ABI definition.
pub use cognacos_hal_contract::{AcpiSciHalServices, AcpiSciIsr as SciIsr, AcpiSciDpc as SciDpc, AcpiQueuePassiveWorker, AcpiRundownPassiveWorker};
pub type SciPassiveWorker = extern "C" fn(context: u64);

/// Lifecycle state makes partial startup and teardown retryable. A route is
/// never enabled until vector allocation, masked route setup, IDT/ISR/DPC
/// registration, and GPE initialization have all succeeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SciState { Stopped, VectorAllocated, RouteConfigured, HandlerRegistered, Enabled, Stopping, Failed }

pub struct SciLifecycle {
    state: SciState,
    vector: u8,
    context: u64,
    vector_allocated: bool,
    handler_registered: bool,
    route_enabled: bool,
}
impl SciLifecycle {
    pub const fn new() -> Self {
        Self { state: SciState::Stopped, vector: 0, context: 0,
            vector_allocated: false, handler_registered: false, route_enabled: false }
    }
    pub fn state(&self) -> SciState { self.state }
    pub fn vector(&self) -> Option<u8> {
        if self.vector_allocated { Some(self.vector) } else { None }
    }
    fn reset_stopped(&mut self) {
        self.state = SciState::Stopped;
        self.vector = 0;
        self.context = 0;
        self.vector_allocated = false;
        self.handler_registered = false;
        self.route_enabled = false;
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
        let rundown = services.rundown_passive_worker.ok_or(SciError::BackendUnavailable)?;
        let mut vector = 0u8;
        if unsafe { allocate(&mut vector) } != 0 { return Err(SciError::BackendFailure); }
        if vector < 0x20 || vector > 0xdf {
            let _ = unsafe { release(vector) };
            return Err(SciError::BackendFailure);
        }
        self.vector = vector;
        self.context = context;
        self.vector_allocated = true;
        self.state = SciState::VectorAllocated;

        if unsafe { configure(vector, route.io_apic_address, route.io_apic_gsi_base,
            route.gsi, route.active_low, route.level_triggered) } != 0 {
            if unsafe { release(vector) } == 0 { self.reset_stopped(); }
            else { self.state = SciState::Failed; }
            return Err(SciError::BackendFailure);
        }
        self.state = SciState::RouteConfigured;
        if !initialize_gpe() {
            if unsafe { release(vector) } == 0 { self.reset_stopped(); }
            else { self.state = SciState::Failed; }
            return Err(SciError::QueueFailure);
        }
        if unsafe { register(vector, context, Some(isr), Some(dpc)) } != 0 {
            if unsafe { release(vector) } == 0 { self.reset_stopped(); }
            else { self.state = SciState::Failed; }
            return Err(SciError::BackendFailure);
        }
        self.handler_registered = true;
        self.state = SciState::HandlerRegistered;
        if unsafe { enable(vector, true) } != 0 {
            // Do not discard ownership flags when any rollback step fails. A
            // later stop() can resume from the exact completed teardown stage.
            if unsafe { enable(vector, false) } == 0 { self.route_enabled = false; }
            if self.handler_registered && unsafe { unregister(vector, context) } == 0 {
                self.handler_registered = false;
                if !rundown(context) { self.state = SciState::Failed; return Err(SciError::QueueFailure); }
            }
            if !self.handler_registered && unsafe { release(vector) } == 0 {
                self.reset_stopped();
            } else {
                self.state = SciState::Failed;
            }
            return Err(SciError::BackendFailure);
        }
        self.route_enabled = true;
        self.state = SciState::Enabled;
        Ok(vector)
    }

    /// Mask first, unregister and drain HAL ISR/DPC activity, then close the
    /// per-context passive queue and wait for AML to return before releasing
    /// the vector. State flags make partial teardown safely retryable.
    pub unsafe fn stop(&mut self, services: &AcpiSciHalServices) -> Result<(), SciError> {
        if !self.vector_allocated { return Ok(()); }
        if !services.compatible() { return Err(SciError::BackendUnavailable); }
        self.state = SciState::Stopping;
        if self.route_enabled {
            let enable = services.set_enabled.ok_or(SciError::BackendUnavailable)?;
            if unsafe { enable(self.vector, false) } != 0 { return Err(SciError::BackendFailure); }
            self.route_enabled = false;
        }
        if self.handler_registered {
            let unregister = services.unregister.ok_or(SciError::BackendUnavailable)?;
            if unsafe { unregister(self.vector, self.context) } != 0 {
                return Err(SciError::BackendFailure);
            }
            self.handler_registered = false;
        }
        // The DPC has completed before this callback is reached. It closes
        // admission for this owner and waits for any active passive callback.
        let rundown = services.rundown_passive_worker.ok_or(SciError::BackendUnavailable)?;
        if !rundown(self.context) { return Err(SciError::QueueFailure); }
        let release = services.release_vector.ok_or(SciError::BackendUnavailable)?;
        if unsafe { release(self.vector) } != 0 { return Err(SciError::BackendFailure); }
        self.reset_stopped();
        Ok(())
    }
}

/// Driver-owned callback context. Allocate this in nonpaged, stable storage
/// before registering the interrupt and retain it until HAL unregister plus
/// passive-worker rundown have completed.
#[repr(C)]
pub struct SciCallbackContext {
    pub owner_context: u64,
    /// Reads enabled GPE status, masks/acks pending sources, and appends
    /// records to the bounded GPE queue. Must be ISR-safe and nonblocking.
    pub capture_gpes: extern "C" fn(owner_context: u64) -> bool,
    /// Enqueues a kernel work item. Must not execute the worker inline.
    pub queue_passive_worker: AcpiQueuePassiveWorker,
    /// Close admission and drain callbacks for this owner context before unload.
    pub rundown_passive_worker: AcpiRundownPassiveWorker,
    /// Drains the GPE/EC queues and invokes AML at PASSIVE_LEVEL.
    pub run_passive_worker: extern "C" fn(owner_context: u64),
}
impl SciCallbackContext {
    /// Bind the executive queue callbacks from the versioned kernel/HAL table.
    /// The caller must place this value in stable nonpaged driver storage.
    pub fn new(
        owner_context: u64,
        capture_gpes: extern "C" fn(u64) -> bool,
        run_passive_worker: extern "C" fn(u64),
        services: &AcpiSciHalServices,
    ) -> Result<Self, SciError> {
        if owner_context == 0 || !services.compatible() {
            return Err(SciError::BackendUnavailable);
        }
        Ok(Self {
            owner_context,
            capture_gpes,
            queue_passive_worker: services.queue_passive_worker.ok_or(SciError::BackendUnavailable)?,
            rundown_passive_worker: services.rundown_passive_worker.ok_or(SciError::BackendUnavailable)?,
            run_passive_worker,
        })
    }

    /// Called only after SCI is masked and HAL has drained ISR/DPC callbacks.
    pub fn rundown(&self) -> bool {
        (self.rundown_passive_worker)(self as *const Self as u64)
    }
}

/// HAL ISR callback: capture only. The HAL queues the associated DPC after
/// interrupt dispatch/EOI; no AML or passive synchronization is permitted here.
pub extern "C" fn sci_isr_trampoline(context: u64) -> bool {
    if context == 0 { return false; }
    let callbacks = unsafe { &*(context as *const SciCallbackContext) };
    (callbacks.capture_gpes)(callbacks.owner_context)
}
/// HAL DPC callback: schedule a PASSIVE_LEVEL work item and return. The kernel
/// queue routine must be nonblocking and must not invoke AML synchronously.
pub extern "C" fn sci_dpc_trampoline(context: u64) {
    if context == 0 { return; }
    let callbacks = unsafe { &*(context as *const SciCallbackContext) };
    let _ = (callbacks.queue_passive_worker)(context, sci_passive_worker_trampoline);
}
/// Kernel work-item entry point. The caller guarantees PASSIVE_LEVEL and holds
/// an ACPI operation/rundown reference for the entire worker invocation.
pub extern "C" fn sci_passive_worker_trampoline(context: u64) {
    if context == 0 { return; }
    let callbacks = unsafe { &*(context as *const SciCallbackContext) };
    (callbacks.run_passive_worker)(callbacks.owner_context);
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

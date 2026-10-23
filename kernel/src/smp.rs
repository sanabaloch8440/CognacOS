//! SMP startup coordination. AP startup remains disabled until a real trampoline is installed.
use crate::{apic, hal};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmpError { InvalidTarget, UnsupportedApicId, InvalidCpuState, StartupTimeout, Apic(apic::ApicError) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApStartupTarget { pub logical_id: usize, pub apic_id: u32, pub trampoline_physical: u64, pub stack_top: u64, pub pml4_physical: u64 }
impl ApStartupTarget {
    pub fn validate(self) -> Result<Self, SmpError> {
        if self.logical_id == 0 || self.trampoline_physical == 0 || self.trampoline_physical >= 0x100000 || self.trampoline_physical & 0xfff != 0 || self.stack_top == 0 || self.stack_top & 0xf != 0 || self.stack_top >= 0x1_0000_0000 || self.pml4_physical == 0 || self.pml4_physical & 0xfff != 0 || self.pml4_physical >= 0x1_0000_0000 { return Err(SmpError::InvalidTarget); }
        if self.apic_id > 255 { return Err(SmpError::UnsupportedApicId); }
        Ok(self)
    }
}
/// Serialized INIT/deassert/SIPI sequence for one discovered AP.
/// The caller must provide a calibrated delay function and an initialized,
/// uncached xAPIC mapping. This routine is not invoked by boot until a real
/// trampoline and AP-side acknowledgement path are installed.
pub unsafe fn start_one_ap(
    local_apic_base: u64,
    target: ApStartupTarget,
    delay_us: fn(u32),
) -> Result<(), SmpError> {
    let target = target.validate()?;
    if hal::cpu_state(target.logical_id).ok() != Some(hal::CPU_DISCOVERED)
        || hal::cpu_local(target.logical_id)
            .map(|cpu| cpu.apic_id.load(core::sync::atomic::Ordering::Acquire)).ok()
            != Some(target.apic_id)
    {
        return Err(SmpError::InvalidCpuState);
    }
    hal::mark_cpu_starting(target.logical_id).map_err(|_| SmpError::InvalidCpuState)?;
    if let Err(error) = unsafe { apic::send_init_ipi(local_apic_base, target.apic_id as u8) } {
        let _ = hal::mark_cpu_failed(target.logical_id);
        return Err(SmpError::Apic(error));
    }
    delay_us(10_000);
    if let Err(error) = unsafe { apic::send_init_deassert_ipi(local_apic_base, target.apic_id as u8) } {
        let _ = hal::mark_cpu_failed(target.logical_id);
        return Err(SmpError::Apic(error));
    }
    delay_us(200);
    if let Err(error) = unsafe { apic::send_startup_ipi(local_apic_base, target.apic_id as u8, target.trampoline_physical) } {
        let _ = hal::mark_cpu_failed(target.logical_id);
        return Err(SmpError::Apic(error));
    }
    delay_us(200);
    if hal::cpu_state(target.logical_id).ok() != Some(hal::CPU_ONLINE) {
        if let Err(error) = unsafe { apic::send_startup_ipi(local_apic_base, target.apic_id as u8, target.trampoline_physical) } {
            let _ = hal::mark_cpu_failed(target.logical_id);
            return Err(SmpError::Apic(error));
        }
    }
    let mut waited = 0;
    while waited < 100_000 {
        if hal::cpu_state(target.logical_id).ok() == Some(hal::CPU_ONLINE) {
            return Ok(());
        }
        delay_us(100);
        waited += 100;
    }
    let _ = hal::mark_cpu_failed(target.logical_id);
    Err(SmpError::StartupTimeout)
}

pub fn self_test() -> bool {
    let target = ApStartupTarget { logical_id: 1, apic_id: 2, trampoline_physical: 0x8000, stack_top: 0x204000, pml4_physical: 0x1000 };
    target.validate() == Ok(target) && ApStartupTarget { apic_id: 256, ..target }.validate() == Err(SmpError::UnsupportedApicId) && ApStartupTarget { trampoline_physical: 0, ..target }.validate() == Err(SmpError::InvalidTarget)
}

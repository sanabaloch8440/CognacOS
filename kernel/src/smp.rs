//! SMP startup coordination. AP startup remains disabled until a real trampoline is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmpError { InvalidTarget, UnsupportedApicId, InvalidCpuState, StartupTimeout }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApStartupTarget { pub logical_id: usize, pub apic_id: u32, pub trampoline_physical: u64, pub stack_top: u64, pub pml4_physical: u64 }
impl ApStartupTarget {
    pub fn validate(self) -> Result<Self, SmpError> {
        if self.logical_id == 0 || self.trampoline_physical == 0 || self.trampoline_physical >= 0x100000 || self.trampoline_physical & 0xfff != 0 || self.stack_top == 0 || self.stack_top & 0xf != 0 || self.stack_top >= 0x1_0000_0000 || self.pml4_physical == 0 || self.pml4_physical & 0xfff != 0 || self.pml4_physical >= 0x1_0000_0000 { return Err(SmpError::InvalidTarget); }
        if self.apic_id > 255 { return Err(SmpError::UnsupportedApicId); }
        Ok(self)
    }
}
pub fn self_test() -> bool {
    let target = ApStartupTarget { logical_id: 1, apic_id: 2, trampoline_physical: 0x8000, stack_top: 0x204000, pml4_physical: 0x1000 };
    target.validate() == Ok(target) && ApStartupTarget { apic_id: 256, ..target }.validate() == Err(SmpError::UnsupportedApicId) && ApStartupTarget { trampoline_physical: 0, ..target }.validate() == Err(SmpError::InvalidTarget)
}

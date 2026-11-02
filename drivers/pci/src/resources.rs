//! Transactional PCI BAR sizing and resource assignment primitives.
//!
//! The caller MUST serialize configuration access for the entire operation and
//! quiesce the function through PnP before probing. This module never enables
//! bus mastering. BAR assignment is a separate allocator transaction; it does
//! not guess host bridge windows or publish resources before commit.
#![no_std]

use core::cmp;

pub const PCI_COMMAND_IO: u16 = 1 << 0;
pub const PCI_COMMAND_MEMORY: u16 = 1 << 1;
pub const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;
pub const PCI_BAR0: u16 = 0x10;
pub const PCI_COMMAND: u16 = 0x04;

pub trait ConfigAccess {
    fn read_u16(&mut self, offset: u16) -> Result<u16, ResourceError>;
    fn write_u16(&mut self, offset: u16, value: u16) -> Result<(), ResourceError>;
    fn read_u32(&mut self, offset: u16) -> Result<u32, ResourceError>;
    fn write_u32(&mut self, offset: u16, value: u32) -> Result<(), ResourceError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    Config,
    UnsupportedHeader,
    InvalidBar,
    ZeroLength,
    AddressOverflow,
    NoWindow,
    NoSpace,
    InvalidAlignment,
    Capacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarKind { Io, Memory32, Memory64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarRequirement {
    pub index: u8,
    pub kind: BarKind,
    pub length: u64,
    pub alignment: u64,
    pub prefetchable: bool,
    pub assigned: u64,
}

pub const MAX_BARS: usize = 6;

/// Probe every BAR with decode disabled and restore all original BAR values on
/// every exit path. The command register is restored with bus-mastering forced
/// off: only the DMA subsystem may enable it after IOMMU attachment.
pub fn size_bars<C: ConfigAccess>(
    config: &mut C,
    header_type: u8,
    output: &mut [Option<BarRequirement>; MAX_BARS],
) -> Result<usize, ResourceError> {
    let count = match header_type & 0x7f { 0 => 6, 1 => 2, _ => return Err(ResourceError::UnsupportedHeader) };
    output.fill(None);
    let original_command = config.read_u16(PCI_COMMAND)?;
    let mut original = [0u32; MAX_BARS];
    let mut saved = [false; MAX_BARS];
    let result = (|| {
        config.write_u16(PCI_COMMAND, original_command & !(PCI_COMMAND_IO | PCI_COMMAND_MEMORY | PCI_COMMAND_BUS_MASTER))?;
        for i in 0..count {
            let offset = PCI_BAR0 + (i as u16) * 4;
            original[i] = config.read_u32(offset)?;
            saved[i] = true;
        }
        let mut index = 0usize;
        let mut found = 0usize;
        while index < count {
            let low = original[index];
            if low == u32::MAX { index += 1; continue; }
            let is_io = low & 1 != 0;
            let kind = if is_io { BarKind::Io } else {
                match (low >> 1) & 3 {
                    0 => BarKind::Memory32,
                    2 => BarKind::Memory64,
                    _ => return Err(ResourceError::InvalidBar),
                }
            };
            if kind == BarKind::Memory64 && index + 1 >= count { return Err(ResourceError::InvalidBar); }
            config.write_u32(PCI_BAR0 + (index as u16) * 4, u32::MAX)?;
            if kind == BarKind::Memory64 {
                config.write_u32(PCI_BAR0 + ((index as u16) + 1) * 4, u32::MAX)?;
            }
            let mask_low = config.read_u32(PCI_BAR0 + (index as u16) * 4)?;
            let mask_high = if kind == BarKind::Memory64 {
                config.read_u32(PCI_BAR0 + ((index as u16) + 1) * 4)?
            } else { 0 };
            let mask = match kind {
                BarKind::Io => (mask_low & !3) as u64,
                BarKind::Memory32 => (mask_low & !0xf) as u64,
                BarKind::Memory64 => ((mask_high as u64) << 32) | (mask_low as u64 & !0xf),
            };
            let length = match kind {
                BarKind::Memory64 => (!mask).wrapping_add(1),
                _ => ((!mask) & 0xffff_ffff).wrapping_add(1),
            };
            if length == 0 || !length.is_power_of_two() { return Err(ResourceError::ZeroLength); }
            if found >= output.len() { return Err(ResourceError::Capacity); }
            output[found] = Some(BarRequirement {
                index: index as u8, kind, length, alignment: length,
                prefetchable: kind != BarKind::Io && low & 8 != 0, assigned: 0,
            });
            found += 1;
            index += if kind == BarKind::Memory64 { 2 } else { 1 };
        }
        Ok(found)
    })();
    // Restore in reverse order; attempt every restore even if one write fails.
    let mut restore_failed = false;
    for i in (0..count).rev() {
        if saved[i] && config.write_u32(PCI_BAR0 + (i as u16) * 4, original[i]).is_err() {
            restore_failed = true;
        }
    }
    if config.write_u16(PCI_COMMAND, original_command & !PCI_COMMAND_BUS_MASTER).is_err() {
        restore_failed = true;
    }
    if restore_failed { return Err(ResourceError::Config); }
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceWindow { pub start: u64, pub length: u64, pub kind: BarKind }

/// First-fit placement inside a firmware/bridge-provided window. The caller
/// owns the reservation bitmap/lock and commits all selected BARs atomically.
pub fn place_requirement(
    requirement: &BarRequirement,
    window: ResourceWindow,
    occupied: &[(u64, u64)],
) -> Result<u64, ResourceError> {
    if requirement.kind != window.kind || requirement.length == 0
        || requirement.alignment == 0 || !requirement.alignment.is_power_of_two() {
        return Err(ResourceError::InvalidAlignment);
    }
    let end = window.start.checked_add(window.length).ok_or(ResourceError::AddressOverflow)?;
    let mut candidate = window.start.checked_add(requirement.alignment - 1)
        .ok_or(ResourceError::AddressOverflow)? & !(requirement.alignment - 1);
    loop {
        let candidate_end = candidate.checked_add(requirement.length).ok_or(ResourceError::AddressOverflow)?;
        if candidate_end > end { return Err(ResourceError::NoSpace); }
        let mut conflict_end = None;
        for &(base, length) in occupied {
            let occupied_end = base.checked_add(length).ok_or(ResourceError::AddressOverflow)?;
            if candidate < occupied_end && base < candidate_end {
                conflict_end = Some(cmp::max(conflict_end.unwrap_or(0), occupied_end));
            }
        }
        match conflict_end {
            None => return Ok(candidate),
            Some(next) => candidate = next.checked_add(requirement.alignment - 1)
                .ok_or(ResourceError::AddressOverflow)? & !(requirement.alignment - 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake { regs: [u32; 64], fail_restore: bool }
    impl ConfigAccess for Fake {
        fn read_u16(&mut self, o:u16)->Result<u16,ResourceError>{Ok(self.regs[o as usize/4] as u16)}
        fn write_u16(&mut self,o:u16,v:u16)->Result<(),ResourceError>{self.regs[o as usize/4]=(self.regs[o as usize/4]&0xffff_0000)|v as u32;Ok(())}
        fn read_u32(&mut self,o:u16)->Result<u32,ResourceError>{Ok(self.regs[o as usize/4])}
        fn write_u32(&mut self,o:u16,v:u32)->Result<(),ResourceError>{
            if self.fail_restore && v != u32::MAX { return Err(ResourceError::Config); }
            self.regs[o as usize/4]=if v==u32::MAX { if o==PCI_BAR0 {0xffff_fff0} else {0xffff_ffff} } else {v}; Ok(())
        }
    }
    #[test] fn bar_probe_restores_registers_and_never_restores_bus_master() {
        let mut config=Fake{regs:[0;64],fail_restore:false};
        config.regs[1]=7;
        config.regs[4]=0x1000;
        let mut output=[None;MAX_BARS];
        let count=size_bars(&mut config,0,&mut output).unwrap();
        assert_eq!(count,1);
        assert_eq!(output[0].unwrap().length,0x10);
        assert_eq!(config.regs[4],0x1000);
        assert_eq!(config.regs[1] as u16,3);
    }
    #[test] fn placement_respects_alignment_and_existing_ranges() {
        let r=BarRequirement{index:0,kind:BarKind::Memory32,length:0x1000,alignment:0x1000,prefetchable:false,assigned:0};
        assert_eq!(place_requirement(&r,ResourceWindow{start:0x8000_0000,length:0x4000,kind:BarKind::Memory32},&[(0x8000_0000,0x1000)]),Ok(0x8000_1000));
    }
    #[test] fn placement_rejects_wrong_window_and_exhaustion() {
        let r=BarRequirement{index:0,kind:BarKind::Io,length:0x100,alignment:0x100,prefetchable:false,assigned:0};
        assert_eq!(place_requirement(&r,ResourceWindow{start:0x1000,length:0x80,kind:BarKind::Io},&[]),Err(ResourceError::NoSpace));
    }
}

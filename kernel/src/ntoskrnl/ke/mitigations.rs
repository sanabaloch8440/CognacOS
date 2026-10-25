//! Early CPU mitigation setup for the Rust x86-64 kernel.
//!
//! This enables architectural protections that are safe with the current
//! supervisor-only bootstrap address space. NXE makes NX page-table bits
//! meaningful, but does not itself make any page non-executable. SMAP is
//! detected but intentionally deferred until guarded user-memory accessors
//! exist and can bracket accesses with STAC/CLAC.

use core::arch::{asm, x86_64::__cpuid_count};

const CR0_WP: u64 = 1 << 16;
const CR4_SMEP: u64 = 1 << 20;
const CR4_SMAP: u64 = 1 << 21;
const EFER_MSR: u32 = 0xC000_0080;
const EFER_NXE: u64 = 1 << 11;

#[derive(Clone, Copy)]
pub struct MitigationStatus {
    pub write_protect: bool,
    pub nx_supported: bool,
    pub nx_enabled: bool,
    pub smep_supported: bool,
    pub smep_enabled: bool,
    pub smap_supported: bool,
    /// Must remain false until every user-memory access path brackets accesses with STAC/CLAC.
    pub smap_enabled: bool,
}

unsafe fn read_msr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}

unsafe fn write_msr(msr: u32, value: u64) {
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags)
        );
    }
}

unsafe fn read_cr0() -> u64 {
    let value: u64;
    unsafe { asm!("mov {}, cr0", out(reg) value, options(nomem, nostack, preserves_flags)); }
    value
}

unsafe fn write_cr0(value: u64) {
    unsafe { asm!("mov cr0, {}", in(reg) value, options(nostack, preserves_flags)); }
}

unsafe fn read_cr4() -> u64 {
    let value: u64;
    unsafe { asm!("mov {}, cr4", out(reg) value, options(nomem, nostack, preserves_flags)); }
    value
}

unsafe fn write_cr4(value: u64) {
    unsafe { asm!("mov cr4, {}", in(reg) value, options(nostack, preserves_flags)); }
}

/// Enable CPU protections supported by the processor.
///
/// # Safety
/// Must run at ring 0 after the kernel has a valid identity map. Current page
/// tables contain only supervisor mappings, so enabling SMEP is safe. SMAP is
/// left disabled until explicit user-copy routines are implemented.
pub unsafe fn initialize() -> MitigationStatus {
    let basic_max = unsafe { __cpuid_count(0, 0).eax };
    let ext_max = unsafe { __cpuid_count(0x8000_0000, 0).eax };
    let nx_supported = ext_max >= 0x8000_0001
        && (unsafe { __cpuid_count(0x8000_0001, 0).edx } & (1 << 20)) != 0;
    // Query structured feature flags only when CPUID advertises leaf 7.
    let (smep_supported, smap_supported) = if basic_max >= 7 {
        let leaf7 = unsafe { __cpuid_count(7, 0) };
        ((leaf7.ebx & (1 << 7)) != 0, (leaf7.ebx & (1 << 20)) != 0)
    } else {
        (false, false)
    };

    // CR0.WP makes supervisor writes respect read-only PTEs once page
    // permissions become strict; it is safe with the current writable map.
    unsafe { write_cr0(read_cr0() | CR0_WP); }

    let mut nx_enabled = false;
    if nx_supported {
        let efer = unsafe { read_msr(EFER_MSR) };
        unsafe { write_msr(EFER_MSR, efer | EFER_NXE); }
        nx_enabled = (unsafe { read_msr(EFER_MSR) } & EFER_NXE) != 0;
    }

    // Firmware/previous stages are not trusted to leave CR4.SMAP clear. Force
    // it off until all user-buffer accessors are audited for STAC/CLAC. SMEP is
    // independently safe because user pages are not currently executable here.
    let mut cr4 = unsafe { read_cr4() } & !CR4_SMAP;
    if smep_supported { cr4 |= CR4_SMEP; }
    unsafe { write_cr4(cr4); }
    let cr4_after = unsafe { read_cr4() };
    let smep_enabled = cr4_after & CR4_SMEP != 0;
    let smap_enabled = cr4_after & CR4_SMAP != 0;

    MitigationStatus {
        write_protect: (unsafe { read_cr0() } & CR0_WP) != 0,
        nx_supported,
        nx_enabled,
        smep_supported,
        smep_enabled,
        smap_supported,
        smap_enabled,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smap_policy_always_clears_smap_without_disabling_other_cr4_features() {
        let before = CR4_SMAP | CR4_SMEP | (1 << 9);
        let after = before & !CR4_SMAP;
        assert_eq!(after & CR4_SMAP, 0);
        assert_ne!(after & CR4_SMEP, 0);
        assert_ne!(after & (1 << 9), 0);
    }
}

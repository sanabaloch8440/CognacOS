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
    let leaf1 = if basic_max >= 1 { unsafe { __cpuid_count(1, 0) } } else {
        core::arch::x86_64::__cpuid(0)
    };
    let _ = leaf1; // Reserved for future feature-policy checks.

    let ext_max = unsafe { __cpuid_count(0x8000_0000, 0).eax };
    let nx_supported = ext_max >= 0x8000_0001
        && (unsafe { __cpuid_count(0x8000_0001, 0).edx } & (1 << 20)) != 0;
    let leaf7 = if basic_max >= 7 { unsafe { __cpuid_count(7, 0) } } else {
        // No structured feature leaf means SMEP/SMAP are unavailable.
        unsafe { __cpuid_count(0, 0) }
    };
    let smep_supported = basic_max >= 7 && (leaf7.ebx & (1 << 7)) != 0;
    let smap_supported = basic_max >= 7 && (leaf7.ebx & (1 << 20)) != 0;

    // CR0.WP makes supervisor writes respect read-only PTEs once page
    // permissions become strict; it is safe with the current writable map.
    unsafe { write_cr0(read_cr0() | CR0_WP); }

    let mut nx_enabled = false;
    if nx_supported {
        let efer = unsafe { read_msr(EFER_MSR) };
        unsafe { write_msr(EFER_MSR, efer | EFER_NXE); }
        nx_enabled = (unsafe { read_msr(EFER_MSR) } & EFER_NXE) != 0;
    }

    let mut smep_enabled = false;
    if smep_supported {
        unsafe { write_cr4(read_cr4() | CR4_SMEP); }
        smep_enabled = (unsafe { read_cr4() } & CR4_SMEP) != 0;
    }

    MitigationStatus {
        write_protect: (unsafe { read_cr0() } & CR0_WP) != 0,
        nx_supported,
        nx_enabled,
        smep_supported,
        smep_enabled,
        smap_supported,
    }
}

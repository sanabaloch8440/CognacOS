//! x86-64 XSAVE/XRSTOR policy and per-context state operations.
//!
//! This module deliberately enables only state components enumerated by CPUID.
//! Boot code must call `ki_initialize_xstate_current_cpu` on every logical CPU
//! before using AVX or the XSAVE helpers. The first implementation manages the
//! architectural x87/SSE/AVX components; supervisor XSS state and dynamic
//! permissioning (AMX/PKRU) are not enabled by this policy.
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const CPUID_XSAVE: u32 = 1 << 26;
const CR4_OSFXSR: u64 = 1 << 9;
const CR4_OSXMMEXCPT: u64 = 1 << 10;
const CR4_OSXSAVE: u64 = 1 << 18;
const XSTATE_X87: u64 = 1 << 0;
const XSTATE_SSE: u64 = 1 << 1;
const XSTATE_AVX: u64 = 1 << 2;
const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;

static XSTATE_SUPPORTED_MASK: AtomicU64 = AtomicU64::new(0);
static XSTATE_ENABLED_MASK: AtomicU64 = AtomicU64::new(0);
static XSTATE_AREA_SIZE: AtomicU32 = AtomicU32::new(0);
static XSTATE_INITIALIZED: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct CognacXStateConfiguration {
    pub supported_features: u64,
    pub enabled_features: u64,
    pub area_size: u32,
    pub standard_area_size: u32,
    pub xsaveopt: u32,
    pub xsavec: u32,
    pub xsaves: u32,
    pub reserved: u32,
    /// Standard-format offsets/sizes indexed by architectural XSTATE feature ID.
    pub feature_offsets: [u32; 64],
    pub feature_sizes: [u32; 64],
}

#[inline]
fn cpuid(leaf: u32, subleaf: u32) -> core::arch::x86_64::CpuidResult {
    // SAFETY: CPUID is available on all x86-64 processors.
    unsafe { core::arch::x86_64::__cpuid_count(leaf, subleaf) }
}

/// Configure XSAVE on the current logical processor. Call during early CPU
/// bring-up, with interrupts disabled and before scheduling user threads.
pub unsafe fn ki_initialize_xstate_current_cpu() -> bool {
    let basic = cpuid(0, 0);
    if basic.eax < 0x0d { return false; }
    let leaf1 = cpuid(1, 0);
    if leaf1.ecx & CPUID_XSAVE == 0 { return false; }
    let leafd0 = cpuid(0x0d, 0);
    let hardware_mask = ((leafd0.edx as u64) << 32) | leafd0.eax as u64;
    if hardware_mask & XSTATE_X87 == 0 || hardware_mask & XSTATE_SSE == 0 { return false; }

    // The OS owns CR4.OSXSAVE. Set the legacy SSE control bits as well, then
    // set XCR0 only to the dependency-closed state components we will save.
    let mut cr4: u64;
    unsafe { core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags)); }
    cr4 |= CR4_OSFXSR | CR4_OSXMMEXCPT | CR4_OSXSAVE;
    unsafe { core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack, preserves_flags)); }

    // Until the scheduler owns a per-thread XSAVE area and saves/restores it
    // on every context switch, expose only the legacy x87/SSE components.
    // Enabling AVX here would allow user code to corrupt another thread's YMM state.
    let enabled = XSTATE_X87 | XSTATE_SSE;
    let lo = enabled as u32;
    let hi = (enabled >> 32) as u32;
    unsafe { core::arch::asm!("xsetbv", in("ecx") 0u32, in("eax") lo, in("edx") hi, options(nostack)); }

    let leafd1 = cpuid(0x0d, 1);
    let size = leafd0.ebx;
    if size < 576 { return false; }
    XSTATE_SUPPORTED_MASK.store(hardware_mask, Ordering::Release);
    XSTATE_ENABLED_MASK.store(enabled, Ordering::Release);
    XSTATE_AREA_SIZE.store(size, Ordering::Release);
    let subfeatures = leafd1.eax;
    XSTATE_INITIALIZED.store(1 | ((subfeatures & 1) << 1) | ((subfeatures & 2) << 1) | ((subfeatures & 8) << 1), Ordering::Release);
    true
}

pub fn ki_query_xstate_configuration() -> CognacXStateConfiguration {
    let flags = XSTATE_INITIALIZED.load(Ordering::Acquire);
    let size = XSTATE_AREA_SIZE.load(Ordering::Acquire);
    let supported = XSTATE_SUPPORTED_MASK.load(Ordering::Acquire);
    let mut offsets = [0u32; 64];
    let mut sizes = [0u32; 64];
    sizes[0] = 160;
    offsets[1] = 160;
    sizes[1] = 352;
    for feature in 2..64 {
        if supported & (1u64 << feature) == 0 { continue; }
        let leaf = cpuid(0x0d, feature as u32);
        offsets[feature] = leaf.ebx;
        sizes[feature] = leaf.eax;
    }
    CognacXStateConfiguration {
        supported_features: supported,
        enabled_features: XSTATE_ENABLED_MASK.load(Ordering::Acquire),
        area_size: size,
        standard_area_size: size,
        xsaveopt: (flags >> 1) & 1,
        xsavec: (flags >> 2) & 1,
        xsaves: (flags >> 4) & 1,
        reserved: 0,
        feature_offsets: offsets,
        feature_sizes: sizes,
    }
}

/// Save enabled XSTATE components into a caller-owned 64-byte-aligned XSAVE
/// area. The buffer must be at least the configured area size.
pub unsafe fn ki_xstate_save(area: *mut u8, length: usize, mask: u64) -> i32 {
    let enabled = XSTATE_ENABLED_MASK.load(Ordering::Acquire);
    let size = XSTATE_AREA_SIZE.load(Ordering::Acquire) as usize;
    if XSTATE_INITIALIZED.load(Ordering::Acquire) & 1 == 0 { return STATUS_NOT_SUPPORTED; }
    if area.is_null() || (area as usize) & 63 != 0 || length < size || mask == 0 || mask & !enabled != 0 {
        return if !area.is_null() && length < size { STATUS_BUFFER_TOO_SMALL } else { STATUS_INVALID_PARAMETER };
    }
    // Initialize reserved/header bytes before XSAVE so absent state is deterministic.
    unsafe { core::ptr::write_bytes(area, 0, size); }
    let lo = mask as u32;
    let hi = (mask >> 32) as u32;
    unsafe { core::arch::asm!("xsave64 [{}]", in(reg) area, in("eax") lo, in("edx") hi, options(nostack)); }
    STATUS_SUCCESS
}

/// Restore only the requested, enabled components from a validated XSAVE area.
pub unsafe fn ki_xstate_restore(area: *const u8, length: usize, mask: u64) -> i32 {
    let enabled = XSTATE_ENABLED_MASK.load(Ordering::Acquire);
    let size = XSTATE_AREA_SIZE.load(Ordering::Acquire) as usize;
    if XSTATE_INITIALIZED.load(Ordering::Acquire) & 1 == 0 { return STATUS_NOT_SUPPORTED; }
    if area.is_null() || (area as usize) & 63 != 0 || length < size || mask == 0 || mask & !enabled != 0 {
        return if !area.is_null() && length < size { STATUS_BUFFER_TOO_SMALL } else { STATUS_INVALID_PARAMETER };
    }
    let lo = mask as u32;
    let hi = (mask >> 32) as u32;
    unsafe { core::arch::asm!("xrstor64 [{}]", in(reg) area, in("eax") lo, in("edx") hi, options(nostack)); }
    STATUS_SUCCESS
}

#[no_mangle]
pub extern "win64" fn CognacQueryXStateConfiguration(out: *mut CognacXStateConfiguration) -> i32 {
    if out.is_null() { return STATUS_INVALID_PARAMETER; }
    if XSTATE_INITIALIZED.load(Ordering::Acquire) & 1 == 0 { return STATUS_NOT_SUPPORTED; }
    unsafe { out.write(ki_query_xstate_configuration()); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "win64" fn CognacSaveXState(area: *mut u8, length: usize, mask: u64) -> i32 {
    unsafe { ki_xstate_save(area, length, mask) }
}

#[no_mangle]
pub unsafe extern "win64" fn CognacRestoreXState(area: *const u8, length: usize, mask: u64) -> i32 {
    unsafe { ki_xstate_restore(area, length, mask) }
}

//! x86-64 XSAVE/XRSTOR policy and per-context state operations.
//!
//! This module deliberately enables only state components enumerated by CPUID.
//! Boot code must call `ki_initialize_xstate_current_cpu` on every logical CPU
//! before using AVX or the XSAVE helpers. The first implementation manages the
//! architectural x87/SSE components; AVX/YMM, supervisor XSS state and dynamic
//! permissioning (AMX/PKRU) remain disabled until the user-mode trap path and
//! full feature-specific context formats are integrated.
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const CPUID_XSAVE: u32 = 1 << 26;
const CR0_MP: u64 = 1 << 1;
const CR0_EM: u64 = 1 << 2;
const CR0_TS: u64 = 1 << 3;
const CR0_NE: u64 = 1 << 5;
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
#[no_mangle]
pub static COGNAC_XSTATE_ENABLED_MASK_ASM: AtomicU64 = AtomicU64::new(0);
static XSTATE_AREA_SIZE: AtomicU32 = AtomicU32::new(0);
static XSTATE_INITIALIZED: AtomicU32 = AtomicU32::new(0);
static XSTATE_POLICY_FINALIZED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Windows 11 24H2 x64 XSTATE_CONFIGURATION as embedded in
/// KUSER_SHARED_DATA at offset 0x3d8. The layout is fixed at 0x348 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WindowsXStateFeature { pub offset: u32, pub size: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WindowsXStateConfiguration {
    pub enabled_features: u64,
    pub enabled_volatile_features: u64,
    pub size: u32,
    pub control_flags: u32,
    pub features: [WindowsXStateFeature; 64],
    pub enabled_supervisor_features: u64,
    pub aligned_features: u64,
    pub all_feature_size: u32,
    pub all_features: [u32; 64],
    pub enabled_user_visible_supervisor_features: u64,
    pub extended_feature_disable_features: u64,
    pub all_non_large_feature_size: u32,
    pub max_sve_vector_length: u16,
    pub spare1: u16,
}
const _: [(); 0x348] = [(); core::mem::size_of::<WindowsXStateConfiguration>()];
const _: [(); 0x18] = [(); core::mem::offset_of!(WindowsXStateConfiguration, features)];
const _: [(); 0x330] = [(); core::mem::offset_of!(WindowsXStateConfiguration, enabled_user_visible_supervisor_features)];

impl WindowsXStateConfiguration {
    pub const fn zeroed() -> Self {
        Self { enabled_features: 0, enabled_volatile_features: 0, size: 0, control_flags: 0,
            features: [WindowsXStateFeature { offset: 0, size: 0 }; 64],
            enabled_supervisor_features: 0, aligned_features: 0, all_feature_size: 0,
            all_features: [0; 64], enabled_user_visible_supervisor_features: 0,
            extended_feature_disable_features: 0, all_non_large_feature_size: 0, max_sve_vector_length: 0, spare1: 0 }
    }
}
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
    pub compaction_enabled: u32,
    /// Standard-format offsets/sizes indexed by architectural XSTATE feature ID.
    pub feature_offsets: [u32; 64],
    pub feature_sizes: [u32; 64],
}

/// Check whether a logical processor can honor the already-published
/// system-wide XSTATE policy before the scheduler migrates threads to it.
fn xstate_policy_compatible(global_enabled: u64, local_supported: u64, local_enabled: u64,
                            local_area_size: u32, global_area_size: u32) -> bool {
    global_enabled != 0 && local_enabled == global_enabled
        && local_supported & global_enabled == global_enabled
        && local_area_size >= global_area_size
}

#[inline]
fn cpuid(leaf: u32, subleaf: u32) -> core::arch::x86_64::CpuidResult {
    // SAFETY: CPUID is available on all x86-64 processors.
    unsafe { core::arch::x86_64::__cpuid_count(leaf, subleaf) }
}

/// Configure XSAVE on the current logical processor. Call during early CPU
/// bring-up, with interrupts disabled and before scheduling user threads.
pub unsafe fn ki_initialize_xstate_current_cpu() -> bool {
    if XSTATE_POLICY_FINALIZED.load(Ordering::Acquire) && !ki_xstate_enabled() { return false; }
    let basic = cpuid(0, 0);
    let leaf1 = cpuid(1, 0);
    if basic.eax < 0x0d || leaf1.ecx & CPUID_XSAVE == 0 {
        if !XSTATE_POLICY_FINALIZED.swap(true, Ordering::AcqRel) {
            XSTATE_INITIALIZED.store(0, Ordering::Release);
        }
        return false;
    }
    let leafd0 = cpuid(0x0d, 0);
    let hardware_mask = ((leafd0.edx as u64) << 32) | leafd0.eax as u64;
    if hardware_mask & XSTATE_X87 == 0 || hardware_mask & XSTATE_SSE == 0 {
        if !XSTATE_POLICY_FINALIZED.swap(true, Ordering::AcqRel) {
            XSTATE_INITIALIZED.store(0, Ordering::Release);
        }
        return false;
    }

    // Enable native x87/SSE execution before enabling OSXSAVE. This is done
    // during CPU bring-up before the processor can run a user thread.
    let mut cr0: u64;
    unsafe { core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags)); }
    cr0 = (cr0 & !(CR0_EM | CR0_TS)) | CR0_MP | CR0_NE;
    unsafe { core::arch::asm!("mov cr0, {}", in(reg) cr0, options(nomem, nostack, preserves_flags)); }
    // The OS owns CR4.OSXSAVE. Set the legacy SSE control bits as well, then
    // set XCR0 only to the dependency-closed state components we will save.
    let mut cr4: u64;
    unsafe { core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags)); }
    cr4 |= CR4_OSFXSR | CR4_OSXMMEXCPT | CR4_OSXSAVE;
    unsafe { core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack, preserves_flags)); }

    // The scheduler now preserves a fixed legacy x87/SSE area per thread, but
    // user-mode trap frames and extended CONTEXT layouts are not yet complete.
    // Keep AVX disabled until those paths can preserve YMM state end-to-end.
    let enabled = XSTATE_X87 | XSTATE_SSE;
    let lo = enabled as u32;
    let hi = (enabled >> 32) as u32;
    unsafe { core::arch::asm!("xsetbv", in("ecx") 0u32, in("eax") lo, in("edx") hi, options(nostack)); }

    let leafd1 = cpuid(0x0d, 1);
    // EBX is the area size for the XCR0 value currently enabled; query it
    // again after XSETBV rather than using the pre-configuration size.
    let size = cpuid(0x0d, 0).ebx;
    if size < 576 {
        if !XSTATE_POLICY_FINALIZED.swap(true, Ordering::AcqRel) {
            XSTATE_INITIALIZED.store(0, Ordering::Release);
        }
        return false;
    }
    if XSTATE_POLICY_FINALIZED.load(Ordering::Acquire) {
        let global_enabled = XSTATE_ENABLED_MASK.load(Ordering::Acquire);
        return xstate_policy_compatible(global_enabled, hardware_mask, enabled,
            size, XSTATE_AREA_SIZE.load(Ordering::Acquire));
    }
    XSTATE_SUPPORTED_MASK.store(hardware_mask, Ordering::Release);
    XSTATE_ENABLED_MASK.store(enabled, Ordering::Release);
    COGNAC_XSTATE_ENABLED_MASK_ASM.store(enabled, Ordering::Release);
    XSTATE_AREA_SIZE.store(size, Ordering::Release);
    let subfeatures = leafd1.eax;
    XSTATE_INITIALIZED.store(1 | ((subfeatures & 1) << 1) | ((subfeatures & 2) << 1) | ((subfeatures & 8) << 1), Ordering::Release);
    XSTATE_POLICY_FINALIZED.store(true, Ordering::Release);
    true
}

/// Produce the Windows 11 24H2 shared-page ABI. Only x87/SSE are enabled
/// in the current kernel policy; unsupported, supervisor, AVX-512, AMX and
/// PKRU state are intentionally not exposed as enabled.
pub fn ki_query_windows_xstate_configuration() -> WindowsXStateConfiguration {
    let internal = ki_query_xstate_configuration();
    let mut out = WindowsXStateConfiguration::zeroed();
    out.enabled_features = internal.enabled_features;
    out.enabled_volatile_features = internal.enabled_features;
    out.size = internal.area_size;
    out.control_flags = (internal.xsaveopt & 1) | ((internal.compaction_enabled & 1) << 1);
    for i in 0..64 {
        out.features[i] = WindowsXStateFeature { offset: internal.feature_offsets[i], size: internal.feature_sizes[i] };
        out.all_features[i] = internal.feature_sizes[i];
    }
    out.all_feature_size = internal.area_size;
    out.all_non_large_feature_size = internal.area_size;
    out
}

pub fn ki_query_xstate_configuration() -> CognacXStateConfiguration {
    let flags = XSTATE_INITIALIZED.load(Ordering::Acquire);
    let size = XSTATE_AREA_SIZE.load(Ordering::Acquire);
    let supported = XSTATE_SUPPORTED_MASK.load(Ordering::Acquire);
    let mut offsets = [0u32; 64];
    let mut sizes = [0u32; 64];
    sizes[0] = 160;
    offsets[1] = 160;
    sizes[1] = 256;
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
        compaction_enabled: 0,
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


/// The currently enabled x87/SSE state occupies the architectural 512-byte
/// legacy region plus the 64-byte XSAVE header. Keep this object 64-byte
/// aligned because XSAVE/XRSTOR require that alignment for standard format.
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct KiThreadXStateArea {
    bytes: [u8; 576],
}
impl KiThreadXStateArea {
    pub const fn zeroed() -> Self { Self { bytes: [0; 576] } }
    pub fn as_mut_ptr(&mut self) -> *mut u8 { self.bytes.as_mut_ptr() }
    pub fn as_ptr(&self) -> *const u8 { self.bytes.as_ptr() }
}
impl Default for KiThreadXStateArea {
    fn default() -> Self { Self::zeroed() }
}

/// Scheduler-only save/restore helpers. They are no-ops before per-CPU XSAVE
/// initialization; callers must not enable AVX until all CPUs and threads use
/// the same fully integrated state policy.
pub unsafe fn ki_xstate_save_thread_mask(area: *mut KiThreadXStateArea, mask: u64) -> bool {
    if area.is_null() || mask == 0 || mask & !XSTATE_ENABLED_MASK.load(Ordering::Acquire) != 0 { return false; }
    unsafe { ki_xstate_save((*area).as_mut_ptr(), core::mem::size_of::<KiThreadXStateArea>(), mask) == STATUS_SUCCESS }
}
pub unsafe fn ki_xstate_restore_thread_mask(area: *const KiThreadXStateArea, mask: u64) -> bool {
    if area.is_null() || mask == 0 || mask & !XSTATE_ENABLED_MASK.load(Ordering::Acquire) != 0 { return false; }
    unsafe { ki_xstate_restore((*area).as_ptr(), core::mem::size_of::<KiThreadXStateArea>(), mask) == STATUS_SUCCESS }
}
pub unsafe fn ki_xstate_save_thread(area: *mut KiThreadXStateArea) -> bool {
    let mask = XSTATE_ENABLED_MASK.load(Ordering::Acquire);
    unsafe { ki_xstate_save_thread_mask(area, mask) }
}
pub unsafe fn ki_xstate_restore_thread(area: *const KiThreadXStateArea) -> bool {
    let mask = XSTATE_ENABLED_MASK.load(Ordering::Acquire);
    unsafe { ki_xstate_restore_thread_mask(area, mask) }
}

/// Deterministic per-CPU XSAVE/XRSTOR smoke check used during processor bring-up.
/// The 64-byte-aligned temporary image is local to the current kernel stack.
pub unsafe fn ki_xstate_self_test_current_cpu() -> bool {
    if !ki_xstate_enabled() { return true; }
    let mut area = KiThreadXStateArea::zeroed();
    if !unsafe { ki_xstate_save_thread(&mut area) } { return false; }
    unsafe { ki_xstate_restore_thread(&area) }
}

/// Exercise nested XSAVE/XRSTOR scopes with separate aligned images on each CPU.
pub unsafe fn ki_xstate_nested_save_restore_self_test() -> bool {
    if !ki_xstate_enabled() { return true; }
    let mut outer = KiThreadXStateArea::zeroed();
    let mut inner = KiThreadXStateArea::zeroed();
    if !unsafe { ki_xstate_save_thread(&mut outer) } { return false; }
    if !unsafe { ki_xstate_save_thread(&mut inner) } {
        let _ = unsafe { ki_xstate_restore_thread(&outer) };
        return false;
    }
    if !unsafe { ki_xstate_restore_thread(&inner) } {
        let _ = unsafe { ki_xstate_restore_thread(&outer) };
        return false;
    }
    unsafe { ki_xstate_restore_thread(&outer) }
}

pub fn ki_xstate_enabled() -> bool {
    XSTATE_INITIALIZED.load(Ordering::Acquire) & 1 != 0
}


const XSTATE_SAVE_MAGIC: u64 = 0x434f_474e_5853_4156; // opaque private signature, not a Windows ABI field
/// Windows x64 WDK declares XSTATE_SAVE opaque. The observed x64 ABI record
/// is 0x38 bytes with 8-byte alignment; no kernel pointers are exposed here.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct XStateSaveRecord { signature: u64, generation: u64, reserved: [u64; 5] }
const _: [(); 0x38] = [(); core::mem::size_of::<XStateSaveRecord>()];
const _: [(); 8] = [(); core::mem::align_of::<XStateSaveRecord>()];

fn valid_xstate_save_record(pointer: *const u8) -> bool {
    let start = pointer as u64;
    let Some(end) = start.checked_add(core::mem::size_of::<XStateSaveRecord>() as u64 - 1) else { return false; };
    if start < 0xffff_8000_0000_0000 || end < start
        || crate::kernel_vm::is_pageable_address(start)
        || crate::kernel_vm::is_pageable_address(end) { return false; }
    let valid = |address: u64| unsafe {
        crate::ntoskrnl::mm::runtime::mm_is_non_paged_system_address_valid(address as *const core::ffi::c_void) != 0
    };
    // Check each page boundary as well as both endpoints before touching the
    // caller-owned opaque record. This safely rejects null, user, pageable,
    // unmapped, wrapping, and cross-gap ranges without speculative dereferences.
    if !valid(start) || !valid(end) { return false; }
    let first_page = start & !0xfff;
    let last_page = end & !0xfff;
    if first_page != last_page && !valid(last_page) { return false; }
    true
}
const XSTATE_SAVE_SLOTS: usize = 128;
const XSTATE_SAVE_NESTING: usize = 16;
const STATUS_INVALID_DEVICE_STATE: i32 = 0xC000_0184u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009Au32 as i32;

#[derive(Clone, Copy)]
struct XStateSaveSlot {
    key: usize,
    owner_thread: u64,
    cpu: usize,
    irql: u8,
    mask: u64,
    generation: u64,
    active: bool,
    area: KiThreadXStateArea,
}
impl XStateSaveSlot {
    const EMPTY: Self = Self { key: 0, owner_thread: 0, cpu: 0, irql: 0, mask: 0,
        generation: 0, active: false, area: KiThreadXStateArea::zeroed() };
}
struct XStateSaveTable {
    slots: [XStateSaveSlot; XSTATE_SAVE_SLOTS],
    owners: [u64; XSTATE_SAVE_SLOTS],
    stack: [[usize; XSTATE_SAVE_NESTING]; XSTATE_SAVE_SLOTS],
    depth: [u8; XSTATE_SAVE_SLOTS],
    generation: u64,
}
impl XStateSaveTable {
    const fn new() -> Self {
        Self { slots: [XStateSaveSlot::EMPTY; XSTATE_SAVE_SLOTS],
            owners: [0; XSTATE_SAVE_SLOTS],
            stack: [[0; XSTATE_SAVE_NESTING]; XSTATE_SAVE_SLOTS],
            depth: [0; XSTATE_SAVE_SLOTS], generation: 1 }
    }
}
static XSTATE_SAVE_TABLE: crate::hal::IrqSpinLock<XStateSaveTable> =
    crate::hal::IrqSpinLock::new(XStateSaveTable::new());

fn current_xstate_execution_context() -> Option<(usize, u64, u8)> {
    let cpu = crate::hal::current_cpu_local()?.logical_id.load(Ordering::Acquire) as usize;
    if cpu >= crate::hal::MAX_CPUS { return None; }
    let thread = crate::thread::current_thread_id()?;
    if thread == 0 { return None; }
    let irql = unsafe { ke_get_current_irql() };
    Some((cpu, thread, irql))
}

/// WDK-compatible entry point for saving processor extended state. XSTATE_SAVE
/// is opaque to drivers; this implementation stores a small signature in its
/// caller-owned record and keeps the aligned XSAVE image in a kernel side table.
/// Calls are bounded and must be properly nested on the same thread and IRQL.
#[no_mangle]
pub unsafe extern "win64" fn KeSaveExtendedProcessorState(mask: u64, save: *mut u8) -> i32 {
    if save.is_null() || (save as usize) & 7 != 0 || mask == 0 || !valid_xstate_save_record(save) {
        return STATUS_INVALID_PARAMETER;
    }
    if !ki_xstate_enabled() { return STATUS_NOT_SUPPORTED; }
    let enabled = XSTATE_ENABLED_MASK.load(Ordering::Acquire);
    if mask & !enabled != 0 { return STATUS_INVALID_PARAMETER; }
    let Some((cpu, owner_thread, irql)) = current_xstate_execution_context() else {
        return STATUS_INVALID_DEVICE_STATE;
    };
    if irql > 2 { return STATUS_INVALID_DEVICE_STATE; }

    let mut table = XSTATE_SAVE_TABLE.lock();
    let Some(owner_index) = table.owners.iter().position(|owner| *owner == owner_thread)
        .or_else(|| table.owners.iter().position(|owner| *owner == 0)) else {
        return STATUS_INSUFFICIENT_RESOURCES;
    };
    table.owners[owner_index] = owner_thread;
    let depth = table.depth[owner_index] as usize;
    if depth >= XSTATE_SAVE_NESTING { return STATUS_INSUFFICIENT_RESOURCES; }
    if table.slots.iter().any(|s| s.active && s.key == save as usize) {
        return STATUS_INVALID_DEVICE_STATE;
    }
    let Some(slot_index) = table.slots.iter().position(|s| !s.active) else {
        if depth == 0 { table.owners[owner_index] = 0; }
        return STATUS_INSUFFICIENT_RESOURCES;
    };
    let generation = table.generation;
    table.generation = table.generation.wrapping_add(1).max(1);
    let slot = &mut table.slots[slot_index];
    slot.key = save as usize;
    slot.owner_thread = owner_thread;
    slot.cpu = cpu;
    slot.irql = irql;
    slot.mask = mask;
    slot.generation = generation;
    slot.active = true;
    if unsafe { !ki_xstate_save_thread_mask(&mut slot.area, mask) } {
        *slot = XStateSaveSlot::EMPTY;
        if depth == 0 { table.owners[owner_index] = 0; }
        return STATUS_NOT_SUPPORTED;
    }
    table.stack[owner_index][depth] = save as usize;
    table.depth[owner_index] = (depth + 1) as u8;
    // The public structure is opaque; do not expose internal kernel pointers.
    unsafe { (save as *mut XStateSaveRecord).write(XStateSaveRecord {
        signature: XSTATE_SAVE_MAGIC, generation, reserved: [0; 5],
    }); }
    STATUS_SUCCESS
}

/// Restore the most recent matching XSTATE_SAVE. Invalid, stale, cross-thread,
/// wrong-IRQL, or out-of-order restores are rejected as no-ops.
#[no_mangle]
pub unsafe extern "win64" fn KeRestoreExtendedProcessorState(save: *mut u8) {
    if save.is_null() || (save as usize) & 7 != 0 || !valid_xstate_save_record(save) || !ki_xstate_enabled() { return; }
    let record = unsafe { (save as *const XStateSaveRecord).read() };
    let generation = record.generation;
    if record.signature != XSTATE_SAVE_MAGIC { return; }
    let Some((_cpu, owner_thread, irql)) = current_xstate_execution_context() else { return; };
    let mut restored_area = KiThreadXStateArea::zeroed();
    let saved_mask;
    {
        let mut table = XSTATE_SAVE_TABLE.lock();
        let Some(owner_index) = table.owners.iter().position(|owner| *owner == owner_thread) else { return; };
        let depth = table.depth[owner_index] as usize;
        if depth == 0 || table.stack[owner_index][depth - 1] != save as usize { return; }
        let Some(index) = table.slots.iter().position(|s| s.active && s.key == save as usize
            && s.generation == generation && s.owner_thread == owner_thread && s.irql == irql) else { return; };
        saved_mask = table.slots[index].mask;
        restored_area = table.slots[index].area;
        table.depth[owner_index] = (depth - 1) as u8;
        table.stack[owner_index][depth - 1] = 0;
        if table.depth[owner_index] == 0 { table.owners[owner_index] = 0; }
        table.slots[index] = XStateSaveSlot::EMPTY;
        unsafe { (save as *mut XStateSaveRecord).write(XStateSaveRecord { signature: 0, generation: 0, reserved: [0; 5] }); }
    }
    // Restore after releasing the global bookkeeping lock so no lock-release
    // code runs between XRSTOR and the return to the caller.
    unsafe { let _ = ki_xstate_restore_thread_mask(&restored_area, saved_mask); }
}


#[cfg(test)]
mod xstate_layout_tests {
    use super::*;
    #[test]
    fn thread_xstate_storage_has_architectural_alignment_and_capacity() {
        assert_eq!(core::mem::align_of::<KiThreadXStateArea>(), 64);
        assert_eq!(core::mem::size_of::<KiThreadXStateArea>(), 576);
    }
    #[test]
    fn processor_feature_masks_must_match_before_migration() {
        assert!(xstate_policy_compatible(3, 3, 3, 576, 576));
        assert!(!xstate_policy_compatible(3, 1, 3, 576, 576));
        assert!(!xstate_policy_compatible(3, 7, 7, 832, 576));
        assert!(!xstate_policy_compatible(3, 3, 3, 512, 576));
        assert!(!xstate_policy_compatible(0, 3, 3, 576, 576));
    }
    #[test]
    fn wdk_xstate_save_record_has_x64_opaque_size_and_alignment() {
        assert_eq!(core::mem::size_of::<XStateSaveRecord>(), 0x38);
        assert_eq!(core::mem::align_of::<XStateSaveRecord>(), 8);
    }
    #[test]
    fn windows_shared_xstate_configuration_matches_24h2_abi_layout() {
        assert_eq!(core::mem::size_of::<WindowsXStateConfiguration>(), 0x348);
        assert_eq!(core::mem::offset_of!(WindowsXStateConfiguration, features), 0x18);
        assert_eq!(core::mem::offset_of!(WindowsXStateConfiguration, enabled_user_visible_supervisor_features), 0x330);
    }
    #[test]
    fn invalid_save_record_addresses_are_rejected_before_dereference() {
        assert!(!valid_xstate_save_record(core::ptr::null()));
        assert!(!valid_xstate_save_record(0x7ffe_0000usize as *const u8));
        assert!(!valid_xstate_save_record(0xffff_ffff_ffff_f000usize as *const u8));
    }
    #[test]
    fn public_configuration_feature_arrays_have_stable_capacity() {
        let config = CognacXStateConfiguration::default();
        assert_eq!(config.feature_offsets.len(), 64);
        assert_eq!(config.feature_sizes.len(), 64);
    }
    #[test]
    fn save_table_capacity_supports_nested_driver_state_scopes() {
        assert_eq!(XSTATE_SAVE_NESTING, 16);
        assert!(XSTATE_SAVE_SLOTS >= XSTATE_SAVE_NESTING);
    }
}

//! xAPIC/I/O APIC register primitives. Interrupt delivery remains disabled until
//! IDT vectors, timer calibration, and all required routes are installed.
use core::sync::atomic::{compiler_fence, Ordering};
const IA32_APIC_BASE: u32 = 0x1b;
const APIC_ENABLE: u64 = 1 << 11;
const X2APIC_ENABLE: u64 = 1 << 10;
const APIC_ID: usize = 0x020;
const APIC_EOI: usize = 0x0b0;
const APIC_SVR: usize = 0x0f0;
const APIC_LVT_TIMER: usize = 0x320;
const APIC_LVT_ERROR: usize = 0x370;
const APIC_SPURIOUS_VECTOR: u32 = 0x100 | 0xff;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApicError { Unsupported, X2ApicActive, InvalidBase, InvalidVector, InvalidGsi, Timeout }
#[inline] unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32; let high: u32;
    unsafe { core::arch::asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nomem, nostack)); }
    ((high as u64) << 32) | low as u64
}
#[inline] unsafe fn wrmsr(msr: u32, value: u64) {
    unsafe { core::arch::asm!("wrmsr", in("ecx") msr, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nomem, nostack)); }
}
/// Enable xAPIC mode and its spurious vector. Requires CPUID APIC support and an
/// identity-mapped, uncached MMIO mapping for the physical APIC page.
pub unsafe fn initialize_local_apic(mmio_base: u64) -> Result<u32, ApicError> {
    if mmio_base == 0 || mmio_base & 0xfff != 0 || mmio_base >= 0x1_0000_0000 { return Err(ApicError::InvalidBase); }
    let max = unsafe { core::arch::x86_64::__cpuid(1) };
    if max.edx & (1 << 9) == 0 { return Err(ApicError::Unsupported); }
    let current = unsafe { rdmsr(IA32_APIC_BASE) };
    if current & X2APIC_ENABLE != 0 { return Err(ApicError::X2ApicActive); }
    unsafe { wrmsr(IA32_APIC_BASE, (current & !0x000f_ffff_ffff_f000) | mmio_base | APIC_ENABLE); }
    let id = unsafe { read_local(mmio_base, APIC_ID) } >> 24;
    unsafe { write_local(mmio_base, APIC_LVT_ERROR, 0xFE); }
    unsafe { write_local(mmio_base, APIC_SVR, APIC_SPURIOUS_VECTOR); }
    Ok(id)
}
#[inline] unsafe fn read_local(base: u64, offset: usize) -> u32 {
    compiler_fence(Ordering::SeqCst);
    let value = unsafe { core::ptr::read_volatile((base + offset as u64) as *const u32) };
    compiler_fence(Ordering::SeqCst); value
}
#[inline] unsafe fn write_local(base: u64, offset: usize, value: u32) {
    compiler_fence(Ordering::SeqCst);
    unsafe { core::ptr::write_volatile((base + offset as u64) as *mut u32, value); }
    compiler_fence(Ordering::SeqCst);
}
pub unsafe fn local_apic_eoi(mmio_base: u64) { unsafe { write_local(mmio_base, APIC_EOI, 0); } }
/// Configure one I/O APIC redirection entry masked. Does not unmask the IRQ:
/// callers must install a matching IDT handler and confirm the GSI belongs here.
pub unsafe fn configure_ioapic_masked(io_base: u64, gsi_base: u32, gsi: u32, vector: u8, destination_apic_id: u8) -> Result<(), ApicError> {
    if io_base == 0 || io_base & 0xfff != 0 || vector < 0x20 || vector == 0xff { return Err(ApicError::InvalidVector); }
    if gsi < gsi_base || gsi - gsi_base >= 120 { return Err(ApicError::InvalidGsi); }
    unsafe fn select(base: u64, reg: u8) {
        unsafe { core::ptr::write_volatile(base as *mut u32, reg as u32); }
    }
    unsafe fn read(base: u64) -> u32 { unsafe { core::ptr::read_volatile((base + 0x10) as *const u32) } }
    unsafe fn write(base: u64, value: u32) { unsafe { core::ptr::write_volatile((base + 0x10) as *mut u32, value); } }
    let index = (gsi - gsi_base) as u8;
    let low_reg = 0x10u8.wrapping_add(index.wrapping_mul(2));
    unsafe { select(io_base, 0x01); }
    let version = unsafe { read(io_base) };
    let max_pin = (version >> 16) & 0xff;
    if index as u32 > max_pin { return Err(ApicError::InvalidGsi); }
    unsafe { select(io_base, low_reg); write(io_base, (1 << 16) | vector as u32); }
    unsafe { select(io_base, low_reg.wrapping_add(1)); write(io_base, (destination_apic_id as u32) << 24); }
    Ok(())
}
/// Timer remains masked; calibration and scheduler integration are separate steps.
pub unsafe fn mask_local_timer(mmio_base: u64) { unsafe { write_local(mmio_base, APIC_LVT_TIMER, 1 << 16); } }

//! Minimal per-process Windows x64 PEB/TEB and TLS storage.
//! Loader lists, process parameters, and full NT object semantics remain future work.
use crate::vm::{AddressSpace, VmError};
const PAGE: u64 = 4096;
const USER_START: u64 = 0x0000_0080_0000_0000;
pub const PEB_ADDRESS: u64 = 0x0000_0080_1000_0000;
pub const TEB_ADDRESS: u64 = 0x0000_0080_2000_0000;
pub const TLS_VECTOR_ADDRESS: u64 = 0x0000_0080_3000_0000;
static NEXT_PID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(4);
#[derive(Clone, Copy, Debug)]
pub enum ProcessError { Vm(VmError), OutOfMemory, InvalidLayout }
impl From<VmError> for ProcessError { fn from(e: VmError) -> Self { Self::Vm(e) } }
#[derive(Clone, Copy, Debug)]
pub struct ProcessEnvironment { pub process_id: u64, pub peb: u64, pub teb: u64, pub tls_vector: u64 }
fn map_zeroed(space: &mut AddressSpace, base: u64, pages: usize) -> Result<(), ProcessError> {
    for i in 0..pages {
        let physical = unsafe { crate::memory::allocate_page() }.map_err(|_| ProcessError::OutOfMemory)?;
        unsafe { space.map_user_page(base + i as u64 * PAGE, physical, true, false)?; }
    }
    Ok(())
}
fn put_u64(space: &AddressSpace, address: u64, value: u64) -> Result<(), ProcessError> {
    unsafe { space.copy_to_user(address, &value.to_le_bytes())?; } Ok(())
}
fn put_u8(space: &AddressSpace, address: u64, value: u8) -> Result<(), ProcessError> {
    unsafe { space.copy_to_user(address, &[value])?; } Ok(())
}
/// Initialize stable x64 PEB/TEB fields in a fresh address space.
pub unsafe fn initialize(space: &mut AddressSpace, image_base: u64, stack_base: u64, stack_limit: u64)
    -> Result<ProcessEnvironment, ProcessError> {
    if image_base == 0 || stack_base <= stack_limit || stack_limit < USER_START {
        return Err(ProcessError::InvalidLayout);
    }
    map_zeroed(space, PEB_ADDRESS, 1)?;
    map_zeroed(space, TEB_ADDRESS, 2)?;
    map_zeroed(space, TLS_VECTOR_ADDRESS, 1)?;
    let pid = NEXT_PID.fetch_add(4, core::sync::atomic::Ordering::Relaxed);
    // PEB x64: BeingDebugged, ImageBaseAddress, Ldr, ProcessParameters.
    put_u8(space, PEB_ADDRESS + 2, 0)?;
    put_u64(space, PEB_ADDRESS + 0x10, image_base)?;
    put_u64(space, PEB_ADDRESS + 0x18, 0)?;
    put_u64(space, PEB_ADDRESS + 0x20, 0)?;
    // NT_TIB-compatible TEB prefix; GS base must point here in user mode.
    put_u64(space, TEB_ADDRESS + 0x08, stack_base)?;
    put_u64(space, TEB_ADDRESS + 0x10, stack_limit)?;
    put_u64(space, TEB_ADDRESS + 0x30, TEB_ADDRESS)?;
    put_u64(space, TEB_ADDRESS + 0x40, pid)?;
    put_u64(space, TEB_ADDRESS + 0x48, 1)?;
    put_u64(space, TEB_ADDRESS + 0x58, TLS_VECTOR_ADDRESS)?;
    put_u64(space, TEB_ADDRESS + 0x60, PEB_ADDRESS)?;
    // The x64 TLS slots begin at offset 0x1480, so TEB occupies two pages.
    put_u64(space, TEB_ADDRESS + 0x1480, 0)?;
    Ok(ProcessEnvironment { process_id: pid, peb: PEB_ADDRESS, teb: TEB_ADDRESS, tls_vector: TLS_VECTOR_ADDRESS })
}

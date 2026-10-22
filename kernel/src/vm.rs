//! First per-address-space VM primitives for user-mode isolation.
//! User mappings occupy PML4 slots 1..255 (512 GiB..128 TiB) so they do not
//! collide with the bootstrap's supervisor-only identity map at slot 0.
use core::arch::asm;

const PAGE_SIZE: u64 = 4096;
const PRESENT: u64 = 1;
const WRITE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const LARGE: u64 = 1 << 7;
const NX: u64 = 1 << 63;
const ADDR: u64 = 0x000f_ffff_ffff_f000;
const USER_START: u64 = 0x0000_0080_0000_0000;
const USER_END: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy, Debug)]
pub enum VmError { InvalidAddress, Unaligned, OutOfMemory, AlreadyMapped, NotMapped, PermissionDenied, ExecutableUserPageUnsupported }

pub struct AddressSpace { root: u64 }
impl AddressSpace {
    /// Allocate an independent PML4 while sharing supervisor-only kernel mappings.
    ///
    /// # Safety
    /// kernel_root must be the active kernel PML4 and identity-map the physical
    /// memory returned by the early allocator.
    pub unsafe fn new(kernel_root: u64) -> Result<Self, VmError> {
        let root = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
        let source = kernel_root as *const u64;
        let target = root as *mut u64;
        for i in 0..512usize {
            let mut entry = unsafe { source.add(i).read() };
            // Never inherit user permission from a kernel top-level mapping.
            entry &= !USER;
            unsafe { target.add(i).write(entry); }
        }
        Ok(Self { root })
    }

    pub fn root_physical(&self) -> u64 { self.root }

    /// Map a user page. Executable user mappings are deliberately rejected until
    /// per-physical-frame W^X accounting and executable-page transitions exist.
    pub unsafe fn map_user_page(&mut self, virtual_address: u64, physical_address: u64,
                                writable: bool, executable: bool) -> Result<(), VmError> {
        if !is_user_range(virtual_address, PAGE_SIZE) { return Err(VmError::InvalidAddress); }
        if virtual_address & (PAGE_SIZE - 1) != 0 || physical_address & (PAGE_SIZE - 1) != 0 {
            return Err(VmError::Unaligned);
        }
        if physical_address >= 0x1_0000_0000 { return Err(VmError::InvalidAddress); }
        if executable { return Err(VmError::ExecutableUserPageUnsupported); }

        let indexes = [
            ((virtual_address >> 39) & 0x1ff) as usize,
            ((virtual_address >> 30) & 0x1ff) as usize,
            ((virtual_address >> 21) & 0x1ff) as usize,
            ((virtual_address >> 12) & 0x1ff) as usize,
        ];
        let mut table = self.root as *mut u64;
        for index in indexes[..3].iter().copied() {
            let slot = unsafe { table.add(index) };
            let mut entry = unsafe { slot.read() };
            if entry & PRESENT == 0 {
                let page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
                entry = page | PRESENT | WRITE | USER;
                unsafe { slot.write(entry); }
            } else {
                if entry & LARGE != 0 { return Err(VmError::InvalidAddress); }
                entry |= USER;
                unsafe { slot.write(entry); }
            }
            table = (entry & ADDR) as *mut u64;
        }
        let leaf = unsafe { table.add(indexes[3]) };
        if unsafe { leaf.read() } & PRESENT != 0 { return Err(VmError::AlreadyMapped); }
        let mut flags = PRESENT | USER | NX;
        if writable { flags |= WRITE; }
        unsafe { leaf.write(physical_address | flags); }
        Ok(())
    }

    pub fn translate_user(&self, virtual_address: u64, write: bool) -> Result<u64, VmError> {
        if !is_user_range(virtual_address, 1) { return Err(VmError::InvalidAddress); }
        let indexes = [
            ((virtual_address >> 39) & 0x1ff) as usize,
            ((virtual_address >> 30) & 0x1ff) as usize,
            ((virtual_address >> 21) & 0x1ff) as usize,
            ((virtual_address >> 12) & 0x1ff) as usize,
        ];
        let mut table = self.root as *const u64;
        for (level, index) in indexes.iter().copied().enumerate() {
            let entry = unsafe { table.add(index).read() };
            if entry & PRESENT == 0 { return Err(VmError::NotMapped); }
            if entry & USER == 0 { return Err(VmError::PermissionDenied); }
            if level == 3 {
                if write && entry & WRITE == 0 { return Err(VmError::PermissionDenied); }
                return Ok((entry & ADDR) | (virtual_address & (PAGE_SIZE - 1)));
            }
            if entry & LARGE != 0 { return Err(VmError::InvalidAddress); }
            table = (entry & ADDR) as *const u64;
        }
        Err(VmError::NotMapped)
    }

    /// Copy through validated page-table translations using the kernel's
    /// supervisor identity map. Each page is permission-checked before access.
    pub unsafe fn copy_from_user(&self, source: u64, destination: &mut [u8]) -> Result<(), VmError> {
        let mut done = 0usize;
        while done < destination.len() {
            let va = source.checked_add(done as u64).ok_or(VmError::InvalidAddress)?;
            let pa = self.translate_user(va, false)?;
            let count = core::cmp::min(destination.len() - done, PAGE_SIZE as usize - (va as usize & 4095));
            unsafe { core::ptr::copy_nonoverlapping(pa as *const u8, destination.as_mut_ptr().add(done), count); }
            done += count;
        }
        Ok(())
    }

    pub unsafe fn copy_to_user(&self, destination: u64, source: &[u8]) -> Result<(), VmError> {
        let mut done = 0usize;
        while done < source.len() {
            let va = destination.checked_add(done as u64).ok_or(VmError::InvalidAddress)?;
            let pa = self.translate_user(va, true)?;
            let count = core::cmp::min(source.len() - done, PAGE_SIZE as usize - (va as usize & 4095));
            unsafe { core::ptr::copy_nonoverlapping(source.as_ptr().add(done), pa as *mut u8, count); }
            done += count;
        }
        Ok(())
    }

    /// Switch to this address space. The kernel identity map is shared so the
    /// current instruction, stack, IDT, and physical allocator remain reachable.
    pub unsafe fn activate(&self) {
        unsafe { asm!("mov cr3, {}", in(reg) self.root, options(nostack, preserves_flags)); }
    }
}

pub fn is_user_range(address: u64, length: u64) -> bool {
    if length == 0 { return address >= USER_START && address < USER_END; }
    address >= USER_START
        && address < USER_END
        && address.checked_add(length).is_some_and(|end| end <= USER_END)
}

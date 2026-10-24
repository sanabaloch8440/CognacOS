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

/// Serializes address-space mutations/destruction with cross-process virtual copies.
pub static ADDRESS_SPACE_LIFETIME_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

#[derive(Clone, Copy, Debug)]
pub enum VmError { InvalidAddress, Unaligned, OutOfMemory, AlreadyMapped, NotMapped, PermissionDenied, AddressSpaceBusy, ExecutableUserPageUnsupported }

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

    /// Map one user page. Writable+executable mappings are forbidden; loaders
    /// populate pages RW/NX and then transition them to final RX/RO/RW-NX permissions.
    pub unsafe fn map_user_page(&mut self, virtual_address: u64, physical_address: u64,
                                writable: bool, executable: bool) -> Result<(), VmError> {
        let _address_space_guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
        if !is_user_range(virtual_address, PAGE_SIZE) { return Err(VmError::InvalidAddress); }
        if virtual_address & (PAGE_SIZE - 1) != 0 || physical_address & (PAGE_SIZE - 1) != 0 {
            return Err(VmError::Unaligned);
        }
        if physical_address >= 0x1_0000_0000 { return Err(VmError::InvalidAddress); }
        if writable && executable { return Err(VmError::PermissionDenied); }

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
        let mut flags = PRESENT | USER;
        if writable { flags |= WRITE; }
        if !executable { flags |= NX; }
        unsafe {
            leaf.write(physical_address | flags);
            asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
        }
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

    /// Verify that a user address resolves through present user-accessible
    /// entries and has no NX bit set at any paging level.
    pub fn is_user_executable(&self, virtual_address: u64) -> bool {
        if !is_user_range(virtual_address, 1) { return false; }
        let indexes = [
            ((virtual_address >> 39) & 0x1ff) as usize,
            ((virtual_address >> 30) & 0x1ff) as usize,
            ((virtual_address >> 21) & 0x1ff) as usize,
            ((virtual_address >> 12) & 0x1ff) as usize,
        ];
        let mut table = self.root as *const u64;
        for (level, index) in indexes.iter().copied().enumerate() {
            let entry = unsafe { table.add(index).read() };
            if entry & PRESENT == 0 || entry & USER == 0 || entry & NX != 0 { return false; }
            if level == 3 { return true; }
            if entry & LARGE != 0 { return false; }
            table = (entry & ADDR) as *const u64;
        }
        false
    }

    /// Copy through validated page-table translations using the kernel's
    /// supervisor identity map. Each page is permission-checked before access.
    pub unsafe fn copy_from_user(&self, source: u64, destination: &mut [u8]) -> Result<(), VmError> {
        let _address_space_guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
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
        let _address_space_guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
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

    /// Return the physical address backing a validated user virtual address.
    pub fn physical_for_user(&self, virtual_address: u64) -> Result<u64, VmError> {
        self.translate_user(virtual_address, false)
    }

    /// Change final page permissions after a loader has populated and relocated it.
    /// W+X is rejected at this boundary as well as during initial mapping.
    pub unsafe fn protect_user_page(&mut self, virtual_address: u64, writable: bool, executable: bool) -> Result<(), VmError> {
        let _address_space_guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
        if !is_user_range(virtual_address, PAGE_SIZE) || virtual_address & (PAGE_SIZE - 1) != 0 {
            return Err(VmError::InvalidAddress);
        }
        if writable && executable { return Err(VmError::PermissionDenied); }
        let indexes = [
            ((virtual_address >> 39) & 0x1ff) as usize,
            ((virtual_address >> 30) & 0x1ff) as usize,
            ((virtual_address >> 21) & 0x1ff) as usize,
            ((virtual_address >> 12) & 0x1ff) as usize,
        ];
        let mut table = self.root as *mut u64;
        for index in indexes[..3].iter().copied() {
            let entry = unsafe { table.add(index).read() };
            if entry & PRESENT == 0 || entry & LARGE != 0 { return Err(VmError::NotMapped); }
            table = (entry & ADDR) as *mut u64;
        }
        let leaf = unsafe { table.add(indexes[3]) };
        let mut entry = unsafe { leaf.read() };
        if entry & PRESENT == 0 || entry & USER == 0 { return Err(VmError::NotMapped); }
        if writable { entry |= WRITE; } else { entry &= !WRITE; }
        if executable { entry &= !NX; } else { entry |= NX; }
        unsafe {
            leaf.write(entry);
            asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
        }
        Ok(())
    }

    /// Destroy this address space after switching CR3 away from it. Frees user
    /// leaves, user page-table pages, and the PML4 root; shared supervisor mappings
    /// are never traversed or reclaimed.
    pub unsafe fn destroy(&mut self) -> Result<(), VmError> {
        let _address_space_guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
        if crate::process::address_space_has_references(self.root) { return Err(VmError::AddressSpaceBusy); }
        let root = self.root as *mut u64;
        // Preflight all user leaves before freeing any frame. An MDL-pinned
        // page must keep its address space intact until MmUnlockPages runs.
        for pml4_index in 1..256usize {
            let top_entry = unsafe { root.add(pml4_index).read() };
            if top_entry & (PRESENT | USER) != (PRESENT | USER) { continue; }
            let pdpt = (top_entry & ADDR) as *const u64;
            for a in 0..512usize {
                let e3 = unsafe { pdpt.add(a).read() };
                if e3 & PRESENT == 0 { continue; }
                if e3 & LARGE != 0 { return Err(VmError::InvalidAddress); }
                let pd = (e3 & ADDR) as *const u64;
                for b in 0..512usize {
                    let e2 = unsafe { pd.add(b).read() };
                    if e2 & PRESENT == 0 { continue; }
                    if e2 & LARGE != 0 { return Err(VmError::InvalidAddress); }
                    let pt = (e2 & ADDR) as *const u64;
                    for d in 0..512usize {
                        let pte = unsafe { pt.add(d).read() };
                        if pte & (PRESENT | USER) == (PRESENT | USER)
                            && (pte & EXTERNAL_USER != 0 || crate::ntoskrnl::mm::runtime::is_page_locked(pte & ADDR)) {
                            return Err(VmError::PermissionDenied);
                        }
                    }
                }
            }
        }
        for pml4_index in 1..256usize {
            let top = unsafe { root.add(pml4_index) };
            let top_entry = unsafe { top.read() };
            if top_entry & (PRESENT | USER) != (PRESENT | USER) { continue; }
            let pdpt = (top_entry & ADDR) as *mut u64;
            for a in 0..512usize {
                let pdpt_slot = unsafe { pdpt.add(a) };
                let pdpt_entry = unsafe { pdpt_slot.read() };
                if pdpt_entry & PRESENT == 0 { continue; }
                if pdpt_entry & LARGE != 0 { return Err(VmError::InvalidAddress); }
                let pd = (pdpt_entry & ADDR) as *mut u64;
                for b in 0..512usize {
                    let pd_slot = unsafe { pd.add(b) };
                    let pd_entry = unsafe { pd_slot.read() };
                    if pd_entry & PRESENT == 0 { continue; }
                    if pd_entry & LARGE != 0 { return Err(VmError::InvalidAddress); }
                    let pt = (pd_entry & ADDR) as *mut u64;
                    for d in 0..512usize {
                        let pte_slot = unsafe { pt.add(d) };
                        let pte = unsafe { pte_slot.read() };
                        if pte & (PRESENT | USER) == (PRESENT | USER) {
                            if pte & EXTERNAL_USER == 0 {
                                unsafe { crate::memory::free_page(pte & ADDR) }.map_err(|_| VmError::OutOfMemory)?;
                            }
                        }
                    }
                    unsafe { crate::memory::free_page(pt as u64) }.map_err(|_| VmError::OutOfMemory)?;
                    unsafe { pd_slot.write(0); }
                }
                unsafe { crate::memory::free_page(pd as u64) }.map_err(|_| VmError::OutOfMemory)?;
                unsafe { pdpt_slot.write(0); }
            }
            unsafe { crate::memory::free_page(pdpt as u64) }.map_err(|_| VmError::OutOfMemory)?;
            unsafe { top.write(0); }
        }
        crate::process::detach_address_space(self.root);
        unsafe { crate::memory::free_page(self.root) }.map_err(|_| VmError::OutOfMemory)?;
        self.root = 0;
        Ok(())
    }

    /// Switch to this address space. The kernel identity map is shared so the
    /// current instruction, stack, IDT, and physical allocator remain reachable.
    pub unsafe fn activate(&self) {
        unsafe { asm!("mov cr3, {}", in(reg) self.root, options(nostack, preserves_flags)); }
    }
}



const EXTERNAL_USER: u64 = 1 << 9;
const MDL_USER_BASE: u64 = 0x0000_7000_0000_0000;
const MDL_USER_LIMIT: u64 = 0x0000_7000_1000_0000;
pub fn active_root_physical() -> u64 {
    let cr3: u64;
    unsafe { asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)); }
    cr3 & ADDR
}
pub unsafe fn map_external_user_pages_active(pfns: &[u64], writable: bool) -> Result<u64, VmError> {
    if pfns.is_empty() || pfns.len() > 8192 { return Err(VmError::InvalidAddress); }
    let _guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
    let root_phys = active_root_physical();
    if root_phys == 0 { return Err(VmError::InvalidAddress); }
    let root = root_phys as *mut u64;
    let bytes = (pfns.len() as u64).checked_mul(PAGE_SIZE).ok_or(VmError::InvalidAddress)?;
    let limit = MDL_USER_LIMIT.checked_sub(bytes).ok_or(VmError::InvalidAddress)?;
    let mut candidate = MDL_USER_BASE;
    while candidate <= limit {
        let mut free = true;
        let mut off = 0u64;
        while off < bytes {
            if translate_in_root_locked(root_phys, candidate + off, false, true).is_some() { free = false; break; }
            off += PAGE_SIZE;
        }
        if free {
            let mut mapped = 0usize;
            for (index, pfn) in pfns.iter().copied().enumerate() {
                let Some(pa) = pfn.checked_mul(PAGE_SIZE) else { break; };
                if !crate::memory::is_ram_physical_address(pa) { break; }
                let va = candidate + index as u64 * PAGE_SIZE;
                let indexes = [
                    ((va >> 39) & 0x1ff) as usize, ((va >> 30) & 0x1ff) as usize,
                    ((va >> 21) & 0x1ff) as usize, ((va >> 12) & 0x1ff) as usize,
                ];
                let mut table = root;
                let mut failed = false;
                for idx in indexes[..3].iter().copied() {
                    let slot = unsafe { table.add(idx) };
                    let mut entry = unsafe { slot.read() };
                    if entry & PRESENT == 0 {
                        let page = match unsafe { crate::memory::allocate_page() } {
                            Ok(page) => page, Err(_) => { failed = true; break; }
                        };
                        entry = page | PRESENT | WRITE | USER;
                        unsafe { slot.write(entry); }
                    } else {
                        if entry & LARGE != 0 || entry & USER == 0 { failed = true; break; }
                        entry |= WRITE | USER;
                        unsafe { slot.write(entry); }
                    }
                    table = (entry & ADDR) as *mut u64;
                }
                if failed { break; }
                let leaf = unsafe { table.add(indexes[3]) };
                if unsafe { leaf.read() } & PRESENT != 0 { break; }
                let mut flags = PRESENT | USER | EXTERNAL_USER | NX;
                if writable { flags |= WRITE; }
                unsafe { leaf.write(pa | flags); asm!("invlpg [{}]", in(reg) va, options(nostack, preserves_flags)); }
                mapped += 1;
            }
            if mapped == pfns.len() { return Ok(candidate); }
            for index in 0..mapped {
                let va = candidate + index as u64 * PAGE_SIZE;
                if let Some((leaf, _)) = unsafe { external_user_leaf(root, va) } {
                    unsafe { leaf.write(0); asm!("invlpg [{}]", in(reg) va, options(nostack, preserves_flags)); }
                }
            }
            return Err(VmError::OutOfMemory);
        }
        candidate = candidate.checked_add(PAGE_SIZE).ok_or(VmError::InvalidAddress)?;
    }
    Err(VmError::OutOfMemory)
}
unsafe fn external_user_leaf(root: *mut u64, va: u64) -> Option<(*mut u64, u64)> {
    if !is_user_range(va, 1) { return None; }
    let indexes = [
        ((va >> 39) & 0x1ff) as usize, ((va >> 30) & 0x1ff) as usize,
        ((va >> 21) & 0x1ff) as usize, ((va >> 12) & 0x1ff) as usize,
    ];
    let mut table = root;
    for idx in indexes[..3].iter().copied() {
        let entry = unsafe { table.add(idx).read() };
        if entry & PRESENT == 0 || entry & USER == 0 || entry & LARGE != 0 { return None; }
        table = (entry & ADDR) as *mut u64;
    }
    let leaf = unsafe { table.add(indexes[3]) };
    let entry = unsafe { leaf.read() };
    if entry & (PRESENT | USER | EXTERNAL_USER) != (PRESENT | USER | EXTERNAL_USER) { return None; }
    Some((leaf, entry & ADDR))
}
pub unsafe fn unmap_external_user_pages_active(base: u64, pfns: &[u64]) -> Result<(), VmError> {
    if base & (PAGE_SIZE - 1) != 0 || pfns.is_empty() { return Err(VmError::InvalidAddress); }
    let _guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
    let root_phys = active_root_physical();
    if root_phys == 0 { return Err(VmError::InvalidAddress); }
    let root = root_phys as *mut u64;
    for (index, pfn) in pfns.iter().copied().enumerate() {
        let va = base.checked_add(index as u64 * PAGE_SIZE).ok_or(VmError::InvalidAddress)?;
        let Some((_, pa)) = (unsafe { external_user_leaf(root, va) }) else { return Err(VmError::NotMapped); };
        if pa / PAGE_SIZE != pfn { return Err(VmError::PermissionDenied); }
    }
    for index in 0..pfns.len() {
        let va = base + index as u64 * PAGE_SIZE;
        let Some((leaf, _)) = (unsafe { external_user_leaf(root, va) }) else { return Err(VmError::NotMapped); };
        unsafe { leaf.write(0); asm!("invlpg [{}]", in(reg) va, options(nostack, preserves_flags)); }
    }
    Ok(())
}

/// Translate an address through a specific process root. The caller must hold
/// ADDRESS_SPACE_LIFETIME_LOCK so page tables cannot be mutated or destroyed
/// during the walk and subsequent physical-memory access.
pub fn translate_in_root_locked(root_physical: u64, virtual_address: u64, write: bool, user_only: bool) -> Option<u64> {
    if root_physical == 0 { return None; }
    let high = virtual_address >> 48;
    let sign = (virtual_address >> 47) & 1;
    if (sign == 0 && high != 0) || (sign == 1 && high != 0xffff) { return None; }
    let user_address = is_user_range(virtual_address, 1);
    if user_only && !user_address { return None; }
    if !user_address && virtual_address < 0xffff_8000_0000_0000 { return None; }
    let indexes = [
        ((virtual_address >> 39) & 0x1ff) as usize,
        ((virtual_address >> 30) & 0x1ff) as usize,
        ((virtual_address >> 21) & 0x1ff) as usize,
        ((virtual_address >> 12) & 0x1ff) as usize,
    ];
    let mut table = (root_physical & ADDR) as *const u64;
    for (level, index) in indexes.iter().copied().enumerate() {
        let entry = unsafe { table.add(index).read_volatile() };
        if entry & PRESENT == 0 || (write && entry & WRITE == 0) { return None; }
        if user_address && entry & USER == 0 { return None; }
        if !user_address && entry & USER != 0 { return None; }
        if level == 3 {
            return Some((entry & ADDR) | (virtual_address & (PAGE_SIZE - 1)));
        }
        if entry & LARGE != 0 { return None; }
        table = (entry & ADDR) as *const u64;
    }
    None
}

pub fn is_user_range(address: u64, length: u64) -> bool {
    if length == 0 { return address >= USER_START && address < USER_END; }
    address >= USER_START
        && address < USER_END
        && address.checked_add(length).is_some_and(|end| end <= USER_END)
}


fn translate_user_root(root_physical: u64, virtual_address: u64, write: bool) -> Result<u64, VmError> {
    if root_physical == 0 || !is_user_range(virtual_address, 1) { return Err(VmError::InvalidAddress); }
    let indexes = [
        ((virtual_address >> 39) & 0x1ff) as usize,
        ((virtual_address >> 30) & 0x1ff) as usize,
        ((virtual_address >> 21) & 0x1ff) as usize,
        ((virtual_address >> 12) & 0x1ff) as usize,
    ];
    let mut table = (root_physical & ADDR) as *const u64;
    for (level, index) in indexes.iter().copied().enumerate() {
        let entry = unsafe { table.add(index).read_volatile() };
        if entry & PRESENT == 0 || entry & USER == 0 { return Err(VmError::NotMapped); }
        if write && entry & WRITE == 0 { return Err(VmError::PermissionDenied); }
        if level == 3 { return Ok((entry & ADDR) | (virtual_address & (PAGE_SIZE - 1))); }
        if entry & LARGE != 0 { return Err(VmError::NotMapped); }
        table = (entry & ADDR) as *const u64;
    }
    Err(VmError::NotMapped)
}

/// Copy user bytes through the supplied process page tables. Page translation and
/// the copy are serialized against address-space destruction/unmapping; arbitrary
/// user virtual addresses are never dereferenced directly.
pub unsafe fn copy_from_user_root(root_physical: u64, source: u64, destination: &mut [u8]) -> Result<(), VmError> {
    if destination.is_empty() { return Ok(()); }
    if root_physical == 0 || !is_user_range(source, destination.len() as u64) { return Err(VmError::InvalidAddress); }
    let _guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
    let mut done = 0usize;
    while done < destination.len() {
        let va = source.checked_add(done as u64).ok_or(VmError::InvalidAddress)?;
        let pa = translate_user_root(root_physical, va, false)?;
        let count = core::cmp::min(destination.len() - done, PAGE_SIZE as usize - (va as usize & 4095));
        unsafe { core::ptr::copy_nonoverlapping(pa as *const u8, destination.as_mut_ptr().add(done), count); }
        done += count;
    }
    Ok(())
}

/// Copy kernel bytes to a user buffer after writable user-page validation.
pub unsafe fn copy_to_user_root(root_physical: u64, destination: u64, source: &[u8]) -> Result<(), VmError> {
    if source.is_empty() { return Ok(()); }
    if root_physical == 0 || !is_user_range(destination, source.len() as u64) { return Err(VmError::InvalidAddress); }
    let _guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
    let mut done = 0usize;
    while done < source.len() {
        let va = destination.checked_add(done as u64).ok_or(VmError::InvalidAddress)?;
        let pa = translate_user_root(root_physical, va, true)?;
        let count = core::cmp::min(source.len() - done, PAGE_SIZE as usize - (va as usize & 4095));
        unsafe { core::ptr::copy_nonoverlapping(source.as_ptr().add(done), pa as *mut u8, count); }
        done += count;
    }
    Ok(())
}

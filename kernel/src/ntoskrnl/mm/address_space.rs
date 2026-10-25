//! First per-address-space VM primitives for user-mode isolation.
//! User mappings occupy PML4 slots 1..255 (512 GiB..128 TiB) so they do not
//! collide with the bootstrap's supervisor-only identity map at slot 0.
use core::arch::asm;
pub use super::user_range::{is_user_range, USER_END, USER_START};

const PAGE_SIZE: u64 = 4096;
const PRESENT: u64 = 1;
const WRITE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const LARGE: u64 = 1 << 7;
const NX: u64 = 1 << 63;
const ADDR: u64 = 0x000f_ffff_ffff_f000;

/// Serializes address-space mutations/destruction with cross-process virtual copies.
pub static ADDRESS_SPACE_LIFETIME_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

#[derive(Clone, Copy, Debug)]
pub enum VmError { InvalidAddress, Unaligned, OutOfMemory, AlreadyMapped, NotMapped, PermissionDenied, AddressSpaceBusy, ExecutableUserPageUnsupported }

pub struct AddressSpace { root: u64, kuser_tables: [u64; 3] }
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
        Ok(Self { root, kuser_tables: [0; 3] })
    }

    pub fn root_physical(&self) -> u64 { self.root }

    /// Map the fixed Windows KUSER_SHARED_DATA page without exposing other
    /// low identity mappings. The low page-table path is privately cloned and
    /// any large identity pages are split before only this leaf is made user.
    ///
    /// # Safety
    /// physical_address must be an allocated page that remains alive for every
    /// address space mapping this shared page.
    pub unsafe fn map_kuser_shared_page(&mut self, virtual_address: u64, physical_address: u64, writable: bool) -> Result<(), VmError> {
        const TWO_MIB: u64 = 0x20_0000;
        const PAT_LARGE: u64 = 1 << 12;
        if self.kuser_tables[0] != 0 { return Err(VmError::AlreadyMapped); }
        if virtual_address != super::user_range::KUSER_SHARED_DATA_ADDRESS
            || physical_address & (PAGE_SIZE - 1) != 0
            || physical_address >= 0x1_0000_0000
        { return Err(VmError::InvalidAddress); }
        let _guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
        let indexes = [
            ((virtual_address >> 39) & 0x1ff) as usize,
            ((virtual_address >> 30) & 0x1ff) as usize,
            ((virtual_address >> 21) & 0x1ff) as usize,
            ((virtual_address >> 12) & 0x1ff) as usize,
        ];
        unsafe fn clone_table(source: u64) -> Result<u64, VmError> {
            let target = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
            unsafe {
                core::ptr::copy_nonoverlapping(source as *const u64, target as *mut u64, 512);
            }
            Ok(target)
        }
        let root = self.root as *mut u64;
        let pml4_slot = unsafe { root.add(indexes[0]) };
        let pml4_entry = unsafe { pml4_slot.read() };
        let pdpt = if pml4_entry & PRESENT == 0 {
            let page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
            unsafe { pml4_slot.write(page | PRESENT | WRITE | USER); }
            page
        } else {
            if pml4_entry & LARGE != 0 { return Err(VmError::PermissionDenied); }
            let old_pdpt = pml4_entry & ADDR;
            let page = unsafe { clone_table(old_pdpt)? };
            let entries = page as *const u64;
            for i in 0..512usize {
                if unsafe { entries.add(i).read() } & USER != 0 {
                    unsafe { crate::memory::free_page(page) }.map_err(|_| VmError::OutOfMemory)?;
                    return Err(VmError::PermissionDenied);
                }
            }
            unsafe { pml4_slot.write(page | (pml4_entry & !ADDR) | USER); }
            page
        };
        self.kuser_tables[0] = pdpt;
        let pdpt_slot = unsafe { (pdpt as *mut u64).add(indexes[1]) };
        let pdpt_entry = unsafe { pdpt_slot.read() };
        let pd = if pdpt_entry & PRESENT == 0 {
            let page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
            unsafe { pdpt_slot.write(page | PRESENT | WRITE | USER); }
            page
        } else if pdpt_entry & LARGE != 0 {
            let page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
            let base = pdpt_entry & 0x000f_ffff_c000_0000;
            let pat = pdpt_entry & PAT_LARGE;
            let flags = pdpt_entry & !(0x000f_ffff_c000_0000 | LARGE | PAT_LARGE);
            let entries = page as *mut u64;
            for i in 0..512usize {
                unsafe { entries.add(i).write(base + i as u64 * TWO_MIB | flags | pat | LARGE); }
            }
            unsafe { pdpt_slot.write(page | (pdpt_entry & !(ADDR | LARGE | PAT_LARGE)) | USER); }
            page
        } else {
            let page = unsafe { clone_table(pdpt_entry & ADDR)? };
            let entries = page as *const u64;
            for i in 0..512usize {
                if unsafe { entries.add(i).read() } & USER != 0 {
                    unsafe { crate::memory::free_page(page) }.map_err(|_| VmError::OutOfMemory)?;
                    return Err(VmError::PermissionDenied);
                }
            }
            unsafe { pdpt_slot.write(page | (pdpt_entry & !ADDR) | USER); }
            page
        };
        self.kuser_tables[1] = pd;
        let pd_slot = unsafe { (pd as *mut u64).add(indexes[2]) };
        let pd_entry = unsafe { pd_slot.read() };
        let pt = if pd_entry & PRESENT == 0 {
            let page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
            unsafe { pd_slot.write(page | PRESENT | WRITE | USER); }
            page
        } else if pd_entry & LARGE != 0 {
            let page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
            let base = pd_entry & 0x000f_ffff_ffe0_0000;
            let pat = pd_entry & PAT_LARGE != 0;
            let flags = pd_entry & !(0x000f_ffff_ffe0_0000 | LARGE | PAT_LARGE);
            let entries = page as *mut u64;
            for i in 0..512usize {
                let mut entry = base + i as u64 * PAGE_SIZE | flags;
                if pat { entry |= 1 << 7; }
                unsafe { entries.add(i).write(entry); }
            }
            unsafe { pd_slot.write(page | (pd_entry & !(ADDR | LARGE | PAT_LARGE)) | USER); }
            page
        } else {
            let page = unsafe { clone_table(pd_entry & ADDR)? };
            let entries = page as *const u64;
            for i in 0..512usize {
                if unsafe { entries.add(i).read() } & USER != 0 {
                    unsafe { crate::memory::free_page(page) }.map_err(|_| VmError::OutOfMemory)?;
                    return Err(VmError::PermissionDenied);
                }
            }
            unsafe { pd_slot.write(page | (pd_entry & !ADDR) | USER); }
            page
        };
        self.kuser_tables[2] = pt;
        let leaf = unsafe { (pt as *mut u64).add(indexes[3]) };
        let mut flags = PRESENT | USER | NX;
        if writable { flags |= WRITE; }
        unsafe {
            leaf.write(physical_address | flags);
            asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
        }
        crate::ntoskrnl::mm::kernel_space::synchronize_all_cpu_tlbs();
        Ok(())
    }

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
        // Keep an undo log for intermediate tables created by this operation.
        // Mapping failures must not leak page-table pages or leave half-built trees.
        let mut created_slots = [core::ptr::null_mut::<u64>(); 3];
        let mut created_pages = [0u64; 3];
        let mut created = 0usize;
        let mut table = self.root as *mut u64;
        for index in indexes[..3].iter().copied() {
            let slot = unsafe { table.add(index) };
            let mut entry = unsafe { slot.read() };
            if entry & PRESENT == 0 {
                let page = match unsafe { crate::memory::allocate_page() } {
                    Ok(page) => page,
                    Err(_) => {
                        for undo in (0..created).rev() {
                            unsafe { created_slots[undo].write(0); }
                            let _ = unsafe { crate::memory::free_page(created_pages[undo]) };
                        }
                        return Err(VmError::OutOfMemory);
                    }
                };
                entry = page | PRESENT | WRITE | USER;
                unsafe { slot.write(entry); }
                created_slots[created] = slot;
                created_pages[created] = page;
                created += 1;
            } else {
                // Never turn a pre-existing supervisor-only hierarchy into a
                // user hierarchy: that could expose unrelated kernel mappings.
                if entry & LARGE != 0 || entry & USER == 0 {
                    for undo in (0..created).rev() {
                        unsafe { created_slots[undo].write(0); }
                        let _ = unsafe { crate::memory::free_page(created_pages[undo]) };
                    }
                    return Err(VmError::PermissionDenied);
                }
            }
            table = (entry & ADDR) as *mut u64;
        }

        let leaf = unsafe { table.add(indexes[3]) };
        if unsafe { leaf.read() } & PRESENT != 0 {
            for undo in (0..created).rev() {
                unsafe { created_slots[undo].write(0); }
                let _ = unsafe { crate::memory::free_page(created_pages[undo]) };
            }
            return Err(VmError::AlreadyMapped);
        }
        let mut flags = PRESENT | USER;
        if writable { flags |= WRITE; }
        if !executable { flags |= NX; }
        unsafe {
            leaf.write(physical_address | flags);
            asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags));
        }
        // Another CPU may have cached a non-present translation for this VA.
        crate::ntoskrnl::mm::kernel_space::synchronize_all_cpu_tlbs();
        Ok(())
    }

    /// Register a demand-zero virtual region; pages are committed on an explicit
    /// call to handle_page_fault. Region metadata is isolated by address-space root.
    pub fn register_demand_zero_region(&self, start:u64, length:u64, writable:bool, executable:bool) -> Result<(),VmError> {
        if start & (PAGE_SIZE-1)!=0 || length==0 || length & (PAGE_SIZE-1)!=0 || !is_user_range(start,length) || writable&&executable { return Err(VmError::InvalidAddress); }
        let end=start.checked_add(length).ok_or(VmError::InvalidAddress)?;
        let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();
        let areas=unsafe{&mut *DEMAND_AREAS.0.get()};
        if areas.iter().any(|a|a.live&&a.root==self.root&&start<a.end&&a.start<end){return Err(VmError::AlreadyMapped)}
        let slot=areas.iter_mut().find(|a|!a.live).ok_or(VmError::OutOfMemory)?;
        *slot=DemandArea{root:self.root,start,end,writable,executable,live:true}; Ok(())
    }

    /// Resolve a demand-zero or pagefile-backed user fault. A busy working-set
    /// entry serializes simultaneous faults and trim/teardown for the same page.
    pub fn handle_page_fault(&mut self,address:u64,error_code:u64)->bool {
        const PF_PRESENT:u64=1; const PF_WRITE:u64=2; const PF_USER:u64=4; const PF_RESERVED:u64=8; const PF_INSTRUCTION:u64=16;
        if error_code&(PF_PRESENT|PF_USER)!=PF_USER || error_code&PF_RESERVED!=0 || !is_user_range(address,1) {return false}
        let page_va=address&!(PAGE_SIZE-1);
        let (area, restore_slot, ws_index)={
            let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();
            let area=unsafe{&*DEMAND_AREAS.0.get()}.iter().copied().find(|a|a.live&&a.root==self.root&&page_va>=a.start&&page_va<a.end);
            let Some(area)=area else{return false};
            let pages=unsafe{&mut *WORKING_SET.0.get()};
            let index=if let Some(i)=pages.iter().position(|p|p.live&&p.root==self.root&&p.va==page_va){
                if pages[i].busy {
                    drop(_g);
                    // Another CPU may be paging this VA in or out. Wait for its
                    // transaction to publish a stable state, then retry instead
                    // of misclassifying contention as an access violation.
                    for _ in 0..1_000_000usize {
                        let busy={
                            let guard=ADDRESS_SPACE_LIFETIME_LOCK.lock();
                            let still=unsafe{&*WORKING_SET.0.get()}.iter()
                                .find(|p|p.live&&p.root==self.root&&p.va==page_va)
                                .is_some_and(|p|p.busy);
                            drop(guard);still
                        };
                        if !busy { return self.handle_page_fault(address,error_code); }
                        core::hint::spin_loop();
                    }
                    return false;
                }
                if !pages[i].paged_out{return false}
                pages[i].busy=true;i
            }else{
                let Some(i)=pages.iter().position(|p|!p.live) else{return false};
                pages[i]=WorkingSetPage{root:self.root,va:page_va,age:WORKING_SET_CLOCK.fetch_add(1,core::sync::atomic::Ordering::Relaxed),live:true,paged_out:false,busy:true,pagefile_slot:0};
                i
            };
            let slot=if pages[index].paged_out{Some(pages[index].pagefile_slot)}else{None};
            (area,slot,index)
        };
        if error_code&PF_WRITE!=0&&!area.writable || error_code&PF_INSTRUCTION!=0&&!area.executable {
            let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();(unsafe{&mut *WORKING_SET.0.get()})[ws_index].busy=false;return false
        }
        if self.translate_user(address,error_code&PF_WRITE!=0).is_ok(){
            let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();(unsafe{&mut *WORKING_SET.0.get()})[ws_index].busy=false;return false
        }
        let physical=match unsafe{crate::memory::allocate_page()}{Ok(p)=>p,Err(_)=>{
            let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();(unsafe{&mut *WORKING_SET.0.get()})[ws_index].busy=false;return false
        }};
        let loaded=if let Some(slot)=restore_slot {
            unsafe{crate::ntoskrnl::mm::pagefile::read_page(slot,physical as *mut u8)}.is_ok()
        }else{
            unsafe{core::ptr::write_bytes(physical as *mut u8,0,PAGE_SIZE as usize);}true
        };
        if !loaded || unsafe{self.map_user_page(page_va,physical,area.writable,area.executable)}.is_err(){
            let _=unsafe{crate::memory::free_page(physical)};
            let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();(unsafe{&mut *WORKING_SET.0.get()})[ws_index].busy=false;return false
        }
        {
            let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();
            let pages=unsafe{&mut *WORKING_SET.0.get()};
            let entry=&mut pages[ws_index];
            entry.paged_out=false;entry.busy=false;entry.pagefile_slot=0;
            entry.age=WORKING_SET_CLOCK.fetch_add(1,core::sync::atomic::Ordering::Relaxed);
        }
        if let Some(slot)=restore_slot { let _=crate::ntoskrnl::mm::pagefile::free_slot(slot); }
        true
    }

    /// Trim least-recently-used resident demand-zero pages to the dedicated
    /// pagefile. A page is reclaimed only after synchronous all-CPU TLB flush.
    pub fn trim_working_set(&mut self,target:usize)->usize {
        if target==0 || !crate::ntoskrnl::mm::pagefile::is_active() {return 0}
        let mut trimmed=0usize;
        while trimmed<target {
            let candidate={
                let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();
                let pages=unsafe{&mut *WORKING_SET.0.get()};
                let Some((index,_))=pages.iter().enumerate()
                    .filter(|(_,p)|p.live&&p.root==self.root&&!p.paged_out&&!p.busy)
                    .min_by_key(|(_,p)|p.age) else {break};
                pages[index].busy=true;
                (index,pages[index].va)
            };
            let (index,va)=candidate;
            let physical={
                let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();
                match user_leaf(self.root,va) {
                    Some((leaf,entry)) if entry&(PRESENT|USER|EXTERNAL_USER)==(PRESENT|USER)
                        && !crate::ntoskrnl::mm::runtime::is_page_locked(entry&ADDR) =>
                        (leaf as usize,entry&ADDR),
                    _=>{
                        (unsafe{&mut *WORKING_SET.0.get()})[index].busy=false;
                        continue
                    }
                }
            };
            let slot=match crate::ntoskrnl::mm::pagefile::allocate_slot(){
                Ok(s)=>s,Err(_)=>{let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();(unsafe{&mut *WORKING_SET.0.get()})[index].busy=false;break}
            };
            if unsafe{crate::ntoskrnl::mm::pagefile::write_page(slot,physical.1 as *const u8)}.is_err(){
                let _=crate::ntoskrnl::mm::pagefile::free_slot(slot);
                let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();(unsafe{&mut *WORKING_SET.0.get()})[index].busy=false;break
            }
            let removed={
                let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();
                let pages=unsafe{&mut *WORKING_SET.0.get()};
                if !pages[index].live||!pages[index].busy||pages[index].root!=self.root||pages[index].va!=va {
                    false
                } else if let Some((leaf,entry))=user_leaf(self.root,va) {
                    if leaf as usize!=physical.0||entry&(PRESENT|USER|EXTERNAL_USER)!=(PRESENT|USER)||entry&ADDR!=physical.1 {
                        false
                    } else {
                        unsafe{leaf.write(0);core::arch::asm!("invlpg [{}]",in(reg)va,options(nostack,preserves_flags));}
                        crate::ntoskrnl::mm::kernel_space::synchronize_all_cpu_tlbs();
                        if unsafe{crate::memory::free_page(physical.1)}.is_err(){false}else{
                            pages[index].paged_out=true;pages[index].pagefile_slot=slot;pages[index].busy=false;
                            pages[index].age=WORKING_SET_CLOCK.fetch_add(1,core::sync::atomic::Ordering::Relaxed);
                            true
                        }
                    }
                } else {false}
            };
            if removed {trimmed+=1;} else {
                let _=crate::ntoskrnl::mm::pagefile::free_slot(slot);
                let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();(unsafe{&mut *WORKING_SET.0.get()})[index].busy=false;
            }
        }
        trimmed
    }

    /// Number of tracked resident demand-zero pages in this address space.
    pub fn working_set_size(&self)->usize {
        let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock();
        unsafe{(&*WORKING_SET.0.get()).iter().filter(|p|p.live&&!p.paged_out&&p.root==self.root).count()}
    }

    /// Remove a region's metadata only after its resident pages have been unmapped.
    pub fn unregister_demand_zero_region(&self,start:u64,length:u64)->Result<(),VmError>{
        let end=start.checked_add(length).ok_or(VmError::InvalidAddress)?;
        if start&(PAGE_SIZE-1)!=0||length==0||length&(PAGE_SIZE-1)!=0||!is_user_range(start,length){return Err(VmError::InvalidAddress)}
        let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock(); let areas=unsafe{&mut *DEMAND_AREAS.0.get()};
        let index=areas.iter().position(|a|a.live&&a.root==self.root&&a.start==start&&a.end==end).ok_or(VmError::NotMapped)?;
        if unsafe{&*WORKING_SET.0.get()}.iter().any(|p|p.live&&p.root==self.root&&p.va>=start&&p.va<end){return Err(VmError::AddressSpaceBusy)}
        areas[index]=DemandArea::EMPTY; Ok(())
    }

    /// Probe a range without dereferencing user-controlled pointers. Use copy_from_user
    /// or copy_to_user for actual access so translations and permissions are checked.
    pub fn probe_user_range(&self,address:u64,length:u64,write:bool,execute:bool)->Result<(),VmError>{
        if !is_user_range(address,length){return Err(VmError::InvalidAddress)} if length==0{return Ok(())}
        let _g=ADDRESS_SPACE_LIFETIME_LOCK.lock(); let mut offset=0u64;
        while offset<length {let va=address.checked_add(offset).ok_or(VmError::InvalidAddress)?;let _=self.translate_user(va,write)?;
            if execute&&!self.is_user_executable(va){return Err(VmError::PermissionDenied)}
            offset=offset.checked_add(core::cmp::min(length-offset,PAGE_SIZE-(va&(PAGE_SIZE-1)))).ok_or(VmError::InvalidAddress)?;
        } Ok(())
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
        if destination.is_empty() { return Ok(()); }
        if !is_user_range(source, destination.len() as u64) { return Err(VmError::InvalidAddress); }
        let _address_space_guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
        // Preflight the entire source before touching the kernel destination.
        let mut probe = 0usize;
        while probe < destination.len() {
            let va = source.checked_add(probe as u64).ok_or(VmError::InvalidAddress)?;
            self.translate_user(va, false)?;
            probe += core::cmp::min(destination.len() - probe, PAGE_SIZE as usize - (va as usize & 4095));
        }
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
        if source.is_empty() { return Ok(()); }
        if !is_user_range(destination, source.len() as u64) { return Err(VmError::InvalidAddress); }
        let _address_space_guard = ADDRESS_SPACE_LIFETIME_LOCK.lock();
        // Probe every destination page first so an invalid later page cannot
        // leave a partially updated user buffer.
        let mut probe = 0usize;
        while probe < source.len() {
            let va = destination.checked_add(probe as u64).ok_or(VmError::InvalidAddress)?;
            self.translate_user(va, true)?;
            probe += core::cmp::min(source.len() - probe, PAGE_SIZE as usize - (va as usize & 4095));
        }
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
        if crate::process::address_space_has_references(self.root)
            || unsafe{&*WORKING_SET.0.get()}.iter().any(|p|p.live&&p.root==self.root&&p.busy) {
            return Err(VmError::AddressSpaceBusy);
        }
        // The caller has switched away locally; flush every online CPU before
        // freeing any user page-table or backing frame from this root.
        crate::ntoskrnl::mm::kernel_space::synchronize_all_cpu_tlbs();
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
        let areas=unsafe{&mut *DEMAND_AREAS.0.get()}; for area in areas.iter_mut().filter(|a|a.live&&a.root==self.root){*area=DemandArea::EMPTY;}
        let ws=unsafe{&mut *WORKING_SET.0.get()};
        for page in ws.iter_mut().filter(|p|p.live&&p.root==self.root){
            if page.paged_out { let _=crate::ntoskrnl::mm::pagefile::free_slot(page.pagefile_slot); }
            *page=WorkingSetPage::EMPTY;
        }
        crate::process::detach_address_space(self.root);
        // The KUSER mapping owns a private low page-table path so that setting
        // USER on its ancestors cannot expose or mutate kernel identity leaves.
        for table in self.kuser_tables.iter().copied() {
            if table != 0 { unsafe { crate::memory::free_page(table) }.map_err(|_| VmError::OutOfMemory)?; }
        }
        self.kuser_tables = [0; 3];
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




const MAX_DEMAND_AREAS: usize = 256;
const MAX_WORKING_SET_PAGES: usize = 8192;
#[derive(Clone, Copy)]
struct DemandArea { root:u64, start:u64, end:u64, writable:bool, executable:bool, live:bool }
impl DemandArea { const EMPTY: Self = Self { root:0,start:0,end:0,writable:false,executable:false,live:false }; }
struct DemandAreas(core::cell::UnsafeCell<[DemandArea; MAX_DEMAND_AREAS]>);
unsafe impl Sync for DemandAreas {}
static DEMAND_AREAS: DemandAreas = DemandAreas(core::cell::UnsafeCell::new([DemandArea::EMPTY; MAX_DEMAND_AREAS]));
#[derive(Clone, Copy)]
struct WorkingSetPage { root:u64, va:u64, age:u64, live:bool, paged_out:bool, busy:bool, pagefile_slot:u32 }
impl WorkingSetPage { const EMPTY: Self = Self { root:0,va:0,age:0,live:false,paged_out:false,busy:false,pagefile_slot:0 }; }
struct WorkingSet(core::cell::UnsafeCell<[WorkingSetPage; MAX_WORKING_SET_PAGES]>);
unsafe impl Sync for WorkingSet {}
static WORKING_SET: WorkingSet = WorkingSet(core::cell::UnsafeCell::new([WorkingSetPage::EMPTY; MAX_WORKING_SET_PAGES]));
static WORKING_SET_CLOCK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// Locate a present user leaf without changing it. The address-space lifetime
/// lock must be held by the caller.
fn user_leaf(root_physical:u64,va:u64)->Option<(*mut u64,u64)> {
    if root_physical==0||!is_user_range(va,1){return None}
    let indexes=[((va>>39)&511)as usize,((va>>30)&511)as usize,((va>>21)&511)as usize,((va>>12)&511)as usize];
    let mut table=root_physical as *mut u64;
    for index in indexes[..3].iter().copied(){
        let entry=unsafe{table.add(index).read()};
        if entry&(PRESENT|USER)!=(PRESENT|USER)||entry&LARGE!=0{return None}
        table=(entry&ADDR)as *mut u64;
    }
    let leaf=unsafe{table.add(indexes[3])};
    let entry=unsafe{leaf.read()};
    (entry&(PRESENT|USER)==(PRESENT|USER)).then_some((leaf,entry))
}

const EXTERNAL_USER: u64 = 1 << 9;
const MDL_USER_BASE: u64 = 0x0000_7000_0000_0000;
const MDL_USER_LIMIT: u64 = 0x0000_7000_1000_0000;
/// Resolve a page fault for the currently executing user process. The caller
/// must have established CPL=3 from the saved interrupt frame; this function
/// intentionally never guesses a process from CR3 alone.
/// Trim the current process working set. The active CR3 must belong to the
/// current process; pages are written to the configured dedicated pagefile and
/// reclaimed only after a synchronous all-CPU TLB shootdown.
pub fn trim_current_working_set(target:usize)->usize {
    let pid=crate::thread::current_process_id();
    let process=crate::process::eprocess_pointer_for_pid(pid);
    if process.is_null(){return 0}
    let root=unsafe{(*process).address_space_root};
    if root==0||root!=active_root_physical(){return 0}
    let mut space=AddressSpace{root, kuser_tables:[0;3]};
    space.trim_working_set(target)
}

pub fn handle_current_user_page_fault(address:u64,error_code:u64)->bool {
    const PF_USER:u64=1<<2;
    if error_code&PF_USER==0 { return false; }
    let pid=crate::thread::current_process_id();
    let process=crate::process::eprocess_pointer_for_pid(pid);
    if process.is_null(){return false}
    let root=unsafe{(*process).address_space_root};
    if root==0 || root!=active_root_physical(){return false}
    let mut space=AddressSpace{root};
    space.handle_page_fault(address,error_code)
}

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
        if entry & LARGE != 0 {
            if level == 1 {
                return Ok((entry & 0x000F_FFFF_C000_0000) | (virtual_address & 0x3FFF_FFFF));
            }
            if level == 2 {
                return Ok((entry & 0x000F_FFFF_FFE0_0000) | (virtual_address & 0x1F_FFFF));
            }
            return Err(VmError::NotMapped);
        }
        if level == 3 { return Ok((entry & ADDR) | (virtual_address & (PAGE_SIZE - 1))); }
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

    // Probe the complete range before modifying the caller's destination. This
    // prevents a bad final page from producing a silently truncated kernel copy.
    // The lifetime lock keeps the validated translations stable until copying ends.
    let mut probe = 0usize;
    while probe < destination.len() {
        let va = source.checked_add(probe as u64).ok_or(VmError::InvalidAddress)?;
        translate_user_root(root_physical, va, false)?;
        probe += core::cmp::min(destination.len() - probe, PAGE_SIZE as usize - (va as usize & 4095));
    }

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

    // Validate every destination page before writing any user bytes. With the
    // address-space lifetime lock held, unmap/protect/destroy cannot invalidate
    // the validation between this pass and the actual copy.
    let mut probe = 0usize;
    while probe < source.len() {
        let va = destination.checked_add(probe as u64).ok_or(VmError::InvalidAddress)?;
        translate_user_root(root_physical, va, true)?;
        probe += core::cmp::min(source.len() - probe, PAGE_SIZE as usize - (va as usize & 4095));
    }

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
